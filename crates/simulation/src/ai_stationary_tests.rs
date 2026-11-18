//! Stationary acquisition ranks disclosed melee contact without changing pursuit policy.
use super::tests::{at, basic, configured_game};
use super::*;
use crate::test_creatures;
use std::num::NonZeroU64;

fn stationary_acquisition_fixture(
    stationary: bool,
    disposition: crate::behavior::Disposition,
) -> (Game, ActorId) {
    let mut foe = test_creatures::species();
    foe.behavior.disposition = disposition;
    foe.behavior.idle = crate::behavior::IdleBehavior::Wait;
    foe.modifiers.push(crate::rules::Modifier {
        cancellable: false,
        scope: crate::rules::Scope::Always,
        op: crate::rules::ModifierOp::Locomotion {
            value: crate::locomotion::LocomotionValue::Stationary {
                enabled: stationary,
            },
        },
    });
    let mut game = configured_game(test_creatures::species(), foe, test_creatures::ruleset());
    game.teleport(ActorId(1), at(1, 4, 1)).unwrap();
    let near = game
        .spawn_actor(at(1, 1, 1), NonZeroU64::new(100).unwrap())
        .unwrap();
    game.configure_creature(
        near,
        crate::CreatureIdentity {
            corpse_identity: "item.corpse.test.other".parse().unwrap(),
            name: "adjacent opponent".into(),
            faction: "hero".into(),
        },
        game.creature(ActorId(1)).unwrap().build().clone(),
    )
    .unwrap();
    game.configure_ai(
        ActorId(2),
        AiProfile {
            idle: crate::behavior::IdleBehavior::Wait,
            ..AiProfile::general()
        },
    )
    .unwrap();
    game.refresh_navigation();
    (game, near)
}

fn assert_stationary_acquisition_geometry(game: &Game, near: ActorId) {
    let owner = ActorId(2);
    let view = game.observe(owner).unwrap();
    for target in [ActorId(1), near] {
        assert!(view
            .visible_actors
            .iter()
            .any(|other| other.id == target && other.disclosure.reference().is_some()));
        assert!(
            game.route_search(owner)
                .unwrap()
                .route(game.actors[&target].location)
                .is_err(),
            "stationary pursuit cannot distinguish either opponent"
        );
    }
    assert!(
        game.attack_available(owner, near),
        "independent live contact oracle confirms legal melee reach"
    );
    assert!(!game.attack_available(owner, ActorId(1)));
}

#[test]
fn stationary_fresh_acquisition_prefers_disclosed_melee_over_unreachable_low_id() {
    let (game, near) = stationary_acquisition_fixture(true, crate::behavior::Disposition::Mean);
    assert_stationary_acquisition_geometry(&game, near);
    assert!(game.actors[&ActorId(2)].behavior.engagement.is_none());
    let before = game.clone();
    let decision = game.choose_ai(ActorId(2)).unwrap();
    assert_eq!(decision.action, basic(near));
    assert_eq!(decision.behavior.engagement.unwrap().target, near);
    assert_eq!(game, before, "preview cannot commit target acquisition");
    assert_eq!(game.choose_ai(ActorId(2)).unwrap().action, decision.action);
}

#[test]
fn stationary_fresh_acquisition_keeps_actor_id_tie_for_two_melee_foes() {
    let (mut game, near) = stationary_acquisition_fixture(true, crate::behavior::Disposition::Mean);
    game.teleport(ActorId(1), at(1, 3, 1)).unwrap();
    game.refresh_navigation();
    assert!(game.attack_available(ActorId(2), ActorId(1)));
    assert!(game.attack_available(ActorId(2), near));
    assert_eq!(
        game.choose_ai(ActorId(2)).unwrap().action,
        basic(ActorId(1))
    );
}

#[test]
fn stationary_acquisition_preserves_passive_gate_and_mobile_route_order() {
    let (passive, near) =
        stationary_acquisition_fixture(true, crate::behavior::Disposition::Passive);
    assert_stationary_acquisition_geometry(&passive, near);
    let decision = passive.choose_ai(ActorId(2)).unwrap();
    assert_eq!(decision.action, Action::Wait);
    assert!(decision.behavior.engagement.is_none());
    let (mobile, near) = stationary_acquisition_fixture(false, crate::behavior::Disposition::Mean);
    let mut search = mobile.route_search(ActorId(2)).unwrap();
    assert!(
        search.route(mobile.actors[&near].location).unwrap().len()
            < search
                .route(mobile.actors[&ActorId(1)].location)
                .unwrap()
                .len()
    );
    assert_eq!(mobile.choose_ai(ActorId(2)).unwrap().action, basic(near));
}

#[test]
fn stationary_acquisition_does_not_replace_an_existing_engagement() {
    let (mut game, near) = stationary_acquisition_fixture(true, crate::behavior::Disposition::Mean);
    assert_stationary_acquisition_geometry(&game, near);
    // Inspect the refresh stage: later failed pursuit may legitimately clear
    // the old target, but acquisition must not retarget an existing engagement.
    let old = crate::behavior::Engagement {
        target: ActorId(1),
        reference: crate::perception::ActorReferenceKind::Creature,
        location: game.actors[&ActorId(1)].location,
        last_seen: game.actor_clock(ActorId(2)),
    };
    game.actors
        .get_mut(&ActorId(2))
        .unwrap()
        .behavior
        .engagement = Some(old);
    let snapshot = game.decision_snapshot(ActorId(2)).unwrap();
    let mut ai = game.combat.ai[&ActorId(2)].clone();
    let mut behavior = game.actors[&ActorId(2)].behavior.clone();
    let mut gold = super::decision_stages::GoldChoiceCache::default();
    let mut search = game.route_search(ActorId(2)).unwrap();
    game.refresh_ai_encounter(
        super::encounter::EncounterInputs {
            actor: ActorId(2),
            policy: game.actors[&ActorId(2)].behavior_policy().unwrap(),
            has_retained: false,
            snapshot: &snapshot,
        },
        &mut ai,
        &mut behavior,
        &mut |at| search.route(at).ok(),
        &mut gold,
    )
    .unwrap();
    assert_eq!(behavior.engagement, Some(old));
}

#[test]
fn stationary_acquisition_pending_checkpoint_commits_only_with_execution() {
    let (mut game, near) = stationary_acquisition_fixture(true, crate::behavior::Disposition::Mean);
    assert_stationary_acquisition_geometry(&game, near);
    game.act(ActorId(1), Action::Wait).unwrap();
    let choice = game.choose_ai(ActorId(2)).unwrap();
    assert_eq!(choice.action, basic(near));
    let before = game.actors[&ActorId(2)].behavior.clone();
    game.admit_ai_intention(ActorId(2)).unwrap();
    assert_eq!(game.actors[&ActorId(2)].behavior, before);
    let mut shared = crate::checkpoint::SharedState::default();
    let root = serde_json::from_value(serde_json::to_value(game.checkpoint(&mut shared)).unwrap())
        .unwrap();
    let shared = serde_json::from_value(serde_json::to_value(shared).unwrap()).unwrap();
    let mut restored = Game::restore_checkpoint(root, &shared, game.content().clone()).unwrap();
    for candidate in [&mut game, &mut restored] {
        assert_eq!(candidate.actors[&ActorId(2)].behavior, before);
        let execution = candidate.execute_next_intention().unwrap();
        assert_eq!(execution.action, Some(choice.action.clone()));
        execution.outcome.unwrap();
        assert_eq!(
            candidate.actors[&ActorId(2)]
                .behavior
                .engagement
                .unwrap()
                .target,
            near
        );
    }
    assert_eq!(game, restored);
}

#[test]
fn stationary_acquisition_retained_refresh_does_not_acquire() {
    let (game, near) = stationary_acquisition_fixture(true, crate::behavior::Disposition::Mean);
    assert_stationary_acquisition_geometry(&game, near);
    let snapshot = game.decision_snapshot(ActorId(2)).unwrap();
    let mut ai = game.combat.ai[&ActorId(2)].clone();
    let mut behavior = game.actors[&ActorId(2)].behavior.clone();
    let before = behavior.clone();
    let mut gold = super::decision_stages::GoldChoiceCache::default();
    let mut search = game.route_search(ActorId(2)).unwrap();
    assert!(before.engagement.is_none());
    let facts = game
        .refresh_ai_encounter(
            super::encounter::EncounterInputs {
                actor: ActorId(2),
                policy: game.actors[&ActorId(2)].behavior_policy().unwrap(),
                has_retained: true,
                snapshot: &snapshot,
            },
            &mut ai,
            &mut behavior,
            &mut |at| search.route(at).ok(),
            &mut gold,
        )
        .unwrap();
    assert_eq!(behavior, before);
    assert!(facts.sensed_target.is_none());
    assert_eq!(
        facts.threat,
        Some(game.actors[&ActorId(1)].location),
        "retained work keeps the legacy unreachable-route/low-ID fallback threat"
    );
}

#[test]
fn stationary_acquisition_never_uses_an_undisclosed_foe() {
    let (mut game, near) = stationary_acquisition_fixture(true, crate::behavior::Disposition::Mean);
    game.teleport(near, at(2, 4, 2)).unwrap();
    game.set_region_light(tor_world::RegionId(2), false)
        .unwrap();
    game.refresh_navigation();
    assert!(!game
        .observe(ActorId(2))
        .unwrap()
        .visible_actors
        .iter()
        .any(|other| other.id == near));
    let choice = game.choose_ai(ActorId(2)).unwrap();
    assert_eq!(choice.action, Action::Wait);
    assert!(choice.behavior.engagement.is_none());
}

#[test]
fn stationary_acquisition_exposed_greedy_owner_can_defend() {
    let (game, near) = stationary_acquisition_fixture(true, crate::behavior::Disposition::Greedy);
    assert_stationary_acquisition_geometry(&game, near);
    let view = game.observe(ActorId(2)).unwrap();
    assert!(game.exposed_to_visible_hostile(ActorId(2), view.location, &view));
    assert_eq!(game.choose_ai(ActorId(2)).unwrap().action, basic(near));
}

#[test]
fn stationary_acquisition_cancellation_preserves_authoritative_target_memory() {
    let (mut game, near) = stationary_acquisition_fixture(true, crate::behavior::Disposition::Mean);
    assert_stationary_acquisition_geometry(&game, near);
    game.act(ActorId(1), Action::Wait).unwrap();
    let before = game.actors[&ActorId(2)].behavior.clone();
    let intention = game.admit_ai_intention(ActorId(2)).unwrap();
    let pending = game.clone();
    assert!(game
        .cancel_intention_with_step(ActorId(2), intention, Some(Direction::Down))
        .is_err());
    assert_eq!(game, pending);
    game.cancel_intention(ActorId(2), intention).unwrap();
    assert_eq!(game.actors[&ActorId(2)].behavior, before);
    assert_eq!(game.choose_ai(ActorId(2)).unwrap().action, basic(near));
}
