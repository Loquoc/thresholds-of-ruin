use tor_client_3d_model::happenings;
use tor_client_common::test_support::{content, observation};
use tor_protocol::{AbilityOutcome, CombatEventView};

#[test]
fn harm_requires_a_worsening_disclosed_value_in_the_same_observer_scope() {
    use tor_client_3d_model::{observed_harm, view};
    use tor_client_common::Palette;
    use tor_protocol::{ActorId, Injury};
    let mut before = view(&super::projection::state(), &Palette::default());
    before.hp = Some((12, 12));
    let other = before.actors.iter().position(|a| !a.is_self).unwrap();
    let target = before.actors[other].id;
    before.actors[other].injury = Some(Injury::Healthy);
    let mut after = before.clone();
    after.actors[other].injury = Some(Injury::Wounded);
    // Multiple body cells still describe one injury.
    after.actors.push(after.actors[other].clone());
    assert_eq!(observed_harm(&before, &after), vec![target]);
    assert!(observed_harm(&after, &before).is_empty(), "healing");
    assert!(observed_harm(&after, &after).is_empty(), "stable");
    let mut unknown = before.clone();
    unknown.actors[other].injury = None;
    assert!(
        observed_harm(&unknown, &after).is_empty(),
        "newly learned injury"
    );
    after = before.clone();
    after.hp = Some((11, 12));
    let own = after.actors.iter().find(|a| a.is_self).unwrap().id;
    assert_eq!(observed_harm(&before, &after), vec![own]);
    for boundary in 0..3 {
        let mut reset = after.clone();
        match boundary {
            0 => reset.context.epoch += 1,
            1 => reset.branch.0 = "other".into(),
            _ => reset.shown.actor = ActorId(99),
        }
        assert!(
            observed_harm(&before, &reset).is_empty(),
            "boundary {boundary}"
        );
    }
}

#[test]
fn projected_techniques_keep_their_disclosed_id_separate_from_the_display_name() {
    let mut observed = observation();
    let mut ability = content("fixture_technique");
    ability.name = "A completely unrelated caption".into();
    observed.combat.as_mut().unwrap().events = vec![CombatEventView::Ability {
        caster: Some(observed.self_target),
        target: None,
        ability,
        outcome: AbilityOutcome::Applied,
        strike: None,
    }];
    assert!(
        matches!(&happenings(&observed)[0], tor_client_3d_model::Happening::Ability { id, name, .. }
        if id == "fixture_technique" && name == "A completely unrelated caption")
    );
}

#[test]
fn unaffected_ability_retains_its_non_applied_outcome() {
    let mut observed = observation();
    observed.combat.as_mut().unwrap().events = vec![CombatEventView::Ability {
        caster: Some(observed.self_target),
        target: Some(observed.visible_actors[0].id),
        ability: content("basic_melee"),
        outcome: AbilityOutcome::Unaffected,
        strike: None,
    }];
    let events = happenings(&observed);
    assert_eq!(
        events.len(),
        1,
        "a resisted effect still has a visible animation"
    );
    assert!(
        !events[0].is_applied(),
        "unaffected must not look like a landed hit"
    );
}

#[test]
fn applied_outcomes_remain_distinct_from_misses() {
    for outcome in [AbilityOutcome::Applied, AbilityOutcome::Miss] {
        let mut observed = observation();
        observed.combat.as_mut().unwrap().events = vec![CombatEventView::Ability {
            caster: Some(observed.self_target),
            target: Some(observed.visible_actors[0].id),
            ability: content("arbitrary_authored_ability"),
            outcome,
            strike: Some(2),
        }];
        assert_eq!(
            happenings(&observed)[0].is_applied(),
            outcome == AbilityOutcome::Applied
        );
    }
}

#[test]
fn authoritative_effect_projection_keeps_redacted_path_breaks_without_actor_lookup() {
    use tor_protocol::{EffectAppearance, EffectCue, EffectDelivery, EffectTiming, Position};
    let mut observed = observation();
    observed.visible_actors.clear();
    let segments = vec![
        vec![Position { x: 0, y: 1, z: 0 }, Position { x: 1, y: 1, z: 0 }],
        vec![Position { x: 9, y: 1, z: 0 }],
    ];
    observed.combat.as_mut().unwrap().events = vec![CombatEventView::Effect {
        caster: None,
        ability: None,
        tick: 17,
        anchors: vec![],
        timing: EffectTiming::Instant,
        appearance: EffectAppearance::Bolt,
        delivery: EffectDelivery::Ray,
        cue: EffectCue::Neutral,
        segments: segments.clone(),
    }];
    let events = happenings(&observed);
    assert_eq!(events.len(), 1);
    assert!(
        matches!(&events[0], tor_client_3d_model::Happening::Effect {
        caster: None, ability: None, tick: 17,
        timing: EffectTiming::Instant,
        appearance: EffectAppearance::Bolt,
        delivery: EffectDelivery::Ray, cue: EffectCue::Neutral, segments: actual,
    } if actual == &segments)
    );
    assert!(!events[0].is_applied(), "geometry is not a damage outcome");
}

#[test]
fn ordered_effects_align_each_cell_and_split_missing_or_rotated_picture_points() {
    use tor_client_3d_model::{outcome_happenings, Happening};
    use tor_protocol::*;
    let point = |x, y| Position { x, y, z: 0 };
    let cell = CellView {
        key: "a".repeat(40),
        position: point(0, 0),
        wall: false,
        material: "stone".into(),
        asset: None,
        lit: true,
        door: None,
        trap: None,
        object_presentation: None,
        stairs_up: false,
        stairs_down: false,
        place_hint: false,
    };
    let cells: Vec<_> = (0..3)
        .map(|i| {
            let mut c = cell.clone();
            c.key = format!("{:040x}", i + 1);
            c.position = point(i + 10, 5);
            c
        })
        .collect();
    let event = CombatEventView::Effect {
        caster: None,
        ability: None,
        tick: 1,
        timing: EffectTiming::Instant,
        appearance: EffectAppearance::Bolt,
        delivery: EffectDelivery::Ray,
        cue: EffectCue::Neutral,
        segments: vec![vec![point(0, 0), point(1, 0), point(2, 0)]],
        anchors: (0..3)
            .map(|i| EffectAnchor {
                key: format!("{:040x}", i + 1),
                position: point(i, 0),
            })
            .collect(),
    };
    let facts = OutcomeFacts {
        self_target: observation().self_target,
        status: None,
        actor_names: vec![],
        facts: vec![OutcomeFact::Combat {
            event,
            basic_attack: false,
        }],
    };
    let projected = |cells: &[CellView]| {
        let events = outcome_happenings(&facts, cells);
        events
            .into_iter()
            .map(|e| match e {
                Happening::Effect { segments, .. } => segments,
                _ => panic!("effect"),
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(
        projected(&cells),
        vec![vec![vec![point(10, 5), point(11, 5), point(12, 5)]]]
    );
    let missing = vec![cells[0].clone(), cells[2].clone()];
    assert_eq!(
        projected(&missing),
        vec![vec![vec![point(10, 5)], vec![point(12, 5)]]]
    );
    let mut rotated = cells.clone();
    rotated[1].position = point(10, 6);
    rotated[2].position = point(10, 7);
    assert_eq!(
        projected(&rotated),
        vec![vec![
            vec![point(10, 5)],
            vec![point(10, 6)],
            vec![point(10, 7)]
        ]]
    );
    let mut ambiguous = cells.clone();
    ambiguous.push(cells[1].clone());
    assert_eq!(
        projected(&ambiguous),
        vec![vec![vec![point(10, 5)], vec![point(12, 5)]]]
    );
    assert!(projected(&[]).is_empty());
    // Defense in depth: even correctly aligned endpoints cannot license a
    // straight interpolation through cells missing from the supplied path.
    let mut gap_facts = facts.clone();
    if let OutcomeFact::Combat {
        event: CombatEventView::Effect { segments, .. },
        ..
    } = &mut gap_facts.facts[0]
    {
        segments[0].remove(1);
    }
    let gaps = outcome_happenings(&gap_facts, &cells);
    assert!(
        matches!(&gaps[..], [Happening::Effect { segments, .. }] if *segments == vec![vec![point(10,5)], vec![point(12,5)]])
    );
}

#[test]
fn ordered_outcome_keeps_ordinary_ability_animation_without_spatial_effect() {
    use tor_client_3d_model::{outcome_happenings, Happening};
    use tor_protocol::*;
    let actor = observation().self_target;
    let facts = OutcomeFacts {
        self_target: actor,
        status: None,
        actor_names: vec![],
        facts: vec![OutcomeFact::Combat {
            event: CombatEventView::Ability {
                caster: Some(actor),
                target: None,
                ability: ContentRef {
                    id: "fixture.swing".into(),
                    name: "Swing".into(),
                },
                outcome: AbilityOutcome::Miss,
                strike: None,
            },
            basic_attack: false,
        }],
    };
    let events = outcome_happenings(&facts, &[]);
    assert!(
        matches!(&events[..], [Happening::Ability {caster: Some(caster), id, outcome: AbilityOutcome::Miss, ..}] if *caster == actor && id == "fixture.swing")
    );
}
