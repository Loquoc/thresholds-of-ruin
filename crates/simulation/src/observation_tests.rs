use super::*;
use std::num::NonZeroU64;
use tor_world::{Extent, Passage, World};

fn at(region: u64, x: i32, y: i32) -> Location {
    Location {
        region: RegionId(region),
        position: Position { x, y, z: 0 },
    }
}

fn membership_fixture() -> (Game, ActorId) {
    let mut game = Game::two_room_in_stone(42, crate::test_creatures::content());
    let actor = game
        .spawn_actor(at(1, 0, 0), NonZeroU64::new(100).unwrap())
        .unwrap();
    (game, actor)
}

#[test]
fn ground_and_inventory_item_knowledge_is_private_independent_and_remembered() {
    let (mut game, actor) = membership_fixture();
    let location = game.actors[&actor].location;
    let identity: crate::ItemIdentity = "item.armor.test-mail".parse().unwrap();
    let mut spec = crate::ItemSpec::ordinary(identity.clone(), "mail of concealed power".into());
    spec.class = crate::ItemClass::Armor;
    spec.concealed = true;
    spec.appearance = "ordinary mail".into();
    spec.asset = Some("item.armor.mail".into());
    spec.equipment = Some(crate::EquipmentSpec {
        slot: crate::EquipmentSlot::new("body_armor").unwrap(),
        periodic_relocation: None,
        armor: Some(crate::ArmorSpec {
            physical_reduction: 10,
            material: crate::ArmorMaterial::Metal,
            rustproof: false,
        }),
        attack: None,
        weapon_profile: None,
        modifiers: vec![],
        potency_modifiers: vec![],
    });
    let item = game.place_item_stack(400, location, None, 1, spec).unwrap();
    let ground = |game: &Game| {
        game.observe(actor)
            .unwrap()
            .ground_items
            .into_iter()
            .find(|i| i.id == item)
            .unwrap()
    };
    assert_eq!(
        ground(&game).identity,
        crate::ItemClass::Armor.unknown_identity()
    );
    assert_eq!(ground(&game).knowledge, crate::ItemKnowledge::default());
    let mut type_known = game.clone();
    type_known.learn_identity(actor, &identity).unwrap();
    assert_eq!(ground(&type_known).identity, identity);
    assert_eq!(ground(&type_known).knowledge.buc, crate::BucStatus::Unknown);
    game.actors
        .get_mut(&actor)
        .unwrap()
        .known_equipment
        .learn_curse(item, true)
        .unwrap();
    assert_eq!(ground(&game).knowledge.buc, crate::BucStatus::Cursed);
    assert!(!ground(&game).identified);
    let remembered = crate::armor::ItemCondition {
        corrosion: 1,
        ..Default::default()
    };
    game.items.edit(item, |i| i.condition = remembered).unwrap();
    game.actors
        .get_mut(&actor)
        .unwrap()
        .known_equipment
        .learn_numbers(item, remembered)
        .unwrap();
    let numeric = ground(&game);
    assert_eq!(numeric.knowledge.enchantments.hit, Some(0));
    assert_eq!(numeric.knowledge.condition.corrosion, Some(1));
    assert_eq!(numeric.knowledge.condition.protected, None);
    assert!(!numeric.identified);
    game.items
        .edit(item, |i| i.condition.corrosion = 2)
        .unwrap();
    assert_eq!(
        ground(&game).knowledge,
        numeric.knowledge,
        "unseen current rust must not replace remembered rust"
    );
    let stranger = game
        .spawn_actor(at(1, 1, 0), NonZeroU64::new(100).unwrap())
        .unwrap();
    let seen = game
        .observe(stranger)
        .unwrap()
        .ground_items
        .into_iter()
        .find(|i| i.id == item)
        .unwrap();
    assert_eq!(seen.knowledge, crate::ItemKnowledge::default());
    game.items
        .edit(item, |i| i.location = crate::ItemLocation::Carried(actor))
        .unwrap();
    let carried = game
        .observe(actor)
        .unwrap()
        .inventory
        .into_iter()
        .find(|i| i.id == item)
        .unwrap();
    assert_eq!(carried.knowledge, numeric.knowledge);
    assert_eq!(carried.identity, numeric.identity);
    assert_eq!(carried.asset, numeric.asset);
}
#[test]
fn streaming_geometry_reuses_scene_witness_without_rebuilding_body_queries() {
    let (game, actor) = membership_fixture();
    game.scene_snapshot_shared(actor).unwrap();
    let expected = (
        game.reach_regions(actor),
        game.perception_scene_regions(actor),
    );
    assert_eq!(game.streaming_actor_regions(actor), expected);
    let before = crate::diagnostics::work_counts().body_cells;
    for _ in 0..4 {
        assert_eq!(game.streaming_actor_regions(actor), expected);
    }
    assert_eq!(
        crate::diagnostics::work_counts().body_cells - before,
        0,
        "an identical scene witness must reuse its geometric streaming queries"
    );
}

#[test]
fn unchanged_actor_classification_is_shared_by_full_and_reference_views() {
    let (game, actor) = membership_fixture();
    let expected = game.observe(actor).unwrap();
    let before = crate::diagnostics::work_counts().perception_projections;
    assert_eq!(game.observe(actor).unwrap(), expected);
    let references = game.observe_actor_references(actor).unwrap();
    assert_eq!(
        references.visible_actors,
        expected
            .visible_actors
            .iter()
            .map(|row| ActorReferenceRow {
                id: row.id,
                disclosure: row.disclosure,
            })
            .collect::<Vec<_>>()
    );
    assert_eq!(
        crate::diagnostics::work_counts().perception_projections - before,
        0,
        "complete and reference views share immutable classification, not later state"
    );
}

#[test]
fn streaming_geometry_checks_candidate_body_and_world_before_reuse() {
    let (game, actor) = membership_fixture();
    let retained = game.scene_snapshot_shared(actor).unwrap();
    let original = game.streaming_actor_regions(actor);
    let mut candidate = game.clone();
    candidate.set_wall(at(1, 1, 0), true).unwrap();
    candidate.actors.get_mut(&actor).unwrap().orientation = 1;
    let expected = (
        candidate.reach_regions(actor),
        candidate.perception_scene_regions(actor),
    );
    assert_eq!(candidate.streaming_actor_regions(actor), expected);
    assert!(Arc::ptr_eq(
        &retained,
        &game.scene_snapshot_shared(actor).unwrap()
    ));
    assert_eq!(game.streaming_actor_regions(actor), original);
    let changed = candidate.scene_snapshot_shared(actor).unwrap();
    assert!(!Arc::ptr_eq(&retained, &changed));
    assert_eq!(candidate.streaming_actor_regions(actor), expected);
    let before = crate::diagnostics::work_counts().body_cells;
    assert_eq!(candidate.streaming_actor_regions(actor), expected);
    assert_eq!(crate::diagnostics::work_counts().body_cells, before);
}

#[test]
fn classification_reuse_keeps_health_time_items_and_rejected_branches_fresh() {
    use crate::perception::contract_fixtures::{fixture_with, OWNER, SUBJECT};
    let mut game = fixture_with(&[], &[], |_, owner, _| {
        owner.modifiers.push(crate::rules::Modifier {
            cancellable: false,
            scope: crate::rules::Scope::Always,
            op: crate::rules::ModifierOp::Set {
                stat: crate::rules::Stat::Health,
                value: 100,
            },
        });
    });
    let original = game.observe(OWNER).unwrap();
    let scene = game.scene_snapshot_shared(OWNER).unwrap();
    let classifications = scene.classified_actors(&game, OWNER).unwrap();
    assert!(game.health(OWNER).unwrap().0 > 1, "nonlethal fixture");
    let visibility = game.actors.visibility_epoch();
    crate::test_creatures::injure(&mut game, OWNER, 1).unwrap();
    game.tick += 1;
    let item = game
        .place_item(
            game.actors[&OWNER].location,
            "item.test.fresh-fact".parse().unwrap(),
            "fresh fact".into(),
        )
        .unwrap();
    assert_eq!(game.actors.visibility_epoch(), visibility);
    assert!(Arc::ptr_eq(
        &classifications,
        &scene.classified_actors(&game, OWNER).unwrap()
    ));
    let fresh = game.observe(OWNER).unwrap();
    assert_eq!(fresh.tick, original.tick + 1);
    assert_eq!(
        fresh.combat.as_ref().unwrap().hp,
        original.combat.as_ref().unwrap().hp - 1
    );
    assert!(fresh.ground_items.iter().any(|row| row.id == item));
    let oracle = |game: &Game| {
        let mut uncached = game.clone();
        uncached.lifecycle.scenes = ActorScenes::default();
        uncached.observe(OWNER).unwrap()
    };
    assert_eq!(fresh, oracle(&game));
    let mut rejected = game.clone();
    crate::test_creatures::injure(&mut rejected, SUBJECT, u32::MAX).unwrap();
    assert!(!rejected.alive(SUBJECT));
    let absent = rejected.observe(OWNER).unwrap();
    assert!(absent.visible_actors.iter().all(|row| row.id != SUBJECT));
    assert_eq!(absent, oracle(&rejected));
    assert_eq!(
        game.observe(OWNER).unwrap(),
        fresh,
        "rejected epoch cannot poison retained facts"
    );
    assert_eq!(
        original.tick + 1,
        fresh.tick,
        "retained values remain immutable"
    );
}

#[test]
fn complete_observations_retain_personal_facts_without_expanding_inspection() {
    use crate::perception::contract_fixtures::{OWNER, SUBJECT};
    let mut game = crate::hallucination::tests::named_fixture(&[]);
    let original = game.clone();
    let expected_stats = game.creature(OWNER).unwrap().own_stats();
    let before = crate::diagnostics::work_counts();
    let (view, scene, _) = game.observe_event_shared_scene(OWNER).unwrap();
    let (repeated, same_scene, _) = game.observe_event_shared_scene(OWNER).unwrap();
    assert_eq!(view, repeated);
    assert_eq!(scene, same_scene);
    assert!(view.visible_actors.iter().any(|row| row.id == SUBJECT));
    let captured = crate::diagnostics::work_counts();
    assert_eq!(
        captured.own_stats_materializations,
        before.own_stats_materializations
    );
    assert_eq!(
        captured.own_stats_static_entries_copied,
        before.own_stats_static_entries_copied
    );
    let retained_stats = view.combat.as_ref().unwrap().own_stats.as_ref().unwrap();
    assert_eq!(retained_stats.materialize(), expected_stats);
    assert_eq!(
        crate::diagnostics::work_counts().own_stats_materializations,
        captured.own_stats_materializations + 1
    );
    assert_eq!(game, original);
    game.apply_condition(
        OWNER,
        &crate::hallucination_clock::integration_tests::condition(),
        OWNER,
        1000,
    )
    .unwrap();
    let changed = game.observe(OWNER).unwrap();
    assert_ne!(changed, view);
    assert_eq!(
        changed
            .visible_actors
            .iter()
            .find(|row| row.id == SUBJECT)
            .unwrap()
            .name,
        "cosmetic c"
    );
    assert_ne!(
        changed
            .combat
            .as_ref()
            .unwrap()
            .own_stats
            .as_ref()
            .unwrap()
            .materialize(),
        expected_stats
    );
    assert_eq!(retained_stats.materialize(), expected_stats);
    assert_eq!(original.observe(OWNER).unwrap(), view);
    assert_eq!(game.rules_rng, original.rules_rng);
}

fn assert_direct_references_match_rows(game: &Game, observer: ActorId) {
    let snapshot = game.scene_snapshot_shared(observer).unwrap();
    let rows = game
        .observe_actor_references_in_snapshot(observer, &snapshot)
        .unwrap();
    let projection = snapshot.classified_actors(game, observer).unwrap();
    for subject in game.loaded_actor_ids() {
        let expected = rows
            .visible_actors
            .iter()
            .find(|row| row.id == subject)
            .and_then(|row| row.disclosure.reference());
        assert_eq!(
            game.projected_actor_reference(
                observer,
                subject,
                snapshot.projection_cells(),
                &projection,
            ),
            expected,
            "direct authority must match canonical rows for {subject:?}"
        );
    }
}

#[test]
fn actor_reference_rows_share_canonical_body_order_and_exclude_self_anchor() {
    use crate::perception::contract_fixtures::{fixture_with_height, OWNER, SUBJECT};
    use crate::perception::ActorDisclosure::Creature;
    let mut game = fixture_with_height(&[], &[], 2, |_, _, _| {});
    for id in [OWNER, SUBJECT] {
        game.set_body(
            id,
            crate::BodySpec {
                cells: vec![[0, 0, 0], [0, 0, 1]],
                eye: [0, 0, 1],
                mass: 80,
            },
        )
        .unwrap();
    }
    let full = game.observe(OWNER).unwrap();
    let before = crate::diagnostics::work_counts();
    let references = game.observe_actor_references(OWNER).unwrap();
    assert_eq!(references.location, game.actors[&OWNER].location);
    // One own upper-body row, then the subject's two body rows. The own
    // anchor is excluded even though its location is ordinarily disclosed.
    assert_eq!(
        references.visible_actors,
        vec![
            ActorReferenceRow {
                id: OWNER,
                disclosure: Creature
            },
            ActorReferenceRow {
                id: SUBJECT,
                disclosure: Creature
            },
            ActorReferenceRow {
                id: SUBJECT,
                disclosure: Creature
            },
        ]
    );
    assert_eq!(
        references.visible_actors,
        full.visible_actors
            .iter()
            .map(|row| ActorReferenceRow {
                id: row.id,
                disclosure: row.disclosure
            })
            .collect::<Vec<_>>()
    );
    let after = crate::diagnostics::work_counts();
    assert_eq!(after.observations, before.observations);
    assert_eq!(
        after.perception_projections - before.perception_projections,
        0
    );
    assert_direct_references_match_rows(&game, OWNER);
}

#[test]
fn actor_reference_rows_preserve_anonymous_hidden_and_guised_authority() {
    use crate::perception::contract_fixtures::{at, fixture, guise, OWNER, SUBJECT};
    use crate::perception::{ActorDisclosure, PerceptionValue};
    let mut invisible = fixture(&[], &[PerceptionValue::Invisible { enabled: true }]);
    assert_direct_references_match_rows(&invisible, OWNER);
    let nearby = invisible.observe_actor_references(OWNER).unwrap();
    assert_eq!(
        nearby.visible_actors,
        vec![ActorReferenceRow {
            id: SUBJECT,
            disclosure: ActorDisclosure::AnonymousContact,
        }]
    );
    let full = invisible.observe(OWNER).unwrap();
    assert_eq!(full.visible_actors.len(), 1);
    assert_eq!(
        full.visible_actors[0].disclosure,
        ActorDisclosure::AnonymousContact
    );
    invisible.teleport(SUBJECT, at(1, 6, 2)).unwrap();
    assert_direct_references_match_rows(&invisible, OWNER);
    assert!(invisible
        .observe_actor_references(OWNER)
        .unwrap()
        .visible_actors
        .is_empty());
    assert!(invisible.observe(OWNER).unwrap().visible_actors.is_empty());
    let detecting = fixture(
        &[PerceptionValue::SeeInvisible { enabled: true }],
        &[PerceptionValue::Invisible { enabled: true }],
    );
    assert_eq!(
        detecting
            .observe_actor_references(OWNER)
            .unwrap()
            .visible_actors,
        vec![ActorReferenceRow {
            id: SUBJECT,
            disclosure: ActorDisclosure::Creature
        }]
    );
    assert_direct_references_match_rows(&detecting, OWNER);
    let guised = fixture(&[], &[guise()]);
    assert_direct_references_match_rows(&guised, OWNER);
    assert!(guised
        .observe_actor_references(OWNER)
        .unwrap()
        .visible_actors
        .is_empty());
    let full = guised.observe(OWNER).unwrap();
    assert!(full.visible_actors.is_empty());
    assert_eq!(full.apparent_objects.len(), 1);
    assert_eq!(full.apparent_objects[0].subject, SUBJECT);
}

#[test]
fn shared_hearing_geometry_keeps_physical_stair_and_source_priority_boundaries() {
    use crate::perception::contract_fixtures::{at, fixture, OWNER, SUBJECT};
    let mut game = fixture(&[], &[]);
    let origin = game.actors[&OWNER].location;
    let source = game.actors[&SUBJECT].location;
    let landing = at(2, 2, 2);
    game.register_named_anchors(
        RegionId(2),
        [("hearing_landing".into(), landing.position)].into(),
    )
    .unwrap();
    game.connect_named_stair(
        origin,
        Direction::Up,
        tor_world::NamedAnchor {
            region: RegionId(2),
            name: "hearing_landing".into(),
        },
    )
    .unwrap();
    let snapshot = game.scene_snapshot_shared(OWNER).unwrap();
    assert!(snapshot
        .cells
        .iter()
        .any(|cell| cell.location == landing && !cell.physical));
    let expected: BTreeSet<_> = game
        .world
        .eye_scene(origin, 0, 6)
        .into_iter()
        .filter(|cell| cell.physical)
        .map(|cell| cell.location)
        .collect();
    assert!(!expected.contains(&landing));
    assert_eq!(snapshot.hearing_locations(&game), &expected);
    assert_eq!(
        game.attack_sound_alert(OWNER, source, Some(origin))
            .unwrap(),
        Some(source)
    );
    assert_eq!(
        game.attack_sound_alert(OWNER, landing, Some(origin))
            .unwrap(),
        Some(origin)
    );
    assert_eq!(game.attack_sound_alert(OWNER, landing, None).unwrap(), None);
}

#[test]
fn shared_hearing_geometry_is_immutable_across_rejected_candidate_locations() {
    use crate::perception::contract_fixtures::{at, fixture, OWNER};
    let game = fixture(&[], &[]);
    let original = game.clone();
    let retained = game.scene_snapshot_shared(OWNER).unwrap();
    let expected = retained.hearing_locations(&game).clone();
    let mut rejected = game.clone();
    rejected.teleport(OWNER, at(2, 2, 2)).unwrap();
    let changed = rejected.scene_snapshot_shared(OWNER).unwrap();
    let changed_locations = changed.hearing_locations(&rejected);
    let exact: BTreeSet<_> = rejected
        .world
        .eye_scene(at(2, 2, 2), 0, 6)
        .into_iter()
        .filter(|cell| cell.physical)
        .map(|cell| cell.location)
        .collect();
    assert_eq!(changed_locations, &exact);
    assert_ne!(changed_locations, &expected);
    assert_eq!(retained.hearing_locations(&game), &expected);
    let before = crate::diagnostics::work_counts();
    let restored = game.scene_snapshot_shared(OWNER).unwrap();
    assert_eq!(restored.hearing_locations(&game), &expected);
    assert_eq!(
        crate::diagnostics::work_counts().hearing_scene_builds - before.hearing_scene_builds,
        1
    );
    assert_eq!(game, original);
    assert_eq!(game.rules_rng, rejected.rules_rng);
}

#[test]
fn shared_scene_geometry_indexes_initialize_once_per_payload() {
    let (game, actor) = membership_fixture();
    let snapshot = game.scene_snapshot_shared(actor).unwrap();
    assert!(!snapshot.cells.is_empty());
    assert!(!snapshot.sight.is_empty());
    assert!(snapshot.projection.get().is_none());
    let expected_occurrences: BTreeSet<_> = snapshot
        .cells
        .iter()
        .map(|cell| (cell.location, cell.rotation))
        .collect();
    let before = crate::diagnostics::work_counts();
    let indexes = snapshot.projection();
    assert_eq!(indexes.occurrences, expected_occurrences);
    let initialized = crate::diagnostics::work_counts();
    assert_eq!(
        initialized.projection_location_indexes - before.projection_location_indexes,
        2
    );
    assert_eq!(
        initialized.projection_location_cells - before.projection_location_cells,
        snapshot.cells.len() + snapshot.sight.len()
    );
    let retained = game.scene_snapshot_shared(actor).unwrap();
    assert!(Arc::ptr_eq(&snapshot, &retained));
    assert!(std::ptr::eq(indexes, retained.projection()));
    assert!(Arc::ptr_eq(&snapshot.cells, &retained.cells));
    assert!(Arc::ptr_eq(&snapshot.sight, &retained.sight));
    let reused = crate::diagnostics::work_counts();
    assert_eq!(
        reused.projection_location_indexes,
        initialized.projection_location_indexes
    );
    assert_eq!(reused.scene_cells_copied, before.scene_cells_copied);
}

#[test]
fn shared_scene_geometry_never_caches_actor_or_item_disclosure() {
    let (mut game, actor) = membership_fixture();
    let (before, shared) = game.observe_shared_scene(actor).unwrap();
    let original = game.clone();
    let other = game
        .spawn_actor(at(1, 1, 0), NonZeroU64::new(100).unwrap())
        .unwrap();
    let item = game
        .place_item(
            at(1, 1, 0),
            "item.test.new-shared-scene-item".parse().unwrap(),
            "new shared-scene item".into(),
        )
        .unwrap();
    let (after, unchanged_geometry) = game.observe_shared_scene(actor).unwrap();
    assert!(Arc::ptr_eq(&shared, &unchanged_geometry));
    assert!(before.visible_actors.iter().all(|row| row.id != other));
    assert!(before.ground_items.iter().all(|row| row.id != item));
    assert!(after.visible_actors.iter().any(|row| row.id == other));
    assert!(after.ground_items.iter().any(|row| row.id == item));
    assert_eq!(original.observe(actor).unwrap(), before);
    assert_ne!(after, before);
}

#[test]
fn shared_scene_eviction_releases_indexes_but_retained_arrays_remain_valid() {
    let (game, actor) = membership_fixture();
    let original = game.clone();
    let snapshot = game.scene_snapshot_shared(actor).unwrap();
    snapshot.projection();
    assert!(snapshot.contains(at(1, 0, 0)));
    assert!(!snapshot.hearing_locations(&game).is_empty());
    let raw = snapshot.raw_world_facts(&game);
    let weak_raw = Arc::downgrade(&raw);
    drop(raw);
    let retained_cells = snapshot.shared_cells();
    let expected_cells = (*retained_cells).clone();
    let weak = Arc::downgrade(&snapshot);
    // This exercises only the derived cache, not authoritative actor creation.
    for id in 2..=513 {
        game.insert_scene(
            ActorId(id),
            Arc::new(CachedScene::new(snapshot.witness.clone(), vec![], vec![])),
        );
    }
    {
        let cache = game.lifecycle.scenes.0.lock().unwrap();
        assert_eq!(cache.len(), 512);
        assert!(!cache.contains_key(&actor));
        assert!(cache.contains_key(&ActorId(2)));
    }
    drop(snapshot);
    assert!(
        weak_raw.upgrade().is_none(),
        "eviction also releases raw World facts"
    );
    assert!(
        weak.upgrade().is_none(),
        "array consumers must not pin cached indexes"
    );
    assert_eq!(*retained_cells, expected_cells);
    let before = crate::diagnostics::work_counts();
    let rebuilt = game.scene_snapshot_shared(actor).unwrap();
    assert_eq!(*rebuilt.cells, expected_cells);
    assert_eq!(crate::diagnostics::work_counts().scenes - before.scenes, 1);
    let cache = game.lifecycle.scenes.0.lock().unwrap();
    assert_eq!(cache.len(), 512);
    assert!(cache.contains_key(&actor));
    assert!(!cache.contains_key(&ActorId(2)));
    assert_eq!(game, original);
}

#[test]
fn shared_scene_handles_are_immutable_across_rejected_candidate_witnesses() {
    let (game, actor) = membership_fixture();
    let original = game.clone();
    let (before, retained) = game.observe_shared_scene(actor).unwrap();
    let expected = (*retained).clone();
    let mut rejected = game.clone();
    rejected.set_cell_light(at(1, 2, 0), false).unwrap();
    let (changed, candidate_cells) = rejected.observe_shared_scene(actor).unwrap();
    assert_ne!(*candidate_cells, expected);
    assert_ne!(changed, before);
    assert_eq!(*retained, expected);
    let (restored_view, restored_cells) = game.observe_shared_scene(actor).unwrap();
    assert_eq!(restored_view, before);
    assert_eq!(*restored_cells, expected);
    assert!(!Arc::ptr_eq(&candidate_cells, &restored_cells));
    assert_eq!(game, original);
    assert_eq!(game.rules_rng, rejected.rules_rng);
}

#[test]
fn owned_observation_scene_wrapper_copies_only_disclosed_cells() {
    let (game, actor) = membership_fixture();
    let (expected_view, shared) = game.observe_shared_scene(actor).unwrap();
    let before = crate::diagnostics::work_counts();
    let (view, mut owned) = game.observe_scene(actor).unwrap();
    assert_eq!(view, expected_view);
    assert_eq!(owned, *shared);
    assert_eq!(
        crate::diagnostics::work_counts().scene_cells_copied - before.scene_cells_copied,
        shared.len()
    );
    owned[0].offset.x += 100;
    let (_, repeated) = game.observe_shared_scene(actor).unwrap();
    assert!(Arc::ptr_eq(&shared, &repeated));
    assert_ne!(owned[0], repeated[0]);
}

#[test]
fn raw_world_facts_track_hint_mutation_and_rejected_candidate_without_scene_change() {
    let (mut game, actor) = membership_fixture();
    let target = at(1, 1, 0);
    let baseline = game.observe(actor).unwrap();
    assert!(
        !baseline
            .visible_cells
            .iter()
            .find(|cell| cell.location == target)
            .unwrap()
            .place_hint
    );
    let scene = game.scene_snapshot_shared(actor).unwrap();
    let original = game.clone();
    let before = crate::diagnostics::work_counts().raw_world_fact_builds;
    game.world
        .set_named_place_hint(target, "fresh hint")
        .unwrap();
    assert!(Arc::ptr_eq(
        &scene,
        &game.scene_snapshot_shared(actor).unwrap()
    ));
    assert!(
        game.observe(actor)
            .unwrap()
            .visible_cells
            .iter()
            .find(|cell| cell.location == target)
            .unwrap()
            .place_hint
    );
    assert_eq!(
        crate::diagnostics::work_counts().raw_world_fact_builds - before,
        1
    );
    assert_eq!(
        original.observe(actor).unwrap(),
        baseline,
        "candidate-populated raw facts cannot cross the World witness"
    );
    drop(original);
    // A weak-only identity must also detach on mutation without a retained Game.
    game.world.set_place_hint(target, false).unwrap();
    assert_eq!(
        game.observe(actor).unwrap().visible_cells,
        baseline.visible_cells
    );
    assert!(Arc::ptr_eq(
        &scene,
        &game.scene_snapshot_shared(actor).unwrap()
    ));
}

fn raw_feature_fixture() -> (Game, ActorId) {
    let mut world = World::new(vec![], vec![]).unwrap();
    world
        .add_chamber(Region {
            id: RegionId(1),
            name: "raw facts".into(),
            bounds: Extent::new(5, 5, 1).unwrap(),
        })
        .unwrap();
    let mut game = Game::new(world, 42, crate::test_creatures::content());
    let actor = game
        .spawn_actor(at(1, 1, 1), NonZeroU64::new(100).unwrap())
        .unwrap();
    (game, actor)
}

#[test]
fn raw_world_features_keep_discovery_and_trap_family_fresh() {
    let (mut game, actor) = raw_feature_fixture();
    let door = at(1, 2, 1);
    let trap = at(1, 1, 1);
    // Warm absence before introducing physical features.
    assert!(game
        .observe(actor)
        .unwrap()
        .visible_cells
        .iter()
        .all(|cell| cell.door.is_none() && cell.trap.is_none()));
    game.world.place_door(door, 77, false, 1).unwrap();
    game.world
        .conceal_door(door, tor_world::Concealment { dc: 100 })
        .unwrap();
    game.world
        .place_trap(
            trap,
            tor_world::Trap {
                id: 88,
                dc: 100,
                avoidance_dc: None,
                kind: tor_world::TrapKind::Rust {
                    slot: "body_armor".into(),
                    amount: 1,
                },
            },
        )
        .unwrap();
    let hidden = game.observe(actor).unwrap();
    let entrance = hidden
        .visible_cells
        .iter()
        .find(|cell| cell.location == door)
        .unwrap();
    assert_eq!(
        (entrance.door, entrance.wall, entrance.material),
        (None, true, "stone")
    );
    assert!(hidden
        .visible_cells
        .iter()
        .find(|cell| cell.location == trap)
        .unwrap()
        .trap
        .is_none());
    let world = game.world.weak_identity();
    let scene = game.scene_snapshot_shared(actor).unwrap();
    let retained_raw = scene.raw_world_facts(&game).cells.clone();
    let expected_raw = (*retained_raw).clone();
    let expected_hidden = (*hidden.visible_cells).clone();
    let before = crate::diagnostics::work_counts().raw_world_fact_builds;
    // Valid restored actor-owned facts, independent of projection helpers.
    game.actors.get_mut(&actor).unwrap().discovery = crate::discovery::read_discovery(serde_json::json!([
            {"feature":{"kind":"door","value":77},"attempted":false,"adjacent_attempted":false,"presence":true,"family":false},
            {"feature":{"kind":"trap","value":88},"attempted":false,"adjacent_attempted":false,"presence":true,"family":false}
        ])).unwrap();
    let before_projection = crate::diagnostics::work_counts().observation_cells_copied;
    let known = game.observe(actor).unwrap();
    let copied = crate::diagnostics::work_counts().observation_cells_copied - before_projection;
    let entrance = known
        .visible_cells
        .iter()
        .find(|cell| cell.location == door)
        .unwrap();
    assert_eq!(
        entrance.door,
        Some(tor_world::Door {
            id: 77,
            open: false,
            height: 1
        })
    );
    assert!(entrance.door_secret && entrance.door_reachable && !entrance.wall);
    assert_eq!(
        known
            .visible_cells
            .iter()
            .find(|cell| cell.location == trap)
            .unwrap()
            .trap,
        Some(TrapView {
            id: 88,
            family: None,
            reachable: true
        })
    );
    assert_eq!(
        copied,
        known.visible_cells.len(),
        "door and trap projection detach the raw cell array exactly once"
    );
    assert!(!known.visible_cells.shares_storage(&retained_raw));
    assert_eq!(*retained_raw, expected_raw);
    assert_eq!(*hidden.visible_cells, expected_hidden);
    game.reveal_feature_family(actor, tor_world::ConcealedFeatureId::Trap(88))
        .unwrap();
    assert_eq!(
        game.observe(actor)
            .unwrap()
            .visible_cells
            .iter()
            .find(|cell| cell.location == trap)
            .unwrap()
            .trap,
        Some(TrapView {
            id: 88,
            family: Some(tor_world::TrapFamily::Rust),
            reachable: true
        })
    );
    assert!(world.matches(&game.world));
    assert!(Arc::ptr_eq(
        &scene,
        &game.scene_snapshot_shared(actor).unwrap()
    ));
    assert_eq!(
        crate::diagnostics::work_counts().raw_world_fact_builds,
        before
    );
    game.world.set_door(door, true);
    assert!(
        game.observe(actor)
            .unwrap()
            .visible_cells
            .iter()
            .find(|cell| cell.location == door)
            .unwrap()
            .door
            .unwrap()
            .open
    );
}

#[test]
fn raw_world_facts_never_cache_other_actor_door_clearance() {
    let (mut game, actor) = raw_feature_fixture();
    let door = at(1, 2, 2);
    let owner_at = game.actors[&actor].location;
    game.world.set_wall(at(1, 1, 2), true).unwrap();
    game.world.place_door(door, 77, true, 1).unwrap();
    let initial = game.observe(actor).unwrap();
    let entrance = initial
        .visible_cells
        .iter()
        .find(|cell| cell.location == door)
        .unwrap();
    assert!(entrance.door_reachable && entrance.door_approaches.contains(&owner_at));
    let scene = game.scene_snapshot_shared(actor).unwrap();
    let world = game.world.weak_identity();
    let blocker = game
        .spawn_actor(at(1, 2, 1), NonZeroU64::new(100).unwrap())
        .unwrap();
    let blocked = game.observe(actor).unwrap();
    let entrance = blocked
        .visible_cells
        .iter()
        .find(|cell| cell.location == door)
        .unwrap();
    assert!(!entrance.door_reachable && !entrance.door_approaches.contains(&owner_at));
    assert!(world.matches(&game.world));
    assert!(Arc::ptr_eq(
        &scene,
        &game.scene_snapshot_shared(actor).unwrap()
    ));
    game.actors.get_mut(&blocker).unwrap().location = at(1, 4, 4);
    let reopened = game.observe(actor).unwrap();
    let entrance = reopened
        .visible_cells
        .iter()
        .find(|cell| cell.location == door)
        .unwrap();
    assert!(entrance.door_reachable && entrance.door_approaches.contains(&owner_at));
    assert_eq!(reopened.visible_cells, initial.visible_cells);
}

#[test]
fn warm_observation_reuses_raw_world_facts_but_projects_fresh_entities() {
    let (mut game, actor) = membership_fixture();
    let cold = crate::diagnostics::work_counts();
    let expected = game.observe(actor).unwrap();
    assert!(
        !expected.visible_cells.is_empty(),
        "positive raw projection fixture"
    );
    let before = crate::diagnostics::work_counts();
    assert_eq!(before.raw_world_fact_builds - cold.raw_world_fact_builds, 1);
    assert_eq!(
        before.raw_world_fact_cells - cold.raw_world_fact_cells,
        expected.visible_cells.len()
    );
    assert_eq!(game.observe(actor).unwrap(), expected);
    let after = crate::diagnostics::work_counts();
    assert_eq!(after.observations - before.observations, 1);
    assert_eq!(
        after.perception_projections - before.perception_projections,
        0
    );
    assert_eq!(
        after.raw_world_fact_builds, before.raw_world_fact_builds,
        "an unchanged observation must reuse raw World facts, not rebuild them"
    );
    assert_eq!(after.raw_world_fact_cells, before.raw_world_fact_cells);
    let item = game
        .place_item(
            at(1, 1, 0),
            "item.test.fresh-raw-cache-item".parse().unwrap(),
            "fresh raw-cache item".into(),
        )
        .unwrap();
    let changed = game.observe(actor).unwrap();
    assert!(changed.ground_items.iter().any(|row| row.id == item));
    assert_eq!(changed.visible_cells, expected.visible_cells);
    assert_eq!(
        crate::diagnostics::work_counts().raw_world_fact_builds,
        before.raw_world_fact_builds
    );
}

#[test]
fn warm_observation_shares_unmodified_cell_views_but_refreshes_entities() {
    let (mut game, actor) = raw_feature_fixture();
    let expected = game.observe(actor).unwrap();
    assert!(
        !expected.visible_cells.is_empty(),
        "positive feature-free cell projection fixture"
    );
    assert!(expected
        .visible_cells
        .iter()
        .all(|cell| cell.door.is_none() && cell.trap.is_none()));
    let before = crate::diagnostics::work_counts();
    assert_eq!(game.observe(actor).unwrap(), expected);
    let other = game
        .spawn_actor(at(1, 1, 0), NonZeroU64::new(100).unwrap())
        .unwrap();
    let item = game
        .place_item(
            at(1, 1, 0),
            "item.test.fresh-shared-cell-item".parse().unwrap(),
            "fresh shared-cell item".into(),
        )
        .unwrap();
    let fresh = game.observe(actor).unwrap();
    assert!(expected.visible_actors.iter().all(|row| row.id != other));
    assert!(expected.ground_items.iter().all(|row| row.id != item));
    assert!(fresh.visible_actors.iter().any(|row| row.id == other));
    assert!(fresh.ground_items.iter().any(|row| row.id == item));
    assert_eq!(fresh.visible_cells, expected.visible_cells);
    let after = crate::diagnostics::work_counts();
    assert_eq!(after.observations - before.observations, 2);
    assert_eq!(
        after.perception_projections - before.perception_projections,
        1
    );
    assert_eq!(after.scenes, before.scenes);
    assert_eq!(after.raw_world_fact_builds, before.raw_world_fact_builds);
    assert_eq!(
        after.observation_cells_copied - before.observation_cells_copied,
        0,
        "feature-free observations must not copy unchanged CellView rows"
    );
    assert!(expected.visible_cells.shares_storage(&fresh.visible_cells));

    // An independently retained caller view remains writable without
    // changing either the raw template or another observer snapshot.
    let mut edited = fresh.clone();
    let previous_hint = edited.visible_cells[0].place_hint;
    edited.visible_cells[0].place_hint = !previous_hint;
    assert!(!edited.visible_cells.shares_storage(&fresh.visible_cells));
    assert_eq!(fresh.visible_cells, expected.visible_cells);
    assert_ne!(edited.visible_cells, expected.visible_cells);
    assert_eq!(game.observe(actor).unwrap(), fresh);

    // Projection-owned mutations count the first detachment exactly once.
    let mut projected = fresh.visible_cells.clone();
    let before_detach = crate::diagnostics::work_counts().observation_cells_copied;
    observation_cells_mut(&mut projected)[0].place_hint = !previous_hint;
    let detached = crate::diagnostics::work_counts().observation_cells_copied;
    assert_eq!(detached - before_detach, fresh.visible_cells.len());
    observation_cells_mut(&mut projected)[0].place_hint = previous_hint;
    assert_eq!(
        crate::diagnostics::work_counts().observation_cells_copied,
        detached,
        "further mutations of the unique projection copy no rows"
    );
    assert_eq!(projected, expected.visible_cells);
    assert_eq!(fresh.visible_cells, expected.visible_cells);
}

#[test]
fn decision_reuses_witnessed_geometry_but_refreshes_disclosure() {
    let (mut game, actor) = membership_fixture();
    let expected = game.observe(actor).unwrap();
    assert!(!expected.visible_cells.is_empty());
    let snapshot = game.scene_snapshot_shared(actor).unwrap();
    let before = crate::diagnostics::work_counts();
    let decision = game.decision_snapshot(actor).unwrap();
    assert_eq!(decision.view, expected);
    assert_eq!(*decision.sight, *snapshot.sight);
    let item = game
        .place_item(
            at(1, 1, 0),
            "item.test.fresh-decision-item".parse().unwrap(),
            "fresh decision item".into(),
        )
        .unwrap();
    let fresh = game.decision_snapshot(actor).unwrap();
    assert!(fresh.view.ground_items.iter().any(|row| row.id == item));
    assert_eq!(fresh.view.visible_cells, expected.visible_cells);
    let after = crate::diagnostics::work_counts();
    assert_eq!(
        after.perception_projections - before.perception_projections,
        0
    );
    assert_eq!(
        after.scenes, before.scenes,
        "warm AI decisions must not reconstruct identical witnessed geometry"
    );
    assert_eq!(after.raw_world_fact_builds, before.raw_world_fact_builds);
    assert!(Arc::ptr_eq(&snapshot.sight, &fresh.sight));
}

#[test]
fn warm_observation_reuses_its_geometry_projection_indexes() {
    let (game, actor) = membership_fixture();
    let expected = game.observe(actor).unwrap();
    let before = crate::diagnostics::work_counts();
    assert_eq!(game.observe(actor).unwrap(), expected);
    let after = crate::diagnostics::work_counts();
    assert_eq!(after.observations - before.observations, 1);
    assert_eq!(
        after.perception_projections - before.perception_projections,
        0
    );
    assert_eq!(
        after.projection_location_indexes, before.projection_location_indexes,
        "unchanged scene and eye indexes are already owned by the cached projection"
    );
    assert_eq!(
        after.projection_location_cells,
        before.projection_location_cells
    );
    assert_eq!(
        after.scene_cells_copied, before.scene_cells_copied,
        "after complete accounting, fresh disclosure must borrow unchanged geometry"
    );
}

#[test]
fn scene_cache_copy_counter_includes_both_arrays_at_install() {
    let (game, actor) = membership_fixture();
    let mut expected_sight = Vec::new();
    let expected_scene = game
        .compute_scene(actor, &mut |cells: &[tor_world::SightCell]| {
            expected_sight.extend_from_slice(cells)
        })
        .unwrap();
    assert!(!expected_scene.is_empty());
    assert!(!expected_sight.is_empty());
    game.lifecycle.scenes.0.lock().unwrap().clear();
    let before = crate::diagnostics::work_counts().scene_cells_copied;
    let (scene, sight) = game.scene_snapshot(actor).unwrap();
    assert_eq!(scene, expected_scene);
    assert_eq!(sight, expected_sight);
    assert_eq!(
        crate::diagnostics::work_counts().scene_cells_copied - before,
        expected_scene.len() + expected_sight.len(),
        "cache installation clones both independently owned arrays"
    );
}

#[test]
fn scene_cache_copy_counter_includes_warm_snapshot_arrays() {
    let (game, actor) = membership_fixture();
    let (expected_scene, expected_sight) = game.scene_snapshot(actor).unwrap();
    assert!(!expected_scene.is_empty());
    assert!(!expected_sight.is_empty());
    let before = crate::diagnostics::work_counts();
    let (scene, sight) = game.scene_snapshot(actor).unwrap();
    assert_eq!(scene, expected_scene);
    assert_eq!(sight, expected_sight);
    let warm = crate::diagnostics::work_counts();
    assert_eq!(warm.scenes, before.scenes);
    assert_eq!(
        warm.scene_cells_copied - before.scene_cells_copied,
        expected_scene.len() + expected_sight.len(),
        "a warm full snapshot clones both cached arrays without building a scene"
    );
    assert_eq!(game.streaming_scene(actor).unwrap(), expected_scene);
    let streamed = crate::diagnostics::work_counts();
    assert_eq!(streamed.scenes, warm.scenes);
    assert_eq!(
        streamed.scene_cells_copied - warm.scene_cells_copied,
        expected_scene.len(),
        "streaming clones only disclosed cells, not the unused sight array"
    );
}

#[test]
fn scene_build_counters_cover_cold_snapshots_placements_and_decisions() {
    let (game, actor) = membership_fixture();
    let before = crate::diagnostics::work_counts();
    game.scene_snapshot(actor).unwrap();
    let cold = crate::diagnostics::work_counts();
    assert_eq!(cold.scenes - before.scenes, 1);
    assert_eq!(cold.ordinary_scenes - before.ordinary_scenes, 1);
    assert_eq!(cold.decision_scenes, before.decision_scenes);
    game.scene_snapshot(actor).unwrap();
    let warm = crate::diagnostics::work_counts();
    assert_eq!(warm.scenes, cold.scenes);
    assert_eq!(warm.ordinary_scenes, cold.ordinary_scenes);
    game.scene_snapshot_at_placement(
        actor,
        crate::abilities::Placement {
            location: at(1, 1, 0),
            orientation: 0,
        },
    )
    .unwrap();
    let placed = crate::diagnostics::work_counts();
    assert_eq!(placed.scenes - warm.scenes, 1);
    assert_eq!(placed.ordinary_scenes - warm.ordinary_scenes, 1);
    game.lifecycle.scenes.0.lock().unwrap().clear();
    game.decision_snapshot(actor).unwrap();
    let decided = crate::diagnostics::work_counts();
    assert_eq!(decided.scenes - placed.scenes, 1);
    assert_eq!(decided.decision_scenes - placed.decision_scenes, 1);
    assert_eq!(decided.ordinary_scenes, placed.ordinary_scenes);
    game.decision_snapshot(actor).unwrap();
    let reused = crate::diagnostics::work_counts();
    assert_eq!(reused.scenes, decided.scenes);
    assert_eq!(reused.decision_scenes, decided.decision_scenes);
    assert_eq!(reused.ordinary_scenes, decided.ordinary_scenes);
    assert_eq!(
        decided.scenes - before.scenes,
        decided.ordinary_scenes - before.ordinary_scenes + decided.decision_scenes
            - before.decision_scenes
    );
}

#[test]
fn phase5_scene_membership_warm_ground_fragments_do_not_copy_or_scan_the_scene() {
    let (mut game, actor) = membership_fixture();
    for id in 10..42 {
        game.place_item_stack(
            id,
            at(1, 1, 0),
            None,
            1,
            crate::ItemSpec::ordinary(
                format!("item.test.fragment-{id}").parse().unwrap(),
                format!("fragment {id}"),
            ),
        )
        .unwrap();
    }
    assert!(game.scene_contains(actor, at(1, 1, 0)).unwrap());
    let before = crate::diagnostics::work_counts();
    for id in 10..42 {
        assert!(game.supply_access(
            actor,
            &game.items[&ItemId(id)],
            &crate::rules::SupplyAccess::CarriedOrGroundTouch
        ));
    }
    let after = crate::diagnostics::work_counts();
    assert_eq!(
        after.scene_membership_lookups - before.scene_membership_lookups,
        32
    );
    assert_eq!(after.scenes, before.scenes);
    assert_eq!(
        after.scene_cells_copied, before.scene_cells_copied,
        "warm fragment checks must not clone the disclosed scene"
    );
    assert_eq!(
        after.scene_membership_cells, before.scene_membership_cells,
        "warm fragment checks must not scan the disclosed scene"
    );
}

#[test]
fn phase5_scene_membership_cold_index_is_built_once_and_is_read_only() {
    let (game, actor) = membership_fixture();
    let expected = game.compute_scene(actor, &mut |_| {}).unwrap();
    game.lifecycle.scenes.0.lock().unwrap().clear();
    let original = game.clone();
    let before = crate::diagnostics::work_counts();
    assert!(!game.scene_contains(actor, at(99, 0, 0)).unwrap());
    let cold = crate::diagnostics::work_counts();
    assert_eq!(cold.scenes - before.scenes, 1);
    assert_eq!(
        cold.scene_membership_cells - before.scene_membership_cells,
        expected.len()
    );
    for _ in 0..32 {
        assert!(!game.scene_contains(actor, at(99, 0, 0)).unwrap());
    }
    let warm = crate::diagnostics::work_counts();
    assert_eq!(warm.scenes, cold.scenes);
    assert_eq!(warm.scene_membership_cells, cold.scene_membership_cells);
    assert_eq!(warm.scene_cells_copied, cold.scene_cells_copied);
    assert_eq!(game, original);
}

fn check_membership(game: &Game, actor: ActorId) {
    let scene = game.compute_scene(actor, &mut |_| {}).unwrap();
    for location in
        scene
            .iter()
            .map(|cell| cell.location)
            .chain([at(1, 2, 0), at(2, 0, 1), at(99, 0, 0)])
    {
        assert_eq!(
            game.scene_contains(actor, location).unwrap(),
            scene.iter().any(|cell| cell.location == location)
        );
    }
}

#[test]
fn phase5_scene_membership_tracks_witness_changes_and_candidate_rollback() {
    let (mut game, actor) = membership_fixture();
    crate::test_creatures::configure(
        &mut game,
        actor,
        "neutral",
        crate::test_creatures::species(),
    );
    check_membership(&game, actor);
    let mut candidate = game.clone();
    candidate.set_cell_light(at(1, 2, 0), false).unwrap();
    check_membership(&candidate, actor);
    check_membership(&game, actor);
    candidate.set_wall(at(1, 1, 0), true).unwrap();
    check_membership(&candidate, actor);
    candidate.set_wall(at(1, 1, 0), false).unwrap();
    candidate.teleport(actor, at(2, 0, 1)).unwrap();
    check_membership(&candidate, actor);
    candidate.actors.get_mut(&actor).unwrap().orientation = 1;
    check_membership(&candidate, actor);
    candidate.actors.get_mut(&actor).unwrap().orientation = 0;
    candidate.teleport(actor, at(1, 0, 0)).unwrap();
    candidate
        .set_body(
            actor,
            crate::BodySpec {
                cells: vec![[0, 0, 0], [1, 0, 0]],
                eye: [1, 0, 0],
                mass: 80,
            },
        )
        .unwrap();
    check_membership(&candidate, actor);
    candidate
        .apply_damage(
            actor,
            &crate::damage::DamageSpec::new(
                vec![crate::damage::DamageComponent::fixed(
                    crate::combat::DamageType::Impact,
                    None,
                    1000,
                )],
                None,
            )
            .unwrap(),
        )
        .unwrap();
    assert!(!candidate.alive(actor));
    check_membership(&candidate, actor);
    candidate.combat.selected = Some(actor);
    check_membership(&candidate, actor);
    candidate.combat.selected = None;
    check_membership(&candidate, actor);
    check_membership(&game, actor);
    // Eviction cannot turn the derived index into authority.
    game.lifecycle.scenes.0.lock().unwrap().clear();
    check_membership(&game, actor);
}

#[test]
fn phase5_scene_membership_deduplicates_occurrences_without_granting_stair_touch() {
    let (mut game, actor) = membership_fixture();
    let landing = at(2, 0, 1);
    game.register_named_anchors(
        RegionId(2),
        std::collections::BTreeMap::from([("landing".into(), landing.position)]),
    )
    .unwrap();
    game.connect_named_stair(
        at(1, 0, 0),
        Direction::Up,
        tor_world::NamedAnchor {
            region: RegionId(2),
            name: "landing".into(),
        },
    )
    .unwrap();
    let scene = game.scene(actor).unwrap();
    assert!(scene.iter().any(|cell| cell.location == landing));
    assert!(game.scene_contains(actor, landing).unwrap());
    assert!(!game.touches_supply(actor, landing));
    let item = game
        .place_item_stack(
            10,
            landing,
            None,
            1,
            crate::ItemSpec::ordinary(
                "item.test.stair-supply".parse().unwrap(),
                "stair supply".into(),
            ),
        )
        .unwrap();
    assert!(!game.supply_access(
        actor,
        &game.items[&item],
        &crate::rules::SupplyAccess::CarriedOrGroundTouch
    ));
    // Cache membership depends on physical locations, never occurrence offsets.
    // Explicitly exercise duplicate projected occurrences at distinct offsets.
    let mut repeated = scene.clone();
    let mut duplicate = repeated[0];
    duplicate.offset.x += 100;
    repeated.push(duplicate);
    game.cache_scene(actor, &repeated, &[]).unwrap();
    for cell in &repeated {
        assert!(game.scene_contains(actor, cell.location).unwrap());
    }
    assert_eq!(game.streaming_scene(actor).unwrap(), repeated);
}

#[test]
fn phase5_scene_membership_index_respects_cache_capacity_and_eviction() {
    let (game, actor) = membership_fixture();
    assert!(game.scene_contains(actor, at(1, 0, 0)).unwrap());
    let cached = game.lifecycle.scenes.0.lock().unwrap()[&actor].clone();
    assert!(cached.locations.get().is_some());
    // Fill only this derived cache to its bound; no actors or claims are
    // synthesized in authoritative state.
    {
        let mut cache = game.lifecycle.scenes.0.lock().unwrap();
        cache.clear();
        for id in 2..=513 {
            cache.insert(ActorId(id), cached.clone());
        }
    }
    let original = game.clone();
    let before = crate::diagnostics::work_counts();
    assert!(game.scene_contains(actor, at(1, 0, 0)).unwrap());
    assert_eq!(crate::diagnostics::work_counts().scenes, before.scenes + 1);
    {
        let cache = game.lifecycle.scenes.0.lock().unwrap();
        assert_eq!(cache.len(), 512);
        assert!(cache.contains_key(&actor));
        assert!(!cache.contains_key(&ActorId(2)));
    }
    check_membership(&game, actor);
    assert_eq!(game, original);
}

#[test]
fn observation_reuses_the_exact_scene_already_resolved_for_streaming() {
    let mut game = Game::two_room_in_stone(42, crate::test_creatures::content());
    let actor = game
        .spawn_actor(at(1, 0, 0), NonZeroU64::new(100).unwrap())
        .unwrap();
    let scene = game.streaming_scene(actor).unwrap();
    let before = crate::diagnostics::work_counts();
    assert_eq!(game.observe_scene(actor).unwrap().1, scene);
    let after = crate::diagnostics::work_counts();
    assert_eq!(
        after.scenes, before.scenes,
        "unchanged perception was projected twice"
    );
    assert_eq!(after.observations, before.observations + 1);
    let mut changed = game.clone();
    changed.set_cell_light(at(1, 2, 0), false).unwrap();
    let dark = changed.observe_scene(actor).unwrap().1;
    assert_ne!(dark, scene);
    assert_eq!(dark, changed.compute_scene(actor, &mut |_| {}).unwrap());
    assert_eq!(game.observe_scene(actor).unwrap().1, scene);
}

#[test]
fn observation_scene_cache_tracks_actor_chart_body_and_death_disclosure() {
    let mut game = Game::two_room_in_stone(42, crate::test_creatures::content());
    let actor = game
        .spawn_actor(at(1, 0, 0), NonZeroU64::new(100).unwrap())
        .unwrap();
    crate::test_creatures::configure(
        &mut game,
        actor,
        "neutral",
        crate::test_creatures::species(),
    );
    let check = |game: &Game| {
        let mut sight = Vec::new();
        let expected = game
            .compute_scene(actor, &mut |cells| sight.extend_from_slice(cells))
            .unwrap();
        let decision = game.decision_snapshot(actor).unwrap();
        assert_eq!(*decision.sight, sight);
        assert_eq!(decision.view, game.observe(actor).unwrap());
        assert_eq!(game.observe_scene(actor).unwrap().1, expected);
    };
    check(&game);
    game.teleport(actor, at(2, 0, 1)).unwrap();
    check(&game);
    game.actors.get_mut(&actor).unwrap().orientation = 1;
    check(&game);
    game.actors.get_mut(&actor).unwrap().orientation = 0;
    game.teleport(actor, at(1, 0, 0)).unwrap();
    game.set_body(
        actor,
        crate::BodySpec {
            cells: vec![[0, 0, 0], [1, 0, 0]],
            eye: [1, 0, 0],
            mass: 80,
        },
    )
    .unwrap();
    check(&game);
    game.apply_damage(
        actor,
        &crate::damage::DamageSpec::new(
            vec![crate::damage::DamageComponent::fixed(
                crate::combat::DamageType::Impact,
                None,
                1000,
            )],
            None,
        )
        .unwrap(),
    )
    .unwrap();
    assert!(!game.alive(actor));
    assert!(game.observe_scene(actor).unwrap().1.is_empty());
    check(&game);
    game.combat.selected = Some(actor);
    assert!(!game.observe_scene(actor).unwrap().1.is_empty());
    check(&game);
    game.combat.selected = None;
    check(&game);
}

#[test]
fn streaming_reuses_only_scenes_with_identical_perception_inputs() {
    let mut game = Game::two_room_in_stone(42, crate::test_creatures::content());
    let actor = game
        .spawn_actor(at(1, 0, 0), NonZeroU64::new(100).unwrap())
        .unwrap();
    let initial = game.scene(actor).unwrap();
    let calls = crate::diagnostics::work_counts().scenes;
    assert_eq!(game.streaming_scene(actor).unwrap(), initial);
    assert_eq!(crate::diagnostics::work_counts().scenes, calls);
    let mut candidate = game.clone();
    let target = at(1, 2, 0);
    assert!(initial.iter().any(|cell| cell.location == target));
    candidate.set_cell_light(target, false).unwrap();
    let dark = candidate.streaming_scene(actor).unwrap();
    assert!(!dark.iter().any(|cell| cell.location == target));
    assert_eq!(crate::diagnostics::work_counts().scenes, calls + 1);
    assert_eq!(game.streaming_scene(actor).unwrap(), initial);
    candidate.set_cell_light(target, true).unwrap();
    candidate.set_wall(at(1, 1, 0), true).unwrap();
    assert!(!candidate
        .streaming_scene(actor)
        .unwrap()
        .iter()
        .any(|cell| cell.location == target));
    candidate.set_wall(at(1, 1, 0), false).unwrap();
    candidate
        .set_body(
            actor,
            crate::BodySpec {
                cells: vec![[0, 0, 0], [1, 0, 0]],
                eye: [0, 0, 0],
                mass: 80,
            },
        )
        .unwrap();
    candidate.set_cell_light(target, false).unwrap();
    // Extending the body puts the dark target in its immediate neighbourhood.
    assert!(candidate
        .streaming_scene(actor)
        .unwrap()
        .iter()
        .any(|cell| cell.location == target));
    assert_eq!(
        candidate.streaming_scene(actor).unwrap(),
        candidate.compute_scene(actor, &mut |_| {}).unwrap()
    );
}

#[test]
fn door_approaches_ignore_only_cells_away_from_the_door() {
    let (mut compared, mut found) = (0, 0);
    for seed in 1..=24u64 {
        let mut state = 0x9E37_79B9_7F4A_7C15 ^ seed;
        let mut below = |n: u64| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state % n
        };
        let rooms = (1..=2)
            .map(|id| Region {
                id: RegionId(id),
                name: String::new(),
                bounds: Extent::new(7, 7, 1).unwrap(),
            })
            .collect();
        let mut world = World::new(rooms, vec![]).unwrap();
        // A straight join and a rotated one, as in the latency fixture.
        for (from, direction, to, back, turns) in [
            (
                at(1, 6, 3),
                Direction::East,
                at(2, 0, 3),
                Direction::West,
                0,
            ),
            (
                at(1, 3, 0),
                Direction::North,
                at(2, 0, 5),
                Direction::West,
                1,
            ),
        ] {
            world
                .connect(
                    Passage {
                        from,
                        direction,
                        to,
                    },
                    turns,
                )
                .unwrap();
            world
                .connect(
                    Passage {
                        from: to,
                        direction: back,
                        to: from,
                    },
                    (4 - turns) % 4,
                )
                .unwrap();
        }
        let cell =
            |below: &mut dyn FnMut(u64) -> u64| at(1 + below(2), below(7) as i32, below(7) as i32);
        for _ in 0..12 {
            let _ = world.set_wall(cell(&mut below), true);
        }
        let mut game = Game::new(world, seed, crate::test_creatures::content());
        for _ in 0..8 {
            let _ = game.place_door(cell(&mut below), below(2) == 0, 1);
        }
        for _ in 0..4 {
            let _ = game.spawn_actor(cell(&mut below), NonZeroU64::new(100).unwrap());
        }
        for _ in 0..16 {
            let eye = cell(&mut below);
            let scene = game.world.eye_scene(eye, below(4) as u8, SIGHT_RANGE);
            let doors: BTreeSet<_> = scene
                .iter()
                .map(|c| c.location)
                .filter(|&at| game.world.door(at).is_some())
                .collect();
            for door in doors {
                let approaches = game.door_approaches_among(&scene, door, true);
                assert_eq!(
                    approaches,
                    game.door_approaches_among(&scene, door, false),
                    "seed {seed}: eye {eye:?}, door {door:?}"
                );
                compared += 1;
                found += approaches.len();
            }
        }
    }
    assert!(
        compared > 200 && found > 200,
        "{compared} doors, {found} approaches"
    );
}
