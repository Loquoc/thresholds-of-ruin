//! Persistent live HUD. It presents disclosed facts and reuses existing actions.
//!
//! The world keeps most of the window: health and status sit in a compact
//! bottom-left corner, actions fold into a bottom-centre menu, and new messages
//! fade from the top-left after a few seconds.
use crate::{chrome, interact, net::Cmd, screen, Game, Net};
use bevy::{
    asset::RenderAssetUsages,
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
    ui::FocusPolicy,
};

const ORB_SIZE: usize = 128;
/// Seconds a new message stays fully shown, then the seconds it takes to fade.
const LOG_HOLD: f32 = 6.0;
const LOG_FADE: f32 = 1.0;
const LOG_BACKDROP: f32 = 0.5;

#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub(crate) enum Action {
    /// Unfolds or folds the action menu.
    Menu,
    Inventory,
    Equipment,
    Character,
    Map,
    Places,
    Journal,
    Screenshot,
    Wait,
    /// Standalone play: ask to save and leave for the title menu.
    Leave,
    /// A touch stand-in for a key: pressed for one frame, as the key would be.
    Press(KeyCode),
    /// Moves the camera nearer (negative) or farther, as the wheel does.
    Zoom(f32),
}

/// On-screen buttons for actions otherwise only on the keyboard or wheel.
const TOUCH: [(Action, &str); 8] = [
    (
        Action::Press(KeyCode::KeyQ),
        "Turn
left",
    ),
    (
        Action::Press(KeyCode::KeyE),
        "Turn
right",
    ),
    (
        Action::Zoom(-2.5),
        "Zoom
in",
    ),
    (
        Action::Zoom(2.5),
        "Zoom
out",
    ),
    (
        Action::Press(KeyCode::KeyR),
        "Stairs
up",
    ),
    (
        Action::Press(KeyCode::KeyF),
        "Stairs
down",
    ),
    (
        Action::Press(KeyCode::KeyX),
        "Open
door",
    ),
    (
        Action::Press(KeyCode::KeyG),
        "Pick
up",
    ),
];

/// Shows the touch buttons; present on Android or with --touch-controls.
#[derive(Resource, Default)]
pub(crate) struct TouchControls {
    pending: Vec<KeyCode>,
    held: Vec<KeyCode>,
}

/// Presses the keys touch buttons asked for after the frame's real input is
/// read, and releases them a frame later, so every key reader sees one press.
pub(crate) fn touch_keys(mut touch: ResMut<TouchControls>, mut keys: ResMut<ButtonInput<KeyCode>>) {
    let touch = &mut *touch;
    for code in touch.held.drain(..) {
        keys.release(code);
    }
    for code in std::mem::take(&mut touch.pending) {
        keys.press(code);
        touch.held.push(code);
    }
}

const ACTIONS: [(Action, &str, &str); 8] = [
    (Action::Inventory, "Inventory", "I"),
    (Action::Equipment, "Equipment", "F6"),
    (Action::Character, "Character", "F10"),
    (Action::Map, "Map", "M"),
    (Action::Places, "Places", "F5"),
    (Action::Journal, "Journal", "F2"),
    (Action::Screenshot, "Screenshot", "F12"),
    (Action::Wait, "Wait", "Space"),
];

#[derive(Component, Debug)]
pub(crate) enum Field {
    Health,
    Readiness,
    Status,
    Message,
    Log,
}

#[derive(Component)]
pub(crate) struct LogFrame;

/// The prompt line under the readiness badge; hidden while it has nothing to say.
#[derive(Component)]
pub(crate) struct PromptFrame;

/// The folding list of dock actions above the Menu button.
#[derive(Component)]
pub(crate) struct MenuList;

/// Whether the action menu is unfolded.
#[derive(Resource, Default)]
pub(crate) struct Menu {
    pub(crate) open: bool,
}

/// When the newest message arrived, by the count of lines received.
#[derive(Resource, Default)]
pub(crate) struct LogFade {
    seen: u64,
    since: f32,
}

#[derive(Resource)]
pub(crate) struct HealthOrb {
    image: Handle<Image>,
    shown: Option<(u64, u64)>,
}

impl HealthOrb {
    /// Repaints the globe for the next game, even when its health is unknown.
    pub(crate) fn forget(&mut self) {
        self.shown = Some((u64::MAX, 0));
    }
}

pub(crate) fn readiness(game: &Game) -> &'static str {
    if game.fatal {
        return "DISCONNECTED";
    }
    let Some(view) = &game.view else {
        return "CONNECTING";
    };
    if !view.has_control {
        return "OBSERVING";
    }
    if view.hp.is_some_and(|(hp, _)| hp == 0) {
        return "FALLEN";
    }
    if game.busy {
        return "WORKING";
    }
    if view.can_act {
        "READY"
    } else {
        "WAITING"
    }
}

/// Original software-painted globe, updated only when disclosed health changes.
/// Unknown health is an empty dark globe, never a fabricated full resource.
fn orb_pixels(health: Option<(u64, u64)>) -> Vec<u8> {
    let fraction = health
        .filter(|(_, max)| *max > 0)
        .map_or(0.0, |(hp, max)| hp.min(max) as f64 / max as f64);
    let mut bytes = Vec::with_capacity(ORB_SIZE * ORB_SIZE * 4);
    for y in 0..ORB_SIZE {
        for x in 0..ORB_SIZE {
            let nx = (x as f64 + 0.5) / ORB_SIZE as f64 * 2.0 - 1.0;
            let ny = (y as f64 + 0.5) / ORB_SIZE as f64 * 2.0 - 1.0;
            let r2 = nx * nx + ny * ny;
            if r2 > 1.0 {
                bytes.extend_from_slice(&[0, 0, 0, 0]);
                continue;
            }
            let rim = r2 > 0.87;
            let filled = (y as f64 + 0.5) / ORB_SIZE as f64 >= 1.0 - fraction;
            let shine = (1.0 - ((nx + 0.32).powi(2) + (ny + 0.43).powi(2)) * 7.0).max(0.0);
            let depth = (1.0 - r2).sqrt();
            let rgb = if rim {
                [
                    100.0 + depth * 110.0,
                    73.0 + depth * 82.0,
                    37.0 + depth * 50.0,
                ]
            } else if filled {
                [
                    45.0 + depth * 115.0 + shine * 80.0,
                    5.0 + shine * 60.0,
                    12.0 + shine * 45.0,
                ]
            } else {
                [12.0 + shine * 20.0, 9.0 + shine * 17.0, 11.0 + shine * 18.0]
            };
            bytes.extend(rgb.map(|v| v.min(255.0) as u8));
            bytes.push(255);
        }
    }
    bytes
}

fn action_button(action: Action, label: impl Into<String>) -> impl Bundle {
    (
        Button,
        action,
        BackgroundColor(chrome::BUTTON),
        BorderColor(chrome::GOLD),
        Node {
            padding: UiRect::axes(Val::Px(12.0), Val::Px(5.0)),
            border: UiRect::all(Val::Px(1.0)),
            justify_content: JustifyContent::Center,
            ..default()
        },
        children![(
            Text::new(label),
            chrome::text(16.0, chrome::INK),
            TextLayout::new_with_justify(JustifyText::Center),
        )],
    )
}

pub(crate) fn setup(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    session: Option<Res<crate::session::Session>>,
    touch: Option<Res<TouchControls>>,
    settings: Option<Res<crate::settings::Settings>>,
) {
    let settings = settings
        .map(|settings| settings.clone())
        .unwrap_or_default();
    let (standalone, touch) = (session.is_some(), touch.is_some());
    let image = images.add(Image::new(
        Extent3d {
            width: ORB_SIZE as u32,
            height: ORB_SIZE as u32,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        orb_pixels(None),
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    ));
    commands.insert_resource(HealthOrb {
        image: image.clone(),
        shown: None,
    });
    commands.init_resource::<Menu>();
    commands.init_resource::<LogFade>();

    // Readiness badge, with the current prompt beneath it.
    commands
        .spawn((
            FocusPolicy::Pass,
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(14.0),
                width: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: Val::Px(6.0),
                ..default()
            },
        ))
        .with_children(|root| {
            root.spawn((
                chrome::BlockWorld,
                Interaction::default(),
                chrome::frame(),
                Node {
                    padding: UiRect::axes(Val::Px(22.0), Val::Px(6.0)),
                    border: UiRect::all(Val::Px(2.0)),
                    ..default()
                },
            ))
            .with_child((
                Field::Readiness,
                Text::new("CONNECTING"),
                chrome::text(15.0, chrome::INK),
            ));
            root.spawn((
                PromptFrame,
                Visibility::Hidden,
                BackgroundColor(Color::srgba(0.0, 0.0, 0.0, LOG_BACKDROP)),
                BorderRadius::all(Val::Px(3.0)),
                Node {
                    max_width: Val::Percent(36.0),
                    padding: UiRect::axes(Val::Px(10.0), Val::Px(4.0)),
                    ..default()
                },
            ))
            .with_child((
                Field::Message,
                Text::new(""),
                chrome::text(15.0, chrome::INK),
                TextLayout::new_with_justify(JustifyText::Center),
            ));
        });

    // New messages, top left. They do not block the world and fade on their own.
    commands
        .spawn((
            LogFrame,
            Visibility::Hidden,
            FocusPolicy::Pass,
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, LOG_BACKDROP)),
            BorderRadius::all(Val::Px(3.0)),
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(16.0),
                top: Val::Px(14.0),
                max_width: Val::Percent(30.0),
                padding: UiRect::axes(Val::Px(10.0), Val::Px(6.0)),
                ..default()
            },
        ))
        .with_child((Field::Log, Text::new(""), chrome::text(16.0, chrome::INK)));

    // Health and status, bottom left.
    commands
        .spawn((
            chrome::BlockWorld,
            Interaction::default(),
            chrome::frame(),
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(12.0),
                bottom: Val::Px(12.0),
                max_width: Val::Percent(38.0),
                padding: UiRect::all(Val::Px(6.0)),
                border: UiRect::all(Val::Px(1.0)),
                column_gap: Val::Px(10.0),
                align_items: AlignItems::Center,
                ..default()
            },
        ))
        .with_children(|corner| {
            corner
                .spawn((
                    ImageNode::new(image),
                    Node {
                        width: Val::Px(72.0),
                        height: Val::Px(72.0),
                        flex_shrink: 0.0,
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        ..default()
                    },
                ))
                .with_child((
                    Field::Health,
                    Text::new("— / —"),
                    chrome::text(15.0, Color::WHITE),
                    TextLayout::new_with_justify(JustifyText::Center),
                ));
            corner.spawn((
                Field::Status,
                Text::new(""),
                chrome::text(15.0, chrome::INK),
                Node {
                    min_width: Val::Px(0.0),
                    flex_shrink: 1.0,
                    ..default()
                },
            ));
        });

    // Actions, folded into a bottom-centre menu.
    commands
        .spawn((
            FocusPolicy::Pass,
            GlobalZIndex(1),
            Node {
                position_type: PositionType::Absolute,
                bottom: Val::Px(12.0),
                width: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: Val::Px(6.0),
                ..default()
            },
        ))
        .with_children(|root| {
            root.spawn((
                MenuList,
                Visibility::Hidden,
                chrome::BlockWorld,
                Interaction::default(),
                chrome::frame(),
                Node {
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Stretch,
                    padding: UiRect::all(Val::Px(8.0)),
                    border: UiRect::all(Val::Px(1.0)),
                    row_gap: Val::Px(4.0),
                    min_width: Val::Px(200.0),
                    ..default()
                },
            ))
            .with_children(|list| {
                for (action, name, key) in ACTIONS {
                    list.spawn(action_button(action, format!("{name}  [{key}]")));
                }
                if standalone {
                    list.spawn(action_button(Action::Leave, "Leave  [Esc]"));
                }
                // Settings flip in place and stay in the open menu.
                list.spawn((
                    Text::new("Settings"),
                    chrome::text(13.0, chrome::GOLD),
                    Node {
                        margin: UiRect::top(Val::Px(6.0)),
                        ..default()
                    },
                ));
                for setting in [
                    crate::settings::Setting::Peaceful,
                    crate::settings::Setting::Unseen,
                ] {
                    list.spawn(crate::settings::toggle_button(setting, &settings));
                }
            });
            // Touch stand-ins for keys stay one tap away rather than folded.
            if touch {
                root.spawn((
                    chrome::BlockWorld,
                    Interaction::default(),
                    Node {
                        flex_wrap: FlexWrap::Wrap,
                        justify_content: JustifyContent::Center,
                        column_gap: Val::Px(6.0),
                        row_gap: Val::Px(4.0),
                        max_width: Val::Percent(60.0),
                        ..default()
                    },
                ))
                .with_children(|buttons| {
                    for (action, label) in TOUCH {
                        buttons.spawn(action_button(action, label));
                    }
                });
            }
            root.spawn((chrome::BlockWorld, action_button(Action::Menu, "Menu")));
        });
}

pub(crate) fn update(
    game: Res<Game>,
    mut fields: Query<(&Field, &mut Text)>,
    mut orb: ResMut<HealthOrb>,
    mut images: ResMut<Assets<Image>>,
    mut prompts: Query<&mut Visibility, With<PromptFrame>>,
) {
    if !game.is_changed() {
        return;
    }
    let hp = game.view.as_ref().and_then(|v| v.hp);
    if orb.shown != hp {
        if let Some(image) = images.get_mut(&orb.image) {
            image.data = Some(orb_pixels(hp));
        }
        orb.shown = hp;
    }
    for (field, mut text) in &mut fields {
        let value = match field {
            Field::Readiness => readiness(&game).to_owned(),
            Field::Health => hp.map_or_else(|| "— / —".into(), |(hp, max)| format!("{hp} / {max}")),
            Field::Status => game.view.as_ref().map_or_else(String::new, |v| {
                v.status
                    .iter()
                    .filter(|s| !s.is_empty())
                    .cloned()
                    .collect::<Vec<_>>()
                    .join("\n")
            }),
            Field::Message => {
                let value = prompt(&game);
                for mut visibility in &mut prompts {
                    visibility.set_if_neq(if value.is_empty() {
                        Visibility::Hidden
                    } else {
                        Visibility::Inherited
                    });
                }
                value
            }
            Field::Log => game
                .log
                .iter()
                .rev()
                .take(4)
                .rev()
                .cloned()
                .collect::<Vec<_>>()
                .join("\n"),
        };
        if text.0 != value {
            text.0 = value;
        }
    }
}

/// Status text and the actions the server currently offers here.
fn prompt(game: &Game) -> String {
    let mut parts = Vec::new();
    if !game.status.is_empty() {
        parts.push(game.status.clone());
    }
    if let Some(view) = &game.view {
        if view.can_resume {
            parts.push("F8: resume".into());
        }
        if view.can_cancel {
            parts.push("F9: cancel".into());
        }
        let here: Vec<_> = view
            .items
            .iter()
            .filter(|i| i.position == view.feet)
            .map(|i| i.name.as_str())
            .collect();
        if !here.is_empty() {
            parts.push(format!("G: take · {}", here.join(", ")));
        }
    }
    parts.join("   |   ")
}

/// Shows the latest messages when they arrive, then fades them out.
pub(crate) fn fade_log(
    time: Res<Time>,
    game: Res<Game>,
    mut fade: ResMut<LogFade>,
    mut frames: Query<(&mut Visibility, &mut BackgroundColor), With<LogFrame>>,
    mut lines: Query<(&Field, &mut TextColor)>,
) {
    let now = time.elapsed_secs();
    if fade.seen != game.log_received {
        fade.seen = game.log_received;
        fade.since = now;
    }
    let alpha = if game.log.is_empty() {
        0.0
    } else {
        ((LOG_HOLD + LOG_FADE - (now - fade.since)) / LOG_FADE).clamp(0.0, 1.0)
    };
    for (mut visibility, mut backdrop) in &mut frames {
        visibility.set_if_neq(if alpha > 0.0 {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
        let shade = Color::srgba(0.0, 0.0, 0.0, LOG_BACKDROP * alpha);
        if backdrop.0 != shade {
            backdrop.0 = shade;
        }
    }
    for (field, mut color) in &mut lines {
        if matches!(field, Field::Log) {
            let ink = chrome::INK.with_alpha(alpha);
            if color.0 != ink {
                color.0 = ink;
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn buttons(
    net: Res<Net>,
    mut game: ResMut<Game>,
    mut panel: ResMut<interact::Panel>,
    mut menu: ResMut<Menu>,
    mut buttons: Query<(&Interaction, &Action, &mut BackgroundColor), Changed<Interaction>>,
    mut lists: Query<&mut Visibility, With<MenuList>>,
    mut touch: Option<ResMut<TouchControls>>,
    mut rig: Option<ResMut<crate::CameraRig>>,
    mut session: Option<ResMut<crate::session::Session>>,
) {
    for (interaction, action, mut color) in &mut buttons {
        color.0 = if *interaction == Interaction::None {
            chrome::BUTTON
        } else {
            chrome::HOT
        };
        if *interaction != Interaction::Pressed {
            continue;
        }
        // Choosing an action folds the menu away; Wait may be pressed
        // repeatedly, and touch stand-ins for keys leave it as it is.
        match action {
            Action::Menu => menu.open = !menu.open,
            Action::Wait | Action::Press(_) | Action::Zoom(_) => {}
            _ => menu.open = false,
        }
        match action {
            Action::Menu => {}
            Action::Screenshot => crate::playtest_capture::request(&net, &mut game),
            Action::Inventory | Action::Equipment => {
                if let Some(open) = game.screen.clone() {
                    screen::toggle(&net, &mut game, open);
                }
                panel.toggle_page(if matches!(action, Action::Equipment) {
                    crate::equipment::Page::Equipment
                } else {
                    crate::equipment::Page::Inventory
                });
            }
            Action::Character => {
                panel.open = false;
                screen::toggle(&net, &mut game, screen::Screen::Advancement);
            }
            Action::Places => {
                panel.open = false;
                screen::toggle(&net, &mut game, screen::Screen::Places { renaming: None });
            }
            Action::Journal => {
                panel.open = false;
                screen::toggle(&net, &mut game, screen::Screen::History);
            }
            Action::Map => {
                panel.open = false;
                screen::toggle(&net, &mut game, screen::Screen::Places { renaming: None });
            }
            Action::Wait
                if !panel.open
                    && game.screen.is_none()
                    && !game.busy
                    && !game.fatal
                    && game
                        .view
                        .as_ref()
                        .is_some_and(|v| v.has_control && v.can_act) =>
            {
                game.send(&net, Cmd::Wait);
            }
            Action::Wait => {}
            Action::Leave => {
                if let Some(session) = session.as_mut() {
                    session.ask_to_leave();
                }
            }
            Action::Press(code) => {
                if let Some(touch) = touch.as_mut() {
                    touch.pending.push(*code);
                }
            }
            Action::Zoom(step) => {
                if let Some(rig) = rig.as_mut() {
                    rig.distance = (rig.distance + step).clamp(4.0, 32.0);
                }
            }
        }
    }
    for mut visibility in &mut lists {
        visibility.set_if_neq(if menu.open {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn log_app() -> (App, Entity) {
        let mut app = App::new();
        app.init_resource::<Assets<Image>>()
            .init_resource::<Game>()
            .init_resource::<Time>()
            .add_systems(Startup, setup)
            .add_systems(Update, (update, fade_log).chain());
        app.update();
        let root = app
            .world_mut()
            .query_filtered::<Entity, With<LogFrame>>()
            .single(app.world())
            .unwrap();
        (app, root)
    }

    fn receive(app: &mut App, line: &str) {
        let mut game = app.world_mut().resource_mut::<Game>();
        game.log.push_back(line.into());
        game.log_received += 1;
    }

    fn wait(app: &mut App, seconds: f32) {
        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(std::time::Duration::from_secs_f32(seconds));
        app.update();
    }

    #[test]
    fn an_empty_message_log_does_not_leave_a_blank_panel_over_the_world() {
        let (mut app, root) = log_app();
        assert_eq!(
            *app.world().get::<Visibility>(root).unwrap(),
            Visibility::Hidden
        );
        receive(&mut app, "You move north.");
        app.update();
        assert_eq!(
            *app.world().get::<Visibility>(root).unwrap(),
            Visibility::Inherited
        );
    }

    #[test]
    fn messages_fade_after_a_while_and_a_new_one_brings_them_back() {
        let (mut app, root) = log_app();
        let backdrop = |app: &App| app.world().get::<BackgroundColor>(root).unwrap().0.alpha();
        let visibility = |app: &App| *app.world().get::<Visibility>(root).unwrap();
        receive(&mut app, "You move north.");
        app.update();
        wait(&mut app, LOG_HOLD - 0.5);
        assert_eq!(
            visibility(&app),
            Visibility::Inherited,
            "still within the hold"
        );
        assert!((backdrop(&app) - LOG_BACKDROP).abs() < 1e-4);
        wait(&mut app, 1.0);
        assert_eq!(
            visibility(&app),
            Visibility::Inherited,
            "fading, not yet gone"
        );
        assert!(backdrop(&app) > 0.0 && backdrop(&app) < LOG_BACKDROP);
        wait(&mut app, LOG_FADE);
        assert_eq!(
            visibility(&app),
            Visibility::Hidden,
            "faded messages leave the world clear"
        );
        assert!(
            !app.world().resource::<Game>().log.is_empty(),
            "fading is presentation only; the log keeps its lines"
        );
        // A repeat of the same words is still a new message.
        receive(&mut app, "You move north.");
        app.update();
        assert_eq!(visibility(&app), Visibility::Inherited);
        assert!((backdrop(&app) - LOG_BACKDROP).abs() < 1e-4);
        let text = app
            .world_mut()
            .query::<(&Field, &Text)>()
            .iter(app.world())
            .find(|(field, _)| matches!(field, Field::Log))
            .map(|(_, text)| text.0.clone())
            .unwrap();
        assert_eq!(text, "You move north.\nYou move north.");
    }

    #[test]
    fn actions_fold_into_a_menu_that_closes_after_a_choice() {
        let (mut app, _receiver) = dock(Action::Menu);
        let list = app.world_mut().spawn((MenuList, Visibility::Hidden)).id();
        let open = |app: &App| *app.world().get::<Visibility>(list).unwrap() != Visibility::Hidden;
        app.update();
        assert!(app.world().resource::<Menu>().open);
        assert!(open(&app));
        let inventory = app
            .world_mut()
            .spawn((
                Action::Inventory,
                Interaction::Pressed,
                BackgroundColor(chrome::BUTTON),
            ))
            .id();
        app.update();
        assert!(!open(&app), "choosing an action folds the menu");
        assert!(app.world().resource::<interact::Panel>().open);
        // Wait stays available for repeated presses.
        app.world_mut().resource_mut::<Menu>().open = true;
        *app.world_mut().get_mut::<Interaction>(inventory).unwrap() = Interaction::None;
        app.world_mut().spawn((
            Action::Wait,
            Interaction::Pressed,
            BackgroundColor(chrome::BUTTON),
        ));
        app.update();
        assert!(open(&app));
        // A touch stand-in for a key leaves the menu as it is.
        app.insert_resource(TouchControls::default());
        app.world_mut().spawn((
            Action::Press(KeyCode::KeyQ),
            Interaction::Pressed,
            BackgroundColor(chrome::BUTTON),
        ));
        app.update();
        assert!(open(&app));
        assert_eq!(
            app.world().resource::<TouchControls>().pending,
            [KeyCode::KeyQ]
        );
    }

    #[test]
    fn standalone_leave_folds_into_the_menu_while_touch_keys_stay_one_tap_away() {
        let mut app = App::new();
        app.init_resource::<Assets<Image>>()
            .init_resource::<Game>()
            .init_resource::<Time>()
            .insert_resource(crate::session::Session::new(
                std::path::PathBuf::from("missing-scenarios"),
                std::path::PathBuf::from("missing-saves"),
            ))
            .insert_resource(TouchControls::default())
            .add_systems(Startup, setup);
        app.update();
        let list = app
            .world_mut()
            .query_filtered::<Entity, With<MenuList>>()
            .single(app.world())
            .unwrap();
        let actions = app
            .world_mut()
            .query::<(&Action, &ChildOf)>()
            .iter(app.world())
            .map(|(action, parent)| (*action, parent.parent()))
            .collect::<Vec<_>>();
        let leave = actions
            .iter()
            .find(|(action, _)| *action == Action::Leave)
            .expect("standalone play offers leaving");
        assert_eq!(leave.1, list, "leaving waits inside the folded menu");
        let touch = actions
            .iter()
            .filter(|(action, _)| matches!(action, Action::Press(_) | Action::Zoom(_)))
            .collect::<Vec<_>>();
        assert_eq!(touch.len(), TOUCH.len());
        let row = touch[0].1;
        assert_ne!(row, list, "touch keys are not folded away");
        assert!(touch.iter().all(|(_, parent)| *parent == row));
        assert!(
            app.world().get::<chrome::BlockWorld>(row).is_some(),
            "gaps between touch keys must not click into the world"
        );
    }

    #[test]
    fn the_hud_spawns_one_menu_button_and_every_action_inside_the_folded_list() {
        let (app, _) = log_app();
        let mut app = app;
        let mut actions = app
            .world_mut()
            .query::<(&Action, &ChildOf)>()
            .iter(app.world())
            .map(|(action, parent)| (*action, parent.parent()))
            .collect::<Vec<_>>();
        actions.sort_by_key(|(action, _)| *action != Action::Menu);
        let list = app
            .world_mut()
            .query_filtered::<Entity, With<MenuList>>()
            .single(app.world())
            .unwrap();
        assert_eq!(
            *app.world().get::<Visibility>(list).unwrap(),
            Visibility::Hidden
        );
        assert_eq!(actions.len(), ACTIONS.len() + 1);
        assert_eq!(actions[0].0, Action::Menu);
        assert_ne!(actions[0].1, list);
        assert!(app
            .world()
            .get::<chrome::BlockWorld>(actions[0].1)
            .is_none());
        assert!(actions[1..].iter().all(|(_, parent)| *parent == list));
    }

    fn dock(action: Action) -> (App, tokio::sync::mpsc::Receiver<crate::net::Input>) {
        let (net, receiver) = crate::net::test_channel();
        let mut view = crate::test_support::view(7);
        view.has_control = true;
        view.can_act = true;
        let mut app = App::new();
        app.insert_resource(net)
            .insert_resource(Game {
                view: Some(view),
                ..default()
            })
            .init_resource::<interact::Panel>()
            .init_resource::<Menu>()
            .add_systems(Update, buttons);
        app.world_mut().spawn((
            action,
            Interaction::Pressed,
            BackgroundColor(chrome::BUTTON),
        ));
        (app, receiver)
    }

    #[test]
    fn screenshot_dock_reserves_pixels_without_submitting_or_erasing_a_report() {
        for control in [true, false] {
            let (mut app, mut receiver) = dock(Action::Screenshot);
            {
                let mut game = app.world_mut().resource_mut::<Game>();
                game.view.as_mut().unwrap().has_control = control;
                game.screen = Some(screen::Screen::Note {
                    snapshot: game.view.as_ref().unwrap().shown.snapshot.clone(),
                    private: true,
                    report: true,
                });
                game.typed = "keep the report draft".into();
            }
            app.update();
            assert!(app.world().resource::<Game>().capture.is_some());
            assert_eq!(app.world().resource::<Game>().capture_serial, 1);
            assert!(receiver.try_recv().is_err(), "pixels have not arrived yet");
            let button = app
                .world_mut()
                .query_filtered::<Entity, With<Action>>()
                .single(app.world())
                .unwrap();
            *app.world_mut().get_mut::<Interaction>(button).unwrap() = Interaction::Pressed;
            app.update();
            let game = app.world().resource::<Game>();
            assert_eq!(
                game.capture_serial, 1,
                "repeated button presses must not allocate another screenshot"
            );
            assert_eq!(game.typed, "keep the report draft");
            assert!(matches!(
                game.screen,
                Some(screen::Screen::Note { report: true, .. })
            ));
            assert!(receiver.try_recv().is_err());
        }
    }

    #[test]
    fn dock_equipment_opens_locally_and_switches_back_to_inventory() {
        let (mut app, mut receiver) = dock(Action::Equipment);
        app.update();
        assert!(app.world().resource::<interact::Panel>().open);
        assert_eq!(
            app.world().resource::<interact::Panel>().page,
            crate::equipment::Page::Equipment
        );
        assert!(receiver.try_recv().is_err());
        let button = app
            .world_mut()
            .query_filtered::<Entity, With<Action>>()
            .single(app.world())
            .unwrap();
        *app.world_mut().get_mut::<Action>(button).unwrap() = Action::Inventory;
        *app.world_mut().get_mut::<Interaction>(button).unwrap() = Interaction::Pressed;
        app.update();
        assert!(app.world().resource::<interact::Panel>().open);
        assert_eq!(
            app.world().resource::<interact::Panel>().page,
            crate::equipment::Page::Inventory
        );
        assert!(receiver.try_recv().is_err());
    }

    #[test]
    fn dock_map_toggles_once_per_press_without_a_server_action() {
        let (mut app, mut receiver) = dock(Action::Map);
        app.update();
        assert_eq!(
            app.world().resource::<Game>().screen,
            Some(screen::Screen::Places { renaming: None })
        );
        app.update();
        assert_eq!(
            app.world().resource::<Game>().screen,
            Some(screen::Screen::Places { renaming: None }),
            "holding the dock button opens only once"
        );
        assert!(receiver.try_recv().is_err());
    }

    #[test]
    fn dock_inventory_leaves_charts_through_their_cancellation_boundary() {
        let (mut app, mut receiver) = dock(Action::Inventory);
        {
            let mut game = app.world_mut().resource_mut::<Game>();
            game.screen = Some(screen::Screen::Charts);
            game.typed = "abandoned input".into();
        }
        app.update();
        assert!(app.world().resource::<interact::Panel>().open);
        let game = app.world().resource::<Game>();
        assert_eq!(game.screen, None);
        assert!(game.typed.is_empty());
        assert!(matches!(
            receiver.try_recv().unwrap().command,
            Cmd::Chart(tor_client_common::charts::ChartInput::Cancel)
        ));
        assert!(receiver.try_recv().is_err());
    }

    #[test]
    fn dock_character_fetches_overview_and_closes_the_inventory() {
        let (mut app, mut receiver) = dock(Action::Character);
        app.world_mut().resource_mut::<interact::Panel>().open = true;
        app.update();
        assert_eq!(
            app.world().resource::<Game>().screen,
            Some(screen::Screen::Advancement)
        );
        assert!(!app.world().resource::<interact::Panel>().open);
        assert!(
            matches!(receiver.try_recv().unwrap().command, Cmd::Advancement(line) if line == "overview")
        );
        assert!(receiver.try_recv().is_err());
    }

    #[test]
    fn dock_read_only_pages_remain_available_to_observers_and_close_inventory() {
        for (action, screen) in [
            (Action::Places, screen::Screen::Places { renaming: None }),
            (Action::Journal, screen::Screen::History),
        ] {
            let (mut app, mut receiver) = dock(action);
            app.world_mut().resource_mut::<interact::Panel>().open = true;
            app.world_mut()
                .resource_mut::<Game>()
                .view
                .as_mut()
                .unwrap()
                .has_control = false;
            app.update();
            assert_eq!(app.world().resource::<Game>().screen, Some(screen));
            assert!(!app.world().resource::<interact::Panel>().open);
            assert!(receiver.try_recv().is_err());
        }
    }

    #[test]
    fn dock_wait_requires_ready_control_without_a_modal_or_outstanding_work() {
        for blocked in 0..8 {
            let (mut app, mut receiver) = dock(Action::Wait);
            if blocked == 6 {
                app.world_mut().resource_mut::<interact::Panel>().open = true;
            }
            {
                let mut game = app.world_mut().resource_mut::<Game>();
                match blocked {
                    0 => game.view = None,
                    1 => game.view.as_mut().unwrap().has_control = false,
                    2 => game.view.as_mut().unwrap().can_act = false,
                    3 => game.busy = true,
                    4 => game.fatal = true,
                    5 => game.screen = Some(screen::Screen::History),
                    _ => {}
                }
            }
            app.update();
            if blocked == 7 {
                let input = receiver.try_recv().expect("a ready owner can wait");
                assert!(matches!(input.command, Cmd::Wait));
                assert_eq!(input.shown.revision, 7);
                assert!(app.world().resource::<Game>().busy);
            }
            assert!(
                receiver.try_recv().is_err(),
                "blocked state {blocked} must not send a command"
            );
        }
    }

    #[test]
    fn live_fields_and_globe_follow_disclosure_without_growing_assets() {
        let mut app = App::new();
        let mut view = crate::test_support::view(0);
        view.hp = Some((9, 18));
        view.has_control = true;
        view.can_act = true;
        view.status = ["Known place".into(), "Stamina 3/4".into()];
        app.init_resource::<Assets<Image>>()
            .insert_resource(Game {
                view: Some(view),
                ..default()
            })
            .add_systems(Startup, setup)
            .add_systems(Update, update);
        app.update();
        let values: Vec<_> = app
            .world_mut()
            .query::<(&Field, &Text)>()
            .iter(app.world())
            .map(|(field, text)| (format!("{field:?}"), text.0.clone()))
            .collect();
        assert!(values.contains(&("Health".into(), "9 / 18".into())));
        assert!(values.contains(&("Status".into(), "Known place\nStamina 3/4".into())));
        let handle = app.world().resource::<HealthOrb>().image.clone();
        let before = app
            .world()
            .resource::<Assets<Image>>()
            .get(&handle)
            .unwrap()
            .data
            .clone();
        app.world_mut()
            .resource_mut::<Game>()
            .view
            .as_mut()
            .unwrap()
            .hp = None;
        app.update();
        assert_eq!(app.world().resource::<Assets<Image>>().len(), 1);
        let after = &app
            .world()
            .resource::<Assets<Image>>()
            .get(&handle)
            .unwrap()
            .data;
        assert_ne!(&before, after);
        assert_eq!(after.as_ref().unwrap(), &orb_pixels(None));
        assert!(app
            .world_mut()
            .query::<(&Field, &Text)>()
            .iter(app.world())
            .any(|(field, text)| matches!(field, Field::Health) && text.0 == "— / —"));
    }
    #[test]
    fn readiness_respects_control_death_and_outstanding_work() {
        let mut game = Game::default();
        assert_eq!(readiness(&game), "CONNECTING");
        let mut view = crate::test_support::view(0);
        view.can_act = true;
        view.hp = Some((18, 18));
        game.view = Some(view);
        assert_eq!(readiness(&game), "OBSERVING");
        game.view.as_mut().unwrap().has_control = true;
        assert_eq!(readiness(&game), "READY");
        game.busy = true;
        assert_eq!(readiness(&game), "WORKING");
        game.view.as_mut().unwrap().hp = Some((0, 18));
        assert_eq!(readiness(&game), "FALLEN");
        game.fatal = true;
        assert_eq!(readiness(&game), "DISCONNECTED");
        game.fatal = false;
        game.busy = false;
        game.view.as_mut().unwrap().hp = None;
        game.view.as_mut().unwrap().can_act = false;
        assert_eq!(readiness(&game), "WAITING");
    }
    #[test]
    fn health_globe_has_transparent_corners_and_truthful_fill() {
        let empty = orb_pixels(None);
        assert_eq!(empty.len(), 128 * 128 * 4);
        assert_eq!(&empty[..4], &[0, 0, 0, 0]);
        assert_eq!(empty, orb_pixels(Some((0, 0))));
        assert_eq!(empty, orb_pixels(Some((0, 18))));
        let full = orb_pixels(Some((18, 18)));
        assert_eq!(full, orb_pixels(Some((u64::MAX, 18))));
        let half = orb_pixels(Some((9, 18)));
        let top = (32 * 128 + 64) * 4;
        let bottom = (96 * 128 + 64) * 4;
        assert_eq!(&half[top..top + 4], &empty[top..top + 4]);
        assert_eq!(&half[bottom..bottom + 4], &full[bottom..bottom + 4]);
        assert_ne!(&full[top..top + 4], &empty[top..top + 4]);
    }
}
