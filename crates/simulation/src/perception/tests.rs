use super::*;
use crate::rules::{Modifier, ModifierOp, Scope};

fn modifier(value: PerceptionValue) -> Modifier {
    Modifier {
        cancellable: false,
        scope: Scope::Always,
        op: ModifierOp::Perception { value },
    }
}

fn guise(name: &str) -> GuiseAppearance {
    GuiseAppearance {
        name: name.into(),
        description: "An ordinary closed chest.".into(),
        class: crate::ItemClass::Tool,
        asset: None,
    }
}

#[test]
fn capability_fold_is_order_independent_and_suppression_wins() {
    let mut sources = vec![
        modifier(PerceptionValue::Invisible { enabled: true }),
        modifier(PerceptionValue::SeeInvisible { enabled: true }),
        modifier(PerceptionValue::ObjectGuise {
            skill: crate::attributes::Skill::Stealth,
            edge: 0,
            appearance: Some(guise("chest")),
        }),
        modifier(PerceptionValue::AmbushFromGuise { enabled: true }),
    ];
    let folded = PerceptionCapabilities::from_modifiers(sources.iter()).unwrap();
    assert!(folded.invisible && folded.see_invisible && folded.ambush_from_guise);
    assert!(folded.sight);
    assert_eq!(folded.object_guise, Some(guise("chest")));
    sources.reverse();
    assert_eq!(
        PerceptionCapabilities::from_modifiers(sources.iter()).unwrap(),
        folded
    );
    sources.extend([
        modifier(PerceptionValue::Invisible { enabled: false }),
        modifier(PerceptionValue::SeeInvisible { enabled: false }),
        modifier(PerceptionValue::Sight { enabled: false }),
        modifier(PerceptionValue::ObjectGuise {
            skill: crate::attributes::Skill::Stealth,
            edge: 0,
            appearance: None,
        }),
        modifier(PerceptionValue::AmbushFromGuise { enabled: false }),
    ]);
    let folded = PerceptionCapabilities::from_modifiers(sources.iter()).unwrap();
    assert!(!folded.invisible && !folded.see_invisible && !folded.sight);
    assert!(!folded.ambush_from_guise && folded.object_guise.is_none());
}

#[test]
fn contradictory_authored_guises_fail_in_either_source_order() {
    let mut sources = [
        modifier(PerceptionValue::ObjectGuise {
            skill: crate::attributes::Skill::Stealth,
            edge: 0,
            appearance: Some(guise("chest")),
        }),
        modifier(PerceptionValue::ObjectGuise {
            skill: crate::attributes::Skill::Stealth,
            edge: 0,
            appearance: Some(guise("urn")),
        }),
    ];
    assert!(PerceptionCapabilities::from_modifiers(sources.iter()).is_err());
    sources.reverse();
    assert!(PerceptionCapabilities::from_modifiers(sources.iter()).is_err());
}

#[test]
fn perception_source_records_reject_positional_extra_and_missing_nullable_fields() {
    for invalid in [
        serde_json::json!(["invisible", true]),
        serde_json::json!({"kind":"invisible", "enabled":true, "secret":17}),
        serde_json::json!({"kind":"object_guise"}),
        serde_json::json!({"kind":"object_guise", "appearance":{"name":"", "description":"", "class":"tool", "asset":null}}),
        serde_json::json!({"kind":"object_guise", "appearance":{"name":"chest", "description":"object", "class":"tool"}}),
    ] {
        assert!(serde_json::from_value::<PerceptionValue>(invalid).is_err());
    }
    let suppression = PerceptionValue::ObjectGuise {
        skill: crate::attributes::Skill::Stealth,
        edge: 0,
        appearance: None,
    };
    assert_eq!(
        serde_json::from_value::<PerceptionValue>(serde_json::to_value(&suppression).unwrap())
            .unwrap(),
        suppression
    );
    let mut invalid = modifier(PerceptionValue::Invisible { enabled: true });
    invalid.scope = Scope::VersusSource;
    assert!(crate::rules::validate_modifier_shapes(&[invalid]).is_err());
}

#[test]
fn guise_enters_once_and_source_loss_never_rearms_it() {
    let source = PerceptionCapabilities {
        object_guise: Some(guise("chest")),
        ambush_from_guise: true,
        ..Default::default()
    };
    let mut state = GuiseState::default();
    state.reconcile(&source, false);
    assert_eq!(state.appearance(), Some(&guise("chest")));
    assert!(state.ambush_available());
    state.reveal();
    assert!(state.appearance().is_none());
    state.reconcile(&PerceptionCapabilities::default(), false);
    state.reconcile(&source, false);
    assert!(state.appearance().is_none());
    assert!(!state.ambush_available());
}

#[test]
fn ordinary_later_ambush_grant_can_arm_only_the_unbroken_initial_guise() {
    let mut source = PerceptionCapabilities {
        object_guise: Some(guise("chest")),
        ..Default::default()
    };
    let mut state = GuiseState::default();
    state.reconcile(&source, false);
    assert!(!state.ambush_available());
    source.ambush_from_guise = true;
    state.reconcile(&source, false);
    assert!(state.ambush_available());
    source.ambush_from_guise = false;
    state.reconcile(&source, false);
    source.ambush_from_guise = true;
    state.reconcile(&source, false);
    assert!(!state.ambush_available());
}

#[test]
fn continual_stealth_and_aggravation_are_nonstacking_and_skill_specific() {
    let stealth = modifier(PerceptionValue::ContinualStealth { edge: 1 });
    let aggravation = modifier(PerceptionValue::Aggravation {
        interval: 500,
        chance_denominator: 3,
        stealth_disadvantage: 1,
    });
    let sources = [stealth.clone(), stealth, aggravation.clone(), aggravation];
    let folded = PerceptionCapabilities::from_modifiers(sources.iter()).unwrap();
    assert_eq!(folded.continual_stealth, Some(1));
    assert_eq!(
        folded.aggravation,
        Some(AggravationPolicy {
            interval: 500,
            chance_denominator: 3,
            stealth_disadvantage: 1,
        })
    );
    assert!(!folded.invisible);
    assert_eq!(folded.search_interval, None);
    let reversed = sources.iter().rev();
    assert_eq!(
        PerceptionCapabilities::from_modifiers(reversed).unwrap(),
        folded
    );
}

#[test]
fn awareness_modifiers_reject_zero_cadence_and_missing_fields() {
    for value in [
        serde_json::json!({"kind":"aggravation", "interval":0,"chance_denominator":3,"stealth_disadvantage":1}),
        serde_json::json!({"kind":"aggravation", "interval":500,"chance_denominator":0,"stealth_disadvantage":1}),
        serde_json::json!({"kind":"aggravation", "interval":500,"chance_denominator":3}),
        serde_json::json!({"kind":"continual_stealth"}),
    ] {
        assert!(serde_json::from_value::<PerceptionValue>(value).is_err());
    }
}

#[test]
fn worn_stealth_edges_are_qualified_for_every_shared_skill_capture() {
    let sources = [modifier(PerceptionValue::Aggravation {
        interval: 500,
        chance_denominator: 3,
        stealth_disadvantage: 1,
    })];
    let folded = PerceptionCapabilities::from_modifiers(sources.iter()).unwrap();
    for skill in crate::attributes::Skill::ALL {
        assert_eq!(
            folded.skill_edge(skill),
            if skill == crate::attributes::Skill::Stealth {
                -1
            } else {
                0
            }
        );
    }
    let sources = [
        sources[0].clone(),
        modifier(PerceptionValue::ContinualStealth { edge: 1 }),
    ];
    let folded = PerceptionCapabilities::from_modifiers(sources.iter()).unwrap();
    assert_eq!(folded.skill_edge(crate::attributes::Skill::Stealth), 0);
}

#[test]
fn one_subject_moving_keeps_every_other_subjects_classification() {
    // A hall of mutually visible creatures: when one steps, each observer's
    // other subjects keep their classification, and only the mover (whose
    // visibility inputs changed) is classified again.
    use crate::perception::contract_fixtures::{at, fixture, spawn, OWNER, SUBJECT};
    let mut game = fixture(&[], &[]);
    let others = [
        spawn(&mut game, "fixture_perception_subject", at(1, 4, 2)),
        spawn(&mut game, "fixture_perception_subject", at(1, 2, 3)),
    ];
    let scene = game.scene_snapshot_shared(OWNER).unwrap();
    let first = scene.classified_actors(&game, OWNER).unwrap();
    for subject in [SUBJECT, others[0], others[1]] {
        assert_eq!(
            first.get(&subject).map(|seen| seen.disclosure),
            Some(ActorDisclosure::Creature),
            "positive fixture: {subject:?} is seen"
        );
    }
    game.actors.get_mut(&SUBJECT).unwrap().location = at(1, 3, 3);
    let moved = game.scene_snapshot_shared(OWNER).unwrap();
    assert!(
        std::sync::Arc::ptr_eq(&scene, &moved),
        "positive fixture: another body's step keeps the observer's scene"
    );
    let before = crate::diagnostics::work_counts();
    let second = moved.classified_actors(&game, OWNER).unwrap();
    let after = crate::diagnostics::work_counts();
    assert_eq!(*second, game.classified_actors(OWNER).unwrap());
    assert_eq!(
        after.actor_disclosures_reused - before.actor_disclosures_reused,
        first.len() - 1,
        "every subject but the mover keeps its classification"
    );
    assert_eq!(
        after.actor_disclosures - before.actor_disclosures,
        1,
        "only the mover is classified again"
    );
    // A change to the observer's own visibility re-classifies everyone.
    game.actors.get_mut(&OWNER).unwrap().orientation ^= 1;
    let turned = game.scene_snapshot_shared(OWNER).unwrap();
    let before = crate::diagnostics::work_counts();
    turned.classified_actors(&game, OWNER).unwrap();
    let after = crate::diagnostics::work_counts();
    assert_eq!(
        after.actor_disclosures_reused,
        before.actor_disclosures_reused
    );
}
