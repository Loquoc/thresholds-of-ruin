//! Carried-item care uses actor knowledge and ordinary work, not birth provenance.
use super::*;
use crate::{content_ids::ConditionId, rules::Effect, test_creatures, ItemClass, ItemId, ItemSpec};
use serde_json::json;

const OWNER: ActorId = ActorId(2);
const ITEM: ItemId = ItemId(41);
fn fixture(scroll: bool) -> Game {
    let mut document = serde_json::to_value(test_creatures::ruleset().document()).unwrap();
    document["conditions"].as_array_mut().unwrap().push(json!({
        "id":"care_fixture_guard", "family":"care_fixture_guard", "name":"Guard",
        "lifecycle":"timed", "timing":{"kind":"actor_active","on_expiry":{"kind":"remove"}},
        "restrictions":[], "preparation":"preserve",
        "modifiers":[{"scope":"always","op":{"kind":"talent_reduction","category":"impact","amount":2}}],
        "stacking":{"kind":"longest"},"source_scoped":false,"descriptors":[],"capacity":"reject"
    }));
    let rules = crate::rules::Ruleset::validate(serde_json::from_value(document).unwrap()).unwrap();
    let mut owner = test_creatures::species();
    owner.default_attributes = crate::attributes::Attributes::new(crate::attributes::ByAttribute {
        strength: 2,
        ..Default::default()
    })
    .unwrap();
    let mut game = super::tests::configured_game(test_creatures::species(), owner, rules);
    let mut item = ItemSpec::ordinary(
        "item.potion.care-fixture".parse().unwrap(),
        "guard dose".into(),
    );
    item.concealed = true;
    item.class = if scroll {
        ItemClass::Scroll
    } else {
        ItemClass::Potion
    };
    item.identity = if scroll {
        "item.scroll.care-fixture"
    } else {
        "item.potion.care-fixture"
    }
    .parse()
    .unwrap();
    let effects = vec![Effect::ApplyCondition {
        condition: ConditionId::new("care_fixture_guard").unwrap(),
        duration: 400,
    }];
    if scroll {
        item.readable = Some(crate::ReadableSpec {
            control: None,
            descriptors: vec![],
            effects,
            identify: None,
            discovery: crate::ItemDiscovery::SelfEffect,
        });
    } else {
        item.consumable = Some(crate::ConsumableSpec {
            descriptors: vec![],
            effects,
            discovery: crate::ItemDiscovery::SelfEffect,
        });
    }
    let location = game.actors[&OWNER].location;
    game.place_item_stack(ITEM.0, location, Some(OWNER), 1, item)
        .unwrap();
    game
}
fn expected(scroll: bool) -> Action {
    if scroll {
        Action::Read { item: ITEM }
    } else {
        Action::Drink { item: ITEM }
    }
}
#[test]
fn known_self_defensive_potion_and_scroll_use_ordinary_work() {
    for scroll in [false, true] {
        let mut game = fixture(scroll);
        game.identify_item(OWNER, ITEM).unwrap();
        assert_eq!(game.choose_ai(OWNER).unwrap().action, expected(scroll));
    }
}
#[test]
fn unknown_carried_care_item_never_borrows_another_observers_knowledge() {
    for scroll in [false, true] {
        let mut game = fixture(scroll);
        assert_ne!(game.choose_ai(OWNER).unwrap().action, expected(scroll));
        game.identify_item(ActorId(1), ITEM).unwrap();
        assert_ne!(game.choose_ai(OWNER).unwrap().action, expected(scroll));
    }
}
#[test]
fn active_self_buff_is_not_consumed_again() {
    for scroll in [false, true] {
        let mut game = fixture(scroll);
        game.identify_item(OWNER, ITEM).unwrap();
        game.apply_effects(
            OWNER,
            &[Effect::ApplyCondition {
                condition: ConditionId::new("care_fixture_guard").unwrap(),
                duration: 400,
            }],
        )
        .unwrap();
        assert_ne!(game.choose_ai(OWNER).unwrap().action, expected(scroll));
    }
}

#[test]
fn care_policy_and_mixed_harm_reject_without_inspecting_hidden_curse() {
    for scroll in [false, true] {
        let mut game = fixture(scroll);
        game.identify_item(OWNER, ITEM).unwrap();
        let mut disabled = game.clone();
        let mut profile = AiProfile::general();
        profile.care = false;
        disabled.configure_ai(OWNER, profile).unwrap();
        assert_ne!(disabled.choose_ai(OWNER).unwrap().action, expected(scroll));
        for cursed in [false, true] {
            let mut hidden = game.clone();
            hidden
                .items
                .edit(ITEM, |item| item.condition.cursed = cursed)
                .unwrap();
            assert_eq!(hidden.choose_ai(OWNER).unwrap().action, expected(scroll));
        }
        let mut spec = game.items[&ITEM].spec.clone();
        let effects = if scroll {
            &mut spec.readable.as_mut().unwrap().effects
        } else {
            &mut spec.consumable.as_mut().unwrap().effects
        };
        effects.push(Effect::LoseMaximumHealth {
            amount: crate::rules::AmountExpr::Fixed { amount: 1 },
        });
        game.items.edit(ITEM, |item| item.spec = spec).unwrap();
        assert_ne!(game.choose_ai(OWNER).unwrap().action, expected(scroll));
    }
}

#[test]
fn known_item_care_plan_is_the_ordinary_plan_and_selection_is_pure() {
    for scroll in [false, true] {
        let mut game = fixture(scroll);
        game.identify_item(OWNER, ITEM).unwrap();
        let before = game.clone();
        let decision = game.choose_ai(OWNER).unwrap();
        let work = if scroll {
            crate::Work::Read { item: ITEM }
        } else {
            crate::Work::Drink { item: ITEM }
        };
        assert_eq!(decision.action, expected(scroll));
        assert_eq!(decision.plan, Some(game.plan_work(OWNER, work).unwrap()));
        assert_eq!(game, before);
        assert_eq!(game.items[&ITEM].quantity, 1);
    }
}

#[test]
fn later_acquired_known_item_uses_the_same_care_path() {
    for scroll in [false, true] {
        let mut game = fixture(scroll);
        game.identify_item(OWNER, ITEM).unwrap();
        let location = game.actors[&OWNER].location;
        game.items
            .edit(ITEM, |item| {
                item.location = crate::ItemLocation::Ground(location)
            })
            .unwrap();
        // The ordinary transfer is supplied by an external controller; an AI
        // action request must otherwise match its autonomous current choice.
        game.combat.ai.remove(&OWNER);
        game.act(ActorId(1), Action::Wait).unwrap();
        game.act(
            OWNER,
            Action::Take {
                item: ITEM,
                quantity: None,
            },
        )
        .unwrap();
        assert_eq!(
            game.items[&ITEM].location,
            crate::ItemLocation::Carried(OWNER)
        );
        game.configure_ai(OWNER, AiProfile::general()).unwrap();
        assert_eq!(game.choose_ai(OWNER).unwrap().action, expected(scroll));
    }
}

#[test]
fn admitted_item_care_reserves_then_consumes_through_normal_scheduler() {
    for scroll in [false, true] {
        let mut game = fixture(scroll);
        game.identify_item(OWNER, ITEM).unwrap();
        // Keep another actor ready at the start tick so the scheduler does
        // not immediately advance through the entire consume preparation.
        game.spawn_actor(
            tor_world::Location {
                region: tor_world::RegionId(1),
                position: tor_world::Position { x: 0, y: 0, z: 0 },
            },
            std::num::NonZeroU64::new(100).unwrap(),
        )
        .unwrap();
        game.act(ActorId(1), Action::Wait).unwrap();
        let before = game.tick();
        game.admit_ai_intention(OWNER).unwrap();
        let execution = game.execute_next_intention().unwrap();
        assert_eq!(execution.action, Some(expected(scroll)));
        execution.outcome.unwrap();
        assert!(game.preparation(OWNER).is_some());
        assert_eq!(
            game.items[&ITEM].quantity, 1,
            "preparation reserves rather than prematurely consuming"
        );
        for _ in 0..32 {
            if game.preparation(OWNER).is_none() {
                break;
            }
            game.act(game.next_actor().unwrap(), Action::Wait).unwrap();
        }
        assert!(game.tick() > before);
        assert!(game.preparation(OWNER).is_none());
        assert!(!game.items.contains_key(&ITEM));
        assert!(game
            .creature(OWNER)
            .unwrap()
            .conditions()
            .instances()
            .any(|entry| entry.condition == ConditionId::new("care_fixture_guard").unwrap()));
    }
}

#[test]
fn remembered_but_no_longer_visible_foe_does_not_prompt_consumable_buff() {
    for scroll in [false, true] {
        let mut game = fixture(scroll);
        game.identify_item(OWNER, ITEM).unwrap();
        let seen = game.choose_ai(OWNER).unwrap();
        game.actors.get_mut(&OWNER).unwrap().behavior = seen.behavior;
        game.combat.ai.insert(OWNER, seen.ai);
        game.add_region(tor_world::Region {
            id: tor_world::RegionId(77),
            name: "Unconnected fixture region".into(),
            bounds: tor_world::Extent::new(8, 8, 3).unwrap(),
        })
        .unwrap();
        game.teleport(
            ActorId(1),
            tor_world::Location {
                region: tor_world::RegionId(77),
                position: tor_world::Position { x: 4, y: 2, z: 0 },
            },
        )
        .unwrap();
        assert!(
            !game
                .observe(OWNER)
                .unwrap()
                .visible_actors
                .iter()
                .any(|actor| actor.id == ActorId(1)),
            "the negative fixture must actually hide the former foe"
        );
        assert_ne!(game.choose_ai(OWNER).unwrap().action, expected(scroll));
    }
}

#[test]
fn paid_item_care_checkpoint_resumes_identical_reserved_work() {
    for scroll in [false, true] {
        let mut game = fixture(scroll);
        game.identify_item(OWNER, ITEM).unwrap();
        // Keep another actor ready at the start tick so the scheduler does
        // not immediately advance through the entire consume preparation.
        game.spawn_actor(
            tor_world::Location {
                region: tor_world::RegionId(1),
                position: tor_world::Position { x: 0, y: 0, z: 0 },
            },
            std::num::NonZeroU64::new(100).unwrap(),
        )
        .unwrap();
        game.act(ActorId(1), Action::Wait).unwrap();
        let origin = game.admit_ai_intention(OWNER).unwrap();
        game.execute_next_intention().unwrap().outcome.unwrap();
        game.pause_preparation(OWNER).unwrap();
        let frozen = game.preparation(OWNER).unwrap().plan.clone();
        let mut shared = crate::checkpoint::SharedState::default();
        let snapshot =
            serde_json::from_value(serde_json::to_value(game.checkpoint(&mut shared)).unwrap())
                .unwrap();
        let shared = serde_json::from_value(serde_json::to_value(shared).unwrap()).unwrap();
        let mut restored =
            Game::restore_checkpoint(snapshot, &shared, game.content().clone()).unwrap();
        assert_eq!(game, restored);
        for candidate in [&mut game, &mut restored] {
            assert_eq!(candidate.choose_ai(OWNER).unwrap().action, expected(scroll));
            candidate.admit_ai_intention(OWNER).unwrap();
            candidate.execute_next_intention().unwrap().outcome.unwrap();
            let pending = candidate.preparation(OWNER).unwrap();
            assert_eq!(pending.origin_intention(), Some(origin));
            assert_eq!(pending.plan, frozen);
            assert_eq!(candidate.items[&ITEM].quantity, 1);
            for _ in 0..32 {
                if candidate.preparation(OWNER).is_none() {
                    break;
                }
                candidate
                    .act(candidate.next_actor().unwrap(), Action::Wait)
                    .unwrap();
            }
            assert!(candidate.preparation(OWNER).is_none());
            assert!(!candidate.items.contains_key(&ITEM));
        }
        assert_eq!(game, restored);
    }
}

#[test]
fn generic_condition_classifier_rejects_mixed_restriction_and_hidden_side_effects() {
    use crate::ai_effect_safety::{condition_safety, ConditionSafety};
    let game = fixture(false);
    let definition = game
        .content()
        .rules()
        .condition(&ConditionId::new("care_fixture_guard").unwrap())
        .unwrap();
    assert_eq!(condition_safety(definition), ConditionSafety::Defensive);
    let mut restricted = definition.clone();
    restricted
        .restrictions
        .push(crate::rules::ConditionRestriction::OrdinaryMovement);
    assert_eq!(condition_safety(&restricted), ConditionSafety::Unsupported);
    let mut paused = definition.clone();
    paused.preparation = crate::rules::ConditionPreparation::Pause;
    assert_eq!(condition_safety(&paused), ConditionSafety::Unsupported);
    let mut stacked = definition.clone();
    stacked.stacking = crate::rules::Stacking::Stack { maximum: 2 };
    assert_eq!(condition_safety(&stacked), ConditionSafety::Unsupported);
    let mut confused = definition.clone();
    confused.hallucinating = true;
    assert_eq!(condition_safety(&confused), ConditionSafety::Unsupported);
    let mut harmful = definition.clone();
    harmful.modifiers.clear();
    harmful
        .restrictions
        .push(crate::rules::ConditionRestriction::VoluntaryActions);
    assert_eq!(condition_safety(&harmful), ConditionSafety::Harmful);
    harmful.modifiers = definition.modifiers.clone();
    assert_eq!(
        condition_safety(&harmful),
        ConditionSafety::Unsupported,
        "removing a condition that also supplies a benefit is not unconditionally safe"
    );
}

#[test]
fn known_restoration_requires_matching_recorded_deficit_not_item_name() {
    for scroll in [false, true] {
        for maximum_health in [false, true] {
            let mut game = fixture(scroll);
            let amount = crate::rules::AmountExpr::Fixed { amount: 2 };
            let effect = if maximum_health {
                Effect::RestoreMaximumHealth {
                    amount: amount.clone(),
                }
            } else {
                Effect::RestoreAttribute {
                    attribute: crate::attributes::Attribute::Strength,
                    amount: amount.clone(),
                }
            };
            game.items
                .edit(ITEM, |item| {
                    if scroll {
                        item.spec.readable.as_mut().unwrap().effects = vec![effect];
                    } else {
                        item.spec.consumable.as_mut().unwrap().effects = vec![effect];
                    }
                })
                .unwrap();
            game.identify_item(OWNER, ITEM).unwrap();
            assert_ne!(
                game.choose_ai(OWNER).unwrap().action,
                expected(scroll),
                "no deficit means no useful restoration"
            );
            let loss = if maximum_health {
                Effect::LoseMaximumHealth { amount }
            } else {
                Effect::LoseAttribute {
                    attribute: crate::attributes::Attribute::Strength,
                    amount,
                }
            };
            game.apply_effects(OWNER, &[loss]).unwrap();
            let recorded = game.creature(OWNER).unwrap().restorable_losses();
            assert!(
                if maximum_health {
                    recorded.maximum_health > 0
                } else {
                    recorded.attributes.strength > 0
                },
                "the positive fixture must establish an actual restorable deficit"
            );
            assert_eq!(game.choose_ai(OWNER).unwrap().action, expected(scroll));
            let mut mixed = game.clone();
            mixed
                .items
                .edit(ITEM, |item| {
                    let effects = if scroll {
                        &mut item.spec.readable.as_mut().unwrap().effects
                    } else {
                        &mut item.spec.consumable.as_mut().unwrap().effects
                    };
                    effects.push(Effect::LoseMaximumHealth {
                        amount: crate::rules::AmountExpr::Fixed { amount: 1 },
                    });
                })
                .unwrap();
            assert_ne!(mixed.choose_ai(OWNER).unwrap().action, expected(scroll));
        }
    }
}

// These tests load the live authored effect lists and relevant condition records,
// while keeping actors/map deterministic and independent of Rogue generation.
fn authored_care_fixture(name: &str, scroll: bool) -> Game {
    let authored: toml::Value =
        toml::from_str(include_str!("../../../scenarios/rogue/catalog/items.toml")).unwrap();
    let patches: toml::Value =
        toml::from_str(include_str!("../../../scenarios/rogue/rules.toml")).unwrap();
    let mut document = serde_json::to_value(test_creatures::ruleset().document()).unwrap();
    for name in ["rogue.blindness", "rogue.hallucinating"] {
        let row = patches["overrides"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| {
                row["path"].as_array().is_some_and(|path| {
                    path.len() == 2
                        && path[0].as_str() == Some("conditions")
                        && path[1].as_str() == Some(name)
                })
            })
            .unwrap();
        document["conditions"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::to_value(&row["value"]).unwrap());
    }
    let rules = crate::rules::Ruleset::validate(serde_json::from_value(document).unwrap()).unwrap();
    let mut game =
        super::tests::configured_game(test_creatures::species(), test_creatures::species(), rules);
    let row = &authored["items"][name];
    let capability = &row[if scroll { "readable" } else { "consumable" }];
    let effects: Vec<Effect> = capability["effects"].clone().try_into().unwrap();
    let discovery = capability["discovery"].clone().try_into().unwrap();
    let mut item = ItemSpec::ordinary(
        row["identity"].as_str().unwrap().parse().unwrap(),
        row["name"].as_str().unwrap().into(),
    );
    item.class = if scroll {
        ItemClass::Scroll
    } else {
        ItemClass::Potion
    };
    item.concealed = true;
    item.stackable = true;
    if scroll {
        item.readable = Some(crate::ReadableSpec {
            control: None,
            descriptors: vec![],
            effects,
            identify: None,
            discovery,
        });
    } else {
        item.consumable = Some(crate::ConsumableSpec {
            descriptors: vec![],
            effects,
            discovery,
        });
    }
    game.place_item_stack(ITEM.0, game.actors[&OWNER].location, Some(OWNER), 1, item)
        .unwrap();
    game
}

// A unit-only external input boundary. No direct knowledge mutation or paid
// completion shortcut; ordinary scheduler work earns the consumption/discovery.
fn perform_care_test_action(game: &mut Game, action: Action) {
    let ai = game.combat.ai.remove(&OWNER).unwrap();
    for _ in 0..32 {
        if game.next_actor() == Some(OWNER) {
            break;
        }
        game.act(game.next_actor().unwrap(), Action::Wait).unwrap();
    }
    assert_eq!(game.next_actor(), Some(OWNER));
    game.admit_intention(OWNER, action, crate::IntentionOrigin::Human)
        .unwrap();
    game.execute_next_intention().unwrap().outcome.unwrap();
    for _ in 0..32 {
        if game.preparation(OWNER).is_none() {
            break;
        }
        game.act(game.next_actor().unwrap(), Action::Wait).unwrap();
    }
    assert!(game.preparation(OWNER).is_none());
    game.combat.ai.insert(OWNER, ai);
}

fn reserve_choice(game: &Game, care: bool) -> Option<Action> {
    let view = game.observe(OWNER).unwrap();
    assert!(!game.exposed_to_visible_hostile(OWNER, view.location, &view));
    let mut profile = AiProfile::general();
    profile.care = care;
    game.choose_nearby_loot(OWNER, &view, &profile, &[], &mut |_| {
        panic!("underfoot reserve needs no route")
    })
}

#[test]
fn authored_healing_discovered_by_drinking_is_reserved_from_ground() {
    for name in ["rogue.healing", "rogue.extra_healing"] {
        let mut game = authored_care_fixture(name, false);
        let spec = (*game.items[&ITEM].spec).clone();
        let identity = spec.identity.clone();
        let mut peer_at = game.actors[&ActorId(1)].location;
        peer_at.position.x = 4;
        game.teleport(ActorId(1), peer_at).unwrap();
        assert!(!game.actors[&OWNER].knowledge.contains(&identity));
        assert!(!game.actors[&ActorId(1)].knowledge.contains(&identity));
        perform_care_test_action(&mut game, Action::Drink { item: ITEM });
        assert!(!game.items.contains_key(&ITEM));
        assert!(game.actors[&OWNER].knowledge.contains(&identity));
        assert!(!game.actors[&ActorId(1)].knowledge.contains(&identity));
        let at = game.actors[&OWNER].location;
        let replacement = ItemId(42);
        game.place_item_stack(replacement.0, at, None, 3, spec)
            .unwrap();
        let view = game.observe(OWNER).unwrap();
        assert!(!game.exposed_to_visible_hostile(OWNER, at, &view));
        let before = game.clone();
        assert_eq!(
            game.choose_nearby_loot(OWNER, &view, &AiProfile::general(), &[], &mut |_| panic!(
                "underfoot reserve needs no route"
            ),),
            Some(Action::Take {
                item: replacement,
                quantity: Some(1)
            }),
            "ordinary discovery should make {name} eligible as one finite reserve"
        );
        assert_eq!(game, before);
        perform_care_test_action(
            &mut game,
            Action::Take {
                item: replacement,
                quantity: Some(1),
            },
        );
        let view = game.observe(OWNER).unwrap();
        assert_eq!(view.inventory.len(), 1);
        assert_eq!(view.inventory[0].quantity, 1);
        assert_eq!(game.items[&replacement].quantity, 2);
        assert_eq!(reserve_choice(&game, true), None);
        let (hp, maximum) = game.health(OWNER).unwrap();
        assert_eq!(hp, maximum, "reserve fixture remains at full health");
        assert!(!matches!(
            game.choose_ai(OWNER).unwrap().action,
            Action::Drink { .. }
        ));
        assert!(!game.actors[&ActorId(1)].knowledge.contains(&identity));
    }
}

#[test]
fn healing_reserve_rejects_unknown_foreign_and_unsafe_mixtures() {
    let mut game = authored_care_fixture("rogue.healing", false);
    assert!(!game.known_healing_reserve(OWNER, ITEM));
    game.identify_item(ActorId(1), ITEM).unwrap();
    assert!(!game.known_healing_reserve(OWNER, ITEM));
    game.identify_item(OWNER, ITEM).unwrap();
    assert!(game.known_healing_reserve(OWNER, ITEM));
    let before = game.clone();
    assert!(game.known_healing_reserve(OWNER, ITEM));
    assert_eq!(game, before);
    for kind in 0..4 {
        let mut altered = game.clone();
        altered
            .items
            .edit(ITEM, |item| {
                let effects = &mut item.spec.consumable.as_mut().unwrap().effects;
                match kind {
                    0 => effects.push(Effect::LoseMaximumHealth {
                        amount: crate::rules::AmountExpr::Fixed { amount: 1 },
                    }),
                    1 => effects.retain(|effect| matches!(effect, Effect::RemoveCondition { .. })),
                    2 => effects.push(Effect::RemoveCondition {
                        condition: ConditionId::new("unknown_reserve_condition").unwrap(),
                    }),
                    _ => effects.clear(),
                }
            })
            .unwrap();
        assert!(
            !altered.known_healing_reserve(OWNER, ITEM),
            "variant {kind}"
        );
    }
    for (sides, bonus) in [(0, 0), (4, -1)] {
        let mut nonpositive = game.clone();
        nonpositive
            .items
            .edit(ITEM, |item| {
                for effect in &mut item.spec.consumable.as_mut().unwrap().effects {
                    if let Effect::HealPerHitDie {
                        sides: old_sides,
                        bonus: old_bonus,
                        ..
                    } = effect
                    {
                        *old_sides = sides;
                        *old_bonus = bonus;
                    }
                }
            })
            .unwrap();
        assert!(!nonpositive.known_healing_reserve(OWNER, ITEM));
    }
    let mut legacy = game.clone();
    legacy
        .items
        .edit(ITEM, |item| {
            item.spec.consumable.as_mut().unwrap().effects = vec![Effect::Heal {
                amount: crate::rules::AmountExpr::Rolled {
                    dice: crate::rules::DiceExpr {
                        count: 1,
                        sides: 4,
                        bonus: -1,
                    },
                },
            }];
        })
        .unwrap();
    assert!(crate::dice::DicePool::new(1, 4, -1).unwrap().maximum() > 0);
    assert!(legacy.known_healing(OWNER, ITEM));
    assert!(
        legacy.known_healing_reserve(OWNER, ITEM),
        "legacy acceptance is unchanged"
    );
    let mut beneficial = fixture(false);
    beneficial.identify_item(OWNER, ITEM).unwrap();
    beneficial
        .items
        .edit(ITEM, |item| {
            item.spec.consumable.as_mut().unwrap().effects = vec![
                Effect::Heal {
                    amount: crate::rules::AmountExpr::Fixed { amount: 1 },
                },
                Effect::RemoveCondition {
                    condition: ConditionId::new("care_fixture_guard").unwrap(),
                },
            ];
        })
        .unwrap();
    assert!(!beneficial.known_healing_reserve(OWNER, ITEM));
}

#[test]
fn healing_reserve_is_shared_across_production_and_legacy_types_and_respects_care() {
    for carried_kind in 0..3 {
        let mut game = authored_care_fixture("rogue.healing", false);
        game.identify_item(OWNER, ITEM).unwrap();
        let mut peer_at = game.actors[&ActorId(1)].location;
        peer_at.position.x = 4;
        game.teleport(ActorId(1), peer_at).unwrap();
        let production = (*game.items[&ITEM].spec).clone();
        let mut legacy = ItemSpec::ordinary(
            "item.test.legacy-reserve".parse().unwrap(),
            "legacy reserve".into(),
        );
        legacy.class = ItemClass::Potion;
        legacy.stackable = true;
        legacy.consumable = Some(crate::ConsumableSpec {
            descriptors: vec![],
            discovery: crate::ItemDiscovery::Never,
            effects: vec![Effect::Heal {
                amount: crate::rules::AmountExpr::Fixed { amount: 1 },
            }],
        });
        let ground = match carried_kind {
            0 => legacy,
            1 => {
                game.items.edit(ITEM, |item| *item.spec = legacy).unwrap();
                production
            }
            _ => (*authored_care_fixture("rogue.extra_healing", false).items[&ITEM].spec).clone(),
        };
        let at = game.actors[&OWNER].location;
        let replacement = ItemId(42);
        game.place_item_stack(replacement.0, at, None, 3, ground)
            .unwrap();
        game.identify_item(OWNER, replacement).unwrap();
        assert!(game.known_healing_reserve(OWNER, ITEM));
        assert!(game.known_healing_reserve(OWNER, replacement));
        // Positive pickup without carried reserve establishes the suppression
        // is not an unknown item, blocked route, or unrelated policy failure.
        let mut without_carried = game.clone();
        perform_care_test_action(&mut without_carried, Action::Drink { item: ITEM });
        assert_eq!(
            reserve_choice(&without_carried, true),
            Some(Action::Take {
                item: replacement,
                quantity: Some(1),
            })
        );
        assert_eq!(reserve_choice(&without_carried, false), None);
        assert_eq!(
            reserve_choice(&game, true),
            None,
            "carried kind {carried_kind}"
        );
    }
}

#[test]
fn authored_rogue_healing_and_extra_healing_cure_without_identity_cheats() {
    for name in ["rogue.healing", "rogue.extra_healing"] {
        let mut game = authored_care_fixture(name, false);
        game.apply_effects(
            OWNER,
            &[Effect::ApplyCondition {
                condition: ConditionId::new("rogue.blindness").unwrap(),
                duration: 400,
            }],
        )
        .unwrap();
        assert_ne!(game.choose_ai(OWNER).unwrap().action, expected(false));
        game.identify_item(OWNER, ITEM).unwrap();
        assert_eq!(
            game.choose_ai(OWNER).unwrap().action,
            expected(false),
            "known cure is useful at full health without seeing a foe"
        );
        game.apply_effects(
            OWNER,
            &[Effect::RemoveCondition {
                condition: ConditionId::new("rogue.blindness").unwrap(),
            }],
        )
        .unwrap();
        assert_ne!(
            game.choose_ai(OWNER).unwrap().action,
            expected(false),
            "no deficit: retain finite medicine"
        );
        let damage = crate::damage::DamageSpec::new(
            vec![crate::damage::DamageComponent::fixed(
                crate::combat::DamageType::Vital,
                None,
                15,
            )],
            None,
        )
        .unwrap();
        game.apply_damage(OWNER, &damage).unwrap();
        assert_eq!(game.choose_ai(OWNER).unwrap().action, expected(false));
    }
}

fn authored_care_armor(game: &mut Game, cursed: bool) -> ItemId {
    let armor = ItemId(42);
    let slot = crate::EquipmentSlot::new("body_armor").unwrap();
    let socket = game
        .creature(OWNER)
        .unwrap()
        .derived()
        .anatomy
        .slots
        .iter()
        .position(|entry| *entry == slot)
        .unwrap();
    let mut item = ItemSpec::ordinary(
        "item.armor.care-fixture".parse().unwrap(),
        "plain armor".into(),
    );
    item.class = ItemClass::Armor;
    item.equipment = Some(crate::EquipmentSpec {
        periodic_relocation: None,
        potency_modifiers: vec![],
        weapon_profile: None,
        armor: Some(crate::armor::ArmorSpec {
            physical_reduction: 2,
            material: crate::armor::ArmorMaterial::Metal,
            rustproof: false,
        }),
        slot,
        attack: None,
        modifiers: vec![],
    });
    game.place_item_stack(armor.0, game.actors[&OWNER].location, Some(OWNER), 1, item)
        .unwrap();
    game.items
        .edit(armor, |item| item.condition.cursed = cursed)
        .unwrap();
    game.equip_authored(OWNER, armor, crate::EquipmentSlotId(socket as u16))
        .unwrap();
    armor
}

#[test]
fn authored_rogue_utility_scrolls_use_remembered_equipment_facts_only() {
    for remove_curse in [false, true] {
        let name = if remove_curse {
            "rogue.remove_curse"
        } else {
            "rogue.protect_armor"
        };
        let mut game = authored_care_fixture(name, true);
        let armor = authored_care_armor(&mut game, remove_curse);
        game.identify_item(OWNER, ITEM).unwrap();
        assert_ne!(
            game.choose_ai(OWNER).unwrap().action,
            expected(true),
            "unobserved instance state is not available to AI"
        );
        game.identify_item(OWNER, armor).unwrap();
        assert_eq!(game.choose_ai(OWNER).unwrap().action, expected(true));
        game.items
            .edit(armor, |item| {
                item.condition.cursed = false;
                item.condition.protected = true;
            })
            .unwrap();
        assert_eq!(
            game.choose_ai(OWNER).unwrap().action,
            expected(true),
            "hidden mutation cannot alter remembered utility"
        );
        game.identify_item(OWNER, armor).unwrap();
        assert_ne!(
            game.choose_ai(OWNER).unwrap().action,
            expected(true),
            "known cured/protected item is not a useful target"
        );
    }
}
