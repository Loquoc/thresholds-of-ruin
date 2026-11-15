use crate::support;
use tor_server::{
    arena_evaluation,
    scenario_package::{ArenaControl, Package},
    Scenario,
};

fn scenario(actions: u64, ticks: u64) -> Scenario {
    let mut scenario = support::load("mob-arena", 42);
    let package = scenario.package.take().unwrap();
    let mut manifest = package.manifest.clone();
    let arena = manifest.arena.as_mut().unwrap();
    arena.control = ArenaControl::AllAi;
    arena.start_paused = false;
    arena.actions = actions;
    arena.ticks = ticks;
    scenario.package = Some(std::sync::Arc::new(
        Package::from_parts(
            manifest,
            package.region_defs().unwrap(),
            &package.rules_source,
        )
        .unwrap(),
    ));
    scenario
}

#[test]
fn a_requested_timeline_records_every_step_without_changing_the_run() {
    let plain = arena_evaluation::run(scenario(32, 100000)).unwrap();
    assert!(plain.timeline.is_none());
    assert!(serde_json::to_value(&plain)
        .unwrap()
        .get("timeline")
        .is_none());
    let traced = arena_evaluation::run_with(
        scenario(32, 100000),
        arena_evaluation::RunOptions { timeline: true },
    )
    .unwrap();
    let timeline = traced.timeline.clone().unwrap();
    // The trace observes the run; the measured outcome is identical.
    assert_eq!(
        arena_evaluation::ArenaEvaluation {
            timeline: None,
            ..traced
        },
        plain
    );
    assert_eq!(timeline.len(), 32);
    assert!(timeline.windows(2).all(|pair| pair[0].tick <= pair[1].tick));
    let ids: Vec<_> = plain.participants.iter().map(|p| p.actor).collect();
    assert!(timeline.iter().all(|step| ids.contains(&step.actor)));
    assert!(timeline.iter().all(|step| !step.journal.is_empty()));
    let resolved = timeline
        .iter()
        .flat_map(|step| &step.events)
        .filter(|event| event.get("Resolved").is_some())
        .count();
    assert!(resolved > 0);
    let value = serde_json::to_value(&timeline[0]).unwrap();
    assert!(value["tick"].is_string());
    assert!(value["journal"][0]["type"].is_string());
}

#[test]
fn ordinary_engine_arena_report_is_deterministic_bounded_and_conserves_damage() {
    let report = arena_evaluation::run(scenario(32, 100000)).unwrap();
    assert_eq!(report, arena_evaluation::run(scenario(32, 100000)).unwrap());
    assert_eq!(report.actions, 32);
    assert_eq!(
        report.termination,
        arena_evaluation::ArenaTermination::ActionLimit
    );
    assert_eq!(report.participants.len(), 4);
    let dealt: u64 = report.participants.iter().map(|p| p.damage_dealt).sum();
    let received: u64 = report.participants.iter().map(|p| p.damage_received).sum();
    assert!(dealt > 0);
    assert_eq!(dealt, received);
    for participant in &report.participants {
        assert_eq!(
            participant.initial.injury + participant.damage_received,
            participant.final_state.injury
                + participant.healing_received
                + participant.health_recovered
        );
        assert!(participant
            .final_state
            .resources
            .iter()
            .all(|r| r.reserved == 0));
    }
    assert!(report
        .participants
        .iter()
        .all(|p| p.hits <= p.attack_checks));
    assert!(report.participants.iter().any(|p| !p.abilities.is_empty()));
    let value = serde_json::to_value(&report).unwrap();
    assert_eq!(value["actions"], "32");
    assert_eq!(report.input_hash.len(), 64);
    let source = scenario(32, 100000);
    let certificate = &source.package.as_ref().unwrap().certificate;
    assert_eq!(report.format, "tor-arena-run-v6");
    assert_eq!(report.ruleset, certificate.ruleset);
    assert_eq!(report.ruleset_hash, certificate.ruleset_hash);
    assert_eq!(report.input.ruleset_hash, report.ruleset_hash);
}

#[test]
fn arena_report_has_explicit_tick_limit_and_rejects_nonarenas_or_manual_control() {
    let report = arena_evaluation::run(scenario(10000, 7)).unwrap();
    assert_eq!(report.tick, 7);
    assert_eq!(
        report.termination,
        arena_evaluation::ArenaTermination::TickLimit
    );
    assert!(arena_evaluation::run(Scenario::two_room(42)).is_err());
    assert!(arena_evaluation::run(support::load("mob-arena", 42)).is_err());
    let mut changed = scenario(32, 100000);
    changed.seed = 43;
    assert_ne!(
        arena_evaluation::run(scenario(32, 100000))
            .unwrap()
            .input_hash,
        arena_evaluation::run(changed).unwrap().input_hash
    );
}

#[test]
fn arena_report_retains_elimination_evidence_and_survivor_faction() {
    let report = arena_evaluation::run(scenario(10000, 100000)).unwrap();
    let arena_evaluation::ArenaTermination::Elimination {
        winner: Some(winner),
    } = &report.termination
    else {
        panic!(
            "seeded duel should reach elimination, got {:?}",
            report.termination
        );
    };
    assert!(report.actions < 10000);
    assert!(report
        .participants
        .iter()
        .filter(|p| !p.final_state.dead)
        .all(|p| &p.faction == winner));
    assert!(report.participants.iter().any(|p| p.final_state.dead));
}

#[test]
fn arena_health_accounting_separates_natural_recovery_from_damage_and_healing() {
    let mut scenario = scenario(32, 100000);
    let package = scenario.package.take().unwrap();
    use tor_simulation::rules::{AmountExpr, Effect, HealthRecoveryDef, StrikeSource};

    let mut document = package.rules().unwrap().document().clone();
    document.health_recovery = HealthRecoveryDef {
        base_period: 1,
        reduction_per_hd: 0,
        minimum_period: 1,
    };
    for ability in &mut document.abilities {
        if let StrikeSource::Authored { strikes } = &mut ability.strikes {
            for strike in strikes {
                // Damage and healing share the target recipient. Preserve
                // source observations and recognized self-defense payloads.
                if strike
                    .on_success
                    .iter()
                    .any(|effect| matches!(effect, Effect::Damage { .. }))
                {
                    strike.on_success.push(Effect::Heal {
                        amount: AmountExpr::Fixed { amount: 1 },
                    });
                }
            }
        }
    }
    // Basic melee lowers the selected natural source instead of owning an
    // authored ability payload. Keep its actual damage-then-heal coverage too.
    let mut manifest = package.manifest.clone();
    for species in manifest.creatures.species.values_mut() {
        for strike in &mut species.melee.strikes {
            strike
                .on_success
                .push(tor_server::scenario_package::Effect::Heal {
                    amount: tor_server::scenario_package::AmountExpr::Fixed { amount: 1 },
                });
        }
    }
    scenario.package = Some(std::sync::Arc::new(
        Package::from_parts(
            manifest,
            package.region_defs().unwrap(),
            &toml::to_string(&document).unwrap(),
        )
        .unwrap(),
    ));
    let report = arena_evaluation::run(scenario).unwrap();
    assert_eq!(report.format, "tor-arena-run-v6");
    assert!(report
        .participants
        .iter()
        .any(|actor| actor.healing_received > 0));
    assert!(report
        .participants
        .iter()
        .any(|actor| actor.health_recovered > 0));
    for actor in &report.participants {
        assert_eq!(
            actor.initial.injury + actor.damage_received,
            actor.final_state.injury + actor.healing_received + actor.health_recovered
        );
    }
}

#[test]
fn funded_ranged_positioning_reopens_exact_actor_states_and_next_decision() {
    use tor_protocol::ActorId;
    use tor_server::{scenario_package, Engine, SavePolicy};
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("funded");
    support::copy_package(&support::package("mob-arena"), &root);
    let edit = |relative: &str, change: &dyn Fn(&mut toml::Value)| {
        let path = root.join(relative);
        let mut value: toml::Value =
            toml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        change(&mut value);
        std::fs::write(path, toml::to_string(&value).unwrap()).unwrap();
    };
    edit("catalog/actors.toml", &|value| {
        let dice = value["actors"]["arena.red_arcanist"]["creature"]["hit_dice"]
            .as_array_mut()
            .unwrap();
        for die in &mut dice[1..] {
            die.as_table_mut().unwrap().remove("purchases");
        }
    });
    edit("regions/1.toml", &|value| {
        let actors = value["actors"].as_array_mut().unwrap();
        actors.retain(|actor| actor["id"].as_integer() == Some(4));
        actors[0]["at"] = toml::Value::Array(vec![2.into(), 2.into(), 0.into()]);
    });
    edit("scenario.toml", &|value| {
        value["arena"]["participants"] = toml::Value::Array(vec![1.into(), 4.into()]);
    });
    // Compile the authored fixture through the genuine validator, not edited
    // certificates or a private runtime mutation of the behavior under test.
    scenario_package::validate(&root).unwrap();
    let mut scenario = scenario_package::load(&root, 0, None, false).unwrap();
    let template = scenario.package.take().unwrap();
    let mut manifest = template.manifest.clone();
    manifest.arena.as_mut().unwrap().control = ArenaControl::AllAi;
    manifest.arena.as_mut().unwrap().start_paused = false;
    scenario.package = Some(std::sync::Arc::new(
        Package::from_parts(
            manifest,
            template.region_defs().unwrap(),
            &template.rules_source,
        )
        .unwrap(),
    ));
    let save = directory.path().join("replay.db");
    let mut engine = Engine::open_with_policy(
        &save,
        scenario.clone(),
        SavePolicy {
            checkpoint_interval: 0,
            ..SavePolicy::default()
        },
    )
    .unwrap();
    let mut repositioned = false;
    let mut cast = false;
    for _ in 0..8 {
        let (actor, action) = engine.next_ai_action().expect("funded arena still active");
        if actor == ActorId(4) {
            repositioned |= matches!(action, tor_server::journal::Action::Move { .. });
            cast |= matches!(&action, tor_server::journal::Action::UseAbility { id, .. } if id.as_str() == "spark_lance_fire");
        }
        engine.advance_ai(actor).unwrap();
    }
    assert!(
        repositioned && cast,
        "replay must exercise actual repositioning and paid ranged work"
    );
    let expected: Vec<_> = engine
        .actors()
        .into_iter()
        .map(|id| (id, engine.state(id).unwrap()))
        .collect();
    let next = engine.next_ai_action();
    let accepted = engine.save_status().accepted_sequence;
    engine.flush().unwrap();
    drop(engine);
    let reopened = Engine::open(&save, scenario).unwrap();
    let recovery = reopened.recovery_profile();
    assert_eq!(recovery.checkpoint_sequence, 0);
    assert!(recovery.records_replayed > 0);
    assert_eq!(recovery.records_replayed, recovery.records_loaded);
    let actual: Vec<_> = reopened
        .actors()
        .into_iter()
        .map(|id| (id, reopened.state(id).unwrap()))
        .collect();
    assert_eq!(actual, expected);
    assert_eq!(reopened.next_ai_action(), next);
    assert_eq!(reopened.save_status().accepted_sequence, accepted);
}

#[test]
fn distant_gold_pursuit_replays_from_journal_without_a_checkpoint() {
    distant_loot_lifecycle_replay(false, false);
}

#[test]
fn distant_useful_armor_is_collected_equipped_and_replayed() {
    distant_loot_lifecycle_replay(true, false);
}

#[test]
fn distant_healing_potion_is_collected_drunk_and_replayed() {
    distant_loot_lifecycle_replay(false, true);
}

fn distant_loot_lifecycle_replay(equipment: bool, potion: bool) {
    use tor_protocol::ActorId;
    use tor_server::{scenario_package, Engine, SavePolicy};
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("gold");
    support::copy_package(&support::package("mob-arena"), &root);
    let edit = |relative: &str, change: &dyn Fn(&mut toml::Value)| {
        let path = root.join(relative);
        let mut value: toml::Value =
            toml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        change(&mut value);
        std::fs::write(path, toml::to_string(&value).unwrap()).unwrap();
    };
    edit("scenario.toml", &|value| {
        value["arena"]["participants"] = toml::Value::Array(vec![1.into(), 4.into()]);
        value["factions"] = toml::Value::Table(Default::default());
        value["ai_profiles"]["sparring"] = toml::from_str::<toml::Value>(
            "doors=false\ncare=false\nequipment=false\nloot=false\ngold=true\nflee_percent=0",
        )
        .unwrap();
        value["creatures"]["species"]["human"]["behavior"]["disposition"] = "passive".into();
        value["creatures"]["species"]["human"]["behavior"]["idle"] = "explore".into();
        value
            .as_table_mut()
            .unwrap()
            .insert("archetypes".into(), toml::Value::Table(Default::default()));
        value["archetypes"].as_table_mut().unwrap().insert("eval_gold".into(), toml::from_str::<toml::Value>(
            "name='gold coin'\nidentity='item.coin.eval-gold'\nclass='coin'\nstackable=true\ncurrency='gold'\nmagical=false").unwrap());
    });
    if equipment {
        edit("scenario.toml", &|value| {
            value["ai_profiles"]["sparring"]["equipment"] = true.into();
            value["ai_profiles"]["sparring"]["loot"] = true.into();
            value["ai_profiles"]["sparring"]["gold"] = false.into();
            value["archetypes"]["eval_gold"] = toml::from_str::<toml::Value>(r#"
name = "protective helm"
identity = "item.armor.replay-helm"
class = "armor"
stackable = false
magical = false
equipment = { slot = "head_armor", modifiers = [{ scope = "always", op = { kind = "add", stat = { kind = "defense", defense = "physical" }, value = 10 } }] }
"#).unwrap();
        });
    }
    if potion {
        edit("scenario.toml", &|value| {
            value["arena"]["actions"] = 64.into();
            // One authored hit die keeps the real trap injury below the normal
            // care threshold without altering the canonical healing policy.
            value["characters"][0]["creature"]["hit_dice"]
                .as_array_mut()
                .unwrap()
                .truncate(1);
            value["ai_profiles"]["sparring"]["care"] = true.into();
            value["ai_profiles"]["sparring"]["loot"] = true.into();
            value["ai_profiles"]["sparring"]["gold"] = false.into();
            value["archetypes"]["eval_gold"] = toml::from_str::<toml::Value>(
                r#"
name = "restorative dose"
identity = "item.potion.replay-restorative"
class = "potion"
stackable = true
consumable = { effects = [{ kind = "heal", amount = { kind = "fixed", amount = 1 } }] }
"#,
            )
            .unwrap();
            value["archetypes"].as_table_mut().unwrap().insert("injury_arrow".into(), toml::from_str::<toml::Value>(r#"
name = "fixture arrow"
identity = "item.weapon.replay-arrow"
class = "weapon"
stackable = true
projectile = { family = "fixture.bow", damage = { primary = { category = "keen", sides = 3 }, components = [{ category = "keen", amount = { type = "rolled", count = 2, sides = 3, bonus = 0 } }] }, recovery = "consumed_on_hit" }
"#).unwrap());
        });
    }
    std::fs::write(root.join("regions/1.toml"), r#"
id = 1
name = "Distant gold replay control"
size = [9, 5, 2]
chamber = true
gravity = [0,0,-1]
anchors = { start = [1, 2, 0] }
walls = [[7,4,0], [7,4,1], [8,3,0], [8,3,1], [7,3,0], [7,3,1]]
actors = [{ id = 4, at = [8,4,0], archetype = "arena.red_arcanist", controller = "ai", ai = "sparring" }]
items = [{ id = 100, at = [7,2,0], archetype = "eval_gold", quantity = 7 }]
"#).unwrap();
    if equipment {
        edit("regions/1.toml", &|value| {
            let items = value["items"].as_array_mut().unwrap();
            items[0]["quantity"] = 1.into();
            let mut duplicate = items[0].clone();
            duplicate["id"] = 101.into();
            items.push(duplicate);
        });
    }
    if potion {
        edit("regions/1.toml", &|value| {
            let items = value["items"].as_array_mut().unwrap();
            items[0]["quantity"] = 1.into();
            value.as_table_mut().unwrap().insert("traps".into(), toml::Value::Array(vec![toml::from_str::<toml::Value>(
                "id=1\nat=[2,2,0]\ndc=100\navoidance_dc=100\nkind={kind='arrow', attack_bonus=100, ammunition='injury_arrow'}"
            ).unwrap()]));
        });
    }
    // Compile the authored fixture through the genuine validator, not edited
    // certificates or a private runtime mutation of the behavior under test.
    scenario_package::validate(&root).unwrap();
    let mut scenario = scenario_package::load(&root, 0, None, false).unwrap();
    let template = scenario.package.take().unwrap();
    let mut manifest = template.manifest.clone();
    manifest.arena.as_mut().unwrap().control = ArenaControl::AllAi;
    manifest.arena.as_mut().unwrap().start_paused = false;
    scenario.package = Some(std::sync::Arc::new(
        Package::from_parts(
            manifest,
            template.region_defs().unwrap(),
            &template.rules_source,
        )
        .unwrap(),
    ));
    let save = directory.path().join("replay.db");
    let mut engine = Engine::open_with_policy(
        &save,
        scenario.clone(),
        SavePolicy {
            checkpoint_interval: 0,
            ..SavePolicy::default()
        },
    )
    .unwrap();
    let mut decisions = 0;
    let mut collected = false;
    let mut equipped = false;
    let mut equip_requested = false;
    let mut drink_requested = false;
    let mut drank = false;
    let mut hp_before_drink = 0;
    for _ in 0..30 {
        let (actor, action) = engine.next_ai_action().expect("gold arena active");
        if actor == ActorId(1) {
            decisions += 1;
            if decisions <= 6 {
                assert!(
                    matches!(action, tor_server::journal::Action::Move { .. }),
                    "each approach decision is real movement"
                );
            }
            if matches!(action, tor_server::journal::Action::Take { .. }) {
                assert_eq!(decisions, 7, "six real movement steps then collection");
                collected = true;
            }
            if potion
                && matches!(action, tor_server::journal::Action::Drink { .. })
                && !drink_requested
            {
                assert!(collected);
                assert_eq!(decisions, 8, "drink follows pickup");
                drink_requested = true;
                hp_before_drink = engine
                    .state(actor)
                    .unwrap()
                    .observation
                    .combat
                    .as_ref()
                    .unwrap()
                    .hp;
            }
            if matches!(action, tor_server::journal::Action::Equip { .. }) {
                assert!(collected, "only owned gear may be equipped");
                if !equip_requested {
                    assert_eq!(decisions, 8, "equip follows six moves and pickup");
                    equip_requested = true;
                }
            }
        }
        let committed = engine.advance_ai(actor).unwrap();
        if actor == ActorId(1) && decisions <= 6 {
            let record = serde_json::to_value(&committed.entry.content).unwrap();
            assert_eq!(record["event"]["type"], "moved");
            assert_eq!(record["event"]["from"]["x"], decisions);
            assert_eq!(
                record["event"]["to"]["x"],
                decisions + 1,
                "real journal confirms each approach cell, not observer-relative coordinates"
            );
        }
        if potion && actor == ActorId(1) && decisions == 1 {
            let state = engine.state(actor).unwrap();
            let combat = state.observation.combat.as_ref().unwrap();
            assert!(
                combat.hp * 2 <= combat.max_hp,
                "real hidden trap injures before five-step pursuit"
            );
        }
        if equipment {
            equipped = engine
                .state(ActorId(1))
                .unwrap()
                .observation
                .interactions
                .as_ref()
                .unwrap()
                .inventory
                .iter()
                .any(|item| item.equipped_slot.is_some());
        }
        if potion && drink_requested {
            let state = engine.state(ActorId(1)).unwrap();
            drank = state.observation.inventory.is_empty()
                && state.observation.combat.as_ref().unwrap().hp >= hp_before_drink + 1;
        }
        if collected && (!equipment || equipped) && (!potion || drank) {
            break;
        }
    }
    assert!(
        collected,
        "replay witness must actually collect the distant gold"
    );
    let state = engine.state(ActorId(1)).unwrap();
    if equipment {
        assert!(equipped && equip_requested);
        let owned = &state.observation.inventory[0];
        assert_eq!(owned.name, "protective helm");
        let interactions = state.observation.interactions.as_ref().unwrap();
        let equipped_row = interactions
            .inventory
            .iter()
            .find(|row| row.item == owned.id)
            .unwrap();
        let slot = equipped_row.equipped_slot.unwrap();
        assert_eq!(interactions.slots[usize::from(slot)].id, "head_armor");
        assert_eq!(state.observation.inventory.len(), 1);
        assert_eq!(
            state.observation.inventory[0].quantity, 1,
            "take one useful armor, not the duplicate"
        );
        assert_eq!(
            state
                .observation
                .interactions
                .as_ref()
                .unwrap()
                .inventory
                .iter()
                .filter(|item| item.equipped_slot.is_some())
                .count(),
            1,
            "successful real equipment commit"
        );
        assert_eq!(
            state.observation.ground_items.len(),
            1,
            "duplicate remains on ground"
        );
        let mut checked_next_owner_decision = false;
        for _ in 0..8 {
            let (id, next) = engine.next_ai_action().unwrap();
            if id == ActorId(1)
                && engine
                    .state(id)
                    .unwrap()
                    .observation
                    .combat
                    .as_ref()
                    .unwrap()
                    .preparation_remaining
                    .is_none()
            {
                assert!(matches!(next, tor_server::journal::Action::Move { .. }),
                    "fresh owner decision explores instead of collecting duplicate or continuing Equip");
                checked_next_owner_decision = true;
                break;
            }
            engine.advance_ai(id).unwrap();
        }
        assert!(
            checked_next_owner_decision,
            "must inspect owner, not other actor's turn"
        );
    } else if potion {
        assert!(
            drink_requested && drank,
            "owned potion consumed with actual healing"
        );
        assert!(state.observation.inventory.is_empty());
        assert!(state.observation.ground_items.is_empty());
    } else {
        assert_eq!(
            state
                .observation
                .inventory
                .iter()
                .filter(|item| item.known_currency == Some(tor_protocol::CurrencyKind::Gold))
                .map(|item| item.quantity)
                .sum::<u64>(),
            7,
            "actual successful collection"
        );
    }
    let expected: Vec<_> = engine
        .actors()
        .into_iter()
        .map(|id| (id, engine.state(id).unwrap()))
        .collect();
    let next = engine.next_ai_action();
    let accepted = engine.save_status().accepted_sequence;
    engine.flush().unwrap();
    drop(engine);
    let reopened = Engine::open(&save, scenario.clone()).unwrap();
    let recovery = reopened.recovery_profile();
    assert_eq!(recovery.checkpoint_sequence, 0);
    assert!(recovery.records_replayed > 0);
    assert_eq!(recovery.records_replayed, recovery.records_loaded);
    let actual: Vec<_> = reopened
        .actors()
        .into_iter()
        .map(|id| (id, reopened.state(id).unwrap()))
        .collect();
    assert_eq!(actual, expected);
    assert_eq!(reopened.next_ai_action(), next);
    assert_eq!(reopened.save_status().accepted_sequence, accepted);
    if potion {
        // The independent deterministic evaluator attributes resolved Heal
        // separately from natural recovery; HP increase alone cannot do that.
        let report = arena_evaluation::run(scenario).unwrap();
        let owner = report
            .participants
            .iter()
            .find(|actor| actor.actor == ActorId(1))
            .unwrap();
        assert_eq!(owner.healing_received, 1, "one genuine restorative effect");
    }
}

#[test]
fn distant_launcher_and_payload_are_taken_equipped_and_replayed() {
    use tor_protocol::ActorId;
    use tor_server::{scenario_package, Engine, SavePolicy};
    use tor_simulation::rules::{Reach, StrikeSource};
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("ranged-loot");
    support::copy_package(&support::package("mob-arena"), &root);
    let source = scenario_package::load(&root, 42, None, false).unwrap();
    let mut rules = source
        .package
        .as_ref()
        .unwrap()
        .rules()
        .unwrap()
        .document()
        .clone();
    let mut launch = rules
        .abilities
        .iter()
        .find(|ability| ability.basic_attack)
        .unwrap()
        .clone();
    launch.id = tor_simulation::content_ids::AbilityId::new("fixture.launch").unwrap();
    launch.basic_attack = false;
    launch.invocation = tor_simulation::abilities::InvocationKind::DirectionalProjectile;
    launch.reach = Reach::Projectile;
    launch.strikes = StrikeSource::Projectile {
        delivery: tor_simulation::projectile_profiles::ProjectileDeliveryKind::Launch,
    };
    launch.ai.priority = 100;
    rules.constants.default_abilities.push(launch.id.clone());
    rules.abilities.push(launch);
    rules
        .expertises
        .push(tor_simulation::expertise::ExpertiseDef {
            id: tor_simulation::content_ids::ExpertiseId::new("fixture.bow").unwrap(),
            name: "fixture bow".into(),
            category: tor_simulation::expertise::ExpertiseCategory::Equipment,
        });
    std::fs::write(root.join("rules.toml"), toml::to_string(&rules).unwrap()).unwrap();
    let path = root.join("scenario.toml");
    let mut manifest: toml::Value =
        toml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    manifest["arena"]["participants"] = toml::Value::Array(vec![1.into(), 4.into()]);
    manifest["arena"]["control"] = "all_ai".into();
    manifest["factions"] = toml::Value::Table(Default::default());
    manifest["ai_profiles"]["sparring"] = toml::from_str(
        "doors=false\ncare=false\nequipment=true\nloot=true\ngold=false\nflee_percent=0",
    )
    .unwrap();
    manifest["creatures"]["species"]["human"]["behavior"]["disposition"] = "passive".into();
    manifest["creatures"]["species"]["human"]["behavior"]["idle"] = "explore".into();
    let items: toml::Value = toml::from_str(r#"
[bow]
name = "fixture bow"
identity = "item.weapon.fixture-bow"
class = "weapon"
equipment = { slot = "weapon", attack = { skill = "light_weaponry", bonus = 0, wind_up = 60, recovery = 40, damage = { primary = { category = "impact" }, components = [{ category = "impact", amount = { type = "fixed", value = 1 } }] } }, weapon_profile = { expertise = "fixture.bow", launcher = { family = "fixture.bow", range = { short_feet = 80, long_feet = 320 } }, base_traits = [], expert_traits = { add = [], remove = [] } }, modifiers = [] }
[arrow]
name = "compatible arrow"
identity = "item.weapon.fixture-arrow"
class = "weapon"
stackable = true
projectile = { family = "fixture.bow", damage = { primary = { category = "keen" }, components = [{ category = "keen", amount = { type = "fixed", value = 3 } }] }, recovery = "consumed_on_hit" }
[decoy]
name = "incompatible arrow"
identity = "item.weapon.fixture-decoy"
class = "weapon"
stackable = true
projectile = { family = "fixture.other", damage = { primary = { category = "keen" }, components = [{ category = "keen", amount = { type = "fixed", value = 3 } }] }, recovery = "consumed_on_hit" }
"#).unwrap();
    manifest
        .as_table_mut()
        .unwrap()
        .insert("archetypes".into(), items);
    std::fs::write(path, toml::to_string(&manifest).unwrap()).unwrap();
    std::fs::write(root.join("regions/1.toml"), r#"
id=1
name="Ranged acquisition replay"
size=[9,5,2]
chamber=true
gravity=[0,0,-1]
anchors={start=[1,2,0]}
walls=[[7,4,0],[7,4,1],[8,3,0],[8,3,1],[7,3,0],[7,3,1]]
actors=[{id=4,at=[8,4,0],archetype="arena.red_arcanist",controller="ai",ai="sparring"}]
items=[{id=100,at=[7,2,0],archetype="bow",quantity=1},{id=101,at=[7,2,0],archetype="arrow",quantity=20},{id=102,at=[7,2,0],archetype="decoy",quantity=20}]
"#).unwrap();
    scenario_package::validate(&root).unwrap();
    let scenario = scenario_package::load(&root, 42, None, false).unwrap();
    let save = directory.path().join("replay.db");
    let mut engine = Engine::open_with_policy(
        &save,
        scenario.clone(),
        SavePolicy {
            checkpoint_interval: 0,
            ..SavePolicy::default()
        },
    )
    .unwrap();
    let initial = engine.state(ActorId(1)).unwrap();
    assert!(initial.observation.inventory.is_empty());
    for name in ["fixture bow", "compatible arrow", "incompatible arrow"] {
        assert!(
            initial
                .observation
                .ground_items
                .iter()
                .any(|item| item.item.name == name && item.item.identified),
            "authored acquisition candidate must be disclosed: {name}"
        );
    }
    let mut owner_decisions = 0;
    let mut equipped = false;
    let mut takes = Vec::new();
    for _ in 0..80 {
        let (actor, action) = engine.next_ai_action().unwrap();
        if actor == ActorId(1) {
            owner_decisions += 1;
        }
        let result = engine.advance_ai(actor).unwrap();
        if actor == ActorId(1) {
            if let tor_server::journal::Action::Take { quantity, .. } = &action {
                takes.push((owner_decisions, *quantity));
                let record = serde_json::to_value(&result.entry.content).unwrap();
                assert_eq!(
                    record["event"]["type"], "taken",
                    "selected Take must execute"
                );
                assert_eq!(record["event"]["quantity"], quantity.unwrap());
            }
        }
        if actor == ActorId(1) && owner_decisions <= 6 {
            assert!(matches!(action, tor_server::journal::Action::Move { .. }));
            let record = serde_json::to_value(&result.entry.content).unwrap();
            assert_eq!(record["event"]["type"], "moved");
            assert_eq!(record["event"]["from"]["x"], owner_decisions);
            assert_eq!(record["event"]["to"]["x"], owner_decisions + 1);
        }
        let state = engine.state(ActorId(1)).unwrap();
        let inventory = &state.observation.inventory;
        if let Some(bow) = inventory.iter().find(|item| item.name == "fixture bow") {
            equipped = state
                .observation
                .interactions
                .as_ref()
                .unwrap()
                .inventory
                .iter()
                .any(|row| row.item == bow.id && row.equipped_slot.is_some());
        }
        if equipped
            && inventory
                .iter()
                .any(|item| item.name == "compatible arrow" && item.quantity == 6)
            && state
                .observation
                .combat
                .as_ref()
                .unwrap()
                .preparation_remaining
                .is_none()
        {
            break;
        }
    }
    assert!(
        equipped,
        "ordinary autonomous actions must actually equip acquired launcher"
    );
    assert_eq!(
        takes,
        vec![(7, Some(1)), (8, Some(6))],
        "six moves then actual launcher and reserve pickup"
    );
    let state = engine.state(ActorId(1)).unwrap();
    assert_eq!(state.observation.inventory.len(), 2);
    let bow = state
        .observation
        .inventory
        .iter()
        .find(|item| item.name == "fixture bow")
        .unwrap();
    let interactions = state.observation.interactions.as_ref().unwrap();
    let slot = interactions
        .inventory
        .iter()
        .find(|row| row.item == bow.id)
        .unwrap()
        .equipped_slot
        .unwrap();
    assert_eq!(interactions.slots[usize::from(slot)].id, "weapon");
    assert_eq!(
        state
            .observation
            .inventory
            .iter()
            .find(|item| item.name == "fixture bow")
            .unwrap()
            .quantity,
        1
    );
    assert_eq!(
        state
            .observation
            .inventory
            .iter()
            .find(|item| item.name == "compatible arrow")
            .unwrap()
            .quantity,
        6
    );
    assert!(!state
        .observation
        .inventory
        .iter()
        .any(|item| item.name == "incompatible arrow"));
    assert_eq!(
        state
            .observation
            .ground_items
            .iter()
            .find(|item| item.item.name == "compatible arrow")
            .unwrap()
            .item
            .quantity,
        14
    );
    assert_eq!(
        state
            .observation
            .ground_items
            .iter()
            .find(|item| item.item.name == "incompatible arrow")
            .unwrap()
            .item
            .quantity,
        20
    );
    let mut checked_owner = false;
    for _ in 0..8 {
        let (actor, action) = engine.next_ai_action().unwrap();
        if actor == ActorId(1) {
            assert!(
                matches!(action, tor_server::journal::Action::Move { .. }),
                "fresh satisfied owner explores instead of overcollecting"
            );
            checked_owner = true;
            break;
        }
        engine.advance_ai(actor).unwrap();
    }
    assert!(checked_owner);
    let expected: Vec<_> = engine
        .actors()
        .into_iter()
        .map(|id| (id, engine.state(id).unwrap()))
        .collect();
    let next = engine.next_ai_action();
    let accepted = engine.save_status().accepted_sequence;
    engine.flush().unwrap();
    drop(engine);
    let reopened = Engine::open(&save, scenario).unwrap();
    let recovery = reopened.recovery_profile();
    assert_eq!(recovery.checkpoint_sequence, 0);
    assert!(recovery.records_replayed > 0);
    assert_eq!(recovery.records_replayed, recovery.records_loaded);
    let actual: Vec<_> = reopened
        .actors()
        .into_iter()
        .map(|id| (id, reopened.state(id).unwrap()))
        .collect();
    assert_eq!(actual, expected);
    assert_eq!(reopened.next_ai_action(), next);
    assert_eq!(reopened.save_status().accepted_sequence, accepted);
}
