//! A software splash paints before world and GPU initialization. Its separate
//! event loop stays responsive while Bevy creates the graphics device.
use crate::*;
use serde::{Deserialize, Serialize};
use std::{
    io::{BufRead, BufReader, Write},
    path::Path,
    process::{Child, ChildStdin, Command, Stdio},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};
mod paint;
mod window;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Phase {
    World,
    Renderer,
    Observation,
    Entering,
    Ready,
}

impl Phase {
    fn completed(self) -> u8 {
        self as u8
    }
    fn label(self) -> &'static str {
        match self {
            Self::World => "Starting your adventure...",
            Self::Renderer => "Preparing the graphics...",
            Self::Observation => "Preparing the dungeon...",
            Self::Entering => "Entering the dungeon...",
            Self::Ready => "Your adventure is ready",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Progress {
    phase: Phase,
    detail: String,
    error: Option<String>,
}
impl Default for Progress {
    fn default() -> Self {
        Self {
            phase: Phase::World,
            detail: "Preparing your connection to the dungeon.".into(),
            error: None,
        }
    }
}
impl Progress {
    fn advance(&mut self, phase: Phase, detail: String) -> bool {
        if phase < self.phase || self.error.is_some() {
            return false;
        }
        if phase == self.phase && detail == self.detail {
            return false;
        }
        self.phase = phase;
        self.detail = detail;
        true
    }
}

#[derive(Serialize, Deserialize)]
enum Message {
    Progress(Progress),
    Close,
}

#[derive(Default)]
struct Handoff {
    world_frame: Option<u64>,
    visible_frame: Option<u64>,
}
#[derive(Debug, PartialEq, Eq)]
enum Action {
    Wait,
    ShowGame,
    Finish,
}
impl Handoff {
    fn update(&mut self, world_ready: bool, render_frame: u64) -> Action {
        if !world_ready {
            return Action::Wait;
        }
        let first = *self.world_frame.get_or_insert(render_frame);
        if let Some(visible) = self.visible_frame {
            if render_frame >= visible.saturating_add(2) {
                Action::Finish
            } else {
                Action::Wait
            }
        } else if render_frame >= first.saturating_add(2) {
            self.visible_frame = Some(render_frame);
            Action::ShowGame
        } else {
            Action::Wait
        }
    }
}

#[derive(Resource)]
pub(crate) struct Splash {
    child: Mutex<Option<Child>>,
    input: Mutex<Option<ChildStdin>>,
    cancelled: Arc<AtomicBool>,
    progress: Progress,
    handoff: Handoff,
    finished: bool,
}

impl Splash {
    /// No splash window: Android cannot start a second process for one, and
    /// the standalone menu draws its own title in the game window.
    pub(crate) fn inert() -> Self {
        Self {
            child: Mutex::new(None),
            input: Mutex::new(None),
            cancelled: Arc::new(AtomicBool::new(false)),
            progress: Progress::default(),
            handoff: Handoff::default(),
            finished: false,
        }
    }

    pub(crate) fn start(captures: Option<&Path>) -> Result<Self, Error> {
        if cfg!(target_os = "android") {
            return Ok(Self::inert());
        }
        let mut command = Command::new(std::env::current_exe()?);
        command
            .arg("--startup-window")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .env_remove("TOR_SERVER_TOKEN")
            .env_remove("TOR_SPECTATOR_TOKEN")
            .env_remove("TOR_WIZARD_TOKEN");
        if let Some(directory) = captures {
            command.arg("--capture").arg(directory.join("startup"));
        }
        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW: the splash owns its graphical window.
        }
        let mut child = command.spawn()?;
        let input = child.stdin.take();
        let output = child.stdout.take().ok_or("Splash has no output")?;
        let cancelled = Arc::new(AtomicBool::new(false));
        let cancel = cancelled.clone();
        let (shown_tx, shown_rx) = mpsc::sync_channel(1);
        std::thread::spawn(move || {
            let mut shown = false;
            let mut completed = false;
            for line in BufReader::new(output).lines().map_while(Result::ok) {
                let Ok(event) = serde_json::from_str::<serde_json::Value>(&line) else {
                    continue;
                };
                println!("{event}");
                if event["event"] == "presented"
                    && (event["completed"] == 4 || event["failed"] == true)
                {
                    completed = true;
                }
                match event["event"].as_str() {
                    Some("presented") if !shown => {
                        shown = true;
                        let _ = shown_tx.send(Ok(()));
                    }
                    Some("cancelled") => {
                        cancel.store(true, Ordering::Release);
                    }
                    Some("error") if !shown => {
                        let _ = shown_tx.send(Err(event["message"]
                            .as_str()
                            .unwrap_or("Splash failed")
                            .to_owned()));
                    }
                    _ => {}
                }
            }
            if !shown {
                let _ = shown_tx.send(Err("Splash closed before its first presentation".into()));
            }
            if shown && !completed {
                cancel.store(true, Ordering::Release);
            }
        });
        let splash = Self {
            child: Mutex::new(Some(child)),
            input: Mutex::new(input),
            cancelled,
            progress: Progress::default(),
            handoff: Handoff::default(),
            finished: false,
        };
        shown_rx
            .recv_timeout(Duration::from_secs(15))
            .map_err(|_| "Splash did not present within its startup deadline")??;
        Ok(splash)
    }

    pub(crate) fn cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
    fn send(&self, message: &Message) {
        if let Some(input) = self.input.lock().unwrap().as_mut() {
            let _ = serde_json::to_writer(&mut *input, message);
            let _ = input.write_all(b"\n");
            let _ = input.flush();
        }
    }
    fn advance(&mut self, phase: Phase, detail: String) {
        if self.progress.advance(phase, detail) {
            self.send(&Message::Progress(self.progress.clone()));
        }
    }
    pub(crate) fn preparing_renderer(&mut self) {
        self.advance(Phase::Renderer, "Preparing lighting and materials.".into());
    }
    fn reap(&self) {
        let mut child = self.child.lock().unwrap();
        if child
            .as_mut()
            .is_some_and(|child| child.try_wait().ok().flatten().is_some())
        {
            child.take();
            self.input.lock().unwrap().take();
        }
    }
}

impl Drop for Splash {
    fn drop(&mut self) {
        self.send(&Message::Close);
        self.input.lock().unwrap().take();
        if let Some(mut child) = self.child.lock().unwrap().take() {
            let deadline = Instant::now() + Duration::from_secs(2);
            while Instant::now() < deadline {
                if child.try_wait().ok().flatten().is_some() {
                    return;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

#[derive(Resource, Clone, Default)]
pub(crate) struct Presented(Arc<AtomicU64>);
impl Presented {
    /// Frames the renderer has finished so far.
    pub(crate) fn count(&self) -> u64 {
        self.0.load(Ordering::Acquire)
    }
    #[cfg(test)]
    pub(crate) fn record(&self) {
        self.0.fetch_add(1, Ordering::Release);
    }
}
pub(crate) struct PresentationPlugin;
impl Plugin for PresentationPlugin {
    fn build(&self, app: &mut App) {
        use bevy::render::{Render, RenderApp, RenderSet};
        let presented = Presented::default();
        app.insert_resource(presented.clone());
        app.sub_app_mut(RenderApp)
            .insert_resource(presented)
            .add_systems(Render, record_frame.in_set(RenderSet::Cleanup));
    }
}
fn record_frame(frame: Res<Presented>) {
    frame.0.fetch_add(1, Ordering::Release);
}
pub(crate) fn renderer_ready(mut splash: ResMut<Splash>) {
    splash.advance(
        Phase::Observation,
        "Graphics ready. Preparing your first view of the dungeon.".into(),
    );
}

/// The game window's preferred logical size.
const GAME_WINDOW: Vec2 = Vec2::new(1440.0, 900.0);

/// A window size that leaves room for the taskbar and title bar on the
/// monitor, so no part of the interface opens below the screen's edge.
fn fitted_game_window(monitor: Vec2) -> Vec2 {
    Vec2::new(
        GAME_WINDOW.x.min(monitor.x * 0.92),
        GAME_WINDOW.y.min(monitor.y * 0.86),
    )
    .floor()
}

/// Before the game window is first shown, fit it to the primary monitor and
/// place it at the monitor's top, centred across it.
pub(crate) fn fit_window(
    mut fitted: Local<bool>,
    monitors: Query<&bevy::window::Monitor, With<bevy::window::PrimaryMonitor>>,
    mut windows: Query<&mut Window, With<bevy::window::PrimaryWindow>>,
) {
    // An Android window always fills its screen.
    if *fitted || cfg!(target_os = "android") {
        return;
    }
    let (Ok(monitor), Ok(mut window)) = (monitors.single(), windows.single_mut()) else {
        return;
    };
    *fitted = true;
    if window.visible {
        return;
    }
    let scale = monitor.scale_factor.max(0.1) as f32;
    let logical = Vec2::new(
        monitor.physical_width as f32,
        monitor.physical_height as f32,
    ) / scale;
    let size = fitted_game_window(logical);
    window.resolution.set(size.x, size.y);
    // Centring on the whole monitor would tuck the bottom under a taskbar, so
    // the window opens at the monitor's top edge, centred across it.
    let left = (monitor.physical_width as f32 - size.x * scale).max(0.0) * 0.5;
    window.position =
        bevy::window::WindowPosition::At(monitor.physical_position + IVec2::new(left as i32, 0));
}

pub(crate) fn tick(
    mut splash: ResMut<Splash>,
    game: Res<Game>,
    frames: Res<Presented>,
    mut windows: Query<&mut Window, With<bevy::window::PrimaryWindow>>,
    pending: Query<(), With<creatures::loading::PendingModel>>,
    mut exit: EventWriter<AppExit>,
) {
    if splash.cancelled() {
        exit.write(AppExit::Success);
        return;
    }
    if splash.finished {
        splash.reap();
        return;
    }
    let Ok(mut window) = windows.single_mut() else {
        return;
    };
    if game.fatal {
        window.visible = true;
        splash.progress.error = Some(game.status.clone());
        splash.send(&Message::Progress(splash.progress.clone()));
        splash.finished = true;
        return;
    }
    let ready = game.view.is_some() && !game.busy;
    if ready {
        splash.advance(
            Phase::Entering,
            "The dungeon is ready. Opening your view.".into(),
        );
    }
    match splash
        .handoff
        .update(ready, frames.0.load(Ordering::Acquire))
    {
        Action::Wait => {}
        Action::ShowGame => {
            window.visible = true;
            window.focused = true;
        }
        Action::Finish => {
            let count = pending.iter().count();
            splash.advance(
                Phase::Ready,
                if count == 0 {
                    "You can move now. Your adventure is ready.".into()
                } else {
                    format!("You can move now. {count} creatures are still taking shape.")
                },
            );
            splash.finished = true;
        }
    }
}

pub(crate) fn window_cli(arguments: &[String]) -> Result<(), Error> {
    window::run(arguments)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_game_window_fits_small_monitors_and_keeps_its_size_on_large_ones() {
        assert_eq!(fitted_game_window(Vec2::new(2560.0, 1440.0)), GAME_WINDOW);
        // A 1920x1080 display at 125% scale is 1536x864 logical; 900 would overflow.
        let laptop = fitted_game_window(Vec2::new(1536.0, 864.0));
        assert_eq!(laptop, Vec2::new(1413.0, 743.0));
        assert!(
            laptop.y + 48.0 + 32.0 <= 864.0,
            "taskbar and title bar still fit"
        );
        assert_eq!(
            fitted_game_window(Vec2::new(1280.0, 720.0)),
            Vec2::new(1177.0, 619.0)
        );
    }
    #[test]
    fn splash_stays_until_world_and_visible_render_frames_are_ready() {
        let mut handoff = Handoff::default();
        assert_eq!(
            handoff.update(false, 100),
            Action::Wait,
            "renderer frames alone cannot establish a playable dungeon"
        );
        assert_eq!(handoff.update(true, 100), Action::Wait);
        assert_eq!(handoff.update(true, 101), Action::Wait);
        assert_eq!(handoff.update(true, 102), Action::ShowGame);
        assert_eq!(handoff.update(true, 103), Action::Wait);
        assert_eq!(handoff.update(true, 104), Action::Finish);
    }
    #[test]
    fn progress_counts_completed_milestones_and_never_moves_backwards() {
        let mut progress = Progress::default();
        assert_eq!(progress.phase.completed(), 0);
        for (phase, expected) in [
            (Phase::Renderer, 1),
            (Phase::Observation, 2),
            (Phase::Entering, 3),
            (Phase::Ready, 4),
        ] {
            assert!(progress.advance(phase, "new step".into()));
            assert_eq!(progress.phase.completed(), expected);
            assert!(!progress.advance(Phase::World, "late connection message".into()));
            assert_eq!(progress.phase.completed(), expected);
        }
        assert!(!progress.advance(Phase::Ready, "new step".into()));
    }
    #[test]
    fn failures_keep_the_last_completed_milestone_and_cannot_be_overwritten() {
        let mut progress = Progress::default();
        progress.advance(Phase::Observation, "graphics ready".into());
        progress.error = Some("connection failed".into());
        assert!(!progress.advance(Phase::Ready, "late ready".into()));
        assert_eq!(progress.phase.completed(), 2);
        assert_eq!(progress.error.as_deref(), Some("connection failed"));
    }
}
