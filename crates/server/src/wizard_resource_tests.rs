//! Host persistence and no-op bounds for authored resource restoration.
use super::*;
use tor_simulation::resources::Resource;

#[test]
fn wizard_resource_fill_clamps_full_pools_keeps_tick_and_knowledge_and_rewinds() {
    let root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/rogue-exploration");
    let scenario = crate::scenario_package::load(&root, 42, Some(1), true).unwrap();
    let mut engine = Engine::memory(scenario).unwrap();
    engine.enable_wizard().unwrap();
    let sim_actor = SimActor(1);
    let identity = "item.potion.hallucination".parse().unwrap();
    assert!(!engine.game.knows_item_identity(sim_actor, &identity));
    let tick = engine.game.tick();

    let missing_before = engine.diagnostic_copy();
    let result = engine.command(
        "player",
        "wizard-resource",
        ActorId(1),
        "missing-resource-target",
        &engine.branch().clone(),
        Command::Wizard {
            display_basis: None,
            expected_boundary: engine.causal_boundary(ActorId(1)).unwrap(),
            operation: crate::developer::parse_wizard("resource fill 999999 mana").unwrap(),
        },
    );
    assert_eq!(result.unwrap_err().code, ErrorCode::InvalidAction);
    exact_equal(&engine, &missing_before);

    let original_branch = engine.branch().clone();
    for (resource, text) in [
        (Resource::Stamina, "resource fill self stamina"),
        (Resource::Focus, "resource restore 1 focus 5"),
        (Resource::Mana, "resource fill self mana"),
    ] {
        let old = engine
            .game
            .creature(sim_actor)
            .unwrap()
            .costs()
            .resources()
            .clone();
        assert_eq!(old.balance(resource), old.maximum(resource));
        let result = wizard(&mut engine, crate::developer::parse_wizard(text).unwrap());
        assert_eq!(result.entry.audience, tor_protocol::Audience::Private);
        assert!(matches!(result.entry.content,
            JournalContent::Wizard { result: WizardResult::ResourceRestored {
                actor: ActorId(1), resource: actual, restored: 0,
            }, .. } if actual == resource));
        assert_eq!(
            &old,
            engine.game.creature(sim_actor).unwrap().costs().resources()
        );
        assert_eq!(engine.game.tick(), tick);
        assert!(!engine.game.knows_item_identity(sim_actor, &identity));
    }
    restored_both_ways(&engine);
    wizard(&mut engine, WizardOperation::Rewind { target: None });
    assert_ne!(*engine.branch(), original_branch);
    assert_eq!(engine.game.tick(), tick);
    restored_both_ways(&engine);
}
