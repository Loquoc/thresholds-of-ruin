use super::*;

fn receipt_fixture() -> Observation {
    let mut view = crate::test_support::portable_light_observation(false);
    let item = view.inventory[0].id;
    let interactions = view.interactions.as_mut().unwrap();
    interactions.completed = vec![Action::Ignite { item }];
    interactions.refused = vec![ItemRefusalView {
        action: Action::Zap {
            item,
            aim: DeviceAim::SelfTarget {},
        },
        reason: ItemRefusalReason::TooWeak,
    }];
    interactions.curse_blocked = vec![item];
    view.combat.as_mut().unwrap().events = vec![CombatEventView::Ability {
        caster: Some(view.self_target),
        target: Some(view.visible_actors[0].id),
        ability: crate::test_support::content("basic_melee"),
        outcome: AbilityOutcome::Miss,
        strike: None,
    }];
    view
}

fn receipt_count(facts: &OutcomeFacts) -> usize {
    facts
        .facts
        .iter()
        .filter(|fact| {
            matches!(
                fact,
                OutcomeFact::Combat { .. }
                    | OutcomeFact::ItemCompleted { .. }
                    | OutcomeFact::ItemRefused { .. }
                    | OutcomeFact::CurseBlocked { .. }
            )
        })
        .count()
}

#[test]
fn causal_receipts_distinguish_old_batch_from_identical_same_tick_new_batch() {
    let before = receipt_fixture();
    let mut later = before.clone();
    later.tick += 1;
    assert_eq!(
        receipt_count(&OutcomeFacts::capture(&before, &later, None)),
        4,
        "legacy tick-only rendering repeats stale receipts"
    );
    let unchanged = OutcomeFacts::capture_causal(
        &before,
        &later,
        None,
        tor_protocol::EventProvenance {
            batch: 5,
            through: 4,
        },
        tor_protocol::EventProvenance {
            batch: 5,
            through: 4,
        },
    )
    .unwrap();
    assert_eq!(receipt_count(&unchanged), 0);
    assert!(!OutcomeFacts::picture_changed(&before, &later));
    assert_eq!(
        receipt_count(&OutcomeFacts::capture(&before, &before, None)),
        0,
        "legacy equality suppresses a distinct identical same-tick batch"
    );
    let fresh = OutcomeFacts::capture_causal(
        &before,
        &before,
        None,
        tor_protocol::EventProvenance {
            batch: 5,
            through: 4,
        },
        tor_protocol::EventProvenance {
            batch: 6,
            through: 4,
        },
    )
    .unwrap();
    assert_eq!(receipt_count(&fresh), 4);
    fresh.validate_at(before.tick).unwrap();
}

#[test]
fn causal_receipts_emit_exact_same_batch_suffix_and_reject_rewritten_prefix() {
    let before = receipt_fixture();
    let mut after = before.clone();
    after
        .combat
        .as_mut()
        .unwrap()
        .events
        .extend(before.combat.as_ref().unwrap().events.clone());
    let old = before.interactions.as_ref().unwrap();
    let current = after.interactions.as_mut().unwrap();
    current.completed.extend(old.completed.clone());
    current.refused.extend(old.refused.clone());
    current.curse_blocked.extend(old.curse_blocked.clone());
    assert_eq!(
        receipt_count(&OutcomeFacts::capture(&before, &after, None)),
        8
    );
    let suffix = OutcomeFacts::capture_causal(
        &before,
        &after,
        None,
        tor_protocol::EventProvenance {
            batch: 5,
            through: 4,
        },
        tor_protocol::EventProvenance {
            batch: 5,
            through: 8,
        },
    )
    .unwrap();
    assert_eq!(receipt_count(&suffix), 4);
    suffix.validate_at(after.tick).unwrap();
    assert!(OutcomeFacts::capture_causal(
        &before,
        &after,
        None,
        tor_protocol::EventProvenance {
            batch: 5,
            through: 4
        },
        tor_protocol::EventProvenance {
            batch: 5,
            through: 4
        }
    )
    .is_err());
    assert!(OutcomeFacts::capture_causal(
        &before,
        &after,
        None,
        tor_protocol::EventProvenance {
            batch: 5,
            through: 4
        },
        tor_protocol::EventProvenance {
            batch: 5,
            through: 3
        }
    )
    .is_err());
    after.combat.as_mut().unwrap().events[0] = CombatEventView::Interrupted {
        actor: after.self_target,
    };
    assert!(OutcomeFacts::capture_causal(
        &before,
        &after,
        None,
        tor_protocol::EventProvenance {
            batch: 5,
            through: 4
        },
        tor_protocol::EventProvenance {
            batch: 5,
            through: 8
        }
    )
    .is_err());
}

#[test]
fn persistent_picture_ignores_receipt_clear_and_motion_pulses_but_keeps_velocity() {
    let mut before = receipt_fixture();
    before.motion = Some(MotionView {
        velocity: [0; 3],
        units_per_cell: 100,
        displaced: true,
        impacted: true,
    });
    let mut after = before.clone();
    after.tick += 1;
    after.combat.as_mut().unwrap().events.clear();
    let interactions = after.interactions.as_mut().unwrap();
    interactions.completed.clear();
    interactions.refused.clear();
    interactions.curse_blocked.clear();
    after.motion = None;
    let cleared = OutcomeFacts::capture_causal(
        &before,
        &after,
        None,
        tor_protocol::EventProvenance {
            batch: 5,
            through: 4,
        },
        tor_protocol::EventProvenance {
            batch: 6,
            through: 0,
        },
    )
    .unwrap();
    assert_eq!(receipt_count(&cleared), 0);
    assert!(!OutcomeFacts::picture_changed(&before, &after));
    after.motion = Some(MotionView {
        velocity: [1, 0, 0],
        units_per_cell: 100,
        displaced: false,
        impacted: false,
    });
    assert!(OutcomeFacts::picture_changed(&before, &after));
    after.motion = None;
    after.combat.as_mut().unwrap().hp -= 1;
    assert!(OutcomeFacts::picture_changed(&before, &after));
}

#[test]
fn outcome_facts_preserve_text_only_injury_and_objective_without_extra_common_lines() {
    let mut before = crate::test_support::observation();
    let actor = before.visible_actors[0].id;
    before.combat.as_mut().unwrap().actors = vec![CombatActorView {
        actor,
        hostile: true,
        allied: false,
        injury: Injury::Healthy,
    }];
    let mut after = before.clone();
    after.combat.as_mut().unwrap().actors[0].injury = Injury::Wounded;
    let goal = vec![UserMessageSegment::Text("Reach the exit.".into())];
    after.combat.as_mut().unwrap().goal = Some(tor_protocol::GoalView {
        segments: goal.clone(),
        cell: None,
    });
    let facts = OutcomeFacts::capture(&before, &after, None);
    facts.validate_at(after.tick).unwrap();
    assert!(facts.facts.contains(&OutcomeFact::Injured {
        actor,
        injury: Injury::Wounded
    }));
    assert!(facts.facts.contains(&OutcomeFact::Goal { segments: goal }));
    assert_eq!(
        outcome_lines(&facts),
        observation_lines_legacy(&before, &after, None)
    );
    after
        .combat
        .as_mut()
        .unwrap()
        .events
        .push(CombatEventView::Died {
            actor,
            cause: DeathCause::Injury,
        });
    let facts = OutcomeFacts::capture(&before, &after, None);
    assert!(!facts
        .facts
        .iter()
        .any(|fact| matches!(fact, OutcomeFact::Injured { .. })));
    let repeat = OutcomeFacts::capture(&after, &after, None);
    assert!(!repeat
        .facts
        .iter()
        .any(|fact| matches!(fact, OutcomeFact::Goal { .. })));
}

#[test]
fn only_watched_item_handling_is_narrated_and_names_come_from_the_sighting() {
    let before = crate::test_support::observation();
    let row = &before.visible_actors[0];
    let (actor, seen) = (row.id, row.name.clone());
    let after = before.clone();
    let compact = (
        OutcomeObservation::from(&before),
        OutcomeObservation::from(&after),
    );
    let activity = |activity| OutcomeFact::Activity {
        actor,
        started_at: after.tick,
        activity,
    };
    let mut facts = OutcomeFacts::capture(&before, &after, None);
    facts.extend_activities(
        vec![
            activity(ActivityKind::Move {
                from: Some(Position { x: 1, y: 0, z: 0 }),
                to: Some(Position { x: 2, y: 0, z: 0 }),
            }),
            activity(ActivityKind::WindUp {
                posture: ActivityPosture::Melee,
            }),
            activity(ActivityKind::FollowThrough {
                after: ActivityFollowThrough::Strike,
            }),
            activity(ActivityKind::ItemHandling {
                work: ItemWorkKind::Drink,
                name: Some("bubbly potion".into()),
            }),
            activity(ActivityKind::ItemHandling {
                work: ItemWorkKind::Zap,
                name: None,
            }),
        ],
        &compact.0,
        &compact.1,
    );
    facts.validate_at(after.tick).unwrap();
    // One name row, from the event-time sighting, however many facts cite it.
    assert_eq!(
        facts
            .actor_names
            .iter()
            .map(|row| (row.actor, row.name.clone()))
            .collect::<Vec<_>>(),
        [(actor, seen.clone())]
    );
    let lines: Vec<_> = outcome_lines(&facts)
        .into_iter()
        .filter(|line| line.topic == Topic::Activity(actor))
        .map(|line| line.text)
        .collect();
    assert_eq!(
        lines,
        [
            format!("The {seen} drinks the bubbly potion."),
            format!("The {seen} zaps something."),
        ]
    );
}

#[test]
fn user_message_names_controls_through_the_client_and_keeps_text_verbatim() {
    let segments = vec![
        UserMessageSegment::Text("Press ".into()),
        UserMessageSegment::Control(ControlHint::PickUp),
        UserMessageSegment::Text(" to take it, then ".into()),
        UserMessageSegment::Control(ControlHint::OpenDoor),
        UserMessageSegment::Text(".".into()),
    ];
    let facts = OutcomeFacts {
        self_target: ActorTarget::from_digest([1; 32]),
        status: None,
        actor_names: vec![],
        facts: vec![OutcomeFact::UserMessage {
            style: UserMessageStyle::Tutorial,
            segments: segments.clone(),
        }],
    };
    facts.validate().unwrap();
    // Without a client binding the line names controls neutrally.
    assert_eq!(
        outcome_lines(&facts),
        [Line::new(
            Topic::UserMessage {
                style: UserMessageStyle::Tutorial,
                segments: segments.clone(),
            },
            "Press [pick up] to take it, then [open door]."
        )]
    );
    // A client's own names replace every control; nothing else changes.
    let keys = |control: ControlHint| match control {
        ControlHint::PickUp => "`,`".to_owned(),
        ControlHint::OpenDoor => "`o`".to_owned(),
        other => panic!("unexpected control {other:?}"),
    };
    assert_eq!(
        user_message_text(&segments, &keys),
        "Press `,` to take it, then `o`."
    );
    let mixed = OutcomeFacts {
        facts: vec![OutcomeFact::Displaced, facts.facts[0].clone()],
        ..facts.clone()
    };
    assert_eq!(
        serde_json::to_value(user_messages(&outcome_lines(&mixed))).unwrap(),
        serde_json::json!([{
            "style": "tutorial",
            "text": "Press [pick up] to take it, then [open door].",
            "segments": [{"text": "Press "}, {"control": "pick_up"},
                {"text": " to take it, then "}, {"control": "open_door"}, {"text": "."}],
        }])
    );
    for &control in ControlHint::ALL {
        let name = neutral_control_name(control);
        assert!(name.starts_with('[') && name.ends_with(']') && name.len() > 2);
    }
}

#[test]
fn outcome_status_updates_even_when_old_narration_is_silent() {
    let before = crate::test_support::observation();
    let mut after = before.clone();
    let combat = after.combat.as_mut().unwrap();
    combat.recovery_remaining = 19;
    combat.max_hp += 1;
    combat.terminal = true;
    let facts = OutcomeFacts::capture(&before, &after, None);
    facts.validate_at(after.tick).unwrap();
    assert_eq!(
        facts.status,
        after.combat.as_ref().map(OutcomeStatusFacts::from)
    );
    assert_ne!(
        facts.status,
        before.combat.as_ref().map(OutcomeStatusFacts::from)
    );
    assert_eq!(
        outcome_lines(&facts),
        observation_lines_legacy(&before, &after, None)
    );
    assert!(outcome_lines(&facts).is_empty());
}

#[test]
fn outcome_facts_round_trip_without_visual_views_and_retain_order() {
    let before = crate::test_support::portable_light_observation(false);
    let mut after = before.clone();
    after.tick += 1;
    after.ready = false;
    after.visible_actors.clear();
    after.motion = Some(MotionView {
        velocity: [0, 0, 0],
        units_per_cell: 100,
        displaced: true,
        impacted: true,
    });
    let item = before.inventory[0].id;
    let interactions = after.interactions.as_mut().unwrap();
    interactions.completed = vec![Action::Ignite { item }];
    interactions.curse_blocked = vec![item];
    let combat = after.combat.as_mut().unwrap();
    combat.hp -= 1;
    combat.events = vec![CombatEventView::Ability {
        caster: Some(before.self_target),
        target: Some(before.visible_actors[0].id),
        ability: crate::test_support::content("basic_melee"),
        outcome: AbilityOutcome::Applied,
        strike: None,
    }];
    let event = Event::ItemStarted {
        action: Action::Ignite { item },
    };
    let expected = observation_lines_legacy(&before, &after, Some(&event));
    let facts = OutcomeFacts::capture(&before, &after, Some(&event));
    facts.validate_at(after.tick).unwrap();
    let wire = serde_json::to_string(&facts).unwrap();
    assert!(!wire.contains("visible_cells"));
    assert!(!wire.contains("own_stats"));
    assert!(!wire.contains("inventory"));
    drop(before);
    drop(after);
    let decoded: OutcomeFacts = serde_json::from_str(&wire).unwrap();
    decoded.validate().unwrap();
    assert_eq!(outcome_lines(&decoded), expected);
    assert!(outcome_lines(&decoded)
        .iter()
        .any(|line| line.basic_attack.is_some()));
}

#[test]
fn outcome_facts_preserve_own_action_absence_empty_names_and_ground_presentations() {
    let before = crate::test_support::portable_light_observation(false);
    let item = before.inventory[0].id;
    for raw in ["", " ", "\u{1b}", "amulet\nname"] {
        for present in [false, true] {
            let mut after = before.clone();
            after.inventory[0].name = raw.into();
            after.ground_items = vec![GroundItemView {
                item: after.inventory[0].clone(),
                position: after.position,
                presentation: None,
                reachable: true,
                supply_reachable: false,
            }];
            if !present {
                after.inventory.clear();
                after.ground_items.clear();
            }
            for event in [
                Event::Taken {
                    item,
                    result: item,
                    quantity: 2,
                },
                Event::Dropped {
                    item,
                    result: item,
                    quantity: 2,
                },
                Event::Escaped {},
                Event::ItemStarted {
                    action: Action::Ignite { item },
                },
            ] {
                assert_eq!(action(&event, &after), action_legacy(&event, &after));
                assert_eq!(
                    outcome_lines(&OutcomeFacts::capture(&before, &after, Some(&event))),
                    observation_lines_legacy(&before, &after, Some(&event))
                );
            }
        }
    }
}

#[test]
fn outcome_facts_keep_cosmetic_ground_name_fallbacks_client_rendered() {
    let before = crate::test_support::portable_light_observation(false);
    let item = before.inventory[0].id;
    let event = Event::Dropped {
        item,
        result: item,
        quantity: 2,
    };
    for kind in [
        ObjectPresentationKind::Item {
            class: ItemClass::Tool,
        },
        ObjectPresentationKind::Stairs,
    ] {
        for raw in ["", " ", "\u{1b}", "sampled stairs"] {
            let mut after = before.clone();
            after.ground_items = vec![GroundItemView {
                item: before.inventory[0].clone(),
                position: after.position,
                reachable: true,
                supply_reachable: false,
                presentation: Some(ObjectPresentation {
                    name: raw.into(),
                    asset: None,
                    kind,
                }),
            }];
            assert_eq!(action(&event, &after), action_legacy(&event, &after));
            assert_eq!(
                outcome_lines(&OutcomeFacts::capture(&before, &after, Some(&event))),
                observation_lines_legacy(&before, &after, Some(&event))
            );
        }
    }
}

#[test]
fn attire_names_visible_armour_and_weapons_without_control_text() {
    let piece = |slot: &str, class, appearance: &str| WornView {
        slot: slot.into(),
        occurrence: 0,
        class,
        appearance: appearance.into(),
        asset: None,
    };
    let attire = Attire::of(&[
        piece("body_armor", ItemClass::Armor, "ring\u{1b} mail"),
        piece("weapon", ItemClass::Weapon, "mace"),
        piece("weapon", ItemClass::Weapon, "  "),
        piece("amulet", ItemClass::Amulet, "unplaced"),
    ]);
    assert_eq!(attire.wears, ["ring mail"]);
    assert_eq!(attire.holds, ["mace"]);
    assert!(Attire::of(&[]).is_empty());
}

fn observation() -> Observation {
    Observation {
        plane: None,
        public_effects: vec![],
        level_name: None,
        known_item_uses: Vec::new(),
        self_appearance: None,
        interactions: None,
        combat: None,
        motion: None,
        places: Vec::new(),
        actor: ActorId(1),
        self_target: tor_protocol::ActorTarget::from_digest([1; 32]),
        tick: 0,
        position: Position { x: 0, y: 0, z: 0 },
        visible_cells: Vec::new(),
        ground_items: Vec::new(),
        inventory: Vec::new(),
        charts: Vec::new(),
        linked_holds: Vec::new(),
        visible_actors: Vec::new(),
        ready: true,
    }
}

#[test]
fn sightings_are_deduplicated_and_never_claim_hidden_actions() {
    let before = observation();
    let mut after = before.clone();
    let actor = ActorView {
        appearance_seed: None,
        worn: Vec::new(),
        asset: None,
        id: tor_protocol::ActorTarget::from_digest([2; 32]),
        name: "figure".into(),
        description: String::new(),
        position: Position { x: 1, y: 0, z: 0 },
    };
    after.visible_actors = vec![actor.clone(), actor];
    assert_eq!(changes(&before, &after), ["You notice a figure."]);
    assert!(changes(&after, &after).is_empty());
    assert_eq!(
        changes(&after, &before),
        ["You can no longer see the figure."]
    );
    after.visible_actors[0].id = after.self_target;
    after.visible_actors.pop();
    assert!(changes(&before, &after).is_empty());
}

#[test]
fn paid_weak_refusal_has_no_success_or_hidden_identity_and_deduplicates() {
    let before = observation();
    let mut after = before.clone();
    after.tick += 100;
    after.interactions = Some(tor_protocol::InteractionView {
        identification: None,
        completed: vec![],
        curse_blocked: vec![],
        slots: vec![],
        preparation: None,
        inventory: vec![],
        refused: vec![tor_protocol::ItemRefusalView {
            action: tor_protocol::Action::Zap {
                item: tor_protocol::ItemTarget::from_digest([19; 32]),
                aim: tor_protocol::DeviceAim::SelfTarget {},
            },
            reason: tor_protocol::ItemRefusalReason::TooWeak,
        }],
    });
    assert_eq!(
        completed_items(&before, &after),
        ["You are too weak to use it."]
    );
    assert!(completed_items(&after, &after).is_empty());
    let mut next = after.clone();
    next.tick += 100;
    assert_eq!(
        completed_items(&after, &next),
        ["You are too weak to use it."]
    );
}

#[test]
fn paid_curse_failure_is_narrated_without_success_or_hidden_item_name() {
    let before = observation();
    let mut after = before.clone();
    after.tick += 300;
    after.interactions = Some(tor_protocol::InteractionView {
        identification: None,
        completed: vec![],
        refused: vec![],
        curse_blocked: vec![tor_protocol::ItemTarget::from_digest([19; 32])],
        slots: vec![],
        preparation: None,
        inventory: vec![],
    });
    assert_eq!(
        completed_items(&before, &after),
        ["You cannot remove the item; it is cursed."]
    );
    assert!(completed_items(&after, &after).is_empty());
}

#[test]
fn item_preparation_names_only_disclosed_items_and_reports_no_effect_early() {
    let mut view = observation();
    let item = tor_protocol::ItemTarget::from_digest([3; 32]);
    view.inventory.push(tor_protocol::ItemView {
        corpse: None,
        class: tor_protocol::ItemClass::Potion,
        quantity: 2,
        appearance: "red potion".into(),
        identified: false,
        identity: "item.potion.unknown".parse().unwrap(),
        knowledge: Default::default(),
        description: String::new(),
        id: item,
        name: "red potion".into(),
        asset: None,
        known_currency: None,
        known_supply: None,
    });
    let event = Event::ItemStarted {
        action: tor_protocol::Action::Drink { item },
    };
    assert_eq!(action(&event, &view), "You begin to drink the red potion.");
    let carried = view.clone();
    view.inventory.clear();
    assert_eq!(action(&event, &view), "You begin to drink the item.");
    let before = view.clone();
    view.interactions = Some(tor_protocol::InteractionView {
        refused: vec![],
        curse_blocked: vec![],
        identification: None,
        completed: vec![tor_protocol::Action::Drink { item }],
        slots: vec![],
        preparation: None,
        inventory: vec![],
    });
    view.tick = 100;
    assert_eq!(
        completed_items(&before, &view),
        ["You finish drinking the item."]
    );
    assert!(completed_items(&view, &view).is_empty());
    assert_eq!(
        completed_items(&carried, &view),
        ["You finish drinking the red potion."]
    );
}

#[test]
fn doors_require_consecutive_sight_and_do_not_name_a_cause() {
    let mut before = observation();
    before.visible_cells.push(CellView {
        object_presentation: None,
        lit: true,
        key: "seen".into(),
        position: Position { x: 1, y: 0, z: 0 },
        wall: false,
        material: "stone".into(),
        place_hint: false,
        stairs_up: false,
        stairs_down: false,
        asset: None,
        trap: None,
        door: Some(DoorView {
            id: tor_protocol::DoorTarget::from_digest([3; 32]),
            name: "iron gate".into(),
            description: String::new(),
            open: false,
            reachable: true,
            approaches: Vec::new(),
            asset: None,
        }),
    });
    let mut after = before.clone();
    after.visible_cells[0].door.as_mut().unwrap().open = true;
    assert_eq!(changes(&before, &after), ["The iron gate is now open."]);
    assert!(changes(&observation(), &after).is_empty());
    assert!(changes(&after, &observation()).is_empty());
    assert_eq!(
        super::observation(
            &before,
            &after,
            Some(&Event::DoorChanged {
                door: tor_protocol::DoorTarget::from_digest([3; 32]),
                open: true
            })
        ),
        ["You open the iron gate."]
    );
}

#[test]
fn taking_the_exit_claims_nothing_about_an_item_or_a_victory() {
    assert_eq!(
        action(&Event::Escaped {}, &observation()),
        "You take the way out."
    );
}

#[test]
fn crumbled_pickup_never_claims_inventory_transfer_or_invents_identity() {
    let view = observation();
    for (quantity, expected) in [
        (1, "The item crumbles as you try to pick it up."),
        (2, "2 items crumble as you try to pick them up."),
    ] {
        assert_eq!(
            action(
                &Event::Crumbled {
                    item: tor_protocol::ItemTarget::from_digest([9; 32]),
                    quantity
                },
                &view
            ),
            expected
        );
    }
}

#[test]
fn action_names_are_disclosed_and_control_characters_are_removed() {
    let mut view = observation();
    assert_eq!(
        action(
            &Event::Taken {
                item: tor_protocol::ItemTarget::from_digest([9; 32]),
                result: tor_protocol::ItemTarget::from_digest([9; 32]),
                quantity: 1
            },
            &view
        ),
        "You pick up the item."
    );
    view.inventory.push(ItemView {
        corpse: None,
        class: Default::default(),
        asset: None,
        quantity: 1,
        appearance: String::new(),
        identified: true,
        identity: "item.misc.fixture".parse().unwrap(),
        knowledge: Default::default(),
        id: tor_protocol::ItemTarget::from_digest([9; 32]),
        name: "copper\ntoken".into(),
        description: String::new(),
        known_currency: None,
        known_supply: None,
    });
    assert_eq!(
        action(
            &Event::Taken {
                item: tor_protocol::ItemTarget::from_digest([9; 32]),
                result: tor_protocol::ItemTarget::from_digest([9; 32]),
                quantity: 1
            },
            &view
        ),
        "You pick up the coppertoken."
    );
    assert_eq!(
        action(
            &Event::DoorChanged {
                door: tor_protocol::DoorTarget::from_digest([77; 32]),
                open: false
            },
            &view
        ),
        "You close the door."
    );
}

#[test]
fn ability_lines_describe_only_disclosed_participants_and_qualitative_results() {
    let view = observation();
    assert_eq!(
        action(
            &Event::AbilityStarted {
                ability: crate::test_support::content("fear"),
                target: view.self_target
            },
            &view
        ),
        "You prepare to use Fear."
    );
    assert_eq!(
        combat_event(
            &CombatEventView::Ability {
                strike: None,
                caster: None,
                target: Some(view.self_target),
                ability: crate::test_support::content("fear"),
                outcome: AbilityOutcome::Applied,
            },
            &view,
            &view
        ),
        "Something used Fear on you."
    );
    assert_eq!(
        combat_event(
            &CombatEventView::Ability {
                strike: None,
                caster: Some(view.self_target),
                target: None,
                ability: crate::test_support::content("power_strike"),
                outcome: AbilityOutcome::Unaffected,
            },
            &view,
            &view
        ),
        "You used Power Strike on something."
    );
    assert_eq!(
        combat_event(
            &CombatEventView::Ability {
                strike: None,
                caster: Some(view.self_target),
                target: None,
                ability: crate::test_support::content("magic_bolt"),
                outcome: AbilityOutcome::Miss,
            },
            &view,
            &view
        ),
        "You used Magic Bolt on something, but missed."
    );
}

#[test]
fn combat_lines_are_written_from_events_and_disclosed_names() {
    let mut view = observation();
    view.visible_actors.push(ActorView {
        appearance_seed: None,
        worn: Vec::new(),
        asset: None,
        id: tor_protocol::ActorTarget::from_digest([2; 32]),
        name: "ruin scout".into(),
        description: String::new(),
        position: Position { x: 1, y: 0, z: 0 },
    });
    let after = observation();
    let line = |event| combat_event(&event, &view, &after);
    assert_eq!(
        line(CombatEventView::Ability {
            strike: None,
            ability: crate::test_support::content("basic_melee"),
            caster: Some(tor_protocol::ActorTarget::from_digest([1; 32])),
            target: Some(tor_protocol::ActorTarget::from_digest([2; 32])),
            outcome: AbilityOutcome::Applied,
        }),
        "You used Basic Melee on the ruin scout."
    );
    assert_eq!(
        line(CombatEventView::Ability {
            strike: None,
            ability: crate::test_support::content("basic_melee"),
            caster: None,
            target: Some(tor_protocol::ActorTarget::from_digest([1; 32])),
            outcome: AbilityOutcome::Miss,
        }),
        "Something used Basic Melee on you, but missed."
    );
    assert_eq!(
        line(CombatEventView::Died {
            actor: tor_protocol::ActorTarget::from_digest([2; 32]),
            cause: DeathCause::Injury,
        }),
        "The ruin scout died."
    );
    assert_eq!(
        line(CombatEventView::Died {
            actor: tor_protocol::ActorTarget::from_digest([2; 32]),
            cause: DeathCause::Starvation,
        }),
        "The ruin scout starved to death."
    );
    assert_eq!(
        line(CombatEventView::Died {
            actor: tor_protocol::ActorTarget::from_digest([1; 32]),
            cause: DeathCause::Starvation,
        }),
        "You starved to death."
    );
    assert_eq!(
        goal_text(
            &[
                UserMessageSegment::Text("Use ".into()),
                UserMessageSegment::Control(ControlHint::OpenDoor),
            ],
            &neutral_control_name
        ),
        "Use [open door]"
    );
    assert_eq!(injury(Injury::BadlyWounded), "badly wounded");
}

#[test]
fn readiness_changes_are_announced_without_inventing_an_unseen_action() {
    let before = observation();
    let mut waiting = before.clone();
    waiting.ready = false;
    assert_eq!(changes(&before, &waiting), ["You must wait."]);
    assert_eq!(changes(&waiting, &before), ["You can act again."]);
}
#[test]
fn terminal_outcomes_do_not_instruct_the_player_to_wait() {
    let before = observation();
    for dead in [false, true] {
        let mut after = before.clone();
        after.ready = false;
        after.combat = Some(CombatView {
            public_plan: None,
            sequence: None,
            own_stats: None,
            hp: if dead { 0 } else { 30 },
            max_hp: 30,
            preparation_remaining: None,
            preparation_active: false,
            recovery_remaining: 0,
            actors: vec![],
            events: vec![],
            goal: None,
            victory: !dead,
            dead,
            terminal: true,
        });
        let text = super::observation(&before, &after, None).join(" ");
        assert!(!text.contains("must wait"));
        assert!(text.contains(if dead { "You died" } else { "Victory" }));
    }
}

#[test]
fn every_line_says_what_it_reports_and_matches_the_plain_text() {
    let scout = tor_protocol::ActorTarget::from_digest([2; 32]);
    let me = tor_protocol::ActorTarget::from_digest([1; 32]);
    let mut before = observation();
    before.ready = false;
    before.visible_actors.push(ActorView {
        appearance_seed: None,
        worn: Vec::new(),
        asset: None,
        id: scout,
        name: "ruin scout".into(),
        description: String::new(),
        position: Position { x: 1, y: 0, z: 0 },
    });
    let mut after = observation();
    let hit = CombatEventView::Ability {
        strike: None,
        ability: crate::test_support::content("basic_melee"),
        caster: Some(me),
        target: Some(scout),
        outcome: AbilityOutcome::Applied,
    };
    after.combat = Some(CombatView {
        public_plan: None,
        sequence: None,
        own_stats: None,
        hp: 28,
        max_hp: 30,
        preparation_remaining: None,
        preparation_active: false,
        recovery_remaining: 0,
        actors: vec![],
        events: vec![
            hit.clone(),
            CombatEventView::Died {
                actor: scout,
                cause: DeathCause::Injury,
            },
        ],
        goal: None,
        victory: false,
        dead: false,
        terminal: false,
    });
    let event = Event::Moved {
        direction: Direction::East,
    };
    let lines = observation_lines(&before, &after, Some(&event));
    let topics: Vec<_> = lines.iter().map(|line| line.topic.clone()).collect();
    assert_eq!(
        topics,
        [
            Topic::Own(event.clone()),
            Topic::LostSight(scout),
            Topic::Pacing,
            Topic::Combat(hit),
            Topic::Combat(CombatEventView::Died {
                actor: scout,
                cause: DeathCause::Injury,
            }),
            Topic::Status,
        ]
    );
    assert_eq!(
        lines
            .iter()
            .map(|line| line.text.clone())
            .collect::<Vec<_>>(),
        super::observation(&before, &after, Some(&event))
    );
    assert_eq!(lines[3].text, "You used Basic Melee on the ruin scout.");
}

#[test]
fn compact_basic_attacks_require_the_current_disclosed_flag() {
    let mut before = crate::test_support::observation();
    let granted = &mut before
        .combat
        .as_mut()
        .unwrap()
        .own_stats
        .as_mut()
        .unwrap()
        .abilities[0];
    granted.id = "custom.attack".into();
    granted.name = "Disclosed attack".into();
    let ability = granted.content();
    let target = before.visible_actors[0].id;
    let start = Event::AbilityStarted {
        ability: ability.clone(),
        target,
    };
    let event = CombatEventView::Ability {
        caster: Some(before.self_target),
        target: Some(target),
        ability,
        outcome: AbilityOutcome::Applied,
        strike: None,
    };
    for (current_flag, expected) in [(true, true), (false, false)] {
        let mut after = before.clone();
        after.tick += 1;
        let combat = after.combat.as_mut().unwrap();
        combat.own_stats.as_mut().unwrap().abilities[0].basic_attack = current_flag;
        combat.events = vec![event.clone()];
        let lines = observation_lines(&before, &after, Some(&start));
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].text, "You prepare to use Disclosed attack.");
        assert_eq!(lines[1].text, "You used Disclosed attack on the sentinel.");
        assert_eq!(
            lines[0].basic_attack,
            expected.then_some(BasicAttack::Preparing)
        );
        assert_eq!(
            lines[1].basic_attack,
            expected.then_some(BasicAttack::Resolved {
                target: "the sentinel".into(),
                outcome: AbilityOutcome::Applied,
            })
        );
    }
}

#[test]
fn ambient_health_updates_do_not_add_a_recovery_countdown() {
    let before = crate::test_support::observation();
    let mut after = before.clone();
    after.tick += 1;
    let combat = after.combat.as_mut().unwrap();
    combat.hp -= 1;
    combat.recovery_remaining = 37;
    assert!(
        combat_status(combat).contains("37 ticks"),
        "explicit combat report"
    );
    let lines = observation_lines(&before, &after, None);
    let line = lines
        .iter()
        .find(|line| line.topic == Topic::Status)
        .unwrap();
    assert_eq!(line.text, "HP 11/12");
}

#[test]
fn unaffected_primary_keeps_neutral_narration_with_visible_or_concealed_riders() {
    // The ordinary event deliberately contains no rider outcome. A fully
    // reduced primary must use the same wording for a visible push, a blocked
    // push, no rider, or a condition rider whose result is not disclosed.
    for (ability, displaced) in [
        ("dislocating_strike", true),
        ("dislocating_strike", false),
        ("basic_melee", false),
        ("rattling_blow", false),
    ] {
        let before = crate::test_support::observation();
        let mut after = before.clone();
        after.tick = 110;
        after.motion = Some(MotionView {
            velocity: [0, 0, 0],
            units_per_cell: 100,
            displaced,
            impacted: false,
        });
        if displaced {
            after.position.x = 1;
        }
        let event = CombatEventView::Ability {
            strike: None,
            caster: Some(ActorTarget::from_digest([2; 32])),
            target: Some(after.self_target),
            ability: crate::test_support::content(ability),
            outcome: AbilityOutcome::Unaffected,
        };
        after.combat.as_mut().unwrap().events = vec![event.clone()];
        let expected = if ability == "basic_melee" {
            "The sentinel attacks you.".to_owned()
        } else {
            format!(
                "The sentinel used {} on you.",
                crate::test_support::content(ability).name
            )
        };
        assert_eq!(combat_event(&event, &before, &after), expected);
        let lines = observation_lines(&before, &after, None);
        assert!(lines
            .iter()
            .any(|line| line.topic == Topic::Combat(event.clone()) && line.text == expected));
        assert_eq!(
            lines
                .iter()
                .filter(|line| line.topic == Topic::Motion)
                .count(),
            usize::from(displaced)
        );
        assert!(!lines.iter().any(|line| line.text.contains("no effect")));
        assert!(matches!(
            &after.combat.as_ref().unwrap().events[0],
            CombatEventView::Ability {
                outcome: AbilityOutcome::Unaffected,
                ..
            }
        ));
    }
}

#[test]
fn anonymous_effect_feedback_stays_neutral_and_has_no_basic_attack_identity() {
    let before = crate::test_support::observation();
    let mut after = before.clone();
    let visible = after.visible_actors[0].id;
    for (caster, target, phrase) in [
        (
            Some(after.self_target),
            Some(visible),
            "You used an unidentified effect on the sentinel",
        ),
        (
            Some(visible),
            Some(after.self_target),
            "The sentinel used an unidentified effect on you",
        ),
        (
            None,
            Some(after.self_target),
            "Something used an unidentified effect on you",
        ),
        (
            Some(after.self_target),
            None,
            "You used an unidentified effect on something",
        ),
    ] {
        for outcome in [
            AbilityOutcome::Applied,
            AbilityOutcome::Unaffected,
            AbilityOutcome::Miss,
        ] {
            let event = CombatEventView::AnonymousAbility {
                caster,
                target,
                outcome,
            };
            after.combat.as_mut().unwrap().events = vec![event.clone()];
            let expected = if outcome == AbilityOutcome::Miss {
                format!("{phrase}, but missed.")
            } else {
                format!("{phrase}.")
            };
            assert_eq!(combat_event(&event, &before, &after), expected);
            let lines = observation_lines(&before, &after, None);
            let line = lines
                .iter()
                .find(|line| line.topic == Topic::Combat(event.clone()))
                .unwrap();
            assert_eq!(line.text, expected);
            assert_eq!(line.basic_attack, None);
            assert!(!line.text.contains("attacks"));
            assert!(!line.text.contains("wand"));
            assert!(!line.text.contains("spell"));
            assert!(!line.text.contains("no effect"));
        }
    }
    let event = CombatEventView::AnonymousAbility {
        caster: None,
        target: Some(after.self_target),
        outcome: AbilityOutcome::Applied,
    };
    after.visible_actors[0].name = "later identified figure".into();
    assert_eq!(
        combat_event(&event, &before, &after),
        "Something used an unidentified effect on you."
    );
}

#[test]
fn rattling_blow_reports_contact_without_inventing_rattled() {
    let before = crate::test_support::observation();
    let mut after = before.clone();
    let caster = after.visible_actors[0].id;
    for name in ["sentinel", "painted phantom"] {
        // A changed apparent name never changes the outcome.
        after.visible_actors[0].name = name.into();
        for (outcome, expected) in [
            (
                AbilityOutcome::Applied,
                format!("The {name} hits you with Rattling Blow."),
            ),
            (
                AbilityOutcome::Miss,
                format!("The {name} used Rattling Blow on you, but missed."),
            ),
            (
                AbilityOutcome::Unaffected,
                format!("The {name} used Rattling Blow on you."),
            ),
        ] {
            let event = CombatEventView::Ability {
                caster: Some(caster),
                target: Some(after.self_target),
                ability: tor_protocol::ContentRef {
                    id: "rattling_blow".into(),
                    name: "Rattling Blow".into(),
                },
                outcome,
                strike: None,
            };
            assert_eq!(combat_event(&event, &before, &after), expected);
        }
    }
}

#[test]
fn unseen_attack_history_never_acquires_a_later_visible_name() {
    let before = crate::test_support::observation();
    let mut after = before.clone();
    after.visible_actors[0].name = "identified phantom".into();
    for caster in [None, Some(after.visible_actors[0].id)] {
        for (outcome, expected) in [
            (AbilityOutcome::Applied, "Something attacks you."),
            (AbilityOutcome::Unaffected, "Something attacks you."),
            (AbilityOutcome::Miss, "Something attacks you, but misses."),
        ] {
            let event = CombatEventView::UnseenAbility {
                caster,
                target: after.self_target,
                outcome,
            };
            after.combat.as_mut().unwrap().events = vec![event.clone()];
            assert_eq!(combat_event(&event, &before, &after), expected);
            let lines = observation_lines(&before, &after, None);
            assert!(lines.iter().any(|line| {
                line.topic == Topic::Combat(event.clone()) && line.text == expected
            }));
        }
    }
}
