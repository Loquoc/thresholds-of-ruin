use super::*;
use crate::abilities::{OutcomeStatus, ResolvedEffect};
use crate::earned_growth::{FatalOrigin, FutureGrowth, GrowthSource};
fn game() -> Game {
    let mut game = super::tests::game_with_configuration(|rules| {
        rules.growth.reward_base = 100;
        rules.growth.reward_per_relative_tier = 0;
        rules.growth.minimum_reward = 100;
        rules.growth.maximum_reward = 100;
    });
    let mut build = game.creature(ActorId(1)).unwrap().build().clone();
    build
        .set_future_growth(FutureGrowth::Selected {
            source: GrowthSource::Racial {},
        })
        .unwrap();
    game.rebuild_creature(ActorId(1), build).unwrap();
    game
}
fn fatal(owner: ActorId, target: ActorId) -> Outcome {
    Outcome {
        resistance: None,
        actor: owner,
        target,
        ability: None,
        intention: None,
        result: OutcomeStatus::Applied,
        check: None,
        participation: crate::abilities::AttemptedParticipation::target(),
        target_rider_participated: false,
        effects: vec![ResolvedEffect::Damage { amount: 1_000_000 }],
    }
}
#[test]
fn corpse_captures_appearance_before_the_actor_can_change() {
    let mut game = game();
    game.actors.get_mut(&ActorId(2)).unwrap().asset = Some("creature.humanoid.sentinel.red".into());
    let captured_identity = game.actors[&ActorId(2)]
        .combat
        .as_ref()
        .unwrap()
        .identity
        .corpse_identity
        .clone();
    game.apply(fatal(ActorId(1), ActorId(2))).unwrap();
    game.actors
        .get_mut(&ActorId(2))
        .unwrap()
        .combat
        .as_mut()
        .unwrap()
        .identity
        .corpse_identity = "item.corpse.test.changed-persona".parse().unwrap();
    game.actors.get_mut(&ActorId(2)).unwrap().asset =
        Some("creature.humanoid.sentinel.blue".into());
    let (corpse_id, corpse) = game
        .items
        .iter()
        .find(|(_, item)| item.spec.class == crate::ItemClass::Corpse)
        .map(|(id, item)| (*id, item))
        .unwrap();
    assert_eq!(corpse.spec.identity, captured_identity);
    let value = serde_json::to_value(corpse).unwrap();
    assert_eq!(value["corpse"], serde_json::json!({
            "actor": 2, "asset": "creature.humanoid.sentinel.red"
        }), "a corpse needs its captured source identity and appearance, independent of its name or the current actor record");
    game.apply_transfer(corpse_id, corpse_id, 1, ItemLocation::Carried(ActorId(1)))
        .unwrap();
    let inventory = game.observe(ActorId(1)).unwrap().inventory;
    let carried = inventory.iter().find(|item| item.id == corpse_id).unwrap();
    assert_eq!(carried.corpse.as_ref().unwrap().actor, ActorId(2));
    assert_eq!(
        carried.corpse.as_ref().unwrap().asset.as_deref(),
        Some("creature.humanoid.sentinel.red")
    );
    let mut shared = crate::checkpoint::SharedState::default();
    let snapshot = game.checkpoint(&mut shared);
    let shared = serde_json::from_value(serde_json::to_value(shared).unwrap()).unwrap();
    let restored = Game::restore_checkpoint(snapshot, &shared, game.content().clone()).unwrap();
    assert_eq!(
        restored, game,
        "captured appearance survives current checkpoint encoding and restoration"
    );
}
fn growth_obstruction(build: &crate::creatures::CreatureBuild) -> Vec<crate::rules::Modifier> {
    let recovery = &build.ruleset().document().health_recovery;
    let current = build.ledger().total_hd();
    let base_period = recovery.period(current);
    let next = crate::HdCount(current.get().checked_add(1).unwrap());
    assert!(recovery.period(next) < base_period);
    // The current raw fold is -NUMERIC, then the recovery rule floors it to 1.
    // The next HD lowers the base and crosses the retained bounded-stat limit.
    vec![
        crate::test_creatures::add(
            crate::rules::Stat::HealthRecoveryPeriod,
            -crate::limits::NUMERIC,
        ),
        crate::test_creatures::add(
            crate::rules::Stat::HealthRecoveryPeriod,
            -i64::try_from(base_period).unwrap(),
        ),
    ]
}
fn equip_growth_obstruction(game: &mut Game, modifiers: Vec<crate::rules::Modifier>) {
    let mut owner = game.actors.get_mut(&ActorId(1)).unwrap();
    owner
        .combat
        .as_mut()
        .unwrap()
        .set_equipment(vec![crate::creatures::EquipmentSource {
            weapon_profile: None,
            item: crate::ItemId(1),
            socket: crate::EquipmentSlotId(0),
            attack: None,
            modifiers,
        }])
        .unwrap();
}
fn assert_fatal_award_retained_without_growth(game: &mut Game) {
    let owner = game.creature(ActorId(1)).unwrap();
    let build = owner.build().clone();
    let epoch = owner.build_epoch();
    let health = owner.health();
    game.apply(fatal(ActorId(1), ActorId(2))).unwrap();
    assert!(!game.alive(ActorId(2)));
    let owner = game.creature(ActorId(1)).unwrap();
    assert_eq!(owner.build(), &build);
    assert_eq!(owner.build_epoch(), epoch);
    assert_eq!(owner.health(), health);
    assert_eq!(owner.earned_progress().earned_xp(), 100);
    assert_eq!(owner.earned_progress().held_xp(), 100);
    assert_eq!(owner.earned_progress().spent_xp(), 0);
    assert_eq!(owner.earned_progress().highest_earned_tier().get(), 2);
}
#[test]
fn temporary_derived_limit_retains_fatal_xp_without_vetoing_death() {
    let mut game = game();
    let modifiers = growth_obstruction(game.creature(ActorId(1)).unwrap().build());
    equip_growth_obstruction(&mut game, modifiers);
    assert_eq!(
        game.creature(ActorId(1))
            .unwrap()
            .derived()
            .health_recovery_period,
        1
    );
    assert_fatal_award_retained_without_growth(&mut game);
}
#[test]
fn removing_a_temporary_growth_obstruction_applies_retained_xp_once() {
    let mut game = game();
    let original = game.creature(ActorId(1)).unwrap().build().clone();
    let modifiers = growth_obstruction(game.creature(ActorId(1)).unwrap().build());
    equip_growth_obstruction(&mut game, modifiers);
    assert_fatal_award_retained_without_growth(&mut game);
    let mut outcome = game.actor_outcome(ActorId(1)).unwrap();
    outcome
        .state
        .combat
        .as_mut()
        .unwrap()
        .set_equipment(vec![])
        .unwrap();
    game.apply_outcome(outcome).unwrap();
    let mut expected = original;
    expected
        .add_hit_die(crate::progression::HdSource::Racial, 7)
        .unwrap();
    let owner = game.creature(ActorId(1)).unwrap();
    assert_eq!(owner.build(), &expected);
    assert_eq!(owner.earned_progress().held_xp(), 0);
    assert_eq!(owner.earned_progress().spent_xp(), 100);
    let epoch = owner.build_epoch();
    let outcome = game.actor_outcome(ActorId(1)).unwrap();
    game.apply_outcome(outcome).unwrap();
    assert_eq!(game.creature(ActorId(1)).unwrap().build_epoch(), epoch);
    assert_eq!(
        game.creature(ActorId(1))
            .unwrap()
            .earned_progress()
            .spent_xp(),
        100
    );
}
#[test]
fn condition_expiry_retries_eligible_retained_growth_only_at_authoritative_publication() {
    let mut game = game();
    let condition = crate::content_ids::ConditionId::new("readied").unwrap();
    game.apply_condition(ActorId(1), &condition, ActorId(1), 10)
        .unwrap();
    // A strictly restorable retained balance is deliberately pending: restore
    // does not silently spend it or invent a separate catch-up timer.
    let mut owner = game.actors.get_mut(&ActorId(1)).unwrap();
    let creature = owner.combat.as_ref().unwrap().creature().clone();
    let mut value =
        serde_json::to_value(crate::creatures::record::CreatureRecord::capture(&creature)).unwrap();
    value["earned_progress"]["earned_xp"] = serde_json::json!(100);
    value["earned_progress"]["held_xp"] = serde_json::json!(100);
    value["earned_progress"]["highest_earned_tier"] = serde_json::json!(2);
    let record: crate::creatures::record::CreatureRecord = serde_json::from_value(value).unwrap();
    let restored = record
        .restore(creature.build().content().clone(), vec![])
        .unwrap();
    let identity = owner.combat.as_ref().unwrap().identity();
    owner.combat = Some(crate::combat::CombatState::from_creature(
        identity, restored,
    ));
    drop(owner);
    game.tick = 10;
    let before_read = game.clone();
    let view = game.creature(ActorId(1)).unwrap();
    assert!(view.conditions().is_empty());
    assert_eq!(view.build().ledger().total_hd().get(), 1);
    assert_eq!(view.earned_progress().held_xp(), 100);
    assert_eq!(
        game, before_read,
        "settled presentation cannot publish an HD or spend XP"
    );
    game.settle_actor_time(ActorId(1)).unwrap();
    let owner = game.creature(ActorId(1)).unwrap();
    assert!(owner.conditions().is_empty());
    assert_eq!(owner.build().ledger().total_hd().get(), 2);
    assert_eq!(owner.earned_progress().held_xp(), 0);
    assert_eq!(owner.earned_progress().spent_xp(), 100);
    assert_eq!(owner.derived(), &owner.build().derive().unwrap());
    let settled = game.clone();
    game.settle_actor_time(ActorId(1)).unwrap();
    assert_eq!(
        game, settled,
        "the same expired condition cannot spend the threshold again"
    );
}
#[test]
fn unchanged_damage_timer_and_read_paths_do_not_retry_growth_or_derive() {
    for pending in [false, true] {
        let mut game = game();
        game.apply_condition(
            ActorId(1),
            &crate::content_ids::ConditionId::new("readied").unwrap(),
            ActorId(1),
            1000,
        )
        .unwrap();
        if pending {
            let modifiers = growth_obstruction(game.creature(ActorId(1)).unwrap().build());
            equip_growth_obstruction(&mut game, modifiers);
            assert_fatal_award_retained_without_growth(&mut game);
        }
        // A publication with no creature change must not detach the payload,
        // even when earned XP is held behind an unchanged obstruction.
        let prior = game.actors[&ActorId(1)].combat.as_ref().unwrap().clone();
        let outcome = game.actor_outcome(ActorId(1)).unwrap();
        game.commit_actor_outcome(outcome).unwrap();
        assert!(std::ptr::eq(
            prior.creature(),
            game.actors[&ActorId(1)].combat.as_ref().unwrap().creature(),
        ));
        let before = game.clone();
        let folds = crate::diagnostics::work_counts().health_derivations;
        for _ in 0..3 {
            let _view = game.inspect_creature(ActorId(1)).unwrap();
        }
        assert_eq!(game, before);
        game.apply(Outcome {
            resistance: None,
            actor: ActorId(1),
            target: ActorId(1),
            ability: None,
            intention: None,
            result: OutcomeStatus::Applied,
            check: None,
            participation: crate::abilities::AttemptedParticipation::target(),
            target_rider_participated: false,
            effects: vec![ResolvedEffect::Damage { amount: 1 }],
        })
        .unwrap();
        game.tick = 1;
        game.settle_actor_time(ActorId(1)).unwrap();
        let _view = game.inspect_creature(ActorId(1)).unwrap();
        assert_eq!(
            crate::diagnostics::work_counts().health_derivations,
            folds,
            "pending={pending}"
        );
        assert_eq!(
            game.creature(ActorId(1))
                .unwrap()
                .build()
                .ledger()
                .total_hd()
                .get(),
            1
        );
        assert_eq!(
            game.creature(ActorId(1))
                .unwrap()
                .earned_progress()
                .held_xp(),
            if pending { 100 } else { 0 }
        );
    }
}
#[test]
fn temporary_override_conflict_retains_fatal_xp_without_vetoing_death() {
    let mut game = super::tests::game_with_configuration(|rules| {
        rules.growth.reward_base = 100;
        rules.growth.reward_per_relative_tier = 0;
        rules.growth.minimum_reward = 100;
        rules.growth.maximum_reward = 100;
        rules
            .classes
            .iter_mut()
            .find(|class| class.id.as_str() == "mage")
            .unwrap()
            .modifiers
            .push(crate::rules::Modifier {
                cancellable: false,
                scope: crate::rules::Scope::Always,
                op: crate::rules::ModifierOp::Set {
                    stat: crate::rules::Stat::Health,
                    value: 20,
                },
            });
    });
    let mut build = game.creature(ActorId(1)).unwrap().build().clone();
    let class = crate::content_ids::ClassId::new("mage").unwrap();
    build
        .set_future_growth(FutureGrowth::Selected {
            source: GrowthSource::Class {
                class: class.clone(),
            },
        })
        .unwrap();
    game.rebuild_creature(ActorId(1), build.clone()).unwrap();
    build
        .add_hit_die(crate::progression::HdSource::Class(class), 7)
        .unwrap();
    assert_eq!(
        build.derive().unwrap().maximum_health,
        20,
        "the permanent next build is independently valid"
    );
    equip_growth_obstruction(
        &mut game,
        vec![crate::rules::Modifier {
            cancellable: false,
            scope: crate::rules::Scope::Always,
            op: crate::rules::ModifierOp::Set {
                stat: crate::rules::Stat::Health,
                value: 30,
            },
        }],
    );
    assert_eq!(
        game.creature(ActorId(1)).unwrap().derived().maximum_health,
        30
    );
    assert_fatal_award_retained_without_growth(&mut game);
}
#[test]
fn recorded_only_owner_retains_fatal_xp_without_inventing_a_health_root() {
    let mut game = game();
    let original = game.creature(ActorId(1)).unwrap().build().clone();
    let mut build = crate::creatures::CreatureBuild::from_recorded(
        original.content().clone(),
        original.species_id().clone(),
        original.initial_attributes(),
        crate::progression::HdLedger::new(original.ledger().entries().to_vec()).unwrap(),
        original.binding(),
        original.advancement().clone(),
        original.template_ids().to_vec(),
    )
    .unwrap();
    build
        .set_future_growth(FutureGrowth::Selected {
            source: GrowthSource::Racial {},
        })
        .unwrap();
    game.rebuild_creature(ActorId(1), build).unwrap();
    assert_eq!(
        game.creature(ActorId(1))
            .unwrap()
            .build()
            .ledger()
            .health_root_seed(),
        None
    );
    assert_fatal_award_retained_without_growth(&mut game);
}
#[test]
fn late_corpse_allocation_failure_rolls_back_fatal_damage_and_xp() {
    let mut game = game();
    game.next_item_id = u64::MAX;
    let before = game.clone();
    assert_eq!(
        game.apply(fatal(ActorId(1), ActorId(2))),
        Err(GameError::IdentityExhausted)
    );
    assert_eq!(game, before);
}
#[test]
fn typed_posthumous_cause_retains_xp_without_resurrecting_its_dead_owner() {
    let mut game = game();
    game.apply(fatal(ActorId(1), ActorId(1))).unwrap();
    let dead = game.creature(ActorId(1)).unwrap().health();
    let hd = game
        .creature(ActorId(1))
        .unwrap()
        .build()
        .ledger()
        .total_hd();
    game.apply_resolved_outcome_with_origin(
        fatal(ActorId(1), ActorId(2)),
        ResolutionPublication::Standalone,
        &mut DamageActivityGuard::default(),
        Some(FatalOrigin::Condition {
            condition: crate::content_ids::ConditionId::new("readied").unwrap(),
            application: 1,
        }),
    )
    .unwrap();
    let owner = game.creature(ActorId(1)).unwrap();
    assert_eq!(owner.health(), dead);
    assert!(owner.health().dead());
    assert_eq!(owner.build().ledger().total_hd(), hd);
    assert_eq!(owner.earned_progress().held_xp(), 100);
    assert_eq!(owner.earned_progress().spent_xp(), 0);
}
#[test]
fn xp_overflow_preflight_rolls_back_the_whole_death_transition() {
    for kind in [
        "selected",
        "disabled",
        "dead",
        "maximum",
        "exhausted_authored",
    ] {
        let mut game = game();
        let mut build = game.creature(ActorId(1)).unwrap().build().clone();
        match kind {
            "disabled" => build.set_future_growth(FutureGrowth::Disabled {}).unwrap(),
            "maximum" => {
                for _ in 1..crate::limits::HIT_DICE {
                    build
                        .add_hit_die(crate::progression::HdSource::Racial, 7)
                        .unwrap();
                }
            }
            "exhausted_authored" => {
                let mut advancement = build.advancement().clone();
                advancement.restoration = crate::creatures::RestorationPolicy::Authored(vec![
                    crate::creatures::PlannedAdvancement {
                        source: crate::progression::HdSource::Racial,
                        choices: build.choices()[0].clone(),
                    },
                ]);
                build = crate::creatures::CreatureBuild::from_recorded(
                    build.content().clone(),
                    build.species_id().clone(),
                    build.initial_attributes(),
                    build.ledger().clone(),
                    build.binding(),
                    advancement,
                    build.template_ids().to_vec(),
                )
                .unwrap();
                build.set_future_growth(FutureGrowth::Authored {}).unwrap();
            }
            _ => {}
        }
        game.rebuild_creature(ActorId(1), build).unwrap();
        if kind == "dead" {
            game.apply(fatal(ActorId(1), ActorId(1))).unwrap();
        }
        let mut state = game.actors.get_mut(&ActorId(1)).unwrap();
        let creature = state.combat.as_ref().unwrap().creature().clone();
        let mut record =
            serde_json::to_value(crate::creatures::record::CreatureRecord::capture(&creature))
                .unwrap();
        record["earned_progress"]["earned_xp"] = serde_json::json!(u64::MAX);
        record["earned_progress"]["held_xp"] = serde_json::json!(u64::MAX);
        let policy = &creature.build().ruleset().document().growth;
        assert_eq!(
            (policy.threshold_base, policy.threshold_per_tier),
            (100, 100)
        );
        let applied = creature.earned_progress().highest_applied_tier();
        assert_eq!(creature.build().ledger().total_hd(), applied);
        // Independently invert this fixture's 100*tier triangular prices.
        // MAX XP funds the largest end with end*(end-1) no greater than
        // applied*(applied-1) + 2*floor(MAX/100).
        let applied = u128::from(applied.get());
        let triangle = applied * (applied - 1) + 2 * u128::from(u64::MAX / 100);
        let funded = u64::try_from((1 + 4 * triangle).isqrt().div_ceil(2)).unwrap();
        record["earned_progress"]["highest_earned_tier"] = serde_json::json!(funded);
        let record: crate::creatures::record::CreatureRecord =
            serde_json::from_value(record).unwrap();
        let restored = record
            .restore(creature.build().content().clone(), vec![])
            .unwrap();
        let identity = state.combat.as_ref().unwrap().identity();
        state.combat = Some(crate::combat::CombatState::from_creature(
            identity, restored,
        ));
        drop(state);
        let before = game.clone();
        assert_eq!(
            game.apply(fatal(ActorId(1), ActorId(2))),
            Err(GameError::Creature(
                crate::creatures::CreatureStateError::Growth(
                    crate::earned_growth::GrowthError::Arithmetic
                )
            ))
        );
        assert_eq!(game, before, "{kind}");
    }
}
#[test]
fn a_dead_target_cannot_reassign_credit_to_a_later_effect_owner() {
    let mut game = game();
    game.apply(fatal(ActorId(1), ActorId(2))).unwrap();
    let before = game.creature(ActorId(1)).unwrap().earned_progress().clone();
    game.apply(fatal(ActorId(2), ActorId(2))).unwrap();
    assert_eq!(
        game.creature(ActorId(1)).unwrap().earned_progress(),
        &before
    );
    assert_eq!(
        game.creature(ActorId(2))
            .unwrap()
            .earned_progress()
            .earned_xp(),
        0
    );
}

fn obstructed_growth_selection_work(game: &Game) -> usize {
    let owner = ActorId(1);
    let basis = crate::player_advancement::AdvancementBasis {
        actor: owner,
        branch: "main".into(),
        stream: "owner".into(),
        reset: 0,
        build_epoch: game.creature(owner).unwrap().build_epoch(),
    };
    let before = game.clone();
    let folds = crate::diagnostics::work_counts().health_derivations;
    let preview = game
        .preview_player_growth_source(owner, &basis, &GrowthSource::Racial {})
        .unwrap();
    let work = crate::diagnostics::work_counts().health_derivations - folds;
    assert_eq!(game, &before, "preview must remain pure");
    assert_eq!(preview.build().ledger().total_hd().get(), 1);
    assert_eq!(preview.runtime().health_recovery.period(), 1);
    assert_eq!(
        preview.build().future_growth(),
        &FutureGrowth::Selected {
            source: GrowthSource::Racial {},
        }
    );
    work
}

fn configured(capabilities: crate::floor_activation::ScenarioCapabilities) -> Game {
    let mut game = game();
    game.configure_scenario_policy(capabilities, None).unwrap();
    game
}

#[test]
fn disabled_corpses_leave_no_body_item_but_still_kill_and_award() {
    let mut game = configured(crate::floor_activation::ScenarioCapabilities {
        corpses: false,
        ..Default::default()
    });
    let next = game.next_item_id;
    let before = game
        .creature(ActorId(1))
        .unwrap()
        .earned_progress()
        .earned_xp();
    game.apply(fatal(ActorId(1), ActorId(2))).unwrap();
    assert!(game.creature(ActorId(2)).unwrap().health().dead());
    assert!(game
        .items
        .iter()
        .all(|(_, item)| item.spec.class != crate::ItemClass::Corpse));
    assert_eq!(game.next_item_id, next, "no item identity is allocated");
    assert!(
        game.creature(ActorId(1))
            .unwrap()
            .earned_progress()
            .earned_xp()
            > before
    );

    let mut ordinary = game_for_corpse_comparison();
    ordinary.apply(fatal(ActorId(1), ActorId(2))).unwrap();
    assert!(ordinary
        .items
        .iter()
        .any(|(_, item)| item.spec.class == crate::ItemClass::Corpse));
}

fn game_for_corpse_comparison() -> Game {
    configured(Default::default())
}

#[test]
fn locked_owner_advancement_refuses_previews_and_actions_without_changes() {
    let mut game = configured(crate::floor_activation::ScenarioCapabilities {
        owner_advancement: false,
        ..Default::default()
    });
    let owner = ActorId(1);
    let basis = crate::player_advancement::AdvancementBasis {
        actor: owner,
        branch: "main".into(),
        stream: "owner".into(),
        reset: 0,
        build_epoch: game.creature(owner).unwrap().build_epoch(),
    };
    assert_eq!(
        game.preview_player_growth_source(owner, &basis, &GrowthSource::Racial {})
            .map(|_| ()),
        Err(GameError::AdvancementLocked)
    );
    let before = game.clone();
    assert_eq!(
        game.act(
            owner,
            crate::Action::ChooseGrowthSource {
                basis,
                source: GrowthSource::Racial {},
            },
        )
        .map(|_| ()),
        Err(GameError::AdvancementLocked)
    );
    assert_eq!(game, before);
    // The default policy reaches ordinary validation instead of the lock.
    let ordinary = configured(Default::default()).preview_player_growth_source(
        owner,
        &crate::player_advancement::AdvancementBasis {
            actor: owner,
            branch: "main".into(),
            stream: "owner".into(),
            reset: 0,
            build_epoch: before.creature(owner).unwrap().build_epoch(),
        },
        &GrowthSource::Racial {},
    );
    assert!(!matches!(ordinary, Err(GameError::AdvancementLocked)));
}

#[test]
fn choosing_future_source_attempts_obstructed_growth_only_once_per_fold() {
    let mut pending = game();
    let mut build = pending.creature(ActorId(1)).unwrap().build().clone();
    build.set_future_growth(FutureGrowth::Disabled {}).unwrap();
    pending.rebuild_creature(ActorId(1), build).unwrap();
    let modifiers = growth_obstruction(pending.creature(ActorId(1)).unwrap().build());
    equip_growth_obstruction(&mut pending, modifiers);
    let no_xp = pending.clone();
    pending.apply(fatal(ActorId(1), ActorId(2))).unwrap();
    assert!(!pending.alive(ActorId(2)));
    assert_eq!(
        pending
            .creature(ActorId(1))
            .unwrap()
            .earned_progress()
            .held_xp(),
        100
    );
    assert_eq!(
        pending
            .creature(ActorId(1))
            .unwrap()
            .earned_progress()
            .spent_xp(),
        0
    );
    let baseline = obstructed_growth_selection_work(&no_xp);
    let actual = obstructed_growth_selection_work(&pending);
    assert_eq!(
        actual,
        baseline + 4,
        "one permanent HD2 fold and one rejected runtime HD2 fold, with no second attempt"
    );
}

fn obstructed_equipment_reconciliation_work(
    mut game: Game,
    modifiers: Vec<crate::rules::Modifier>,
) -> usize {
    let owner = ActorId(1);
    let before = game.creature(owner).unwrap().clone().into_owned();
    let mut outcome = game.actor_outcome(owner).unwrap();
    let folds = crate::diagnostics::work_counts().health_derivations;
    outcome
        .state
        .combat
        .as_mut()
        .unwrap()
        .set_equipment(vec![crate::creatures::EquipmentSource {
            weapon_profile: None,
            item: crate::ItemId(2),
            socket: crate::EquipmentSlotId(0),
            attack: None,
            modifiers,
        }])
        .unwrap();
    game.apply_outcome(outcome).unwrap();
    let work = crate::diagnostics::work_counts().health_derivations - folds;
    let after = game.creature(owner).unwrap();
    assert_eq!(after.build(), before.build());
    assert_eq!(after.build_epoch(), before.build_epoch());
    assert_eq!(after.health(), before.health());
    assert_eq!(after.earned_progress(), before.earned_progress());
    assert_eq!(after.build().ledger().total_hd().get(), 1);
    work
}

#[test]
fn changed_equipment_attempts_obstructed_growth_only_once_per_fold() {
    let mut pending = game();
    let modifiers = growth_obstruction(pending.creature(ActorId(1)).unwrap().build());
    equip_growth_obstruction(&mut pending, modifiers.clone());
    let no_xp = pending.clone();
    assert_fatal_award_retained_without_growth(&mut pending);
    let baseline = obstructed_equipment_reconciliation_work(no_xp, modifiers.clone());
    let actual = obstructed_equipment_reconciliation_work(pending, modifiers);
    assert_eq!(
        actual,
        baseline + 4,
        "one permanent HD2 fold and one rejected runtime HD2 fold, with no second attempt"
    );
}
fn timed_growth_obstruction_game(obstruct: bool) -> (Game, crate::content_ids::ConditionId) {
    let modifiers = if obstruct {
        growth_obstruction(game().creature(ActorId(1)).unwrap().build())
    } else {
        vec![crate::test_creatures::add(
            crate::rules::Stat::HealthRecoveryPeriod,
            1,
        )]
    };
    let condition = crate::content_ids::ConditionId::new("growth_health_buffer").unwrap();
    let mut game = super::tests::game_with_configuration(|rules| {
        rules.growth.reward_base = 100;
        rules.growth.reward_per_relative_tier = 0;
        rules.growth.minimum_reward = 100;
        rules.growth.maximum_reward = 100;
        rules.conditions.push(crate::rules::ConditionDef {
            detecting_monsters: false,
            hallucinating: false,
            id: condition.clone(),
            family: crate::content_ids::ConditionFamilyId::new("growth_health_buffer").unwrap(),
            lifecycle: crate::rules::ConditionLifecycle::Timed,
            timing: tor_simulation::rules::ConditionTiming::expiring(),
            restrictions: vec![],
            preparation: tor_simulation::rules::ConditionPreparation::Preserve,
            name: "Temporary growth health buffer".into(),
            modifiers,
            stacking: crate::rules::Stacking::Longest,
            source_scoped: false,
            descriptors: vec![],
            capacity: crate::rules::Eviction::Oldest,
        });
    });
    let mut build = game.creature(ActorId(1)).unwrap().build().clone();
    build
        .set_future_growth(FutureGrowth::Selected {
            source: GrowthSource::Racial {},
        })
        .unwrap();
    game.rebuild_creature(ActorId(1), build).unwrap();
    game.apply_condition(ActorId(1), &condition, ActorId(1), 10)
        .unwrap();
    (game, condition)
}

#[test]
fn automatic_growth_retains_valid_timed_condition_then_retries_once_at_expiry() {
    let (mut game, condition) = timed_growth_obstruction_game(true);
    let owner = ActorId(1);
    let before = game.creature(owner).unwrap().into_owned();
    assert_eq!(before.derived().health_recovery_period, 1);
    assert_eq!(before.conditions().remaining(&condition, owner), Some(10));
    let mut expected = before.build().clone();
    expected
        .add_hit_die(
            crate::progression::HdSource::Racial,
            expected.ledger().health_root_seed().unwrap(),
        )
        .unwrap();
    assert!(
        expected.derive().unwrap().maximum_health > before.build().derive().unwrap().maximum_health,
        "the permanent next HD is independently valid and increases Health"
    );

    game.apply(fatal(owner, ActorId(2))).unwrap();
    assert!(
        !game.alive(ActorId(2)),
        "temporary obstruction cannot veto death"
    );
    let retained = game.creature(owner).unwrap();
    assert_eq!(
        retained.conditions(),
        before.conditions(),
        "automatic growth must not evict a valid timed condition to fit its next HD"
    );
    assert_eq!(retained.build(), before.build());
    assert_eq!(retained.build_epoch(), before.build_epoch());
    assert_eq!(retained.health(), before.health());
    assert_eq!(retained.costs(), before.costs());
    assert_eq!(retained.health_recovery(), before.health_recovery());
    assert_eq!(retained.earned_progress().earned_xp(), 100);
    assert_eq!(retained.earned_progress().held_xp(), 100);
    assert_eq!(retained.earned_progress().spent_xp(), 0);
    assert_eq!(retained.earned_progress().highest_earned_tier().get(), 2);
    drop(retained);

    game.tick = 9;
    let folds = crate::diagnostics::work_counts().health_derivations;
    game.settle_actor_time(owner).unwrap();
    assert_eq!(
        crate::diagnostics::work_counts().health_derivations,
        folds,
        "unchanged timed source cannot retry the same growth obstruction"
    );
    assert_eq!(
        game.creature(owner)
            .unwrap()
            .conditions()
            .remaining(&condition, owner),
        Some(1)
    );
    game.tick = 10;
    let before_read = game.clone();
    let projected = game.creature(owner).unwrap();
    assert!(projected.conditions().is_empty());
    assert_eq!(projected.build().ledger().total_hd().get(), 1);
    assert_eq!(projected.earned_progress().held_xp(), 100);
    drop(projected);
    assert_eq!(
        game, before_read,
        "read-only expiry projection cannot publish growth"
    );
    game.settle_actor_time(owner).unwrap();
    let grown = game.creature(owner).unwrap();
    assert!(grown.conditions().is_empty());
    assert_eq!(grown.build(), &expected);
    assert_eq!(grown.earned_progress().earned_xp(), 100);
    assert_eq!(grown.earned_progress().spent_xp(), 100);
    assert_eq!(grown.earned_progress().held_xp(), 0);
    drop(grown);
    let once = game.clone();
    game.settle_actor_time(owner).unwrap();
    assert_eq!(game, once, "same expiry must not publish or spend twice");
}

#[test]
fn compatible_timed_condition_does_not_defer_automatic_growth() {
    let (mut game, condition) = timed_growth_obstruction_game(false);
    let owner = ActorId(1);
    let conditions = game.creature(owner).unwrap().conditions().clone();
    game.apply(fatal(owner, ActorId(2))).unwrap();
    assert!(!game.alive(ActorId(2)));
    let grown = game.creature(owner).unwrap();
    assert_eq!(grown.conditions(), &conditions);
    assert_eq!(grown.conditions().remaining(&condition, owner), Some(10));
    assert_eq!(grown.build().ledger().total_hd().get(), 2);
    assert_eq!(grown.earned_progress().earned_xp(), 100);
    assert_eq!(grown.earned_progress().spent_xp(), 100);
    assert_eq!(grown.earned_progress().held_xp(), 0);
}
#[test]
fn explicit_rebuild_keeps_its_existing_timed_condition_eviction_semantics() {
    let (mut game, condition) = timed_growth_obstruction_game(true);
    let owner = ActorId(1);
    let mut next = game.creature(owner).unwrap().build().clone();
    next.add_hit_die(
        crate::progression::HdSource::Racial,
        next.ledger().health_root_seed().unwrap(),
    )
    .unwrap();
    let outcome = game.rebuild_creature(owner, next.clone()).unwrap();
    let creature = game.creature(owner).unwrap();
    assert_eq!(creature.build(), &next);
    assert_eq!(creature.conditions().remaining(&condition, owner), None);
    assert!(
        outcome.conditions_cleared,
        "Game returns final explicit rebuild consequences"
    );
    assert!(!outcome.died);
    assert_eq!(creature.earned_progress().earned_xp(), 0);
}

#[test]
fn standalone_domain_equipment_and_source_setters_still_apply_held_growth_immediately() {
    let mut pending = game();
    let modifiers = growth_obstruction(pending.creature(ActorId(1)).unwrap().build());
    equip_growth_obstruction(&mut pending, modifiers);
    assert_fatal_award_retained_without_growth(&mut pending);
    let mut creature = pending.creature(ActorId(1)).unwrap().into_owned();
    creature.set_equipment(vec![]).unwrap();
    assert_eq!(creature.build().ledger().total_hd().get(), 2);
    assert_eq!(creature.earned_progress().spent_xp(), 100);

    let mut creature = game().creature(ActorId(1)).unwrap().into_owned();
    let mut disabled = creature.build().clone();
    disabled
        .set_future_growth(FutureGrowth::Disabled {})
        .unwrap();
    creature.rebuild(disabled).unwrap();
    creature.award_xp(100).unwrap();
    assert_eq!(creature.build().ledger().total_hd().get(), 1);
    let mut selected = creature.build().clone();
    selected
        .set_future_growth(FutureGrowth::Selected {
            source: GrowthSource::Racial {},
        })
        .unwrap();
    creature.rebuild(selected).unwrap();
    assert_eq!(creature.build().ledger().total_hd().get(), 2);
    assert_eq!(creature.earned_progress().spent_xp(), 100);
}

#[test]
fn automatic_growth_still_removes_a_genuinely_newly_immune_timed_condition() {
    let obstruction = growth_obstruction(game().creature(ActorId(1)).unwrap().build());
    let condition = crate::content_ids::ConditionId::new("growth_immune_buffer").unwrap();
    let class = crate::content_ids::ClassId::new("mage").unwrap();
    let mut game = super::tests::game_with_configuration(|rules| {
        rules.growth.reward_base = 100;
        rules.growth.reward_per_relative_tier = 0;
        rules.growth.minimum_reward = 100;
        rules.growth.maximum_reward = 100;
        let mut buffer = rules
            .conditions
            .iter()
            .find(|def| def.id.as_str() == "fear")
            .unwrap()
            .clone();
        assert!(!buffer.descriptors.is_empty());
        let modifiers: Vec<_> = buffer
            .descriptors
            .iter()
            .cloned()
            .map(|descriptor| crate::rules::Modifier {
                cancellable: false,
                scope: crate::rules::Scope::Always,
                op: crate::rules::ModifierOp::Immunity {
                    selector: crate::rules::Selector::Descriptor { descriptor },
                },
            })
            .collect();
        rules
            .classes
            .iter_mut()
            .find(|def| def.id == class)
            .unwrap()
            .modifiers
            .extend(modifiers);
        buffer.id = condition.clone();
        buffer.family = crate::content_ids::ConditionFamilyId::new("growth_immune_buffer").unwrap();
        buffer.lifecycle = crate::rules::ConditionLifecycle::Timed;
        buffer.modifiers = obstruction;
        rules.conditions.push(buffer);
    });
    let owner = ActorId(1);
    let mut build = game.creature(owner).unwrap().build().clone();
    build
        .set_future_growth(FutureGrowth::Selected {
            source: GrowthSource::Class { class },
        })
        .unwrap();
    game.rebuild_creature(owner, build).unwrap();
    game.apply_condition(owner, &condition, owner, 10).unwrap();
    assert_eq!(
        game.creature(owner)
            .unwrap()
            .derived()
            .health_recovery_period,
        1
    );
    game.apply(fatal(owner, ActorId(2))).unwrap();
    let creature = game.creature(owner).unwrap();
    assert!(!game.alive(ActorId(2)));
    assert_eq!(creature.build().ledger().total_hd().get(), 2);
    assert_eq!(creature.earned_progress().spent_xp(), 100);
    assert_eq!(creature.earned_progress().held_xp(), 0);
    assert_eq!(creature.conditions().remaining(&condition, owner), None);
}
