use super::*;
use std::num::NonZeroU64;
use tor_world::{Location, Position, RegionId};

#[test]
fn restored_item_knowledge_requires_known_leaves_but_keeps_historical_identity() {
    let mut game = Game::two_room_in_stone(42, crate::test_creatures::content());
    let actor = game
        .spawn_actor(
            Location {
                region: RegionId(1),
                position: Position { x: 1, y: 1, z: 0 },
            },
            NonZeroU64::new(100).unwrap(),
        )
        .unwrap();
    let historical: crate::ItemIdentity = "item.potion.historical-known-type".parse().unwrap();
    assert!(game
        .items
        .iter()
        .all(|(_, item)| item.spec.identity != historical));
    game.learn_identity(actor, &historical).unwrap();
    let mut shared = SharedState::default();
    let snapshot = game.checkpoint(&mut shared);
    let snapshot_value = serde_json::to_value(&snapshot).unwrap();
    let good = serde_json::to_value(&shared).unwrap();
    let decoded: SharedState = serde_json::from_value(good.clone()).unwrap();
    let restored = Game::restore_checkpoint(
        serde_json::from_value(snapshot_value.clone()).unwrap(),
        &decoded,
        game.content().clone(),
    )
    .unwrap();
    assert!(restored.actors[&actor].knowledge.contains(&historical));
    for invalid in ["item", "item.potion", "item.unknown", "item.potion.unknown"] {
        let mut forged = good.clone();
        forged["actor_records"][shared.actors[snapshot.actors][&ActorId(1)]]["knowledge"] =
            serde_json::json!([historical.as_str(), invalid]);
        let decoded: SharedState = serde_json::from_value(forged).unwrap();
        assert!(
            matches!(
                Game::restore_checkpoint(serde_json::from_value(snapshot_value.clone()).unwrap(), &decoded, game.content().clone()),
                Err(PersistError::ActorState { actor: rejected }) if rejected == actor
            ),
            "syntactically valid but nonconcrete knowledge {invalid} must reject"
        );
    }
}

#[test]
fn restored_actor_clocks_cannot_come_from_the_future() {
    let mut game = Game::two_room_in_stone(42, crate::test_creatures::content());
    game.spawn_actor(
        Location {
            region: RegionId(1),
            position: Position { x: 1, y: 1, z: 0 },
        },
        NonZeroU64::new(100).unwrap(),
    )
    .unwrap();
    let mut shared = SharedState::default();
    let snapshot = game.checkpoint(&mut shared);
    let mut value = serde_json::to_value(&shared).unwrap();
    value["actor_records"][shared.actors[snapshot.actors][&ActorId(1)]]["active_since"] =
        serde_json::json!(game.tick() + 1);
    let forged: SharedState = serde_json::from_value(value).unwrap();
    assert_eq!(
        Game::restore_checkpoint(snapshot, &forged, game.content().clone()),
        Err(PersistError::ActorClock { actor: ActorId(1) })
    );
}

#[test]
fn shared_navigation_validation_cannot_cross_world_bounds_or_actor_membership() {
    let world = |width| {
        let mut world = World::new(vec![], vec![]).unwrap();
        world
            .add_region(tor_world::Region {
                id: RegionId(1),
                name: "validation".into(),
                bounds: tor_world::Extent::new(width, 2, 1).unwrap(),
            })
            .unwrap();
        world
    };
    let mut game = Game::new(world(8), 42, crate::test_creatures::content());
    let actor = game
        .spawn_actor(
            Location {
                region: RegionId(1),
                position: Position { x: 0, y: 0, z: 0 },
            },
            NonZeroU64::new(100).unwrap(),
        )
        .unwrap();
    game.refresh_navigation();
    assert!(game.known_cells(actor).any(|at| at.position.x == 6));
    let mut shared = SharedState::default();
    let valid = game.checkpoint(&mut shared);
    let mut other_world = game.checkpoint(&mut shared);
    shared.worlds.push(world(2));
    other_world.world = shared.worlds.len() - 1;
    let mut other_actor = game.checkpoint(&mut shared);
    let index = other_actor.navigation.remove(&actor).unwrap();
    other_actor.navigation.insert(ActorId(999), index);
    let mut context = RestoreContext::new(&shared, game.content().clone());
    context
        .bind_spatial_roots(&[&valid, &other_world, &other_actor])
        .unwrap();
    assert!(context.restore(valid).is_ok());
    assert!(context.restore(other_world).is_err());
    assert!(context.restore(other_actor).is_err());
}

fn pooled_ai_fixture() -> Game {
    let mut game = Game::two_room_in_stone(42, crate::test_creatures::content());
    for x in 1..=3 {
        let actor = game
            .spawn_actor(
                Location {
                    region: RegionId(1),
                    position: Position { x, y: 1, z: 0 },
                },
                NonZeroU64::new(100).unwrap(),
            )
            .unwrap();
        crate::test_creatures::configure(
            &mut game,
            actor,
            "peaceful",
            crate::test_creatures::species(),
        );
        game.configure_ai(actor, crate::ai::AiProfile::default())
            .unwrap();
        for x in 1..=4 {
            game.combat.ai.get_mut(&actor).unwrap().visits.insert(
                Location {
                    region: RegionId(1),
                    position: Position { x, y: 1, z: 0 },
                },
                actor.0 + x as u64,
            );
        }
    }
    game
}

#[test]
fn exploration_destination_capture_is_independent_of_visits_and_snapshot_mutation() {
    let mut game = pooled_ai_fixture();
    let actor = ActorId(2);
    let goal = game.actors[&ActorId(3)].location;
    game.navigation.remove(&actor);
    let mut shared = SharedState::default();
    let before = game.checkpoint(&mut shared);
    let original = game.clone();
    game.combat
        .ai
        .get_mut(&actor)
        .unwrap()
        .exploration_destination = Some(goal);
    let after = game.checkpoint(&mut shared);
    assert_ne!(before.combat.ai[&actor], after.combat.ai[&actor]);
    assert_capture_sequence_round_trip(
        &[before, after],
        &shared,
        &[original.clone(), game.clone()],
    );
    // A single-root restore binds only the pool for that root.
    let mut shared = SharedState::default();
    let snapshot = game.checkpoint(&mut shared);
    let mut restored = Game::restore_checkpoint(snapshot, &shared, game.content().clone()).unwrap();
    assert_eq!(
        restored.combat.ai[&actor].exploration_destination,
        Some(goal)
    );
    restored
        .combat
        .ai
        .get_mut(&actor)
        .unwrap()
        .exploration_destination = None;
    assert_eq!(game.combat.ai[&actor].exploration_destination, Some(goal));
    assert_eq!(original.combat.ai[&actor].exploration_destination, None);
    assert_eq!(
        restored.combat.ai[&actor].visits,
        game.combat.ai[&actor].visits
    );
}

#[test]
fn checkpoint_rejects_exploration_destination_outside_known_world_bounds() {
    for goal in [
        Location {
            region: RegionId(999),
            position: Position { x: 1, y: 1, z: 0 },
        },
        Location {
            region: RegionId(1),
            position: Position {
                x: i32::MAX,
                y: 1,
                z: 0,
            },
        },
    ] {
        let mut game = pooled_ai_fixture();
        let mut shared = SharedState::default();
        let good = game.checkpoint(&mut shared);
        assert!(Game::restore_checkpoint(good, &shared, game.content().clone()).is_ok());
        game.combat
            .ai
            .get_mut(&ActorId(2))
            .unwrap()
            .exploration_destination = Some(goal);
        // Do not retain unused good-root records in this one-root pool.
        let mut shared = SharedState::default();
        let snapshot = game.checkpoint(&mut shared);
        assert!(matches!(
            Game::restore_checkpoint(snapshot, &shared, game.content().clone()),
            Err(PersistError::RunState)
        ));
    }
}

fn assert_capture_sequence_round_trip(
    snapshots: &[Snapshot],
    shared: &SharedState,
    games: &[Game],
) {
    let bytes = serde_json::to_vec(&(snapshots, shared)).unwrap();
    let (decoded, shared): (Vec<Snapshot>, SharedState) = serde_json::from_slice(&bytes).unwrap();
    let mut restore = RestoreContext::new(&shared, games[0].content().clone());
    restore
        .bind_spatial_roots(&decoded.iter().collect::<Vec<_>>())
        .unwrap();
    let actual: Vec<_> = decoded
        .into_iter()
        .map(|root| restore.restore(root).unwrap())
        .collect();
    assert_eq!(actual, games);
}

#[test]
fn capture_memo_skips_unchanged_actor_fingerprints_and_tracks_cow() {
    let mut game = pooled_ai_fixture();
    let original = game.clone();
    let mut shared = SharedState::default();
    let mut snapshots = vec![game.checkpoint(&mut shared)];
    let first_work = shared.actor_records.capture_fingerprint_work();
    assert_eq!(first_work.0, game.actors.len());
    assert!(first_work.1 > 0);
    let mut expected = vec![game.clone()];
    for _ in 0..8 {
        snapshots.push(game.checkpoint(&mut shared));
        expected.push(game.clone());
    }
    game.actors.get_mut(&ActorId(1)).unwrap().ready_at += 100;
    let changed_bytes = serde_json::to_vec(&ActorRecord::capture(&game.actors[&ActorId(1)]))
        .unwrap()
        .len();
    snapshots.push(game.checkpoint(&mut shared));
    expected.push(game.clone());
    // Returning to an older immutable source must recover its original index.
    snapshots.push(original.checkpoint(&mut shared));
    expected.push(original.clone());
    let original_bytes = serde_json::to_vec(&ActorRecord::capture(&original.actors[&ActorId(1)]))
        .unwrap()
        .len();
    assert_capture_sequence_round_trip(&snapshots, &shared, &expected);
    assert_eq!(shared.actor_records.len(), first_work.0 + 1);
    assert_eq!(
        shared.actor_records.capture_fingerprint_work(),
        (
            first_work.0 + 2,
            first_work.1 + changed_bytes + original_bytes
        ),
        "unchanged source allocations must not be serialized again"
    );
}

#[test]
fn capture_memo_skips_equal_ai_and_detects_profile_state_and_visit_changes() {
    let mut game = pooled_ai_fixture();
    let mut shared = SharedState::default();
    let mut snapshots = vec![game.checkpoint(&mut shared)];
    let mut expected = vec![game.clone()];
    let first_work = shared.ai_records.capture_fingerprint_work();
    assert_eq!(first_work.0, game.combat.ai.len());
    assert!(first_work.1 > 0);
    let mut changed_bytes = 0;
    for change in 0..4 {
        for _ in 0..4 {
            snapshots.push(game.checkpoint(&mut shared));
            expected.push(game.clone());
        }
        let ai = game.combat.ai.get_mut(&ActorId(2)).unwrap();
        match change {
            0 => ai.profile.flee_percent += 1,
            1 => ai.state = crate::ai::State::Flee,
            2 => {
                ai.visits.insert(
                    Location {
                        region: RegionId(1),
                        position: Position { x: 1, y: 1, z: 0 },
                    },
                    999,
                );
            }
            _ => {
                ai.visits = original_ai_visits();
            }
        }
        changed_bytes += serde_json::to_vec(ai).unwrap().len();
        snapshots.push(game.checkpoint(&mut shared));
        expected.push(game.clone());
    }
    assert_capture_sequence_round_trip(&snapshots, &shared, &expected);
    // The final change revisits the complete value after change 1: it
    // needs a fingerprint on the memo miss, but no new pooled payload.
    assert_eq!(shared.ai_records.len(), first_work.0 + 3);
    assert_eq!(
        shared.ai_records.capture_fingerprint_work(),
        (first_work.0 + 4, first_work.1 + changed_bytes),
        "only changed complete AI values require new fingerprints"
    );
}

fn original_ai_visits() -> crate::navigation_map::RegionMap<Location, u64> {
    pooled_ai_fixture()
        .combat
        .ai
        .remove(&ActorId(2))
        .unwrap()
        .visits
}

#[test]
fn capture_memo_preserves_structural_dedup_and_decoded_round_trip_bytes() {
    let mut game = pooled_ai_fixture();
    let mut warm = SharedState::default();
    let mut cold = SharedState::default();
    let mut warm_roots = Vec::new();
    let mut cold_roots = Vec::new();
    let mut expected = Vec::new();
    for step in 0..5 {
        if step == 2 {
            // Equal new allocation must use structural fallback, never mint a record.
            let actor = game.actors[&ActorId(1)].clone();
            game.actors.insert(ActorId(1), actor);
        }
        if step == 3 {
            game.actors.get_mut(&ActorId(1)).unwrap().ready_at += 1;
        }
        warm_roots.push(game.checkpoint(&mut warm));
        // Clear only capture hints, keeping the exact same canonical pools.
        cold.clear_capture_memos();
        cold_roots.push(game.checkpoint(&mut cold));
        expected.push(game.clone());
    }
    assert_eq!(warm.actor_records.len(), game.actors.len() + 1);
    assert_eq!(
        serde_json::to_vec(&(&warm_roots, &warm)).unwrap(),
        serde_json::to_vec(&(&cold_roots, &cold)).unwrap()
    );
    let bytes = serde_json::to_vec(&(&warm_roots, &warm)).unwrap();
    let (decoded_roots, mut decoded): (Vec<Snapshot>, SharedState) =
        serde_json::from_slice(&bytes).unwrap();
    decoded
        .prepare_spatial_serialization(&decoded_roots.iter().collect::<Vec<_>>())
        .unwrap();
    assert_eq!(
        serde_json::to_vec(&(&decoded_roots, &decoded)).unwrap(),
        bytes
    );
    assert_capture_sequence_round_trip(&decoded_roots, &decoded, &expected);
    assert_capture_sequence_round_trip(&warm_roots, &warm, &expected);
    assert_capture_sequence_round_trip(&cold_roots, &cold, &expected);
    assert_eq!(
        warm.actor_records.capture_fingerprint_work().0,
        game.actors.len() + 2
    );
}

#[test]
fn retained_ai_records_share_visits_and_restore_independent_mutable_states() {
    let mut game = pooled_ai_fixture();
    let mut shared = SharedState::default();
    let mut expected = Vec::new();
    let snapshots: Vec<_> = (0..32)
        .map(|n| {
            game.actors.get_mut(&ActorId(1)).unwrap().ready_at = 100 + n;
            if n == 16 {
                let at = game.actors[&ActorId(2)].location;
                game.combat
                    .ai
                    .get_mut(&ActorId(2))
                    .unwrap()
                    .visits
                    .insert(at, 1000);
            }
            expected.push(game.clone());
            game.checkpoint(&mut shared)
        })
        .collect();
    let bytes = serde_json::to_vec(&(snapshots, shared)).unwrap();
    // This schema-independent witness also fails against the old embedded
    // AI representation: 32 snapshots multiply three complete visit maps.
    let visit_fields = std::str::from_utf8(&bytes)
        .unwrap()
        .matches("\"visits\":")
        .count();
    let (snapshots, shared): (Vec<Snapshot>, SharedState) = serde_json::from_slice(&bytes).unwrap();
    let mut context = RestoreContext::new(&shared, game.content().clone());
    context
        .bind_spatial_roots(&snapshots.iter().collect::<Vec<_>>())
        .unwrap();
    let mut restored: Vec<_> = snapshots
        .into_iter()
        .map(|s| context.restore(s).unwrap())
        .collect();
    assert_eq!(restored, expected);
    let at = restored[0].actors[&ActorId(2)].location;
    restored[0]
        .combat
        .ai
        .get_mut(&ActorId(2))
        .unwrap()
        .visits
        .insert(at, 2000);
    assert_eq!(restored[1], expected[1]);
    assert_eq!(restored[16], expected[16]);
    assert_eq!(
        visit_fields, 4,
        "only one of three complete AI records changes"
    );
}

#[test]
fn changing_one_actor_does_not_repeat_other_actor_records() {
    let mut world = World::new(vec![], vec![]).unwrap();
    world
        .add_region(tor_world::Region {
            id: RegionId(1),
            name: "record pooling".into(),
            bounds: tor_world::Extent::new(34, 3, 1).unwrap(),
        })
        .unwrap();
    let mut game = Game::new(world, 42, crate::test_creatures::content());
    for x in 1..=32 {
        game.spawn_actor(
            Location {
                region: RegionId(1),
                position: Position { x, y: 1, z: 0 },
            },
            NonZeroU64::new(100).unwrap(),
        )
        .unwrap();
    }
    let mut one = SharedState::default();
    let first = game.checkpoint(&mut one);
    let single_bytes = serde_json::to_vec(&(vec![first], one)).unwrap().len();
    let mut shared = SharedState::default();
    let mut expected = Vec::new();
    let snapshots: Vec<_> = (0..64)
        .map(|n| {
            game.actors.get_mut(&ActorId(1)).unwrap().body.mass = 80 + n;
            expected.push(game.clone());
            game.checkpoint(&mut shared)
        })
        .collect();
    let bytes = serde_json::to_vec(&(snapshots, shared)).unwrap();
    let (snapshots, shared): (Vec<Snapshot>, SharedState) = serde_json::from_slice(&bytes).unwrap();
    let mut context = RestoreContext::new(&shared, game.content().clone());
    context
        .bind_spatial_roots(&snapshots.iter().collect::<Vec<_>>())
        .unwrap();
    let mut restored: Vec<_> = snapshots
        .into_iter()
        .map(|s| context.restore(s).unwrap())
        .collect();
    assert_eq!(restored, expected);
    restored[0].actors.get_mut(&ActorId(2)).unwrap().body.mass += 1;
    assert_eq!(restored[1], expected[1]);
    eprintln!(
        "actor pooling fixture: single={single_bytes}, retained={}",
        bytes.len()
    );
    assert!(
        bytes.len() < single_bytes * 8,
        "one changed actor repeated unrelated records: {} versus {}",
        bytes.len(),
        single_bytes
    );
}

#[test]
fn expanded_record_budget_charges_aliases_but_not_other_snapshots() {
    let game = pooled_ai_fixture();
    let mut shared = SharedState::default();
    let mut snapshot = game.checkpoint(&mut shared);
    let total = shared.actors[snapshot.actors]
        .values()
        .map(|&index| shared.actor_records.encoded_bytes(index).unwrap())
        .sum::<usize>()
        + snapshot
            .combat
            .ai
            .values()
            .map(|&index| shared.ai_records.encoded_bytes(index).unwrap())
            .sum::<usize>();
    assert!(shared
        .record_expansion_with_budget(&[&snapshot], total)
        .unwrap());
    assert!(shared
        .record_expansion_with_budget(&[&snapshot, &snapshot], total)
        .unwrap());
    assert!(!shared
        .record_expansion_with_budget(&[&snapshot], total - 1)
        .unwrap());
    let record = shared.actors[snapshot.actors][&ActorId(1)];
    let actor_bytes = shared.actor_records.encoded_bytes(record).unwrap();
    shared.actors[snapshot.actors].insert(ActorId(999), record);
    assert!(!shared
        .record_expansion_with_budget(&[&snapshot], total)
        .unwrap());
    assert!(shared
        .record_expansion_with_budget(&[&snapshot], total + actor_bytes)
        .unwrap());
    let ai = snapshot.combat.ai[&ActorId(1)];
    let ai_bytes = shared.ai_records.encoded_bytes(ai).unwrap();
    snapshot.combat.ai.insert(ActorId(999), ai);
    assert!(!shared
        .record_expansion_with_budget(&[&snapshot], total + actor_bytes)
        .unwrap());
    assert!(shared
        .record_expansion_with_budget(&[&snapshot], total + actor_bytes + ai_bytes)
        .unwrap());
    let bytes = serde_json::to_vec(&(snapshot, shared)).unwrap();
    let (snapshot, shared): (Snapshot, SharedState) = serde_json::from_slice(&bytes).unwrap();
    assert!(shared
        .record_expansion_with_budget(&[&snapshot], total + actor_bytes + ai_bytes)
        .unwrap());
    assert!(!shared
        .record_expansion_with_budget(&[&snapshot], total + actor_bytes + ai_bytes - 1)
        .unwrap());
    assert!(shared.record_expansion_within_limit(&[&snapshot]).unwrap());
}

#[test]
fn restored_record_cache_shares_actors_across_distinct_directories() {
    let mut game = pooled_ai_fixture();
    let earlier = game.clone();
    let mut shared = SharedState::default();
    let before = game.checkpoint(&mut shared);
    game.actors.get_mut(&ActorId(1)).unwrap().ready_at += 100;
    let after = game.checkpoint(&mut shared);
    assert_ne!(before.actors, after.actors);
    let bytes = serde_json::to_vec(&(vec![before, after], shared)).unwrap();
    let (snapshots, shared): (Vec<Snapshot>, SharedState) = serde_json::from_slice(&bytes).unwrap();
    let mut context = RestoreContext::new(&shared, game.content().clone());
    context
        .bind_spatial_roots(&snapshots.iter().collect::<Vec<_>>())
        .unwrap();
    let mut snapshots = snapshots.into_iter();
    let mut first = context.restore(snapshots.next().unwrap()).unwrap();
    let second = context.restore(snapshots.next().unwrap()).unwrap();
    assert_eq!(first, earlier);
    assert_eq!(second, game);
    assert!(
        std::ptr::eq(&first.actors[&ActorId(2)], &second.actors[&ActorId(2)]),
        "an unchanged complete actor record was reconstructed for another directory"
    );
    first.actors.get_mut(&ActorId(2)).unwrap().body.mass += 1;
    assert_eq!(second, game);
}

#[test]
fn restored_record_cache_is_scoped_to_item_table() {
    let mut game = pooled_ai_fixture();
    let at = game.actors[&ActorId(1)].location;
    let mut spec =
        crate::ItemSpec::ordinary("item.test.pool-token".parse().unwrap(), "pool token".into());
    spec.stackable = true;
    let item = game.place_item_stack(100, at, None, 2, spec).unwrap();
    let earlier = game.clone();
    let mut shared = SharedState::default();
    let before = game.checkpoint(&mut shared);
    game.items.edit(item, |entry| entry.quantity = 1).unwrap();
    let after = game.checkpoint(&mut shared);
    assert_eq!(before.actors, after.actors);
    assert_ne!(before.items, after.items);
    let mut context = RestoreContext::new(&shared, game.content().clone());
    context.bind_spatial_roots(&[&before, &after]).unwrap();
    let first = context.restore(before).unwrap();
    let second = context.restore(after).unwrap();
    assert_eq!(first, earlier);
    assert_eq!(second, game);
    assert!(
        !std::ptr::eq(&first.actors[&ActorId(2)], &second.actors[&ActorId(2)]),
        "record restoration must not reuse an actor validated against another item table"
    );
}

#[test]
fn pooled_records_reject_missing_unused_duplicate_and_malformed_entries() {
    let game = pooled_ai_fixture();
    let mut shared = SharedState::default();
    let snapshot = game.checkpoint(&mut shared);
    let good = serde_json::to_value((&snapshot, &shared)).unwrap();
    let valid: (Snapshot, SharedState) = serde_json::from_value(good.clone()).unwrap();
    assert_eq!(
        Game::restore_checkpoint(valid.0, &valid.1, game.content().clone()).unwrap(),
        game
    );
    for field in ["actor_records", "ai_records"] {
        let mut duplicate = good.clone();
        let record = duplicate[1][field][0].clone();
        duplicate[1][field].as_array_mut().unwrap().push(record);
        assert!(
            serde_json::from_value::<(Snapshot, SharedState)>(duplicate).is_err(),
            "duplicate {field}"
        );
        let mut missing = good.clone();
        missing[1].as_object_mut().unwrap().remove(field);
        assert!(
            serde_json::from_value::<(Snapshot, SharedState)>(missing).is_err(),
            "missing {field}"
        );
        let mut malformed = good.clone();
        malformed[1][field][0] = serde_json::json!({"unexpected": true});
        assert!(
            serde_json::from_value::<(Snapshot, SharedState)>(malformed).is_err(),
            "malformed {field}"
        );
    }
    for index in [usize::MAX, 999] {
        let mut invalid = good.clone();
        invalid[1]["actors"][0]["1"] = serde_json::json!(index);
        let (snapshot, shared) =
            serde_json::from_value::<(Snapshot, SharedState)>(invalid).unwrap();
        assert!(Game::restore_checkpoint(snapshot, &shared, game.content().clone()).is_err());
        let mut invalid = good.clone();
        invalid[0]["combat"]["ai"]["1"] = serde_json::json!(index);
        let (snapshot, shared) =
            serde_json::from_value::<(Snapshot, SharedState)>(invalid).unwrap();
        assert!(Game::restore_checkpoint(snapshot, &shared, game.content().clone()).is_err());
    }
    for field in ["actor_records", "ai_records"] {
        let mut unused = good.clone();
        let mut record = unused[1][field][0].clone();
        if field == "actor_records" {
            record["ready_at"] = serde_json::json!(1001);
        } else {
            record["profile"]["flee_percent"] = serde_json::json!(26);
        }
        unused[1][field].as_array_mut().unwrap().push(record);
        let (snapshot, shared) = serde_json::from_value::<(Snapshot, SharedState)>(unused).unwrap();
        assert!(
            matches!(
                Game::restore_checkpoint(snapshot, &shared, game.content().clone()),
                Err(PersistError::RecordPool)
            ),
            "unused {field} record"
        );
    }
    let mut unknown_actor = good.clone();
    let index = unknown_actor[0]["combat"]["ai"]
        .as_object_mut()
        .unwrap()
        .remove("1")
        .unwrap();
    unknown_actor[0]["combat"]["ai"]["999"] = index;
    let (snapshot, shared) =
        serde_json::from_value::<(Snapshot, SharedState)>(unknown_actor).unwrap();
    assert!(matches!(
        Game::restore_checkpoint(snapshot, &shared, game.content().clone()),
        Err(PersistError::RecordPool)
    ));
    let mut excessive = good;
    excessive[1]["actors"] = serde_json::Value::Array(
        (0..=MAX_CHECKPOINT_ROOTS)
            .map(|i| serde_json::json!({(i+1).to_string():0}))
            .collect(),
    );
    assert!(serde_json::from_value::<(Snapshot, SharedState)>(excessive).is_err());
}

#[test]
fn pooled_record_indices_and_bytes_are_deterministic() {
    let capture = || {
        let mut game = pooled_ai_fixture();
        let mut pool = SharedState::default();
        let mut snapshots = Vec::new();
        for value in [10, 20, 10] {
            let at = game.actors[&ActorId(1)].location;
            game.combat
                .ai
                .get_mut(&ActorId(1))
                .unwrap()
                .visits
                .insert(at, value);
            snapshots.push(game.checkpoint(&mut pool));
        }
        serde_json::to_vec(&(snapshots, pool)).unwrap()
    };
    assert_eq!(capture(), capture());
}

#[test]
fn pooled_actor_and_ai_directories_reject_duplicate_keys_and_unused_directories() {
    let game = pooled_ai_fixture();
    let mut shared = SharedState::default();
    let snapshot = game.checkpoint(&mut shared);
    let snapshot_json = serde_json::to_string(&snapshot).unwrap();
    let duplicate_ai = snapshot_json.replace("\"ai\":{\"1\":0", "\"ai\":{\"1\":0,\"1\":0");
    assert_ne!(snapshot_json, duplicate_ai);
    assert!(serde_json::from_str::<Snapshot>(&duplicate_ai).is_err());
    let shared_json = serde_json::to_string(&shared).unwrap();
    let duplicate_actor =
        shared_json.replace("\"actors\":[{\"1\":0", "\"actors\":[{\"1\":0,\"1\":0");
    assert_ne!(shared_json, duplicate_actor);
    assert!(serde_json::from_str::<SharedState>(&duplicate_actor).is_err());
    let mut zero_actor = serde_json::to_value(&shared).unwrap();
    let index = zero_actor["actors"][0]
        .as_object_mut()
        .unwrap()
        .remove("1")
        .unwrap();
    zero_actor["actors"][0]["0"] = index;
    assert!(serde_json::from_value::<SharedState>(zero_actor).is_err());
    let mut duplicate_directory = serde_json::to_value(&shared).unwrap();
    let directory = duplicate_directory["actors"][0].clone();
    duplicate_directory["actors"]
        .as_array_mut()
        .unwrap()
        .push(directory);
    assert!(serde_json::from_value::<SharedState>(duplicate_directory).is_err());
    let mut value = serde_json::to_value(&shared).unwrap();
    value["actors"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({}));
    let shared: SharedState = serde_json::from_value(value).unwrap();
    assert!(Game::restore_checkpoint(snapshot, &shared, game.content().clone()).is_err());
}

#[test]
fn repeated_boundaries_do_not_repeat_unchanged_actor_payloads() {
    let mut world = World::new(vec![], vec![]).unwrap();
    world
        .add_region(tor_world::Region {
            id: RegionId(1),
            name: "retention".into(),
            bounds: tor_world::Extent::new(66, 3, 1).unwrap(),
        })
        .unwrap();
    let mut game = Game::new(world, 42, crate::test_creatures::content());
    for x in 1..=64 {
        game.spawn_actor(
            Location {
                region: RegionId(1),
                position: Position { x, y: 1, z: 0 },
            },
            NonZeroU64::new(100).unwrap(),
        )
        .unwrap();
    }
    let mut one = SharedState::default();
    let snapshot = game.checkpoint(&mut one);
    let single_bytes = serde_json::to_vec(&(vec![snapshot], one)).unwrap().len();
    let mut shared = SharedState::default();
    let snapshots: Vec<_> = (0..256)
        .map(|_| {
            let mut independently_owned = game.clone();
            independently_owned.actors =
                crate::actor_store::ActorStore::from_entries(game.actors.owned_actor_entries());
            independently_owned.checkpoint(&mut shared)
        })
        .collect();
    assert_eq!(shared.actors.len(), 1);
    let bytes = serde_json::to_vec(&(snapshots, shared)).unwrap();
    assert!(
        bytes.len() < single_bytes * 8,
        "unchanged actors repeated at retained boundaries: {} versus {}",
        bytes.len(),
        single_bytes
    );
    let (snapshots, shared): (Vec<Snapshot>, SharedState) = serde_json::from_slice(&bytes).unwrap();
    let mut context = RestoreContext::new(&shared, game.content().clone());
    context
        .bind_spatial_roots(&snapshots.iter().collect::<Vec<_>>())
        .unwrap();
    let mut restored: Vec<_> = snapshots
        .into_iter()
        .map(|snapshot| context.restore(snapshot).unwrap())
        .collect();
    for boundary in &restored {
        assert_eq!(boundary, &game);
    }
    assert!(std::ptr::eq(
        &restored[0].actors[&ActorId(1)],
        &restored[1].actors[&ActorId(1)]
    ));
    restored[0].actors.get_mut(&ActorId(1)).unwrap().body.mass += 1;
    assert_eq!(restored[1], game);
}

#[test]
fn retained_queues_share_complete_intentions_at_the_supported_capacity() {
    for count in [64, crate::intention::MAX_QUEUED_INTENTIONS] {
        let mut world = World::new(vec![], vec![]).unwrap();
        world
            .add_region(tor_world::Region {
                id: RegionId(1),
                name: "queue retention".into(),
                bounds: tor_world::Extent::new(count as i32 + 2, 3, 1).unwrap(),
            })
            .unwrap();
        let mut queued = Game::new(world, 42, crate::test_creatures::content());
        for x in 1..=count as i32 {
            let actor = queued
                .spawn_actor(
                    Location {
                        region: RegionId(1),
                        position: Position { x, y: 1, z: 0 },
                    },
                    NonZeroU64::new(100).unwrap(),
                )
                .unwrap();
            queued
                .admit_intention(
                    actor,
                    crate::Action::Wait,
                    crate::intention::IntentionOrigin::Human,
                )
                .unwrap();
        }
        let id = queued.pending_intention(ActorId(1)).unwrap().id;
        let mut suspended = queued.clone();
        suspended.suspend_intention(ActorId(1), id).unwrap();
        let mut single = SharedState::default();
        let snapshot = queued.checkpoint(&mut single);
        let single_bytes = serde_json::to_vec(&(vec![snapshot], single)).unwrap().len();
        let mut shared = SharedState::default();
        let snapshots: Vec<_> = (0..256)
            .map(|index| {
                if index % 2 == 0 {
                    queued.checkpoint(&mut shared)
                } else {
                    suspended.checkpoint(&mut shared)
                }
            })
            .collect();
        let bytes = serde_json::to_vec(&(snapshots, shared)).unwrap();
        assert!(
            bytes.len() < single_bytes * 8,
            "retained queues repeat complete payloads at {count} actors: {} versus {single_bytes}",
            bytes.len()
        );
        assert!(bytes.len() < 16 * 1024 * 1024);
        let (snapshots, shared): (Vec<Snapshot>, SharedState) =
            serde_json::from_slice(&bytes).unwrap();
        let mut context = RestoreContext::new(&shared, queued.content().clone());
        context
            .bind_spatial_roots(&snapshots.iter().collect::<Vec<_>>())
            .unwrap();
        let mut restored: Vec<_> = snapshots
            .into_iter()
            .map(|s| context.restore(s).unwrap())
            .collect();
        for (index, game) in restored.iter().enumerate() {
            assert_eq!(game, if index % 2 == 0 { &queued } else { &suspended });
        }
        restored[0].cancel_intention(ActorId(1), id).unwrap();
        assert_eq!(restored[2], queued);
        let next = restored[0]
            .admit_intention(
                ActorId(1),
                crate::Action::Wait,
                crate::intention::IntentionOrigin::Human,
            )
            .unwrap();
        assert_eq!(next.0, count as u64 + 1);
    }
}

#[test]
fn decoded_restore_shares_equal_bodies_without_changing_values() {
    for count in [16, 256, 4096] {
        let mut world = World::new(vec![], vec![]).unwrap();
        world
            .add_region(tor_world::Region {
                id: RegionId(1),
                name: "restore".into(),
                bounds: tor_world::Extent::new(count * 3 + 3, 3, 1).unwrap(),
            })
            .unwrap();
        let mut game = Game::new(world, 42, crate::test_creatures::content());
        for x in 0..count {
            let id = game
                .spawn_actor(
                    Location {
                        region: RegionId(1),
                        position: Position {
                            x: x * 3,
                            y: 1,
                            z: 0,
                        },
                    },
                    NonZeroU64::new(100).unwrap(),
                )
                .unwrap();
            if x % 2 == 1 {
                game.set_body(
                    id,
                    crate::BodySpec {
                        cells: vec![[0, 0, 0], [1, 0, 0]],
                        eye: [1, 0, 0],
                        mass: 91,
                    },
                )
                .unwrap();
            }
        }
        let mut shared = SharedState::default();
        let snapshot = game.checkpoint(&mut shared);
        let bytes = serde_json::to_vec(&(snapshot, shared)).unwrap();
        let (snapshot, shared): (Snapshot, SharedState) = serde_json::from_slice(&bytes).unwrap();
        let restored = Game::restore_checkpoint(snapshot, &shared, game.content().clone()).unwrap();
        assert_eq!(restored, game);
        let first = &restored.actors[&ActorId(1)].body;
        let second = &restored.actors[&ActorId(2)].body;
        assert!(!first.shares_storage(second));
        for (id, actor) in restored.actors.iter() {
            let expected = if id.0 % 2 == 1 { first } else { second };
            assert!(
                actor.body.shares_storage(expected),
                "actor {id:?}, count {count}"
            );
        }
        let mut encoded_shared = SharedState::default();
        let encoded_snapshot = restored.checkpoint(&mut encoded_shared);
        assert_eq!(
            serde_json::to_vec(&(encoded_snapshot, encoded_shared)).unwrap(),
            bytes
        );
    }
}

#[test]
fn ground_item_references_are_checked_at_runtime_checkpoint_and_detached_restore() {
    use crate::content_ids::{AbilityId, ConditionId, DescriptorId};
    use crate::rules::{Effect, Modifier, ModifierOp, RulesetError, Scope};
    let mut modifier =
        crate::ItemSpec::ordinary("item.test.armor".parse().unwrap(), "armor".into());
    modifier.class = crate::ItemClass::Armor;
    modifier.equipment = Some(crate::EquipmentSpec {
        periodic_relocation: None,
        potency_modifiers: Vec::new(),
        weapon_profile: None,
        armor: None,
        slot: crate::EquipmentSlot::new("body_armor").unwrap(),
        attack: None,
        modifiers: vec![Modifier {
            cancellable: false,
            scope: Scope::Always,
            op: ModifierOp::GrantAbility {
                ability: AbilityId::new("missing_ability").unwrap(),
            },
        }],
    });
    let mut effect =
        crate::ItemSpec::ordinary("item.test.potion".parse().unwrap(), "potion".into());
    effect.class = crate::ItemClass::Potion;
    effect.consumable = Some(crate::ConsumableSpec {
        descriptors: vec![],
        discovery: crate::ItemDiscovery::SelfEffect,
        effects: vec![Effect::ApplyCondition {
            condition: ConditionId::new("missing_condition").unwrap(),
            duration: 100,
        }],
    });
    let mut attack =
        crate::ItemSpec::ordinary("item.test.weapon".parse().unwrap(), "weapon".into());
    attack.class = crate::ItemClass::Weapon;
    attack.equipment = Some(crate::EquipmentSpec {
        periodic_relocation: None,
        potency_modifiers: Vec::new(),
        weapon_profile: None,
        armor: None,
        slot: crate::EquipmentSlot::new("weapon").unwrap(),
        modifiers: vec![],
        attack: Some(
            crate::attacks::MeleeAttack::fixed(
                crate::attributes::Skill::HeavyWeaponry,
                0,
                100,
                40,
                crate::combat::DamageType::Impact,
                Some(DescriptorId::new("missing_descriptor").unwrap()),
                2,
            )
            .unwrap(),
        ),
    });
    let mut slot = crate::ItemSpec::ordinary(
        "item.test.unknown-slot-armor".parse().unwrap(),
        "unknown slot armor".into(),
    );
    slot.class = crate::ItemClass::Armor;
    slot.equipment = Some(crate::EquipmentSpec {
        periodic_relocation: None,
        potency_modifiers: Vec::new(),
        weapon_profile: None,
        armor: None,
        slot: crate::EquipmentSlot::new("missing_slot").unwrap(),
        attack: None,
        modifiers: vec![],
    });
    for (spec, category, missing) in [
        (modifier, "ability", "missing_ability"),
        (effect, "condition", "missing_condition"),
        (attack, "descriptor", "missing_descriptor"),
        (slot, "equipment_slot", "missing_slot"),
    ] {
        let mut game = Game::two_room_in_stone(42, crate::test_creatures::content());
        let at = Location {
            region: RegionId(1),
            position: Position { x: 1, y: 1, z: 0 },
        };
        game.spawn_actor(at, NonZeroU64::new(100).unwrap()).unwrap();
        let before = game.clone();
        assert_eq!(
            game.place_item_stack(100, at, None, 1, spec.clone()),
            Err(crate::GameError::Rule(
                crate::abilities::RuleError::InvalidDefinition
            ))
        );
        assert_eq!(game, before, "invalid item insertion is atomic");
        let item = game
            .place_item_stack(
                100,
                at,
                None,
                1,
                crate::ItemSpec::ordinary("item.test.valid".parse().unwrap(), "valid".into()),
            )
            .unwrap();
        let content = game.content().clone();
        let mut shared = SharedState::default();
        let snapshot = game.checkpoint(&mut shared);
        shared.items[snapshot.items].get_mut(&item).unwrap().spec = Shared::new(spec.clone());
        let shared = serde_json::from_slice(&serde_json::to_vec(&shared).unwrap()).unwrap();
        let expected = PersistError::ItemDefinition {
            item,
            error: RulesetError::UnknownReference {
                category,
                id: missing.into(),
            },
        };
        assert_eq!(
            Game::restore_checkpoint(snapshot, &shared, content.clone()),
            Err(expected.clone())
        );
        let record = game.into_region_record(RegionId(1)).unwrap();
        let mut value =
            serde_json::to_value(crate::streaming::RegionRecordData::capture(&record)).unwrap();
        value["items"][item.0.to_string()]["spec"] = serde_json::to_value(spec).unwrap();
        let saved: crate::streaming::RegionRecordData = serde_json::from_value(value).unwrap();
        assert_eq!(saved.restore(content), Err(expected));
    }
}

#[test]
fn restoring_unknown_catalog_references_preserves_the_typed_cause() {
    let mut game = Game::two_room_in_stone(42, crate::test_creatures::content());
    let actor = game
        .spawn_actor(
            Location {
                region: RegionId(1),
                position: Position { x: 1, y: 1, z: 0 },
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
    let empty = Content::new(
        game.content().rules.clone(),
        crate::creatures::ScenarioCatalog::default(),
    )
    .unwrap();
    let mut shared = SharedState::default();
    let snapshot = game.checkpoint(&mut shared);
    assert_eq!(
        Game::restore_checkpoint(snapshot, &shared, empty),
        Err(PersistError::CreatureRecord {
            actor,
            error: crate::creatures::CreatureStateError::Build(
                crate::creatures::BuildError::UnknownSpecies
            ),
        })
    );
}

#[test]
fn decoded_catalog_and_item_definitions_share_across_distinct_boundaries() {
    let mut game = Game::two_room_in_stone(42, crate::test_creatures::content());
    let at = Location {
        region: RegionId(1),
        position: Position { x: 1, y: 1, z: 0 },
    };
    let actor = game.spawn_actor(at, NonZeroU64::new(100).unwrap()).unwrap();
    crate::test_creatures::configure(
        &mut game,
        actor,
        "neutral",
        crate::test_creatures::species(),
    );
    assert!(
        game.creature(actor).is_some(),
        "checkpoint subjects own their builds"
    );
    let mut spec = crate::ItemSpec::ordinary("item.test.token".parse().unwrap(), "token".into());
    spec.stackable = true;
    let item = game.place_item_stack(100, at, None, 2, spec).unwrap();
    let mut shared = SharedState::default();
    let before = game.checkpoint(&mut shared);
    game.items.edit(item, |entry| entry.quantity = 1).unwrap();
    crate::test_creatures::injure(&mut game, actor, 1).unwrap();
    let after = game.checkpoint(&mut shared);
    let bytes = serde_json::to_vec(&(vec![before, after], shared)).unwrap();
    let (snapshots, shared): (Vec<Snapshot>, SharedState) = serde_json::from_slice(&bytes).unwrap();
    let mut context = RestoreContext::new(&shared, game.content().clone());
    context
        .bind_spatial_roots(&snapshots.iter().collect::<Vec<_>>())
        .unwrap();
    let mut restored: Vec<_> = snapshots
        .into_iter()
        .map(|s| context.restore(s).unwrap())
        .collect();
    assert!(!restored[0].items.shares_storage(&restored[1].items));
    assert!(restored[0].actors[&actor]
        .anatomy
        .shares_storage(&restored[1].actors[&actor].anatomy));
    assert!(restored[0].items[&item]
        .spec
        .shares_storage(&restored[1].items[&item].spec));
    assert!(Arc::ptr_eq(
        restored[0].creature(actor).unwrap().build().content(),
        restored[1].creature(actor).unwrap().build().content()
    ));
    drop(context);
    let mut encoded_shared = SharedState::default();
    let encoded: Vec<_> = restored
        .iter()
        .map(|g| g.checkpoint(&mut encoded_shared))
        .collect();
    assert_eq!(
        serde_json::to_vec(&(encoded, encoded_shared)).unwrap(),
        bytes
    );
    restored[0]
        .items
        .edit(item, |entry| entry.spec.name = "changed".into())
        .unwrap();
    crate::test_creatures::injure(&mut restored[0], actor, 2).unwrap();
    assert_eq!(restored[0].creature(actor).unwrap().health().injury(), 2);
    assert_eq!(restored[1].creature(actor).unwrap().health().injury(), 1);
    assert_eq!(restored[1].items[&item].spec.name, "token");
}

#[test]
fn common_restore_context_shares_decoded_pools_and_preserves_copy_on_write() {
    let mut game = Game::two_room_in_stone(42, crate::test_creatures::content());
    let at = Location {
        region: RegionId(1),
        position: Position { x: 1, y: 1, z: 0 },
    };
    let actor = game.spawn_actor(at, NonZeroU64::new(100).unwrap()).unwrap();
    let item = game
        .place_item(at, "item.test.token".parse().unwrap(), "token".into())
        .unwrap();
    game.refresh_navigation();
    let mut shared = SharedState::default();
    let before = game.checkpoint(&mut shared);
    let original = game.clone();
    game.act(actor, crate::Action::Wait).unwrap();
    let after = game.checkpoint(&mut shared);
    let bytes = serde_json::to_vec(&(vec![before, after], shared)).unwrap();
    let (snapshots, shared): (Vec<Snapshot>, SharedState) = serde_json::from_slice(&bytes).unwrap();
    let mut context = RestoreContext::new(&shared, game.content().clone());
    context
        .bind_spatial_roots(&snapshots.iter().collect::<Vec<_>>())
        .unwrap();
    let mut restored: Vec<_> = snapshots
        .into_iter()
        .map(|s| context.restore(s).unwrap())
        .collect();
    assert_eq!(restored[0], original);
    assert_eq!(restored[1], game);
    assert!(restored[0].world.shares_storage(&restored[1].world));
    assert!(restored[0].navigation[&actor].shares_storage(&restored[1].navigation[&actor]));
    assert!(restored[0].items.shares_storage(&restored[1].items));
    assert!(restored[0].actors[&actor]
        .body
        .shares_storage(&restored[1].actors[&actor].body));
    drop(context);
    let mut encoded_shared = SharedState::default();
    let encoded: Vec<_> = restored
        .iter()
        .map(|g| g.checkpoint(&mut encoded_shared))
        .collect();
    assert_eq!(
        serde_json::to_vec(&(encoded, encoded_shared)).unwrap(),
        bytes
    );
    restored[0]
        .set_body(
            actor,
            crate::BodySpec {
                mass: 99,
                ..Default::default()
            },
        )
        .unwrap();
    restored[0]
        .items
        .edit(item, |item| item.spec.name = "edited".into())
        .unwrap();
    restored[0]
        .navigation
        .get_mut(&actor)
        .unwrap()
        .cells
        .remove(&at);
    restored[0]
        .world
        .add_region(tor_world::Region {
            id: RegionId(9),
            name: "new".into(),
            bounds: tor_world::Extent::new(3, 3, 1).unwrap(),
        })
        .unwrap();
    assert_eq!(restored[1], game);
    assert_eq!(restored[1].items[&item].spec.name, "token");
    assert_eq!(restored[1].actors[&actor].body.mass, 80);
}

#[test]
fn body_pool_preserves_cell_order_eye_and_mass_and_rejects_invalid_snapshots() {
    let base = crate::BodySpec {
        cells: vec![[0, 0, 0], [1, 0, 0]],
        eye: [0, 0, 0],
        mass: 80,
    };
    let mut reordered = base.clone();
    reordered.cells.reverse();
    let mut eye = base.clone();
    eye.eye = [1, 0, 0];
    let mut mass = base.clone();
    mass.mass = 81;
    let pool = BTreeSet::from([
        Shared::new(base),
        Shared::new(reordered),
        Shared::new(eye),
        Shared::new(mass),
    ]);
    assert_eq!(pool.len(), 4);
    let mut game = Game::two_room_in_stone(42, crate::test_creatures::content());
    let actor = game
        .spawn_actor(
            Location {
                region: RegionId(1),
                position: Position { x: 1, y: 1, z: 0 },
            },
            NonZeroU64::new(100).unwrap(),
        )
        .unwrap();
    // Corrupt the source before capture: canonical pooled records are
    // immutable, including their fingerprint and encoded-size metadata.
    let mut forged = game.clone();
    forged.actors.get_mut(&actor).unwrap().body.eye = [1, 0, 0];
    let mut shared = SharedState::default();
    let broken = forged.checkpoint(&mut shared);
    let valid = game.checkpoint(&mut shared);
    let mut context = RestoreContext::new(&shared, game.content().clone());
    context.bind_spatial_roots(&[&broken, &valid]).unwrap();
    assert!(context.restore(broken).is_err());
    assert_eq!(context.restore(valid), Ok(game));
}

#[test]
fn snapshots_preserve_complete_game_and_reject_invalid_shared_references() {
    let mut game = Game::two_room_in_stone(42, crate::test_creatures::content());
    let actor = game
        .spawn_actor(
            Location {
                region: RegionId(1),
                position: Position { x: 1, y: 1, z: 0 },
            },
            NonZeroU64::new(100).unwrap(),
        )
        .unwrap();
    game.refresh_navigation();
    let mut shared = SharedState::default();
    let before = game.checkpoint(&mut shared);
    let earlier = game.clone();
    game.act(actor, crate::Action::Wait).unwrap();
    let after = game.checkpoint(&mut shared);
    let mut context = RestoreContext::new(&shared, game.content().clone());
    context.bind_spatial_roots(&[&before, &after]).unwrap();
    assert_eq!(context.restore(before), Ok(earlier));
    assert_eq!(context.restore(after), Ok(game.clone()));
    // Each single-root negative fixture owns a matching single-root pool.
    let mut shared = SharedState::default();
    let mut broken = game.checkpoint(&mut shared);
    broken.navigation.insert(actor, usize::MAX);
    assert!(Game::restore_checkpoint(broken, &shared, game.content().clone()).is_err());
    let mut broken = game.checkpoint(&mut shared);
    broken.world = usize::MAX;
    assert!(Game::restore_checkpoint(broken, &shared, game.content().clone()).is_err());
    let mut broken = game.checkpoint(&mut shared);
    broken.actors = usize::MAX;
    assert!(Game::restore_checkpoint(broken, &shared, game.content().clone()).is_err());
}
