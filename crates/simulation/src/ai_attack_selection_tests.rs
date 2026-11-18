//! Direct probe witnesses: the coordinator's earlier retained return must not
//! accidentally make these selector control-flow boundaries untested.
use super::super::tests::{basic, paid_fixture};
use super::*;
use crate::abilities::{AbilityAim, Plan, StepDirection};

fn paused_attack() -> Game {
    let mut game = paid_fixture(false, false);
    game.actors.get_mut(&ActorId(1)).unwrap().ready_at = 1;
    let origin = game.admit_ai_intention(ActorId(2)).unwrap();
    let execution = game.execute_next_intention().unwrap();
    assert_eq!(execution.intention.id, origin);
    execution.outcome.unwrap();
    let pending = game.preparation(ActorId(2)).unwrap();
    assert!(pending.active && pending.remaining > 0);
    assert!(!pending.has_prefix());
    assert!(
        matches!(pending.plan.work(), crate::Work::UseAbility { ref id, .. }
        if id.as_str() == "fixture_paid_melee")
    );
    game.pause_preparation(ActorId(2)).unwrap();
    game
}

fn replace_retained_aim(game: &mut Game, aim: AbilityAim) {
    let mut owner = game.actors.get_mut(&ActorId(2)).unwrap();
    let Plan::Ability(plan) = &mut owner.pending.as_mut().unwrap().plan else {
        panic!("paid melee plan");
    };
    plan.invocation.aim = aim;
}

fn probe(game: &Game, mode: AttackSelectionMode) -> RetainedAttackProbe<'_> {
    let view = game.observe(ActorId(2)).unwrap();
    let (_, sight) = game.scene_snapshot(ActorId(2)).unwrap();
    let creature = game.creature(ActorId(2)).unwrap();
    game.probe_retained_attack(
        ActorId(2),
        ActorId(1),
        &sight,
        &view,
        creature.build().ruleset(),
        mode,
    )
}

fn select(game: &Game, mode: AttackSelectionMode) -> Option<crate::Action> {
    let view = game.observe(ActorId(2)).unwrap();
    let (_, sight) = game.scene_snapshot(ActorId(2)).unwrap();
    game.choose_ai_ability(ActorId(2), ActorId(1), &sight, &view, mode)
        .map(|choice| match choice {
            AiChoice::Frozen(plan) => plan.work().action(),
            AiChoice::Request(action) => action,
        })
}

#[test]
fn retained_attack_probe_distinguishes_frozen_mismatch_and_unavailable() {
    let game = paused_attack();
    let before = game.clone();
    let RetainedAttackProbe::Frozen(plan) = probe(&game, AttackSelectionMode::Engage) else {
        panic!("ordinary paused paid attack must remain frozen");
    };
    assert_eq!(plan, &game.preparation(ActorId(2)).unwrap().plan);
    assert_eq!(
        select(&game, AttackSelectionMode::Engage),
        Some(plan.work().action())
    );
    assert_eq!(game, before);

    // Synthetic negative seam mutations, never executed or accepted as saves.
    // A mismatch and an unavailable matching plan both permit fresh selection.
    let mut mismatch = game.clone();
    replace_retained_aim(
        &mut mismatch,
        AbilityAim::ActorTarget { target: ActorId(2) },
    );
    let mut unavailable = game.clone();
    unavailable
        .actors
        .get_mut(&ActorId(2))
        .unwrap()
        .pending
        .as_mut()
        .unwrap()
        .hold = None;
    for candidate in [mismatch, unavailable] {
        let before = candidate.clone();
        assert!(matches!(
            probe(&candidate, AttackSelectionMode::Engage),
            RetainedAttackProbe::ContinueFresh
        ));
        assert!(select(&candidate, AttackSelectionMode::Engage).is_some());
        assert_eq!(candidate, before);
    }
}

#[test]
fn unsupported_retained_ray_stops_selection_unless_mode_filters_it_first() {
    let original = paused_attack();
    // These intentionally malformed retained aims isolate both legacy early
    // declines: authored melee reach cannot supply either retained ray range.
    for aim in [
        AbilityAim::DirectionalRay {
            direction: StepDirection::West,
        },
        AbilityAim::CellRay { offset: [-1, 0, 0] },
    ] {
        let mut game = original.clone();
        replace_retained_aim(&mut game, aim);
        let before = game.clone();
        for mode in [
            AttackSelectionMode::Engage,
            AttackSelectionMode::RepositionProbe,
        ] {
            assert!(matches!(
                probe(&game, mode),
                RetainedAttackProbe::StopSelection
            ));
            assert!(select(&game, mode).is_none());
        }
        assert!(matches!(
            probe(&game, AttackSelectionMode::CorneredBasic),
            RetainedAttackProbe::ContinueFresh
        ));
        assert_eq!(
            select(&game, AttackSelectionMode::CorneredBasic),
            Some(basic(ActorId(1)))
        );
        assert_eq!(game, before);
    }
}

#[test]
fn absent_or_active_retained_attack_allows_fresh_selection() {
    let absent = paid_fixture(false, false);
    let mut active = paused_attack();
    active
        .actors
        .get_mut(&ActorId(2))
        .unwrap()
        .pending
        .as_mut()
        .unwrap()
        .active = true;
    for game in [absent, active] {
        let before = game.clone();
        assert!(matches!(
            probe(&game, AttackSelectionMode::Engage),
            RetainedAttackProbe::ContinueFresh
        ));
        assert!(select(&game, AttackSelectionMode::Engage).is_some());
        assert_eq!(game, before);
    }
}
