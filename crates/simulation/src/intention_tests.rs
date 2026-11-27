use super::*;
use crate::{Action, ActorId, Game, GameError};
use std::num::NonZeroU64;
use tor_world::{Direction, Location, Position, RegionId};

fn fixture() -> (Game, ActorId) {
    let mut game = Game::new(
        (*Game::two_room(42, crate::test_creatures::content()).world).clone(),
        42,
        crate::test_creatures::content(),
    );
    let actor = game
        .spawn_actor(
            Location {
                region: RegionId(1),
                position: Position { x: 1, y: 1, z: 0 },
            },
            NonZeroU64::new(100).unwrap(),
        )
        .unwrap();
    (game, actor)
}

#[test]
fn input_controls_match_queue_and_preparation_operations_without_mutating_state() {
    let (mut game, actor, _, original) = paused_attack_fixture();
    let paused = game.clone();
    let input = game.intention_input(actor);
    let controls: Vec<_> = input.controls().collect();
    assert!(input.slot_available);
    assert_eq!(controls.len(), 1);
    assert_eq!(controls[0].intention, original);
    assert_eq!(controls[0].state, IntentionControlState::Paused);
    assert!(controls[0].can_resume);
    assert!(controls[0].can_cancel);
    assert_eq!(
        game, paused,
        "query preserves time, RNG, preparation and identities"
    );

    let independent = game
        .admit_intention(actor, Action::Wait, IntentionOrigin::Human)
        .unwrap();
    let before = game.clone();
    let input = game.intention_input(actor);
    let controls: Vec<_> = input.controls().collect();
    assert!(!input.slot_available);
    assert_eq!(controls.len(), 2);
    assert_eq!(controls[0].intention, independent);
    assert_eq!(controls[0].state, IntentionControlState::Queued);
    assert!(!controls[0].can_resume);
    assert!(controls[0].can_cancel);
    assert_eq!(controls[1].intention, original);
    assert!(!controls[1].can_resume);
    assert!(controls[1].can_cancel);
    assert_eq!(game, before);
    for control in controls {
        let mut resumed = game.clone();
        assert_eq!(
            resumed.resume_intention(actor, control.intention).is_ok(),
            control.can_resume
        );
        if !control.can_resume {
            assert_eq!(resumed, before);
        }
        let mut cancelled = game.clone();
        assert_eq!(
            cancelled.cancel_intention(actor, control.intention).is_ok(),
            control.can_cancel
        );
    }
    game.suspend_intention(actor, independent).unwrap();
    let input = game.intention_input(actor);
    let controls: Vec<_> = input.controls().collect();
    assert_eq!(controls[0].state, IntentionControlState::Suspended);
    assert!(controls[0].can_resume);
    game.resume_intention(actor, independent).unwrap();
    game.cancel_intention(actor, independent).unwrap();
    assert_eq!(game.intention_input(actor).controls().count(), 1);
    game.resume_intention(actor, original).unwrap();
    let input = game.intention_input(actor);
    let controls: Vec<_> = input.controls().collect();
    assert_eq!(
        controls.len(),
        1,
        "queued continuation and preparation share one identity"
    );
    assert_eq!(controls[0].state, IntentionControlState::Queued);
    assert!(!controls[0].can_resume);
    assert!(controls[0].can_cancel);
}

#[test]
fn travel_origin_queues_only_movement_and_preserves_its_checkpoint_identity() {
    let origin: IntentionOrigin = serde_json::from_str("\"travel\"").unwrap();
    let (mut game, actor) = fixture();
    let before = game.clone();
    assert_eq!(
        game.admit_intention(actor, Action::Wait, origin),
        Err(GameError::Schedule(crate::ScheduleError::InvalidIntention))
    );
    assert_eq!(game, before);
    let id = game
        .admit_intention(actor, Action::Move(Direction::East), origin)
        .unwrap();
    assert_eq!(game.tick(), before.tick());
    assert_eq!(game.actors[&actor].location, before.actors[&actor].location);
    let mut shared = crate::checkpoint::SharedState::default();
    let snapshot = game.checkpoint(&mut shared);
    let mut restored = Game::restore_checkpoint(snapshot, &shared, game.content().clone()).unwrap();
    assert_eq!(restored.pending_intention(actor).unwrap().id, id);
    assert_eq!(restored.pending_intention(actor).unwrap().origin, origin);
    assert_eq!(
        restored.execute_next_intention(),
        game.execute_next_intention()
    );
    assert_eq!(restored, game);
    assert_eq!(game.tick(), 100);
}

#[test]
fn planned_travel_destination_is_checked_without_consuming_queue_identity() {
    let (mut game, actor) = fixture();
    let before = game.clone();
    let wrong = crate::TravelStep {
        direction: Direction::East,
        destination: Location {
            region: RegionId(2),
            position: Position { x: 2, y: 1, z: 0 },
        },
    };
    assert_eq!(
        game.admit_travel_intention(actor, wrong),
        Err(GameError::InvalidLocation)
    );
    assert_eq!(game, before);
    let step = crate::TravelStep {
        destination: Location {
            region: RegionId(1),
            ..wrong.destination
        },
        ..wrong
    };
    let id = game.admit_travel_intention(actor, step).unwrap();
    assert_eq!(id, IntentionId(1));
    assert_eq!(
        game.pending_intention(actor).unwrap().origin,
        IntentionOrigin::Travel
    );
    let mut cancelled = game.clone();
    cancelled.cancel_intention(actor, id).unwrap();
    assert_eq!(cancelled.tick(), before.tick());
    assert!(cancelled.pending_intention(actor).is_none());
    assert!(game.execute_next_intention().unwrap().outcome.is_ok());
    assert_eq!(game.tick(), 100);
    assert_eq!(game.actors[&actor].location, step.destination);
}

#[test]
fn queued_move_does_not_reinterpret_direction_after_teleport_or_frame_change() {
    for teleport in [true, false] {
        let (mut game, actor) = fixture();
        let id = game
            .admit_intention(actor, Action::Move(Direction::East), IntentionOrigin::Human)
            .unwrap();
        if teleport {
            game.teleport(
                actor,
                Location {
                    region: RegionId(1),
                    position: Position { x: 3, y: 1, z: 0 },
                },
            )
            .unwrap();
        } else {
            game.actors.get_mut(&actor).unwrap().orientation = 1;
        }
        let mut expected = game.clone();
        expected.cancel_intention(actor, id).unwrap();
        let execution = game.execute_next_intention().unwrap();
        assert_eq!(execution.intention.id, id);
        assert_eq!(execution.outcome, Err(GameError::InvalidLocation));
        assert_eq!(
            game, expected,
            "failed guard must not move time, state or RNG"
        );
    }
}

#[test]
fn saved_movement_context_is_required_and_preserves_execution() {
    let (mut game, actor) = fixture();
    game.admit_intention(actor, Action::Move(Direction::East), IntentionOrigin::Human)
        .unwrap();
    let mut shared = crate::checkpoint::SharedState::default();
    let snapshot = game.checkpoint(&mut shared);
    let original = serde_json::to_value((snapshot, shared)).unwrap();
    let (snapshot, shared) = serde_json::from_value(original.clone()).unwrap();
    let mut restored = Game::restore_checkpoint(snapshot, &shared, game.content().clone()).unwrap();
    assert_eq!(
        restored.execute_next_intention(),
        game.execute_next_intention()
    );
    assert_eq!(restored, game);
    // An absent context is representable for non-movement intentions, but
    // cannot restore this queued move. Malformed present contexts fail at
    // the current strict decoder before checkpoint admission is reached.
    let mut absent = original.clone();
    absent[1]["intentions"]["entries"][0]["movement_context"] = serde_json::Value::Null;
    let (snapshot, shared) = serde_json::from_value(absent).unwrap();
    assert!(Game::restore_checkpoint(snapshot, &shared, game.content().clone()).is_err());
    let mut malformed = original.clone();
    malformed[1]["intentions"]["entries"][0]["movement_context"] = serde_json::json!({
        "from": {"region": 0, "position": {"x": 1, "y": 1, "z": 0}},
        "orientation": 0, "to": {"region": 1, "position": {"x": 2, "y": 1, "z": 0}},
        "next_orientation": 24
    });
    assert!(
        serde_json::from_value::<(crate::checkpoint::Snapshot, crate::checkpoint::SharedState)>(
            malformed
        )
        .is_err()
    );
    let mut missing = original;
    missing[1]["intentions"]["entries"][0]
        .as_object_mut()
        .unwrap()
        .remove("movement_context");
    assert!(
        serde_json::from_value::<(crate::checkpoint::Snapshot, crate::checkpoint::SharedState)>(
            missing
        )
        .is_err()
    );
}

#[test]
fn queued_move_preserves_rotated_portal_destination_and_frame() {
    use tor_world::{Extent, Passage, Region, World};
    for origin in [IntentionOrigin::Human, IntentionOrigin::Travel] {
        let mut world = World::new(
            (1..=2)
                .map(|id| Region {
                    id: RegionId(id),
                    name: id.to_string(),
                    bounds: Extent::new(3, 3, 1).unwrap(),
                })
                .collect(),
            vec![],
        )
        .unwrap();
        let from = Location {
            region: RegionId(1),
            position: Position { x: 2, y: 1, z: 0 },
        };
        let to = Location {
            region: RegionId(2),
            position: Position { x: 1, y: 0, z: 0 },
        };
        world
            .connect(
                Passage {
                    from,
                    direction: Direction::East,
                    to,
                },
                1,
            )
            .unwrap();
        let mut game = Game::new(world, 42, crate::test_creatures::content());
        let actor = game
            .spawn_actor(from, NonZeroU64::new(100).unwrap())
            .unwrap();
        let id = if origin == IntentionOrigin::Travel {
            game.admit_travel_intention(
                actor,
                crate::TravelStep {
                    direction: Direction::East,
                    destination: to,
                },
            )
        } else {
            game.admit_intention(actor, Action::Move(Direction::East), origin)
        }
        .unwrap();
        let context = game
            .pending_intention(actor)
            .unwrap()
            .movement_context
            .unwrap();
        assert_eq!((context.to, context.next_orientation), (to, 1));
        let mut remapped = game.clone();
        let mut changed_world = World::new(
            (1..=2)
                .map(|id| Region {
                    id: RegionId(id),
                    name: id.to_string(),
                    bounds: Extent::new(3, 3, 1).unwrap(),
                })
                .collect(),
            vec![],
        )
        .unwrap();
        changed_world
            .connect(
                Passage {
                    from,
                    direction: Direction::East,
                    to,
                },
                3,
            )
            .unwrap();
        remapped.world = tor_world::Shared::new(changed_world);
        assert!(
            remapped.actor_translation(actor, Direction::East).is_some(),
            "the changed portal still permits movement, but has a different meaning"
        );
        let mut rejected = remapped.clone();
        rejected.cancel_intention(actor, id).unwrap();
        assert_eq!(
            remapped.execute_next_intention().unwrap().outcome,
            Err(GameError::InvalidLocation)
        );
        assert_eq!(remapped, rejected);
        let mut expected = game.clone();
        expected.cancel_intention(actor, id).unwrap();
        expected.act(actor, Action::Move(Direction::East)).unwrap();
        assert!(game.execute_next_intention().unwrap().outcome.is_ok());
        assert_eq!(game.actors[&actor].location, to);
        assert_eq!(game.actors[&actor].orientation, 1);
        assert_eq!(game, expected);
    }
}

fn paused_attack_fixture() -> (Game, ActorId, ActorId, IntentionId) {
    let (mut game, actor) = fixture();
    let target = game
        .spawn_actor(
            Location {
                region: RegionId(1),
                position: Position { x: 2, y: 1, z: 0 },
            },
            NonZeroU64::new(100).unwrap(),
        )
        .unwrap();
    for id in [actor, target] {
        crate::test_creatures::configure(
            &mut game,
            id,
            "neutral",
            crate::test_creatures::species(),
        );
    }
    assert!(
        game.creature(actor).is_some(),
        "queued subjects own their builds"
    );
    game.actors.get_mut(&target).unwrap().ready_at = 50;
    let intention = game
        .admit_intention(
            actor,
            Action::UseAbility {
                id: crate::content_ids::AbilityId::new("basic_melee").unwrap(),
                invocation: crate::abilities::AbilityInvocation::actor_target(target),
                supplies: Default::default(),
            },
            IntentionOrigin::Human,
        )
        .unwrap();
    game.execute_next_intention().unwrap().outcome.unwrap();
    let full_wind_up = game.preparation(actor).unwrap().remaining;
    game.pause_preparation(actor).unwrap();
    let preparation = game.preparation(actor).unwrap().clone();
    assert!(preparation.remaining > 0 && preparation.remaining < full_wind_up);
    (game, actor, target, intention)
}

#[test]
fn paused_attack_resume_queues_original_identity_without_restarting_progress() {
    let (mut game, actor, target, intention) = paused_attack_fixture();
    let preparation = game.preparation(actor).unwrap().clone();
    let tick = game.tick();
    let combat = game.combat.clone();
    game.resume_intention(actor, intention).unwrap();
    assert_eq!(
        game.tick(),
        tick,
        "resume admits work without advancing time"
    );
    assert_eq!(
        game.combat, combat,
        "admission preserves combat and RNG state"
    );
    assert_eq!(game.preparation(actor), Some(&preparation));
    assert_eq!(game.pending_intention(actor).unwrap().id, intention);
    let mut shared = crate::checkpoint::SharedState::default();
    let snapshot = game.checkpoint(&mut shared);
    let mut restored = Game::restore_checkpoint(snapshot, &shared, game.content().clone()).unwrap();
    assert_eq!(restored, game);
    let execution = restored.execute_next_intention().unwrap();
    assert_eq!(execution.intention.id, intention);
    execution.outcome.unwrap();
    if let Some(progress) = restored.preparation(actor) {
        assert_eq!(progress.intention, Some(intention));
        assert_eq!(progress.plan.work().target(), Some(target));
        assert!(progress.remaining <= preparation.remaining);
        assert!(progress.active);
    } else {
        assert!(restored.combat_events().iter().any(|event| matches!(event,
                crate::combat::CombatEvent::Resolved { outcome, .. } if outcome.intention == Some(intention))));
    }
}

#[test]
fn cancelling_original_progress_preserves_independent_queued_work() {
    let (mut game, actor, _, original) = paused_attack_fixture();
    let later = game
        .admit_intention(actor, Action::Wait, IntentionOrigin::Human)
        .unwrap();
    let queued = game.pending_intention(actor).unwrap().clone();
    let next_id = game.intentions.next_id;
    game.cancel_intention(actor, original).unwrap();
    assert!(game.preparation(actor).is_none());
    assert_eq!(game.pending_intention(actor), Some(&queued));
    assert_eq!(game.intentions.next_id, next_id);
    let executed = game.execute_next_intention().unwrap();
    assert_eq!(executed.intention.id, later);
    executed.outcome.unwrap();
}

#[test]
fn cancelling_paused_or_queued_continuation_removes_original_progress_without_time() {
    for queued in [false, true] {
        let (mut game, actor, _, intention) = paused_attack_fixture();
        if queued {
            game.resume_intention(actor, intention).unwrap();
        }
        let before = game.clone();
        assert!(game
            .cancel_intention(actor, IntentionId(intention.0 + 1))
            .is_err());
        assert_eq!(game, before, "wrong identity must preserve progress");
        game.cancel_intention(actor, intention).unwrap();
        assert_eq!(game.tick(), before.tick());
        assert_eq!(
            game.combat, before.combat,
            "cancellation preserves RNG and combat events"
        );
        assert_eq!(game.intentions.next_id, before.intentions.next_id);
        assert!(game.preparation(actor).is_none());
        assert!(game.pending_intention(actor).is_none());
        assert!(game.resume_intention(actor, intention).is_err());
        assert!(game.cancel_intention(actor, intention).is_err());
        let mut shared = crate::checkpoint::SharedState::default();
        let snapshot = game.checkpoint(&mut shared);
        assert_eq!(
            Game::restore_checkpoint(snapshot, &shared, game.content().clone()).ok(),
            Some(game)
        );
    }
}

#[test]
fn failed_continuation_concludes_original_progress_without_retargeting() {
    let (mut game, actor, target, intention) = paused_attack_fixture();
    game.resume_intention(actor, intention).unwrap();
    game.teleport(
        target,
        Location {
            region: RegionId(1),
            position: Position { x: 3, y: 1, z: 0 },
        },
    )
    .unwrap();
    let alternate = game
        .spawn_actor(
            Location {
                region: RegionId(1),
                position: Position { x: 2, y: 1, z: 0 },
            },
            NonZeroU64::new(100).unwrap(),
        )
        .unwrap();
    crate::test_creatures::configure(
        &mut game,
        alternate,
        "neutral",
        crate::test_creatures::species(),
    );
    let tick = game.tick();
    let health = game.health(alternate);
    let rng = game.rules_rng;
    let execution = game.execute_next_intention().unwrap();
    assert_eq!(execution.intention.id, intention);
    assert_eq!(
        execution.outcome,
        Err(GameError::Rule(crate::abilities::RuleError::OutOfReach))
    );
    assert_eq!(game.tick(), tick);
    assert_eq!(game.health(alternate), health);
    assert_eq!(game.rules_rng, rng);
    assert!(
        game.preparation(actor).is_none(),
        "failed continuation is terminal"
    );
    assert!(game.pending_intention(actor).is_none());
}

#[test]
fn admitted_attack_identity_survives_wind_up_restore_and_impact() {
    let (mut game, actor) = fixture();
    let target = game
        .spawn_actor(
            Location {
                region: RegionId(1),
                position: Position { x: 2, y: 1, z: 0 },
            },
            NonZeroU64::new(100).unwrap(),
        )
        .unwrap();
    for id in [actor, target] {
        crate::test_creatures::configure(
            &mut game,
            id,
            "neutral",
            crate::test_creatures::species(),
        );
    }
    let intention = game
        .admit_intention(
            actor,
            Action::UseAbility {
                id: crate::content_ids::AbilityId::new("basic_melee").unwrap(),
                invocation: crate::abilities::AbilityInvocation::actor_target(target),
                supplies: Default::default(),
            },
            IntentionOrigin::Human,
        )
        .unwrap();
    game.execute_next_intention().unwrap().outcome.unwrap();
    let preparation = game
        .preparation(actor)
        .expect("target's turn holds wind-up open");
    assert_eq!(
        serde_json::to_value(preparation).unwrap()["intention"],
        serde_json::json!(intention)
    );
    let mut missing = serde_json::to_value(preparation).unwrap();
    missing.as_object_mut().unwrap().remove("intention");
    assert!(
        serde_json::from_value::<crate::combat::Preparation>(missing).is_err(),
        "saved progress requires an explicit admission context, including null for direct calls"
    );
    let mut shared = crate::checkpoint::SharedState::default();
    let snapshot = game.checkpoint(&mut shared);
    for corrupt in [0, intention.0 + 1] {
        let value = serde_json::to_value(&snapshot).unwrap();
        let mut pool = serde_json::to_value(&shared).unwrap();
        let actors = value["actors"].as_u64().unwrap() as usize;
        crate::checkpoint_fixture::actor_mut(&mut pool, actors, "1")["pending"]["intention"] =
            serde_json::json!(corrupt);
        let invalid_shared = serde_json::from_value(pool).unwrap();
        assert!(Game::restore_checkpoint(
            serde_json::from_value(value).unwrap(),
            &invalid_shared,
            game.content().clone()
        )
        .is_err());
    }
    let mut restored = Game::restore_checkpoint(snapshot, &shared, game.content().clone()).unwrap();
    assert_eq!(restored, game);
    restored.act(target, Action::Wait).unwrap();
    assert!(restored.preparation(actor).is_none());
    let resolved = restored
        .combat_events()
        .iter()
        .find_map(|event| {
            let value = serde_json::to_value(event).unwrap();
            value.get("Resolved").cloned()
        })
        .expect("attack impact");
    assert_eq!(
        resolved["outcome"]["intention"],
        serde_json::json!(intention)
    );
}

#[test]
fn admitted_attack_identity_is_assigned_before_same_execution_impact() {
    let (mut game, actor) = fixture();
    let target = game
        .spawn_actor(
            Location {
                region: RegionId(1),
                position: Position { x: 2, y: 1, z: 0 },
            },
            NonZeroU64::new(100).unwrap(),
        )
        .unwrap();
    let mut species = crate::test_creatures::species();
    species.melee = crate::attacks::NaturalAttack::from_melee(crate::test_creatures::melee(
        2,
        1,
        crate::combat::DamageType::Impact,
        4,
    ))
    .unwrap();
    game.content = crate::test_creatures::content_with(species.clone(), vec![]);
    for id in [actor, target] {
        crate::test_creatures::configure(&mut game, id, "neutral", species.clone());
    }
    game.actors.get_mut(&target).unwrap().ready_at = 100;
    let intention = game
        .admit_intention(
            actor,
            Action::UseAbility {
                id: crate::content_ids::AbilityId::new("basic_melee").unwrap(),
                invocation: crate::abilities::AbilityInvocation::actor_target(target),
                supplies: Default::default(),
            },
            IntentionOrigin::Human,
        )
        .unwrap();
    game.execute_next_intention().unwrap().outcome.unwrap();
    assert!(game.preparation(actor).is_none());
    assert!(game.combat_events().iter().any(|event| matches!(event,
            crate::combat::CombatEvent::Resolved { outcome, .. } if outcome.intention == Some(intention))));
}

#[test]
fn interrupted_wind_up_retains_its_admission_identity() {
    let (mut game, actor) = fixture();
    let target = game
        .spawn_actor(
            Location {
                region: RegionId(1),
                position: Position { x: 2, y: 1, z: 0 },
            },
            NonZeroU64::new(100).unwrap(),
        )
        .unwrap();
    for id in [actor, target] {
        crate::test_creatures::configure(
            &mut game,
            id,
            "neutral",
            crate::test_creatures::species(),
        );
    }
    let intention = game
        .admit_intention(
            actor,
            Action::UseAbility {
                id: crate::content_ids::AbilityId::new("basic_melee").unwrap(),
                invocation: crate::abilities::AbilityInvocation::actor_target(target),
                supplies: Default::default(),
            },
            IntentionOrigin::Human,
        )
        .unwrap();
    game.execute_next_intention().unwrap().outcome.unwrap();
    game.apply_damage(
        actor,
        &crate::damage::DamageSpec::new(
            vec![crate::damage::DamageComponent::fixed(
                crate::combat::DamageType::Vital,
                None,
                1,
            )],
            None,
        )
        .unwrap(),
    )
    .unwrap();
    // Complete the outer publication omitted by the private damage helper.
    game.seal_event_observations().unwrap();
    assert!(game.event_receipts_valid());
    let preparation = game.preparation(actor).unwrap();
    assert_eq!(preparation.intention, Some(intention));
    assert!(!preparation.active);
    assert!(game.combat_events().iter().any(|event| matches!(event,
            crate::combat::CombatEvent::Interrupted { intention: Some(id), .. } if *id == intention)));
    let mut shared = crate::checkpoint::SharedState::default();
    let snapshot = game.checkpoint(&mut shared);
    assert_eq!(
        Game::restore_checkpoint(snapshot, &shared, game.content().clone()).unwrap(),
        game
    );
}

#[test]
fn preparation_checkpoint_requires_bounded_original_timing() {
    let (mut game, actor) = fixture();
    let target = game
        .spawn_actor(
            Location {
                region: RegionId(1),
                position: Position { x: 2, y: 1, z: 0 },
            },
            NonZeroU64::new(100).unwrap(),
        )
        .unwrap();
    for id in [actor, target] {
        crate::test_creatures::configure(
            &mut game,
            id,
            "neutral",
            crate::test_creatures::species(),
        );
    }
    game.act(
        actor,
        Action::UseAbility {
            id: crate::content_ids::AbilityId::new("basic_melee").unwrap(),
            invocation: crate::abilities::AbilityInvocation::actor_target(target),
            supplies: Default::default(),
        },
    )
    .unwrap();
    let original = game.preparation(actor).unwrap();
    assert_eq!(
        (original.plan.preparation(), original.plan.recovery()),
        (60, 40)
    );
    for field in ["preparation", "recovery"] {
        let mut missing = serde_json::to_value(original).unwrap();
        missing["plan"].as_object_mut().unwrap().remove(field);
        assert!(serde_json::from_value::<crate::combat::Preparation>(missing).is_err());
    }
    for (duration, recovery) in [(0, 40), (1_000_001, 40), (60, 1_000_001), (60, u64::MAX)] {
        let mut forged = game.clone();
        let mut state = forged.actors.get_mut(&actor).unwrap();
        let pending = state.pending.as_mut().unwrap();
        let crate::abilities::Plan::Ability(plan) = &mut pending.plan else {
            panic!("ability plan")
        };
        plan.preparation = duration;
        plan.recovery = recovery;
        drop(state);
        let mut shared = crate::checkpoint::SharedState::default();
        let snapshot = forged.checkpoint(&mut shared);
        assert!(Game::restore_checkpoint(snapshot, &shared, game.content().clone()).is_err());
    }
}

#[test]
fn new_resume_admission_preserves_the_original_preparation_identity() {
    let (mut game, actor) = fixture();
    let target = game
        .spawn_actor(
            Location {
                region: RegionId(1),
                position: Position { x: 2, y: 1, z: 0 },
            },
            NonZeroU64::new(100).unwrap(),
        )
        .unwrap();
    for id in [actor, target] {
        crate::test_creatures::configure(
            &mut game,
            id,
            "neutral",
            crate::test_creatures::species(),
        );
    }
    let original = game
        .admit_intention(
            actor,
            Action::UseAbility {
                id: crate::content_ids::AbilityId::new("basic_melee").unwrap(),
                invocation: crate::abilities::AbilityInvocation::actor_target(target),
                supplies: Default::default(),
            },
            IntentionOrigin::Human,
        )
        .unwrap();
    game.execute_next_intention().unwrap().outcome.unwrap();
    game.apply_damage(
        actor,
        &crate::damage::DamageSpec::new(
            vec![crate::damage::DamageComponent::fixed(
                crate::combat::DamageType::Vital,
                None,
                1,
            )],
            None,
        )
        .unwrap(),
    )
    .unwrap();
    let remaining = game.preparation(actor).unwrap().remaining;
    let resumed = game
        .admit_intention(
            actor,
            Action::UseAbility {
                id: crate::content_ids::AbilityId::new("basic_melee").unwrap(),
                invocation: crate::abilities::AbilityInvocation::actor_target(target),
                supplies: Default::default(),
            },
            IntentionOrigin::Human,
        )
        .unwrap();
    assert_ne!(original, resumed);
    game.execute_next_intention().unwrap().outcome.unwrap();
    let preparation = game.preparation(actor).unwrap();
    assert_eq!(preparation.intention, Some(resumed));
    assert_eq!(preparation.remaining, remaining);
    assert_eq!(preparation.origin_intention(), Some(original));
    assert_eq!(preparation.hold, None);
    let mut shared = crate::checkpoint::SharedState::default();
    let snapshot = game.checkpoint(&mut shared);
    let restored = Game::restore_checkpoint(snapshot, &shared, game.content().clone()).unwrap();
    assert_eq!(
        restored.preparation(actor).unwrap().origin_intention(),
        Some(original)
    );
    assert_eq!(restored, game);
    let mut missing = serde_json::to_value(game.preparation(actor).unwrap()).unwrap();
    missing.as_object_mut().unwrap().remove("origin");
    assert!(serde_json::from_value::<crate::combat::Preparation>(missing).is_err());
    for invalid in [IntentionId(0), IntentionId(resumed.0 + 1)] {
        let mut forged = game.clone();
        forged
            .actors
            .get_mut(&actor)
            .unwrap()
            .pending
            .as_mut()
            .unwrap()
            .origin = Some(invalid);
        let mut shared = crate::checkpoint::SharedState::default();
        let snapshot = forged.checkpoint(&mut shared);
        assert!(Game::restore_checkpoint(snapshot, &shared, game.content().clone()).is_err());
    }
    let foreign = game
        .admit_intention(target, Action::Wait, IntentionOrigin::Human)
        .unwrap();
    let mut forged = game.clone();
    let mut actor_state = forged.actors.get_mut(&actor).unwrap();
    let preparation = actor_state.pending.as_mut().unwrap();
    preparation.intention = None;
    preparation.origin = Some(foreign);
    drop(actor_state);
    let mut shared = crate::checkpoint::SharedState::default();
    let snapshot = forged.checkpoint(&mut shared);
    assert!(Game::restore_checkpoint(snapshot, &shared, game.content().clone()).is_err());
}

#[test]
fn admission_has_no_gameplay_effect_until_simulation_executes() {
    let (mut game, actor) = fixture();
    let before = game.clone();
    let id = game
        .admit_intention(actor, Action::Move(Direction::East), IntentionOrigin::Human)
        .unwrap();
    assert_eq!(game.tick(), before.tick());
    assert_eq!(game.actors[&actor].location, before.actors[&actor].location);
    assert_eq!(game.actors[&actor].ready_at, before.actors[&actor].ready_at);
    assert_eq!(game.pending_intention(actor).unwrap().id, id);
    let executed = game.execute_next_intention().unwrap();
    assert_eq!(executed.intention.id, id);
    assert!(executed.outcome.is_ok());
    assert_eq!(
        game.actors[&actor].location.position,
        Position { x: 2, y: 1, z: 0 }
    );
    assert!(game.pending_intention(actor).is_none());
}

#[test]
fn queued_actions_follow_actor_scheduler_order_not_submission_order() {
    let (mut game, first) = fixture();
    let second = game
        .spawn_actor(
            Location {
                region: RegionId(1),
                position: Position { x: 3, y: 1, z: 0 },
            },
            NonZeroU64::new(100).unwrap(),
        )
        .unwrap();
    let second_id = game
        .admit_intention(second, Action::Wait, IntentionOrigin::Human)
        .unwrap();
    let first_id = game
        .admit_intention(first, Action::Wait, IntentionOrigin::Human)
        .unwrap();
    assert_eq!(
        game.execute_next_intention().unwrap().intention.id,
        first_id
    );
    assert_eq!(
        game.execute_next_intention().unwrap().intention.id,
        second_id
    );
    assert!(game.execute_next_intention().is_none());
}

#[test]
fn deferred_ai_selects_once_at_execution_and_matches_direct_simulation() {
    let (mut game, actor) = fixture();
    crate::test_creatures::configure(&mut game, actor, "foe", crate::test_creatures::species());
    let hero = game
        .spawn_actor(
            Location {
                region: RegionId(1),
                position: Position { x: 2, y: 1, z: 0 },
            },
            NonZeroU64::new(100).unwrap(),
        )
        .unwrap();
    crate::test_creatures::configure(&mut game, hero, "hero", crate::test_creatures::species());
    game.configure_run(
        hero,
        BTreeSet::from([hero]),
        BTreeMap::from([("foe".into(), BTreeSet::from(["hero".into()]))]),
    )
    .unwrap();
    game.configure_ai(actor, crate::ai::AiProfile::general())
        .unwrap();
    game.act(hero, Action::Wait).unwrap();
    game.refresh_navigation();
    let before = crate::diagnostics::work_counts().route_searches;
    let id = game.admit_ai_intention(actor).unwrap();
    assert_eq!(crate::diagnostics::work_counts().route_searches, before);
    let mut expected = game.clone();
    expected.cancel_intention(actor, id).unwrap();
    // Supply the same provenance so equality checks rules, RNG and AI state
    // as well as the newly retained admission identity.
    let (action, outcome) = expected.act_ai_with_intention(actor, id).unwrap();
    let before = crate::diagnostics::work_counts().route_searches;
    let execution = game.execute_next_intention().unwrap();
    assert_eq!(crate::diagnostics::work_counts().route_searches - before, 1);
    assert_eq!(execution.action, Some(action));
    assert_eq!(execution.outcome, Ok(outcome));
    assert_eq!(game, expected);
}

#[test]
fn execution_revalidates_the_original_target_and_records_failure_without_effect() {
    let (mut game, actor) = fixture();
    let at = game.actors[&actor].location;
    let item = game
        .place_item(at, "item.test.token".parse().unwrap(), "token".into())
        .unwrap();
    let id = game
        .admit_intention(
            actor,
            Action::Take {
                item,
                quantity: None,
            },
            IntentionOrigin::Human,
        )
        .unwrap();
    game.items.remove(&item);
    let tick = game.tick();
    let execution = game.execute_next_intention().unwrap();
    assert_eq!(execution.intention.id, id);
    assert_eq!(execution.outcome, Err(GameError::ItemUnavailable));
    assert_eq!(game.tick(), tick);
    assert!(game.pending_intention(actor).is_none());
    assert!(game.observe(actor).unwrap().inventory.is_empty());
}

#[test]
fn saved_queue_preserves_identity_and_requires_explicit_resume_when_suspended() {
    let (mut game, actor) = fixture();
    let id = game
        .admit_intention(actor, Action::Wait, IntentionOrigin::Human)
        .unwrap();
    let before = game.clone();
    assert!(
        game.resume_intention(actor, id).is_err(),
        "queued work is not suspended work"
    );
    assert!(game
        .suspend_intention(actor, IntentionId(id.0 + 1))
        .is_err());
    assert_eq!(game, before);
    game.suspend_intention(actor, id).unwrap();
    let suspended = game.clone();
    assert!(game.suspend_intention(actor, id).is_err());
    assert_eq!(game, suspended);
    game.suspend_human_intentions();
    let mut shared = crate::checkpoint::SharedState::default();
    let snapshot = game.checkpoint(&mut shared);
    let bytes = serde_json::to_vec(&(snapshot, shared)).unwrap();
    let (snapshot, shared) = serde_json::from_slice(&bytes).unwrap();
    let mut restored = Game::restore_checkpoint(snapshot, &shared, game.content().clone()).unwrap();
    assert_eq!(restored, game);
    assert_eq!(restored.pending_intention(actor).unwrap().id, id);
    assert_eq!(restored.next_intention_actor(), None);
    assert!(restored.execute_next_intention().is_none());
    restored.resume_intention(actor, id).unwrap();
    assert_eq!(restored.next_intention_actor(), Some(actor));
    assert_eq!(restored.execute_next_intention().unwrap().intention.id, id);
}

#[test]
fn malformed_saved_action_structure_is_rejected_without_rechecking_availability() {
    let (mut game, actor) = fixture();
    let item = game
        .place_item(
            game.actors[&actor].location,
            "item.test.token".parse().unwrap(),
            "token".into(),
        )
        .unwrap();
    game.admit_intention(
        actor,
        Action::Take {
            item,
            quantity: Some(1),
        },
        IntentionOrigin::Human,
    )
    .unwrap();
    let mut shared = crate::checkpoint::SharedState::default();
    let snapshot = game.checkpoint(&mut shared);
    let original = serde_json::to_value((snapshot, shared)).unwrap();
    for field in ["quantity", "item"] {
        let mut invalid = original.clone();
        invalid[1]["intentions"]["entries"][0]["work"]["action"]["value"][field] =
            serde_json::json!(0);
        let (snapshot, shared) = serde_json::from_value(invalid).unwrap();
        assert!(
            Game::restore_checkpoint(snapshot, &shared, game.content().clone()).is_err(),
            "invalid {field}"
        );
    }
}

#[test]
fn rewind_restores_pending_work_without_reusing_abandoned_future_identities() {
    let (mut game, actor) = fixture();
    let first = game
        .admit_intention(actor, Action::Wait, IntentionOrigin::Human)
        .unwrap();
    let mut boundary = game.clone();
    game.execute_next_intention().unwrap();
    let abandoned = game
        .admit_intention(actor, Action::Wait, IntentionOrigin::Human)
        .unwrap();
    boundary.continue_intention_ids(&game);
    assert_eq!(boundary.pending_intention(actor).unwrap().id, first);
    boundary.cancel_intention(actor, first).unwrap();
    let next = boundary
        .admit_intention(actor, Action::Wait, IntentionOrigin::Human)
        .unwrap();
    assert!(next > abandoned);
    assert!(boundary.cancel_intention(actor, first).is_err());
    assert_eq!(boundary.pending_intention(actor).unwrap().id, next);
}

fn capacity_fixture() -> (Game, Vec<ActorId>) {
    let mut world = tor_world::World::new(vec![], vec![]).unwrap();
    world
        .add_region(tor_world::Region {
            id: RegionId(1),
            name: "queue".into(),
            bounds: tor_world::Extent::new(MAX_QUEUED_INTENTIONS as i32 + 2, 3, 1).unwrap(),
        })
        .unwrap();
    let mut game = Game::new(world, 42, crate::test_creatures::content());
    let mut actors = Vec::new();
    for x in 0..=MAX_QUEUED_INTENTIONS {
        actors.push(
            game.spawn_actor(
                Location {
                    region: RegionId(1),
                    position: Position {
                        x: x as i32,
                        y: 1,
                        z: 0,
                    },
                },
                NonZeroU64::new(100).unwrap(),
            )
            .unwrap(),
        );
    }
    (game, actors)
}

#[test]
fn capacity_and_busy_rejections_preserve_state_and_identity_allocation() {
    let (mut game, actors) = capacity_fixture();
    for &actor in &actors[..MAX_QUEUED_INTENTIONS] {
        game.admit_intention(actor, Action::Wait, IntentionOrigin::Human)
            .unwrap();
    }
    let full = game.clone();
    assert!(!game.intention_input(actors[0]).slot_available);
    assert!(
        !game
            .intention_input(actors[MAX_QUEUED_INTENTIONS])
            .slot_available
    );
    assert_eq!(game, full);
    assert_eq!(
        game.admit_intention(actors[0], Action::Wait, IntentionOrigin::Human),
        Err(GameError::ActorBusy)
    );
    assert_eq!(
        game.admit_intention(
            actors[MAX_QUEUED_INTENTIONS],
            Action::Wait,
            IntentionOrigin::Human
        ),
        Err(GameError::Schedule(crate::ScheduleError::QueueFull))
    );
    assert_eq!(game, full);
    game.cancel_intention(actors[0], game.pending_intention(actors[0]).unwrap().id)
        .unwrap();
    assert!(
        game.intention_input(actors[MAX_QUEUED_INTENTIONS])
            .slot_available
    );
    let id = game
        .admit_intention(
            actors[MAX_QUEUED_INTENTIONS],
            Action::Wait,
            IntentionOrigin::Human,
        )
        .unwrap();
    assert_eq!(id.0, MAX_QUEUED_INTENTIONS as u64 + 1);
}

#[test]
fn recovery_availability_distinguishes_capacity_from_new_identity_exhaustion() {
    let (mut game, actors) = capacity_fixture();
    let actor = actors[0];
    let target = actors[1];
    for id in [actor, target] {
        crate::test_creatures::configure(
            &mut game,
            id,
            "neutral",
            crate::test_creatures::species(),
        );
    }
    game.actors.get_mut(&target).unwrap().ready_at = 50;
    let original = game
        .admit_intention(
            actor,
            Action::UseAbility {
                id: crate::content_ids::AbilityId::new("basic_melee").unwrap(),
                invocation: crate::abilities::AbilityInvocation::actor_target(target),
                supplies: Default::default(),
            },
            IntentionOrigin::Human,
        )
        .unwrap();
    game.execute_next_intention().unwrap().outcome.unwrap();
    game.pause_preparation(actor).unwrap();
    for &other in &actors[1..] {
        game.admit_intention(other, Action::Wait, IntentionOrigin::Human)
            .unwrap();
    }
    // Exhausted fresh identities do not prohibit reusing existing identities.
    game.intentions.next_id = u64::MAX;
    let before = game.clone();
    let input = game.intention_input(actor);
    assert!(!input.slot_available);
    let control = input.controls().next().unwrap();
    assert_eq!(control.intention, original);
    assert!(
        !control.can_resume,
        "paused preparation needs one free queue slot"
    );
    assert!(control.can_cancel);
    assert_eq!(game, before);
    assert_eq!(
        game.resume_intention(actor, original),
        Err(GameError::Schedule(crate::ScheduleError::QueueFull))
    );
    assert_eq!(game, before);

    // Existing suspended work occupies a slot already, so it can resume
    // even when both queue capacity and fresh identity allocation are full.
    let existing = game.pending_intention(target).unwrap().id;
    game.suspend_intention(target, existing).unwrap();
    let full = game.clone();
    let control = game.intention_input(target).controls().next().unwrap();
    assert_eq!(control.state, IntentionControlState::Suspended);
    assert!(control.can_resume);
    assert_eq!(game, full);
    game.resume_intention(target, existing).unwrap();
    assert_eq!(game.intentions.next_id, u64::MAX);
    game.cancel_intention(target, existing).unwrap();

    let input = game.intention_input(actor);
    assert!(
        !input.slot_available,
        "fresh admission still needs a new identity"
    );
    assert!(input.controls().next().unwrap().can_resume);
    let mut shared = crate::checkpoint::SharedState::default();
    let checkpoint = game.checkpoint(&mut shared);
    let mut restored =
        Game::restore_checkpoint(checkpoint, &shared, game.content().clone()).unwrap();
    assert_eq!(restored.intention_input(actor), input);
    restored.resume_intention(actor, original).unwrap();
    assert_eq!(restored.pending_intention(actor).unwrap().id, original);
    assert_eq!(restored.intentions.next_id, u64::MAX);
}

#[test]
fn a_dead_queued_actor_yields_a_failure_instead_of_stuck_work() {
    let (mut game, actor) = fixture();
    crate::test_creatures::configure(
        &mut game,
        actor,
        "neutral",
        crate::test_creatures::species(),
    );
    let id = game
        .admit_intention(actor, Action::Wait, IntentionOrigin::Human)
        .unwrap();
    game.apply_damage(
        actor,
        &crate::damage::DamageSpec::new(
            vec![crate::damage::DamageComponent::fixed(
                crate::combat::DamageType::Vital,
                None,
                100,
            )],
            None,
        )
        .unwrap(),
    )
    .unwrap();
    let before = game.tick();
    assert_eq!(game.next_intention_actor(), Some(actor));
    assert_eq!(game.tick(), before);
    let failed = game
        .execute_next_intention()
        .expect("dead queued actor must resolve");
    assert_eq!(failed.intention.id, id);
    assert_eq!(failed.outcome, Err(GameError::UnknownActor));
    assert_eq!(game.tick(), before);
    assert!(game.pending_intention(actor).is_none());
}
#[test]
fn queued_data_only_self_heal_round_trips_and_executes_identically() {
    use crate::rules::{AbilityDef, AiHints, AmountExpr, Effect, Reach, Timing, TimingSource};
    let (mut game, actor) = fixture();
    let ability = crate::content_ids::AbilityId::new("self_mend").unwrap();
    let mut document = game.content.rules.document().clone();
    document.abilities.push(AbilityDef {
        effect_visual: crate::rules::EffectVisual::Neutral,
        effect_style: None,
        cancellable: false,
        interaction: crate::rules::Interaction::NonHostile,
        invocation: Default::default(),
        id: ability.clone(),
        tags: vec![],
        name: "Self Mend".into(),
        basic_attack: false,
        outgoing_edge_policy: Default::default(),
        cost: None,
        cooldown: None,
        supply_costs: vec![],
        timing: Timing {
            source: TimingSource::Fixed {
                prepare: 1,
                recover: 1,
            },
            scaled_by: None,
        },
        reach: Reach::SelfOnly,
        strikes: crate::rules::StrikeSource::Authored {
            strikes: vec![crate::rules::StrikeDef {
                cancellable: false,
                offset: 0,
                unlock: None,
                check: None,
                on_success: vec![Effect::Heal {
                    amount: AmountExpr::Fixed { amount: 3 },
                }],
                on_failure: vec![],
                descriptors: vec![],
                riders: vec![],
            }],
        },
        ai: AiHints {
            priority: 0,
            hostile: false,
            minimum_resource_percent: 0,
            range_preference: crate::rules::RangePreference::Any,
        },
    });
    document.constants.default_abilities.push(ability.clone());
    game.content = crate::creatures::Content::new(
        crate::rules::Ruleset::validate(document).unwrap(),
        (*game.content.catalog).clone(),
    )
    .unwrap();
    let build = crate::creatures::CreatureBuild::new(
        game.content.clone(),
        crate::test_creatures::species().id,
        crate::progression::HdLedger::seeded(vec![crate::progression::HdSource::Racial], 42)
            .unwrap(),
        crate::attributes::ManaBinding::Intellect,
        [
            tor_simulation::attributes::Skill::Deception,
            tor_simulation::attributes::Skill::Persuasion,
        ],
    )
    .unwrap();
    game.configure_creature(
        actor,
        crate::CreatureIdentity {
            corpse_identity: "item.corpse.test.healer".parse().unwrap(),
            name: "healer".into(),
            faction: "neutral".into(),
        },
        build,
    )
    .unwrap();
    game.apply_damage(
        actor,
        &crate::damage::DamageSpec::new(
            vec![crate::damage::DamageComponent::fixed(
                crate::combat::DamageType::Vital,
                None,
                5,
            )],
            None,
        )
        .unwrap(),
    )
    .unwrap();
    // Complete the outer publication omitted by the private damage helper.
    game.seal_event_observations().unwrap();
    assert!(game.event_receipts_valid());
    let before = game.health(actor).unwrap().0;
    let receipt = game
        .admit_intention(
            actor,
            Action::UseAbility {
                id: ability,
                invocation: crate::abilities::AbilityInvocation::actor_target(actor),
                supplies: Default::default(),
            },
            IntentionOrigin::Human,
        )
        .unwrap();
    let mut shared = crate::checkpoint::SharedState::default();
    let saved = game.checkpoint(&mut shared);
    let mut restored = Game::restore_checkpoint(saved, &shared, game.content.clone()).unwrap();
    assert_eq!(restored, game);
    for candidate in [&mut game, &mut restored] {
        let executed = candidate.execute_next_intention().unwrap();
        assert_eq!(executed.intention.id, receipt);
        executed.outcome.unwrap();
        assert_eq!(candidate.health(actor).unwrap().0, before + 3);
    }
    assert_eq!(restored, game);
}
