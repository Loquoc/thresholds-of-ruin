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
    authored_item_birth(false, false);
}

#[test]
fn explicit_recipe_birth_preserves_mechanics_knowledge_location_and_replay() {
    authored_item_birth(true, false);
}

#[test]
fn relative_item_birth_journals_absolute_position_and_replays() {
    authored_item_birth(false, true);
}

fn authored_item_birth(by_recipe: bool, relative: bool) {
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
    if relative {
        let WizardOperation::SpawnItem { position, .. } = op.clone() else {
            unreachable!()
        };
        let origin = Position {
            x: position.x - 1,
            ..position
        };
        let resolved = crate::developer::parse_wizard_at(
            "item spawn here offset +1 +0 +0 identity item.wand.embers",
            origin,
        )
        .unwrap();
        assert_eq!(resolved, op);
        op = resolved;
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
    let result = wizard(&mut engine, op);
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

#[test]
fn wizard_context_queries_preserve_authority_and_exact_ids() {
    let mut engine = item_engine();
    engine.enable_wizard().unwrap();
    let before = authority(&engine);
    let boundary = engine.presentation_boundary();
    let origin = engine.wizard_origin_at(&boundary, ActorId(1)).unwrap();
    let report = engine
        .wizard_report_at(&boundary, ActorId(1), crate::developer::WizardQuery::Where)
        .unwrap();
    assert!(report.lines.contains(&"actor=1".into()));
    let report = engine
        .wizard_report_at(
            &boundary,
            ActorId(1),
            crate::developer::WizardQuery::Tile { position: origin },
        )
        .unwrap();
    assert!(report.lines.contains(&"actor=1".into()));
    assert!(engine
        .wizard_report_at(
            &boundary,
            ActorId(1),
            crate::developer::WizardQuery::Tile {
                position: Position {
                    region: origin.region + 1,
                    ..origin
                }
            }
        )
        .is_err());
    assert!(engine
        .wizard_report_at(
            &boundary,
            ActorId(1),
            crate::developer::WizardQuery::Tile {
                position: Position {
                    x: origin.x + 9,
                    ..origin
                }
            }
        )
        .is_err());
    assert_eq!(authority(&engine), before);
}

#[test]
fn relative_wizard_rejects_retained_picture_after_authoritative_change() {
    let mut engine = item_engine();
    engine.enable_wizard().unwrap();
    let old = engine.presentation_boundary();
    let text = "item spawn here recipe ember";
    let op = engine.resolve_wizard_at(&old, ActorId(1), text).unwrap();
    wizard(&mut engine, op);
    let before = authority(&engine);
    assert!(engine.resolve_wizard_at(&old, ActorId(1), text).is_err());
    assert_eq!(authority(&engine), before);
    // Frozen private reads retain the issued picture; mutations cannot use it.
    assert!(engine
        .wizard_report_at(&old, ActorId(1), crate::developer::WizardQuery::Where)
        .is_ok());
}

#[test]
fn wizard_item_give_uses_real_recipe_and_carried_ownership_without_teaching_identity() {
    let mut engine = item_engine();
    let at = engine.game.observe(SimActor(1)).unwrap().location;
    let target_position = engine
        .game
        .known_cells(SimActor(1))
        .find(|location| {
            *location != at && engine.game.travel_route(SimActor(1), *location).is_ok()
        })
        .unwrap();
    let birth = wizard(
        &mut engine,
        WizardOperation::SpawnActor {
            position: Position {
                region: target_position.region.0,
                x: target_position.position.x,
                y: target_position.position.y,
                z: target_position.position.z,
            },
            turn_ticks: 100,
        },
    );
    let JournalContent::Wizard {
        result: WizardResult::ActorSpawned { actor },
        ..
    } = birth.entry.content
    else {
        panic!("expected a loaded recipient");
    };
    let expected = engine
        .regions
        .as_ref()
        .unwrap()
        .wizard_item(Some("item.wand.embers"), None, Some("ember"))
        .unwrap();
    let start = engine.game.tick();
    for (target, recipient) in [
        ("self".to_string(), ActorId(1)),
        (actor.0.to_string(), actor),
    ] {
        let before = engine.game.observe(SimActor(recipient.0)).unwrap();
        let operation = crate::developer::parse_wizard(&format!(
            "item give {target} recipe ember identity item.wand.embers"
        ))
        .unwrap();
        let result = wizard(&mut engine, operation);
        assert_eq!(result.entry.audience, tor_protocol::Audience::Private);
        let JournalContent::Wizard {
            result: WizardResult::ItemGiven { actor: owner, item },
            ..
        } = result.entry.content
        else {
            panic!("expected a carried item");
        };
        assert_eq!(owner, recipient);
        let new = engine.game.observe(SimActor(recipient.0)).unwrap();
        let item_view = new.inventory.iter().find(|row| row.id.0 == item).unwrap();
        assert_eq!(item_view.quantity, 1);
        assert_eq!(item_view.appearance, expected.appearance);
        assert!(!item_view.identified);
        assert!(!engine
            .game
            .knows_item_identity(SimActor(recipient.0), &expected.identity));
        assert_eq!(new.ground_items, before.ground_items);
        assert_eq!(new.inventory.len(), before.inventory.len() + 1);
        let saved = authority(&engine);
        let pool = saved["game"]["items"].as_u64().unwrap() as usize;
        let record = &saved["shared"]["items"][pool][item.to_string()];
        assert_eq!(record["spec"], serde_json::to_value(&expected).unwrap());
        assert_eq!(record["location"]["Carried"], json!(recipient.0));
        assert_eq!(record["charges"], 3);
    }
    assert_eq!(engine.game.tick(), start);
    restored_both_ways(&engine);
    wizard(&mut engine, WizardOperation::Rewind { target: None });
    assert!(engine
        .game
        .observe(SimActor(1))
        .unwrap()
        .inventory
        .is_empty());
    restored_both_ways(&engine);
}

#[test]
fn wizard_item_give_rejects_invalid_target_or_selector_without_consuming_authority() {
    let mut engine = item_engine();
    engine.enable_wizard().unwrap();
    for text in [
        "item give 999 recipe ember",
        "item give self recipe missing",
        "item give self recipe ember identity item.potion.mending",
    ] {
        let before = engine.diagnostic_copy();
        let result = engine.command(
            "player",
            "wizard-give-test",
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
    for text in [
        "item give self",
        "item give self recipe ember recipe ember",
        "item give self identity",
        "item give 0 recipe ember",
    ] {
        assert!(crate::developer::parse_wizard(text).is_err());
    }
}

#[test]
fn private_item_catalog_and_instance_reads_preserve_checkpoint_and_allocations() {
    let mut engine = item_engine();
    engine.enable_wizard().unwrap();
    let before = authority(&engine);
    let boundary = engine.presentation_boundary();
    let report = engine
        .wizard_report_at(
            &boundary,
            ActorId(1),
            crate::developer::WizardQuery::ItemList { after: None },
        )
        .unwrap();
    assert!(report.lines.contains(&"recipe=ember".into()));
    assert!(report.lines.contains(&"identity=item.wand.embers".into()));
    let report = engine
        .wizard_report_at(
            &boundary,
            ActorId(1),
            crate::developer::WizardQuery::ItemInspect { item: 30 },
        )
        .unwrap();
    assert!(report.lines.contains(&"charges=3".into()));
    assert!(report.lines.contains(&"quantity=1".into()));
    assert!(report.lines.contains(&"ground region=1 x=1 y=2 z=0".into()));
    assert_eq!(authority(&engine), before);
}
