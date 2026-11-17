use super::*;

// Exact live Rogue ability/condition/device definitions; synthetic actors and
// ordinary Identify acquisition. This is NOT normal generated NPC birth stock.
fn production_device_fixture(ability: &str) -> (Game, ItemId, ItemId) {
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
    let exact_ability: crate::rules::AbilityDef =
        definition("abilities", ability).try_into().unwrap();
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
    let row = &authored["items"][ability];
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
fn engage_device_probe(game: &mut Game) {
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
    let (mut game, item, scroll) = production_device_fixture("rogue.wand_haste");
    let mut unknown = game.clone();
    engage_device_probe(&mut unknown);
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

    engage_device_probe(&mut game);
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

#[test]
fn ordinary_identified_production_magic_missile_is_available_to_ai() {
    let (mut game, item, scroll) = production_device_fixture("rogue.wand_magic_missile");
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
        .interactions
        .as_ref()
        .unwrap()
        .inventory
        .iter()
        .any(|row| row.item == item && row.zappable && row.charges.is_some_and(|n| n > 0)));
    assert!(!game.attack_available(ActorId(1), ActorId(2)));
    let expected = Action::Zap {
        item,
        aim: crate::DeviceAim::Direction {
            direction: StepDirection::East,
        },
    };
    let mut control = game.clone();
    let charges = control.items[&item].charges.unwrap();
    let hp = control.health(ActorId(2)).unwrap().0;
    execute(&mut control, expected.clone());
    assert_eq!(control.items[&item].charges, Some(charges - 1));
    assert!(
        control.health(ActorId(2)).unwrap().0 < hp,
        "ordinary authored missile actually harms the disclosed hostile target"
    );
    engage_device_probe(&mut game);
    let before = game.clone();
    let selected = game.choose_ai(ActorId(1)).unwrap().action;
    assert_eq!(game, before, "preview cannot pay or learn");
    assert_eq!(
        selected, expected,
        "known legal first-body device should be usable autonomously"
    );
}

fn identified_missile() -> (Game, ItemId) {
    let (mut game, item, scroll) = production_device_fixture("rogue.wand_magic_missile");
    let choice = execute(&mut game, Action::Read { item: scroll });
    execute(
        &mut game,
        Action::Identify {
            choice,
            item: Some(item),
        },
    );
    engage_device_probe(&mut game);
    (game, item)
}

#[test]
fn first_body_device_requires_owned_type_and_positive_charge_knowledge() {
    let (mut game, item, _) = production_device_fixture("rogue.wand_magic_missile");
    engage_device_probe(&mut game);
    assert!(!matches!(
        game.choose_ai(ActorId(1)).unwrap().action,
        Action::Zap { .. }
    ));
    game.identify_item(ActorId(1), item).unwrap();
    let before = game.clone();
    assert!(!matches!(
        game.choose_ai(ActorId(1)).unwrap().action,
        Action::Zap { .. }
    ));
    assert_eq!(game, before);
    let mut twin = game.clone();
    twin.items.edit(item, |row| row.charges = Some(1)).unwrap();
    assert_eq!(
        game.observe(ActorId(1)).unwrap(),
        twin.observe(ActorId(1)).unwrap()
    );
    assert_eq!(
        game.choose_ai(ActorId(1)).unwrap().action,
        twin.choose_ai(ActorId(1)).unwrap().action
    );
}

#[test]
fn first_body_device_keeps_disclosed_neutral_interception_veto() {
    let (mut game, item) = identified_missile();
    assert!(
        matches!(game.choose_ai(ActorId(1)).unwrap().action, Action::Zap { item: chosen, .. } if chosen == item)
    );
    add_disclosed_neutral_interceptor(&mut game);
    let before = game.clone();
    assert!(!matches!(
        game.choose_ai(ActorId(1)).unwrap().action,
        Action::Zap { .. }
    ));
    assert_eq!(game, before);
}

#[test]
fn first_body_device_paid_checkpoint_keeps_frozen_bearing_and_one_charge() {
    let (mut game, item) = identified_missile();
    let charges = game.items[&item].charges.unwrap();
    let moved = Location {
        region: RegionId(1),
        position: Position { x: 3, y: 2, z: 0 },
    };
    assert!(game
        .actor_translation(ActorId(2), tor_world::Direction::South)
        .is_some());
    let mut fresh_after_move = game.clone();
    fresh_after_move.teleport(ActorId(2), moved).unwrap();
    assert_eq!(
        fresh_after_move.choose_ai(ActorId(1)).unwrap().action,
        Action::Zap {
            item,
            aim: crate::DeviceAim::Cell { offset: [2, 1, 0] }
        }
    );
    game.actors.get_mut(&ActorId(2)).unwrap().ready_at = game.tick + 1;
    let origin = game.admit_ai_intention(ActorId(1)).unwrap();
    let result = game.execute_next_intention().unwrap();
    assert_eq!(result.intention.id, origin);
    result.outcome.unwrap();
    let pending = game.preparation(ActorId(1)).unwrap();
    assert!(pending.remaining > 0);
    let frozen = pending.plan.clone();
    assert_eq!(game.items[&item].charges, Some(charges));
    game.pause_preparation(ActorId(1)).unwrap();
    game.teleport(ActorId(2), moved).unwrap();
    assert!(game
        .observe(ActorId(1))
        .unwrap()
        .visible_actors
        .iter()
        .any(|row| row.id == ActorId(2) && row.location == moved));
    let target_hp = game.health(ActorId(2)).unwrap().0;
    let mut restored = roundtrip(&game);
    assert_eq!(game, restored);
    for current in [&mut game, &mut restored] {
        let before = current.clone();
        assert_eq!(
            current.choose_ai(ActorId(1)).unwrap().action,
            Action::Zap {
                item,
                aim: crate::DeviceAim::Direction {
                    direction: StepDirection::East
                }
            }
        );
        assert_eq!(*current, before);
        current.admit_ai_intention(ActorId(1)).unwrap();
        current.execute_next_intention().unwrap().outcome.unwrap();
        let pending = current.preparation(ActorId(1)).unwrap();
        assert_eq!(pending.origin_intention(), Some(origin));
        assert_eq!(pending.plan, frozen);
        for _ in 0..32 {
            if current.preparation(ActorId(1)).is_none() {
                break;
            }
            current
                .act(current.next_actor().unwrap(), Action::Wait)
                .unwrap();
        }
        assert!(current.preparation(ActorId(1)).is_none());
        assert_eq!(current.items[&item].charges, Some(charges - 1));
        assert!(current.items[&item].holds.is_empty());
        assert_eq!(
            current.health(ActorId(2)).unwrap().0,
            target_hp,
            "frozen East bearing misses the target that moved off that ray"
        );
    }
    assert_eq!(game, restored);
}

#[test]
fn first_body_device_arbitrary_far_bearing_and_disclosed_wall_policy() {
    for wall in [false, true] {
        let (mut game, item) = identified_missile();
        let location = |x, y, z| Location {
            region: RegionId(77),
            position: Position { x, y, z },
        };
        game.add_region(tor_world::Region {
            id: RegionId(77),
            name: "First body device".into(),
            bounds: tor_world::Extent::new(12, 8, 4).unwrap(),
        })
        .unwrap();
        game.set_region_light(RegionId(77), true).unwrap();
        let destination = if wall {
            location(5, 2, 0)
        } else {
            location(9, 4, 0)
        };
        for (actor, at) in [(ActorId(1), location(1, 2, 0)), (ActorId(2), destination)] {
            game.teleport(actor, at).unwrap();
            let mut body = (*game.actors[&actor].body).clone();
            body.cells = vec![[0, 0, 0], [0, 0, 1]];
            body.eye = [0, 0, 1];
            game.set_body(actor, body).unwrap();
        }
        game.refresh_navigation();
        let expected = Action::Zap {
            item,
            aim: if wall {
                crate::DeviceAim::Direction {
                    direction: StepDirection::East,
                }
            } else {
                crate::DeviceAim::Cell { offset: [8, 2, 0] }
            },
        };
        assert_eq!(game.choose_ai(ActorId(1)).unwrap().action, expected);
        let restored = roundtrip(&game);
        assert_eq!(restored.choose_ai(ActorId(1)).unwrap().action, expected);
        if wall {
            game.set_wall(location(2, 2, 0), true).unwrap();
            game.refresh_navigation();
            let view = game.observe(ActorId(1)).unwrap();
            assert!(view.visible_actors.iter().any(|row| row.id == ActorId(2)));
            for x in 2..=5 {
                assert!(view
                    .visible_cells
                    .iter()
                    .any(|cell| cell.location == location(x, 2, 0) && cell.wall == (x == 2)));
            }
            let before = game.clone();
            assert!(!matches!(
                game.choose_ai(ActorId(1)).unwrap().action,
                Action::Zap { .. }
            ));
            assert_eq!(game, before);
        } else {
            let mut hidden = game.clone();
            hidden
                .add_region(tor_world::Region {
                    id: RegionId(78),
                    name: "Unseen".into(),
                    bounds: tor_world::Extent::new(8, 8, 4).unwrap(),
                })
                .unwrap();
            hidden
                .spawn_actor(
                    Location {
                        region: RegionId(78),
                        position: Position { x: 3, y: 3, z: 0 },
                    },
                    std::num::NonZeroU64::new(100).unwrap(),
                )
                .unwrap();
            assert_eq!(
                game.observe(ActorId(1)).unwrap(),
                hidden.observe(ActorId(1)).unwrap()
            );
            assert_eq!(hidden.choose_ai(ActorId(1)).unwrap().action, expected);
        }
    }
}

#[test]
fn ordinary_checkless_device_admission_and_known_zero_survive_roundtrip() {
    let (mut game, item, scroll) = production_device_fixture("rogue.wand_cancellation");
    let choice = execute(&mut game, Action::Read { item: scroll });
    execute(
        &mut game,
        Action::Identify {
            choice,
            item: Some(item),
        },
    );
    let action = Action::Zap {
        item,
        aim: crate::DeviceAim::Direction {
            direction: StepDirection::East,
        },
    };
    let initial = game.items[&item].charges.unwrap();
    assert!((3..=7).contains(&initial));
    let mut ai = game.clone();
    engage_device_probe(&mut ai);
    let before = ai.clone();
    assert_eq!(ai.choose_ai(ActorId(1)).unwrap().action, action);
    assert_eq!(ai, before);
    let plan = game
        .plan_work(
            ActorId(1),
            crate::Work::Zap {
                item,
                aim: crate::DeviceAim::Direction {
                    direction: StepDirection::East,
                },
            },
        )
        .unwrap();
    assert_eq!(plan.as_ray().unwrap().traversal, RayTraversal::FirstBody);
    assert_eq!(plan.as_ray().unwrap().resolution, RayResolution::Checkless);
    let rng = game.rules_rng;
    let hp = game.health(ActorId(2)).unwrap().0;
    // Repeated cancellation is intentional payment/knowledge boundary coverage,
    // not a claim that cancelling an unchanged target is tactically useful.
    for remaining in (0..initial).rev() {
        for _ in 0..32 {
            let actor = game.next_actor().unwrap();
            if actor == ActorId(1) {
                break;
            }
            game.act(actor, Action::Wait).unwrap();
        }
        assert_eq!(game.next_actor(), Some(ActorId(1)));
        execute(&mut game, action.clone());
        for _ in 0..32 {
            if game.preparation(ActorId(1)).is_none() {
                break;
            }
            game.act(game.next_actor().unwrap(), Action::Wait).unwrap();
        }
        assert!(game.preparation(ActorId(1)).is_none());
        assert_eq!(game.items[&item].charges, Some(remaining));
        assert!(game.items[&item].holds.is_empty());
        assert_eq!(game.rules_rng, rng);
        let view = game.observe(ActorId(1)).unwrap();
        assert_eq!(
            view.interactions
                .unwrap()
                .inventory
                .iter()
                .find(|row| row.item == item)
                .unwrap()
                .charges,
            Some(remaining)
        );
    }
    assert_eq!(game.health(ActorId(2)).unwrap().0, hp);
    engage_device_probe(&mut game);
    let mut restored = roundtrip(&game);
    assert_eq!(game, restored);
    for current in [&mut game, &mut restored] {
        assert!(current
            .observe(ActorId(1))
            .unwrap()
            .visible_actors
            .iter()
            .any(|row| row.id == ActorId(2)));
        let before = current.clone();
        assert!(!matches!(
            current.choose_ai(ActorId(1)).unwrap().action,
            Action::Zap { .. }
        ));
        assert_eq!(*current, before);
    }
    assert_eq!(
        game.choose_ai(ActorId(1)).unwrap().action,
        restored.choose_ai(ActorId(1)).unwrap().action
    );
}
