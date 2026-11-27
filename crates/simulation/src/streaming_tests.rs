use super::*;
use crate::Action;
use std::num::NonZeroU64;
use tor_world::Position;

include!("streaming_retrieval_tests.rs");

#[test]
fn checkpoint_rejects_a_token_reservation_with_only_one_loaded_partner() {
    use crate::content_ids::{AbilityId, ConditionId};
    for detach_owner in [false, true] {
        let mut game = Game::region_corridor(42, 3, crate::test_creatures::content());
        let mut actors = Vec::new();
        for (region, x) in [(1, 1), (2, 11), (3, 0)] {
            let actor = game
                .spawn_actor(
                    Location {
                        region: RegionId(region),
                        position: Position { x, y: 1, z: 0 },
                    },
                    NonZeroU64::new(100).unwrap(),
                )
                .unwrap();
            crate::test_creatures::configure(
                &mut game,
                actor,
                "neutral",
                crate::test_creatures::species(),
            );
            actors.push(actor);
        }
        let (owner, holder) = (actors[1], actors[2]);
        game.apply_condition(holder, &ConditionId::new("exposed").unwrap(), holder, 500)
            .unwrap();
        let plan = game
            .ability_plan(owner, &AbilityId::new("basic_melee").unwrap(), holder)
            .unwrap();
        game.start_work(plan, None).unwrap();
        // Bypass the public transition's pins to model a malformed save.
        let region = RegionId(if detach_owner { 2 } else { 3 });
        game.freeze_region(region).unwrap();
        game.detach_record(region).unwrap();
        assert!(game.lifecycle_state_valid());
        let mut shared = crate::checkpoint::SharedState::default();
        let snapshot = game.checkpoint(&mut shared);
        assert!(matches!(
            Game::restore_checkpoint(snapshot, &shared, game.content().clone()),
            Err(crate::checkpoint::PersistError::RunState)
        ));
    }
}

#[test]
fn phase5_checkpoint_rejects_a_supply_hold_with_missing_item() {
    checkpoint_with_one_supply_endpoint(false);
}
#[test]
fn phase5_checkpoint_rejects_a_supply_hold_with_missing_owner() {
    checkpoint_with_one_supply_endpoint(true);
}
fn checkpoint_with_one_supply_endpoint(detach_owner: bool) {
    use crate::supplies::{SelectedSupply, SupplySelection};
    let mut game = Game::region_corridor(42, 3, crate::test_creatures::content());
    let owner = game
        .spawn_actor(
            Location {
                region: RegionId(2),
                position: Position { x: 11, y: 1, z: 0 },
            },
            NonZeroU64::new(100).unwrap(),
        )
        .unwrap();
    crate::test_creatures::configure(
        &mut game,
        owner,
        "neutral",
        crate::test_creatures::species(),
    );
    game.spawn_actor(
        Location {
            region: RegionId(1),
            position: Position { x: 1, y: 1, z: 0 },
        },
        NonZeroU64::new(100).unwrap(),
    )
    .unwrap();
    let mut bandage =
        crate::ItemSpec::ordinary("item.test.bandage".parse().unwrap(), "bandage".into());
    bandage.supply = Some(crate::content_ids::SupplyId::new("bandage").unwrap());
    let item = game
        .place_item_stack(
            10,
            Location {
                region: RegionId(3),
                position: Position { x: 0, y: 1, z: 0 },
            },
            None,
            1,
            bandage,
        )
        .unwrap();
    game.admit_intention(
        owner,
        Action::UseAbility {
            id: crate::content_ids::AbilityId::new("bind_wounds").unwrap(),
            invocation: crate::abilities::AbilityInvocation::actor_target(owner),
            supplies: SupplySelection::Explicit {
                items: vec![SelectedSupply {
                    cost_ordinal: 0,
                    item,
                }],
            },
        },
        crate::IntentionOrigin::Human,
    )
    .unwrap();
    game.execute_next_intention().unwrap().outcome.unwrap();
    let region = RegionId(if detach_owner { 2 } else { 3 });
    // Deliberately bypass public reciprocal pins to model corrupt stored state.
    game.freeze_region(region).unwrap();
    game.detach_record(region).unwrap();
    assert!(game.lifecycle_state_valid());
    let mut shared = crate::checkpoint::SharedState::default();
    let snapshot = game.checkpoint(&mut shared);
    assert!(
        Game::restore_checkpoint(snapshot, &shared, game.content().clone()).is_err(),
        "accepted a supply with only one loaded partner; detach_owner={detach_owner}"
    );
}

#[test]
fn phase5_attach_rejects_duplicate_issued_supply_origins() {
    let mut game = Game::region_corridor(42, 3, crate::test_creatures::content());
    let mut origins = Vec::new();
    let mut holders = Vec::new();
    for region in [2, 3] {
        let at = Location {
            region: RegionId(region),
            position: Position { x: 5, y: 1, z: 0 },
        };
        let actor = game.spawn_actor(at, NonZeroU64::new(1).unwrap()).unwrap();
        crate::test_creatures::configure(
            &mut game,
            actor,
            "neutral",
            crate::test_creatures::species(),
        );
        holders.push((region, at, actor));
    }
    game.spawn_actor(
        Location {
            region: RegionId(1),
            position: Position { x: 1, y: 1, z: 0 },
        },
        NonZeroU64::new(1).unwrap(),
    )
    .unwrap();
    for (region, at, actor) in holders {
        let mut bandage =
            crate::ItemSpec::ordinary("item.test.bandage".parse().unwrap(), "bandage".into());
        bandage.supply = Some(crate::content_ids::SupplyId::new("bandage").unwrap());
        game.place_item_stack(10 + region, at, Some(actor), 1, bandage)
            .unwrap();
        game.admit_intention(
            actor,
            Action::UseAbility {
                id: crate::content_ids::AbilityId::new("bind_wounds").unwrap(),
                invocation: crate::abilities::AbilityInvocation::actor_target(actor),
                supplies: crate::supplies::SupplySelection::Carried,
            },
            crate::IntentionOrigin::Human,
        )
        .unwrap();
        game.execute_next_intention().unwrap().outcome.unwrap();
        origins.push(game.preparation(actor).unwrap().origin.unwrap());
    }
    let mut records = MemoryRecords::default();
    for region in [2, 3] {
        game.freeze_region(RegionId(region)).unwrap();
        let (id, record) = game.detach_record(RegionId(region)).unwrap();
        let mut record = (*record).clone();
        if region == 3 {
            let pending = record
                .actors
                .values_mut()
                .next()
                .unwrap()
                .pending
                .as_mut()
                .unwrap();
            pending.origin = Some(origins[0]);
            for item in record.items.values_mut() {
                for hold in &mut item.holds {
                    hold.key.lineage.origin = origins[0];
                }
            }
        }
        records.put(id, Shared::new(record));
    }
    let before = game.clone();
    assert!(
        game.transition_regions(&sets(&[], &[1, 2, 3]), &mut records)
            .is_err(),
        "completed attach accepted duplicate preparation origins"
    );
    assert_eq!(game, before);
}

#[test]
fn phase5_supply_actor_first_arrival_preserves_frozen_reciprocal_claims() {
    supply_arrival_order(false);
}
#[test]
fn phase5_supply_item_first_arrival_pins_the_owner_active() {
    supply_arrival_order(true);
}
fn supply_arrival_order(item_first: bool) {
    let mut game = Game::region_corridor(42, 3, crate::test_creatures::content());
    let owner_region = RegionId(if item_first { 3 } else { 2 });
    let item_region = RegionId(if item_first { 2 } else { 3 });
    let at = Location {
        region: owner_region,
        position: Position {
            x: if item_first { 0 } else { 11 },
            y: 1,
            z: 0,
        },
    };
    let item_at = Location {
        region: item_region,
        position: Position {
            x: if item_first { 11 } else { 0 },
            y: 1,
            z: 0,
        },
    };
    let owner = game.spawn_actor(at, NonZeroU64::new(100).unwrap()).unwrap();
    crate::test_creatures::configure(
        &mut game,
        owner,
        "neutral",
        crate::test_creatures::species(),
    );
    game.spawn_actor(
        Location {
            region: RegionId(1),
            position: Position { x: 1, y: 1, z: 0 },
        },
        NonZeroU64::new(100).unwrap(),
    )
    .unwrap();
    let mut spec =
        crate::ItemSpec::ordinary("item.test.bandage".parse().unwrap(), "bandage".into());
    spec.supply = Some(crate::content_ids::SupplyId::new("bandage").unwrap());
    let item = game.place_item_stack(10, item_at, None, 1, spec).unwrap();
    game.admit_intention(
        owner,
        Action::UseAbility {
            id: crate::content_ids::AbilityId::new("bind_wounds").unwrap(),
            invocation: crate::abilities::AbilityInvocation::actor_target(owner),
            supplies: crate::supplies::SupplySelection::Explicit {
                items: vec![crate::supplies::SelectedSupply {
                    cost_ordinal: 0,
                    item,
                }],
            },
        },
        crate::IntentionOrigin::Human,
    )
    .unwrap();
    game.execute_next_intention().unwrap().outcome.unwrap();
    let pending = game.preparation(owner).unwrap().clone();
    let mut records = MemoryRecords::default();
    game.transition_regions(&sets(&[], &[]), &mut records)
        .unwrap();
    game.transition_regions(&sets(&[], &[2, 3]), &mut records)
        .unwrap();
    assert_eq!(game.region_state(owner_region), Some(RegionState::Frozen));
    assert_eq!(game.region_state(item_region), Some(RegionState::Frozen));
    assert_eq!(game.preparation(owner), Some(&pending));
    let mut shared = crate::checkpoint::SharedState::default();
    let snapshot = game.checkpoint(&mut shared);
    let restored = Game::restore_checkpoint(snapshot, &shared, game.content().clone()).unwrap();
    assert_eq!(restored, game);
    game.add_reference_point(ReferencePoint {
        target: ReferenceTarget::Item(item),
        active_radius: Some(0),
        load_radius: Some(0),
        observes: false,
    })
    .unwrap();
    game.transition_regions(&sets(&[item_region.0], &[item_region.0]), &mut records)
        .unwrap();
    assert_eq!(game.region_state(item_region), Some(RegionState::Active));
    assert_eq!(
        game.region_state(owner_region),
        Some(RegionState::Active),
        "item-only activity must pin its claimant active"
    );
    assert_eq!(game.preparation(owner), Some(&pending));
    assert_eq!(game.items[&item].quantity, 1);
}

struct UnavailableSource;
impl RecordStore for UnavailableSource {
    fn can_prepare(&self, _: RegionId) -> bool {
        false
    }
    fn put(&mut self, _: RecordId, _: Shared<RegionRecord>) {}
    fn get(&mut self, _: RecordId) -> Option<Shared<RegionRecord>> {
        None
    }
}

fn unresolved_stair_game() -> (Game, ActorId) {
    let mut game = Game::new(
        tor_world::World::new(vec![], vec![]).unwrap(),
        1,
        crate::test_creatures::content(),
    );
    let region = |id| Region {
        id: RegionId(id),
        name: "chamber".into(),
        bounds: tor_world::Extent::new(3, 3, 2).unwrap(),
    };
    game.add_chamber(region(1)).unwrap();
    game.add_unbuilt_region(region(2), true, RegionIdentities::default())
        .unwrap();
    let at = Location {
        region: RegionId(1),
        position: Position { x: 1, y: 1, z: 0 },
    };
    let actor = game.spawn_actor(at, NonZeroU64::new(100).unwrap()).unwrap();
    game.connect_named_stair(
        at,
        Direction::Down,
        tor_world::NamedAnchor {
            region: RegionId(2),
            name: "up".into(),
        },
    )
    .unwrap();
    game.add_reference_point(ReferencePoint {
        target: ReferenceTarget::Actor(actor),
        active_radius: Some(0),
        load_radius: Some(0),
        observes: true,
    })
    .unwrap();
    (game, actor)
}

#[test]
fn unavailable_unresolved_stair_is_deferred_without_erasing_topology() {
    let (mut game, actor) = unresolved_stair_game();
    let proposed = sets(&[1], &[1]);
    assert!(game
        .settle_counted(&proposed)
        .0
        .loaded
        .contains(&RegionId(2)));
    let mut source = UnavailableSource;
    let (settled, _) = game.settle_counted_from_source(&proposed, &source);
    assert_eq!(settled, proposed);
    game.transition_regions(&proposed, &mut source).unwrap();
    let at = game.actors[&actor].location;
    assert!(game.world.is_stair(at, Direction::Down));
    let before = game.clone();
    assert!(game.act(actor, Action::Move(Direction::Down)).is_err());
    assert_eq!(game, before);
    game.act(actor, Action::Wait).unwrap();
    game.act(actor, Action::Move(Direction::West)).unwrap();
}

#[test]
fn unavailable_source_cannot_relax_resolved_or_explicit_destination_pins() {
    let (mut game, _) = unresolved_stair_game();
    let mut source = UnavailableSource;
    let requested = sets(&[1, 2], &[1, 2]);
    assert!(game
        .settle_counted_from_source(&requested, &source)
        .0
        .active
        .contains(&RegionId(2)));
    let before = game.clone();
    assert!(game.transition_regions(&requested, &mut source).is_err());
    assert_eq!(game, before);
    game.register_named_anchors(
        RegionId(2),
        BTreeMap::from([("up".into(), Position { x: 1, y: 1, z: 0 })]),
    )
    .unwrap();
    let proposed = sets(&[1], &[1]);
    assert!(game
        .settle_counted_from_source(&proposed, &source)
        .0
        .loaded
        .contains(&RegionId(2)));
    let before = game.clone();
    assert!(game.transition_regions(&proposed, &mut source).is_err());
    assert_eq!(game, before);
}

#[test]
fn unavailable_source_cannot_relax_physical_link_pins() {
    let mut game = Game::region_corridor(1, 2, crate::test_creatures::content());
    let _record = game.detach_record(RegionId(2)).unwrap();
    let mut source = UnavailableSource;
    let proposed = sets(&[1], &[1]);
    assert!(game
        .settle_counted_from_source(&proposed, &source)
        .0
        .loaded
        .contains(&RegionId(2)));
    let before = game.clone();
    assert!(game.transition_regions(&proposed, &mut source).is_err());
    assert_eq!(game, before);
}

#[test]
fn attaching_an_observer_batches_all_newly_discovered_pins() {
    struct Reads {
        records: MemoryRecords,
        counts: BTreeMap<RecordId, usize>,
    }
    impl RecordStore for Reads {
        fn put(&mut self, id: RecordId, record: Shared<RegionRecord>) {
            self.records.put(id, record);
        }
        fn get(&mut self, id: RecordId) -> Option<Shared<RegionRecord>> {
            *self.counts.entry(id).or_default() += 1;
            self.records.get(id)
        }
    }
    let mut game = Game::region_corridor(1, 7, crate::test_creatures::content());
    let actor = game
        .spawn_actor(
            Location {
                region: RegionId(4),
                position: Position { x: 6, y: 1, z: 0 },
            },
            NonZeroU64::new(100).unwrap(),
        )
        .unwrap();
    let mut store = Reads {
        records: MemoryRecords::default(),
        counts: BTreeMap::new(),
    };
    game.transition_regions(&sets(&[], &[]), &mut store)
        .unwrap();
    let observer_record = game
        .detached_records()
        .find(|(r, _)| *r == RegionId(4))
        .unwrap()
        .1;
    game.add_reference_point(ReferencePoint {
        target: ReferenceTarget::Actor(actor),
        active_radius: Some(0),
        load_radius: Some(0),
        observes: true,
    })
    .unwrap();
    let (sets, _) = game
        .transition_regions(&sets(&[4], &[4]), &mut store)
        .unwrap();
    assert_eq!(
        sets.active,
        BTreeSet::from([RegionId(3), RegionId(4), RegionId(5)])
    );
    assert_eq!(
        sets.loaded,
        BTreeSet::from([
            RegionId(2),
            RegionId(3),
            RegionId(4),
            RegionId(5),
            RegionId(6)
        ])
    );
    assert!(
        store.counts[&observer_record] <= 3,
        "observer reread {} times",
        store.counts[&observer_record]
    );
    assert!(game.lifecycle_state_valid());
}

#[test]
fn thawing_shifts_ai_memory_so_it_cannot_expire_while_frozen() {
    let at = |region, x| Location {
        region: RegionId(region),
        position: Position { x, y: 1, z: 0 },
    };
    let mut species = crate::test_creatures::species();
    species.behavior.memory_ticks = 150;
    let mut game = Game::region_corridor(
        1,
        4,
        crate::test_creatures::content_with(species.clone(), vec![]),
    );
    let player = game
        .spawn_actor(at(1, 2), NonZeroU64::new(100).unwrap())
        .unwrap();
    let watcher = game
        .spawn_actor(at(3, 5), NonZeroU64::new(100).unwrap())
        .unwrap();
    crate::test_creatures::configure(&mut game, watcher, "neutral", species);
    assert!(
        game.creature(watcher).is_some(),
        "frozen subjects own their builds"
    );
    let profile = crate::ai::AiProfile {
        care: true,
        equipment: true,
        loot: true,
        gold: false,
        flee_percent: 25,
        idle: crate::behavior::IdleBehavior::Wait,
        doors: false,
    };
    game.configure_ai(watcher, profile).unwrap();
    game.configure_run(player, BTreeSet::from([player]), BTreeMap::new())
        .unwrap();
    game.add_default_reference_points().unwrap();
    game.actors.get_mut(&watcher).unwrap().behavior.engagement =
        Some(crate::behavior::Engagement {
            reference: crate::perception::ActorReferenceKind::Creature,
            target: player,
            location: at(1, 2),
            last_seen: 0,
        });
    let mut records = MemoryRecords::default();
    game.transition_regions(&sets(&[1], &[1]), &mut records)
        .unwrap();
    for _ in 0..4 {
        game.act(player, Action::Wait).unwrap();
    }
    // Longer than the watcher's memory, but it was frozen throughout.
    assert_eq!(game.tick(), 400);
    game.transition_regions(&sets(&[1, 2, 3, 4], &[1, 2, 3, 4]), &mut records)
        .unwrap();
    let seen = game.actors[&watcher].behavior.engagement.unwrap().last_seen;
    assert_eq!(seen, 400);
}

fn exploration_goal_corridor() -> (Game, ActorId, ActorId, Location) {
    let at = |region, x| Location {
        region: RegionId(region),
        position: Position { x, y: 1, z: 0 },
    };
    let mut game = Game::region_corridor(42, 6, crate::test_creatures::content());
    let player = game
        .spawn_actor(at(1, 2), NonZeroU64::new(100).unwrap())
        .unwrap();
    let explorer = game
        .spawn_actor(at(4, 5), NonZeroU64::new(100).unwrap())
        .unwrap();
    crate::test_creatures::configure(
        &mut game,
        explorer,
        "neutral",
        crate::test_creatures::species(),
    );
    game.configure_ai(explorer, crate::ai::AiProfile::default())
        .unwrap();
    game.configure_run(player, BTreeSet::from([player]), BTreeMap::new())
        .unwrap();
    game.add_default_reference_points().unwrap();
    let goal = at(6, 5);
    game.combat
        .ai
        .get_mut(&explorer)
        .unwrap()
        .exploration_destination = Some(goal);
    // Losing route memory is a decision-time obstacle, not malformed storage.
    game.navigation.remove(&explorer);
    assert!(game.combat_valid());
    (game, player, explorer, goal)
}

#[test]
fn exploration_destination_survives_detach_frozen_clock_and_forgotten_navigation() {
    let (mut game, player, explorer, goal) = exploration_goal_corridor();
    let mut records = MemoryRecords::default();
    game.transition_regions(&sets(&[1], &[1]), &mut records)
        .unwrap();
    assert_eq!(game.region_state(RegionId(6)), Some(RegionState::Detached));
    assert!(game.world.knows(goal));
    assert!(!game.world.contains(goal));
    let id = game.detached_record(RegionId(4)).unwrap();
    let record = records.get(id).unwrap();
    assert_eq!(record.ai[&explorer].exploration_destination, Some(goal));
    assert!(!record.navigation.contains_key(&explorer));
    let bytes = serde_json::to_vec(&*record).unwrap();
    let decoded: RegionRecordData = serde_json::from_slice(&bytes).unwrap();
    let restored = decoded.restore(game.content().clone()).unwrap();
    assert_eq!(restored, *record);
    records.put(id, Shared::new(restored));
    assert!(game.detached_records_valid(&mut records));
    for _ in 0..4 {
        game.act(player, Action::Wait).unwrap();
    }
    assert_eq!(game.tick(), 400);
    game.transition_regions(
        &sets(&[1, 2, 3, 4, 5, 6], &[1, 2, 3, 4, 5, 6]),
        &mut records,
    )
    .unwrap();
    assert_eq!(
        game.combat.ai[&explorer].exploration_destination,
        Some(goal)
    );
    assert!(game.combat_valid());
}

#[test]
fn detached_exploration_destination_rejects_unknown_or_outside_bounds_atomically() {
    for invalid in [
        Location {
            region: RegionId(999),
            position: Position { x: 5, y: 1, z: 0 },
        },
        Location {
            region: RegionId(3),
            position: Position {
                x: i32::MAX,
                y: 1,
                z: 0,
            },
        },
    ] {
        let (mut game, _, explorer, _) = exploration_goal_corridor();
        let mut records = MemoryRecords::default();
        game.transition_regions(&sets(&[1], &[1]), &mut records)
            .unwrap();
        assert!(game.detached_records_valid(&mut records));
        let id = game.detached_record(RegionId(4)).unwrap();
        let mut record = (*records.get(id).unwrap()).clone();
        record
            .ai
            .get_mut(&explorer)
            .unwrap()
            .exploration_destination = Some(invalid);
        // Simulate a corrupt read with a fresh immutable store rather than
        // illegally replacing already-published bytes under one RecordId.
        let mut damaged = MemoryRecords::default();
        for other in records.ids().collect::<Vec<_>>() {
            let value = if other == id {
                Shared::new(record.clone())
            } else {
                records.get(other).unwrap()
            };
            damaged.put(other, value);
        }
        records = damaged;
        assert!(!game.detached_records_valid(&mut records));
        let before = game.clone();
        assert!(matches!(
            game.transition_regions(
                &sets(&[1, 2, 3, 4, 5, 6], &[1, 2, 3, 4, 5, 6]),
                &mut records
            ),
            Err(TransitionError::InvalidRecord(_))
        ));
        assert_eq!(game, before);
    }
}

#[test]
fn thawing_rejects_engagement_deadline_overflow_atomically() {
    let at = |region, x| Location {
        region: RegionId(region),
        position: Position { x, y: 1, z: 0 },
    };
    let mut game = Game::region_corridor(1, 4, crate::test_creatures::content());
    let player = game
        .spawn_actor(at(1, 2), NonZeroU64::new(100).unwrap())
        .unwrap();
    let watcher = game
        .spawn_actor(at(3, 5), NonZeroU64::new(100).unwrap())
        .unwrap();
    crate::test_creatures::configure(
        &mut game,
        watcher,
        "neutral",
        crate::test_creatures::species(),
    );
    game.configure_run(player, BTreeSet::from([player]), BTreeMap::new())
        .unwrap();
    game.actors.get_mut(&watcher).unwrap().behavior.engagement =
        Some(crate::behavior::Engagement {
            reference: crate::perception::ActorReferenceKind::Creature,
            target: player,
            location: at(1, 2),
            last_seen: 0,
        });
    game.freeze_region(RegionId(3)).unwrap();
    game.tick = u64::MAX - 100;
    let before = game.clone();
    let mut records = MemoryRecords::default();
    assert_eq!(
        game.transition_regions(&sets(&[1, 2, 3, 4], &[1, 2, 3, 4]), &mut records),
        Err(TransitionError::Rule(GameError::Schedule(
            crate::ScheduleError::TimeExhausted
        )))
    );
    assert_eq!(game, before);
}

/// A player in region 1 of a four-region corridor and another actor in
/// region 3, with everything but region 1 and its neighbour detached.
fn detached_corridor() -> (Game, ActorId, MemoryRecords) {
    let at = |region, x| Location {
        region: RegionId(region),
        position: Position { x, y: 1, z: 0 },
    };
    let mut game = Game::region_corridor(1, 4, crate::test_creatures::content());
    let player = game
        .spawn_actor(at(1, 2), NonZeroU64::new(100).unwrap())
        .unwrap();
    let other = game
        .spawn_actor(at(4, 5), NonZeroU64::new(100).unwrap())
        .unwrap();
    game.configure_run(player, BTreeSet::from([player]), BTreeMap::new())
        .unwrap();
    game.add_default_reference_points().unwrap();
    let mut records = MemoryRecords::default();
    game.transition_regions(&sets(&[1], &[1]), &mut records)
        .unwrap();
    assert_eq!(game.region_state(RegionId(4)), Some(RegionState::Detached));
    (game, other, records)
}

fn sets(active: &[u64], loaded: &[u64]) -> RegionTransition {
    let only = |ids: &[u64]| ids.iter().map(|id| RegionId(*id)).collect::<BTreeSet<_>>();
    RegionTransition {
        active: only(active),
        loaded: only(loaded),
    }
}

#[test]
fn attach_rejects_a_record_that_does_not_match_the_directory() {
    let (mut game, other, mut records) = detached_corridor();
    assert!(game.lifecycle_state_valid());
    assert!(game.detached_records_valid(&mut records));

    // A record that's valid on its own but lost an actor the directory
    // still places in its region, as a damaged store might return.
    let four = RegionId(4);
    let mut damaged = MemoryRecords::default();
    for (region, id) in game.detached_records() {
        let mut record = (*records.get(id).unwrap()).clone();
        if region == four {
            assert!(record.actors.remove(&other).is_some());
            record.stamps.remove(&other);
            record.ai.remove(&other);
            record.navigation.remove(&other);
            record.explored_maps.remove(&other);
            record.displaced.remove(&other);
            assert!(record.valid_contents(four, game.tick()));
            assert!(!record.matches_directory(four, &game.lifecycle.directory));
        }
        damaged.put(id, Shared::new(record));
    }
    assert!(!game.detached_records_valid(&mut damaged));

    let before = game.clone();
    let error = game
        .transition_regions(&sets(&[1, 2, 3, 4], &[1, 2, 3, 4]), &mut damaged)
        .unwrap_err();
    assert_eq!(error, TransitionError::InvalidRecord(four));
    assert_eq!(game, before);
}

#[test]
fn a_missing_record_fails_the_transition_without_changes() {
    let (mut game, _, mut records) = detached_corridor();
    let (_, lost) = game
        .detached_records()
        .find(|(region, _)| *region == RegionId(4))
        .unwrap();
    records.retain(|id| id != lost);
    let before = game.clone();
    let error = game
        .transition_regions(&sets(&[1, 2, 3, 4], &[1, 2, 3, 4]), &mut records)
        .unwrap_err();
    assert_eq!(error, TransitionError::RecordUnavailable(RegionId(4)));
    assert_eq!(game, before);
}

#[test]
fn records_are_put_only_when_a_transition_succeeds() {
    let (mut game, _, mut records) = detached_corridor();
    game.transition_regions(&sets(&[1, 2, 3, 4], &[1, 2, 3, 4]), &mut records)
        .unwrap();
    let known: Vec<_> = records.ids().collect();
    // Region 1 holds the observing point, so it can't be detached.
    let before = game.clone();
    let error = game
        .apply_region_transition(&sets(&[], &[2, 3, 4]), &mut records)
        .unwrap_err();
    assert!(matches!(error, TransitionError::MustBeActive(_)));
    assert_eq!(game, before);
    assert_eq!(records.ids().collect::<Vec<_>>(), known);
}

#[test]
fn record_ids_are_never_reused_across_a_rewind() {
    let (mut game, _, mut records) = detached_corridor();
    let earlier = game.clone();
    game.transition_regions(&sets(&[1, 2, 3, 4], &[1, 2, 3, 4]), &mut records)
        .unwrap();
    game.transition_regions(&sets(&[1], &[1]), &mut records)
        .unwrap();
    let later: BTreeSet<_> = game.detached_records().map(|(_, id)| id).collect();

    // Rewinding to `earlier` and detaching again must not reuse the
    // later records' identities, which retained boundaries may hold.
    let mut rewound = earlier.clone();
    rewound.continue_record_ids(&game);
    rewound
        .transition_regions(&sets(&[1, 2, 3, 4], &[1, 2, 3, 4]), &mut records)
        .unwrap();
    rewound
        .transition_regions(&sets(&[1], &[1]), &mut records)
        .unwrap();
    assert!(rewound
        .detached_records()
        .all(|(_, id)| !later.contains(&id)));
    assert!(rewound.lifecycle_state_valid());
}

/// Builds regions from a fully built template game, as a scenario
/// package would.
struct TemplateSource {
    template: Game,
    records: MemoryRecords,
    /// Drop this actor from built records, as a faulty source might.
    lose: Option<ActorId>,
}

impl RecordStore for TemplateSource {
    fn put(&mut self, id: RecordId, record: Shared<RegionRecord>) {
        self.records.put(id, record);
    }
    fn get(&mut self, id: RecordId) -> Option<Shared<RegionRecord>> {
        self.records.get(id)
    }
    fn build(&mut self, region: RegionId) -> Option<RegionRecord> {
        let mut record = self.template.clone().into_region_record(region).ok()?;
        if let Some(lost) = self.lose {
            record.actors.remove(&lost);
            record.stamps.remove(&lost);
        }
        Some(record)
    }
    fn unbuilt(&mut self, region: RegionId) -> Option<UnbuiltRegion> {
        Some(UnbuiltRegion {
            region: self.template.world.region(region)?.clone(),
            chamber: false,
            identities: self
                .template
                .clone()
                .into_region_record(region)
                .ok()?
                .identities(),
        })
    }
}

/// A four-region corridor with an actor in region 3, and the same game
/// with every region unbuilt.
fn unbuilt_corridor() -> (Game, Game, ActorId) {
    let mut template = Game::region_corridor(1, 4, crate::test_creatures::content());
    let far = template
        .spawn_actor(
            Location {
                region: RegionId(3),
                position: Position { x: 5, y: 1, z: 0 },
            },
            NonZeroU64::new(100).unwrap(),
        )
        .unwrap();
    let mut game = Game::new(
        tor_world::World::new(vec![], vec![]).unwrap(),
        1,
        crate::test_creatures::content(),
    );
    for id in 1..=4 {
        let region = template.world.region(RegionId(id)).unwrap().clone();
        let mut identities = RegionIdentities::default();
        if id == 3 {
            identities.actors.insert(far);
        }
        game.add_unbuilt_region(region, false, identities).unwrap();
    }
    (template, game, far)
}

#[test]
fn generated_batch_commits_detached_and_streams_independently() {
    let (template, mut game, _) = unbuilt_corridor();
    let mut source = TemplateSource {
        template,
        records: MemoryRecords::default(),
        lose: None,
    };
    let batch = (1..=4)
        .rev()
        .map(|id| {
            let region = RegionId(id);
            (
                source.unbuilt(region).unwrap(),
                source.build(region).unwrap(),
            )
        })
        .collect();
    game.register_generated_regions(batch, &mut source).unwrap();
    assert!(game.loaded_regions().next().is_none());
    assert_eq!(game.detached_records().count(), 4);
    assert_eq!(game.detached_record(RegionId(1)), Some(RecordId(1)));
    game.transition_regions(&sets(&[1], &[1]), &mut source)
        .unwrap();
    // Existing body/perception pins can additionally keep the adjoining
    // region; registration must not pin the complete batch.
    assert!(game.loaded_regions().count() < 4);
    assert!(game.loaded_regions().any(|id| id == RegionId(1)));
    assert_eq!(game.region_state(RegionId(4)), Some(RegionState::Detached));
    let before = game.clone();
    let duplicate = vec![(
        source.unbuilt(RegionId(1)).unwrap(),
        source.build(RegionId(1)).unwrap(),
    )];
    assert!(game
        .register_generated_regions(duplicate, &mut source)
        .is_err());
    assert_eq!(game, before);
}

#[test]
fn generated_feature_refinement_is_atomic_and_cannot_steal_reserved_identity() {
    use tor_world::{Extent, World};
    let make = |region: u64, door: u64, trap: u64| {
        let mut world = World::new(vec![], vec![]).unwrap();
        world
            .add_chamber(Region {
                id: RegionId(region),
                name: "features".into(),
                bounds: Extent::new(5, 5, 2).unwrap(),
            })
            .unwrap();
        let at = Location {
            region: RegionId(region),
            position: Position { x: 2, y: 2, z: 0 },
        };
        let mut template = Game::new(world, 1, crate::test_creatures::content());
        template.place_authored_door(door, at, false, 2).unwrap();
        template
            .conceal_door(at, tor_world::Concealment { dc: 12 })
            .unwrap();
        template
            .place_trap(
                at,
                tor_world::Trap {
                    id: trap,
                    dc: 12,
                    avoidance_dc: None,
                    kind: tor_world::TrapKind::Rust {
                        slot: "body_armor".into(),
                        amount: 1,
                    },
                },
            )
            .unwrap();
        let record = template.into_region_record(RegionId(region)).unwrap();
        (
            UnbuiltRegion {
                region: Region {
                    id: RegionId(region),
                    name: "features".into(),
                    bounds: Extent::new(5, 5, 2).unwrap(),
                },
                chamber: true,
                identities: record.identities(),
            },
            record,
        )
    };
    let mut game = Game::new(
        World::new(vec![], vec![]).unwrap(),
        1,
        crate::test_creatures::content(),
    );
    for id in [1, 2] {
        game.add_unbuilt_region(
            Region {
                id: RegionId(id),
                name: "features".into(),
                bounds: Extent::new(5, 5, 2).unwrap(),
            },
            true,
            RegionIdentities::default(),
        )
        .unwrap();
    }
    let mut store = MemoryRecords::default();
    let before = game.clone();
    assert!(game
        .register_generated_regions(vec![make(1, 10, 20), make(2, 10, 21)], &mut store)
        .is_err());
    assert_eq!(game, before);
    assert_eq!(store.len(), 0);
    game.register_generated_regions(vec![make(1, 10, 20), make(2, 11, 21)], &mut store)
        .unwrap();
    assert!(game.lifecycle_state_valid());
    assert_eq!(store.len(), 2);
    assert_eq!(game.lifecycle.directory.traps[&20], RegionId(1));
    assert_eq!(game.next_door_id, 12);
    // Detached records retain metadata; reopening cannot reroll a generated choice.
    assert!(game.detached_records_valid(&mut store));
}

#[test]
fn invalid_generated_batch_does_not_publish_any_member() {
    let (template, mut game, _) = unbuilt_corridor();
    let mut source = TemplateSource {
        template,
        records: MemoryRecords::default(),
        lose: None,
    };
    let mut batch = (1..=4)
        .map(|id| {
            let region = RegionId(id);
            (
                source.unbuilt(region).unwrap(),
                source.build(region).unwrap(),
            )
        })
        .collect::<Vec<_>>();
    batch[3].1.actors.clear();
    batch[3].1.stamps.clear();
    // Region 3 owns the actor; removing it invalidates that member.
    batch[2].1.actors.clear();
    batch[2].1.stamps.clear();
    let before = game.clone();
    assert!(game.register_generated_regions(batch, &mut source).is_err());
    assert_eq!(game, before);
    assert!(source.records.ids().next().is_none());
}

/// A game that knows no regions declares each from the source only when
/// a transition loads it or a built region links to it, so game state
/// grows with the regions played, not the source.
#[test]
fn transitions_declare_regions_from_their_source_only_as_needed() {
    let (template, _, _) = unbuilt_corridor();
    let mut game = Game::new(
        tor_world::World::new(vec![], vec![]).unwrap(),
        1,
        crate::test_creatures::content(),
    );
    game.reserve_identities(
        template.next_actor_id,
        template.next_item_id,
        template.next_door_id,
    );
    let before = game.clone();
    let error = game
        .clone()
        .transition_regions(&sets(&[1], &[1]), &mut MemoryRecords::default())
        .unwrap_err();
    assert_eq!(error, TransitionError::UnknownRegion(RegionId(1)));
    assert_eq!(game, before);

    let mut source = TemplateSource {
        template: template.clone(),
        records: MemoryRecords::default(),
        lose: None,
    };
    game.transition_regions(&sets(&[1], &[1]), &mut source)
        .unwrap();
    // Region 1 is active; pins keep region 2, which it links to, loaded;
    // region 2's build declared region 3; region 4 isn't known at all.
    assert_eq!(game.region_state(RegionId(1)), Some(RegionState::Active));
    assert_eq!(game.region_state(RegionId(2)), Some(RegionState::Frozen));
    assert_eq!(game.region_state(RegionId(3)), Some(RegionState::Unbuilt));
    assert_eq!(game.region_state(RegionId(4)), None);
    assert!(game.lifecycle_state_valid());
    game.transition_regions(&sets(&[1, 2, 3, 4], &[1, 2, 3, 4]), &mut source)
        .unwrap();
    assert_eq!(game, template);
}

#[test]
fn reserved_identities_are_never_allocated() {
    let mut game = Game::region_corridor(1, 2, crate::test_creatures::content());
    game.reserve_identities(50, 60, 70);
    let at = Location {
        region: RegionId(1),
        position: Position { x: 2, y: 1, z: 0 },
    };
    let actor = game.spawn_actor(at, NonZeroU64::new(100).unwrap()).unwrap();
    assert_eq!(actor, ActorId(50));
    assert_eq!((game.next_item_id, game.next_door_id), (60, 70));
    // Reserving less never moves allocation back.
    game.reserve_identities(1, 1, 1);
    assert_eq!(game.next_actor_id, 51);
}

#[test]
fn unbuilt_regions_build_from_their_source() {
    let (template, mut game, far) = unbuilt_corridor();
    assert_eq!(game.region_state(RegionId(3)), Some(RegionState::Unbuilt));
    assert!(game.lifecycle_state_valid());
    assert!(game.detached_actor(far), "declared identities are reserved");
    let mut source = TemplateSource {
        template: template.clone(),
        records: MemoryRecords::default(),
        lose: None,
    };
    let (_, report) = game
        .transition_regions(&sets(&[1, 2, 3, 4], &[1, 2, 3, 4]), &mut source)
        .unwrap();
    assert_eq!(report.built, report.attached);
    assert_eq!(report.built.len(), 4);
    assert_eq!(game, template);
}

#[test]
fn building_needs_a_source_that_keeps_its_declared_identities() {
    let (template, game, far) = unbuilt_corridor();
    let all = sets(&[1, 2, 3, 4], &[1, 2, 3, 4]);

    let mut nothing = MemoryRecords::default();
    let mut unchanged = game.clone();
    let error = unchanged
        .transition_regions(&all, &mut nothing)
        .unwrap_err();
    assert_eq!(error, TransitionError::RecordUnavailable(RegionId(1)));
    assert_eq!(unchanged, game);

    let mut faulty = TemplateSource {
        template,
        records: MemoryRecords::default(),
        lose: Some(far),
    };
    let error = unchanged.transition_regions(&all, &mut faulty).unwrap_err();
    assert_eq!(error, TransitionError::InvalidRecord(RegionId(3)));
    assert_eq!(unchanged, game);
}

#[test]
fn an_unbuilt_region_cannot_declare_an_identity_in_use() {
    let (template, mut game, far) = unbuilt_corridor();
    let mut region = template.world.region(RegionId(1)).unwrap().clone();
    region.id = RegionId(5);
    let identities = RegionIdentities {
        actors: BTreeSet::from([far]),
        ..Default::default()
    };
    let before = game.clone();
    assert!(game.add_unbuilt_region(region, false, identities).is_err());
    assert_eq!(game, before);
}

/// The world's reach answers (exit field, then cached search) must never
/// disagree with an uncached search, from any cell. The
/// world has a tall chamber with a stair in its middle, joins across two of
/// its faces to other chambers (rim projection), and walls
/// near the faces.
#[test]
fn reach_answers_agree_with_the_search_everywhere() {
    use tor_world::{Extent, Passage, World};
    let at = |region, x, y, z| Location {
        region: RegionId(region),
        position: Position { x, y, z },
    };
    let mut game = Game::new(
        World::new(vec![], vec![]).unwrap(),
        1,
        crate::test_creatures::content(),
    );
    for (id, [x, y, z]) in [
        (1, [12, 12, 8]),
        (2, [9, 9, 3]),
        (3, [7, 7, 3]),
        (4, [5, 5, 2]),
    ] {
        game.add_chamber(Region {
            id: RegionId(id),
            name: String::new(),
            bounds: Extent::new(x, y, z).unwrap(),
        })
        .unwrap();
    }
    let join = |game: &mut Game, from, direction, to, turns, width, height| {
        game.connect_area(
            Passage {
                from,
                direction,
                to,
            },
            turns,
            width,
            height,
        )
        .unwrap_or_else(|e| panic!("join {from:?} {direction:?}: {e:?}"));
    };
    join(
        &mut game,
        at(1, 11, 3, 0),
        Direction::East,
        at(2, 0, 3, 0),
        0,
        3,
        2,
    );
    join(
        &mut game,
        at(2, 0, 3, 0),
        Direction::West,
        at(1, 11, 3, 0),
        0,
        3,
        2,
    );
    join(
        &mut game,
        at(1, 4, 0, 0),
        Direction::North,
        at(3, 2, 6, 0),
        0,
        2,
        2,
    );
    join(
        &mut game,
        at(3, 2, 6, 0),
        Direction::South,
        at(1, 4, 0, 0),
        0,
        2,
        2,
    );
    for (from, direction, to) in [
        (at(1, 6, 6, 0), Direction::Up, at(4, 2, 2, 0)),
        (at(4, 2, 2, 0), Direction::Down, at(1, 6, 6, 0)),
    ] {
        game.connect(
            Passage {
                from,
                direction,
                to,
            },
            0,
        )
        .unwrap();
    }
    for wall in [
        at(1, 10, 1, 0),
        at(1, 11, 7, 1),
        at(1, 1, 10, 0),
        at(2, 1, 6, 0),
        at(3, 5, 1, 1),
    ] {
        let _ = game.set_wall(wall, true);
    }
    let fielded = game
        .check_reach()
        .unwrap_or_else(|cell| panic!("disagreement at {cell:?}"));
    let cells: i32 = (1..=4)
        .map(|id| {
            let (x, y, z) = game.world.region(RegionId(id)).unwrap().bounds.dimensions();
            x * y * z
        })
        .sum();
    // Both kinds of cell occur: answered by the field, and searched.
    assert!(
        fielded > 50 && (cells as usize) - fielded > 50,
        "{fielded} of {cells}"
    );
}

/// Every command clones the game, so lifecycle state that grows with the
/// world (unbuilt and detached regions, their identities) must be
/// shared by the clone, not copied.
#[test]
fn cloning_a_game_shares_state_that_grows_with_the_world() {
    let (_, game, _) = unbuilt_corridor();
    let clone = game.clone();
    let (l, c) = (&game.lifecycle, &clone.lifecycle);
    assert!(l.unbuilt.shares_storage(&c.unbuilt));
    assert!(l.detached.shares_storage(&c.detached));
    assert!(l.directory.shares_storage(&c.directory));
}

/// The directory's region index, used when a region attaches, agrees with
/// its maps through insertion, removal and a save round trip.
#[test]
fn the_directory_region_index_stays_consistent() {
    let identities = |actor, item, door| RegionIdentities {
        actors: BTreeSet::from([ActorId(actor)]),
        items: BTreeSet::from([ItemId(item)]),
        doors: BTreeSet::from([door]),
        traps: BTreeSet::from([door + 100]),
    };
    let mut directory = Directory::default();
    directory.insert(RegionId(1), identities(2, 3, 4));
    directory.insert(RegionId(5), identities(6, 7, 8));
    let saved = serde_json::to_string(&directory).unwrap();
    assert!(!saved.contains("by_region"), "the index is never saved");
    let restored: Directory = serde_json::from_str(&saved).unwrap();
    assert_eq!(restored, directory);
    assert_eq!(directory.remove_region(RegionId(1)), identities(2, 3, 4));
    assert_eq!(directory.actors.keys().collect::<Vec<_>>(), [&ActorId(6)]);
    assert_eq!(
        directory.remove_region(RegionId(1)),
        RegionIdentities::default()
    );
    let mut expected = Directory::default();
    expected.insert(RegionId(5), identities(6, 7, 8));
    assert_eq!(directory, expected);
}

/// A record from storage gets the checks restoring a checkpoint gives
/// loaded actors and items, not only its own consistency: one with an
/// impossible actor (a 30th orientation) is rejected, changing nothing.
#[test]
fn attach_checks_a_record_as_thoroughly_as_restore() {
    let (mut game, other, mut records) = detached_corridor();
    let three = RegionId(4);
    let mut damaged = MemoryRecords::default();
    for (region, id) in game.detached_records() {
        let mut record = (*records.get(id).unwrap()).clone();
        if region == three {
            record.actors.get_mut(&other).unwrap().orientation = 30;
            assert!(record.valid_contents(three, game.tick()));
            assert!(record.matches_directory(three, &game.lifecycle.directory));
        }
        damaged.put(id, Shared::new(record));
    }
    let before = game.clone();
    let error = game
        .transition_regions(&sets(&[1, 2, 3, 4], &[1, 2, 3, 4]), &mut damaged)
        .unwrap_err();
    assert_eq!(error, TransitionError::InvalidRecord(three));
    assert_eq!(game, before);
}
#[test]
fn observing_points_activate_the_same_cells_as_merged_actor_perception() {
    let mut world = tor_world::World::new(vec![], vec![]).unwrap();
    for id in 1..=3 {
        world
            .add_chamber(Region {
                id: RegionId(id),
                name: "shaft".into(),
                bounds: tor_world::Extent::new(3, 3, 2).unwrap(),
            })
            .unwrap();
    }
    let at = |region, z| Location {
        region: RegionId(region),
        position: Position { x: 1, y: 1, z },
    };
    for (from, direction, to) in [
        (at(1, 1), Direction::Up, at(2, 0)),
        (at(2, 0), Direction::Down, at(3, 1)),
    ] {
        world
            .connect_portal_area(
                tor_world::Passage {
                    from,
                    direction,
                    to,
                },
                0,
                1,
                1,
            )
            .unwrap();
    }
    world.set_region_light(RegionId(3), false).unwrap();
    let mut game = Game::new(world, 42, crate::test_creatures::content());
    let actor = game
        .spawn_actor(at(1, 1), NonZeroU64::new(100).unwrap())
        .unwrap();
    game.set_body(
        actor,
        crate::BodySpec {
            cells: vec![[0, 0, 0], [0, 0, 1]],
            eye: [0, 0, 1],
            mass: 80,
        },
    )
    .unwrap();
    game.add_reference_point(ReferencePoint {
        target: ReferenceTarget::Actor(actor),
        active_radius: Some(0),
        load_radius: Some(0),
        observes: true,
    })
    .unwrap();
    let perceived: BTreeSet<_> = game
        .scene(actor)
        .unwrap()
        .iter()
        .map(|cell| cell.location.region)
        .collect();
    assert_eq!(perceived, BTreeSet::from([RegionId(1), RegionId(2)]));
    let need = game.point_requirements(&|_| true);
    assert_eq!(need.active, perceived);
    assert!(
        need.loaded.contains(&RegionId(3)),
        "dark geometry still needs to be read"
    );
}
