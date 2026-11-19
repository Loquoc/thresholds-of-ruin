//! Wizard conditions use the real authored lifecycle and private journal.
use super::*;
use crate::journal::WizardConditionOutcome as Outcome;
use tor_simulation::{conditions::ConditionPhase, content_ids::ConditionId};

#[test]
fn wizard_condition_frozen_clear_keeps_authored_protection_through_recovery_and_rewind() {
    let root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/rogue-exploration");
    let scenario = crate::scenario_package::load(&root, 42, Some(1), true).unwrap();
    let mut engine = Engine::memory(scenario).unwrap();
    engine.enable_wizard().unwrap();
    for text in [
        "condition apply 999 rogue.frozen 200",
        "condition apply self rogue.frozen 200 source 999",
        "condition clear self unknown.condition",
    ] {
        let before = engine.diagnostic_copy();
        let result = engine.command(
            "player",
            "wizard-condition",
            ActorId(1),
            text,
            &engine.branch().clone(),
            Command::Wizard {
                display_basis: None,
                expected_boundary: engine.causal_boundary(ActorId(1)).unwrap(),
                operation: crate::developer::parse_wizard(text).unwrap(),
            },
        );
        assert_eq!(result.unwrap_err().code, ErrorCode::InvalidAction);
        exact_equal(&engine, &before);
    }
    let initial_tick = engine.game.tick();
    let frozen = ConditionId::new("rogue.frozen").unwrap();
    let row = |engine: &Engine| {
        let creature = engine.game.creature(SimActor(1)).unwrap();
        let current = creature
            .conditions()
            .instances()
            .find(|row| row.condition == frozen)
            .map(|row| (row.phase, row.remaining, row.source));
        current
    };
    let run = |engine: &mut Engine, command, outcome| {
        let result = wizard(engine, crate::developer::parse_wizard(command).unwrap());
        assert_eq!(result.entry.audience, tor_protocol::Audience::Private);
        assert!(matches!(
            result.entry.content,
            JournalContent::Wizard {
                result: WizardResult::ConditionChanged { actor: ActorId(1), outcome: actual, .. },
                ..
            } if actual == outcome
        ));
    };
    run(
        &mut engine,
        "condition apply self rogue.frozen 200",
        Outcome::Applied,
    );
    assert_eq!(
        row(&engine),
        Some((ConditionPhase::Active, Some(200), SimActor(1)))
    );
    run(
        &mut engine,
        "condition clear 1 rogue.frozen",
        Outcome::Cleared,
    );
    assert_eq!(
        row(&engine),
        Some((ConditionPhase::Protection, Some(400), SimActor(1)))
    );
    restored_both_ways(&engine);
    run(
        &mut engine,
        "condition clear self rogue.frozen",
        Outcome::NotActive,
    );
    run(
        &mut engine,
        "condition apply 1 rogue.frozen 200 source self",
        Outcome::Immune,
    );
    assert_eq!(
        row(&engine),
        Some((ConditionPhase::Protection, Some(400), SimActor(1)))
    );
    assert_eq!(engine.game.tick(), initial_tick);
    restored_both_ways(&engine);
    wizard(&mut engine, WizardOperation::Rewind { target: None });
    assert_eq!(row(&engine), None);
    restored_both_ways(&engine);
}
