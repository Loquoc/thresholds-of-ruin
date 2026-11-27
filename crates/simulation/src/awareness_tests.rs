use super::*;
use crate::perception::contract_fixtures::{at, OWNER, SUBJECT};
use crate::perception::PerceptionValue;

fn encounter(stealth: bool) -> Game {
    encounter_with_height(stealth, 1)
}

fn encounter_with_height(stealth: bool, height: i32) -> Game {
    let modifiers = if stealth {
        vec![PerceptionValue::ContinualStealth { edge: 1 }]
    } else {
        vec![]
    };
    let mut game = crate::perception::contract_fixtures::fixture_with_height(
        &[],
        &modifiers,
        height,
        |_, owner, _| {
            owner.behavior.disposition = crate::behavior::Disposition::Mean;
            owner.behavior.idle = crate::behavior::IdleBehavior::Wait;
        },
    );
    game.actors
        .get_mut(&OWNER)
        .unwrap()
        .combat
        .as_mut()
        .unwrap()
        .identity
        .faction = "observer".into();
    game.combat
        .hostility
        .insert("observer".into(), ["private affiliation".into()].into());
    game.combat
        .hostility
        .insert("private affiliation".into(), ["observer".into()].into());
    game.configure_ai(
        OWNER,
        crate::ai::AiProfile {
            idle: crate::behavior::IdleBehavior::Wait,
            ..Default::default()
        },
    )
    .unwrap();
    game.refresh_navigation();
    game.initialize_discovery().unwrap();
    game
}

#[test]
fn irrelevant_visibility_change_reconciles_without_detaching_unchanged_contacts() {
    let mut game = encounter(false);
    assert!(
        game.actors[&OWNER].awareness.detected(SUBJECT),
        "positive established contact"
    );
    let remote = game
        .spawn_actor(at(2, 2, 2), std::num::NonZeroU64::new(100).unwrap())
        .unwrap();
    game.reconcile_awareness().unwrap();
    assert!(!game
        .observe(OWNER)
        .unwrap()
        .visible_actors
        .iter()
        .any(|actor| actor.id == remote));
    let retained = game.clone();
    let visibility = game.actors.visibility_epoch();
    game.actors.get_mut(&remote).unwrap().location.position.x += 1;
    assert_ne!(
        game.actors.visibility_epoch(),
        visibility,
        "remote movement invalidates the global awareness witness"
    );
    let expected = game.clone();
    let contacts = game.actors[&OWNER].awareness.contacts.clone();
    let rng = game.rules_rng;
    let before = crate::diagnostics::work_counts();
    game.reconcile_awareness().unwrap();
    let after = crate::diagnostics::work_counts();
    assert_eq!(
        game, expected,
        "reconciliation is semantically unchanged, including candidate snapshots"
    );
    assert_eq!(game.actors[&OWNER].awareness.contacts, contacts);
    assert_eq!(game.rules_rng, rng);
    assert!(
        after.perception_projections > before.perception_projections,
        "the invalidated witness must really rescan observers"
    );
    assert_eq!(after.observations, before.observations);
    assert_eq!(
        after.actor_table_copies - before.actor_table_copies,
        0,
        "unchanged contact writes must not detach actor payloads"
    );
    assert!(game.actors.shares_actor_payload(&retained.actors, OWNER));
}

/// Invalidate the global witness so the next reconciliation rescans.
fn move_bystander(game: &mut Game) {
    let remote = game
        .spawn_actor(at(2, 2, 2), std::num::NonZeroU64::new(100).unwrap())
        .unwrap();
    game.reconcile_awareness().unwrap();
    game.actors.get_mut(&remote).unwrap().location.position.x += 1;
}

#[test]
fn an_observer_with_no_one_to_be_hostile_toward_settles_without_projecting() {
    // A hall of peaceful creatures: each sees the others, but contacts hold
    // only perceived-hostile subjects, so no view needs projecting.
    let mut game = encounter(false);
    assert!(
        game.actors[&OWNER].awareness.detected(SUBJECT),
        "positive fixture"
    );
    game.combat.hostility.clear();
    move_bystander(&mut game);
    let rng = game.rules_rng;
    let before = crate::diagnostics::work_counts();
    game.reconcile_awareness().unwrap();
    let after = crate::diagnostics::work_counts();
    assert!(
        game.actors[&OWNER].awareness.contacts.is_empty(),
        "ended hostility clears the established contact"
    );
    assert_eq!(game.rules_rng, rng, "no contest is rolled");
    assert_eq!(after.perception_projections, before.perception_projections);
    assert_eq!(
        after.awareness_subject_candidates,
        before.awareness_subject_candidates
    );
    assert_eq!(
        after.awareness_observers_without_hostility - before.awareness_observers_without_hostility,
        1
    );
}

#[test]
fn an_engagement_alone_keeps_its_observer_projecting_and_its_contact() {
    let mut game = encounter(false);
    game.combat.hostility.clear();
    let clock = game.actor_clock(OWNER);
    game.actors.get_mut(&OWNER).unwrap().behavior.engagement = Some(crate::behavior::Engagement {
        reference: crate::perception::ActorReferenceKind::Creature,
        target: SUBJECT,
        location: game.actors[&SUBJECT].location,
        last_seen: clock,
    });
    assert!(
        game.actors[&OWNER].engagement_at(clock).is_some(),
        "positive fixture: the engagement is current"
    );
    move_bystander(&mut game);
    let before = crate::diagnostics::work_counts();
    game.reconcile_awareness().unwrap();
    let after = crate::diagnostics::work_counts();
    assert!(after.perception_projections > before.perception_projections);
    assert!(
        game.actors[&OWNER].awareness.detected(SUBJECT),
        "a personal engagement is perceived hostility without a faction enemy"
    );
}

#[test]
fn compact_awareness_subjects_match_canonical_body_rows_without_row_vectors() {
    for stealth in [false, true] {
        let mut game = encounter_with_height(stealth, 2);
        game.set_body(
            SUBJECT,
            crate::BodySpec {
                cells: vec![[0, 0, 0], [0, 0, 1]],
                eye: [0, 0, 1],
                mass: 80,
            },
        )
        .unwrap();
        let oracle_counts = crate::diagnostics::work_counts();
        let view = game.observe_actor_references(OWNER).unwrap();
        assert!(
            crate::diagnostics::work_counts().actor_reference_vectors
                > oracle_counts.actor_reference_vectors,
            "the differential oracle really materializes canonical body rows"
        );
        let mut expected = Vec::new();
        for visible in view.visible_actors {
            if visible.id == OWNER
                || expected
                    .last()
                    .is_some_and(|prior: &AwarenessSubject| prior.subject == visible.id)
            {
                continue;
            }
            expected.push(AwarenessSubject {
                subject: visible.id,
                disclosure: visible.disclosure,
                previous: game.actors[&OWNER]
                    .awareness
                    .contacts
                    .iter()
                    .find(|c| c.subject == visible.id)
                    .copied(),
            });
        }
        assert!(expected
            .iter()
            .any(|candidate| candidate.subject == SUBJECT));
        let unchanged = game.clone();
        let before = crate::diagnostics::work_counts();
        let (at, actual) = game.awareness_subjects(OWNER).unwrap();
        assert_eq!(at, view.location);
        assert_eq!(
            actual, expected,
            "compact membership and prior receipts match canonical rows"
        );
        assert_eq!(
            game, unchanged,
            "candidate staging has no rules or RNG effects"
        );
        let after = crate::diagnostics::work_counts();
        assert_eq!(
            after.awareness_subject_candidates - before.awareness_subject_candidates,
            actual.len()
        );
        assert_eq!(
            after.actor_reference_vectors - before.actor_reference_vectors,
            0,
            "compact awareness must not materialize the discarded body-row vector"
        );
    }
}

#[test]
fn compact_awareness_reconciliation_matches_cold_contact_order_and_rng() {
    for stealth in [false, true] {
        let mut game = encounter_with_height(stealth, 2);
        game.set_body(
            SUBJECT,
            crate::BodySpec {
                cells: vec![[0, 0, 0], [0, 0, 1]],
                eye: [0, 0, 1],
                mass: 80,
            },
        )
        .unwrap();
        game.actors
            .get_mut(&OWNER)
            .unwrap()
            .awareness
            .contacts
            .clear();
        game.lifecycle.awareness_contexts = Default::default();
        assert_eq!(
            game.combat.ai.keys().copied().collect::<Vec<_>>(),
            vec![OWNER]
        );
        let mut cold = game.clone();
        let view = cold.observe_actor_references(OWNER).unwrap();
        let old = cold.actors[&OWNER].awareness.contacts.clone();
        let mut contacts = Vec::new();
        let mut subjects = std::collections::BTreeSet::new();
        for visible in &view.visible_actors {
            if visible.id == OWNER
                || !cold.perceived_hostile(OWNER, visible.id, visible.disclosure)
                || !subjects.insert(visible.id)
            {
                continue;
            }
            assert!(contacts.len() < MAX_CONTACTS);
            let source_at = cold.actors[&visible.id].location;
            let previous = old.iter().find(|c| c.subject == visible.id);
            let unchanged = previous
                .is_some_and(|c| c.observer_at == view.location && c.source_at == source_at);
            let unhidden = cold.actors[&visible.id]
                .perception()
                .continual_stealth
                .is_none();
            let detected = if unhidden || previous.is_some_and(|c| c.detected) {
                true
            } else if unchanged {
                previous.unwrap().detected
            } else {
                cold.resolve_awareness_contest(OWNER, visible.id).unwrap()
            };
            contacts.push(Contact {
                subject: visible.id,
                observer_at: view.location,
                source_at,
                detected,
            });
        }
        contacts.sort_by_key(|c| c.subject);
        cold.actors.get_mut(&OWNER).unwrap().awareness.contacts = contacts;
        game.reconcile_awareness().unwrap();
        assert_eq!(game.rules_rng, cold.rules_rng, "same ordered contest draws");
        assert_eq!(
            game, cold,
            "canonical body-row oracle retains contacts and all rules state"
        );
    }
}

#[test]
fn awareness_reconciliation_projects_actor_rows_without_full_observations() {
    let mut game = encounter(false);
    assert!(game.actors[&OWNER].awareness.detected(SUBJECT));
    game.actors
        .get_mut(&OWNER)
        .unwrap()
        .awareness
        .contacts
        .clear();
    game.lifecycle.awareness_contexts = Default::default();
    let rng = game.rules_rng;
    let before = crate::diagnostics::work_counts();
    game.reconcile_awareness().unwrap();
    assert_eq!(
        game.actors[&OWNER].awareness.contacts,
        vec![Contact {
            subject: SUBJECT,
            observer_at: game.actors[&OWNER].location,
            source_at: game.actors[&SUBJECT].location,
            detected: true,
        }]
    );
    assert_eq!(game.rules_rng, rng);
    let after = crate::diagnostics::work_counts();
    assert_eq!(
        after.perception_projections - before.perception_projections,
        1
    );
    assert_eq!(
        after.observations, before.observations,
        "awareness needs fresh actor references, not a terrain/inventory/combat view"
    );
}

#[test]
fn attack_awareness_capture_and_application_do_not_build_full_observations() {
    let mut game = encounter(false);
    game.actors
        .get_mut(&OWNER)
        .unwrap()
        .awareness
        .contacts
        .clear();
    let rng = game.rules_rng;
    // Explicitly cold geometry/classification makes the work bound
    // independent of incidental fixture setup. Capture and fresh reference
    // revalidation may share the same exact visibility witness.
    game.lifecycle.scenes = Default::default();
    let visibility = game.actors.visibility_epoch();
    let before = crate::diagnostics::work_counts();
    let witness = game.capture_attack_awareness(SUBJECT, OWNER).unwrap();
    assert_eq!(witness.cues.len(), 1);
    let cue = &witness.cues[0];
    assert_eq!(cue.listener, OWNER);
    assert_eq!(cue.alert, Some(game.actors[&SUBJECT].location));
    assert_eq!(
        cue.reference,
        Some(crate::perception::ActorReferenceKind::Creature)
    );
    assert!(cue.others.is_empty());
    game.apply_attack_awareness(witness).unwrap();
    assert_eq!(
        game.actors[&OWNER].awareness.contacts,
        vec![Contact {
            subject: SUBJECT,
            observer_at: game.actors[&OWNER].location,
            source_at: game.actors[&SUBJECT].location,
            detected: true,
        }]
    );
    assert_eq!(
        game.actors[&OWNER].awareness.alert_at(game.tick),
        Some(game.actors[&SUBJECT].location)
    );
    assert_eq!(game.rules_rng, rng);
    assert_eq!(game.actors.visibility_epoch(), visibility);
    let after = crate::diagnostics::work_counts();
    assert_eq!(
        after.perception_projections - before.perception_projections,
        1
    );
    assert_eq!(
        after.observations, before.observations,
        "capture and revalidation must not construct unrelated full disclosure"
    );
}

#[test]
fn unchanged_attack_listener_reuses_physical_hearing_geometry() {
    let game = encounter(false);
    let before_game = game.clone();
    let first = game.capture_attack_awareness(SUBJECT, OWNER).unwrap();
    assert_eq!(first.cues.len(), 1);
    assert_eq!(first.cues[0].alert, Some(game.actors[&SUBJECT].location));
    let before = crate::diagnostics::work_counts();
    let repeated = game.capture_attack_awareness(SUBJECT, OWNER).unwrap();
    assert_eq!(repeated.cues.len(), 1);
    assert_eq!(repeated.cues[0].listener, first.cues[0].listener);
    assert_eq!(repeated.cues[0].alert, first.cues[0].alert);
    assert_eq!(repeated.cues[0].reference, first.cues[0].reference);
    assert!(repeated.cues[0].others.is_empty());
    assert_eq!(game, before_game);
    assert_eq!(game.rules_rng, before_game.rules_rng);
    let after = crate::diagnostics::work_counts();
    assert_eq!(
        after.hearing_scene_builds, before.hearing_scene_builds,
        "unchanged listener geometry is independent of the current attack"
    );
    assert_eq!(after.hearing_scene_cells, before.hearing_scene_cells);
}

#[test]
fn ordinary_approach_detects_without_lottery_or_query_mutation() {
    let mut game = encounter(false);
    assert!(game.actors[&OWNER].awareness.detected(SUBJECT));
    let before = game.clone();
    for _ in 0..8 {
        game.observe(OWNER).unwrap();
        game.choose_ai(OWNER).unwrap();
    }
    assert_eq!(game, before);
    game.reconcile_awareness().unwrap();
    assert_eq!(game.rules_rng, before.rules_rng);
}

#[test]
fn awareness_cache_tracks_hostility_and_personal_contact_expiry() {
    let mut game = encounter(false);
    game.combat.hostility.clear();
    game.reconcile_awareness().unwrap();
    assert!(!game.actors[&OWNER].awareness.detected(SUBJECT));
    game.actors.get_mut(&OWNER).unwrap().behavior.engagement = Some(crate::behavior::Engagement {
        reference: crate::perception::ActorReferenceKind::Creature,
        target: SUBJECT,
        location: game.actors[&SUBJECT].location,
        last_seen: game.tick,
    });
    game.reconcile_awareness().unwrap();
    assert!(game.actors[&OWNER].awareness.detected(SUBJECT));
    // No geometry or visibility input changes when personal memory expires.
    game.tick = game.actors[&OWNER].engagement_deadline().unwrap();
    game.reconcile_awareness().unwrap();
    assert!(!game.actors[&OWNER].awareness.detected(SUBJECT));
}

#[test]
fn tall_visible_subject_owns_one_contest_and_strict_checkpoint_receipt() {
    let mut game = encounter_with_height(true, 2);
    game.set_body(
        SUBJECT,
        crate::BodySpec {
            cells: vec![[0, 0, 0], [0, 0, 1]],
            eye: [0, 0, 1],
            mass: 80,
        },
    )
    .unwrap();
    assert!(
        game.observe(OWNER)
            .unwrap()
            .visible_actors
            .iter()
            .filter(|row| row.id == SUBJECT)
            .count()
            >= 2
    );
    game.actors
        .get_mut(&OWNER)
        .unwrap()
        .awareness
        .contacts
        .clear();
    game.lifecycle.awareness_contexts = Default::default();
    let mut expected = game.clone();
    let detected = expected.resolve_awareness_contest(OWNER, SUBJECT).unwrap();
    game.reconcile_awareness().unwrap();
    assert_eq!(game.rules_rng, expected.rules_rng, "one actor, one contest");
    let contacts = &game.actors[&OWNER].awareness.contacts;
    assert_eq!(contacts.len(), 1);
    assert_eq!(contacts[0].source_at, game.actors[&SUBJECT].location);
    assert_eq!(contacts[0].detected, detected);
    let valid = serde_json::to_value(&game.actors[&OWNER].awareness).unwrap();
    let decoded: Awareness = serde_json::from_value(valid.clone()).unwrap();
    assert_eq!(decoded, game.actors[&OWNER].awareness);
    let mut duplicate = valid.clone();
    duplicate["contacts"]
        .as_array_mut()
        .unwrap()
        .push(valid["contacts"][0].clone());
    assert!(serde_json::from_value::<Awareness>(duplicate).is_err());
    let mut unordered = valid.clone();
    let mut earlier = valid["contacts"][0].clone();
    earlier["subject"] = serde_json::json!(OWNER.0);
    unordered["contacts"].as_array_mut().unwrap().push(earlier);
    assert!(serde_json::from_value::<Awareness>(unordered).is_err());
    let mut shared = crate::checkpoint::SharedState::default();
    let snapshot = game.checkpoint(&mut shared);
    let bytes = serde_json::to_vec(&(snapshot, shared)).unwrap();
    let (snapshot, shared) = serde_json::from_slice(&bytes).unwrap();
    let restored = Game::restore_checkpoint(snapshot, &shared, game.content().clone()).unwrap();
    assert_eq!(restored, game);
}

#[test]
fn visible_upper_body_contact_uses_occluded_actor_anchor() {
    let mut game = encounter_with_height(true, 2);
    for actor in [OWNER, SUBJECT] {
        game.set_body(
            actor,
            crate::BodySpec {
                cells: vec![[0, 0, 0], [0, 0, 1]],
                eye: [0, 0, 1],
                mass: 80,
            },
        )
        .unwrap();
    }
    game.teleport(SUBJECT, at(1, 5, 2)).unwrap();
    game.set_wall(at(1, 4, 2), true).unwrap();
    let view = game.observe(OWNER).unwrap();
    let cells: Vec<_> = view
        .visible_actors
        .iter()
        .filter(|row| row.id == SUBJECT)
        .map(|row| row.location)
        .collect();
    assert!(!cells.is_empty(), "upper body remains physically visible");
    assert!(
        !cells.contains(&game.actors[&SUBJECT].location),
        "anchor is occluded"
    );
    game.actors
        .get_mut(&OWNER)
        .unwrap()
        .awareness
        .contacts
        .clear();
    game.lifecycle.awareness_contexts = Default::default();
    game.reconcile_awareness().unwrap();
    let contacts = &game.actors[&OWNER].awareness.contacts;
    assert_eq!(contacts.len(), 1);
    assert_eq!(contacts[0].source_at, game.actors[&SUBJECT].location);
    let rng = game.rules_rng;
    game.lifecycle.awareness_contexts = Default::default();
    game.reconcile_awareness().unwrap();
    assert_eq!(
        game.rules_rng, rng,
        "head-only receipt is still the same contact"
    );
}

#[test]
fn attempted_stealth_receipt_survives_queries_and_checkpoint() {
    let mut game = encounter(true);
    let before = game.rules_rng;
    let receipt = game.actors[&OWNER].awareness.clone();
    for _ in 0..8 {
        game.observe(OWNER).unwrap();
        game.choose_ai(OWNER).unwrap();
        game.reconcile_awareness().unwrap();
    }
    assert_eq!(game.rules_rng, before);
    assert_eq!(game.actors[&OWNER].awareness, receipt);
    let mut shared = crate::checkpoint::SharedState::default();
    let snapshot =
        serde_json::from_slice(&serde_json::to_vec(&game.checkpoint(&mut shared)).unwrap())
            .unwrap();
    let shared = serde_json::from_slice(&serde_json::to_vec(&shared).unwrap()).unwrap();
    let mut restored = Game::restore_checkpoint(snapshot, &shared, game.content().clone()).unwrap();
    restored.reconcile_awareness().unwrap();
    assert_eq!(restored.rules_rng, before);
    assert_eq!(restored.actors[&OWNER].awareness, receipt);
}

#[test]
fn location_pulse_has_no_identity_and_does_not_track_source() {
    let mut game = encounter(false);
    game.alert_location_pulse(SUBJECT).unwrap();
    assert!(game.actors[&OWNER].behavior.engagement.is_none());
    assert_eq!(
        game.actors[&OWNER].awareness.alert_at(game.tick),
        Some(at(1, 3, 2))
    );
    game.teleport(SUBJECT, at(2, 4, 2)).unwrap();
    assert_eq!(
        game.actors[&OWNER].awareness.alert_at(game.tick),
        Some(at(1, 3, 2))
    );
    let expires = game.actors[&OWNER].awareness.alert.unwrap().expires;
    assert_eq!(game.actors[&OWNER].awareness.alert_at(expires), None);
}

#[test]
fn completed_location_investigation_does_not_reopen_after_departure() {
    let mut game = encounter(false);
    let destination = game.actors[&SUBJECT].location;
    game.alert_location_pulse(SUBJECT).unwrap();
    game.teleport(SUBJECT, at(2, 4, 2)).unwrap();
    game.configure_ai(
        OWNER,
        crate::ai::AiProfile {
            idle: crate::behavior::IdleBehavior::Explore,
            ..Default::default()
        },
    )
    .unwrap();
    game.refresh_navigation();
    let receipt = game.actors[&OWNER].awareness.alert;
    assert!(receipt.is_some());
    let before = game.clone();
    let action = game.next_ai_action().unwrap();
    assert_eq!(
        action,
        (OWNER, crate::Action::Move(tor_world::Direction::East))
    );
    assert_eq!(game, before, "investigation preview is pure");
    game.act(action.0, action.1).unwrap();
    assert_eq!(game.actors[&OWNER].location, destination);
    assert_eq!(
        game.actors[&OWNER].awareness.alert, None,
        "arrival completes this location-only investigation, not just its current step"
    );
    assert!(game.actors[&OWNER].behavior.engagement.is_none());
}

#[test]
fn location_investigation_completion_is_endpoint_local_and_transactional() {
    let mut game = encounter(false);
    let destination = game.actors[&SUBJECT].location;
    game.alert_location_pulse(SUBJECT).unwrap();
    game.teleport(SUBJECT, at(2, 4, 2)).unwrap();
    let original = game.clone();
    let mut outcome = game.settled_actor_outcome(OWNER).unwrap();
    outcome.state.location = destination;
    let prepared = game.prepare_actor_outcome(outcome).unwrap();
    drop(prepared);
    assert_eq!(
        game, original,
        "unpublished arrival cannot retire a receipt"
    );
    let mut outcome = game.settled_actor_outcome(OWNER).unwrap();
    outcome.state.location = at(1, 4, 2);
    game.apply_outcome(outcome).unwrap();
    assert_eq!(
        game.actors[&OWNER].awareness.alert, original.actors[&OWNER].awareness.alert,
        "passing no licensed endpoint is not completion"
    );
    // Authorized relocation reaches the point too: this is physical visit
    // completion, not a hidden search contest or a new target license.
    game.teleport(OWNER, destination).unwrap();
    assert!(game.actors[&OWNER].awareness.alert.is_none());
    game.install_location_alert(OWNER, destination).unwrap();
    assert!(
        game.actors[&OWNER].awareness.alert.is_some(),
        "a fresh cue may be installed"
    );
    let before = game.clone();
    let _ = game.choose_ai(OWNER);
    assert_eq!(
        game, before,
        "current-point preview must not consume the cue"
    );
    game.teleport(OWNER, at(1, 4, 2)).unwrap();
    assert!(
        game.actors[&OWNER].awareness.alert.is_none(),
        "departure completes a fresh cue received at the current point"
    );
    game.install_location_alert(OWNER, at(1, 2, 2)).unwrap();
    let renewed = game.actors[&OWNER].awareness.alert;
    let outcome = game.settled_actor_outcome(OWNER).unwrap();
    game.apply_outcome(outcome).unwrap();
    assert_eq!(
        game.actors[&OWNER].awareness.alert, renewed,
        "an unrelated committed transition cannot retire a fresh remote cue"
    );
}

#[test]
fn cadence_preserves_fraction_and_serialization_rejects_malformed_state() {
    let mut clock = Awareness::default();
    clock.advance(499, 3).unwrap();
    clock.rephase(None);
    clock.rephase(Some(crate::perception::AggravationPolicy {
        interval: 500,
        chance_denominator: 3,
        stealth_disadvantage: 1,
    }));
    assert_eq!(clock.elapsed, 499);
    let value = serde_json::to_value(&clock).unwrap();
    assert_eq!(
        serde_json::from_value::<Awareness>(value.clone()).unwrap(),
        clock
    );
    for key in ["contacts", "alert", "period", "elapsed", "pending"] {
        let mut invalid = value.clone();
        invalid.as_object_mut().unwrap().remove(key);
        assert!(
            serde_json::from_value::<Awareness>(invalid).is_err(),
            "missing {key}"
        );
    }
    clock.advance(1, 3).unwrap();
    assert_eq!(clock.take_pending(), Some(3));
    assert_eq!(clock.take_pending(), None);
    assert_eq!(clock.elapsed, 0);
}

#[test]
fn frozen_alert_shifts_exactly_without_live_tracking() {
    let mut state = Awareness {
        alert: Some(LocationAlert {
            location: at(1, 3, 2),
            expires: 1000,
        }),
        ..Default::default()
    };
    state.shift(3000).unwrap();
    assert_eq!(state.alert_at(3999), Some(at(1, 3, 2)));
    assert_eq!(state.alert_at(4000), None);
}
