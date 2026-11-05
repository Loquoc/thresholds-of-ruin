//! Optional, non-interactive labels for currently disclosed ground loot.
//! Names and exact quantities come only from the observation, never the model.
use bevy::{prelude::*, ui::FocusPolicy, window::PrimaryWindow};
use std::collections::BTreeMap;
use tor_client_3d_model::View;
use tor_protocol::Position;

#[derive(Resource, Default)]
pub(crate) struct LootLabels {
    latched: bool,
}

impl LootLabels {
    fn update(&mut self, toggle: bool, held: bool, blocked: bool, focused: bool) -> bool {
        if focused && !blocked && toggle {
            self.latched = !self.latched;
        }
        focused && !blocked && (self.latched || held)
    }
}

#[derive(Component)]
pub(crate) struct GroundLabel;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LabelGroup {
    pub(crate) position: Position,
    pub(crate) text: String,
    pub(crate) stacks: usize,
}

fn item_line(name: &str, quantity: u64) -> String {
    #[cfg(test)]
    work::add(|counts| counts.names += 1);
    let cleaned: String = name
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    let mut short: String = cleaned.chars().take(32).collect();
    if cleaned.chars().count() > 32 {
        short.push('…');
    }
    format!("{short} ×{quantity}")
}

/// Includes neither memory-only cells nor inventory. A stale item whose cell is
/// no longer disclosed cannot leave a label behind.
pub(crate) fn groups(view: &View) -> Vec<LabelGroup> {
    disclosed_groups(
        view.items
            .iter()
            .filter(|item| {
                view.visible
                    .iter()
                    .any(|cell| cell.position == item.position)
            })
            .map(|item| (item.position, item.name.as_str(), item.quantity)),
    )
}

/// Always-visible summary only for the aggregate graphic (more than four
/// noncorpse stacks). Counts are stacks, never inferred item units.
pub(crate) fn pile_counts(groups: Vec<LabelGroup>) -> Vec<LabelGroup> {
    groups
        .into_iter()
        .filter(|group| group.stacks > 4)
        .map(|mut group| {
            group.text = format!("{} stacks", group.stacks);
            group
        })
        .collect()
}

fn summaries(view: &View, names: bool) -> Vec<LabelGroup> {
    if names {
        return groups(view);
    }
    pile_counts(disclosed_groups(
        view.items
            .iter()
            .filter(|item| {
                item.model != tor_client_3d_model::ItemModel::Corpse
                    && view
                        .visible
                        .iter()
                        .any(|cell| cell.position == item.position)
            })
            .map(|item| (item.position, item.name.as_str(), item.quantity)),
    ))
}

/// Shared presentation for already-disclosed live observations and explicitly
/// named review fixtures. The caller owns disclosure; no model is consulted.
pub(crate) fn disclosed_groups<'a>(
    items: impl IntoIterator<Item = (Position, &'a str, u64)>,
) -> Vec<LabelGroup> {
    let mut cells: BTreeMap<(i32, i32, i32), Vec<String>> = BTreeMap::new();
    for (p, name, quantity) in items {
        cells
            .entry((p.x, p.y, p.z))
            .or_default()
            .push(item_line(name, quantity));
    }
    cells
        .into_iter()
        .map(|((x, y, z), mut lines)| {
            lines.sort();
            let stacks = lines.len();
            lines.truncate(3);
            if stacks > 3 {
                lines.push(format!("+{} more stacks", stacks - 3));
            }
            LabelGroup {
                position: Position { x, y, z },
                text: lines.join("\n"),
                stacks,
            }
        })
        .collect()
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PlacedLabel {
    pub(crate) text: String,
    pub(crate) left: f32,
    pub(crate) top: f32,
    pub(crate) width: f32,
    pub(crate) height: f32,
}

/// Stable placement, bounded to the viewport. Offscreen anchors are not edge
/// indicators. Dense clusters get an explicit overflow count instead of overlap.
pub(crate) fn layout(mut anchors: Vec<(Vec2, LabelGroup)>, viewport: Vec2) -> Vec<PlacedLabel> {
    if viewport.x < 100.0 || viewport.y < 80.0 {
        return vec![];
    }
    anchors.sort_by(|(a, ga), (b, gb)| {
        a.y.total_cmp(&b.y)
            .then_with(|| a.x.total_cmp(&b.x))
            .then_with(|| {
                (ga.position.x, ga.position.y, ga.position.z).cmp(&(
                    gb.position.x,
                    gb.position.y,
                    gb.position.z,
                ))
            })
    });
    let mut placed: Vec<PlacedLabel> = Vec::new();
    let mut overflow = 0;
    for (at, group) in anchors {
        if !at.is_finite() || at.x < 0.0 || at.y < 0.0 || at.x >= viewport.x || at.y >= viewport.y {
            continue;
        }
        let width = (group
            .text
            .lines()
            .map(|line| line.chars().count())
            .max()
            .unwrap_or(0) as f32
            * 8.0
            + 16.0)
            .max(72.0)
            .min(viewport.x - 16.0);
        let height = group.text.lines().count() as f32 * 18.0 + 10.0;
        let left = (at.x - width * 0.5).clamp(8.0, viewport.x - width - 8.0);
        // A shared cell anchor sits between spread-out items. Leave clearance
        // above the whole cluster rather than covering its upper silhouettes.
        let clearance = if group.stacks > 1 { 28.0 } else { 12.0 };
        let desired = at.y - height - clearance;
        let candidate = [0.0, -32.0, 32.0, -64.0, 64.0, -96.0, 96.0]
            .into_iter()
            .find_map(|shift| {
                let top = (desired + shift).clamp(8.0, (viewport.y - height - 44.0).max(8.0));
                let free = placed.iter().all(|p| {
                    left + width + 4.0 <= p.left
                        || p.left + p.width + 4.0 <= left
                        || top + height + 4.0 <= p.top
                        || p.top + p.height + 4.0 <= top
                });
                free.then_some(top)
            });
        if let Some(top) = candidate {
            placed.push(PlacedLabel {
                text: group.text,
                left,
                top,
                width,
                height,
            });
        } else {
            overflow += group.stacks;
        }
    }
    if overflow > 0 {
        placed.push(PlacedLabel {
            text: format!("+{overflow} crowded stacks — zoom in"),
            left: 8.0,
            top: viewport.y - 36.0,
            width: (viewport.x - 16.0).min(330.0),
            height: 28.0,
        });
    }
    placed
}

/// Shared by live exploration and the offscreen review fixture. These nodes
/// deliberately have neither Interaction nor BlockWorld: clicks pass through.
pub(crate) fn spawn(commands: &mut Commands, labels: &[PlacedLabel]) {
    for label in labels {
        commands
            .spawn((
                GroundLabel,
                FocusPolicy::Pass,
                GlobalZIndex(2),
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(label.left),
                    top: Val::Px(label.top),
                    width: Val::Px(label.width),
                    height: Val::Px(label.height),
                    padding: UiRect::axes(Val::Px(6.0), Val::Px(4.0)),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.055, 0.045, 0.035, 0.94)),
                BorderRadius::all(Val::Px(3.0)),
            ))
            .with_child((
                Text::new(label.text.clone()),
                crate::chrome::text(14.0, crate::chrome::INK),
                TextLayout::new_with_linebreak(bevy::text::LineBreak::NoWrap),
                FocusPolicy::Pass,
            ));
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn show(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    mut mode: ResMut<LootLabels>,
    game: Res<crate::Game>,
    panel: Res<crate::interact::Panel>,
    picker: Option<Res<crate::pile_picker::PilePicker>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    camera: Query<(&Camera, &GlobalTransform), crate::portrait::ExplorationCamera>,
    models: crate::picking::Models,
    old: Query<Entity, With<GroundLabel>>,
    mut previous: Local<Vec<PlacedLabel>>,
) {
    let window = windows.single().ok();
    let held = keys.pressed(KeyCode::AltLeft) || keys.pressed(KeyCode::AltRight);
    let blocked = game.fatal
        || game.screen.is_some()
        || panel.open
        || picker.as_ref().is_some_and(|p| p.open());
    let focused = window.is_some_and(|w| w.focused);
    let names = mode.update(keys.just_pressed(KeyCode::F3), held, blocked, focused);
    let mut next = Vec::new();
    if !blocked && focused {
        if let (Some(view), Some(window), Ok((camera, eye))) = (&game.view, window, camera.single())
        {
            let anchors = summaries(view, names)
                .into_iter()
                .filter_map(|group| {
                    let world =
                        crate::stand(crate::ivec(group.position) + game.offset) + Vec3::Y * 0.12;
                    let at = camera.world_to_viewport(eye, world).ok()?;
                    // Match hover's terrain omission and foreground mesh occlusion.
                    let ray = camera.viewport_to_world(eye, at).ok()?;
                    let focus = models.select(ray, view, game.offset, false)?;
                    (focus.position(view) == Some(group.position)).then_some((at, group))
                })
                .collect();
            next = layout(anchors, Vec2::new(window.width(), window.height()));
        }
    }
    if *previous != next {
        for entity in &old {
            commands.entity(entity).despawn();
        }
        spawn(&mut commands, &next);
        *previous = next;
    }
}

#[cfg(test)]
pub(crate) mod work {
    use std::cell::Cell;

    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub(crate) struct Counts {
        pub(crate) maps: usize,
        pub(crate) terrain: usize,
        pub(crate) meshes: usize,
        pub(crate) names: usize,
    }

    thread_local! {
        static COUNTS: Cell<Counts> = Cell::new(Counts::default());
    }

    pub(crate) fn add(f: impl FnOnce(&mut Counts)) {
        COUNTS.with(|counts| {
            let mut next = counts.get();
            f(&mut next);
            counts.set(next);
        });
    }

    pub(crate) fn take() -> Counts {
        COUNTS.with(|counts| counts.replace(Counts::default()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Real label system and camera projection, without a GPU or network.
    fn ready_scene() -> (App, Entity, Entity) {
        use bevy::render::{camera::camera_system, camera::ManualTextureViews, primitives::Aabb};
        use bevy::window::{WindowCreated, WindowResized, WindowScaleFactorChanged};
        use tor_client_3d_model::{CreatureModel, GroundItem, ItemModel, ItemSurface};
        use tor_protocol::{CellView, ItemClass, ItemTarget};
        let mut app = App::new();
        app.init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<LootLabels>()
            .init_resource::<crate::Game>()
            .init_resource::<crate::interact::Panel>()
            .init_resource::<Assets<Image>>()
            .init_resource::<ManualTextureViews>()
            .add_event::<WindowCreated>()
            .add_event::<WindowResized>()
            .add_event::<WindowScaleFactorChanged>()
            .add_event::<AssetEvent<Image>>()
            .add_systems(Update, (camera_system, show).chain());
        app.edit_schedule(Update, |schedule| {
            schedule.set_executor_kind(bevy::ecs::schedule::ExecutorKind::SingleThreaded);
        });
        let window = app
            .world_mut()
            .spawn((
                PrimaryWindow,
                Window {
                    focused: true,
                    resolution: (800.0_f32, 600.0_f32).into(),
                    ..default()
                },
            ))
            .id();
        let camera = app
            .world_mut()
            .spawn((
                Camera3d::default(),
                Camera::default(),
                Projection::Perspective(PerspectiveProjection::default()),
                GlobalTransform::from(
                    Transform::from_xyz(0.0, 10.0, 10.0).looking_at(Vec3::ZERO, Vec3::Y),
                ),
            ))
            .id();
        let mut view = crate::test_support::view(1);
        for (index, x) in [-2, 0, 2].into_iter().enumerate() {
            let position = Position { x, y: 0, z: 0 };
            for z in [-1, 0] {
                view.visible.push(CellView {
                    position: Position { z, ..position },
                    object_presentation: None,
                    wall: z == -1,
                    lit: true,
                    key: format!("{x}:{z}"),
                    material: String::new(),
                    asset: None,
                    trap: None,
                    door: None,
                    stairs_up: false,
                    stairs_down: false,
                    place_hint: false,
                });
            }
            for n in 0..5 {
                view.items.push(GroundItem {
                    surface: ItemSurface::Neutral,
                    corpse_seed: None,
                    model: ItemModel::Ring,
                    corpse_model: CreatureModel::Placeholder,
                    id: ItemTarget::from_digest([(index * 5 + n) as u8; 32]),
                    name: format!("ring {n}"),
                    class: ItemClass::Ring,
                    quantity: 999,
                    description: String::new(),
                    position,
                    reachable: true,
                });
            }
        }
        for x in 20..24 {
            app.world_mut().spawn((
                crate::picking::Pick(tor_client_3d_model::Focus::Cell(view.visible[1].position)),
                Mesh3d(Handle::default()),
                Aabb {
                    center: Vec3::ZERO.into(),
                    half_extents: Vec3::splat(0.25).into(),
                },
                GlobalTransform::from_xyz(x as f32, 0.0, 0.0),
                InheritedVisibility::VISIBLE,
            ));
        }
        app.world_mut().resource_mut::<crate::Game>().view = Some(view);
        (app, camera, window)
    }

    fn label_entities(app: &mut App) -> Vec<Entity> {
        let world = app.world_mut();
        let mut result: Vec<_> = world
            .query_filtered::<Entity, With<GroundLabel>>()
            .iter(world)
            .collect();
        result.sort();
        result
    }

    #[test]
    fn multi_pile_update_prepares_scene_once_and_retains_unchanged_nodes() {
        let (mut app, _, _) = ready_scene();
        work::take();
        app.update();
        let counts = work::take();
        let labels = label_entities(&mut app);
        assert_eq!(
            labels.len(),
            3,
            "fixture must project three selectable piles"
        );
        assert_eq!(
            counts.maps, 1,
            "one terrain preparation per update: {counts:?}"
        );
        assert_eq!(counts.terrain, 6, "each known terrain entry prepared once");
        assert_eq!(counts.meshes, 4, "each shaped mesh prepared once");
        assert_eq!(
            counts.names, 0,
            "count mode must not format discarded names"
        );
        app.update();
        assert_eq!(
            label_entities(&mut app),
            labels,
            "equal placement keeps UI entities"
        );
    }

    #[test]
    fn count_only_summary_never_formats_item_names() {
        let (app, _, _) = ready_scene();
        let view = app.world().resource::<crate::Game>().view.as_ref().unwrap();
        work::take();
        let counts = summaries(view, false);
        assert_eq!(counts.len(), 3);
        assert!(counts
            .iter()
            .all(|group| group.text == "5 stacks" && group.stacks == 5));
        assert_eq!(
            work::take().names,
            0,
            "count-only path discards all item names"
        );
    }

    fn group(x: i32, stacks: usize) -> LabelGroup {
        LabelGroup {
            position: Position { x, y: 0, z: 0 },
            text: "gold pieces ×123".into(),
            stacks,
        }
    }

    #[test]
    fn hold_releases_even_when_a_modal_or_focus_intervenes() {
        let mut mode = LootLabels::default();
        assert!(mode.update(false, true, false, true));
        assert!(!mode.update(false, true, true, true));
        assert!(!mode.update(false, false, false, true));
        assert!(!mode.update(false, true, false, false));
        assert!(!mode.update(false, false, false, true));
    }

    #[test]
    fn toggle_is_an_edge_and_modal_input_cannot_change_preference() {
        let mut mode = LootLabels::default();
        assert!(mode.update(true, false, false, true));
        assert!(mode.update(false, false, false, true));
        assert!(!mode.update(true, false, true, true));
        assert!(mode.update(false, false, false, true));
        assert!(!mode.update(true, false, false, true));
        assert!(!mode.update(true, false, false, false));
        assert!(!mode.update(false, false, false, true));
    }

    #[test]
    fn actual_f3_binding_is_local_and_ignores_pack_input() {
        let mut app = App::new();
        app.init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<LootLabels>()
            .init_resource::<crate::Game>()
            .init_resource::<crate::interact::Panel>()
            .add_systems(Update, show);
        app.world_mut().spawn((
            PrimaryWindow,
            Window {
                focused: true,
                ..default()
            },
        ));
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::F3);
        app.update();
        assert!(app.world().resource::<LootLabels>().latched);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .reset_all();
        app.world_mut()
            .resource_mut::<crate::interact::Panel>()
            .open = true;
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::F3);
        app.update();
        assert!(app.world().resource::<LootLabels>().latched);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .reset_all();
        app.world_mut()
            .resource_mut::<crate::interact::Panel>()
            .open = false;
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::F3);
        app.update();
        assert!(!app.world().resource::<LootLabels>().latched);
        // No Net resource exists: these controls cannot submit a gameplay command.
    }

    #[test]
    fn quantity_is_exact_even_when_name_is_bounded() {
        assert_eq!(item_line("milky potion", 7), "milky potion ×7");
        let line = item_line(&"a".repeat(100), u64::MAX);
        assert_eq!(line, format!("{}… ×18446744073709551615", "a".repeat(32)));
        assert_eq!(item_line("unknown\nring", 1), "unknown ring ×1");
    }

    #[test]
    fn group_labels_clear_the_upper_items_without_lifting_single_labels() {
        let labels = layout(
            vec![
                (Vec2::new(200.0, 240.0), group(0, 1)),
                (Vec2::new(500.0, 240.0), group(1, 4)),
            ],
            Vec2::new(800.0, 600.0),
        );
        assert_eq!(labels.len(), 2);
        assert_eq!(labels[0].top + labels[0].height, 228.0);
        assert_eq!(labels[1].top + labels[1].height, 212.0);
    }

    #[test]
    fn layout_is_deterministic_non_overlapping_and_reports_crowding() {
        let anchors: Vec<_> = (0..20)
            .map(|x| (Vec2::new(320.0, 240.0), group(x, 2)))
            .collect();
        let placed = layout(anchors.clone(), Vec2::new(640.0, 480.0));
        let mut reversed = anchors;
        reversed.reverse();
        assert_eq!(placed, layout(reversed, Vec2::new(640.0, 480.0)));
        assert!(placed.last().unwrap().text.contains("crowded stacks"));
        for (i, a) in placed.iter().enumerate() {
            assert!(
                a.left >= 0.0
                    && a.top >= 0.0
                    && a.left + a.width <= 640.0
                    && a.top + a.height <= 480.0
            );
            for b in &placed[i + 1..] {
                assert!(
                    a.left + a.width <= b.left
                        || b.left + b.width <= a.left
                        || a.top + a.height <= b.top
                        || b.top + b.height <= a.top
                );
            }
        }
    }

    #[test]
    fn groups_only_current_disclosure_without_model_identity_and_with_overflow() {
        use tor_client_3d_model::{CreatureModel, GroundItem, ItemModel};
        use tor_protocol::{CellView, ItemClass, ItemTarget};
        let mut view = crate::test_support::view(1);
        let position = Position { x: 0, y: 0, z: 0 };
        view.visible.push(CellView {
            position,
            object_presentation: None,
            lit: false,
            door: None,
            trap: None,
            material: String::new(),
            key: "cell".into(),
            stairs_up: false,
            stairs_down: false,
            wall: false,
            place_hint: false,
            asset: None,
        });
        for n in 0..5 {
            view.items.push(GroundItem {
                surface: tor_client_3d_model::ItemSurface::Neutral,
                corpse_seed: None,
                model: ItemModel::Ring,
                corpse_model: CreatureModel::Placeholder,
                id: ItemTarget::from_digest([n; 32]),
                name: "unidentified copper ring".into(),
                class: ItemClass::Ring,
                quantity: 2,
                description: "Not a source of label identity".into(),
                position,
                reachable: true,
            });
        }
        assert_eq!(summaries(&view, false)[0].text, "5 stacks");
        assert_eq!(summaries(&view, true), groups(&view));
        view.items[0].model = ItemModel::Corpse;
        assert!(summaries(&view, false).is_empty());
        view.items[0].model = ItemModel::Ring;
        let labels = groups(&view);
        assert_eq!(labels.len(), 1);
        assert_eq!(labels[0].stacks, 5);
        assert_eq!(labels[0].text, "unidentified copper ring ×2\nunidentified copper ring ×2\nunidentified copper ring ×2\n+2 more stacks");
        // Appearance recipe changes cannot expose an identity, rarity or curse.
        view.items[0].model = ItemModel::Gem;
        assert_eq!(groups(&view), labels);
        // Current dark-but-disclosed items are allowed; remembered-only items are not.
        view.visible.clear();
        assert!(groups(&view).is_empty());
        assert!(summaries(&view, false).is_empty());
        view.items.clear();
        assert!(groups(&view).is_empty());
    }

    #[test]
    fn aggregate_counts_are_stacks_and_name_mode_replaces_them() {
        let position = Position { x: 0, y: 0, z: 0 };
        let four = disclosed_groups((0..4).map(|_| (position, "coin", 999)));
        assert!(pile_counts(four).is_empty());
        let five = disclosed_groups((0..5).map(|_| (position, "coin", 999)));
        let counts = pile_counts(five.clone());
        assert_eq!(counts.len(), 1);
        assert_eq!(counts[0].text, "5 stacks");
        assert_eq!(
            five[0].text,
            "coin ×999\ncoin ×999\ncoin ×999\n+2 more stacks"
        );
    }

    #[test]
    fn spawned_labels_cannot_capture_world_clicks() {
        let mut app = App::new();
        app.add_systems(Update, |mut commands: Commands| {
            spawn(
                &mut commands,
                &[PlacedLabel {
                    text: "coin ×1".into(),
                    left: 8.0,
                    top: 8.0,
                    width: 120.0,
                    height: 28.0,
                }],
            );
        });
        app.update();
        let world = app.world_mut();
        let mut labels = world.query_filtered::<(
            &FocusPolicy,
            Option<&Interaction>,
            Option<&crate::chrome::BlockWorld>,
        ), With<GroundLabel>>();
        let (policy, interaction, block) = labels.single(world).unwrap();
        assert_eq!(*policy, FocusPolicy::Pass);
        assert!(interaction.is_none() && block.is_none());
    }

    #[test]
    fn out_of_view_anchors_are_not_edge_indicators() {
        assert!(layout(
            vec![
                (Vec2::new(-1.0, 40.0), group(0, 1)),
                (Vec2::splat(f32::NAN), group(1, 1))
            ],
            Vec2::new(640.0, 480.0)
        )
        .is_empty());
        assert!(groups(&crate::test_support::view(1)).is_empty());
    }
}
