//! A cell-scoped chooser for disclosed large piles; the server still owns Take.
use bevy::{prelude::*, ui::FocusPolicy, window::PrimaryWindow};
use tor_client_3d_model::{GroundItem, ItemModel, View};
use tor_protocol::{ItemTarget, Position, ReplyContext};

pub(crate) const SMALL_PILE_LIMIT: usize = 4;

/// Never admit remembered, undisclosed or corpse entries into the aggregate.
pub(crate) fn items(view: &View, at: Position) -> Vec<&GroundItem> {
    if !view.visible.iter().any(|cell| cell.position == at) {
        return vec![];
    }
    let mut items: Vec<_> = view
        .items
        .iter()
        .filter(|item| item.position == at && item.model != ItemModel::Corpse)
        .collect();
    items.sort_by_key(|item| item.id);
    items
}
pub(crate) fn stack_count(view: &View, at: Position) -> usize {
    items(view, at).len()
}
pub(crate) fn is_pile(view: &View, at: Position) -> bool {
    stack_count(view, at) > SMALL_PILE_LIMIT
}

#[derive(Resource, Default)]
pub(crate) struct PilePicker {
    cell: Option<Position>,
    shown: Option<ReplyContext>,
    cell_key: Option<String>,
    /// Closing this frame must not pass the same Escape/click through to the world.
    captured: bool,
}
impl PilePicker {
    pub(crate) fn open(&self) -> bool {
        self.cell.is_some()
    }
    pub(crate) fn captures(&self) -> bool {
        self.open() || self.captured
    }
    pub(crate) fn open_at(&mut self, view: &View, at: Position) -> bool {
        if !is_pile(view, at) {
            return false;
        }
        self.cell = Some(at);
        self.cell_key = view
            .visible
            .iter()
            .find(|cell| cell.position == at)
            .map(|cell| cell.key.clone());
        self.shown = Some(view.shown.clone());
        self.captured = true;
        true
    }
    fn close(&mut self) {
        self.cell = None;
        self.shown = None;
        self.cell_key = None;
    }
    fn same_context(&self, view: &View) -> bool {
        self.shown.as_ref().is_some_and(|shown| {
            self.cell.is_some_and(|at| {
                view.visible
                    .iter()
                    .any(|cell| cell.position == at && Some(&cell.key) == self.cell_key.as_ref())
            }) && shown.actor == view.shown.actor
                && shown.branch == view.shown.branch
                && shown.input.stream == view.shown.input.stream
        })
    }
    fn reconcile(&mut self, view: Option<&View>, blocked: bool, focused: bool, escape: bool) {
        self.captured = self.open();
        if blocked
            || !focused
            || escape
            || view.is_none_or(|view| {
                !self.same_context(view) || self.cell.is_none_or(|at| !is_pile(view, at))
            })
        {
            self.close();
        }
    }
    fn choice(&self, view: &View, id: ItemTarget, busy: bool) -> Option<ItemTarget> {
        if busy || !view.has_control || !view.can_act || !self.same_context(view) {
            return None;
        }
        let at = self.cell?;
        items(view, at)
            .into_iter()
            .any(|item| item.id == id)
            .then_some(id)
    }
}
#[derive(Component)]
pub(crate) struct PickerRoot;
#[derive(Component, Clone, Copy)]
pub(crate) enum Choice {
    Item(ItemTarget),
    Close,
}

/// Runs before world keyboard handling. Choices are revalidated against this frame's view.
pub(crate) fn update(
    keys: Res<ButtonInput<KeyCode>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    game: Res<crate::Game>,
    panel: Res<crate::interact::Panel>,
    mut picker: ResMut<PilePicker>,
    mut pursuit: ResMut<crate::interact::Pursuit>,
    mut buttons: Query<(&Interaction, &Choice, &mut BackgroundColor), Changed<Interaction>>,
) {
    picker.reconcile(
        game.view.as_ref(),
        game.fatal || game.screen.is_some() || panel.open || game.aiming.is_some(),
        windows.single().is_ok_and(|w| w.focused),
        keys.just_pressed(KeyCode::Escape),
    );
    if !picker.open() {
        return;
    }
    for (interaction, choice, mut background) in &mut buttons {
        background.0 = if *interaction == Interaction::Hovered {
            crate::chrome::HOT
        } else {
            crate::chrome::BUTTON
        };
        if *interaction != Interaction::Pressed {
            continue;
        }
        match choice {
            Choice::Close => picker.close(),
            Choice::Item(id) => {
                if let Some(id) = game
                    .view
                    .as_ref()
                    .and_then(|view| picker.choice(view, *id, game.busy))
                {
                    pursuit.fetch(id);
                    picker.close();
                }
            }
        }
    }
}

#[derive(Clone, PartialEq)]
pub(crate) struct Drawing {
    pub(crate) at: Position,
    pub(crate) rows: Vec<(ItemTarget, String, bool)>,
    pub(crate) enabled: bool,
}
fn drawing(picker: &PilePicker, game: &crate::Game) -> Option<Drawing> {
    let at = picker.cell?;
    let view = game.view.as_ref()?;
    Some(Drawing {
        at,
        rows: items(view, at)
            .into_iter()
            .map(|item| {
                let name: String = item
                    .name
                    .chars()
                    .map(|c| if c.is_control() { ' ' } else { c })
                    .collect();
                (
                    item.id,
                    format!("{name} ×{}", item.quantity),
                    item.reachable,
                )
            })
            .collect(),
        enabled: view.has_control && view.can_act && !game.busy,
    })
}
pub(crate) fn draw(
    mut commands: Commands,
    picker: Res<PilePicker>,
    game: Res<crate::Game>,
    old: Query<Entity, With<PickerRoot>>,
    mut previous: Local<Option<Drawing>>,
) {
    let next = drawing(&picker, &game);
    if *previous == next {
        return;
    }
    for entity in &old {
        commands.entity(entity).despawn();
    }
    *previous = next.clone();
    let Some(drawing) = next else {
        return;
    };
    spawn(&mut commands, &drawing);
}

/// The actual popup renderer, also used by explicitly disclosed offline fixtures.
pub(crate) fn spawn(commands: &mut Commands, drawing: &Drawing) {
    commands
        .spawn((
            PickerRoot,
            crate::chrome::BlockWorld,
            FocusPolicy::Block,
            GlobalZIndex(90),
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.25)),
        ))
        .with_children(|overlay| {
            overlay
                .spawn((
                    Node {
                        width: Val::Percent(50.0),
                        max_height: Val::Percent(64.0),
                        padding: UiRect::all(Val::Px(16.0)),
                        row_gap: Val::Px(10.0),
                        flex_direction: FlexDirection::Column,
                        ..default()
                    },
                    BackgroundColor(crate::chrome::STONE),
                    BorderColor(crate::chrome::GOLD),
                ))
                .with_children(|root| {
                    root.spawn((
                        Text::new(format!("Loot pile — {} stacks", drawing.rows.len())),
                        crate::chrome::text(23.0, crate::chrome::INK),
                    ));
                    root.spawn((
                        Text::new("Choose one item. Scroll for more. Esc closes."),
                        crate::chrome::text(16.0, crate::chrome::MUTED),
                    ));
                    root.spawn((
                        crate::chrome::ScrollArea,
                        ScrollPosition::default(),
                        Node {
                            overflow: Overflow::scroll_y(),
                            min_height: Val::Px(0.0),
                            flex_shrink: 1.0,
                            flex_direction: FlexDirection::Column,
                            row_gap: Val::Px(6.0),
                            ..default()
                        },
                    ))
                    .with_children(|rows| {
                        for (id, label, reachable) in &drawing.rows {
                            let action = if !drawing.enabled {
                                "Unavailable"
                            } else if *reachable {
                                "Take"
                            } else {
                                "Go and take"
                            };
                            let mut row = rows.spawn((
                                Node {
                                    padding: UiRect::all(Val::Px(8.0)),
                                    width: Val::Percent(100.0),
                                    flex_shrink: 0.0,
                                    ..default()
                                },
                                BackgroundColor(crate::chrome::BUTTON),
                            ));
                            if drawing.enabled {
                                row.insert((Button, Choice::Item(*id)));
                            }
                            row.with_child((
                                Node {
                                    max_width: Val::Percent(100.0),
                                    ..default()
                                },
                                Text::new(format!("{label}   ·   {action}")),
                                crate::chrome::text(
                                    18.0,
                                    if drawing.enabled {
                                        crate::chrome::INK
                                    } else {
                                        crate::chrome::MUTED
                                    },
                                ),
                            ));
                        }
                    });
                    root.spawn((
                        Button,
                        Choice::Close,
                        BackgroundColor(crate::chrome::BUTTON),
                        Node {
                            padding: UiRect::all(Val::Px(8.0)),
                            ..default()
                        },
                    ))
                    .with_child((
                        Text::new("Close"),
                        crate::chrome::text(18.0, crate::chrome::INK),
                    ));
                });
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> View {
        let mut view = crate::test_support::view(0);
        view.has_control = true;
        view.can_act = true;
        let at = view.feet;
        view.visible.push(tor_protocol::CellView {
            position: at,
            wall: false,
            lit: true,
            key: "floor".into(),
            material: String::new(),
            asset: None,
            trap: None,
            door: None,
            stairs_up: false,
            stairs_down: false,
            place_hint: false,
            object_presentation: None,
        });
        for i in 0..6 {
            view.items.push(GroundItem {
                surface: tor_client_3d_model::ItemSurface::Neutral,
                id: ItemTarget::from_digest([i; 32]),
                name: format!("unknown {i}"),
                quantity: u64::from(i) + 1,
                position: at,
                reachable: i % 2 == 0,
                model: ItemModel::Potion,
                class: tor_protocol::ItemClass::Potion,
                description: String::new(),
                corpse_seed: None,
                corpse_model: tor_client_3d_model::CreatureModel::Placeholder,
            });
        }
        view
    }
    #[test]
    fn pile_threshold_counts_stacks_not_units_and_excludes_corpses_and_memory() {
        let mut view = fixture();
        let at = view.feet;
        assert_eq!(stack_count(&view, at), 6);
        assert!(is_pile(&view, at));
        view.items[0].quantity = u64::MAX;
        assert_eq!(stack_count(&view, at), 6);
        view.items[0].model = ItemModel::Corpse;
        view.items[1].position.x = 1;
        assert!(!is_pile(&view, at));
        assert_eq!(stack_count(&view, at), 4);
        view.memory = vec![tor_protocol::RememberedMapCell {
            object_presentation: None,
            key: "remembered".into(),
            position: at,
            wall: false,
            material: "stone".into(),
            asset: None,
            place_hint: false,
            stairs_up: false,
            stairs_down: false,
            trap: None,
            door: None,
            ground_items: vec![],
        }];
        view.visible.clear();
        assert_eq!(stack_count(&view, at), 0);
    }
    #[test]
    fn picker_uses_current_ids_cell_and_authority_for_near_and_distant_choices() {
        let mut view = fixture();
        let mut picker = PilePicker::default();
        assert!(picker.open_at(&view, view.feet));
        for item in &view.items {
            assert_eq!(picker.choice(&view, item.id, false), Some(item.id));
        }
        let removed = view.items.remove(0);
        assert_eq!(picker.choice(&view, removed.id, false), None);
        let moved = view.items[0].id;
        view.items[0].position.x = 1;
        assert_eq!(picker.choice(&view, moved, false), None);
        let id = view.items[1].id;
        assert_eq!(picker.choice(&view, id, true), None);
        view.has_control = false;
        assert_eq!(picker.choice(&view, id, false), None);
        view.has_control = true;
        view.can_act = false;
        assert_eq!(picker.choice(&view, id, false), None);
        view.can_act = true;
        view.shown.input.stream.epoch += 1;
        assert_eq!(picker.choice(&view, id, false), None);
    }
    #[test]
    fn picker_closes_on_small_transition_disclosure_loss_focus_modal_escape_and_context() {
        for reason in 0..7 {
            let mut view = fixture();
            let mut picker = PilePicker::default();
            assert!(picker.open_at(&view, view.feet));
            if reason == 0 {
                view.items.truncate(4);
            }
            if reason == 1 {
                view.visible.clear();
            }
            if reason == 5 {
                view.shown.input.stream.epoch += 1;
            }
            if reason == 6 {
                view.items.clear();
            }
            picker.reconcile(Some(&view), reason == 2, reason != 3, reason == 4);
            assert!(!picker.open());
            assert!(picker.captures(), "closing input must not pass through");
            picker.reconcile(Some(&view), false, true, false);
            assert!(!picker.captures());
        }
    }
    #[test]
    fn refresh_keeps_live_picker_but_never_reuses_removed_rows() {
        let mut view = fixture();
        let mut picker = PilePicker::default();
        assert!(picker.open_at(&view, view.feet));
        view.revision += 1;
        view.shown.revision += 1;
        view.items.remove(0);
        picker.reconcile(Some(&view), false, true, false);
        assert!(picker.open());
        assert_eq!(items(&view, view.feet).len(), 5);
    }
    #[test]
    fn actual_ui_choice_uses_pursuit_and_closing_captures_keyboard_without_net() {
        let view = fixture();
        let chosen = view.items[1].id; // Distant item deliberately shares the same UI path.
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<crate::Game>()
            .init_resource::<crate::interact::Panel>()
            .init_resource::<crate::interact::Pursuit>()
            .init_resource::<PilePicker>()
            .add_systems(Update, (update, draw).chain());
        app.world_mut().spawn((
            PrimaryWindow,
            Window {
                focused: true,
                ..default()
            },
        ));
        assert!(app
            .world_mut()
            .resource_mut::<PilePicker>()
            .open_at(&view, view.feet));
        app.world_mut().resource_mut::<crate::Game>().view = Some(view);
        app.update();
        let roots = app
            .world_mut()
            .query_filtered::<Entity, (With<PickerRoot>, With<crate::chrome::BlockWorld>)>()
            .iter(app.world())
            .count();
        assert_eq!(roots, 1, "popup consumes pointer input");
        let button = app
            .world_mut()
            .query::<(Entity, &Choice)>()
            .iter(app.world())
            .find_map(|(e, choice)| {
                matches!(choice, Choice::Item(id) if *id == chosen).then_some(e)
            })
            .unwrap();
        app.world_mut()
            .entity_mut(button)
            .insert(Interaction::Pressed);
        app.update();
        assert!(app.world().resource::<crate::interact::Pursuit>().active());
        assert!(!app.world().resource::<PilePicker>().open());
        assert!(app.world().resource::<PilePicker>().captures());
        assert_eq!(
            app.world_mut()
                .query_filtered::<Entity, With<PickerRoot>>()
                .iter(app.world())
                .count(),
            0
        );
        // There is intentionally no Net: choosing schedules the existing pursuit,
        // rather than bypassing current view/context checks or sending a blind Take.
    }

    #[test]
    fn disabled_authority_renders_no_item_buttons_and_escape_closes() {
        let mut view = fixture();
        view.has_control = false;
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<crate::Game>()
            .init_resource::<crate::interact::Panel>()
            .init_resource::<crate::interact::Pursuit>()
            .init_resource::<PilePicker>()
            .add_systems(Update, (update, draw).chain());
        app.world_mut().spawn((
            PrimaryWindow,
            Window {
                focused: true,
                ..default()
            },
        ));
        assert!(app
            .world_mut()
            .resource_mut::<PilePicker>()
            .open_at(&view, view.feet));
        app.world_mut().resource_mut::<crate::Game>().view = Some(view);
        app.update();
        assert_eq!(
            app.world_mut()
                .query::<&Choice>()
                .iter(app.world())
                .filter(|c| matches!(c, Choice::Item(_)))
                .count(),
            0
        );
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Escape);
        app.update();
        assert!(!app.world().resource::<PilePicker>().open());
        assert!(app.world().resource::<PilePicker>().captures());
    }
    #[test]
    fn observer_origin_shift_does_not_retarget_an_open_picker_to_another_pile() {
        let mut view = fixture();
        let mut picker = PilePicker::default();
        assert!(picker.open_at(&view, view.feet));
        picker.reconcile(Some(&view), false, true, false);
        assert!(
            picker.open(),
            "positive fixture remains the same disclosed pile"
        );
        view.visible[0].key = "another-physical-cell-after-teleport".into();
        assert_eq!(stack_count(&view, view.feet), 6);
        assert_eq!(
            picker.choice(&view, view.items[0].id, false),
            None,
            "stale button must reject new physical cell even before reconciliation"
        );
        picker.reconcile(Some(&view), false, true, false);
        assert!(
            !picker.open(),
            "same relative coordinate is not the same physical pile"
        );
    }
}
