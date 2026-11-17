use super::*;

// Exact live Rogue ability/condition/device definitions; synthetic actors and
// ordinary Identify acquisition. This is NOT normal generated NPC birth stock.
fn production_haste_device_fixture() -> (Game, ItemId, ItemId) {
    let patches: toml::Value =
        toml::from_str(include_str!("../../../../scenarios/rogue/rules.toml")).unwrap();
    let authored: toml::Value = toml::from_str(include_str!(
        "../../../../scenarios/rogue/catalog/items.toml"
    ))
    .unwrap();
    let definition = |category: &str, id: &str| -> toml::Value {
        patches["overrides"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| {
                row["path"].as_array().is_some_and(|path| {
                    path.len() == 2
                        && path[0].as_str() == Some(category)
                        && path[1].as_str() == Some(id)
                })
            })
            .unwrap()["value"]
            .clone()
    };
    let exact_ability: crate::rules::AbilityDef = definition("abilities", "rogue.wand_haste")
        .try_into()
        .unwrap();
    let expected_definition = exact_ability.clone();
    let mut game = super::super::ray_tests::fixture_with(|document, ray, species| {
        // Do not turn the device into a granted creature ability.
        species.modifiers.retain(|modifier| {
            !matches!(&modifier.op,
            crate::rules::ModifierOp::GrantAbility { ability } if ability == &ray.id)
        });
        for id in ["rogue.hasted", "rogue.slowed"] {
            document
                .conditions
                .push(definition("conditions", id).try_into().unwrap());
        }
        *ray = exact_ability;
        // Fixture-only deterministic failed target Agility dodge: increase the
        // opposed caster Lore operand through Intellect, not the ability/check.
        // Both actors share species; target Agility uses Speed, not Intellect.
        species.modifiers.push(crate::rules::Modifier {
            cancellable: false,
            scope: crate::rules::Scope::Always,
            op: crate::rules::ModifierOp::Add {
                stat: crate::rules::Stat::Attribute {
                    attribute: crate::attributes::Attribute::Intellect,
                },
                value: 100,
            },
        });
    });
    assert_eq!(
        game.content
            .rules()
            .ability(&expected_definition.id)
            .unwrap(),
        &expected_definition
    );
    assert!(!game
        .creature(ActorId(1))
        .unwrap()
        .derived()
        .abilities
        .contains(&expected_definition.id));
    let row = &authored["items"]["rogue.wand_haste"];
    let mut item = ItemSpec::ordinary(
        row["identity"].as_str().unwrap().parse().unwrap(),
        row["name"].as_str().unwrap().into(),
    );
    item.class = ItemClass::Wand;
    item.concealed = true;
    item.appearance = "unidentified fixture wand".into();
    item.device = Some(row["device"].clone().try_into().unwrap());
    let item = game
        .place_item_stack(10, at(1), Some(ActorId(1)), 1, item)
        .unwrap();
    assert!(game.items[&item]
        .charges
        .is_some_and(|n| (3..=7).contains(&n)));
    let scroll = identify_scroll(&mut game, 11, ItemClass::Wand, 1);
    (game, item, scroll)
}
fn engage_haste_probe(game: &mut Game) {
    game.configure_ai(ActorId(1), crate::ai::AiProfile::general())
        .unwrap();
    let clock = game.actor_clock(ActorId(1));
    game.actors
        .get_mut(&ActorId(1))
        .unwrap()
        .behavior
        .engagement = Some(crate::behavior::Engagement {
        reference: crate::perception::ActorReferenceKind::Creature,
        target: ActorId(2),
        location: at(3),
        last_seen: clock,
    });
}

#[test]
fn ordinary_identified_production_haste_device_is_not_offensive_ai_work() {
    let (mut game, item, scroll) = production_haste_device_fixture();
    let mut unknown = game.clone();
    engage_haste_probe(&mut unknown);
    assert!(!matches!(
        unknown.choose_ai(ActorId(1)).unwrap().action,
        Action::Zap { .. }
    ));
    let mut hidden_identity = unknown.clone();
    hidden_identity
        .items
        .edit(item, |value| {
            value.spec.identity = "item.wand.hidden-other-haste".parse().unwrap();
        })
        .unwrap();
    assert_eq!(
        unknown.observe(ActorId(1)).unwrap(),
        hidden_identity.observe(ActorId(1)).unwrap()
    );
    assert_eq!(
        unknown.choose_ai(ActorId(1)).unwrap().action,
        hidden_identity.choose_ai(ActorId(1)).unwrap().action
    );
    unknown.identify_item(ActorId(1), item).unwrap();
    assert!(
        !matches!(
            unknown.choose_ai(ActorId(1)).unwrap().action,
            Action::Zap { .. }
        ),
        "known type alone cannot supply an unobserved positive balance"
    );
    let mut hidden_balance = unknown.clone();
    hidden_balance
        .items
        .edit(item, |value| value.charges = Some(1))
        .unwrap();
    assert_eq!(
        unknown.observe(ActorId(1)).unwrap(),
        hidden_balance.observe(ActorId(1)).unwrap()
    );
    assert_eq!(
        unknown.choose_ai(ActorId(1)).unwrap().action,
        hidden_balance.choose_ai(ActorId(1)).unwrap().action
    );

    let choice = execute(&mut game, Action::Read { item: scroll });
    execute(
        &mut game,
        Action::Identify {
            choice,
            item: Some(item),
        },
    );
    let view = game.observe(ActorId(1)).unwrap();
    assert!(view
        .inventory
        .iter()
        .any(|row| row.id == item && row.identified));
    assert!(view
        .interactions
        .as_ref()
        .unwrap()
        .inventory
        .iter()
        .any(|row| row.item == item && row.charges.is_some_and(|charges| charges > 0)));
    assert!(view.visible_actors.iter().any(|row| row.id == ActorId(2)
        && row.disclosure == crate::perception::ActorDisclosure::Creature));
    assert!(
        !game.attack_available(ActorId(1), ActorId(2)),
        "target is outside ordinary melee"
    );
    let action = Action::Zap {
        item,
        aim: crate::DeviceAim::Direction {
            direction: StepDirection::East,
        },
    };
    let mut effect = game.clone();
    let charge = effect.items[&item].charges.unwrap();
    let ordinary_ticks = effect.actors[&ActorId(2)]
        .movement_duration(tor_world::Direction::East)
        .unwrap();
    execute(&mut effect, action.clone());
    assert_eq!(effect.items[&item].charges, Some(charge - 1));
    assert!(
        effect
            .creature(ActorId(2))
            .unwrap()
            .conditions()
            .instances()
            .any(|condition| condition.condition.as_str() == "rogue.hasted"),
        "unchanged production effect actually benefits the disclosed foe"
    );
    assert!(
        effect.actors[&ActorId(2)]
            .movement_duration(tor_world::Direction::East)
            .unwrap()
            < ordinary_ticks
    );
    assert_eq!(effect, roundtrip(&effect));

    engage_haste_probe(&mut game);
    let before = game.clone();
    let selected = game.choose_ai(ActorId(1)).unwrap().action;
    assert_eq!(
        game, before,
        "preview cannot spend charges or learn target state"
    );
    assert_ne!(
        selected, action,
        "ordinary identified positive-charge haste must not be proposed against the foe"
    );
}
