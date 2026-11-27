//! Destination continuity is actor-owned draft state, not a hidden-world plan.
use super::*;
use crate::behavior::IdleBehavior;
use std::collections::BTreeSet;
use tor_world::{Extent, Position, Region, RegionId};

const OWNER: ActorId = ActorId(2);
fn at(x: i32, y: i32) -> Location {
    Location {
        region: RegionId(77),
        position: Position { x, y, z: 0 },
    }
}
fn fixture() -> Game {
    let species = crate::test_creatures::species();
    let mut game =
        super::tests::configured_game(species.clone(), species, crate::test_creatures::ruleset());
    game.add_region(Region {
        id: RegionId(77),
        name: "Disclosed exploration room".into(),
        bounds: Extent::new(9, 3, 1).unwrap(),
    })
    .unwrap();
    game.set_region_light(RegionId(77), true).unwrap();
    game.teleport(OWNER, at(2, 1)).unwrap();
    game.configure_run(ActorId(1), BTreeSet::from([ActorId(1)]), BTreeMap::new())
        .unwrap();
    game.configure_ai(OWNER, AiProfile::default()).unwrap();
    game.refresh_navigation();
    let view = game.observe(OWNER).unwrap();
    assert!(view
        .visible_cells
        .iter()
        .any(|cell| cell.location == at(6, 1) && !cell.wall));
    let route = game.route_search(OWNER).unwrap().route(at(6, 1)).unwrap();
    assert_eq!(
        route.len(),
        4,
        "positive fixture has a bounded multi-step known route"
    );
    assert!(game.actor_translation(OWNER, route[0].direction).is_some());
    game
}
fn prefer_only(game: &mut Game, destination: Location) {
    let view = game.observe(OWNER).unwrap();
    let ai = game.combat.ai.get_mut(&OWNER).unwrap();
    for cell in view.visible_cells.iter() {
        ai.visits.insert(
            cell.location,
            if cell.location == destination { 0 } else { 10 },
        );
    }
}

#[test]
fn exploration_acquires_disclosed_multistep_destination_in_pure_preview() {
    let mut game = fixture();
    prefer_only(&mut game, at(6, 1));
    let before = game.clone();
    let first = game.choose_ai(OWNER).unwrap();
    let again = game.choose_ai(OWNER).unwrap();
    assert_eq!(first.ai.exploration_destination, Some(at(6, 1)));
    let route = game.route_search(OWNER).unwrap().route(at(6, 1)).unwrap();
    assert_eq!(first.action, Action::Move(route[0].direction));
    assert_eq!(first.action, again.action);
    assert_eq!(first.ai, again.ai);
    assert_eq!(first.behavior, again.behavior);
    assert_eq!(
        first.behavior.rng, game.actors[&OWNER].behavior.rng,
        "unique destination needs no draw"
    );
    assert_eq!(
        game, before,
        "discarded previews cannot install destinations or visits"
    );
}

#[test]
fn exploration_retained_destination_overrides_adjacent_visit_distraction() {
    let mut game = fixture();
    prefer_only(&mut game, at(1, 1));
    game.combat
        .ai
        .get_mut(&OWNER)
        .unwrap()
        .exploration_destination = Some(at(6, 1));
    let decision = game.choose_ai(OWNER).unwrap();
    let route = game.route_search(OWNER).unwrap().route(at(6, 1)).unwrap();
    assert_eq!(decision.action, Action::Move(route[0].direction));
    assert_ne!(decision.action, Action::Move(Direction::West));
    assert_eq!(decision.ai.exploration_destination, Some(at(6, 1)));
    assert_eq!(decision.behavior.rng, game.actors[&OWNER].behavior.rng);
}

#[test]
fn exploration_arrival_pauses_once_without_rng_and_clears_only_draft_goal() {
    let mut game = fixture();
    game.combat
        .ai
        .get_mut(&OWNER)
        .unwrap()
        .exploration_destination = Some(at(2, 1));
    let before = game.clone();
    let arrived = game.choose_ai(OWNER).unwrap();
    assert_eq!(arrived.action, Action::Wait);
    assert_eq!(arrived.ai.exploration_destination, None);
    assert_eq!(arrived.behavior.rng, game.actors[&OWNER].behavior.rng);
    assert_eq!(game, before);
    // Install the proposal only in a separate test copy to isolate the next
    // decision. Ordinary scheduler commit/restore is covered independently.
    game.combat.ai.insert(OWNER, arrived.ai);
    game.actors.get_mut(&OWNER).unwrap().behavior = arrived.behavior;
    assert!(matches!(
        game.choose_ai(OWNER).unwrap().action,
        Action::Move(_)
    ));
}

#[test]
fn exploration_wait_reconfiguration_discards_retained_destination() {
    let mut game = fixture();
    game.combat
        .ai
        .get_mut(&OWNER)
        .unwrap()
        .exploration_destination = Some(at(6, 1));
    game.configure_ai(
        OWNER,
        AiProfile {
            idle: IdleBehavior::Wait,
            ..Default::default()
        },
    )
    .unwrap();
    let decision = game.choose_ai(OWNER).unwrap();
    assert_eq!(decision.action, Action::Wait);
    assert_eq!(decision.ai.exploration_destination, None);
    assert_eq!(decision.behavior.rng, game.actors[&OWNER].behavior.rng);
}

#[test]
fn exploration_hidden_remote_changes_do_not_change_known_destination_choice() {
    let mut game = fixture();
    prefer_only(&mut game, at(6, 1));
    let mut twin = game.clone();
    twin.set_wall(
        Location {
            region: RegionId(2),
            position: Position { x: 3, y: 1, z: 0 },
        },
        true,
    )
    .unwrap();
    assert_eq!(game.observe(OWNER).unwrap(), twin.observe(OWNER).unwrap());
    let left = game.choose_ai(OWNER).unwrap();
    let right = twin.choose_ai(OWNER).unwrap();
    assert_eq!(left.ai.exploration_destination, Some(at(6, 1)));
    assert_eq!(left.action, right.action);
    assert_eq!(left.ai, right.ai);
    assert_eq!(left.behavior, right.behavior);
}

#[test]
fn exploration_destination_commits_and_pending_checkpoint_continues_identically() {
    let mut game = fixture();
    prefer_only(&mut game, at(6, 1));
    game.act(ActorId(1), Action::Wait).unwrap();
    let chosen = game.choose_ai(OWNER).unwrap();
    assert_eq!(chosen.ai.exploration_destination, Some(at(6, 1)));
    game.admit_ai_intention(OWNER).unwrap();
    assert_eq!(
        game.combat.ai[&OWNER].exploration_destination, None,
        "admission is not commit"
    );
    let mut shared = crate::checkpoint::SharedState::default();
    let snapshot =
        serde_json::from_value(serde_json::to_value(game.checkpoint(&mut shared)).unwrap())
            .unwrap();
    let shared = serde_json::from_value(serde_json::to_value(shared).unwrap()).unwrap();
    let mut restored = Game::restore_checkpoint(snapshot, &shared, game.content().clone()).unwrap();
    for candidate in [&mut game, &mut restored] {
        let execution = candidate.execute_next_intention().unwrap();
        assert_eq!(execution.action, Some(chosen.action.clone()));
        execution.outcome.unwrap();
        assert_eq!(
            candidate.combat.ai[&OWNER].exploration_destination,
            Some(at(6, 1))
        );
        assert_eq!(candidate.combat.ai[&OWNER].visits, chosen.ai.visits);
        assert_eq!(candidate.actors[&OWNER].behavior.rng, chosen.behavior.rng);
    }
    assert_eq!(game, restored);
    assert_eq!(
        game.choose_ai(OWNER).unwrap().action,
        restored.choose_ai(OWNER).unwrap().action
    );
}
// Append to ai_exploration_tests.rs only after the tracked-source freeze ends.

#[test]
fn exploration_impossible_retained_goal_clears_without_immediate_reselection() {
    let mut game = fixture();
    prefer_only(&mut game, at(6, 1));
    let impossible = Location {
        region: RegionId(2),
        position: Position { x: 4, y: 1, z: 0 },
    };
    assert!(game.route_search(OWNER).unwrap().route(impossible).is_err());
    game.combat
        .ai
        .get_mut(&OWNER)
        .unwrap()
        .exploration_destination = Some(impossible);
    let decision = game.choose_ai(OWNER).unwrap();
    assert!(
        matches!(decision.action, Action::Move(_)),
        "failed goal releases local exploration"
    );
    assert_eq!(
        decision.ai.exploration_destination, None,
        "even a valid disclosed replacement must wait until the next decision"
    );
    assert_eq!(
        game.combat.ai[&OWNER].exploration_destination,
        Some(impossible)
    );
}

#[test]
fn exploration_disclosed_occupied_goal_releases_local_fallback() {
    let mut game = fixture();
    game.teleport(ActorId(1), at(6, 1)).unwrap();
    game.refresh_navigation();
    let view = game.observe(OWNER).unwrap();
    assert!(
        view.visible_actors.iter().any(|row| row.id == ActorId(1)),
        "blocker is disclosed"
    );
    assert_eq!(
        game.route_search(OWNER)
            .unwrap()
            .route(at(6, 1))
            .unwrap()
            .len(),
        4,
        "remembered topology alone still routes to this occupied goal"
    );
    game.combat
        .ai
        .get_mut(&OWNER)
        .unwrap()
        .exploration_destination = Some(at(6, 1));
    let decision = game.choose_ai(OWNER).unwrap();
    assert!(matches!(decision.action, Action::Move(_)));
    assert_eq!(decision.ai.exploration_destination, None);
    assert!(decision.behavior.engagement.is_none());
}

#[test]
fn exploration_cancelled_intention_never_commits_proposed_destination() {
    let mut game = fixture();
    prefer_only(&mut game, at(6, 1));
    game.act(ActorId(1), Action::Wait).unwrap();
    let expected = game.choose_ai(OWNER).unwrap();
    assert_eq!(expected.ai.exploration_destination, Some(at(6, 1)));
    let ai = game.combat.ai[&OWNER].clone();
    let behavior = game.actors[&OWNER].behavior.clone();
    let intention = game.admit_ai_intention(OWNER).unwrap();
    let admitted = game.clone();
    assert!(
        game.cancel_intention_with_step(OWNER, intention, Some(Direction::Down))
            .is_err(),
        "one-layer fixture cannot admit a downward cancellation step"
    );
    assert_eq!(
        game, admitted,
        "rejected atomic cancellation preserves the whole pending proposal"
    );
    game.cancel_intention(OWNER, intention).unwrap();
    assert_eq!(game.combat.ai[&OWNER], ai);
    assert_eq!(game.actors[&OWNER].behavior, behavior);
    let retry = game.choose_ai(OWNER).unwrap();
    assert_eq!(retry.action, expected.action);
    assert_eq!(retry.ai, expected.ai);
    assert_eq!(retry.behavior, expected.behavior);
}

#[test]
fn exploration_underfoot_acquisition_interrupts_without_forgetting_destination() {
    let mut game = fixture();
    let policy = AiProfile {
        gold: true,
        ..Default::default()
    };
    game.configure_ai(OWNER, policy).unwrap();
    game.combat
        .ai
        .get_mut(&OWNER)
        .unwrap()
        .exploration_destination = Some(at(6, 1));
    let mut gold =
        crate::ItemSpec::ordinary("item.test.exploration-gold".parse().unwrap(), "gold".into());
    gold.class = crate::ItemClass::Coin;
    gold.currency = Some(crate::CurrencyKind::Gold);
    gold.stackable = true;
    let item = game.place_item_stack(91, at(2, 1), None, 3, gold).unwrap();
    let before = game.clone();
    let decision = game.choose_ai(OWNER).unwrap();
    assert!(matches!(decision.action, Action::Take { item: taken, .. } if taken == item));
    assert_eq!(decision.ai.exploration_destination, Some(at(6, 1)));
    assert_eq!(decision.behavior.rng, game.actors[&OWNER].behavior.rng);
    assert_eq!(game, before);
}

#[test]
fn exploration_destination_decode_requires_explicit_nullable_strict_location() {
    let game = fixture();
    let source = serde_json::to_value(&game.combat.ai[&OWNER]).unwrap();
    assert!(source.get("exploration_destination").unwrap().is_null());
    assert!(serde_json::from_value::<Ai>(source.clone()).is_ok());
    let mut omitted = source.clone();
    omitted
        .as_object_mut()
        .unwrap()
        .remove("exploration_destination");
    assert!(serde_json::from_value::<Ai>(omitted).is_err());
    let location = serde_json::to_value(at(6, 1)).unwrap();
    let mut present = source.clone();
    present["exploration_destination"] = location;
    assert_eq!(
        serde_json::from_value::<Ai>(present.clone())
            .unwrap()
            .exploration_destination,
        Some(at(6, 1))
    );
    for invalid in [
        serde_json::json!(false),
        serde_json::json!(4),
        serde_json::json!([]),
        serde_json::json!({"region":77}),
        serde_json::json!({"region":77,"position":{"x":6,"y":1,"z":0,"extra":0}}),
        serde_json::json!({"region":77,"position":{"x":6,"y":1,"z":0},"extra":0}),
        serde_json::json!({"region":77,"position":{"x":6,"y":1}}),
    ] {
        let mut malformed = source.clone();
        malformed["exploration_destination"] = invalid;
        assert!(serde_json::from_value::<Ai>(malformed).is_err());
    }
    present["unexpected"] = serde_json::json!(0);
    assert!(serde_json::from_value::<Ai>(present).is_err());
    // from_value cannot retain duplicate keys, so exercise raw JSON separately.
    let encoded = serde_json::to_string(&source).unwrap();
    let duplicate = encoded.replacen(
        "\"exploration_destination\":null",
        "\"exploration_destination\":null,\"exploration_destination\":null",
        1,
    );
    assert_ne!(duplicate, encoded);
    assert!(serde_json::from_str::<Ai>(&duplicate).is_err());
}
// Append only after source freeze. Uses the production 512-node allowance.
#[test]
fn exploration_production_budget_exhaustion_abandons_costly_interest_without_reselection() {
    let mut game = fixture();
    let region = RegionId(88);
    let point = |x, y| Location {
        region,
        position: Position { x, y, z: 0 },
    };
    game.add_region(Region {
        id: region,
        name: "Broad previously surveyed floor".into(),
        bounds: Extent::new(41, 41, 1).unwrap(),
    })
    .unwrap();
    game.set_region_light(region, true).unwrap();
    game.teleport(OWNER, point(1, 1)).unwrap();
    game.refresh_navigation();
    // Explicitly model this actor's prior survey, not a live world lookup by
    // the decision. All ordinary cardinal edges match the empty physical room.
    let navigation = game.navigation.get_mut(&OWNER).unwrap();
    for x in 0..41 {
        for y in 0..41 {
            navigation.cells.insert(point(x, y), false);
            for (direction, dx, dy) in [
                (Direction::North, 0, -1),
                (Direction::East, 1, 0),
                (Direction::South, 0, 1),
                (Direction::West, -1, 0),
            ] {
                if (0..41).contains(&(x + dx)) && (0..41).contains(&(y + dy)) {
                    navigation.edges.insert(
                        (point(x, y), direction),
                        crate::travel::KnownEdge::ordinary(point(x + dx, y + dy), 0),
                    );
                }
            }
        }
    }
    let goal = point(39, 39);
    let view = game.observe(OWNER).unwrap();
    assert!(game.actor_translation(OWNER, Direction::East).is_some());
    assert!(view
        .visible_cells
        .iter()
        .any(|cell| cell.location == point(2, 1) && !cell.wall));
    assert!(
        game.route_search(OWNER).unwrap().route(goal).is_ok(),
        "the remembered goal really is reachable with sufficient work"
    );
    let mut remaining = 512;
    assert_eq!(
        game.route_search(OWNER)
            .unwrap()
            .route_bounded(goal, &mut remaining)
            .unwrap(),
        None,
        "independent search exhausts the actual production allowance"
    );
    assert_eq!(remaining, 0);
    game.combat
        .ai
        .get_mut(&OWNER)
        .unwrap()
        .exploration_destination = Some(goal);
    let before = game.clone();
    let decision = game.choose_ai(OWNER).unwrap();
    assert!(
        matches!(decision.action, Action::Move(_)),
        "exhaustion leaves ordinary local exploration available"
    );
    assert_eq!(decision.ai.exploration_destination, None,
               "the interest exceeds its work allowance, not its reachability; local fallback must not immediately select a replacement");
    assert_eq!(game, before);
}

fn erratic_exploration_fixture(denominator: u16) -> Game {
    let mut species = crate::test_creatures::species();
    species.modifiers.push(crate::rules::Modifier {
        cancellable: false,
        scope: crate::rules::Scope::Always,
        op: crate::rules::ModifierOp::Locomotion {
            value: crate::locomotion::LocomotionValue::Erratic {
                chance: Some(crate::locomotion::MovementChance {
                    numerator: 1,
                    denominator,
                }),
            },
        },
    });
    let mut game = super::tests::configured_game(
        crate::test_creatures::species(),
        species,
        crate::test_creatures::ruleset(),
    );
    game.add_region(Region {
        id: RegionId(77),
        name: "Erratic goal transaction".into(),
        bounds: Extent::new(9, 3, 1).unwrap(),
    })
    .unwrap();
    game.set_region_light(RegionId(77), true).unwrap();
    game.teleport(OWNER, at(2, 1)).unwrap();
    game.configure_run(ActorId(1), BTreeSet::from([ActorId(1)]), BTreeMap::new())
        .unwrap();
    game.configure_ai(OWNER, AiProfile::default()).unwrap();
    game.refresh_navigation();
    game.combat
        .ai
        .get_mut(&OWNER)
        .unwrap()
        .exploration_destination = Some(at(6, 1));
    game.combat
        .ai
        .get_mut(&OWNER)
        .unwrap()
        .visits
        .insert(at(2, 1), 7);
    game.actors.get_mut(&OWNER).unwrap().behavior.rng = 0;
    let view = game.observe(OWNER).unwrap();
    // A fully disclosed eight-direction floor makes the independent seeded
    // oracle exact. No seed search, dynamic candidate oracle or forced action.
    for direction in Direction::HORIZONTAL {
        let (location, _) = game.actor_translation(OWNER, direction).unwrap();
        assert!(view
            .visible_cells
            .iter()
            .any(|cell| cell.location == location && !cell.wall));
    }
    assert!(game.actor_translation(OWNER, Direction::Down).is_none());
    assert!(game.actor_translation(OWNER, Direction::Up).is_none());
    assert_eq!(
        game.route_search(OWNER).unwrap().route(at(6, 1)).unwrap()[0].direction,
        Direction::East
    );
    game
}

fn forced_erratic_rng() -> u64 {
    let mut rng = 0;
    assert_eq!(
        crate::dice::random_word(&mut rng),
        16_294_208_416_658_607_535
    );
    assert_eq!(
        crate::dice::random_word(&mut rng),
        7_960_286_522_194_355_700
    );
    // Second draw modulo eight is index 4, NorthEast in HORIZONTAL.
    rng
}

#[test]
fn exploration_erratic_deviation_releases_goal_but_preserves_visit_history() {
    let game = erratic_exploration_fixture(1);
    let before = game.clone();
    let decision = game.choose_ai(OWNER).unwrap();
    assert_eq!(decision.action, Action::Move(Direction::NorthEast));
    assert_eq!(decision.ai.exploration_destination, None);
    assert_eq!(decision.ai.visits, game.combat.ai[&OWNER].visits);
    assert_eq!(decision.behavior.rng, forced_erratic_rng());
    assert_eq!(
        game.combat.ai[&OWNER].exploration_destination,
        Some(at(6, 1))
    );
    assert_eq!(game.choose_ai(OWNER).unwrap().behavior, decision.behavior);
    assert_eq!(game, before);
}

#[test]
fn exploration_erratic_cancelled_or_rejected_intention_keeps_authoritative_goal() {
    let mut game = erratic_exploration_fixture(1);
    game.act(ActorId(1), Action::Wait).unwrap();
    let expected = game.choose_ai(OWNER).unwrap();
    assert_eq!(expected.action, Action::Move(Direction::NorthEast));
    assert_eq!(expected.ai.exploration_destination, None);
    let ai = game.combat.ai[&OWNER].clone();
    let behavior = game.actors[&OWNER].behavior.clone();
    let intention = game.admit_ai_intention(OWNER).unwrap();
    assert_eq!(
        game.combat.ai[&OWNER], ai,
        "admission cannot commit draft goal release"
    );
    let admitted = game.clone();
    assert!(game
        .cancel_intention_with_step(OWNER, intention, Some(Direction::Down))
        .is_err());
    assert_eq!(game, admitted, "invalid cancellation must be atomic");
    game.cancel_intention(OWNER, intention).unwrap();
    assert_eq!(game.combat.ai[&OWNER], ai);
    assert_eq!(game.actors[&OWNER].behavior, behavior);
    let retry = game.choose_ai(OWNER).unwrap();
    assert_eq!(retry.action, expected.action);
    assert_eq!(retry.ai, expected.ai);
    assert_eq!(retry.behavior, expected.behavior);
}

#[test]
fn exploration_erratic_pending_checkpoint_releases_goal_only_on_execution_commit() {
    let mut game = erratic_exploration_fixture(1);
    game.act(ActorId(1), Action::Wait).unwrap();
    let chosen = game.choose_ai(OWNER).unwrap();
    assert_eq!(chosen.action, Action::Move(Direction::NorthEast));
    assert_eq!(chosen.ai.exploration_destination, None);
    game.admit_ai_intention(OWNER).unwrap();
    assert_eq!(
        game.combat.ai[&OWNER].exploration_destination,
        Some(at(6, 1))
    );
    assert_eq!(game.actors[&OWNER].behavior.rng, 0);
    let mut shared = crate::checkpoint::SharedState::default();
    let snapshot =
        serde_json::from_value(serde_json::to_value(game.checkpoint(&mut shared)).unwrap())
            .unwrap();
    let shared = serde_json::from_value(serde_json::to_value(shared).unwrap()).unwrap();
    let mut restored = Game::restore_checkpoint(snapshot, &shared, game.content().clone()).unwrap();
    assert_eq!(
        restored.combat.ai[&OWNER].exploration_destination,
        Some(at(6, 1))
    );
    for candidate in [&mut game, &mut restored] {
        let execution = candidate.execute_next_intention().unwrap();
        assert_eq!(execution.action, Some(Action::Move(Direction::NorthEast)));
        execution.outcome.unwrap();
        assert_eq!(candidate.actors[&OWNER].location, at(3, 0));
        assert_eq!(candidate.combat.ai[&OWNER].exploration_destination, None);
        assert_eq!(candidate.combat.ai[&OWNER].visits, chosen.ai.visits);
        assert_eq!(candidate.actors[&OWNER].behavior.rng, forced_erratic_rng());
    }
    assert_eq!(game, restored);
}

#[test]
fn exploration_declined_erratic_draw_preserves_goal_and_draws_no_direction() {
    let game = erratic_exploration_fixture(2);
    let before = game.clone();
    let mut expected_rng = 0;
    assert_eq!(
        crate::dice::random_word(&mut expected_rng) % 2,
        1,
        "seed 0 declines 1/2"
    );
    let decision = game.choose_ai(OWNER).unwrap();
    assert_eq!(decision.action, Action::Move(Direction::East));
    assert_eq!(decision.ai.exploration_destination, Some(at(6, 1)));
    assert_eq!(
        decision.behavior.rng, expected_rng,
        "only chance draw; retained goal requires no destination tie draw"
    );
    assert_eq!(game, before);
}

// Append after the tracked-source freeze ends.
#[test]
fn exploration_failed_fresh_tie_scan_preserves_exact_local_fallback_rng() {
    let game = fixture();
    let mut view = game.observe(OWNER).unwrap();
    // Keep only the origin and physically legal adjacent disclosed cells. Every
    // tied destination is too near for the new three-hop minimum, so selection
    // must fail without consuming its speculative cyclic-start draw.
    let adjacent: Vec<_> = Direction::HORIZONTAL
        .into_iter()
        .chain([Direction::Up, Direction::Down])
        .filter_map(|direction| {
            game.actor_translation(OWNER, direction)
                .map(|(location, _)| (location, direction))
        })
        .collect();
    let origin = view.location;
    view.visible_cells.retain(|cell| {
        cell.location == origin
            || adjacent
                .iter()
                .any(|(location, _)| *location == cell.location)
    });
    let mut expected: Vec<_> = adjacent
        .into_iter()
        .filter(|(location, _)| {
            view.visible_cells
                .iter()
                .any(|cell| cell.location == *location && !cell.wall)
        })
        .collect();
    expected.sort_by_key(|(location, _)| *location);
    assert!(
        expected.len() > 1,
        "fixture must exercise both speculative and fallback ties"
    );
    for (location, _) in &expected {
        assert!(
            game.route_search(OWNER)
                .unwrap()
                .route(*location)
                .unwrap()
                .len()
                < 3
        );
    }
    let mut ai = game.combat.ai[&OWNER].clone();
    ai.state = State::Search;
    let mut behavior = game.actors[&OWNER].behavior.clone();
    let mut expected_rng = behavior.rng;
    let index = (crate::dice::random_word(&mut expected_rng) % expected.len() as u64) as usize;
    let mut search = game.route_search(OWNER).unwrap();
    let before = game.clone();
    let chosen = game.choose_exploration(
        OWNER,
        &mut ai,
        &mut behavior,
        false,
        &view,
        Some(&mut search),
    );
    assert_eq!(chosen, Some(Action::Move(expected[index].1)));
    assert_eq!(
        behavior.rng, expected_rng,
        "exactly the original one local tie draw, not two"
    );
    assert_eq!(ai.exploration_destination, None);
    assert_eq!(game, before);
}

#[test]
fn exploration_retained_goal_requires_matching_disclosed_first_step_frame() {
    let game = fixture();
    let view = game.observe(OWNER).unwrap();
    let route = game.route_search(OWNER).unwrap().route(at(6, 1)).unwrap();
    let first = route[0];
    let (location, frame) = game.actor_translation(OWNER, first.direction).unwrap();
    assert_eq!(location, first.destination);
    assert!(view
        .visible_cells
        .iter()
        .any(|cell| cell.location == location && cell.frame == frame && !cell.wall));
    let choose = |observation: &crate::Observation| {
        let mut ai = game.combat.ai[&OWNER].clone();
        ai.state = State::Search;
        ai.exploration_destination = Some(at(6, 1));
        let mut behavior = game.actors[&OWNER].behavior.clone();
        let mut search = game.route_search(OWNER).unwrap();
        let action = game.choose_exploration(
            OWNER,
            &mut ai,
            &mut behavior,
            false,
            observation,
            Some(&mut search),
        );
        (action, ai)
    };
    let (action, positive) = choose(&view);
    assert_eq!(action, Some(Action::Move(first.direction)));
    assert_eq!(positive.exploration_destination, Some(at(6, 1)));
    let mut wrong_frame = view;
    for cell in wrong_frame.visible_cells.iter_mut() {
        if cell.location == location {
            cell.frame = (frame + 1) % 24;
        }
    }
    let before = game.clone();
    let (_, rejected) = choose(&wrong_frame);
    assert_eq!(
        rejected.exploration_destination, None,
        "location alone cannot license continuation through another disclosed occurrence"
    );
    // Local fallback intentionally preserves its older location-only contract;
    // rejection here concerns the new retained goal, not a forced Wait.
    assert_eq!(game, before);
}
// Append after tracked-source freeze. Height is essential to this regression.
#[test]
fn exploration_unreachable_disclosed_air_does_not_starve_visited_floor_interest() {
    let mut game = fixture();
    let region = RegionId(89);
    let point = |x, y, z| Location {
        region,
        position: Position { x, y, z },
    };
    game.add_region(Region {
        id: region,
        name: "Tall disclosed room with unreachable air".into(),
        bounds: Extent::new(9, 3, 4).unwrap(),
    })
    .unwrap();
    game.set_region_light(region, true).unwrap();
    game.teleport(OWNER, point(2, 1, 0)).unwrap();
    game.refresh_navigation();
    assert!(!game.actors[&OWNER].locomotion().flying);
    let view = game.observe(OWNER).unwrap();
    let air = point(6, 1, 1);
    let floor = point(6, 1, 0);
    for location in [air, floor] {
        assert!(
            view.visible_cells
                .iter()
                .any(|cell| cell.location == location && !cell.wall),
            "both candidate tiers must be genuinely disclosed"
        );
        assert_eq!(game.navigation[&OWNER].cells.get(&location), Some(&false));
    }
    assert!(
        game.route_search(OWNER).unwrap().route(air).is_err(),
        "ground species cannot route to ordinary upper air despite disclosure"
    );
    let route = game.route_search(OWNER).unwrap().route(floor).unwrap();
    assert_eq!(route.len(), 4);
    assert!(game.actor_translation(OWNER, route[0].direction).is_some());
    {
        let ai = game.combat.ai.get_mut(&OWNER).unwrap();
        for cell in view.visible_cells.iter() {
            // Keep all unreachable air at tier0, a single useful floor at tier1,
            // and the remaining floor at tier2. Tier sampling must reach tier1
            // even when there are more than16 less-visited air candidates.
            let visits = if cell.location.position.z > 0 {
                0
            } else if cell.location == floor {
                1
            } else {
                2
            };
            ai.visits.insert(cell.location, visits);
        }
    }
    let air_count = view
        .visible_cells
        .iter()
        .filter(|cell| !cell.wall && cell.location.position.z > 0)
        .count();
    assert!(
        air_count > 16,
        "fixture must exceed the per-decision candidate allowance"
    );
    let before = game.clone();
    let choice = game.choose_ai(OWNER).unwrap();
    assert_eq!(
        choice.ai.exploration_destination,
        Some(floor),
        "unreachable least-visited air cannot exclude the next reachable tier"
    );
    assert_eq!(choice.action, Action::Move(route[0].direction));
    assert_eq!(game, before);
}
// Append after tracked-source freeze. This is distinct from the height-only
// starvation regression: the first target exhausts, rather than completes,
// the remembered graph search, and the useful second target is already settled.
#[test]
fn exploration_later_tier_reuses_settled_route_after_production_budget_exhaustion() {
    let mut game = fixture();
    let region = RegionId(90);
    let point = |x, y, z| Location {
        region,
        position: Position { x, y, z },
    };
    game.add_region(Region {
        id: region,
        name: "Tall previously surveyed broad floor".into(),
        bounds: Extent::new(41, 41, 4).unwrap(),
    })
    .unwrap();
    game.set_region_light(region, true).unwrap();
    game.teleport(OWNER, point(1, 1, 0)).unwrap();
    game.refresh_navigation();
    assert!(!game.actors[&OWNER].locomotion().flying);
    // Populate only ground edges as this actor's explicit prior survey. Actual
    // current perception supplies the disclosed upper-air cells independently.
    let navigation = game.navigation.get_mut(&OWNER).unwrap();
    for x in 0..41 {
        for y in 0..41 {
            navigation.cells.insert(point(x, y, 0), false);
            for (direction, dx, dy) in [
                (Direction::North, 0, -1),
                (Direction::East, 1, 0),
                (Direction::South, 0, 1),
                (Direction::West, -1, 0),
            ] {
                if (0..41).contains(&(x + dx)) && (0..41).contains(&(y + dy)) {
                    navigation.edges.insert(
                        (point(x, y, 0), direction),
                        crate::travel::KnownEdge::ordinary(point(x + dx, y + dy, 0), 0),
                    );
                }
            }
        }
    }
    let air = point(5, 1, 1);
    let floor = point(5, 1, 0);
    let view = game.observe(OWNER).unwrap();
    for location in [air, floor] {
        assert!(view
            .visible_cells
            .iter()
            .any(|cell| cell.location == location && !cell.wall));
        assert_eq!(game.navigation[&OWNER].cells.get(&location), Some(&false));
    }
    let mut search = game.route_search(OWNER).unwrap();
    let mut remaining = 512;
    assert_eq!(search.route_bounded(air, &mut remaining).unwrap(), None);
    assert_eq!(remaining, 0);
    let settled = search
        .route_bounded(floor, &mut remaining)
        .unwrap()
        .unwrap();
    assert_eq!(settled.len(), 4);
    assert_eq!(
        remaining, 0,
        "cached floor route needs no additional settled node"
    );
    assert!(game
        .actor_translation(OWNER, settled[0].direction)
        .is_some());
    assert!(
        game.route_search(OWNER).unwrap().route(air).is_err(),
        "the eventual completed search confirms unreachable air"
    );
    {
        let ai = game.combat.ai.get_mut(&OWNER).unwrap();
        for cell in view.visible_cells.iter() {
            // A unique air target makes the exhausting first query exact,
            // independent of tie RNG; the next tier is the useful floor.
            ai.visits.insert(
                cell.location,
                if cell.location == air {
                    0
                } else if cell.location == floor {
                    1
                } else {
                    2
                },
            );
        }
    }
    let before = game.clone();
    let decision = game.choose_ai(OWNER).unwrap();
    assert_eq!(decision.ai.exploration_destination, Some(floor));
    assert_eq!(decision.action, Action::Move(settled[0].direction));
    assert_eq!(
        decision.behavior.rng, game.actors[&OWNER].behavior.rng,
        "both sampled tiers have unique candidates"
    );
    assert_eq!(game, before);
}
// Append only after the current tracked-source freeze.
#[test]
fn exploration_keeps_searching_lower_tier_after_provisional_visited_floor_success() {
    let mut game = fixture();
    let region = RegionId(91);
    let point = |x, y, z| Location {
        region,
        position: Position { x, y, z },
    };
    game.add_region(Region {
        id: region,
        name: "Unvisited floor hidden behind sampled air candidate".into(),
        bounds: Extent::new(9, 3, 4).unwrap(),
    })
    .unwrap();
    game.set_region_light(region, true).unwrap();
    game.teleport(OWNER, point(2, 1, 0)).unwrap();
    game.refresh_navigation();
    let air = point(6, 1, 1);
    let unvisited_floor = point(6, 1, 0);
    let visited_floor = point(5, 1, 0);
    let view = game.observe(OWNER).unwrap();
    for location in [air, unvisited_floor, visited_floor] {
        assert!(view
            .visible_cells
            .iter()
            .any(|cell| cell.location == location && !cell.wall));
        assert_eq!(game.navigation[&OWNER].cells.get(&location), Some(&false));
    }
    assert!(game.route_search(OWNER).unwrap().route(air).is_err());
    let better_route = game
        .route_search(OWNER)
        .unwrap()
        .route(unvisited_floor)
        .unwrap();
    let provisional_route = game
        .route_search(OWNER)
        .unwrap()
        .route(visited_floor)
        .unwrap();
    assert_eq!(better_route.len(), 4);
    assert_eq!(provisional_route.len(), 3);
    for route in [&better_route, &provisional_route] {
        assert!(game.actor_translation(OWNER, route[0].direction).is_some());
    }
    let mut minimum_tier = [air, unvisited_floor];
    minimum_tier.sort();
    let air_index = minimum_tier
        .iter()
        .position(|location| *location == air)
        .unwrap();
    let seed = (0..64u64)
        .find(|seed| {
            let mut rng = *seed;
            (crate::dice::random_word(&mut rng) % 2) as usize == air_index
        })
        .expect("a seed must select the unreachable air first");
    game.actors.get_mut(&OWNER).unwrap().behavior.rng = seed;
    let mut expected_rng = seed;
    assert_eq!(
        (crate::dice::random_word(&mut expected_rng) % 2) as usize,
        air_index
    );
    {
        let ai = game.combat.ai.get_mut(&OWNER).unwrap();
        for cell in view.visible_cells.iter() {
            let visits = if [air, unvisited_floor].contains(&cell.location) {
                0
            } else if cell.location == visited_floor {
                1
            } else {
                2
            };
            ai.visits.insert(cell.location, visits);
        }
    }
    let before = game.clone();
    let decision = game.choose_ai(OWNER).unwrap();
    assert_eq!(
        decision.ai.exploration_destination,
        Some(unvisited_floor),
        "first sampled air fails, visited floor is provisional, later unvisited floor must win"
    );
    assert_eq!(decision.action, Action::Move(better_route[0].direction));
    assert_eq!(
        decision.behavior.rng, expected_rng,
        "skip equal/higher tiers after success; revisiting the same minimum tier draws no more"
    );
    assert_eq!(game, before);
}
// Append to ai_exploration_tests.rs after the build-input freeze ends.
fn interior_corridor(
    left_visits: [u64; 2],
    right_visits: [u64; 4],
    endpoints: [u64; 2],
    first: usize,
) -> (Game, Location, Location, u64) {
    let mut game = fixture();
    let region = RegionId(92);
    let point = |x| Location {
        region,
        position: Position { x, y: 0, z: 0 },
    };
    game.add_region(Region {
        id: region,
        name: "Unequal straight exploration routes".into(),
        bounds: Extent::new(13, 1, 4).unwrap(),
    })
    .unwrap();
    game.set_region_light(region, true).unwrap();
    game.teleport(OWNER, point(5)).unwrap();
    game.refresh_navigation();
    let left = point(2);
    let right = point(10);
    let view = game.observe(OWNER).unwrap();
    for goal in [left, right] {
        assert!(view
            .visible_cells
            .iter()
            .any(|cell| cell.location == goal && !cell.wall));
    }
    // Independent geometric oracle: a one-cell-wide ground corridor has precisely
    // these straight routes. No expected score is obtained from the selector.
    let left_route = game.route_search(OWNER).unwrap().route(left).unwrap();
    let right_route = game.route_search(OWNER).unwrap().route(right).unwrap();
    assert_eq!(
        left_route
            .iter()
            .map(|step| step.destination)
            .collect::<Vec<_>>(),
        vec![point(4), point(3), point(2)]
    );
    assert_eq!(
        right_route
            .iter()
            .map(|step| step.destination)
            .collect::<Vec<_>>(),
        vec![point(6), point(7), point(8), point(9), point(10)]
    );
    assert!(game.actor_translation(OWNER, Direction::West).is_some());
    assert!(game.actor_translation(OWNER, Direction::East).is_some());
    {
        let ai = game.combat.ai.get_mut(&OWNER).unwrap();
        for cell in view.visible_cells.iter() {
            ai.visits.insert(cell.location, 100);
        }
        for (x, count) in [4, 3].into_iter().zip(left_visits) {
            ai.visits.insert(point(x), count);
        }
        for (x, count) in [6, 7, 8, 9].into_iter().zip(right_visits) {
            ai.visits.insert(point(x), count);
        }
        ai.visits.insert(left, endpoints[0]);
        ai.visits.insert(right, endpoints[1]);
    }
    let seed = (0..64u64)
        .find(|seed| {
            let mut rng = *seed;
            (crate::dice::random_word(&mut rng) % 2) as usize == first
        })
        .unwrap();
    game.actors.get_mut(&OWNER).unwrap().behavior.rng = seed;
    let mut drawn = seed;
    assert_eq!((crate::dice::random_word(&mut drawn) % 2) as usize, first);
    (game, left, right, drawn)
}

#[test]
fn exploration_equal_endpoint_tier_prefers_lower_mean_interior_not_lower_sum() {
    // Left:8/2=4, right:12/4=3. Raw sum would incorrectly prefer the short left
    // route. Seed deliberately samples left first; current first-success policy
    // must fail at the goal assertion after all corridor positives pass.
    let (game, left, right, expected_rng) = interior_corridor([4, 4], [3; 4], [0, 0], 0);
    assert!(left < right);
    let before = game.clone();
    let choice = game.choose_ai(OWNER).unwrap();
    assert_eq!(choice.ai.exploration_destination, Some(right));
    assert_eq!(choice.action, Action::Move(Direction::East));
    assert_eq!(choice.behavior.rng, expected_rng);
    assert_eq!(game, before);
}

#[test]
fn exploration_interior_mean_tie_keeps_first_seeded_goal_and_excludes_endpoint() {
    // Both means are4. Including the zero-visit endpoint would make the short
    // left route8/3 beat right16/5, violating the interior-only tie contract.
    let (game, _, right, expected_rng) = interior_corridor([4; 2], [4; 4], [0, 0], 1);
    let before = game.clone();
    let choice = game.choose_ai(OWNER).unwrap();
    assert_eq!(choice.ai.exploration_destination, Some(right));
    assert_eq!(choice.action, Action::Move(Direction::East));
    assert_eq!(choice.behavior.rng, expected_rng);
    assert_eq!(game, before);
}

#[test]
fn exploration_endpoint_visits_remain_primary_over_interior_mean() {
    // Unique endpoint tier2 beats tier3 despite interior9 versus4. All interior
    // visits exceed endpoint counts, so no accidental lower-tier waypoint can
    // become the selected destination.
    let (game, left, _, _) = interior_corridor([9; 2], [4; 4], [2, 3], 0);
    let before = game.clone();
    let choice = game.choose_ai(OWNER).unwrap();
    assert_eq!(choice.ai.exploration_destination, Some(left));
    assert_eq!(choice.action, Action::Move(Direction::West));
    assert_eq!(
        choice.behavior.rng, game.actors[&OWNER].behavior.rng,
        "unique best endpoint tier has no tie draw"
    );
    assert_eq!(game, before);
}

#[test]
fn exploration_zero_interior_score_cannot_stop_at_a_higher_endpoint_tier() {
    let (mut game, left, right, _) = interior_corridor([4; 2], [0; 4], [0, 1], 0);
    let origin = game.actors[&OWNER].location;
    let air = Location {
        position: Position {
            z: 1,
            ..left.position
        },
        ..left
    };
    let left_first = Location {
        position: Position {
            x: 4,
            ..origin.position
        },
        ..origin
    };
    let right_first = Location {
        position: Position {
            x: 6,
            ..origin.position
        },
        ..origin
    };
    let mut view = game.observe(OWNER).unwrap();
    assert!(view
        .visible_cells
        .iter()
        .any(|cell| cell.location == air && !cell.wall));
    assert!(game.route_search(OWNER).unwrap().route(air).is_err());
    game.combat
        .ai
        .get_mut(&OWNER)
        .unwrap()
        .visits
        .insert(air, 0);
    // Synthetic disclosure isolates the nominated goals and both legal first
    // steps. Remembered routes remain the independently verified corridor paths;
    // no claim is made that this subset is the complete physical observation.
    view.visible_cells.retain(|cell| {
        [origin, air, left, right, left_first, right_first].contains(&cell.location)
    });
    let mut minimum = [left, air, right_first];
    minimum.sort();
    let air_index = minimum.iter().position(|at| *at == air).unwrap();
    let seed = (0..128u64)
        .find(|seed| {
            let mut rng = *seed;
            (crate::dice::random_word(&mut rng) % 3) as usize == air_index
        })
        .unwrap();
    game.actors.get_mut(&OWNER).unwrap().behavior.rng = seed;
    let mut ai = game.combat.ai[&OWNER].clone();
    ai.state = State::Search;
    let mut behavior = game.actors[&OWNER].behavior.clone();
    let mut search = game.route_search(OWNER).unwrap();
    let before = game.clone();
    let action = game.choose_exploration(
        OWNER,
        &mut ai,
        &mut behavior,
        false,
        &view,
        Some(&mut search),
    );
    assert_eq!(
        ai.exploration_destination,
        Some(left),
        "tier1 right goal has zero mean but must remain provisional until tier0 is examined"
    );
    assert_eq!(action, Some(Action::Move(Direction::West)));
    let mut expected_rng = seed;
    crate::dice::random_word(&mut expected_rng);
    assert_eq!(behavior.rng, expected_rng);
    assert_eq!(game, before);
}

#[test]
fn exploration_interior_mean_handles_saturated_visits_without_overflow() {
    // Both means equal u64::MAX, despite unequal interior lengths. Summation
    // and cross-products must remain exact, retaining the first seeded tie.
    let (game, left, _, expected_rng) = interior_corridor([u64::MAX; 2], [u64::MAX; 4], [0, 0], 0);
    let before = game.clone();
    let choice = game.choose_ai(OWNER).unwrap();
    assert_eq!(choice.ai.exploration_destination, Some(left));
    assert_eq!(choice.action, Action::Move(Direction::West));
    assert_eq!(choice.behavior.rng, expected_rng);
    assert_eq!(game, before);
}
