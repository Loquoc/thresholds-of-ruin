//! A see-through overlay map of the character's level, drawn over the middle
//! of the screen in the manner of an automap: what is seen and remembered,
//! doors and stairs, items, creatures, and the way the camera faces. It turns
//! with the camera, so screen-up is always the camera's heading. `N` shows or
//! hides it; `M` opens the larger north-up map and journal.

use crate::{CameraRig, Game};
use bevy::{
    asset::RenderAssetUsages,
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
    ui::FocusPolicy,
};

/// Cells shown each way from the character.
const REACH: i32 = 16;
/// Texture pixels per cell.
const CELL: i32 = 16;
const SIZE: i32 = REACH * 2 * CELL;
const HALF: f32 = SIZE as f32 * 0.5;
/// The overlay's share of the window's shorter side.
const SCREEN_SHARE: f32 = 92.0;

#[derive(Resource)]
pub(crate) struct MiniMap {
    image: Handle<Image>,
    /// The view and camera heading last drawn.
    drawn: Option<(u64, f32)>,
    /// Whether the player has asked for the overlay.
    pub(crate) shown: bool,
}

impl MiniMap {
    fn new(image: Handle<Image>) -> Self {
        Self {
            image,
            drawn: None,
            shown: false,
        }
    }
}

impl MiniMap {
    /// Redraws for the next game.
    pub(crate) fn forget(&mut self) {
        self.drawn = None;
    }
}

#[derive(Component)]
pub(crate) struct MiniMapFrame;

/// The rim marker that points north as the overlay turns.
#[derive(Component)]
pub(crate) struct NorthMark;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Ground {
    Wall,
    Floor,
    Object,
    Door,
    Up,
    Down,
    Trap,
}

impl Ground {
    pub(crate) fn color(self) -> [f32; 3] {
        match self {
            Self::Floor => [0.62, 0.52, 0.38],
            Self::Object => [0.95, 0.78, 0.3],
            Self::Wall => [0.3, 0.32, 0.4],
            Self::Door => [0.86, 0.6, 0.24],
            Self::Up => [0.36, 0.84, 0.86],
            Self::Down => [0.4, 0.55, 0.95],
            // Orange, as the ASCII map marks a discovered trap.
            Self::Trap => [0.98, 0.5, 0.16],
        }
    }

    /// Overlay opacity: walls and features read clearly, floors stay faint so
    /// the world shows through.
    fn opacity(self) -> f32 {
        match self {
            Self::Floor => 0.2,
            Self::Wall => 0.78,
            _ => 0.9,
        }
    }

    /// The brighter wall tone the overlay uses against the 3D scene.
    fn overlay_color(self) -> [f32; 3] {
        match self {
            Self::Wall => [0.72, 0.68, 0.58],
            other => other.color(),
        }
    }
}

pub(crate) fn terrain_kind(cell: &tor_client_3d_model::Terrain<'_>, dz: i32) -> Option<Ground> {
    Some(if cell.stairs_up || cell.stairs_down {
        match cell.object_model {
            Some(tor_client_3d_model::ItemModel::Stairs) => Ground::Down,
            Some(_) => Ground::Object,
            None if cell.stairs_up => Ground::Up,
            None => Ground::Down,
        }
    } else if cell.door.is_some() {
        Ground::Door
    } else if cell.trap.is_some() && dz == 0 {
        Ground::Trap
    } else if cell.wall && dz == 0 {
        Ground::Wall
    } else if !cell.wall && dz == 0 {
        Ground::Floor
    } else {
        return None;
    })
}

fn item_color(model: tor_client_3d_model::ItemModel) -> [f32; 3] {
    if model == tor_client_3d_model::ItemModel::Stairs {
        [0.4, 0.55, 0.95]
    } else {
        [0.95, 0.78, 0.3]
    }
}

/// Screen-space axes for a camera heading: map cells along `right` run to the
/// screen's right, and along `forward` run up the screen.
fn axes(yaw: f32) -> (Vec2, Vec2) {
    (
        Vec2::new(yaw.cos(), yaw.sin()),
        Vec2::new(yaw.sin(), -yaw.cos()),
    )
}

/// Where a map offset in cells lands on the overlay texture, in pixels.
fn to_pixel(offset: Vec2, yaw: f32) -> Vec2 {
    let (right, forward) = axes(yaw);
    let scaled = offset * CELL as f32;
    Vec2::new(HALF + scaled.dot(right), HALF - scaled.dot(forward))
}

/// The map offset in cells under an overlay texture pixel.
fn to_cells(pixel: Vec2, yaw: f32) -> Vec2 {
    let (right, forward) = axes(yaw);
    let s = pixel - Vec2::splat(HALF);
    (right * s.x - forward * s.y) / CELL as f32
}

pub(crate) fn setup(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    let image = images.add(Image::new_fill(
        Extent3d {
            width: SIZE as u32,
            height: SIZE as u32,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        &[0, 0, 0, 0],
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    ));
    // Not `BlockWorld`: the world stays playable through the overlay.
    commands
        .spawn((
            MiniMapFrame,
            Visibility::Hidden,
            FocusPolicy::Pass,
            GlobalZIndex(-1),
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
        ))
        .with_children(|frame| {
            frame
                .spawn((
                    ImageNode::new(image.clone()),
                    FocusPolicy::Pass,
                    Node {
                        width: Val::VMin(SCREEN_SHARE),
                        height: Val::VMin(SCREEN_SHARE),
                        ..default()
                    },
                ))
                .with_child((
                    NorthMark,
                    Text::new("N"),
                    TextFont {
                        font_size: 16.0,
                        ..default()
                    },
                    TextColor(Color::srgba(0.92, 0.82, 0.6, 0.85)),
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Percent(50.0),
                        top: Val::Percent(3.0),
                        margin: UiRect::new(Val::Px(-5.0), Val::ZERO, Val::Px(-9.0), Val::ZERO),
                        ..default()
                    },
                ));
        });
    commands.insert_resource(MiniMap::new(image));
}

pub(crate) fn toggle(
    keys: Res<ButtonInput<KeyCode>>,
    net: Res<crate::Net>,
    mut game: ResMut<Game>,
    mut map: ResMut<MiniMap>,
    panel: Res<crate::interact::Panel>,
) {
    if panel.keyboard_captured || (panel.open && panel.search_focused) {
        return;
    }
    if keys.just_pressed(KeyCode::KeyN) && game.screen.is_none() {
        map.shown = !map.shown;
    }
    if keys.just_pressed(KeyCode::KeyM)
        && game
            .screen
            .as_ref()
            .is_none_or(|open| *open == crate::screen::Screen::Places { renaming: None })
    {
        crate::screen::toggle(
            &net,
            &mut game,
            crate::screen::Screen::Places { renaming: None },
        );
    }
}

/// The overlay shows only over the open world: screens and the inventory
/// panel hide it without forgetting that the player wants it.
pub(crate) fn show(
    map: Res<MiniMap>,
    game: Res<Game>,
    panel: Res<crate::interact::Panel>,
    mut frames: Query<&mut Visibility, With<MiniMapFrame>>,
) {
    let visible = map.shown && game.view.is_some() && game.screen.is_none() && !panel.open;
    for mut visibility in &mut frames {
        visibility.set_if_neq(if visible {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
    }
}

pub(crate) fn draw(
    game: Res<Game>,
    rig: Res<CameraRig>,
    mut map: ResMut<MiniMap>,
    mut images: ResMut<Assets<Image>>,
    frames: Query<&Visibility, With<MiniMapFrame>>,
    mut north: Query<&mut Node, With<NorthMark>>,
) {
    if !frames.iter().any(|v| *v != Visibility::Hidden) {
        return;
    }
    let Some(view) = &game.view else {
        return;
    };
    let now = (game.view_serial, rig.yaw);
    if map
        .drawn
        .is_some_and(|(serial, yaw)| serial == now.0 && (yaw - now.1).abs() < 0.01)
    {
        return;
    }
    map.drawn = Some(now);
    let yaw = rig.yaw;
    let north_at = to_pixel(Vec2::new(0.0, -(REACH as f32) * 0.86), yaw) / SIZE as f32 * 100.0;
    for mut node in &mut north {
        node.left = Val::Percent(north_at.x);
        node.top = Val::Percent(north_at.y);
    }
    let Some(image) = images.get_mut(&map.image) else {
        return;
    };
    let Some(data) = image.data.as_mut() else {
        return;
    };
    let feet = view.feet;

    // What lies in each column of the character's level; seen beats remembered.
    let side = (REACH * 2 + 1) as usize;
    let mut ground: Vec<Option<(Ground, bool)>> = vec![None; side * side];
    for cell in view.terrain() {
        let (dx, dy, dz) = (
            cell.position.x - feet.x,
            cell.position.y - feet.y,
            cell.position.z - feet.z,
        );
        if dx.abs() > REACH || dy.abs() > REACH || !(-1..=1).contains(&dz) {
            continue;
        }
        let Some(kind) = terrain_kind(&cell, dz) else {
            continue;
        };
        let slot = &mut ground[(dy + REACH) as usize * side + (dx + REACH) as usize];
        let entry = slot.get_or_insert((kind, cell.seen));
        if kind > entry.0 {
            entry.0 = kind;
        }
        entry.1 |= cell.seen;
    }

    let mut canvas = Canvas(data);
    for y in 0..SIZE {
        for x in 0..SIZE {
            let pixel = Vec2::new(x as f32 + 0.5, y as f32 + 0.5);
            let rim = (pixel - Vec2::splat(HALF)).length() / HALF;
            if rim > 1.0 {
                canvas.put(x, y, [0.0; 3], 0.0);
                continue;
            }
            let cells = to_cells(pixel, yaw);
            let (cx, cy) = (cells.x.round() as i32, cells.y.round() as i32);
            let found = (cx.abs() <= REACH && cy.abs() <= REACH)
                .then(|| ground[(cy + REACH) as usize * side + (cx + REACH) as usize])
                .flatten();
            let Some((kind, seen)) = found else {
                canvas.put(x, y, [0.0; 3], 0.0);
                continue;
            };
            // A darker hairline at cell edges keeps the grid legible.
            let edge = (cells.x - cx as f32).abs().max((cells.y - cy as f32).abs()) > 0.42;
            let shade = if edge { 0.7 } else { 1.0 } * if seen { 1.0 } else { 0.65 };
            let fade = if rim > 0.9 { (1.0 - rim) / 0.1 } else { 1.0 };
            let alpha = kind.opacity() * if seen { 1.0 } else { 0.6 } * fade;
            canvas.put(x, y, kind.overlay_color().map(|c| c * shade), alpha);
        }
    }

    // The camera's view, a soft wedge up the screen from the character.
    let reach = CELL as f32 * 7.0;
    for y in 0..SIZE {
        for x in 0..SIZE {
            let d = Vec2::new(x as f32 + 0.5, y as f32 + 0.5) - Vec2::splat(HALF);
            let length = d.length();
            if length < 2.0 || length > reach {
                continue;
            }
            let along = -d.y / length;
            if along > 0.8 {
                let fade = (1.0 - length / reach) * ((along - 0.8) / 0.2).min(1.0);
                canvas.blend(x, y, [1.0, 0.95, 0.8], 0.35 * fade);
            }
        }
    }

    let marker = |dx: i32, dy: i32| to_pixel(Vec2::new(dx as f32, dy as f32), yaw);
    for item in &view.items {
        let (dx, dy) = (item.position.x - feet.x, item.position.y - feet.y);
        if (item.position.z - feet.z).abs() <= 1 {
            canvas.dot(marker(dx, dy), 4, item_color(item.model));
        }
    }
    for actor in view.actors.iter().filter(|a| !a.is_self) {
        let (dx, dy) = (actor.position.x - feet.x, actor.position.y - feet.y);
        if (actor.position.z - feet.z).abs() <= 2 {
            let color = if actor.hostile {
                [0.95, 0.28, 0.22]
            } else {
                [0.85, 0.85, 0.78]
            };
            canvas.dot(marker(dx, dy), 6, color);
        }
    }
    canvas.dot(Vec2::splat(HALF), 6, [0.35, 0.95, 0.85]);
}

/// The overlay's pixels.
struct Canvas<'a>(&'a mut Vec<u8>);

impl Canvas<'_> {
    fn put(&mut self, x: i32, y: i32, color: [f32; 3], alpha: f32) {
        if !(0..SIZE).contains(&x) || !(0..SIZE).contains(&y) {
            return;
        }
        let i = ((y * SIZE + x) * 4) as usize;
        let byte = |c: f32| (c.clamp(0.0, 1.0) * 255.0) as u8;
        self.0[i..i + 4].copy_from_slice(&[
            byte(color[0]),
            byte(color[1]),
            byte(color[2]),
            byte(alpha),
        ]);
    }

    fn blend(&mut self, x: i32, y: i32, color: [f32; 3], amount: f32) {
        let i = ((y * SIZE + x) * 4) as usize;
        for (c, channel) in color.into_iter().enumerate() {
            let old = f32::from(self.0[i + c]) / 255.0;
            self.0[i + c] = ((old + (channel - old) * amount) * 255.0) as u8;
        }
        self.0[i + 3] = self.0[i + 3].max((amount * 255.0) as u8);
    }

    /// A round, dark-ringed marker centred on a texture point.
    fn dot(&mut self, centre: Vec2, radius: i32, color: [f32; 3]) {
        if (centre - Vec2::splat(HALF)).length() > HALF * 0.94 {
            return;
        }
        let (cx, cy) = (centre.x.floor() as i32, centre.y.floor() as i32);
        let r = radius as f32 + 0.5;
        for y in -radius - 1..=radius + 1 {
            for x in -radius - 1..=radius + 1 {
                let d = ((x * x + y * y) as f32).sqrt();
                if d <= r {
                    self.put(cx + x, cy + y, color, 1.0);
                } else if d <= r + 1.0 {
                    self.put(cx + x, cy + y, [0.05, 0.05, 0.07], 1.0);
                }
            }
        }
    }
}

#[cfg(test)]
mod cosmetic_tests {
    use super::*;
    use tor_client_3d_model::{ItemModel, Terrain};
    use tor_protocol::{ItemClass, ObjectPresentation, ObjectPresentationKind, Position};

    #[test]
    fn map_shortcut_opens_the_combined_places_screen_without_a_world_action() {
        let (net, mut receiver) = crate::net::test_channel();
        let mut app = App::new();
        app.insert_resource(net)
            .init_resource::<Game>()
            .init_resource::<crate::interact::Panel>()
            .init_resource::<ButtonInput<KeyCode>>()
            .insert_resource(MiniMap::new(Handle::default()))
            .add_systems(Update, toggle);
        app.world_mut().spawn((MiniMapFrame, Visibility::Inherited));
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyM);
        app.update();
        assert_eq!(
            app.world().resource::<Game>().screen,
            Some(crate::screen::Screen::Places { renaming: None })
        );
        assert!(receiver.try_recv().is_err());
    }

    #[test]
    fn typing_fields_do_not_toggle_the_map_and_the_shortcut_returns_afterwards() {
        let (net, mut receiver) = crate::net::test_channel();
        let mut app = App::new();
        app.insert_resource(net)
            .init_resource::<Game>()
            .init_resource::<crate::interact::Panel>()
            .init_resource::<ButtonInput<KeyCode>>()
            .insert_resource(MiniMap::new(Handle::default()))
            .add_systems(Update, toggle);
        for (screen, search, captured) in [
            (None, true, false),
            (None, false, true),
            (Some(crate::screen::Screen::History), false, false),
            (
                Some(crate::screen::Screen::Places { renaming: Some(0) }),
                false,
                false,
            ),
            (
                Some(crate::screen::Screen::Note {
                    snapshot: tor_protocol::SnapshotId("draft-snapshot".into()),
                    private: true,
                    report: false,
                }),
                false,
                false,
            ),
        ] {
            app.world_mut().resource_mut::<Game>().screen = screen.clone();
            {
                let mut panel = app.world_mut().resource_mut::<crate::interact::Panel>();
                panel.open = search;
                panel.search_focused = search;
                panel.keyboard_captured = captured;
            }
            app.world_mut()
                .resource_mut::<ButtonInput<KeyCode>>()
                .reset_all();
            app.world_mut()
                .resource_mut::<ButtonInput<KeyCode>>()
                .press(KeyCode::KeyM);
            app.update();
            assert_eq!(
                app.world().resource::<Game>().screen,
                screen,
                "typing fields retain M"
            );
        }
        app.world_mut().resource_mut::<Game>().screen = None;
        app.update();
        assert_eq!(
            app.world().resource::<Game>().screen,
            Some(crate::screen::Screen::Places { renaming: None })
        );
        app.update();
        assert_eq!(app.world().resource::<Game>().screen, None);
        assert!(receiver.try_recv().is_err());
    }

    #[test]
    fn the_overlay_starts_hidden_and_its_key_toggles_it_only_over_the_open_world() {
        let (net, mut receiver) = crate::net::test_channel();
        let mut app = App::new();
        app.insert_resource(net)
            .insert_resource(Game {
                view: Some(crate::test_support::view(7)),
                ..default()
            })
            .init_resource::<crate::interact::Panel>()
            .init_resource::<ButtonInput<KeyCode>>()
            .insert_resource(MiniMap::new(Handle::default()))
            .add_systems(Update, (toggle, show).chain());
        let frame = app
            .world_mut()
            .spawn((MiniMapFrame, Visibility::Hidden))
            .id();
        let visible =
            |app: &App| *app.world().get::<Visibility>(frame).unwrap() != Visibility::Hidden;
        let press = |app: &mut App, key: KeyCode| {
            let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            keys.reset_all();
            keys.press(key);
        };
        app.update();
        assert!(!visible(&app), "the overlay waits to be asked for");
        press(&mut app, KeyCode::KeyN);
        app.update();
        assert!(visible(&app));
        // The inventory and full screens cover it without forgetting the choice.
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .reset_all();
        app.world_mut()
            .resource_mut::<crate::interact::Panel>()
            .open = true;
        app.update();
        assert!(!visible(&app));
        app.world_mut()
            .resource_mut::<crate::interact::Panel>()
            .open = false;
        app.world_mut().resource_mut::<Game>().screen = Some(crate::screen::Screen::History);
        app.update();
        assert!(!visible(&app));
        press(&mut app, KeyCode::KeyN);
        app.update();
        assert!(
            app.world().resource::<MiniMap>().shown,
            "N belongs to the open screen"
        );
        app.world_mut().resource_mut::<Game>().screen = None;
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .reset_all();
        app.update();
        assert!(visible(&app));
        // A typing field keeps N as text.
        app.world_mut()
            .resource_mut::<crate::interact::Panel>()
            .keyboard_captured = true;
        press(&mut app, KeyCode::KeyN);
        app.update();
        assert!(app.world().resource::<MiniMap>().shown);
        app.world_mut()
            .resource_mut::<crate::interact::Panel>()
            .keyboard_captured = false;
        press(&mut app, KeyCode::KeyN);
        app.update();
        assert!(!visible(&app));
        assert!(
            receiver.try_recv().is_err(),
            "the overlay is local presentation"
        );
    }

    #[test]
    fn the_overlay_turns_so_the_camera_heading_points_up_the_screen() {
        use std::f32::consts::FRAC_PI_2;
        let up = Vec2::new(HALF, HALF - CELL as f32);
        // Facing north, north (-y) is up and east (+x) is right.
        assert!(to_pixel(Vec2::new(0.0, -1.0), 0.0).distance(up) < 1e-3);
        assert!(
            to_pixel(Vec2::new(1.0, 0.0), 0.0).distance(Vec2::new(HALF + CELL as f32, HALF)) < 1e-3
        );
        // Facing east, east is up and north is to the left.
        assert!(to_pixel(Vec2::new(1.0, 0.0), FRAC_PI_2).distance(up) < 1e-3);
        assert!(to_pixel(Vec2::new(0.0, -1.0), FRAC_PI_2).x < HALF - CELL as f32 + 1e-3);
        for yaw in [0.0, 0.4, FRAC_PI_2, 2.5, -1.2] {
            let cells = Vec2::new(3.0, -7.0);
            assert!(
                to_cells(to_pixel(cells, yaw), yaw).distance(cells) < 1e-3,
                "yaw {yaw}"
            );
        }
    }

    #[test]
    fn a_discovered_trap_is_marked_on_its_level_only() {
        let trap = Terrain {
            object_presentation: None,
            object_model: None,
            key: "trap",
            position: Position { x: 1, y: 0, z: 0 },
            wall: false,
            stairs_up: false,
            stairs_down: false,
            door: None,
            trap: Some(None),
            seen: true,
            lit: true,
        };
        assert_eq!(terrain_kind(&trap, 0), Some(Ground::Trap));
        assert_eq!(terrain_kind(&trap, 1), None);
    }

    #[test]
    fn sampled_stair_colors_leave_factual_geometry_and_navigation_unchanged() {
        let mut cell = Terrain {
            object_presentation: None,
            object_model: None,
            key: "stairs",
            position: Position { x: 1, y: 0, z: 0 },
            wall: false,
            stairs_up: true,
            stairs_down: false,
            door: None,
            trap: None,
            seen: true,
            lit: true,
        };
        assert_eq!(terrain_kind(&cell, 0), Some(Ground::Up));
        let potion = ObjectPresentation {
            name: "flask".into(),
            asset: None,
            kind: ObjectPresentationKind::Item {
                class: ItemClass::Potion,
            },
        };
        cell.object_presentation = Some(&potion);
        cell.object_model = Some(ItemModel::Potion);
        assert_eq!(terrain_kind(&cell, 0), Some(Ground::Object));
        assert!(cell.stairs_up && !cell.stairs_down);
        let stairs = ObjectPresentation {
            name: "stairs".into(),
            asset: None,
            kind: ObjectPresentationKind::Stairs,
        };
        cell.object_presentation = Some(&stairs);
        cell.object_model = Some(ItemModel::Stairs);
        assert_eq!(terrain_kind(&cell, 0), Some(Ground::Down));
        assert_eq!(item_color(ItemModel::Stairs), [0.4, 0.55, 0.95]);
        assert_eq!(item_color(ItemModel::Potion), [0.95, 0.78, 0.3]);
        cell.object_model = None;
        cell.object_presentation = None;
        assert_eq!(terrain_kind(&cell, 0), Some(Ground::Up));
        // Even a malformed cosmetic-only cell cannot manufacture stair topology.
        cell.stairs_up = false;
        cell.object_model = Some(ItemModel::Stairs);
        assert_eq!(terrain_kind(&cell, 0), Some(Ground::Floor));
    }
}
