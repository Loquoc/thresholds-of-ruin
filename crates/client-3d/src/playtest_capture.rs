//! Explicit private captures of this game window, never the user's desktop.
//! View consumption is held until readback, so pixels cannot acquire a later
//! observation while retaining an old snapshot anchor.
use crate::{
    net::{Cmd, Input},
    Game, Net,
};
use bevy::{
    prelude::*,
    render::view::screenshot::{Screenshot, ScreenshotCaptured},
    window::PrimaryWindow,
};
use image::ImageEncoder;
use std::time::{Duration, Instant};
use tor_client_common::captures::CapturedAttachment;
use tor_protocol::{AttachmentMediaType, ReplyContext, MAX_ATTACHMENT_BYTES};

const DEADLINE: Duration = Duration::from_secs(15);

pub(crate) struct Pending {
    serial: u64,
    shown: ReplyContext,
    started: Instant,
    entity: Option<Entity>,
}

/// Reserve the current disclosure before requesting any asynchronous pixels.
pub(crate) fn request(net: &Net, game: &mut Game) {
    game.status = match begin(net, game) {
        Ok(()) => "Capturing game window privately; Escape cancels.".into(),
        Err(reason) => reason.into(),
    };
}

fn begin(net: &Net, game: &mut Game) -> Result<(), &'static str> {
    if game.capture.is_some() {
        return Err("A screenshot is already pending.");
    }
    if game.fatal {
        return Err("Cannot capture a disconnected session.");
    }
    let view = game.view.as_ref().ok_or("No presented state to capture.")?;
    let serial = game
        .capture_serial
        .checked_add(1)
        .ok_or("Capture sequence exhausted.")?;
    net.0.lock().unwrap().reserve_capture()?;
    game.capture_serial = serial;
    game.capture = Some(Pending {
        serial,
        shown: view.shown.clone(),
        started: Instant::now(),
        entity: None,
    });
    Ok(())
}

fn cancel(net: &Net, game: &mut Game, commands: &mut Commands, reason: &str) {
    if let Some(pending) = game.capture.take() {
        if let Some(entity) = pending.entity {
            commands.entity(entity).try_despawn();
        }
        net.0.lock().unwrap().release_capture();
        game.status = reason.into();
    }
}

pub(crate) fn keyboard(
    mut commands: Commands,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    net: Option<Res<Net>>,
    game: Option<ResMut<Game>>,
) {
    let (Some(net), Some(mut game)) = (net, game) else {
        return;
    };
    if game
        .capture
        .as_ref()
        .is_some_and(|p| p.started.elapsed() >= DEADLINE)
    {
        cancel(
            &net,
            &mut game,
            &mut commands,
            "Screenshot readback timed out; nothing uploaded.",
        );
    }
    if keys.just_pressed(KeyCode::Escape) {
        let had_capture = game.capture.is_some();
        cancel(
            &net,
            &mut game,
            &mut commands,
            "Screenshot cancelled before upload.",
        );
        if had_capture {
            keys.clear_just_pressed(KeyCode::Escape);
        }
    } else if keys.just_pressed(KeyCode::F12) {
        request(&net, &mut game);
    }
}

fn png(image: &Image) -> Result<Vec<u8>, String> {
    let size = image.texture_descriptor.size;
    if size.width == 0
        || size.height == 0
        || size.width > 4096
        || size.height > 4096
        || size.depth_or_array_layers != 1
    {
        return Err("Screenshot dimensions must be between 1 and 4096 pixels per side.".into());
    }
    let rgba = image
        .clone()
        .try_into_dynamic()
        .map_err(|e| e.to_string())?
        .into_rgba8();
    let mut bytes = Vec::new();
    image::codecs::png::PngEncoder::new(&mut bytes)
        .write_image(
            rgba.as_raw(),
            rgba.width(),
            rgba.height(),
            image::ExtendedColorType::Rgba8,
        )
        .map_err(|e| e.to_string())?;
    if bytes.len() > MAX_ATTACHMENT_BYTES {
        return Err("Screenshot exceeds the 8 MiB attachment limit.".into());
    }
    Ok(bytes)
}

fn completed(net: &Net, game: &mut Game, serial: u64, image: &Image) {
    if game.capture.as_ref().is_none_or(|p| p.serial != serial) {
        return;
    }
    let pending = game.capture.take().unwrap();
    net.0.lock().unwrap().release_capture();
    if pending.started.elapsed() >= DEADLINE || game.fatal {
        game.status = "Screenshot expired or disconnected; nothing uploaded.".into();
        return;
    }
    // Snapshot consumption is frozen; still reject any unexpected replacement
    // rather than silently attaching pixels to an unrelated history state.
    if game.view.as_ref().is_none_or(|v| v.shown != pending.shown) {
        game.status = "Presented state changed during screenshot; nothing uploaded.".into();
        return;
    }
    let capture = png(image).and_then(|bytes| {
        CapturedAttachment::new(
            pending.shown.input.clone(),
            pending.shown.branch.clone(),
            pending.shown.snapshot.clone(),
            AttachmentMediaType::Png,
            bytes,
        )
    });
    match capture {
        Ok(capture) => match net.0.lock().unwrap().try_command(Input {
            shown: pending.shown,
            command: Cmd::Capture(capture),
        }) {
            Ok(()) => {
                game.busy = true;
                game.status = "Uploading private screenshot…".into();
            }
            Err(error) => game.status = format!("Screenshot not uploaded: {error}"),
        },
        Err(error) => game.status = format!("Screenshot not uploaded: {error}"),
    }
}

/// Last runs after UI layout and scene reconciliation. Bevy copies this exact
/// render target; completion may be several frames later, without a re-capture.
pub(crate) fn capture(
    mut commands: Commands,
    net: Option<Res<Net>>,
    game: Option<ResMut<Game>>,
    windows: Query<&Window, With<PrimaryWindow>>,
) {
    let (Some(net), Some(mut game)) = (net, game) else {
        return;
    };
    let Some(pending) = &game.capture else {
        return;
    };
    if pending.entity.is_some() {
        return;
    }
    let Ok(window) = windows.single() else {
        cancel(
            &net,
            &mut game,
            &mut commands,
            "No game window available; nothing uploaded.",
        );
        return;
    };
    if window.physical_width() == 0
        || window.physical_height() == 0
        || window.physical_width() > 4096
        || window.physical_height() > 4096
    {
        cancel(
            &net,
            &mut game,
            &mut commands,
            "Game window exceeds screenshot dimensions; nothing uploaded.",
        );
        return;
    }
    let serial = pending.serial;
    let entity = commands
        .spawn(Screenshot::primary_window())
        .observe(
            move |trigger: Trigger<ScreenshotCaptured>, net: Res<Net>, mut game: ResMut<Game>| {
                completed(&net, &mut game, serial, &trigger.event().0);
            },
        )
        .id();
    game.capture.as_mut().unwrap().entity = Some(entity);
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::{
        asset::RenderAssetUsages,
        render::render_resource::{Extent3d, TextureDimension, TextureFormat},
    };

    fn fixture() -> (Net, Game, tokio::sync::mpsc::Receiver<Input>) {
        let (net, receiver) = crate::net::test_channel();
        let mut view = crate::test_support::view(7);
        view.has_control = true;
        (
            net,
            Game {
                view: Some(view),
                ..default()
            },
            receiver,
        )
    }
    fn pixels() -> Image {
        Image::new(
            Extent3d {
                width: 2,
                height: 1,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            vec![255, 0, 0, 255, 0, 255, 0, 255],
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::MAIN_WORLD,
        )
    }
    #[test]
    fn screenshot_command_reserves_the_shown_frame_before_readback() {
        let (net, mut game, mut rx) = fixture();
        let shown = game.view.as_ref().unwrap().shown.clone();
        request(&net, &mut game);
        assert!(
            game.capture.is_some(),
            "explicit screenshot must reserve actual shown frame"
        );
        assert_eq!(game.capture.as_ref().unwrap().shown, shown);
        assert!(
            rx.try_recv().is_err(),
            "never upload before GPU pixels arrive"
        );
    }
    #[test]
    fn png_preserves_actual_pixels_and_rejects_oversized_or_empty_dimensions() {
        let bytes = png(&pixels()).unwrap();
        assert_eq!(
            image::load_from_memory(&bytes).unwrap().to_rgba8().as_raw(),
            &vec![255, 0, 0, 255, 0, 255, 0, 255]
        );
        for width in [0, 4097] {
            let mut image = pixels();
            image.texture_descriptor.size.width = width;
            assert!(png(&image).is_err());
        }
    }
    #[test]
    fn repeated_requests_and_late_callbacks_never_duplicate_uploads() {
        let (net, mut game, mut rx) = fixture();
        begin(&net, &mut game).unwrap();
        let serial = game.capture.as_ref().unwrap().serial;
        assert!(begin(&net, &mut game).is_err());
        assert!(
            !game.send(&net, Cmd::Wait),
            "game input cannot overtake pending capture"
        );
        completed(&net, &mut game, serial, &pixels());
        let input = rx.try_recv().unwrap();
        assert!(matches!(input.command, Cmd::Capture(_)));
        assert_eq!(
            input.shown.snapshot,
            crate::test_support::view(7).shown.snapshot
        );
        completed(&net, &mut game, serial, &pixels());
        assert!(rx.try_recv().is_err());
    }
    #[test]
    fn stale_timed_out_disconnected_and_missing_view_captures_never_upload() {
        for failure in 0..4 {
            let (net, mut game, mut rx) = fixture();
            if failure == 3 {
                game.view = None;
                assert!(begin(&net, &mut game).is_err());
            } else {
                begin(&net, &mut game).unwrap();
                let serial = game.capture.as_ref().unwrap().serial;
                match failure {
                    0 => game.view = Some(crate::test_support::view(8)),
                    1 => game.capture.as_mut().unwrap().started -= DEADLINE,
                    _ => game.fatal = true,
                }
                completed(&net, &mut game, serial, &pixels());
                assert!(game.capture.is_none());
            }
            assert!(rx.try_recv().is_err());
        }
    }
    #[test]
    fn escape_cancels_before_upload_and_stale_readback_cannot_complete_next_request() {
        let (net, mut game, mut rx) = fixture();
        begin(&net, &mut game).unwrap();
        let old = game.capture.as_ref().unwrap().serial;
        let mut app = App::new();
        let mut keys = ButtonInput::<KeyCode>::default();
        keys.press(KeyCode::Escape);
        app.insert_resource(net)
            .insert_resource(game)
            .insert_resource(keys)
            .add_systems(Update, keyboard);
        app.update();
        assert!(app.world().resource::<Game>().capture.is_none());
        assert!(
            !app.world()
                .resource::<ButtonInput<KeyCode>>()
                .just_pressed(KeyCode::Escape),
            "capture cancellation must not also quit the game"
        );
        assert!(rx.try_recv().is_err());
        app.world_mut()
            .resource_scope(|world, mut game: Mut<Game>| {
                let net = world.resource::<Net>();
                begin(net, &mut game).unwrap();
                let current = game.capture.as_ref().unwrap().serial;
                assert_ne!(current, old);
                completed(net, &mut game, old, &pixels());
                assert_eq!(game.capture.as_ref().unwrap().serial, current);
                assert!(rx.try_recv().is_err());
                completed(net, &mut game, current, &pixels());
            });
        assert!(matches!(rx.try_recv().unwrap().command, Cmd::Capture(_)));
    }
    #[test]
    fn player_observer_capture_does_not_take_gameplay_control() {
        let (net, mut game, mut rx) = fixture();
        game.view.as_mut().unwrap().has_control = false;
        request(&net, &mut game);
        let serial = game.capture.as_ref().unwrap().serial;
        completed(&net, &mut game, serial, &pixels());
        assert!(matches!(rx.try_recv().unwrap().command, Cmd::Capture(_)));
        assert!(!game.view.as_ref().unwrap().has_control);
        assert!(
            rx.try_recv().is_err(),
            "capture must not issue AcquireControl"
        );
    }
    #[test]
    fn native_input_stage_starts_and_cancels_capture_before_gameplay_handlers() {
        use bevy::input::{
            keyboard::{Key, KeyboardInput},
            ButtonState, InputPlugin, InputSystem,
        };
        #[derive(Resource, Default)]
        struct Escaped(bool);
        let (net, game, mut rx) = fixture();
        let mut app = App::new();
        app.add_plugins(InputPlugin)
            .insert_resource(net)
            .insert_resource(game)
            .init_resource::<Escaped>()
            .add_systems(PreUpdate, keyboard.after(InputSystem))
            .add_systems(
                Update,
                |keys: Res<ButtonInput<KeyCode>>, mut escaped: ResMut<Escaped>| {
                    escaped.0 = keys.just_pressed(KeyCode::Escape);
                },
            );
        for (code, logical) in [(KeyCode::F12, Key::F12), (KeyCode::Escape, Key::Escape)] {
            app.world_mut().send_event(KeyboardInput {
                key_code: code,
                logical_key: logical,
                state: ButtonState::Pressed,
                text: None,
                repeat: false,
                window: Entity::PLACEHOLDER,
            });
            app.update();
            assert_eq!(
                app.world().resource::<Game>().capture.is_some(),
                code == KeyCode::F12
            );
        }
        assert!(
            !app.world().resource::<Escaped>().0,
            "Escape cancellation must precede gameplay quit handling"
        );
        assert!(rx.try_recv().is_err());
    }
}
