use super::*;
use crate::{combat::DamageType, test_creatures};
use std::{collections::BTreeSet, num::NonZeroU64};
use tor_world::{Position, RegionId};
pub(super) fn at(region: u64, x: i32, y: i32) -> Location {
    Location {
        region: RegionId(region),
        position: Position { x, y, z: 0 },
    }
}
#[test]
fn explore_equal_neighbors_use_actor_rng_without_mutating_preview() {
    let mut game = fixture();
    game.configure_run(ActorId(1), BTreeSet::from([ActorId(1)]), BTreeMap::new())
        .unwrap();
    let mut choices = BTreeSet::new();
    for seed in 0..26 {
        game.actors.get_mut(&ActorId(2)).unwrap().behavior.rng = seed;
        let before = game.clone();
        let decision = game.choose_ai(ActorId(2)).unwrap();
        assert!(matches!(decision.action, Action::Move(_)));
        assert_eq!(decision.action, game.choose_ai(ActorId(2)).unwrap().action);
        assert_eq!(game, before, "preview must not commit RNG or visit history");
        assert_ne!(
            decision.behavior.rng,
            before.actors[&ActorId(2)].behavior.rng
        );
        choices.insert(format!("{:?}", decision.action));
    }
    assert!(
        choices.len() >= 3,
        "equally good neighbors must not all resolve by coordinate: {choices:?}"
    );
}

#[test]
fn explore_local_fallback_unique_neighbor_and_idle_wait_do_not_draw() {
    let mut game = fixture();
    game.configure_run(ActorId(1), BTreeSet::from([ActorId(1)]), BTreeMap::new())
        .unwrap();
    let candidates: Vec<_> = Direction::HORIZONTAL
        .into_iter()
        .filter_map(|direction| {
            game.actor_translation(ActorId(2), direction)
                .map(|(at, _)| (at, direction))
        })
        .collect();
    assert!(candidates.len() > 2);
    let preferred = candidates[1];
    for (at, _) in candidates {
        game.combat
            .ai
            .get_mut(&ActorId(2))
            .unwrap()
            .visits
            .insert(at, if at == preferred.0 { 0 } else { 10 });
    }
    let rng = game.actors[&ActorId(2)].behavior.rng;
    let before = game.clone();
    let view = game.observe(ActorId(2)).unwrap();
    let mut ai = game.combat.ai[&ActorId(2)].clone();
    let mut behavior = game.actors[&ActorId(2)].behavior.clone();
    // This is the local fallback contract, now below the destination stage.
    // Missing remembered routing cannot make the unique local choice random.
    let action = game.choose_exploration(ActorId(2), &mut ai, &mut behavior, false, &view, None);
    assert_eq!(action, Some(Action::Move(preferred.1)));
    assert_eq!(behavior.rng, rng);
    assert_eq!(game, before);
    game.combat.ai.get_mut(&ActorId(2)).unwrap().profile.idle = crate::behavior::IdleBehavior::Wait;
    let decision = game.choose_ai(ActorId(2)).unwrap();
    assert_eq!(decision.action, Action::Wait);
    assert_eq!(decision.behavior.rng, rng);
}

#[test]
fn explore_rng_commits_with_execution_and_survives_checkpoint() {
    let mut game = fixture();
    game.configure_run(ActorId(1), BTreeSet::from([ActorId(1)]), BTreeMap::new())
        .unwrap();
    game.act(ActorId(1), Action::Wait).unwrap();
    let chosen = game.choose_ai(ActorId(2)).unwrap();
    let rng = game.actors[&ActorId(2)].behavior.rng;
    game.admit_ai_intention(ActorId(2)).unwrap();
    assert_eq!(game.actors[&ActorId(2)].behavior.rng, rng);
    let mut shared = crate::checkpoint::SharedState::default();
    let snapshot =
        serde_json::from_value(serde_json::to_value(game.checkpoint(&mut shared)).unwrap())
            .unwrap();
    let shared = serde_json::from_value(serde_json::to_value(shared).unwrap()).unwrap();
    let mut restored = Game::restore_checkpoint(snapshot, &shared, game.content().clone()).unwrap();
    for candidate in [&mut game, &mut restored] {
        let execution = candidate.execute_next_intention().unwrap();
        assert_eq!(execution.action, Some(chosen.action.clone()));
        execution.outcome.unwrap();
        assert_eq!(
            candidate.actors[&ActorId(2)].behavior.rng,
            chosen.behavior.rng
        );
        assert_eq!(candidate.combat.ai[&ActorId(2)].visits, chosen.ai.visits);
    }
    assert_eq!(game, restored);
}

#[test]
fn invalid_ai_profile_and_missing_creature_have_distinct_errors() {
    let mut game = fixture();
    let before = game.clone();
    assert_eq!(
        game.configure_ai(
            ActorId(2),
            AiProfile {
                flee_percent: 101,
                ..AiProfile::general()
            }
        ),
        Err(GameError::Rule(
            crate::abilities::RuleError::InvalidDefinition
        ))
    );
    assert_eq!(
        game.configure_ai(ActorId(999), AiProfile::general()),
        Err(GameError::UnknownActor)
    );
    assert_eq!(game, before);
}

fn fixture() -> Game {
    fixture_with_anatomy(crate::AnatomySpec::from_rules(
        &crate::test_creatures::ruleset(),
    ))
}
fn fixture_with_anatomy(anatomy: crate::AnatomySpec) -> Game {
    let hero = test_creatures::species();
    let mut foe = test_creatures::species();
    foe.anatomy = anatomy;
    configured_game(hero, foe, paid_probe_rules())
}
pub(super) fn configured_game(
    mut hero: crate::creatures::Species,
    mut foe: crate::creatures::Species,
    rules: std::sync::Arc<crate::rules::Ruleset>,
) -> Game {
    use crate::{
        attributes::ManaBinding,
        content_ids::SpeciesId,
        creatures::{Content, CreatureBuild, ScenarioCatalog},
        progression::{HdLedger, HdSource, HitDie},
    };
    hero.id = SpeciesId::new("hero").unwrap();
    foe.id = SpeciesId::new("foe").unwrap();
    foe.behavior.memory_ticks = 50;
    let content = Content::new(
        rules,
        ScenarioCatalog {
            species: BTreeMap::from([(hero.id.clone(), hero), (foe.id.clone(), foe)]),
            templates: BTreeMap::new(),
        },
    )
    .unwrap();
    let mut game = Game::two_room_in_stone(42, content.clone());
    for (x, name, faction, corpse_identity) in [
        (1, "hero", "hero", "item.corpse.test.hero"),
        (2, "foe", "foe", "item.corpse.test.foe"),
    ] {
        let actor = game
            .spawn_actor(at(1, x, 1), NonZeroU64::new(100).unwrap())
            .unwrap();
        let build = CreatureBuild::new(
            content.clone(),
            SpeciesId::new(name).unwrap(),
            HdLedger::new(vec![HitDie::new(HdSource::Racial, 42)]).unwrap(),
            ManaBinding::Intellect,
            [
                tor_simulation::attributes::Skill::Deception,
                tor_simulation::attributes::Skill::Persuasion,
            ],
        )
        .unwrap();
        game.configure_creature(
            actor,
            crate::CreatureIdentity {
                corpse_identity: corpse_identity.parse().unwrap(),
                name: name.into(),
                faction: faction.into(),
            },
            build,
        )
        .unwrap();
    }
    game.configure_run(
        ActorId(1),
        BTreeSet::from([ActorId(1)]),
        BTreeMap::from([("foe".into(), BTreeSet::from(["hero".into()]))]),
    )
    .unwrap();
    game.configure_ai(ActorId(2), AiProfile::general()).unwrap();
    game.refresh_navigation();
    game
}
fn paid_probe_rules() -> std::sync::Arc<crate::rules::Ruleset> {
    let mut document = test_creatures::ruleset().document().clone();
    document.abilities.extend([
        test_creatures::paid_melee(),
        test_creatures::mana_ray(),
        test_creatures::focus_fear(),
    ]);
    crate::rules::Ruleset::validate(document).unwrap()
}
pub(super) fn paid_fixture(magical: bool, fear: bool) -> Game {
    use crate::{
        attributes::{Attributes, ByAttribute},
        content_ids::{AbilityId, DescriptorId, FlagId},
        rules::{Modifier, ModifierOp, Scope, Selector},
    };
    let mut hero = test_creatures::species();
    hero.modifiers.push(Modifier {
        cancellable: false,
        scope: Scope::Always,
        op: ModifierOp::Immunity {
            selector: Selector::Descriptor {
                descriptor: DescriptorId::new("fear").unwrap(),
            },
        },
    });
    let mut foe = test_creatures::species();
    foe.default_attributes = Attributes::new(ByAttribute {
        strength: 2,
        speed: 1,
        intellect: 2,
        willpower: 2,
        awareness: 1,
        presence: 1,
    })
    .unwrap();
    for name in ["fixture_paid_melee", "fixture_mana_ray"]
        .into_iter()
        .chain(fear.then_some("fixture_focus_fear"))
    {
        foe.modifiers.push(Modifier {
            cancellable: false,
            scope: Scope::Always,
            op: ModifierOp::GrantAbility {
                ability: AbilityId::new(name).unwrap(),
            },
        });
    }
    if magical {
        foe.modifiers.push(Modifier {
            cancellable: false,
            scope: Scope::Always,
            op: ModifierOp::Flag {
                flag: FlagId::new("magical").unwrap(),
            },
        });
    }
    configured_game(hero, foe, paid_probe_rules())
}
pub(super) fn basic(target: ActorId) -> Action {
    Action::UseAbility {
        id: crate::content_ids::AbilityId::new("basic_melee").unwrap(),
        invocation: crate::abilities::AbilityInvocation::actor_target(target),
        supplies: Default::default(),
    }
}
fn heal(amount: u32) -> crate::rules::Effect {
    crate::rules::Effect::Heal {
        amount: crate::rules::AmountExpr::Fixed { amount },
    }
}
fn injury(game: &mut Game, actor: ActorId, amount: u32) {
    let damage = crate::damage::DamageSpec::new(
        vec![crate::damage::DamageComponent::fixed(
            DamageType::Vital,
            None,
            amount,
        )],
        None,
    )
    .unwrap();
    game.apply_damage(actor, &damage).unwrap();
}

#[test]
fn paid_choices_do_not_read_hidden_fear_immunity() {
    let game = paid_fixture(false, true);
    assert!(game
        .creature(ActorId(1))
        .unwrap()
        .derived()
        .protection
        .has_immunity(&crate::rules::Selector::Descriptor {
            descriptor: crate::content_ids::DescriptorId::new("fear").unwrap()
        }));
    let before = game.clone();
    assert_eq!(
        game.choose_ai(ActorId(2)).unwrap().action,
        Action::UseAbility {
            id: crate::content_ids::AbilityId::new("fixture_focus_fear").unwrap(),
            invocation: crate::abilities::AbilityInvocation::actor_target(ActorId(1)),
            supplies: Default::default(),
        }
    );
    assert_eq!(game, before);
}

#[test]
fn paid_choices_use_granted_power_strike_without_mutating_state() {
    let game = paid_fixture(false, false);
    let before = game.clone();
    assert_eq!(
        game.choose_ai(ActorId(2)).unwrap().action,
        Action::UseAbility {
            id: crate::content_ids::AbilityId::new("fixture_paid_melee").unwrap(),
            invocation: crate::abilities::AbilityInvocation::actor_target(ActorId(1)),
            supplies: Default::default(),
        }
    );
    assert_eq!(game, before);
}

fn ranged_specialist_fixture(magical: bool) -> Game {
    ranged_specialist_fixture_with_modifiers(magical, Vec::new())
}
fn ranged_specialist_fixture_with_modifiers(
    magical: bool,
    modifiers: Vec<crate::rules::Modifier>,
) -> Game {
    use crate::{
        content_ids::{AbilityId, FlagId},
        rules::{Modifier, ModifierOp, Scope},
    };
    let mut foe = test_creatures::species();
    for op in [
        ModifierOp::GrantAbility {
            ability: AbilityId::new("fixture_mana_ray").unwrap(),
        },
        ModifierOp::Flag {
            flag: FlagId::new("magical").unwrap(),
        },
    ] {
        if magical || matches!(op, ModifierOp::GrantAbility { .. }) {
            foe.modifiers.push(Modifier {
                cancellable: false,
                scope: Scope::Always,
                op,
            });
        }
    }
    foe.modifiers.extend(modifiers);
    configured_game(test_creatures::species(), foe, paid_probe_rules())
}

#[test]
fn funded_ranged_specialist_repositions_before_basic_melee() {
    let game = ranged_specialist_fixture(true);
    let before = game.clone();
    let decision = game.choose_ai(ActorId(2)).unwrap();
    assert!(
        matches!(decision.action, Action::Move(_)),
        "funded ranged specialist should seek shooting space, got {:?}",
        decision.action
    );
    assert_eq!(game, before, "preview remains pure");
}

#[test]
fn ranged_positioning_supports_upright_eye_height() {
    let mut game = ranged_specialist_fixture(true);
    game.set_body(
        ActorId(2),
        crate::BodySpec {
            cells: vec![[0, 0, 0], [0, 0, 1]],
            eye: [0, 0, 1],
            mass: 80,
        },
    )
    .unwrap();
    assert!(matches!(
        game.choose_ai(ActorId(2)).unwrap().action,
        Action::Move(_)
    ));
}

#[test]
fn ranged_positioning_rejects_negative_eye_without_disclosure() {
    let mut game = ranged_specialist_fixture(true);
    for actor in [ActorId(1), ActorId(2)] {
        let mut location = game.actors[&actor].location;
        location.position.z = 1;
        game.teleport(actor, location).unwrap();
    }
    game.set_body(
        ActorId(2),
        crate::BodySpec {
            cells: vec![[0, 0, 0], [0, 0, -1]],
            eye: [0, 0, -1],
            mass: 80,
        },
    )
    .unwrap();
    let snapshot = game.decision_snapshot(ActorId(2)).unwrap();
    let mut view = snapshot.view.clone();
    assert!(view
        .visible_actors
        .iter()
        .any(|other| other.id == ActorId(1)));
    view.visible_cells = tor_world::Shared::new(Vec::new());
    assert_eq!(
        game.ai_ranged_reposition(
            ActorId(2),
            ActorId(1),
            &snapshot.sight,
            &snapshot.scene,
            &view
        ),
        None
    );
}

#[test]
fn ranged_positioning_rejects_wide_or_pitched_own_bodies() {
    let mut wide = ranged_specialist_fixture(true);
    wide.set_body(
        ActorId(2),
        crate::BodySpec {
            cells: vec![[0, 0, 0], [0, 1, 0]],
            eye: [0, 0, 0],
            mass: 80,
        },
    )
    .unwrap();
    assert!(!matches!(
        wide.choose_ai(ActorId(2)).unwrap().action,
        Action::Move(_)
    ));
    let mut pitched = ranged_specialist_fixture(true);
    pitched
        .set_body(
            ActorId(2),
            crate::BodySpec {
                cells: vec![[0, 0, 0], [0, 0, 1]],
                eye: [0, 0, 1],
                mass: 80,
            },
        )
        .unwrap();
    let rotation = (0..24)
        .find(|r| tor_world::rotate_vector(*r, [0, 0, 1]) == [1, 0, 0])
        .unwrap();
    pitched.actors.get_mut(&ActorId(2)).unwrap().orientation = rotation;
    pitched.refresh_navigation();
    assert!(!matches!(
        pitched.choose_ai(ActorId(2)).unwrap().action,
        Action::Move(_)
    ));
}

#[test]
fn ranged_positioning_requires_disclosed_destination_and_corridor() {
    let game = ranged_specialist_fixture(true);
    let snapshot = game.decision_snapshot(ActorId(2)).unwrap();
    assert!(game
        .ai_ranged_reposition(
            ActorId(2),
            ActorId(1),
            &snapshot.sight,
            &snapshot.scene,
            &snapshot.view
        )
        .is_some());
    let mut view = snapshot.view.clone();
    view.visible_cells = tor_world::Shared::new(Vec::new());
    assert_eq!(
        game.ai_ranged_reposition(
            ActorId(2),
            ActorId(1),
            &snapshot.sight,
            &snapshot.scene,
            &view
        ),
        None
    );
}

#[test]
fn ranged_positioning_requires_upper_body_disclosure() {
    let mut game = ranged_specialist_fixture(true);
    game.set_body(
        ActorId(2),
        crate::BodySpec {
            cells: vec![[0, 0, 0], [0, 0, 1]],
            eye: [0, 0, 0],
            mass: 80,
        },
    )
    .unwrap();
    let snapshot = game.decision_snapshot(ActorId(2)).unwrap();
    assert!(game
        .ai_ranged_reposition(
            ActorId(2),
            ActorId(1),
            &snapshot.sight,
            &snapshot.scene,
            &snapshot.view
        )
        .is_some());
    let mut view = snapshot.view.clone();
    // Keep the entire ground-level corridor and support, omit only upper
    // destination body cells. Eye height zero keeps them outside the corridor.
    view.visible_cells = tor_world::Shared::new(
        view.visible_cells
            .iter()
            .filter(|cell| !(cell.location.position.x == 3 && cell.location.position.z == 1))
            .cloned()
            .collect(),
    );
    assert_eq!(
        game.ai_ranged_reposition(
            ActorId(2),
            ActorId(1),
            &snapshot.sight,
            &snapshot.scene,
            &view
        ),
        None
    );
}

#[test]
fn ranged_positioning_requires_intermediate_corridor_disclosure() {
    let game = ranged_specialist_fixture(true);
    let snapshot = game.decision_snapshot(ActorId(2)).unwrap();
    assert!(game
        .ai_ranged_reposition(
            ActorId(2),
            ActorId(1),
            &snapshot.sight,
            &snapshot.scene,
            &snapshot.view
        )
        .is_some());
    let mut view = snapshot.view.clone();
    // Preserve every destination cell and ground support. Each candidate's
    // rectangle crosses x=2, between the target x=1 and destination x=3.
    view.visible_cells = tor_world::Shared::new(
        view.visible_cells
            .iter()
            .filter(|cell| !(cell.location.position.x == 2 && cell.location.position.z == 0))
            .cloned()
            .collect(),
    );
    assert_eq!(
        game.ai_ranged_reposition(
            ActorId(2),
            ActorId(1),
            &snapshot.sight,
            &snapshot.scene,
            &view
        ),
        None
    );
}

#[test]
fn ranged_positioning_requires_support_occurrence_not_location_alias() {
    let game = ranged_specialist_fixture(true);
    let snapshot = game.decision_snapshot(ActorId(2)).unwrap();
    assert!(game
        .ai_ranged_reposition(
            ActorId(2),
            ActorId(1),
            &snapshot.sight,
            &snapshot.scene,
            &snapshot.view
        )
        .is_some());
    // Projection-boundary witness: location-only public rows remain, but
    // candidate footing is no longer disclosed at its physical offset.
    // This is not claimed to construct a real-world portal failure.
    let scene: Vec<_> = snapshot
        .scene
        .iter()
        .filter(|cell| {
            !(cell.location.region == RegionId(1)
                && cell.location.position.x == 3
                && cell.location.position.z == -1)
        })
        .copied()
        .collect();
    assert_eq!(
        game.ai_ranged_reposition(
            ActorId(2),
            ActorId(1),
            &snapshot.sight,
            &scene,
            &snapshot.view
        ),
        None
    );
}

#[test]
fn ranged_positioning_footing_is_yaw_equivariant() {
    for orientation in (0..24).filter(|r| tor_world::rotate_vector(*r, [0, 0, 1]) == [0, 0, 1]) {
        let mut game = ranged_specialist_fixture(true);
        game.actors.get_mut(&ActorId(2)).unwrap().orientation = orientation;
        game.refresh_navigation();
        assert!(
            matches!(game.choose_ai(ActorId(2)).unwrap().action, Action::Move(_)),
            "upright yaw {orientation} must retain disclosed footing"
        );
    }
}

#[test]
fn ranged_positioning_rejects_non_downward_current_gravity() {
    for gravity in [[0, 0, 1], [1, 0, 0], [-1, 0, 0], [0, 1, 0], [0, -1, 0]] {
        let mut game = ranged_specialist_fixture(true);
        game.world.set_gravity(RegionId(1), gravity).unwrap();
        let snapshot = game.decision_snapshot(ActorId(2)).unwrap();
        assert_eq!(
            game.ai_ranged_reposition(
                ActorId(2),
                ActorId(1),
                &snapshot.sight,
                &snapshot.scene,
                &snapshot.view
            ),
            None,
            "unsupported current gravity {gravity:?}"
        );
    }
}

#[test]
fn ranged_positioning_physical_downward_portal_boundary() {
    for rotation in (0..24).filter(|r| tor_world::rotate_vector(*r, [0, 0, 1]) == [0, 0, 1]) {
        let mut game = ranged_specialist_fixture(true);
        game.world.set_gravity(RegionId(1), [0, 0, -1]).unwrap();
        let source = at(1, 3, 1);
        let destination = at(2, 1, 1);
        game.connect_portal_area(
            tor_world::Passage {
                from: source,
                direction: Direction::Down,
                to: destination,
            },
            rotation,
            1,
            1,
        )
        .unwrap();
        let below = Location {
            position: Position {
                z: -1,
                ..source.position
            },
            ..source
        };
        assert!(game.world.is_wall(below));
        assert_eq!(
            game.world.physics_neighbor(source, Direction::Down),
            Some((destination, rotation))
        );
        let snapshot = game.decision_snapshot(ActorId(2)).unwrap();
        let cross_region = snapshot.scene.iter().any(|cell| {
            cell.physical && cell.offset.z < 0 && cell.location.region != source.region
        });
        let disclosed_coordinate_support = snapshot
            .view
            .visible_cells
            .iter()
            .any(|cell| cell.location == below && cell.wall);
        assert!(
            cross_region || !disclosed_coordinate_support,
            "real snapshot must disclose the crossing or withhold coordinate-only support"
        );
        assert_ne!(
            game.ai_ranged_reposition(
                ActorId(2),
                ActorId(1),
                &snapshot.sight,
                &snapshot.scene,
                &snapshot.view
            ),
            Some(Direction::East)
        );
    }
}

#[test]
fn ranged_positioning_avoids_every_disclosed_hostile_body_cell() {
    let mut game = ranged_specialist_fixture(true);
    game.set_body(
        ActorId(1),
        crate::BodySpec {
            cells: vec![[0, 0, 0], [1, -1, 0]],
            eye: [0, 0, 0],
            mass: 80,
        },
    )
    .unwrap();
    let view = game.observe(ActorId(2)).unwrap();
    assert_eq!(
        view.visible_actors
            .iter()
            .filter(|row| row.id == ActorId(1))
            .count(),
        2
    );
    let Action::Move(direction) = game.choose_ai(ActorId(2)).unwrap().action else {
        panic!("expected disclosed safe alternative");
    };
    let (at, _) = game.actor_translation(ActorId(2), direction).unwrap();
    assert!(view
        .visible_actors
        .iter()
        .filter(|row| row.id == ActorId(1))
        .all(|row| at
            .position
            .x
            .abs_diff(row.location.position.x)
            .max(at.position.y.abs_diff(row.location.position.y))
            > 1));
}

#[test]
fn ranged_positioning_effective_range_guard() {
    use crate::rules::{AbilityStat, Modifier, ModifierOp, Scope, Stat};
    let game = ranged_specialist_fixture_with_modifiers(
        true,
        vec![Modifier {
            cancellable: false,
            scope: Scope::Always,
            op: ModifierOp::Add {
                stat: Stat::Ability {
                    ability: crate::content_ids::AbilityId::new("fixture_mana_ray").unwrap(),
                    stat: AbilityStat::Range,
                },
                value: -5,
            },
        }],
    );
    assert_eq!(
        game.creature(ActorId(2))
            .unwrap()
            .derived()
            .ability_reach(&crate::content_ids::AbilityId::new("fixture_mana_ray").unwrap())
            .unwrap(),
        crate::rules::Reach::Sight { range: 1 }
    );
    assert_eq!(
        game.choose_ai(ActorId(2)).unwrap().action,
        basic(ActorId(1))
    );
}

#[test]
fn ranged_positioning_ledge_guard() {
    let mut game = ranged_specialist_fixture(true);
    game.world.set_gravity(RegionId(1), [0, 0, -1]).unwrap();
    for y in 0..3 {
        game.set_wall(
            Location {
                region: RegionId(1),
                position: Position { x: 3, y, z: -1 },
            },
            false,
        )
        .unwrap();
    }
    let snapshot = game.decision_snapshot(ActorId(2)).unwrap();
    assert!(
        game.actor_translation(ActorId(2), Direction::East)
            .is_some(),
        "a legal step is not proof of support"
    );
    assert_eq!(
        game.ai_ranged_reposition(
            ActorId(2),
            ActorId(1),
            &snapshot.sight,
            &snapshot.scene,
            &snapshot.view
        ),
        None
    );
}

#[test]
fn unfunded_ranged_specialist_does_not_reposition() {
    let game = ranged_specialist_fixture(false);
    assert_eq!(
        game.choose_ai(ActorId(2)).unwrap().action,
        basic(ActorId(1))
    );
}

#[test]
fn ranged_specialist_uses_created_space_for_a_shot() {
    let mut game = ranged_specialist_fixture(true);
    let Action::Move(direction) = game.choose_ai(ActorId(2)).unwrap().action else {
        panic!("expected reposition")
    };
    let (at, _) = game.actor_translation(ActorId(2), direction).unwrap();
    game.act(ActorId(1), Action::Wait).unwrap();
    game.act(ActorId(2), Action::Move(direction)).unwrap();
    assert_eq!(game.actors[&ActorId(2)].location, at);
    assert!(
        matches!(game.choose_ai(ActorId(2)).unwrap().action, Action::UseAbility { id, .. } if id.as_str() == "fixture_mana_ray")
    );
}

#[test]
fn paid_choices_preserve_mana_for_ranged_combat() {
    let mut unfunded = paid_fixture(false, false);
    unfunded.teleport(ActorId(1), at(1, 4, 1)).unwrap();
    assert_eq!(
        unfunded.choose_ai(ActorId(2)).unwrap().action,
        Action::Move(Direction::East)
    );
    let mut game = paid_fixture(true, false);
    game.teleport(ActorId(1), at(1, 4, 1)).unwrap();
    assert_eq!(
        game.choose_ai(ActorId(2)).unwrap().action,
        Action::UseAbility {
            id: crate::content_ids::AbilityId::new("fixture_mana_ray").unwrap(),
            invocation: crate::abilities::AbilityInvocation::actor_target(ActorId(1)),
            supplies: Default::default(),
        }
    );
}
#[test]
fn one_decision_shares_its_search_across_visible_targets() {
    let mut game = fixture();
    for actor in [ActorId(1), ActorId(2)] {
        assert!(
            game.creature(actor).is_some(),
            "AI subjects own their builds"
        );
    }
    let other = game
        .spawn_actor(at(1, 3, 1), NonZeroU64::new(100).unwrap())
        .unwrap();
    let build = game.creature(ActorId(1)).unwrap().build().clone();
    game.configure_creature(
        other,
        crate::CreatureIdentity {
            corpse_identity: "item.corpse.test.other".parse().unwrap(),
            name: "other".into(),
            faction: "hero".into(),
        },
        build,
    )
    .unwrap();
    game.refresh_navigation();
    assert_eq!(game.observe(ActorId(2)).unwrap().visible_actors.len(), 2);
    let before = crate::diagnostics::work_counts();
    let AiDecision { action, .. } = game.choose_ai(ActorId(2)).unwrap();
    assert_eq!(action, basic(ActorId(1)));
    let after = crate::diagnostics::work_counts();
    assert_eq!(after.route_searches - before.route_searches, 1);
    assert_eq!(
        after.scenes - before.scenes,
        0,
        "the decision reuses the exact geometry warmed by observation"
    );
}

#[test]
fn hidden_movement_does_not_update_target_memory_and_memory_expires() {
    let mut game = fixture();
    let AiDecision { ai, behavior, .. } = game.choose_ai(ActorId(2)).unwrap();
    let expected = Some(crate::behavior::Engagement {
        reference: crate::perception::ActorReferenceKind::Creature,
        target: ActorId(1),
        location: at(1, 1, 1),
        last_seen: 0,
    });
    assert_eq!(behavior.engagement, expected);
    game.actors.get_mut(&ActorId(2)).unwrap().behavior = behavior;
    game.combat.ai.insert(ActorId(2), ai);
    game.set_region_light(RegionId(2), false).unwrap();
    game.teleport(ActorId(1), at(2, 4, 2)).unwrap();
    let AiDecision {
        behavior: remembered,
        ..
    } = game.choose_ai(ActorId(2)).unwrap();
    assert_eq!(remembered.engagement, expected);
    game.tick = 50;
    let AiDecision {
        ai,
        behavior: expired,
        ..
    } = game.choose_ai(ActorId(2)).unwrap();
    assert!(expired.engagement.is_none());
    assert_eq!(ai.state, State::Search);
}
#[test]
fn wounded_ai_flees_and_a_cornered_ai_fights() {
    let mut game = fixture();
    injury(&mut game, ActorId(2), 25);
    let AiDecision { action, ai, .. } = game.choose_ai(ActorId(2)).unwrap();
    assert_eq!(ai.state, State::Flee);
    assert!(matches!(action, Action::Move(_)));
    for (x, y) in [(1, 0), (1, 2), (2, 0), (2, 2), (3, 0), (3, 1), (3, 2)] {
        game.set_wall(at(1, x, y), true).unwrap();
    }
    let AiDecision { action, ai, .. } = game.choose_ai(ActorId(2)).unwrap();
    assert_eq!(ai.state, State::Flee);
    assert_eq!(action, basic(ActorId(1)));
}
#[test]
fn a_scenario_without_low_health_flight_keeps_wounded_ai_fighting() {
    let mut game = fixture();
    game.scenario_capabilities_mut().low_health_flight = false;
    injury(&mut game, ActorId(2), 25);
    let AiDecision { action, ai, .. } = game.choose_ai(ActorId(2)).unwrap();
    assert_eq!(ai.state, State::Attack);
    assert_eq!(action, basic(ActorId(1)));
}

fn ring(game: &mut Game, item: u64, defense: i32, concealed: bool) {
    let mut spec = crate::ItemSpec::ordinary(
        format!("item.test.ring-{item}").parse().unwrap(),
        format!("ring-{item}"),
    );
    spec.class = crate::ItemClass::Ring;
    spec.concealed = concealed;
    spec.appearance = "silver ring".into();
    spec.equipment = Some(crate::EquipmentSpec {
        periodic_relocation: None,
        potency_modifiers: Vec::new(),
        weapon_profile: None,
        armor: None,
        slot: crate::EquipmentSlot::new("ring").unwrap(),
        attack: None,
        modifiers: vec![crate::rules::Modifier {
            cancellable: false,
            scope: crate::rules::Scope::Always,
            op: crate::rules::ModifierOp::Add {
                stat: crate::rules::Stat::Defense {
                    defense: crate::rules::DefenseKind::Physical,
                },
                value: i64::from(defense),
            },
        }],
    });
    game.place_item_stack(item, at(1, 2, 1), Some(ActorId(2)), 1, spec)
        .unwrap();
}

#[test]
fn loot_pursues_distant_remembered_routes_but_preserves_safe_first_step() {
    use tor_world::{Extent, Region, World};
    let mut world = World::new(vec![], vec![]).unwrap();
    world
        .add_region(Region {
            id: RegionId(1),
            name: "arena".into(),
            bounds: Extent::new(10, 5, 1).unwrap(),
        })
        .unwrap();
    let mut game = Game::new(world, 42, test_creatures::content());
    let human = game
        .spawn_actor(at(1, 3, 1), NonZeroU64::new(100).unwrap())
        .unwrap();
    let actor = game
        .spawn_actor(at(1, 1, 2), NonZeroU64::new(100).unwrap())
        .unwrap();
    for (id, faction) in [(human, "hero"), (actor, "foe")] {
        test_creatures::configure(&mut game, id, faction, test_creatures::species());
    }
    game.configure_run(
        human,
        BTreeSet::from([human]),
        BTreeMap::from([("foe".into(), BTreeSet::from(["hero".into()]))]),
    )
    .unwrap();
    let mut potion =
        crate::ItemSpec::ordinary("item.test.healing".parse().unwrap(), "healing".into());
    potion.class = crate::ItemClass::Potion;
    potion.consumable = Some(crate::ConsumableSpec {
        descriptors: vec![],
        discovery: crate::ItemDiscovery::SelfEffect,
        effects: vec![heal(10)],
    });
    game.place_item_stack(20, at(1, 4, 2), None, 1, potion.clone())
        .unwrap();
    game.place_item_stack(21, at(1, 1, 0), None, 1, potion.clone())
        .unwrap();
    game.refresh_navigation();
    let view = game.observe(actor).unwrap();
    let mut search = game.route_search(actor).unwrap();
    let mut route = |at| search.route(at).ok();
    // The eastward first step is adjacent to the hostile; the northward
    // alternative remains safe and must still be considered.
    assert_eq!(
        game.choose_nearby_loot(actor, &view, &AiProfile::general(), &[], &mut route),
        Some(Action::Move(Direction::North))
    );
    game.teleport(human, at(1, 8, 2)).unwrap();
    game.place_item_stack(22, at(1, 5, 2), None, 1, potion)
        .unwrap();
    game.refresh_navigation();
    let mut view = game.observe(actor).unwrap();
    view.ground_items
        .retain(|item| item.id == crate::ItemId(22));
    let mut search = game.route_search(actor).unwrap();
    assert_eq!(
        game.choose_nearby_loot(actor, &view, &AiProfile::general(), &[], &mut |at| search
            .route(at)
            .ok()),
        Some(Action::Move(Direction::East))
    );
}

#[test]
fn nearby_loot_uses_own_knowledge_and_takes_only_one_unit() {
    let mut game = fixture();
    let actor = ActorId(2);
    game.teleport(ActorId(1), at(2, 4, 2)).unwrap();
    game.set_region_light(RegionId(2), false).unwrap();
    let mut potion =
        crate::ItemSpec::ordinary("item.test.healing".parse().unwrap(), "healing".into());
    potion.class = crate::ItemClass::Potion;
    potion.stackable = true;
    potion.concealed = true;
    potion.appearance = "red potion".into();
    potion.consumable = Some(crate::ConsumableSpec {
        descriptors: vec![],
        discovery: crate::ItemDiscovery::SelfEffect,
        effects: vec![heal(10)],
    });
    game.place_item_stack(20, at(1, 2, 1), None, 3, potion)
        .unwrap();
    game.identify_item(ActorId(1), crate::ItemId(20)).unwrap();
    assert!(!matches!(
        game.choose_ai(actor).unwrap().action,
        Action::Take { .. }
    ));
    game.identify_item(actor, crate::ItemId(20)).unwrap();
    assert_eq!(
        game.choose_ai(actor).unwrap().action,
        Action::Take {
            item: crate::ItemId(20),
            quantity: Some(1)
        }
    );
}

#[test]
fn nearby_loot_routes_share_one_search_and_do_not_step_beside_hostiles() {
    let mut game = fixture();
    let actor = ActorId(2);
    game.teleport(ActorId(1), at(1, 5, 1)).unwrap();
    for (id, x) in [(20, 3), (21, 4)] {
        let mut potion = crate::ItemSpec::ordinary(
            format!("item.test.healing-{id}").parse().unwrap(),
            format!("healing {id}"),
        );
        potion.class = crate::ItemClass::Potion;
        potion.consumable = Some(crate::ConsumableSpec {
            descriptors: vec![],
            discovery: crate::ItemDiscovery::SelfEffect,
            effects: vec![heal(10)],
        });
        game.place_item_stack(id, at(1, x, 1), None, 1, potion)
            .unwrap();
    }
    game.refresh_navigation();
    let before = crate::diagnostics::work_counts().route_searches;
    assert_eq!(
        game.choose_ai(actor).unwrap().action,
        Action::Move(Direction::East)
    );
    assert_eq!(crate::diagnostics::work_counts().route_searches - before, 1);
    game.teleport(actor, at(1, 3, 1)).unwrap();
    assert_eq!(
        game.choose_ai(actor).unwrap().action,
        Action::Move(Direction::East),
        "engagement takes priority over even an underfoot healing reserve"
    );
    let mut unengaged = game.clone();
    unengaged.teleport(ActorId(1), at(2, 4, 2)).unwrap();
    unengaged.set_region_light(RegionId(2), false).unwrap();
    assert_eq!(
        unengaged.choose_ai(actor).unwrap().action,
        Action::Take {
            item: crate::ItemId(20),
            quantity: Some(1)
        }
    );
    game.teleport(ActorId(1), at(1, 4, 1)).unwrap();
    assert_eq!(game.choose_ai(actor).unwrap().action, basic(ActorId(1)));
}

#[test]
fn equipment_uses_duplicate_anatomy_sockets_and_only_the_ais_known_stats() {
    let mut game = fixture_with_anatomy(crate::AnatomySpec {
        capacities: Default::default(),
        primary_attack_socket: crate::equipment_capacity::PrimaryAttackSocket::None,
        slots: vec![crate::EquipmentSlot::new("ring").unwrap(); 2],
    });
    game.teleport(ActorId(1), at(2, 4, 2)).unwrap();
    game.set_region_light(RegionId(2), false).unwrap();
    ring(&mut game, 10, 1, false);
    ring(&mut game, 11, 2, false);
    ring(&mut game, 12, 3, true);
    game.equip_authored(ActorId(2), crate::ItemId(10), crate::EquipmentSlotId(0))
        .unwrap();
    let expected = Action::Equip {
        item: crate::ItemId(11),
        slot: crate::EquipmentSlotId(1),
    };
    assert_eq!(game.choose_ai(ActorId(2)).unwrap().action, expected);
    game.identify_item(ActorId(1), crate::ItemId(12)).unwrap();
    assert_eq!(game.choose_ai(ActorId(2)).unwrap().action, expected);
    game.identify_item(ActorId(2), crate::ItemId(12)).unwrap();
    assert_eq!(
        game.choose_ai(ActorId(2)).unwrap().action,
        Action::Equip {
            item: crate::ItemId(12),
            slot: crate::EquipmentSlotId(1)
        }
    );
}

#[test]
fn gear_decisions_do_not_read_hidden_curses_and_stop_after_observed_failure() {
    let mut game = fixture_with_anatomy(crate::AnatomySpec {
        capacities: Default::default(),
        primary_attack_socket: crate::equipment_capacity::PrimaryAttackSocket::None,
        slots: vec![crate::EquipmentSlot::new("ring").unwrap()],
    });
    let actor = ActorId(2);
    game.teleport(ActorId(1), at(2, 4, 2)).unwrap();
    game.set_region_light(RegionId(2), false).unwrap();
    ring(&mut game, 10, 1, false);
    ring(&mut game, 11, 2, false);
    game.equip_authored(actor, crate::ItemId(10), crate::EquipmentSlotId(0))
        .unwrap();
    let action = game.choose_ai(actor).unwrap().action;
    assert_eq!(
        action,
        Action::Unequip {
            item: crate::ItemId(10)
        }
    );
    game.items
        .edit(crate::ItemId(10), |item| item.condition.cursed = true)
        .unwrap();
    assert_eq!(
        game.choose_ai(actor).unwrap().action,
        action,
        "Hidden curse changes cannot alter the decision"
    );
    assert!(game
        .known_equipment_view(actor, crate::ItemId(10))
        .is_none());
    assert_eq!(
        game.actors[&actor].known_equipment.curse(crate::ItemId(10)),
        None
    );
    game.act(ActorId(1), Action::Wait).unwrap();
    game.act(actor, action).unwrap();
    while game.preparation(actor).is_some() {
        let next = game.next_actor().unwrap();
        game.act(next, Action::Wait).unwrap();
    }
    assert_eq!(
        game.actors[&actor].known_equipment.curse(crate::ItemId(10)),
        Some(true)
    );
    assert_eq!(
        game.equipment(actor)
            .unwrap()
            .get(&crate::EquipmentSlotId(0)),
        Some(&crate::ItemId(10))
    );
    assert!(game
        .known_equipment_view(actor, crate::ItemId(10))
        .is_none());
    assert!(!matches!(
        game.choose_ai(actor).unwrap().action,
        Action::Unequip { .. }
    ));
}

#[test]
fn ordinary_gear_estimates_ignore_hidden_bonuses_without_disclosing_numbers() {
    let slot = crate::EquipmentSlot::new("body_armor").unwrap();
    let mut game = fixture_with_anatomy(crate::AnatomySpec {
        capacities: Default::default(),
        primary_attack_socket: crate::equipment_capacity::PrimaryAttackSocket::None,
        slots: vec![slot.clone()],
    });
    let actor = ActorId(2);
    game.teleport(ActorId(1), at(2, 4, 2)).unwrap();
    game.set_region_light(RegionId(2), false).unwrap();
    for (id, reduction) in [(10, 1), (11, 2)] {
        let mut spec = crate::ItemSpec::ordinary(
            format!("item.test.armor-{id}").parse().unwrap(),
            format!("armor-{id}"),
        );
        spec.class = crate::ItemClass::Armor;
        spec.equipment = Some(crate::EquipmentSpec {
            periodic_relocation: None,
            potency_modifiers: Vec::new(),
            weapon_profile: None,
            slot: slot.clone(),
            armor: Some(crate::ArmorSpec {
                physical_reduction: reduction,
                material: crate::ArmorMaterial::Metal,
                rustproof: false,
            }),
            attack: None,
            modifiers: vec![],
        });
        game.place_item_stack(id, at(1, 2, 1), Some(actor), 1, spec)
            .unwrap();
    }
    game.equip_authored(actor, crate::ItemId(10), crate::EquipmentSlotId(0))
        .unwrap();
    // Authored placement supplies no inspection witness for either instance.
    assert!(game
        .known_equipment_view(actor, crate::ItemId(10))
        .is_none());
    assert!(game
        .known_equipment_view(actor, crate::ItemId(11))
        .is_none());
    let expected = Action::Unequip {
        item: crate::ItemId(10),
    };
    assert_eq!(game.choose_ai(actor).unwrap().action, expected);
    let mut hidden = game.clone();
    hidden
        .items
        .edit(crate::ItemId(10), |item| {
            item.condition.cursed = true;
            item.condition.armor_enchantment = 10;
        })
        .unwrap();
    hidden
        .items
        .edit(crate::ItemId(11), |item| {
            item.condition.armor_penalty = 2;
        })
        .unwrap();
    assert_eq!(hidden.choose_ai(actor).unwrap().action, expected);
    assert!(hidden
        .known_equipment_view(actor, crate::ItemId(10))
        .is_none());
    assert!(hidden
        .known_equipment_view(actor, crate::ItemId(11))
        .is_none());
    // Once observed, actual instance facts replace the ordinary estimate.
    hidden.identify_item(actor, crate::ItemId(11)).unwrap();
    assert!(!matches!(
        hidden.choose_ai(actor).unwrap().action,
        Action::Unequip { .. }
    ));
}

#[test]
fn gear_removal_chooses_an_upgrade_and_does_not_reequip_the_dominated_old_item() {
    let mut game = fixture_with_anatomy(crate::AnatomySpec {
        capacities: Default::default(),
        primary_attack_socket: crate::equipment_capacity::PrimaryAttackSocket::None,
        slots: vec![crate::EquipmentSlot::new("ring").unwrap(); 1],
    });
    game.teleport(ActorId(1), at(2, 4, 2)).unwrap();
    game.set_region_light(RegionId(2), false).unwrap();
    ring(&mut game, 10, 1, false);
    ring(&mut game, 11, 2, false);
    game.equip_authored(ActorId(2), crate::ItemId(10), crate::EquipmentSlotId(0))
        .unwrap();
    assert_eq!(
        game.choose_ai(ActorId(2)).unwrap().action,
        Action::Unequip {
            item: crate::ItemId(10)
        }
    );
    game.act(ActorId(1), Action::Wait).unwrap();
    game.act(
        ActorId(2),
        Action::Unequip {
            item: crate::ItemId(10),
        },
    )
    .unwrap();
    while game.preparation(ActorId(2)).is_some() {
        let next = game.next_actor().unwrap();
        game.act(next, Action::Wait).unwrap();
    }
    assert_eq!(
        game.choose_ai(ActorId(2)).unwrap().action,
        Action::Equip {
            item: crate::ItemId(11),
            slot: crate::EquipmentSlotId(0)
        }
    );
}

#[test]
fn gear_work_does_not_replace_unknown_equipment_or_start_beside_a_visible_hostile() {
    let mut game = fixture_with_anatomy(crate::AnatomySpec {
        capacities: Default::default(),
        primary_attack_socket: crate::equipment_capacity::PrimaryAttackSocket::None,
        slots: vec![crate::EquipmentSlot::new("ring").unwrap(); 1],
    });
    ring(&mut game, 10, 1, true);
    ring(&mut game, 11, 2, false);
    game.equip_authored(ActorId(2), crate::ItemId(10), crate::EquipmentSlotId(0))
        .unwrap();
    assert_eq!(
        game.choose_ai(ActorId(2)).unwrap().action,
        basic(ActorId(1))
    );
    game.teleport(ActorId(1), at(2, 4, 2)).unwrap();
    assert!(!matches!(
        game.choose_ai(ActorId(2)).unwrap().action,
        Action::Unequip { .. } | Action::Equip { .. }
    ));
}

#[test]
fn healing_decisions_require_the_ai_actors_own_identity_knowledge() {
    let mut game = fixture();
    let actor = ActorId(2);
    let mut potion =
        crate::ItemSpec::ordinary("item.test.healing".parse().unwrap(), "healing".into());
    potion.class = crate::ItemClass::Potion;
    potion.concealed = true;
    potion.appearance = "red potion".into();
    potion.consumable = Some(crate::ConsumableSpec {
        descriptors: vec![],
        discovery: crate::ItemDiscovery::SelfEffect,
        effects: vec![heal(10)],
    });
    game.place_item_stack(20, at(1, 2, 1), Some(actor), 1, potion)
        .unwrap();
    injury(&mut game, actor, 15);
    assert_eq!(game.choose_ai(actor).unwrap().action, basic(ActorId(1)));
    game.identify_item(ActorId(1), crate::ItemId(20)).unwrap();
    assert_eq!(game.choose_ai(actor).unwrap().action, basic(ActorId(1)));
    game.identify_item(actor, crate::ItemId(20)).unwrap();
    assert_eq!(
        game.choose_ai(actor).unwrap().action,
        Action::Drink {
            item: crate::ItemId(20)
        }
    );
}

#[test]
fn healing_priority_does_not_drink_known_mixed_harmful_effects_or_waste_full_health() {
    let mut game = fixture();
    let actor = ActorId(2);
    let mut potion = crate::ItemSpec::ordinary("item.test.mixed".parse().unwrap(), "mixed".into());
    potion.class = crate::ItemClass::Potion;
    potion.consumable = Some(crate::ConsumableSpec {
        descriptors: vec![],
        discovery: crate::ItemDiscovery::SelfEffect,
        effects: vec![
            heal(10),
            crate::rules::Effect::Damage {
                damage: crate::rules::DamageExpr::Components {
                    components: vec![crate::rules::DamageComponentExpr {
                        category: DamageType::Vital,
                        descriptor: None,
                        amount: crate::rules::AmountExpr::Fixed { amount: 1 },
                    }],
                },
            },
        ],
    });
    game.place_item_stack(20, at(1, 2, 1), Some(actor), 1, potion)
        .unwrap();
    injury(&mut game, actor, 15);
    assert_eq!(game.choose_ai(actor).unwrap().action, basic(ActorId(1)));
    let mut healing =
        crate::ItemSpec::ordinary("item.test.healing".parse().unwrap(), "healing".into());
    healing.class = crate::ItemClass::Potion;
    healing.consumable = Some(crate::ConsumableSpec {
        descriptors: vec![],
        discovery: crate::ItemDiscovery::SelfEffect,
        effects: vec![heal(20)],
    });
    game.place_item_stack(21, at(1, 2, 1), Some(actor), 1, healing)
        .unwrap();
    game.apply_effects(actor, &[heal(30)]).unwrap();
    assert_eq!(game.choose_ai(actor).unwrap().action, basic(ActorId(1)));
    injury(&mut game, actor, 25);
    assert_eq!(
        game.choose_ai(actor).unwrap().action,
        Action::Drink {
            item: crate::ItemId(21)
        }
    );
}
fn data_ability_game(reverse: bool) -> Game {
    use crate::{
        content_ids::{AbilityId, FlagId},
        rules::{Modifier, ModifierOp, RangePreference, Ruleset, Scope},
    };
    let rules = paid_probe_rules();
    let mut document = rules.document().clone();
    let base = rules
        .ability(&AbilityId::new("fixture_mana_ray").unwrap())
        .unwrap();
    let mut foe = test_creatures::species();
    foe.modifiers.push(Modifier {
        cancellable: false,
        scope: Scope::Always,
        op: ModifierOp::Flag {
            flag: FlagId::new("magical").unwrap(),
        },
    });
    for name in ["custom_arc", "custom_zap"] {
        let mut definition = base.clone();
        definition.id = AbilityId::new(name).unwrap();
        definition.name = name.into();
        definition.ai.priority = 100;
        definition.ai.range_preference = RangePreference::Any;
        definition.cost = None;
        foe.modifiers.push(Modifier {
            cancellable: false,
            scope: Scope::Always,
            op: ModifierOp::GrantAbility {
                ability: definition.id.clone(),
            },
        });
        document.abilities.push(definition);
    }
    for index in 0..8 {
        let mut definition = base.clone();
        definition.id = AbilityId::new(format!("unfunded_{index}")).unwrap();
        definition.name = format!("Unfunded {index}");
        definition.ai.priority = 200;
        definition.ai.range_preference = RangePreference::Any;
        definition.cost = Some(crate::rules::Cost {
            resource: crate::resources::Resource::Mana,
            start: 100,
            resolution: 100,
        });
        foe.modifiers.push(Modifier {
            cancellable: false,
            scope: Scope::Always,
            op: ModifierOp::GrantAbility {
                ability: definition.id.clone(),
            },
        });
        document.abilities.push(definition);
    }
    if reverse {
        document.abilities.reverse();
    }
    configured_game(
        test_creatures::species(),
        foe,
        Ruleset::validate(document).unwrap(),
    )
}

#[test]
fn authored_abilities_use_priority_and_stable_ids_with_one_sight_scene_per_decision() {
    for reverse in [false, true] {
        let mut game = data_ability_game(reverse);
        game.teleport(ActorId(1), at(1, 4, 1)).unwrap();
        // Measure one cold construction, independently of fixture reads.
        game.lifecycle.scenes = Default::default();
        let before = crate::diagnostics::work_counts();
        let AiDecision { action, .. } = game.choose_ai(ActorId(2)).unwrap();
        let after = crate::diagnostics::work_counts();
        assert_eq!(
            action,
            Action::UseAbility {
                id: crate::content_ids::AbilityId::new("custom_arc").unwrap(),
                invocation: crate::abilities::AbilityInvocation::actor_target(ActorId(1)),
                supplies: Default::default(),
            }
        );
        assert_eq!(after.scenes - before.scenes, 1);
        assert_eq!(after.route_searches - before.route_searches, 1);
    }
}

#[test]
fn admitted_ai_executes_one_captured_plan_from_one_decision_scene() {
    let mut game = data_ability_game(false);
    game.teleport(ActorId(1), at(1, 4, 1)).unwrap();
    game.act(ActorId(1), Action::Wait).unwrap();
    assert_eq!(game.next_actor(), Some(ActorId(2)));
    // Keep this a cold-build witness; admission itself must not build.
    game.lifecycle.scenes = Default::default();
    let before_admission = crate::diagnostics::work_counts();
    game.admit_ai_intention(ActorId(2)).unwrap();
    let before = crate::diagnostics::work_counts();
    assert_eq!(before.scenes, before_admission.scenes);
    assert_eq!(before.ability_plans, before_admission.ability_plans);
    let execution = game.execute_next_intention().unwrap();
    let after = crate::diagnostics::work_counts();
    assert_eq!(
        execution.action,
        Some(Action::UseAbility {
            id: crate::content_ids::AbilityId::new("custom_arc").unwrap(),
            invocation: crate::abilities::AbilityInvocation::actor_target(ActorId(1)),
            supplies: Default::default(),
        })
    );
    execution.outcome.unwrap();
    assert_eq!(
        after.ability_plans - before.ability_plans,
        1,
        "unfunded candidates are filtered before planning and selected operands are reused"
    );
    assert_eq!(after.route_searches - before.route_searches, 1);
    assert_eq!(after.decision_scenes - before.decision_scenes, 1);
    assert_eq!(
        after.ai_precommit_ordinary_scenes - before.ai_precommit_ordinary_scenes,
        0,
        "no ordinary scene may be built before consuming the chosen plan"
    );
    // Stationary sight/reach and source-origin queries reuse the caster's
    // decision scene. Only the teleported target's participant disclosure
    // builds an ordinary scene, after commitment. Count actual builds,
    // including cold snapshot misses, rather than cached scene requests.
    assert_eq!(after.ordinary_scenes - before.ordinary_scenes, 1);
    assert_eq!(after.scenes - before.scenes, 2);
    assert_eq!(
        after.scenes - before.scenes,
        (after.decision_scenes - before.decision_scenes)
            + (after.ordinary_scenes - before.ordinary_scenes)
    );
    assert!(
        game.disclosed_event_history(ActorId(1))
            .iter()
            .any(|event| {
                matches!(event, crate::combat::DisclosedCombatEvent::Ability {
                caster: Some(ActorId(2)),
                caster_reference: Some(crate::perception::ActorReferenceKind::Creature),
                target: Some(ActorId(1)),
                id,
                ..
            } if id.as_str() == "custom_arc")
            }),
        "the shared scenes retain the target's identified participant feedback"
    );
}

#[test]
fn tactile_awareness_does_not_count_as_illuminated_sight_for_ranged_abilities() {
    let mut game = data_ability_game(false);
    game.set_region_light(RegionId(1), false).unwrap();
    // Keep the cold-build assertion independent of setup observations.
    game.lifecycle.scenes = Default::default();
    let before = crate::diagnostics::work_counts();
    let snapshot = game.decision_snapshot(ActorId(2)).unwrap();
    assert!(snapshot
        .view
        .visible_actors
        .iter()
        .any(|actor| actor.id == ActorId(1)));
    assert!(!snapshot
        .sight
        .iter()
        .any(|cell| cell.location == at(1, 1, 1)));
    assert_eq!(crate::diagnostics::work_counts().scenes - before.scenes, 1);
    assert_eq!(
        game.choose_ai(ActorId(2)).unwrap().action,
        basic(ActorId(1))
    );
}
/// Exercise the shipped paid W05 definition rather than a colliding clone.
fn public_plan_ai_rules() -> std::sync::Arc<crate::rules::Ruleset> {
    test_creatures::ruleset()
}
#[test]
fn public_plan_ai_uses_authored_policy_then_spends_readied_on_a_real_attack() {
    use crate::{
        content_ids::AbilityId,
        resources::Resource,
        rules::{Modifier, ModifierOp, Scope, Stat},
    };
    let rules = public_plan_ai_rules();
    assert_eq!(
        rules
            .document()
            .resources
            .iter()
            .find(|definition| definition.resource == Resource::Focus)
            .unwrap()
            .recovery_period,
        300
    );
    let observation = AbilityId::new("read_the_rhythm").unwrap();
    let definition = rules
        .ability(&observation)
        .expect("ordinary authored W05 definition");
    assert_eq!(definition.cost.as_ref().unwrap().resource, Resource::Focus);
    assert_eq!(
        definition.cost.as_ref().unwrap().start + definition.cost.as_ref().unwrap().resolution,
        2
    );
    assert!(definition.ai.hostile);
    assert_eq!(definition.ai.minimum_resource_percent, 100);
    let mut foe = test_creatures::species();
    foe.modifiers.extend([
        Modifier {
            cancellable: false,
            scope: Scope::Always,
            op: ModifierOp::GrantAbility {
                ability: observation.clone(),
            },
        },
        Modifier {
            cancellable: false,
            scope: Scope::Always,
            op: ModifierOp::Set {
                stat: Stat::ResourceMax {
                    resource: Resource::Focus,
                },
                value: 2,
            },
        },
    ]);
    let mut game = configured_game(test_creatures::species(), foe, rules);
    game.act(ActorId(1), Action::Wait).unwrap();
    let decision = game.choose_ai(ActorId(2)).unwrap();
    assert_eq!(
        decision.action,
        Action::UseAbility {
            id: observation,
            invocation: crate::abilities::AbilityInvocation::actor_target(ActorId(1)),
            supplies: Default::default()
        }
    );
    let plan = decision.plan.unwrap();
    assert_eq!(
        (plan.preparation(), plan.as_ability().unwrap().recovery),
        (100, 100)
    );
    game.admit_ai_intention(ActorId(2)).unwrap();
    game.execute_next_intention().unwrap().outcome.unwrap();
    assert_eq!(game.tick(), 100);
    let remembered = game.public_plan_observation(ActorId(2)).unwrap().clone();
    assert_eq!(remembered.observed_at, 100);
    assert_eq!(
        game.creature(ActorId(2))
            .unwrap()
            .costs()
            .resources()
            .balance(Resource::Focus),
        0
    );
    assert_eq!(
        game.creature(ActorId(2))
            .unwrap()
            .conditions()
            .instances()
            .filter(|entry| entry.condition.as_str() == "readied")
            .count(),
        1
    );
    for _ in 0..3 {
        if game.next_actor() == Some(ActorId(2)) {
            break;
        }
        game.act(ActorId(1), Action::Wait).unwrap();
    }
    assert_eq!(game.tick(), 200);
    assert_eq!(game.next_actor(), Some(ActorId(2)));
    let attack = game.choose_ai(ActorId(2)).unwrap();
    let plan = attack.plan.as_ref().unwrap().as_ability().unwrap();
    assert!(game
        .content()
        .rules()
        .ability(&plan.ability)
        .unwrap()
        .tags
        .iter()
        .any(|tag| tag.as_str() == "attack"));
    assert_eq!(plan.tokens.len(), 1);
    assert_eq!(
        plan.tokens[0]
            .reservation
            .qualification
            .as_ref()
            .unwrap()
            .opponent,
        ActorId(1)
    );
    game.admit_ai_intention(ActorId(2)).unwrap();
    game.execute_next_intention().unwrap().outcome.unwrap();
    for _ in 0..4 {
        if game.preparation(ActorId(2)).is_none() {
            break;
        }
        game.act(ActorId(1), Action::Wait).unwrap();
    }
    assert!(game.preparation(ActorId(2)).is_none());
    assert!(game
        .creature(ActorId(2))
        .unwrap()
        .conditions()
        .instances()
        .all(|entry| entry.condition.as_str() != "readied"));
    assert_eq!(game.tick(), 300);
    assert_eq!(
        game.creature(ActorId(2))
            .unwrap()
            .costs()
            .resources()
            .balance(Resource::Focus),
        1
    );
    assert_eq!(game.public_plan_observation(ActorId(2)), Some(&remembered));
}
#[test]
fn public_plan_ai_choice_is_equal_for_publicly_equal_idle_and_private_work() {
    use crate::{
        content_ids::AbilityId,
        rules::{Modifier, ModifierOp, Scope},
    };
    let mut foe = test_creatures::species();
    foe.modifiers.push(Modifier {
        cancellable: false,
        scope: Scope::Always,
        op: ModifierOp::GrantAbility {
            ability: AbilityId::new("read_the_rhythm").unwrap(),
        },
    });
    let mut idle = configured_game(test_creatures::species(), foe, public_plan_ai_rules());
    let hero = ActorId(1);
    let observer = ActorId(2);
    let slot = idle
        .actors
        .get(&hero)
        .unwrap()
        .anatomy
        .slots
        .iter()
        .position(|slot| slot.as_str() == "body_armor")
        .unwrap();
    let mut item = crate::ItemSpec::ordinary(
        "item.test.private-armor".parse().unwrap(),
        "private armor".into(),
    );
    item.class = crate::ItemClass::Armor;
    item.equipment = Some(crate::EquipmentSpec {
        periodic_relocation: None,
        potency_modifiers: Vec::new(),
        weapon_profile: None,
        armor: None,
        slot: crate::EquipmentSlot::new("body_armor").unwrap(),
        attack: None,
        modifiers: vec![],
    });
    let item = idle
        .place_item_stack(20, at(1, 1, 1), Some(hero), 1, item)
        .unwrap();
    let mut private = idle.clone();
    idle.act(hero, Action::Wait).unwrap();
    private
        .admit_intention(
            hero,
            Action::Equip {
                item,
                slot: crate::EquipmentSlotId(slot as u16),
            },
            crate::IntentionOrigin::Human,
        )
        .unwrap();
    private.execute_next_intention().unwrap().outcome.unwrap();
    assert_eq!(idle.tick(), private.tick());
    assert_eq!(
        idle.observe(observer).unwrap(),
        private.observe(observer).unwrap(),
        "same authorized facts precede the decision"
    );
    let idle_choice = idle.choose_ai(observer).unwrap();
    let private_choice = private.choose_ai(observer).unwrap();
    assert_eq!(idle_choice.action, private_choice.action);
    assert_eq!(idle_choice.plan, private_choice.plan);
    assert!(
        matches!(idle_choice.action, Action::UseAbility { ref id, invocation: crate::abilities::AbilityInvocation { aim: crate::abilities::AbilityAim::ActorTarget { target }, .. }, .. } if id.as_str() == "read_the_rhythm" && target == hero)
    );
}
#[test]
fn declined_erratic_draw_reaches_engagement_and_commits_once_after_restore() {
    use crate::locomotion::{LocomotionValue, MovementChance};
    use crate::rules::{Modifier, ModifierOp, Scope};
    let mut foe = test_creatures::species();
    foe.modifiers.push(Modifier {
        cancellable: false,
        scope: Scope::Always,
        op: ModifierOp::Locomotion {
            value: LocomotionValue::Erratic {
                chance: Some(MovementChance {
                    numerator: 1,
                    denominator: 2,
                }),
            },
        },
    });
    let mut game = configured_game(test_creatures::species(), foe, paid_probe_rules());
    let actor = ActorId(2);
    game.configure_ai(
        actor,
        AiProfile {
            idle: crate::behavior::IdleBehavior::Wait,
            flee_percent: 0,
            ..Default::default()
        },
    )
    .unwrap();
    game.configure_run(ActorId(1), BTreeSet::from([ActorId(1)]), BTreeMap::new())
        .unwrap();
    game.act(ActorId(1), Action::Wait).unwrap();
    let clock = game.actor_clock(actor);
    {
        let behavior = &mut game.actors.get_mut(&actor).unwrap().behavior;
        behavior.rng = 0;
        behavior.engagement = Some(crate::behavior::Engagement {
            reference: crate::perception::ActorReferenceKind::Creature,
            target: ActorId(1),
            location: at(1, 1, 1),
            last_seen: clock,
        });
    }
    let mut expected_rng = 0;
    assert_eq!(
        crate::dice::random_word(&mut expected_rng) % 2,
        1,
        "pinned seed declines erratic movement"
    );
    let original = game.clone();
    let chosen = game.choose_ai(actor).unwrap();
    assert!(matches!(chosen.action, Action::UseAbility { .. }));
    assert!(chosen.plan.is_some());
    assert_eq!(
        chosen.behavior.rng, expected_rng,
        "declining consumes exactly the chance draw, not a direction draw"
    );
    assert_eq!(game.choose_ai(actor).unwrap().behavior.rng, expected_rng);
    assert_eq!(game, original, "preview keeps private RNG unchanged");
    game.admit_ai_intention(actor).unwrap();
    assert_eq!(game.actors[&actor].behavior.rng, 0);
    let mut shared = crate::checkpoint::SharedState::default();
    let checkpoint =
        serde_json::from_value(serde_json::to_value(game.checkpoint(&mut shared)).unwrap())
            .unwrap();
    let shared = serde_json::from_value(serde_json::to_value(shared).unwrap()).unwrap();
    let mut restored =
        Game::restore_checkpoint(checkpoint, &shared, game.content().clone()).unwrap();
    for candidate in [&mut game, &mut restored] {
        let execution = candidate.execute_next_intention().unwrap();
        assert_eq!(execution.action, Some(chosen.action.clone()));
        execution.outcome.unwrap();
        assert_eq!(candidate.actors[&actor].behavior.rng, expected_rng);
    }
    assert_eq!(game, restored);
}

#[test]
fn erratic_move_can_replace_a_fresh_adjacent_attack_but_preview_does_not_draw() {
    use crate::locomotion::{LocomotionValue, MovementChance};
    use crate::rules::{Modifier, ModifierOp, Scope};
    let mut foe = test_creatures::species();
    let ordinary = foe.clone();
    foe.modifiers.push(Modifier {
        cancellable: false,
        scope: Scope::Always,
        op: ModifierOp::Locomotion {
            value: LocomotionValue::Erratic {
                chance: Some(MovementChance {
                    numerator: 1,
                    denominator: 1,
                }),
            },
        },
    });
    let mut game = configured_game(test_creatures::species(), foe, paid_probe_rules());
    game.configure_ai(
        ActorId(2),
        AiProfile {
            idle: crate::behavior::IdleBehavior::Wait,
            flee_percent: 0,
            ..Default::default()
        },
    )
    .unwrap();
    let engagement = Some(crate::behavior::Engagement {
        reference: crate::perception::ActorReferenceKind::Creature,
        target: ActorId(1),
        location: at(1, 1, 1),
        last_seen: game.actor_clock(ActorId(2)),
    });
    game.actors
        .get_mut(&ActorId(2))
        .unwrap()
        .behavior
        .engagement = engagement;
    let before = game.clone();
    let chosen = game.choose_ai(ActorId(2)).unwrap();
    assert!(matches!(chosen.action, Action::Move(_)));
    assert!(chosen.plan.is_none());
    assert_ne!(chosen.behavior.rng, before.actors[&ActorId(2)].behavior.rng);
    assert_eq!(game.choose_ai(ActorId(2)).unwrap().action, chosen.action);
    assert_eq!(game, before);
    // The same fixture without the modifier chooses its ordinary adjacent attack.
    let mut ordinary = configured_game(test_creatures::species(), ordinary, paid_probe_rules());
    ordinary
        .configure_ai(
            ActorId(2),
            AiProfile {
                idle: crate::behavior::IdleBehavior::Wait,
                flee_percent: 0,
                ..Default::default()
            },
        )
        .unwrap();
    ordinary
        .actors
        .get_mut(&ActorId(2))
        .unwrap()
        .behavior
        .engagement = engagement;
    assert!(matches!(
        ordinary.choose_ai(ActorId(2)).unwrap().action,
        Action::UseAbility { .. }
    ));
}
