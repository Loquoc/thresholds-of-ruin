//! Standalone play: the title, menu, loading and saving screens around a game
//! hosted inside this process. One stage at a time says what is happening,
//! so the player never faces a blank or frozen screen without a reason.
//!
//! The order is title → menu → loading → playing → saving → menu. Leaving a
//! game returns the window to the state it had before the first game started.
use crate::{
    chrome,
    local_host::{self, SavedGame, ScenarioChoice},
    net::Network,
    startup::Presented,
    Game, Net,
};
use bevy::{
    input::{
        keyboard::{Key, KeyboardInput},
        ButtonState,
    },
    prelude::*,
};
#[cfg(test)]
use std::time::Duration;
use std::{
    path::PathBuf,
    sync::{
        mpsc::{self, Receiver, TryRecvError},
        Mutex,
    },
};
use tor_protocol::ActorId;

/// Saves shown on one page of the load list.
const PAGE: usize = 6;
/// The title stays at least this long, so it reads as a screen, not a flicker.
const TITLE_SECONDS: f32 = 1.2;
/// Frames the finished world must have been presented before it is revealed.
const REVEAL_FRAMES: u64 = 2;

/// Present before the first game: kept when a game is left.
#[derive(Component)]
pub(crate) struct Persistent;
#[derive(Component)]
struct Overlay;
#[derive(Component)]
struct Body;
#[derive(Component)]
struct Spinner;
#[derive(Component)]
struct WarmUp;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Tab {
    #[default]
    New,
    Load,
}

/// What the menu asks the session to start.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Request {
    New { scenario: ScenarioChoice, seed: u64 },
    Resume(SavedGame),
}

/// Everything the menu shows and edits, without any UI.
#[derive(Clone, Debug, Default)]
pub(crate) struct MenuModel {
    pub(crate) tab: Tab,
    pub(crate) scenarios: Vec<ScenarioChoice>,
    pub(crate) scenario: usize,
    pub(crate) seed: String,
    pub(crate) saves: Vec<SavedGame>,
    pub(crate) save: Option<usize>,
    pub(crate) page: usize,
    pub(crate) error: Option<String>,
}

impl MenuModel {
    /// Appends a digit unless the seed would no longer fit in 64 bits.
    pub(crate) fn digit(&mut self, digit: u8) -> bool {
        if digit > 9 {
            return false;
        }
        let mut next = if self.seed == "0" {
            String::new()
        } else {
            self.seed.clone()
        };
        next.push(char::from(b'0' + digit));
        if next.parse::<u64>().is_err() {
            return false;
        }
        self.seed = next;
        true
    }
    pub(crate) fn backspace(&mut self) {
        self.seed.pop();
    }
    pub(crate) fn set_seed(&mut self, seed: u64) {
        self.seed = seed.to_string();
    }
    pub(crate) fn set_saves(&mut self, saves: Vec<SavedGame>) {
        self.save = (!saves.is_empty()).then_some(0);
        self.page = 0;
        self.saves = saves;
    }
    pub(crate) fn pages(&self) -> usize {
        self.saves.len().div_ceil(PAGE).max(1)
    }
    pub(crate) fn turn_page(&mut self, forward: bool) {
        self.page = if forward {
            (self.page + 1).min(self.pages() - 1)
        } else {
            self.page.saturating_sub(1)
        };
    }
    /// What Start or Load would begin, or why it cannot.
    pub(crate) fn request(&self) -> Result<Request, String> {
        match self.tab {
            Tab::New => {
                let scenario = self
                    .scenarios
                    .get(self.scenario)
                    .ok_or("No scenario is installed.")?;
                let seed = self
                    .seed
                    .parse()
                    .map_err(|_| "Enter a seed, or choose Random.")?;
                Ok(Request::New {
                    scenario: scenario.clone(),
                    seed,
                })
            }
            Tab::Load => self
                .save
                .and_then(|index| self.saves.get(index))
                .cloned()
                .map(Request::Resume)
                .ok_or_else(|| "Choose a saved game to load.".into()),
        }
    }
}

/// The parts of loading the player is told about, in order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Step {
    Opening { resume: bool },
    Connecting,
    Building,
}

impl Step {
    fn label(self) -> &'static str {
        match self {
            Step::Opening { resume: true } => "Opening your saved game",
            Step::Opening { resume: false } => "Creating a new world",
            Step::Connecting => "Joining the game",
            Step::Building => "Building the scene",
        }
    }
    fn number(self) -> usize {
        match self {
            Step::Opening { .. } => 1,
            Step::Connecting => 2,
            Step::Building => 3,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Then {
    Menu,
    Quit,
}

type Started = Result<local_host::Started, String>;

pub(crate) enum Stage {
    Title {
        shown: Option<u64>,
        warm: Option<u64>,
        elapsed: f32,
    },
    Menu,
    Loading {
        step: Step,
        job: Option<Mutex<Receiver<Started>>>,
        ready: Option<u64>,
    },
    Playing {
        confirm: bool,
    },
    Closing {
        job: Mutex<Receiver<Result<(), String>>>,
        then: Then,
        /// Why the game is closing, when it failed to start.
        error: Option<String>,
    },
}

impl Stage {
    pub(crate) fn name(&self) -> &'static str {
        match self {
            Stage::Title { .. } => "title",
            Stage::Menu => "menu",
            Stage::Loading { .. } => "loading",
            Stage::Playing { confirm: false } => "playing",
            Stage::Playing { confirm: true } => "confirm",
            Stage::Closing { .. } => "closing",
        }
    }
}

#[derive(Resource)]
pub(crate) struct Session {
    scenario_root: PathBuf,
    save_root: PathBuf,
    pub(crate) stage: Stage,
    pub(crate) menu: MenuModel,
    host: Option<tor_server::in_process::Host>,
    dirty: bool,
    reset: bool,
    /// Games started, for tests and diagnostics.
    pub(crate) games: u32,
}

impl Session {
    pub(crate) fn new(scenario_root: PathBuf, save_root: PathBuf) -> Self {
        let mut menu = MenuModel {
            scenarios: local_host::discover_scenarios(&scenario_root),
            ..default()
        };
        // The desktop launchers' default, when it is installed.
        menu.scenario = ["rogue-exploration", "rogue"]
            .iter()
            .find_map(|id| menu.scenarios.iter().position(|choice| choice.id == *id))
            .unwrap_or(0);
        menu.set_seed(local_host::random_seed());
        menu.set_saves(local_host::list_saves(&save_root));
        if menu.scenarios.is_empty() {
            menu.error = Some(format!(
                "No scenarios were found in {}.",
                scenario_root.display()
            ));
        }
        Self {
            scenario_root,
            save_root,
            stage: Stage::Title {
                shown: None,
                warm: None,
                elapsed: 0.0,
            },
            menu,
            host: None,
            dirty: true,
            reset: false,
            games: 0,
        }
    }

    /// Whether the game underneath may take keyboard and pointer input.
    pub(crate) fn game_has_input(&self) -> bool {
        matches!(self.stage, Stage::Playing { confirm: false })
    }

    /// Escape or the Android back key while playing: ask before leaving.
    pub(crate) fn ask_to_leave(&mut self) {
        if let Stage::Playing { confirm } = &mut self.stage {
            *confirm = !*confirm;
            self.dirty = true;
        }
    }

    fn return_to_menu(&mut self, error: Option<String>) {
        self.menu.set_saves(local_host::list_saves(&self.save_root));
        self.menu.error = error;
        self.stage = Stage::Menu;
        self.dirty = true;
    }

    /// Starts the private server off the frame loop, so the screen keeps
    /// drawing while a world is generated or a save is replayed.
    fn begin(&mut self, request: Request) {
        let resume = matches!(request, Request::Resume(_));
        let (scenario_root, save_root) = (self.scenario_root.clone(), self.save_root.clone());
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let started = match request {
                Request::New { scenario, seed } => {
                    local_host::new_game(&scenario, seed, &save_root)
                }
                Request::Resume(saved) => local_host::resume(&saved, &scenario_root),
            };
            let _ = tx.send(started.map_err(|error| error.to_string()));
        });
        self.menu.error = None;
        self.games += 1;
        self.stage = Stage::Loading {
            step: Step::Opening { resume },
            job: Some(Mutex::new(rx)),
            ready: None,
        };
        self.dirty = true;
    }

    /// Saves and stops the game off the frame loop. The connection's close
    /// is the durable save; the server then flushes its journal.
    fn close(&mut self, net: &Net, then: Then, error: Option<String>) {
        let network = std::mem::replace(&mut *net.0.lock().unwrap(), Network::idle());
        let host = self.host.take();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let mut network = network;
            let saved = network.shutdown();
            let stopped = host.map_or(Ok(()), tor_server::in_process::Host::shutdown);
            let _ = tx.send(saved.and(stopped));
        });
        self.stage = Stage::Closing {
            job: Mutex::new(rx),
            then,
            error,
        };
        self.dirty = true;
    }

    /// Stops a game still running when the window closes without the menu.
    pub(crate) fn shutdown(&mut self) -> Result<(), String> {
        self.host
            .take()
            .map_or(Ok(()), tor_server::in_process::Host::shutdown)
    }
}

pub(crate) fn has_input(session: Option<Res<Session>>) -> bool {
    session.is_none_or(|session| session.game_has_input())
}

pub(crate) struct SessionPlugin;

impl Plugin for SessionPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup.after(chrome::setup))
            .add_systems(PostStartup, mark_persistent)
            .add_systems(
                Update,
                (
                    controls,
                    keys,
                    drive,
                    reset.run_if(|session: Res<Session>| session.reset),
                    draw,
                    spin,
                )
                    .chain()
                    .after(crate::scene::receive)
                    // Escape that opens the leave panel must not also close it.
                    .before(crate::interact::keys),
            );
    }
}

fn setup(mut commands: Commands) {
    commands
        .spawn((
            Overlay,
            chrome::BlockWorld,
            Interaction::default(),
            GlobalZIndex(50),
            BackgroundColor(BACKDROP),
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
        ))
        .with_child((
            Body,
            Node {
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: Val::Px(14.0),
                max_width: Val::Percent(94.0),
                ..default()
            },
        ));
}

/// Everything that exists before the first game is kept when one is left.
fn mark_persistent(world: &mut World) {
    let entities: Vec<Entity> = world.iter_entities().map(|entity| entity.id()).collect();
    for entity in entities {
        world.entity_mut(entity).insert(Persistent);
    }
}

const BACKDROP: Color = Color::srgb(0.02, 0.018, 0.016);
const WARNING: Color = Color::srgb(0.95, 0.55, 0.42);

#[derive(Component, Clone, Debug, PartialEq, Eq)]
enum Control {
    Tab(Tab),
    Scenario(usize),
    Digit(u8),
    Backspace,
    Random,
    Start,
    Save(usize),
    Page(bool),
    Quit,
    ReturnToMenu,
    SaveAndQuit,
    KeepPlaying,
}

fn controls(
    mut session: ResMut<Session>,
    net: Res<Net>,
    mut pressed: Query<(&Interaction, &Control, &mut BackgroundColor), Changed<Interaction>>,
    mut exit: EventWriter<AppExit>,
) {
    for (interaction, control, mut color) in &mut pressed {
        color.0 = match interaction {
            Interaction::None => chrome::BUTTON,
            _ => chrome::HOT,
        };
        if *interaction != Interaction::Pressed {
            continue;
        }
        act(&mut session, &net, control.clone(), &mut exit);
    }
}

fn act(session: &mut Session, net: &Net, control: Control, exit: &mut EventWriter<AppExit>) {
    session.dirty = true;
    match control {
        Control::Tab(tab) => session.menu.tab = tab,
        Control::Scenario(index) => session.menu.scenario = index,
        Control::Digit(digit) => {
            session.menu.digit(digit);
        }
        Control::Backspace => session.menu.backspace(),
        Control::Random => session.menu.set_seed(local_host::random_seed()),
        Control::Save(index) => session.menu.save = Some(index),
        Control::Page(forward) => session.menu.turn_page(forward),
        Control::Start => match session.menu.request() {
            Ok(request) => session.begin(request),
            Err(reason) => session.menu.error = Some(reason),
        },
        Control::Quit => {
            exit.write(AppExit::Success);
        }
        Control::ReturnToMenu => session.close(net, Then::Menu, None),
        Control::SaveAndQuit => session.close(net, Then::Quit, None),
        Control::KeepPlaying => session.ask_to_leave(),
    }
}

/// Digits, Backspace and Enter on a keyboard; Escape and the Android back
/// key ask before leaving a game, and leave the menu.
fn keys(
    keys: Res<ButtonInput<KeyCode>>,
    mut typed: EventReader<KeyboardInput>,
    mut session: ResMut<Session>,
    net: Res<Net>,
    mut exit: EventWriter<AppExit>,
) {
    // Android's back key has no physical key code, only its logical meaning.
    // Counting reads every event, so none is left over for the next frame.
    let android_back = typed
        .read()
        .filter(|event| {
            event.logical_key == Key::BrowserBack
                && event.state == ButtonState::Pressed
                && !event.repeat
        })
        .count()
        > 0;
    let back = android_back || keys.just_pressed(KeyCode::Escape);
    match session.stage {
        Stage::Menu => {
            if back {
                exit.write(AppExit::Success);
                return;
            }
            if session.menu.tab == Tab::New {
                for (index, code) in DIGIT_KEYS.iter().enumerate() {
                    if keys.just_pressed(*code) {
                        act(
                            &mut session,
                            &net,
                            Control::Digit(index as u8 % 10),
                            &mut exit,
                        );
                    }
                }
                if keys.just_pressed(KeyCode::Backspace) {
                    act(&mut session, &net, Control::Backspace, &mut exit);
                }
            }
            if keys.any_just_pressed([KeyCode::Enter, KeyCode::NumpadEnter]) {
                act(&mut session, &net, Control::Start, &mut exit);
            }
        }
        Stage::Playing { confirm: true } if back => session.ask_to_leave(),
        // While playing, Escape first closes whatever the game has open;
        // the game asks the session only when it has nothing left to close.
        Stage::Playing { confirm: false } if android_back => session.ask_to_leave(),
        _ => {}
    }
}

const DIGIT_KEYS: [KeyCode; 20] = [
    KeyCode::Digit0,
    KeyCode::Digit1,
    KeyCode::Digit2,
    KeyCode::Digit3,
    KeyCode::Digit4,
    KeyCode::Digit5,
    KeyCode::Digit6,
    KeyCode::Digit7,
    KeyCode::Digit8,
    KeyCode::Digit9,
    KeyCode::Numpad0,
    KeyCode::Numpad1,
    KeyCode::Numpad2,
    KeyCode::Numpad3,
    KeyCode::Numpad4,
    KeyCode::Numpad5,
    KeyCode::Numpad6,
    KeyCode::Numpad7,
    KeyCode::Numpad8,
    KeyCode::Numpad9,
];

#[allow(clippy::too_many_arguments)]
fn drive(
    mut commands: Commands,
    time: Res<Time>,
    mut session: ResMut<Session>,
    net: Res<Net>,
    game: Res<Game>,
    frames: Res<Presented>,
    palette: Option<Res<crate::Palette>>,
    mut meshes: ResMut<Assets<Mesh>>,
    warm: Query<Entity, With<WarmUp>>,
    mut exit: EventWriter<AppExit>,
) {
    let frame = frames.count();
    let session = &mut *session;
    match &mut session.stage {
        Stage::Title {
            shown,
            warm: warmed,
            elapsed,
        } => {
            *elapsed += time.delta_secs();
            // Draw the title first; only then start the materials compiling,
            // which can hold one frame for a long time on a first launch.
            let shown = *shown.get_or_insert(frame);
            if warmed.is_none() && frame >= shown + 2 {
                if let Some(palette) = &palette {
                    spawn_warm_up(&mut commands, palette, &mut meshes);
                }
                *warmed = Some(frame);
            }
            if warmed.is_some_and(|warm| frame >= warm + 3) && *elapsed >= TITLE_SECONDS {
                for entity in &warm {
                    commands.entity(entity).despawn();
                }
                session.stage = Stage::Menu;
                session.dirty = true;
            }
        }
        Stage::Loading { step, job, ready } => {
            if let Some(receiver) = job {
                let result = receiver.get_mut().unwrap().try_recv();
                match result {
                    Err(TryRecvError::Empty) => {}
                    Err(TryRecvError::Disconnected) => {
                        session.return_to_menu(Some("The game could not start.".into()));
                    }
                    Ok(Err(error)) => session.return_to_menu(Some(error)),
                    Ok(Ok(started)) => {
                        // Logged, so it reaches Android's log as well as a terminal.
                        info!(
                            "{}",
                            serde_json::json!({"type": "local_game",
                                "address": started.host.address(), "save": started.save})
                        );
                        *net.0.lock().unwrap() = Network::start(
                            started.host.address(),
                            started.token,
                            ActorId(1),
                            false,
                        );
                        session.host = Some(started.host);
                        *step = Step::Connecting;
                        *job = None;
                        session.dirty = true;
                    }
                }
                return;
            }
            if game.fatal {
                let error = if game.status.is_empty() {
                    "The game stopped while loading.".into()
                } else {
                    game.status.clone()
                };
                session.close(&net, Then::Menu, Some(error));
                return;
            }
            if *step == Step::Connecting && game.view.is_some() {
                *step = Step::Building;
                session.dirty = true;
            }
            if *step == Step::Building && !game.busy {
                let ready = *ready.get_or_insert(frame);
                if frame >= ready + REVEAL_FRAMES {
                    session.stage = Stage::Playing { confirm: false };
                    session.dirty = true;
                }
            }
        }
        Stage::Closing { job, then, error } => {
            let result = match job.get_mut().unwrap().try_recv() {
                Err(TryRecvError::Empty) => return,
                Err(TryRecvError::Disconnected) => Err("Saving stopped unexpectedly.".into()),
                Ok(result) => result,
            };
            let (then, error) = (*then, error.take());
            session.reset = true;
            match (then, result) {
                (Then::Quit, Ok(())) => {
                    exit.write(AppExit::Success);
                }
                (_, result) => session.return_to_menu(error.or_else(|| {
                    result
                        .err()
                        .map(|reason| format!("The save could not be confirmed: {reason}"))
                })),
            }
        }
        Stage::Menu | Stage::Playing { .. } => {}
    }
}

/// Materials the dungeon draws with, shown once behind the title so their
/// shaders are ready before the first world is.
fn spawn_warm_up(commands: &mut Commands, palette: &crate::Palette, meshes: &mut Assets<Mesh>) {
    commands.spawn((
        WarmUp,
        Mesh3d(meshes.add(Cuboid::new(1.0, 1.0, 1.0))),
        MeshMaterial3d(palette.cave_mat.clone()),
        Transform::from_xyz(-0.8, 0.5, 0.0),
    ));
    for (index, material) in palette.crystal_mat.iter().enumerate() {
        commands.spawn((
            WarmUp,
            Mesh3d(meshes.add(Sphere::new(0.3))),
            MeshMaterial3d(material.clone()),
            Transform::from_xyz(0.6 + index as f32 * 0.7, 0.5, 0.0),
        ));
    }
}

/// Removes everything a game created and restores the window's state from
/// before it, so the next game starts as the first one did.
fn reset(world: &mut World) {
    world.resource_mut::<Session>().reset = false;
    let doomed: Vec<Entity> = world
        .query_filtered::<Entity, (With<Transform>, Without<Persistent>, Without<ChildOf>)>()
        .iter(world)
        .collect();
    for entity in doomed {
        world.despawn(entity);
    }
    let old = std::mem::take(&mut *world.resource_mut::<Game>());
    *world.resource_mut::<Game>() = old.restarted();
    if let Some(mut rig) = world.get_resource_mut::<crate::CameraRig>() {
        // The player's zoom is a preference, not part of the game.
        *rig = crate::CameraRig {
            distance: rig.distance,
            ..default()
        };
    }
    world.insert_resource(crate::interact::Hover::default());
    world.insert_resource(crate::interact::Pursuit::default());
    world.insert_resource(crate::interact::Panel::default());
    world.insert_resource(crate::loot_labels::LootLabels::default());
    world.insert_resource(crate::pile_picker::PilePicker::default());
    world.insert_resource(crate::progression::State::default());
    world.insert_resource(crate::map_journal::route::State::default());
    world.insert_resource(crate::creatures::loading::ModelLoading::default());
    world.insert_resource(crate::creatures::Boredom(0.0));
    // Drawn images are kept and redrawn for the next game.
    if let Some(mut orb) = world.get_resource_mut::<crate::hud::HealthOrb>() {
        orb.forget();
    }
    if let Some(mut map) = world.get_resource_mut::<crate::minimap::MiniMap>() {
        map.forget();
    }
    if let Some(mut map) = world.get_resource_mut::<crate::map_journal::Map>() {
        map.forget();
    }
    if let Some(mut portrait) = world.get_resource_mut::<crate::portrait::Portrait>() {
        portrait.forget();
    }
}

fn draw(
    mut commands: Commands,
    mut session: ResMut<Session>,
    mut overlay: Query<(&mut Visibility, &mut BackgroundColor), With<Overlay>>,
    body: Query<Entity, With<Body>>,
) {
    if !session.dirty {
        return;
    }
    session.dirty = false;
    let step = match &session.stage {
        Stage::Loading { step, .. } => Some(step.label()),
        _ => None,
    };
    info!(
        "{}",
        serde_json::json!({"type": "session", "stage": session.stage.name(), "step": step,
            "error": session.menu.error})
    );
    let (Ok((mut visibility, mut backdrop)), Ok(body)) = (overlay.single_mut(), body.single())
    else {
        return;
    };
    *visibility = match session.stage {
        Stage::Playing { confirm: false } => Visibility::Hidden,
        _ => Visibility::Inherited,
    };
    backdrop.0 = match session.stage {
        Stage::Playing { .. } => Color::srgba(0.0, 0.0, 0.0, 0.55),
        _ => BACKDROP,
    };
    commands.entity(body).despawn_related::<Children>();
    commands
        .entity(body)
        .with_children(|body| match &session.stage {
            Stage::Title { .. } => title(
                body,
                "Preparing graphics. The first launch can take about half a minute.",
            ),
            Stage::Menu => menu(body, &session.menu),
            Stage::Loading { step, .. } => {
                heading(body, 40.0);
                line(
                    body,
                    format!("Step {} of 3: {}", step.number(), step.label()),
                    24.0,
                    chrome::INK,
                );
                spinner(body);
                if *step == Step::Building {
                    line(
                        body,
                        "Preparing the dungeon's look can take a while the first time.",
                        17.0,
                        chrome::MUTED,
                    );
                }
            }
            Stage::Playing { confirm: true } => confirm(body),
            Stage::Playing { confirm: false } => {}
            Stage::Closing { then, .. } => {
                heading(body, 40.0);
                line(
                    body,
                    match then {
                        Then::Menu => "Saving your game before returning to the menu",
                        Then::Quit => "Saving your game before closing",
                    },
                    24.0,
                    chrome::INK,
                );
                spinner(body);
            }
        });
}

fn spin(time: Res<Time>, mut spinners: Query<&mut Text, With<Spinner>>) {
    const MARKS: [&str; 4] = ["·", "· ·", "· · ·", "· ·"];
    let mark = MARKS[(time.elapsed_secs() * 3.0) as usize % MARKS.len()];
    for mut text in &mut spinners {
        if text.0 != mark {
            text.0 = mark.into();
        }
    }
}

fn heading(parent: &mut ChildSpawnerCommands, size: f32) {
    line(parent, "THRESHOLDS OF RUIN", size, chrome::GOLD);
}

fn line(parent: &mut ChildSpawnerCommands, text: impl Into<String>, size: f32, color: Color) {
    parent.spawn((
        Text::new(text),
        chrome::text(size, color),
        TextLayout::new_with_justify(JustifyText::Center),
    ));
}

fn spinner(parent: &mut ChildSpawnerCommands) {
    parent.spawn((Spinner, Text::new("·"), chrome::text(32.0, chrome::GOLD)));
}

fn title(parent: &mut ChildSpawnerCommands, detail: &str) {
    heading(parent, 64.0);
    line(parent, "A dungeon on this device", 22.0, chrome::INK);
    spinner(parent);
    line(parent, detail, 17.0, chrome::MUTED);
}

fn button(
    parent: &mut ChildSpawnerCommands,
    control: Control,
    label: impl Into<String>,
    selected: bool,
    width: f32,
) {
    parent
        .spawn((
            Button,
            control,
            BackgroundColor(chrome::BUTTON),
            BorderColor(if selected { chrome::INK } else { chrome::GOLD }),
            Node {
                min_width: Val::Px(width),
                min_height: Val::Px(52.0),
                padding: UiRect::axes(Val::Px(14.0), Val::Px(8.0)),
                border: UiRect::all(Val::Px(if selected { 3.0 } else { 1.0 })),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
        ))
        .with_child((
            Text::new(label),
            chrome::text(19.0, if selected { Color::WHITE } else { chrome::INK }),
            TextLayout::new_with_justify(JustifyText::Center),
        ));
}

fn row(
    parent: &mut ChildSpawnerCommands,
    gap: f32,
    children: impl FnOnce(&mut ChildSpawnerCommands),
) {
    parent
        .spawn(Node {
            flex_direction: FlexDirection::Row,
            flex_wrap: FlexWrap::Wrap,
            justify_content: JustifyContent::Center,
            column_gap: Val::Px(gap),
            row_gap: Val::Px(gap),
            ..default()
        })
        .with_children(children);
}

fn column(parent: &mut ChildSpawnerCommands, children: impl FnOnce(&mut ChildSpawnerCommands)) {
    parent
        .spawn(Node {
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            row_gap: Val::Px(10.0),
            ..default()
        })
        .with_children(children);
}

fn menu(body: &mut ChildSpawnerCommands, model: &MenuModel) {
    heading(body, 48.0);
    row(body, 12.0, |tabs| {
        button(
            tabs,
            Control::Tab(Tab::New),
            "New game",
            model.tab == Tab::New,
            200.0,
        );
        button(
            tabs,
            Control::Tab(Tab::Load),
            "Load game",
            model.tab == Tab::Load,
            200.0,
        );
    });
    if let Some(error) = &model.error {
        line(body, error.clone(), 18.0, WARNING);
    }
    match model.tab {
        Tab::New => row(body, 48.0, |sides| {
            column(sides, |list| {
                line(list, "Scenario", 18.0, chrome::MUTED);
                for (index, scenario) in model.scenarios.iter().enumerate() {
                    button(
                        list,
                        Control::Scenario(index),
                        scenario.label(),
                        index == model.scenario,
                        260.0,
                    );
                }
            });
            column(sides, |seed| {
                line(seed, "Seed", 18.0, chrome::MUTED);
                line(
                    seed,
                    if model.seed.is_empty() {
                        "—".to_owned()
                    } else {
                        model.seed.clone()
                    },
                    30.0,
                    Color::WHITE,
                );
                for digits in [[1, 2, 3], [4, 5, 6], [7, 8, 9]] {
                    row(seed, 8.0, |keys| {
                        for digit in digits {
                            button(keys, Control::Digit(digit), digit.to_string(), false, 64.0);
                        }
                    });
                }
                row(seed, 8.0, |keys| {
                    button(keys, Control::Backspace, "Delete", false, 64.0);
                    button(keys, Control::Digit(0), "0", false, 64.0);
                    button(keys, Control::Random, "Random", false, 64.0);
                });
            });
        }),
        Tab::Load => column(body, |list| {
            if model.saves.is_empty() {
                line(list, "No saved games yet.", 20.0, chrome::MUTED);
            }
            let start = model.page * PAGE;
            for (index, saved) in model.saves.iter().enumerate().skip(start).take(PAGE) {
                button(
                    list,
                    Control::Save(index),
                    saved.label(),
                    model.save == Some(index),
                    560.0,
                );
            }
            if model.pages() > 1 {
                row(list, 12.0, |pages| {
                    button(pages, Control::Page(false), "Newer", false, 120.0);
                    line(
                        pages,
                        format!("Page {} of {}", model.page + 1, model.pages()),
                        18.0,
                        chrome::MUTED,
                    );
                    button(pages, Control::Page(true), "Older", false, 120.0);
                });
            }
        }),
    }
    row(body, 16.0, |actions| {
        button(
            actions,
            Control::Start,
            match model.tab {
                Tab::New => "Start",
                Tab::Load => "Load",
            },
            true,
            200.0,
        );
        button(actions, Control::Quit, "Quit", false, 140.0);
    });
}

fn confirm(body: &mut ChildSpawnerCommands) {
    body.spawn((
        chrome::frame(),
        Node {
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            row_gap: Val::Px(14.0),
            padding: UiRect::all(Val::Px(28.0)),
            border: UiRect::all(Val::Px(2.0)),
            ..default()
        },
    ))
    .with_children(|panel| {
        line(panel, "Leave this game?", 26.0, chrome::INK);
        line(panel, "Your game is saved either way.", 17.0, chrome::MUTED);
        button(
            panel,
            Control::ReturnToMenu,
            "Save and return to menu",
            true,
            320.0,
        );
        button(panel, Control::SaveAndQuit, "Save and quit", false, 320.0);
        button(panel, Control::KeepPlaying, "Keep playing", false, 320.0);
    });
}

/// How long tests wait for the private server's background work.
#[cfg(test)]
const PATIENCE: Duration = Duration::from_secs(60);

#[cfg(test)]
mod tests;
