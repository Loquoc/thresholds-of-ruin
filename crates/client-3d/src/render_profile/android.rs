//! The parts of a render profile that only the experimental Android client
//! has: a smaller drawing surface, thread pinning and touch as a pointer.
use super::{RenderProfile, ScaleGovernor};
use bevy::{prelude::*, window::PrimaryWindow};

/// The screen size and scale factor before the surface was shrunk.
#[derive(Resource, Default)]
struct Native(Option<(UVec2, f32)>);

/// Runs after `Last`. The window system restores the screen's size during
/// `Last`, so the smaller size set here is the one the renderer sees.
#[derive(bevy::ecs::schedule::ScheduleLabel, Clone, Debug, PartialEq, Eq, Hash)]
struct AfterLast;

pub(super) fn build(app: &mut App, profile: &RenderProfile) {
    app.init_resource::<Native>()
        .add_systems(PreUpdate, touch.after(bevy::input::InputSystem));
    // Review scenes fit hair inside the frame by default; this hardware cannot.
    if !app
        .world()
        .contains_resource::<crate::creatures::HairSolving>()
    {
        app.insert_resource(crate::creatures::HairSolving::Background);
    }
    if profile.scale < 1.0 {
        app.world_mut()
            .resource_mut::<bevy::app::MainScheduleOrder>()
            .insert_after(Last, AfterLast);
        app.add_systems(AfterLast, (native, pace, shrink).chain());
    }
    // Pinning needs the serial executor, which keeps a schedule on one thread.
    if profile.pin_threads && profile.scheduling == super::Scheduling::Serial {
        app.add_systems(First, pin);
        if let Some(render) = app.get_sub_app_mut(bevy::render::RenderApp) {
            render.add_systems(bevy::render::Render, pin);
        }
    }
}

/// Records the screen's own size once it has held for ten frames; the first
/// few report the size the client asked for instead.
fn native(
    mut native: ResMut<Native>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut steady: Local<(UVec2, u32)>,
) {
    let (None, Ok(window)) = (native.0, windows.single()) else {
        return;
    };
    let size = window.physical_size();
    if steady.0 == size {
        steady.1 += 1;
    } else {
        *steady = (size, 0);
    }
    if size.x > 0 && size.y > 0 && steady.1 >= 10 {
        native.0 = Some((size, window.resolution.scale_factor()));
    }
}

/// Every two seconds, compares the pace with thirty frames a second.
fn pace(
    profile: Res<RenderProfile>,
    time: Res<Time<Real>>,
    mut governor: ResMut<ScaleGovernor>,
    mut window: Local<(f64, u32)>,
) {
    if !profile.adaptive_scale {
        return;
    }
    let now = time.elapsed_secs_f64();
    window.1 += 1;
    let seconds = now - window.0;
    if seconds < 2.0 {
        return;
    }
    let mean = seconds * 1000.0 / f64::from(window.1);
    *window = (now, 0);
    if governor.observe(mean, seconds) {
        info!(
            "Render scale now {:.2} after a mean frame of {mean:.1} ms",
            governor.scale(&profile)
        );
    }
}

/// Sizes the drawing surface to the governed scale. The system compositor
/// stretches it over the screen; the interface keeps its layout because the
/// scale factor shrinks with it.
fn shrink(
    profile: Res<RenderProfile>,
    native: Res<Native>,
    governor: Res<ScaleGovernor>,
    mut windows: Query<&mut Window, With<PrimaryWindow>>,
) {
    let (Some((size, factor)), Ok(mut window)) = (native.0, windows.single_mut()) else {
        return;
    };
    let scale = governor.scale(&profile);
    let small = (size.as_vec2() * scale).round().as_uvec2();
    if window.physical_size() != small {
        let wanted = factor * scale;
        if window.resolution.scale_factor_override() != Some(wanted) {
            window.resolution.set_scale_factor_override(Some(wanted));
        }
        window.resolution.set_physical_resolution(small.x, small.y);
    }
}

/// Lets one finger act as the mouse the client already understands: it moves
/// the pointer to the touch and holds the left button while it stays down.
fn touch(
    touches: Res<Touches>,
    profile: Res<RenderProfile>,
    governor: Res<ScaleGovernor>,
    mut mouse: ResMut<ButtonInput<MouseButton>>,
    mut windows: Query<&mut Window, With<PrimaryWindow>>,
    mut held: Local<Option<u64>>,
) {
    let Ok(mut window) = windows.single_mut() else {
        return;
    };
    // Touches arrive in screen pixels; a shrunken surface has fewer of them.
    let scale = governor.scale(&profile);
    match *held {
        None => {
            if let Some(finger) = touches.iter_just_pressed().next() {
                *held = Some(finger.id());
                window.set_cursor_position(Some(finger.position() * scale));
                mouse.press(MouseButton::Left);
            }
        }
        Some(id) => match touches.get_pressed(id) {
            Some(finger) => window.set_cursor_position(Some(finger.position() * scale)),
            None => {
                mouse.release(MouseButton::Left);
                *held = None;
            }
        },
    }
}

/// The cores whose top speed is the highest, when cores differ.
fn fastest_cores() -> Vec<usize> {
    let speeds: Vec<(usize, u64)> = (0..64)
        .filter_map(|core| {
            let path = format!("/sys/devices/system/cpu/cpu{core}/cpufreq/cpuinfo_max_freq");
            let speed = std::fs::read_to_string(path).ok()?.trim().parse().ok()?;
            Some((core, speed))
        })
        .collect();
    let top = speeds.iter().map(|(_, speed)| *speed).max().unwrap_or(0);
    let fastest: Vec<usize> = speeds
        .iter()
        .filter(|(_, speed)| *speed == top)
        .map(|(core, _)| *core)
        .collect();
    if fastest.len() == speeds.len() {
        Vec::new()
    } else {
        fastest
    }
}

/// Keeps the thread that runs this schedule on the fastest cores.
fn pin(mut done: Local<bool>) {
    if *done {
        return;
    }
    *done = true;
    let cores = fastest_cores();
    if let (Some(pinner), false) = (crate::THREAD_PINNER.get(), cores.is_empty()) {
        pinner(&cores);
    }
}
