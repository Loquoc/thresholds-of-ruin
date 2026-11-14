use tor_client_3d_model::{view, CreatureModel, ItemModel};
use tor_client_common::{test_support::observation, ClientState, Palette};
use tor_protocol::{ItemClass, ItemTarget, ItemView, Snapshot, StreamContext, StreamId};

#[test]
fn character_attributes_follow_disclosure_and_do_not_survive_redaction() {
    let mut observed = observation();
    let mut stats = tor_client_common::test_support::stats();
    stats.attributes.strength = 7;
    stats.attributes.speed = 4;
    observed.combat.as_mut().unwrap().own_stats = Some(stats);
    let projected = view(&from_observation(observed.clone()), &Palette::default());
    let attributes = projected.attributes.unwrap();
    assert_eq!(attributes.strength, 7);
    assert_eq!(attributes.speed, 4);
    observed.combat.as_mut().unwrap().own_stats = None;
    assert!(
        view(&from_observation(observed.clone()), &Palette::default())
            .attributes
            .is_none()
    );
    observed.combat = None;
    assert!(view(&from_observation(observed), &Palette::default())
        .attributes
        .is_none());
}

#[test]
fn upper_body_and_portal_images_do_not_replace_the_players_known_foot_position() {
    use tor_protocol::{AppearanceSeed, CreatureAppearance, Position, WornView};
    let mut observed = observation();
    let seed = AppearanceSeed::from_digest([27; 32]);
    // The own body outside the eye scene still wears what it discloses.
    observed.self_appearance = Some(CreatureAppearance {
        seed,
        asset: None,
        worn: vec![WornView {
            slot: "body_armor".into(),
            occurrence: 0,
            class: ItemClass::Armor,
            appearance: "ring mail".into(),
            asset: None,
        }],
    });
    let mut upper = observed.visible_actors[0].clone();
    upper.id = observed.self_target;
    upper.appearance_seed = Some(seed);
    upper.asset = None;
    upper.position = Position {
        z: observed.position.z + 1,
        ..observed.position
    };
    let mut echo = upper.clone();
    echo.position = Position {
        x: observed.position.x + 5,
        ..observed.position
    };
    observed.visible_actors = vec![upper, echo];
    let feet = observed.position;
    let projected = view(&from_observation(observed), &Palette::default());
    let own = projected
        .actors
        .iter()
        .find(|actor| actor.is_self && actor.position == feet)
        .expect("own location remains known when an upper cell or portal image is visible");
    assert_eq!(own.appearance_seed, Some(seed));
    assert_eq!(
        own.wearing.body,
        Some(ItemModel::RingMail),
        "undrawn palette assets fall back by disclosed class"
    );
    assert_eq!(
        projected.actors[0].position, feet,
        "the primary representation must precede its aliases"
    );
}

pub(super) fn state() -> ClientState {
    let mut observed = observation();
    observed.visible_actors[0].name = "unfamiliar creature".into();
    observed.visible_actors[0].description = "A shape you cannot identify.".into();
    observed.visible_actors[0].asset = None;
    observed.inventory.push(ItemView {
        corpse: None,
        id: ItemTarget::from_digest([11; 32]),
        name: "unfamiliar object".into(),
        description: "Wrapped in plain paper.".into(),
        class: ItemClass::Misc,
        quantity: 2,
        appearance: "wrapped object".into(),
        identified: false,
        identity: "item.unknown".parse().unwrap(),
        knowledge: Default::default(),
        asset: None,
        known_currency: None,
        known_supply: None,
    });
    from_observation(observed)
}

fn from_observation(observed: tor_protocol::Observation) -> ClientState {
    let snapshot: Snapshot = serde_json::from_value(serde_json::json!({
        "readiness": {"revision": "0", "admission": false, "resume": [], "cancel": []},
        "context": StreamContext { stream: StreamId("projection".into()), epoch: 0 },
        "actor": observed.actor, "branch": "play", "cursor": {"sequence": "0", "tick": observed.tick.to_string()},
        "has_control": true, "intentions": [], "history": {"entries": [], "older_before": null},
        "state": {"snapshot":"fixture-snapshot","wizard_game": false, "revision": "0", "observation": observed},
    })).unwrap();
    ClientState::from_snapshot(snapshot).unwrap()
}

#[test]
fn own_body_and_corpses_use_disclosed_appearance_without_parsing_names() {
    use tor_protocol::{
        AppearanceSeed, CreatureAppearance, GroundItemView, PaletteBody, PaletteUpdate,
    };
    let appearance = CreatureAppearance {
        seed: AppearanceSeed::from_digest([23; 32]),
        worn: Vec::new(),
        asset: Some("creature.equine.authored".into()),
    };
    let mut observed = observation();
    observed.visible_actors.clear();
    observed.self_appearance = Some(appearance.clone());
    observed.ground_items.clear();
    observed.ground_items.push(GroundItemView {
        presentation: None,
        item: ItemView {
            corpse: Some(appearance.clone()),
            class: ItemClass::Corpse,
            quantity: 1,
            id: ItemTarget::from_digest([13; 32]),
            name: "misleading human corpse".into(),
            description: String::new(),
            appearance: "remains".into(),
            identified: true,
            identity: "item.corpse.fixture".parse().unwrap(),
            knowledge: Default::default(),
            asset: Some("item.corpse".into()),
            known_currency: None,
            known_supply: None,
        },
        reachable: true,
        supply_reachable: false,
        position: observed.position,
    });
    let mut palette = Palette::default();
    palette.apply(&PaletteUpdate {
        revision: 1,
        body: PaletteBody::Full {
            assets: ["creature.equine.authored".into(), "item.corpse".into()]
                .into_iter()
                .collect(),
        },
    });
    let projected = view(&from_observation(observed), &palette);
    let own = projected.actors.iter().find(|actor| actor.is_self).unwrap();
    assert_eq!(own.model, CreatureModel::Equine);
    assert_eq!(own.appearance_seed, Some(appearance.seed));
    assert_eq!(projected.items[0].corpse_model, own.model);
    assert_eq!(projected.items[0].corpse_seed, own.appearance_seed);
}

#[test]
fn unknown_models_keep_disclosed_targets_text_and_inventory_actions_available() {
    let state = state();
    let source = &state.state().observation;
    let projected = view(&state, &Palette::default());
    assert_eq!(projected.shown, state.reply_context());
    let source_actor = &source.visible_actors[0];
    let actor = projected
        .actors
        .iter()
        .find(|actor| actor.id == source_actor.id)
        .unwrap();
    assert_eq!(actor.model, CreatureModel::Placeholder);
    assert_eq!(actor.position, source_actor.position);
    assert_eq!(actor.name, source_actor.name);
    assert_eq!(actor.description, source_actor.description);
    let source_item = source.inventory.last().unwrap();
    let item = projected
        .inventory
        .iter()
        .find(|item| item.id == source_item.id)
        .unwrap();
    assert_eq!(item.model, ItemModel::Placeholder);
    assert_eq!(item.name, source_item.name);
    assert_eq!(item.description, source_item.description);
    assert_eq!(item.quantity, 2);
    assert!(
        projected.actors.iter().any(|actor| actor.is_self),
        "a missing self model cannot remove the player"
    );
}

#[test]
fn hydrated_terrain_is_dim_and_never_grants_live_occupants_or_door_controls() {
    use tor_protocol::*;
    let mut client = state();
    let query = client.next_map_query().unwrap();
    client.bind_map_query(&query, "map").unwrap();
    let remembered = RememberedMapCell {
        object_presentation: None,
        key: "remembered-only".into(),
        position: Position { x: 12, y: 4, z: 0 },
        wall: false,
        material: "granite".into(),
        asset: None,
        place_hint: false,
        stairs_up: false,
        stairs_down: true,
        trap: None,
        door: Some(MapDoorAppearance {
            name: "old door".into(),
            description: "stale".into(),
            open: false,
            asset: None,
        }),
        ground_items: vec![],
    };
    let page = MapPage {
        transfer: "transfer".into(),
        actor: client.actor(),
        branch: client.branch().clone(),
        base: client.observation_base(),
        window: query.window,
        start: 0,
        page: 0,
        total_cells: 1,
        complete: true,
        ambiguous: false,
        limited: false,
        cells: vec![remembered.clone()],
        next: None,
    };
    client
        .accept_map_page(&client.reply_context(), "map", &page, 1000)
        .unwrap();
    let projected = view(&client, &Palette::default());
    assert_eq!(projected.memory, vec![remembered]);
    let terrain = projected
        .terrain()
        .find(|c| c.key == "remembered-only")
        .unwrap();
    assert!(!terrain.seen);
    assert!(!terrain.lit);
    assert_eq!(terrain.door, Some(false));
    assert!(terrain.stairs_down);
    assert!(projected
        .actors
        .iter()
        .all(|a| a.position != terrain.position));
    assert!(projected
        .items
        .iter()
        .all(|i| i.position != terrain.position));
    assert!(!matches!(
        tor_client_3d_model::describe(&projected, terrain.position).2,
        Some(tor_client_3d_model::Intent::Door(..))
    ));
}

#[test]
fn visible_terrain_uses_explicit_illumination_instead_of_visibility() {
    let mut observed = observation();
    let dark = tor_protocol::CellView {
        object_presentation: None,
        lit: false,
        key: "dark".into(),
        position: observed.position,
        wall: false,
        stairs_up: false,
        stairs_down: false,
        material: "stone".into(),
        place_hint: false,
        asset: None,
        trap: None,
        door: None,
    };
    let bright = tor_protocol::CellView {
        object_presentation: None,
        lit: true,
        key: "bright".into(),
        position: tor_protocol::Position {
            x: observed.position.x + 1,
            ..observed.position
        },
        ..dark.clone()
    };
    observed.visible_cells = vec![dark, bright];
    let projected = view(&from_observation(observed), &Palette::default());
    let cells: Vec<_> = projected.terrain().collect();
    assert!(cells[0].seen && !cells[0].lit);
    assert!(cells[1].seen && cells[1].lit);
}

#[test]
fn portable_light_projection_and_real_commands_follow_disclosed_eligibility() {
    use tor_client_3d_model::{request_from, Cmd};
    use tor_client_common::items::ItemOperation;
    use tor_protocol::{Action, Command, Request};
    for lit in [false, true] {
        let observed = tor_client_common::test_support::portable_light_observation(lit);
        let item = observed.inventory[0].id;
        let mut snapshot = from_observation(observed).reset_snapshot().unwrap();
        snapshot.readiness.admission = true;
        let state = ClientState::from_snapshot(snapshot).unwrap();
        let projected = view(&state, &Palette::default());
        let light = projected.inventory[0].portable_light.as_ref().unwrap();
        assert_eq!(
            (light.lit, light.ignitable, light.extinguishable),
            (lit, !lit, lit)
        );
        assert_eq!(light.remaining, 100_000);
        let operation = if lit {
            ItemOperation::Extinguish
        } else {
            ItemOperation::Ignite
        };
        let request = request_from(&state, &projected.shown, Cmd::Use(item, operation)).unwrap();
        let Request::Command {
            command: Command::Act { action, .. },
            ..
        } = request
        else {
            panic!("ordinary action")
        };
        assert_eq!(
            action,
            if lit {
                Action::Extinguish { item }
            } else {
                Action::Ignite { item }
            }
        );
        let wrong = if lit {
            ItemOperation::Ignite
        } else {
            ItemOperation::Extinguish
        };
        assert!(request_from(&state, &projected.shown, Cmd::Use(item, wrong)).is_err());
    }
}

#[test]
fn false_corpse_and_stair_models_keep_real_handles_and_item_mechanics() {
    use tor_protocol::{
        AppearanceSeed, CreatureAppearance, GroundItemView, ObjectPresentation,
        ObjectPresentationKind, Position,
    };
    let mut o = observation();
    o.visible_actors.clear();
    let real = ItemView {
        corpse: Some(CreatureAppearance {
            seed: AppearanceSeed::from_digest([28; 32]),
            worn: Vec::new(),
            asset: Some("creature.equine".into()),
        }),
        id: ItemTarget::from_digest([29; 32]),
        name: "true corpse".into(),
        description: "true creature".into(),
        class: ItemClass::Corpse,
        quantity: 1,
        appearance: "true corpse".into(),
        identified: true,
        identity: "item.corpse.fixture".parse().unwrap(),
        knowledge: Default::default(),
        asset: Some("item.corpse".into()),
        known_currency: None,
        known_supply: None,
    };
    let mut sanitized = real.clone();
    sanitized.identity = ItemClass::Misc.unknown_identity();
    sanitized.knowledge = Default::default();
    sanitized.corpse = None;
    sanitized.name = "object".into();
    sanitized.appearance = "object".into();
    sanitized.description.clear();
    sanitized.asset = None;
    o.ground_items.push(GroundItemView {
        presentation: Some(ObjectPresentation {
            name: "silver flask".into(),
            asset: None,
            kind: ObjectPresentationKind::Item {
                class: ItemClass::Potion,
            },
        }),
        item: sanitized,
        position: Position { x: 0, y: 0, z: 0 },
        reachable: true,
        supply_reachable: false,
    });
    o.inventory.push(ItemView {
        id: ItemTarget::from_digest([30; 32]),
        ..real.clone()
    });
    let projected = view(&from_observation(o.clone()), &Palette::default());
    let false_item = &projected.items[0];
    assert_eq!(false_item.model, ItemModel::Potion);
    assert_eq!(false_item.name, "silver flask");
    assert_eq!(false_item.corpse_seed, None);
    assert_eq!(false_item.corpse_model, CreatureModel::Placeholder);
    assert!(false_item.description.is_empty());
    assert_eq!(false_item.id, real.id);
    assert_eq!(false_item.class, ItemClass::Corpse);
    assert_eq!(projected.inventory[0].model, ItemModel::Corpse);
    assert_eq!(projected.inventory[0].name, "true corpse");
    o.ground_items[0].presentation.as_mut().unwrap().kind = ObjectPresentationKind::Stairs;
    o.ground_items[0].presentation.as_mut().unwrap().name = "stairs".into();
    let projected = view(&from_observation(o), &Palette::default());
    assert_eq!(projected.items[0].model, ItemModel::Stairs);
    assert!(
        matches!(tor_client_3d_model::describe_focus(&projected, tor_client_3d_model::Focus::Item(real.id)).2,
        Some(tor_client_3d_model::Intent::Take(id)) if id == real.id)
    );
    assert!(!projected
        .terrain()
        .any(|cell| cell.stairs_up || cell.stairs_down));
}

#[test]
fn actual_stair_cosmetic_models_use_the_palette_and_cure_to_factual_art() {
    use tor_protocol::{
        CellView, ObjectPresentation, ObjectPresentationKind, PaletteBody, PaletteUpdate, Position,
    };
    let at = Position { x: 1, y: 0, z: 0 };
    let mut o = observation();
    o.visible_actors.clear();
    o.visible_cells.push(CellView {
        object_presentation: Some(ObjectPresentation {
            name: "silver flask".into(),
            asset: Some("item.wand.authored".into()),
            kind: ObjectPresentationKind::Item {
                class: ItemClass::Potion,
            },
        }),
        lit: true,
        door: None,
        trap: None,
        material: String::new(),
        key: "real-stairs".into(),
        stairs_up: true,
        stairs_down: false,
        position: at,
        wall: false,
        place_hint: false,
        asset: None,
    });
    let mut palette = Palette::default();
    let state = from_observation(o.clone());
    let fallback = view(&state, &palette);
    assert_eq!(fallback.object_models[&at], ItemModel::Potion);
    palette.apply(&PaletteUpdate {
        revision: 1,
        body: PaletteBody::Full {
            assets: ["item.wand.authored".into()].into(),
        },
    });
    let projected = view(&state, &palette);
    assert_eq!(projected.object_models[&at], ItemModel::Wand);
    let terrain = projected
        .terrain()
        .find(|cell| cell.position == at)
        .unwrap();
    assert_eq!(terrain.object_model, Some(ItemModel::Wand));
    assert!(terrain.stairs_up && !terrain.stairs_down);
    assert_eq!(terrain.object_presentation.unwrap().name, "silver flask");
    o.visible_cells[0].object_presentation = None;
    let cured = view(&from_observation(o), &palette);
    assert!(cured.object_models.is_empty());
    let terrain = cured.terrain().find(|cell| cell.position == at).unwrap();
    assert_eq!(terrain.object_model, None);
    assert!(terrain.stairs_up && !terrain.stairs_down);
}

#[test]
fn hydrated_stair_art_repairs_with_palette_then_current_none_overrides_only_that_cell() {
    use tor_protocol::*;
    let current_at = Position { x: 12, y: 4, z: 0 };
    let retained_at = Position { x: 14, y: 4, z: 0 };
    let mut observed = observation();
    observed.visible_actors.clear();
    observed.ground_items.clear();
    // Three fixed, non-collinear disclosed anchors certify the unchanged frame
    // across the later observation delta; retained placement is not guessed.
    observed.visible_cells = [
        Position { x: 0, y: 0, z: 0 },
        Position { x: 0, y: 1, z: 0 },
        Position { x: 1, y: 0, z: 0 },
    ]
    .into_iter()
    .enumerate()
    .map(|(index, position)| CellView {
        object_presentation: None,
        key: format!("anchor-{index}"),
        position,
        lit: true,
        wall: false,
        material: "stone".into(),
        asset: None,
        place_hint: false,
        stairs_up: false,
        stairs_down: false,
        trap: None,
        door: None,
    })
    .collect();
    observed.visible_cells.sort_by_key(|cell| cell.position);
    let mut client = from_observation(observed);
    let query = client.next_map_query().unwrap();
    client.bind_map_query(&query, "historical-stairs").unwrap();
    let sample = ObjectPresentation {
        name: "silver flask".into(),
        asset: Some("item.wand.authored".into()),
        kind: ObjectPresentationKind::Item {
            class: ItemClass::Potion,
        },
    };
    let cells: Vec<_> = [
        ("current-stair", current_at),
        ("retained-stair", retained_at),
    ]
    .into_iter()
    .map(|(key, position)| RememberedMapCell {
        object_presentation: Some(sample.clone()),
        key: key.into(),
        position,
        wall: false,
        material: "stone".into(),
        asset: None,
        place_hint: false,
        stairs_up: true,
        stairs_down: false,
        trap: None,
        door: None,
        ground_items: vec![],
    })
    .collect();
    let page = MapPage {
        transfer: "historical-stair-transfer".into(),
        actor: client.actor(),
        branch: client.branch().clone(),
        base: client.observation_base(),
        window: query.window,
        start: 0,
        page: 0,
        total_cells: 2,
        complete: true,
        ambiguous: false,
        limited: false,
        cells: cells.clone(),
        next: None,
    };
    client
        .accept_map_page(&client.reply_context(), "historical-stairs", &page, 2048)
        .unwrap();
    let mut palette = Palette::default();
    let before = view(&client, &palette);
    assert_eq!(before.memory, cells);
    assert!(
        before.items.is_empty(),
        "remembered art never creates targetable items"
    );
    for at in [current_at, retained_at] {
        let cell = before.terrain().find(|c| c.position == at).unwrap();
        assert!(!cell.seen && !cell.lit && cell.stairs_up && !cell.stairs_down);
        assert_eq!(cell.object_model, Some(ItemModel::Potion));
    }
    // The immutable stored sample stays the same; only resolving its selected
    // asset changes. No current observation supplies this retained artwork.
    palette.apply(&PaletteUpdate {
        revision: 1,
        body: PaletteBody::Full {
            assets: ["item.wand.authored".into()].into(),
        },
    });
    let repaired = view(&client, &palette);
    assert_eq!(repaired.memory, before.memory);
    assert_eq!(repaired.shown, before.shown);
    for at in [current_at, retained_at] {
        assert_eq!(repaired.object_models[&at], ItemModel::Wand);
    }
    let mut next = client.state().clone();
    next.revision += 1;
    next.snapshot = SnapshotId(format!("fixture-display-{}", next.revision));
    next.observation.visible_cells.push(CellView {
        object_presentation: None,
        key: "current-stair".into(),
        position: current_at,
        lit: true,
        wall: false,
        material: "stone".into(),
        asset: None,
        place_hint: false,
        stairs_up: true,
        stairs_down: false,
        trap: None,
        door: None,
    });
    let delta = StateDelta::between(client.state(), &next).unwrap();
    client
        .apply(StreamUpdate {
            context: client.context().clone(),
            actor: next.observation.actor,
            branch: client.branch().clone(),
            cursor: StreamCursor {
                sequence: client.cursor().sequence + 1,
                tick: next.observation.tick,
            },
            body: UpdateBody::ObservationDelta {
                base: client.observation_base(),
                state: Box::new(delta),
                event: None,
            },
        })
        .unwrap();
    let current = view(&client, &palette);
    let seen = current
        .terrain()
        .find(|c| c.position == current_at)
        .unwrap();
    assert!(seen.seen && seen.lit && seen.stairs_up && !seen.stairs_down);
    assert_eq!(seen.object_presentation, None);
    assert_eq!(
        seen.object_model, None,
        "current factual None must not fall back to old art"
    );
    assert!(!current.object_models.contains_key(&current_at));
    assert_eq!(
        current.memory,
        vec![cells[1].clone()],
        "unseen art survives another cell's cure"
    );
    let retained = current
        .terrain()
        .find(|c| c.position == retained_at)
        .unwrap();
    assert!(!retained.seen && !retained.lit);
    assert_eq!(retained.object_presentation, Some(&sample));
    assert_eq!(retained.object_model, Some(ItemModel::Wand));
    assert!(current.items.is_empty());
}

#[test]
fn persistent_public_appearance_survives_identification_and_sampled_disguise() {
    use tor_client_3d_model::ItemSurface;
    use tor_protocol::{
        GroundItemView, ObjectPresentation, ObjectPresentationKind, PaletteBody, PaletteUpdate,
    };
    let mut palette = Palette::default();
    palette.apply(&PaletteUpdate {
        revision: 1,
        body: PaletteBody::Full {
            assets: [
                "item.potion.amber".into(),
                "item.potion.crimson".into(),
                "item.potion.unknown".into(),
            ]
            .into_iter()
            .collect(),
        },
    });
    let mut observed = observation();
    observed.inventory.clear();
    observed.ground_items.clear();
    let item = ItemView {
        corpse: None,
        class: ItemClass::Potion,
        quantity: 1,
        id: ItemTarget::from_digest([91; 32]),
        name: "amber potion".into(),
        description: String::new(),
        appearance: "amber potion".into(),
        identified: false,
        identity: "item.potion.unknown".parse().unwrap(),
        knowledge: Default::default(),
        asset: Some("item.potion.amber".into()),
        known_supply: None,
        known_currency: None,
    };
    observed.inventory.push(item.clone());
    let mut ground_item = item;
    ground_item.id = ItemTarget::from_digest([92; 32]);
    observed.ground_items.push(GroundItemView {
        presentation: None,
        item: ground_item,
        reachable: true,
        supply_reachable: false,
        position: observed.position,
    });
    let unknown = view(&from_observation(observed.clone()), &palette);
    assert_eq!(unknown.items[0].surface, ItemSurface::Potion(0));
    assert_eq!(unknown.inventory[0].surface, ItemSurface::Potion(0));
    for (name, identity) in [
        ("potion of healing", "item.potion.healing"),
        ("potion of strength", "item.potion.strength"),
        ("crimson potion", "item.potion.healing"),
    ] {
        observed.inventory[0].name = name.into();
        observed.inventory[0].identified = true;
        observed.inventory[0].identity = identity.parse().unwrap();
        observed.ground_items[0].item.name = name.into();
        observed.ground_items[0].item.identified = true;
        observed.ground_items[0].item.identity = identity.parse().unwrap();
        let known = view(&from_observation(observed.clone()), &palette);
        assert_eq!(known.items[0].surface, unknown.items[0].surface);
        assert_eq!(known.inventory[0].surface, unknown.inventory[0].surface);
    }
    observed.ground_items[0].item.name = "object".into();
    // A sampled disguise blanks semantic identity without revoking known actions.
    observed.ground_items[0].item.identity = "item.unknown".parse().unwrap();
    observed.ground_items[0].item.knowledge = Default::default();
    assert!(observed.ground_items[0].item.identified);
    observed.ground_items[0].item.appearance = "object".into();
    observed.ground_items[0].item.asset = None;
    observed.ground_items[0].presentation = Some(ObjectPresentation {
        name: "arbitrary sampled display".into(),
        asset: Some("item.potion.crimson".into()),
        kind: ObjectPresentationKind::Item {
            class: ItemClass::Potion,
        },
    });
    let sampled = view(&from_observation(observed.clone()), &palette);
    assert_eq!(sampled.items[0].surface, ItemSurface::Potion(4));
    observed.ground_items[0].item.appearance = "violet potion".into();
    observed.ground_items[0].item.asset = Some("item.potion.amber".into());
    assert!(
        serde_json::from_value::<GroundItemView>(
            serde_json::to_value(&observed.ground_items[0]).unwrap()
        )
        .is_err(),
        "wire boundary rejects leaking real cosmetics behind a sampled disguise"
    );
    observed.ground_items[0].item.appearance = "object".into();
    observed.ground_items[0].item.asset = None;
    assert_eq!(
        view(&from_observation(observed.clone()), &palette).items[0].surface,
        sampled.items[0].surface
    );
    observed.ground_items[0]
        .presentation
        .as_mut()
        .unwrap()
        .asset = Some("item.potion.unknown".into());
    assert_eq!(
        view(&from_observation(observed), &palette).items[0].surface,
        ItemSurface::Neutral
    );
}

#[test]
fn reset_picture_spatial_history_does_not_replay_ordered_effects() {
    use tor_protocol::*;
    let mut observed = observation();
    observed.visible_cells.push(CellView {
        key: "a".repeat(40),
        position: observed.position,
        wall: false,
        material: "stone".into(),
        asset: None,
        lit: true,
        door: None,
        trap: None,
        object_presentation: None,
        stairs_up: false,
        stairs_down: false,
        place_hint: false,
    });
    let cell = &observed.visible_cells[0];
    observed.combat.as_mut().unwrap().events = vec![CombatEventView::Effect {
        caster: None,
        ability: None,
        tick: observed.tick,
        timing: EffectTiming::Instant,
        appearance: EffectAppearance::Bolt,
        delivery: EffectDelivery::Ray,
        cue: EffectCue::Neutral,
        segments: vec![vec![cell.position]],
        anchors: vec![EffectAnchor {
            key: cell.key.clone(),
            position: cell.position,
        }],
    }];
    let state = from_observation(observed);
    let picture = view(&state, &Palette::default());
    assert!(
        picture.events.is_empty(),
        "snapshot history is not a new transient"
    );
}

#[test]
fn public_effect_lifecycle_replaces_shared_state_and_model_without_start_or_end_events() {
    use tor_protocol::*;
    let mut observed = observation();
    let at = observed.position;
    observed.visible_cells.push(CellView {
        key: "a".repeat(40),
        position: at,
        wall: false,
        material: "stone".into(),
        asset: None,
        lit: true,
        door: None,
        trap: None,
        object_presentation: None,
        stairs_up: false,
        stairs_down: false,
        place_hint: false,
    });
    let effect = PublicEffectView {
        id: "b".repeat(40),
        observed_since: observed.tick,
        expires_at: Some(observed.tick + 20),
        appearance: EffectAppearance::Cloud,
        cue: EffectCue::Fire,
        delivery: EffectDelivery::Area,
        segments: vec![vec![at]],
        anchors: vec![EffectAnchor {
            key: "a".repeat(40),
            position: at,
        }],
    };
    observed.public_effects = vec![effect.clone()];
    let mut client = from_observation(observed.clone());
    let first = view(&client, &Palette::default());
    assert_eq!(first.public_effects, vec![effect.clone()]);
    assert!(
        first.events.is_empty(),
        "presence is not a replayed Start event"
    );
    let mut updated = effect.clone();
    updated.cue = EffectCue::Cold;
    updated.expires_at = None;
    for effects in [vec![updated.clone()], vec![], vec![updated.clone()]] {
        let mut next = client.state().clone();
        next.revision += 1;
        next.snapshot = SnapshotId(format!("effect-picture-{}", next.revision));
        next.observation.public_effects = effects.clone();
        let delta = StateDelta::between(client.state(), &next).unwrap();
        client
            .apply(StreamUpdate {
                context: client.context().clone(),
                actor: next.observation.actor,
                branch: client.branch().clone(),
                cursor: StreamCursor {
                    sequence: client.cursor().sequence + 1,
                    tick: next.observation.tick,
                },
                body: UpdateBody::ObservationDelta {
                    base: client.observation_base(),
                    state: Box::new(delta),
                    event: None,
                },
            })
            .unwrap();
        let picture = view(&client, &Palette::default());
        assert_eq!(picture.public_effects, effects);
        assert!(picture.events.is_empty());
    }
    // Reconnect/reset uses the current full snapshot, never a remembered old instance.
    observed.public_effects.clear();
    observed.visible_cells.clear();
    let reset = from_observation(observed);
    assert!(view(&reset, &Palette::default()).public_effects.is_empty());
}
