#[test]
fn projectile_archetype_reaches_stackable_item_without_equipment() {
    let mut manifest = creature_manifest();
    let projectile = serde_json::from_value(serde_json::json!({"family":"fixture.arrow",
            "recovery":"consumed_on_hit", "damage":{"components":[{"category":"keen",
            "descriptor":null,"amount":{"type":"fixed","value":2}}],
            "primary":{"category":"keen","descriptor":null,"sides":null}}}))
    .unwrap();
    manifest.archetypes.insert(
        "arrow".into(),
        super::Archetype {
            class: super::super::ItemClass::Weapon,
            name: Some("Fixture arrow".into()),
            identity: Some("item.test.fixture-arrow".parse().unwrap()),
            projectile: Some(projectile),
            stackable: true,
            equipment_generation: Some(super::super::EquipmentGeneration::Authored {
                hit_enchantment: 1,
                damage_enchantment: 2,
            }),
            ..Default::default()
        },
    );
    let definitions = super::PreparedDefinitions::new(&manifest, 1).unwrap();
    let item = serde_json::from_value(serde_json::json!({"id":100,"at":[1,1,0],
            "archetype":"arrow","quantity":12}))
    .unwrap();
    let compiled = definitions.item(&item, 42, &Default::default()).unwrap();
    assert!(compiled.equipment.is_none());
    assert!(compiled.stackable);
    assert_eq!(
        compiled.projectile.as_ref().unwrap().family.as_str(),
        "fixture.arrow"
    );
    assert!(compiled.equipment_generation.unwrap().valid_for(
        compiled.class,
        compiled.equipment.as_ref(),
        compiled.projectile.as_ref()
    ));
}
#[test]
fn authored_supply_identity_compiles_and_rejects_unknown_or_equipment_supplies() {
    let mut manifest = creature_manifest();
    manifest.archetypes.insert(
        "bandage".into(),
        super::Archetype {
            supply: Some("bandage".into()),
            name: Some("Bandage".into()),
            identity: Some("item.test.bandage".parse().unwrap()),
            stackable: true,
            ..Default::default()
        },
    );
    let definitions = super::PreparedDefinitions::new(&manifest, 1).unwrap();
    let item =
        serde_json::from_value(serde_json::json!({"id":100, "at":[1,1,0], "archetype":"bandage"}))
            .unwrap();
    let compiled = definitions.item(&item, 42, &Default::default()).unwrap();
    assert_eq!(compiled.supply.unwrap().as_str(), "bandage");
    manifest.archetypes.get_mut("bandage").unwrap().supply = Some("unknown".into());
    assert!(super::PreparedDefinitions::new(&manifest, 1).is_err());
    let author = manifest.archetypes.get_mut("bandage").unwrap();
    author.supply = Some("bandage".into());
    author.equipment =
        Some(serde_json::from_value(serde_json::json!({"slot":"shield", "modifiers":[]})).unwrap());
    assert!(super::PreparedDefinitions::new(&manifest, 1).is_err());
}
fn arena_package() -> super::super::Package {
    let mut value = serde_json::to_value(creature_manifest()).unwrap();
    value["arena"] = serde_json::json!({
        "participants": [1, 2], "control": "all_ai", "ticks": 7, "actions": 100
    });
    let mut manifest: super::Manifest = serde_json::from_value(value).unwrap();
    manifest
        .ai_profiles
        .insert("practice".into(), Default::default());
    manifest.characters[0].unselected = "ai".into();
    manifest.characters[0].ai = Some("practice".into());
    let build = manifest.characters[0].creature.clone().unwrap();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/two-room");
    let template = super::super::read_package(&root).unwrap();
    let mut regions = template.region_defs().unwrap();
    regions[0].actors.push(
        serde_json::from_value(serde_json::json!({
            "id": 2, "at": [2,1,0], "creature": build,
            "controller": "ai", "ai": "practice"
        }))
        .unwrap(),
    );
    super::super::Package::from_parts_scripted(
        manifest,
        regions,
        &template.rules_source,
        template.scripts.clone(),
    )
    .unwrap()
}

#[test]
fn authored_arena_uses_selected_ai_and_configured_simulation_limits() {
    let package = arena_package();
    let mut game = package.build(42, true).unwrap();
    assert_eq!(game.arena_run().unwrap().limits.ticks, 7);
    assert_eq!(game.next_ai_action().unwrap().0, tor_simulation::ActorId(1));
    for _ in 0..10 {
        if game.run_outcome().terminal {
            break;
        }
        let (actor, _) = game.next_ai_action().unwrap();
        game.admit_ai_intention(actor).unwrap();
        game.execute_next_intention().unwrap().outcome.unwrap();
    }
    assert_eq!(game.tick(), 7);
    assert_eq!(
        game.arena_run().unwrap().stop().cloned(),
        Some(tor_simulation::arena::ArenaStop::TickLimit)
    );
}

#[test]
fn authored_arena_runs_through_streaming_engine_at_zero_loading_radius() {
    let mut scenario = crate::Scenario::two_room(42);
    scenario.actors.clear();
    scenario.package = Some(std::sync::Arc::new(arena_package()));
    scenario.streaming = Some(crate::Streaming {
        active_radius: 0,
        load_radius: 0,
    });
    let mut engine = crate::Engine::memory(scenario).unwrap();
    for _ in 0..10 {
        let Some((actor, _)) = engine.next_ai_action() else {
            break;
        };
        engine.advance_ai(actor).unwrap();
    }
    assert!(engine.next_ai_action().is_none());
    let observation =
        serde_json::to_value(engine.observation(tor_protocol::ActorId(1)).unwrap()).unwrap();
    assert_eq!(observation["tick"], serde_json::json!("7"));
}

#[test]
fn authored_arena_session_executes_without_a_player_controller() {
    let mut scenario = crate::Scenario::two_room(42);
    scenario.actors.clear();
    scenario.package = Some(std::sync::Arc::new(arena_package()));
    scenario.streaming = Some(crate::Streaming {
        active_radius: 0,
        load_radius: 0,
    });
    let mut service = crate::session::Service::new(crate::Engine::memory(scenario).unwrap());
    let mut committed = 0;
    for _ in 0..10 {
        match service.step() {
            crate::session::Step::Progress => committed += 1,
            crate::session::Step::Blocked => break,
            _ => panic!("unattended arena cannot wait for output"),
        }
    }
    assert!(committed >= 2, "both AI participants must execute");
    assert!(matches!(service.step(), crate::session::Step::Blocked));
}

#[test]
fn arena_pins_keep_remote_participants_active_at_zero_radius() {
    let template = arena_package();
    let mut regions = template.region_defs().unwrap();
    let actor = regions[0].actors.pop().unwrap();
    assert_eq!(actor.id, 2);
    regions[1].actors.push(actor);
    let package = super::super::Package::from_parts_scripted(
        template.manifest.clone(),
        regions,
        &template.rules_source,
        template.scripts.clone(),
    )
    .unwrap();
    let mut scenario = crate::Scenario::two_room(42);
    scenario.actors.clear();
    scenario.package = Some(std::sync::Arc::new(package));
    scenario.streaming = Some(crate::Streaming {
        active_radius: 0,
        load_radius: 0,
    });
    let mut engine = crate::Engine::memory(scenario).unwrap();
    let mut acted = std::collections::BTreeSet::new();
    for _ in 0..10 {
        let Some((actor, _)) = engine.next_ai_action() else {
            break;
        };
        acted.insert(actor);
        engine.advance_ai(actor).unwrap();
    }
    assert_eq!(
        acted,
        std::collections::BTreeSet::from([tor_protocol::ActorId(1), tor_protocol::ActorId(2)])
    );
    assert_eq!(
        serde_json::to_value(engine.observation(tor_protocol::ActorId(2)).unwrap()).unwrap()
            ["tick"],
        serde_json::json!("7")
    );
}

#[test]
fn manual_arena_waits_for_selected_input_then_runs_other_ai_normally() {
    let template = arena_package();
    let mut manifest = template.manifest.clone();
    manifest.arena.as_mut().unwrap().control = super::super::ArenaControl::Manual;
    let package = super::super::Package::from_parts_scripted(
        manifest,
        template.region_defs().unwrap(),
        &template.rules_source,
        template.scripts.clone(),
    )
    .unwrap();
    let mut game = package.build(42, true).unwrap();
    assert!(!game.is_ai(tor_simulation::ActorId(1)));
    assert!(game.next_ai_action().is_none());
    assert_eq!(game.tick(), 0);
    game.act(tor_simulation::ActorId(1), tor_simulation::Action::Wait)
        .unwrap();
    let (actor, _) = game.next_ai_action().unwrap();
    assert_eq!(actor, tor_simulation::ActorId(2));
    game.admit_ai_intention(actor).unwrap();
    game.execute_next_intention().unwrap().outcome.unwrap();
    assert_eq!(game.tick(), 7);
    assert!(game.run_outcome().terminal);
}

#[test]
fn authored_arena_rejects_invalid_participants_limits_and_controllers() {
    let template = arena_package();
    for case in 0..7 {
        let mut manifest = template.manifest.clone();
        let arena = manifest.arena.as_mut().unwrap();
        match case {
            0 => arena.participants.push(1),
            1 => arena.participants = vec![2],
            2 => arena.participants.push(99),
            3 => arena.ticks = 0,
            4 => arena.actions = 10001,
            5 => {
                manifest.characters[0].unselected = "omit".into();
                manifest.characters[0].ai = None;
            }
            6 => manifest.characters[0].creature = None,
            _ => unreachable!(),
        }
        let result = super::super::Package::from_parts_scripted(
            manifest,
            template.region_defs().unwrap(),
            &template.rules_source,
            template.scripts.clone(),
        )
        .and_then(|package| package.build(42, true));
        assert!(result.is_err(), "invalid arena case {case}");
    }
}

fn creature_manifest() -> super::Manifest {
    let mut manifest: super::Manifest =
        toml::from_str(include_str!("../../../scenarios/two-room/scenario.toml")).unwrap();
    manifest.creatures = serde_json::from_value(serde_json::json!({"species": {"human": {
            "kind": "humanoid", "behavior": {"disposition":"mean","idle":"explore","memory_ticks":1000}, "attributes": {"strength": 2, "speed": 1, "intellect": 2, "willpower": 2, "awareness": 1, "presence": 1},
            "melee":{"skill":"heavy_weaponry","bonus":0,"wind_up":60,"recovery":40,"strikes":[{"offset":0,"check":{"actor":"user","skill":{"kind":"selected_weapon"},"against":{"kind":"defense","defense":"physical"}},"on_success":[{"kind":"damage","damage":{"kind":"components","components":[{"category":"impact","descriptor":null,"amount":{"kind":"rolled","dice":{"count":1,"sides":6,"bonus":0}}}]}}],"on_failure":[],"descriptors":[],"riders":[]}]},
            "modifiers": [{"scope":"always", "op":{"kind":"grant_ability", "ability":"spark_lance_fire"}}]
        }}})).unwrap();
    manifest.characters[0].creature = Some(serde_json::from_value(serde_json::json!({
            "species": "human", "name": "test creature", "corpse_identity": "item.corpse.test.test-creature", "faction": "neutral", "binding": "intellect",
            "starting_skills": ["deception", "persuasion"], "restoration": "choose_again", "nutrition": {"kind":"exempt"}, "hit_dice": [{"source": "racial"}, {"source": "warrior"}]
        })).unwrap());
    manifest
}

#[test]
fn direct_item_polymorph_rosters_require_explicit_catalog_eligibility() {
    let mut manifest = creature_manifest();
    manifest.creatures.body_packages.insert(
        "eligible_body".into(),
        serde_json::from_value(serde_json::json!({
            "species": "human",
            "body": {"cells": [[0, 0, 0]], "eye": [0, 0, 0], "mass": 80},
            "hit_dice": [{"source": "racial"}],
            "polymorph_eligible": true
        }))
        .unwrap(),
    );
    for capability in ["consumable", "readable"] {
        let item = serde_json::json!({
            "identity": "item.test.polymorph-item",
            capability: {"effects": [{"kind": "baleful_polymorph", "forms": ["eligible_body"]}]}
        });
        manifest.archetypes.insert(
            "polymorph_item".into(),
            serde_json::from_value(item).unwrap(),
        );
        assert!(
            PreparedDefinitions::new(&manifest, 1).is_ok(),
            "{capability} positive fixture"
        );
        manifest
            .creatures
            .body_packages
            .get_mut("eligible_body")
            .unwrap()
            .polymorph_eligible = false;
        let error = PreparedDefinitions::new(&manifest, 1).unwrap_err();
        assert!(
            error
                .message
                .contains("Unknown or ineligible polymorph body package"),
            "{}",
            error.message
        );
        manifest
            .creatures
            .body_packages
            .get_mut("eligible_body")
            .unwrap()
            .polymorph_eligible = true;
        let mut missing = manifest.clone();
        missing.creatures.body_packages.clear();
        let error = PreparedDefinitions::new(&missing, 1).unwrap_err();
        assert!(
            error
                .message
                .contains("Unknown or ineligible polymorph body package"),
            "{}",
            error.message
        );
    }
}

#[test]
fn creature_package_spawns_selected_and_inherited_actors_with_independent_health() {
    let mut manifest = creature_manifest();
    let build = manifest.characters[0].creature.clone().unwrap();
    manifest.archetypes.insert(
        "creature".into(),
        super::Archetype {
            creature: Some(build),
            ..Default::default()
        },
    );
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/two-room");
    let template = super::super::read_package(&root).unwrap();
    let mut regions = template.region_defs().unwrap();
    regions[0].actors.push(
        serde_json::from_value(
            serde_json::json!({"id": 2, "at": [2, 1, 0], "archetype": "creature"}),
        )
        .unwrap(),
    );
    let package = super::super::Package::from_parts_scripted(
        manifest,
        regions,
        &template.rules_source,
        template.scripts.clone(),
    )
    .unwrap();
    let first = package.build(42, true).unwrap();
    let second = package.build(42, true).unwrap();
    let mut shared = tor_simulation::checkpoint::SharedState::default();
    let snapshot = serde_json::to_value(first.checkpoint(&mut shared)).unwrap();
    assert_eq!(
        snapshot["combat"]["selected"],
        serde_json::json!(1),
        "creature-backed character participates in run lifecycle"
    );
    let selected = first.creature(tor_simulation::ActorId(1)).unwrap();
    let mob = first.creature(tor_simulation::ActorId(2)).unwrap();
    assert_eq!(
        selected,
        second.creature(tor_simulation::ActorId(1)).unwrap()
    );
    assert_eq!(selected.build().choices(), mob.build().choices());
    assert_ne!(selected.build().ledger(), mob.build().ledger());
    assert!(selected
        .derived()
        .abilities
        .contains(&tor_simulation::content_ids::AbilityId::new("spark_lance_fire").unwrap()));
}

#[test]
fn required_nutrition_survives_player_npc_and_population_recipe_construction() {
    use tor_simulation::nutrition::{Nutrition, NutritionPolicy, NutritionRequirement};
    let mut manifest = creature_manifest();
    manifest.characters[0].creature.as_mut().unwrap().nutrition = NutritionRequirement::Required;
    manifest.archetypes.insert(
        "creature".into(),
        super::Archetype {
            creature: manifest.characters[0].creature.clone(),
            ..Default::default()
        },
    );
    let mut document: tor_simulation::rules::RulesetDocument =
        toml::from_str(tor_simulation::rules::DEFAULT_RULESET_TOML).unwrap();
    document.nutrition = NutritionPolicy::Enabled {
        initial: 8,
        maximum: 12,
        hungry_below: 7,
        weak_below: 3,
        starve_at: -3,
        unit_ticks: 100,
        hungry_recovery: tor_simulation::health_recovery::RecoveryNutrition::Slowed {
            multiplier: 2,
        },
        weak_recovery: tor_simulation::health_recovery::RecoveryNutrition::Stopped,
        empty_recovery: tor_simulation::health_recovery::RecoveryNutrition::Stopped,
        regeneration_cost: tor_simulation::nutrition::RegenerationCost {
            denominator: 1,
            health: 0,
            stamina: 0,
            focus: 0,
            mana: 0,
        },
    };
    let rules_source = toml::to_string(&document).unwrap();
    let definitions = PreparedDefinitions::new_with_source(
        &manifest,
        1,
        tor_simulation::rules::Ruleset::validate(document).unwrap(),
        || None,
    )
    .unwrap();
    let prepared = definitions.population_actor("creature", 1, None).unwrap();
    assert_eq!(prepared.actor.nutrition, NutritionRequirement::Required);
    let restored: tor_simulation::floor_activation::PreparedActorChoice =
        serde_json::from_value(serde_json::to_value(&prepared).unwrap()).unwrap();
    assert_eq!(restored, prepared);
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/two-room");
    let template = super::super::read_package(&root).unwrap();
    let mut regions = template.region_defs().unwrap();
    regions[0].actors.push(
        serde_json::from_value(serde_json::json!({
            "id":2, "at":[2,1,0], "archetype":"creature"
        }))
        .unwrap(),
    );
    let package = super::super::Package::from_parts(manifest, regions, &rules_source).unwrap();
    let mut game = package.build(42, true).unwrap();
    for actor in [tor_simulation::ActorId(1), tor_simulation::ActorId(2)] {
        assert_eq!(
            game.creature(actor).unwrap().nutrition(),
            Nutrition::Required {
                reserve: 8,
                elapsed: 0,
                digestion_remainder: 0,
                regeneration_remainder: 0
            }
        );
    }
    let actor = game
        .spawn_actor(
            tor_world::Location {
                region: tor_world::RegionId(1),
                position: tor_world::Position { x: 3, y: 1, z: 0 },
            },
            std::num::NonZeroU64::new(100).unwrap(),
        )
        .unwrap();
    game.configure_prepared_actor(actor, &restored.actor, 42)
        .unwrap();
    assert_eq!(
        game.creature(actor).unwrap().nutrition(),
        Nutrition::Required {
            reserve: 8,
            elapsed: 0,
            digestion_remainder: 0,
            regeneration_remainder: 0
        }
    );
}

#[test]
fn creature_validation_precedes_lazy_faction_diagnostics() {
    let mut manifest = creature_manifest();
    PreparedDefinitions::new_with_source(
        &manifest,
        1,
        crate::scenario_package::parse_rules(
            include_str!("../../simulation/data/default-ruleset.toml"),
            "test rules",
        )
        .unwrap(),
        || panic!("successful compilation must not acquire diagnostic source"),
    )
    .unwrap();
    manifest.factions.insert("other".into(), Default::default());
    let original = manifest.characters[0].creature.as_ref().unwrap().clone();
    manifest.characters[0]
        .creature
        .as_mut()
        .unwrap()
        .hit_dice
        .clear();
    let error = PreparedDefinitions::new_with_source(
        &manifest,
        1,
        crate::scenario_package::parse_rules(
            include_str!("../../simulation/data/default-ruleset.toml"),
            "test rules",
        )
        .unwrap(),
        || panic!("invalid choices precede faction source acquisition"),
    )
    .unwrap_err();
    assert!(error.message.contains("require 1 to 256 hit dice"));
    manifest.characters[0].creature = Some(original);
    let source: Arc<str> = toml::to_string(&manifest).unwrap().into();
    let line = source
        .lines()
        .position(|line| line.contains("faction = \"neutral\""))
        .expect("the creature faction is declared")
        + 1;
    let reads = std::cell::Cell::new(0);
    let error = PreparedDefinitions::new_with_source(
        &manifest,
        1,
        crate::scenario_package::parse_rules(
            include_str!("../../simulation/data/default-ruleset.toml"),
            "test rules",
        )
        .unwrap(),
        || {
            reads.set(reads.get() + 1);
            Some(source.clone())
        },
    )
    .unwrap_err();
    assert_eq!(reads.get(), 1);
    assert!(
        error.message.contains(&format!("scenario.toml:{line}:")),
        "{}",
        error.message
    );
    assert!(error.message.contains("Unknown creature faction"));
}

#[test]
fn creature_preparation_rejects_external_anatomy_and_unknown_factions() {
    let mut manifest = creature_manifest();
    manifest.characters[0].anatomy = Some(super::super::AnatomySpec {
        slots: vec![],
        capacities: Default::default(),
        primary_attack_socket: super::super::PrimaryAttackSocket::None {},
    });
    assert!(super::PreparedDefinitions::new(&manifest, 1).is_err());
    manifest.characters[0].anatomy = None;
    manifest.factions.insert("other".into(), Default::default());
    assert!(super::PreparedDefinitions::new(&manifest, 1).is_err());
    manifest.factions.clear();
    let prepared = super::PreparedDefinitions::new(&manifest, 1).unwrap();
    assert!(prepared.characters[0].creature.recipe.is_some());

    let actor: super::Actor = serde_json::from_value(serde_json::json!({"id": 2, "at": [2, 1, 0], "creature": manifest.characters[0].creature, "anatomy": {"slots": [], "capacities": {}, "primary_attack_socket": {"kind": "none"}}})).unwrap();
    assert!(prepared.actor(&actor).is_err());
}
use super::*;

#[test]
fn concealed_item_cannot_override_its_physical_class() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/tests/items");
    let package = super::super::read_package(&root).unwrap();
    let prepared = PreparedDefinitions::new(&package.manifest, 1).unwrap();
    let mut item: Item =
        toml::from_str("id = 99\nat = [1, 1, 0]\narchetype = 'healing'\nclass = 'weapon'\n")
            .unwrap();
    assert!(prepared.item(&item, 42, &BTreeMap::new()).is_err());
    item.class = None;
    let spec = prepared.item(&item, 42, &BTreeMap::new()).unwrap();
    assert_eq!(
        spec.class,
        package.manifest.archetypes["healing"].class.into()
    );
}

#[test]
fn assigned_appearance_keeps_label_and_key_paired_including_explicit_absence() {
    let mut manifest: Manifest =
        toml::from_str(include_str!("../../../scenarios/tests/items/scenario.toml")).unwrap();
    manifest.archetypes.get_mut("healing").unwrap().asset = Some("identity.secret-healing".into());
    let identity = manifest.archetypes["healing"].identity.clone().unwrap();
    let prepared = PreparedDefinitions::new(&manifest, 1).unwrap();
    let item: Item = toml::from_str("id = 99\nat = [1, 1, 0]\narchetype = 'healing'\n").unwrap();
    for (name, asset) in [
        ("translated amber label", Some("item.potion.amber")),
        (
            "same identity different public appearance",
            Some("item.potion.crimson"),
        ),
        ("unmarked", None),
    ] {
        let assignment = BTreeMap::from([(
            identity.clone(),
            AppearanceEntry {
                name: name.into(),
                asset: asset.map(str::to_owned),
            },
        )]);
        let spec = prepared.item(&item, 42, &assignment).unwrap();
        assert_eq!(spec.appearance, name);
        assert_eq!(spec.asset.as_deref(), asset);
        assert_eq!(spec.identity, identity);
        assert_ne!(spec.asset.as_deref(), Some("identity.secret-healing"));
    }
}

#[test]
fn prepared_item_assets_keep_pool_absence_and_do_not_reveal_identity_assets() {
    let mut manifest: Manifest =
        toml::from_str(include_str!("../../../scenarios/tests/items/scenario.toml")).unwrap();
    manifest.archetypes.get_mut("arrow").unwrap().asset = Some("item.arrow".into());
    manifest.archetypes.get_mut("healing").unwrap().asset = Some("identity.healing".into());
    for entry in &mut manifest
        .appearance_pools
        .get_mut("potions")
        .unwrap()
        .appearances
    {
        entry.asset = Some("item.shared".into());
    }
    let arrow = &manifest.archetypes["arrow"];
    assert_eq!(item_asset(arrow, &manifest), Some("item.arrow"));
    assert_eq!(
        PreparedArchetype::new(
            arrow,
            &manifest,
            &manifest
                .creatures
                .compile(
                    crate::scenario_package::parse_rules(
                        include_str!("../../simulation/data/default-ruleset.toml"),
                        "test rules"
                    )
                    .unwrap()
                )
                .unwrap()
        )
        .unwrap()
        .item_asset
        .as_deref(),
        Some("item.arrow")
    );
    let hidden = &manifest.archetypes["healing"];
    assert_eq!(item_asset(hidden, &manifest), None);
    assert_eq!(
        item_assets(hidden, &manifest).collect::<BTreeSet<_>>(),
        BTreeSet::from(["item.shared"])
    );
    assert_eq!(
        PreparedArchetype::new(
            hidden,
            &manifest,
            &manifest
                .creatures
                .compile(
                    crate::scenario_package::parse_rules(
                        include_str!("../../simulation/data/default-ruleset.toml"),
                        "test rules"
                    )
                    .unwrap()
                )
                .unwrap()
        )
        .unwrap()
        .item_asset
        .as_deref(),
        None
    );
    for entry in &mut manifest
        .appearance_pools
        .get_mut("potions")
        .unwrap()
        .appearances
    {
        entry.asset = None;
    }
    let hidden = &manifest.archetypes["healing"];
    assert_eq!(item_asset(hidden, &manifest), None);
    assert_eq!(
        PreparedArchetype::new(
            hidden,
            &manifest,
            &manifest
                .creatures
                .compile(
                    crate::scenario_package::parse_rules(
                        include_str!("../../simulation/data/default-ruleset.toml"),
                        "test rules"
                    )
                    .unwrap()
                )
                .unwrap()
        )
        .unwrap()
        .item_asset,
        None
    );
}
use crate::scenario_package::{AiProfile, AppearancePool, BodySpec};
use std::path::Path;

#[test]
fn prepared_controls_resolve_profiles_and_preserve_selection_and_omission() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/two-room");
    let mut manifest = super::super::read_package(&root).unwrap().manifest;
    manifest.ai_profiles.insert(
        "careful".into(),
        AiProfile {
            care: Some(true),
            equipment: Some(true),
            loot: Some(true),
            flee_percent: Some(19),
            ..Default::default()
        },
    );
    let mut other = manifest.characters[0].clone();
    other.id = 2;
    other.unselected = "ai".into();
    other.ai = Some("careful".into());
    manifest.characters.push(other);
    let mut omitted = manifest.characters[0].clone();
    omitted.id = 3;
    manifest.characters.push(omitted);
    let prepared = PreparedDefinitions::new(&manifest, 1).unwrap();
    let first = &prepared.characters[0].creature.control;
    assert!(first.spawned() && first.profile().unwrap().is_none());
    assert!(prepared.characters[1].creature.control.spawned());
    assert!(!prepared.characters[2].creature.control.spawned());
    assert!(!prepared.omitted_carrier(Some(1)));
    assert!(!prepared.omitted_carrier(Some(2)));
    assert!(prepared.omitted_carrier(Some(3)));
    manifest
        .ai_profiles
        .get_mut("careful")
        .unwrap()
        .flee_percent = Some(99);
    assert_eq!(
        prepared.characters[1]
            .creature
            .control
            .profile()
            .unwrap()
            .unwrap()
            .flee_percent,
        Some(19)
    );
    let selected_other = PreparedDefinitions::new(&manifest, 2).unwrap();
    assert!(selected_other.characters[1]
        .creature
        .control
        .profile()
        .unwrap()
        .is_none());
    assert!(selected_other.omitted_carrier(Some(1)));
    manifest.characters[1].ai = None;
    let defaulted = PreparedDefinitions::new(&manifest, 1).unwrap();
    assert_eq!(
        defaulted.characters[1]
            .creature
            .control
            .profile()
            .unwrap()
            .unwrap(),
        &AiProfile::default()
    );
    assert!(
        PreparedDefinitions::new(&manifest, 2).unwrap().characters[1]
            .creature
            .control
            .profile()
            .unwrap()
            .is_none()
    );
    manifest.characters[1].ai = Some("missing".into());
    let deferred = PreparedDefinitions::new(&manifest, 1).unwrap();
    assert!(deferred.characters[1].creature.control.spawned());
    assert_eq!(
        deferred.characters[1]
            .creature
            .control
            .profile()
            .unwrap_err()
            .name,
        "missing"
    );
    // Selected characters are externally controlled even if their unused
    // unselected-mode reference is missing; preserve the existing policy.
    assert!(
        PreparedDefinitions::new(&manifest, 2).unwrap().characters[1]
            .creature
            .control
            .profile()
            .unwrap()
            .is_none()
    );
}

#[test]
fn compilation_preserves_manifest_attributes_and_concealed_assets() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/two-room");
    let mut manifest = super::super::read_package(&root).unwrap().manifest;
    manifest.ai_profiles.insert(
        "careful".into(),
        AiProfile {
            care: Some(true),
            equipment: Some(true),
            loot: Some(true),
            flee_percent: Some(19),
            ..Default::default()
        },
    );
    manifest.appearance_pools.insert(
        "hidden".into(),
        AppearancePool {
            appearances: vec![AppearanceEntry {
                name: "unmarked".into(),
                asset: Some("item.hidden".into()),
            }],
            confounding: false,
        },
    );
    let definitions: Manifest = toml::from_str(include_str!(
        "../../../scenarios/tests/interactions/scenario.toml"
    ))
    .unwrap();
    manifest.creatures = definitions.creatures;
    manifest
        .creatures
        .species
        .get_mut("figure")
        .unwrap()
        .modifiers = vec![crate::creature_authoring::Modifier {
        cancellable: false,
        scope: crate::creature_authoring::Scope::Always,
        op: crate::creature_authoring::Op::Add {
            stat: crate::creature_authoring::Stat::Health,
            value: 33,
        },
    }];
    let mut creature = definitions.characters[0].creature.clone().unwrap();
    creature.name = "guard".into();
    creature.corpse_identity = "item.corpse.test.guard".parse().unwrap();
    manifest.archetypes.insert(
        "guard".into(),
        Archetype {
            creature: Some(creature),
            anatomy: None,
            equipment: None,
            projectile: None,
            equipment_generation: None,
            consumable: None,
            readable: None,
            ground_scare: None,
            device: None,
            held_light: None,
            edible: None,
            supply: None,
            magical: false,
            currency: None,
            class: Default::default(),
            body: Some(BodySpec {
                cells: vec![[0, 0, 0]],
                eye: [0, 0, 0],
                mass: 91,
            }),
            identity: Some("item.misc.guard-item".parse().unwrap()),
            appearance_pool: Some("hidden".into()),
            forms: vec![],
            stackable: true,
            properties: BTreeMap::from([("quality".into(), "fine".into())]),
            name: Some("guard token".into()),
            turn_ticks: Some(73),
            asset: Some("creature.guard".into()),
        },
    );
    let prepared = PreparedDefinitions::new(&manifest, 1).unwrap();
    let guard = prepared.archetype(&Some("guard".into())).unwrap();
    let recipe = guard.recipe.as_ref().unwrap();
    assert_eq!(recipe.identity().name, "guard");
    assert_eq!(
        recipe
            .instantiate(42)
            .unwrap()
            .derive()
            .unwrap()
            .maximum_health,
        41
    );
    assert_eq!(guard.body.as_ref().unwrap().mass, 91);
    assert_eq!(
        guard.identity.as_ref().map(|key| key.as_str()),
        Some("item.misc.guard-item")
    );
    assert!(guard.concealed && guard.stackable);
    assert_eq!(
        guard.properties.get("quality").map(String::as_str),
        Some("fine")
    );
    assert_eq!(guard.name.as_deref(), Some("guard token"));
    assert_eq!(guard.turn_ticks, Some(73));
    assert_eq!(guard.actor_asset.as_deref(), Some("creature.guard"));
    assert_eq!(
        guard.item_asset, None,
        "concealed asset waits for paired seeded appearance"
    );
    assert_eq!(prepared.ai_profiles["careful"].care, Some(true));
    assert_eq!(prepared.ai_profiles["careful"].flee_percent, Some(19));
    let ordinary = prepared.archetype(&None).unwrap();
    assert!(ordinary.recipe.is_none() && ordinary.body.is_none() && !ordinary.stackable);
    assert!(prepared.archetype(&Some("missing".into())).is_err());
}
