//! Authored items use ordinary allocation, private wizard authority and replay.
use super::*;

fn item_engine() -> Engine {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/tests/magic-items");
    Engine::memory(crate::scenario_package::load(&path, 42, None, true).unwrap()).unwrap()
}
fn operation(engine: &Engine, identity: &str) -> WizardOperation {
    let at = engine.game.observe(SimActor(1)).unwrap().location;
    crate::developer::parse_wizard(&format!(
        "item spawn {} {} {} {} identity {identity}",
        at.region.0, at.position.x, at.position.y, at.position.z
    ))
    .unwrap()
}

#[test]
fn authored_item_birth_preserves_mechanics_knowledge_location_and_replay() {
    authored_item_birth(false);
}

#[test]
fn explicit_recipe_birth_preserves_mechanics_knowledge_location_and_replay() {
    authored_item_birth(true);
}

fn authored_item_birth(by_recipe: bool) {
    let mut engine = item_engine();
    // Use the actual pinned fixture's ordinary equipment recipe, not a custom
    // privileged representation. Its identity is independent of its display label.
    let identity = "item.wand.embers";
    let mut op = operation(&engine, identity);
    if by_recipe {
        let WizardOperation::SpawnItem {
            identity, recipe, ..
        } = &mut op
        else {
            unreachable!()
        };
        *identity = None;
        *recipe = Some("ember".into());
    }
    let expected = engine
        .regions
        .as_ref()
        .unwrap()
        .wizard_item(Some(identity), None, None)
        .unwrap();
    let before = engine.game.observe(SimActor(1)).unwrap();
    let known = engine
        .game
        .knows_item_identity(SimActor(1), &expected.identity);
    let result = wizard(&mut engine, op.clone());
    let JournalContent::Wizard {
        result: WizardResult::ItemPlaced { item },
        ..
    } = result.entry.content
    else {
        panic!("item result")
    };
    let found = engine
        .game
        .observe(SimActor(1))
        .unwrap()
        .ground_items
        .into_iter()
        .find(|row| row.id.0 == item)
        .unwrap();
    assert_eq!(found.appearance, expected.appearance);
    assert_eq!(found.location, before.location);
    assert!(!found.identified);
    let saved = authority(&engine);
    let pool = saved["game"]["items"].as_u64().unwrap() as usize;
    let record = &saved["shared"]["items"][pool][item.to_string()];
    assert_eq!(record["spec"], serde_json::to_value(&expected).unwrap());
    assert_eq!(record["charges"], 3);
    assert_eq!(found.quantity, 1);
    assert_eq!(engine.game.tick(), before.tick);
    assert_eq!(
        engine
            .game
            .knows_item_identity(SimActor(1), &expected.identity),
        known
    );
    restored_both_ways(&engine);
    wizard(&mut engine, WizardOperation::Rewind { target: None });
    assert!(!engine
        .game
        .observe(SimActor(1))
        .unwrap()
        .ground_items
        .iter()
        .any(|row| row.id.0 == item));
    restored_both_ways(&engine);
}

#[test]
fn authored_item_invalid_selection_and_placement_are_atomic() {
    let mut engine = item_engine();
    engine.enable_wizard().unwrap();
    let good = operation(&engine, "item.wand.embers");
    let WizardOperation::SpawnItem { position, .. } = good else {
        unreachable!()
    };
    for op in [
        operation(&engine, "item.potion.missing"),
        WizardOperation::SpawnItem {
            position,
            identity: None,
            appearance: None,
            recipe: None,
        },
        WizardOperation::SpawnItem {
            position,
            identity: Some("item.wand.embers".into()),
            appearance: Some("item.potion.missing".into()),
            recipe: None,
        },
        WizardOperation::SpawnItem {
            position: Position {
                x: i32::MAX,
                ..position
            },
            identity: Some("item.wand.embers".into()),
            appearance: None,
            recipe: None,
        },
    ] {
        let before = engine.diagnostic_copy();
        let result = engine.command(
            "player",
            "wizard-item",
            ActorId(1),
            &uuid::Uuid::new_v4().to_string(),
            &engine.branch().clone(),
            Command::Wizard {
                display_basis: None,
                expected_boundary: engine.causal_boundary(ActorId(1)).unwrap(),
                operation: op,
            },
        );
        assert_eq!(result.unwrap_err().code, ErrorCode::InvalidAction);
        exact_equal(&engine, &before);
    }
}
