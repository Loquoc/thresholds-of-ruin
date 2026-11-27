use super::*;

#[test]
fn unnamed_noticed_outcome_uses_generic_figure_without_a_new_picture() {
    let before = view();
    let actor = ActorTarget::from_digest([91; 32]);
    let mut chronicler = Chronicler::default();
    for name in ["guard", "", "   "] {
        let facts = OutcomeFacts {
            self_target: before.observation.self_target,
            status: None,
            actor_names: Vec::new(),
            facts: vec![OutcomeFact::Noticed {
                actor,
                name: name.into(),
                asset: None,
                position: Position { x: 1, y: 0, z: 0 },
            }],
        };
        let beats = chronicler.outcome(&facts, &Palette::default());
        assert!(
            matches!(beats.as_slice(), [Beat::Appeared { figure, whereabouts }]
                if figure.id == actor
                    && figure.name == (if name.trim().is_empty() { "figure" } else { name })
                    && whereabouts == "to the east")
        );
    }
}

#[test]
fn event_time_actor_assets_keep_legacy_palette_names_without_a_later_picture() {
    let before = view();
    let mut after = before.clone();
    let actor = &mut after.observation.visible_actors[0];
    actor.id = ActorTarget::from_digest([99; 32]);
    actor.name.clear();
    actor.asset = Some("creature.jabberwock".into());
    let mut palette = Palette::default();
    palette.apply(&PaletteUpdate {
        revision: 1,
        body: PaletteBody::Full {
            assets: ["creature.jabberwock".into()].into(),
        },
    });
    let facts = OutcomeFacts::capture(&before.observation, &after.observation, None);
    let expected = Chronicler::default().observe(&before, &after, None, &palette);
    after.observation.visible_actors.clear();
    assert_eq!(Chronicler::default().outcome(&facts, &palette), expected);
    let unnamed = Chronicler::default().outcome(&facts, &Palette::default());
    assert!(unnamed
        .iter()
        .any(|beat| matches!(beat, Beat::Appeared { figure, .. } if figure.name == "figure")));
    assert_eq!(
        disclosed_figure_name("Ruin Scout", Some("creature.jabberwock"), &palette),
        "ruin scout"
    );
}

#[test]
fn current_anonymous_appearance_and_prior_common_name_keep_distinct_policies() {
    let mut before = view();
    before.observation.visible_actors[0].name = "Earlier Guard".into();
    let subject = before.observation.visible_actors[0].id;
    let mut after = before.clone();
    after.observation.visible_actors[0].name.clear();
    after.observation.visible_actors[0].asset = Some("creature.jabberwock".into());
    after.observation.tick += 1;
    after.observation.combat.as_mut().unwrap().events = vec![CombatEventView::Ability {
        caster: Some(subject),
        target: Some(after.observation.self_target),
        ability: ContentRef {
            id: "strike".into(),
            name: "Strike".into(),
        },
        outcome: AbilityOutcome::Applied,
        strike: None,
    }];
    let mut palette = Palette::default();
    palette.apply(&PaletteUpdate {
        revision: 1,
        body: PaletteBody::Full {
            assets: ["creature.jabberwock".into()].into(),
        },
    });
    let facts = OutcomeFacts::capture(&before.observation, &after.observation, None);
    assert_eq!(facts.actor_names[0].name, "Earlier Guard");
    assert!(facts.actor_names[0]
        .current_disclosed_appearance
        .as_ref()
        .unwrap()
        .name
        .is_empty());
    assert!(tor_client_common::narration::outcome_lines(&facts)
        .iter()
        .any(|line| line.text.contains("Earlier Guard")));
    let expected = Chronicler::default().observe(&before, &after, None, &palette);
    assert_eq!(Chronicler::default().outcome(&facts, &palette), expected);
}

#[test]
fn snapshot_reseeds_hp_but_ordinary_pictures_do_not_overwrite_outcomes() {
    let initial = view();
    let mut hurt = initial.clone();
    hurt.observation.combat.as_mut().unwrap().hp = 40;
    let mut chronicler = Chronicler::default();
    chronicler.learn(&initial, &Palette::default());
    chronicler.outcome(
        &OutcomeFacts::capture(&initial.observation, &hurt.observation, None),
        &Palette::default(),
    );
    chronicler.learn(&initial, &Palette::default());
    assert_eq!(chronicler.status.as_ref().unwrap().hp, 40);
    chronicler.reset_baseline(&initial, &Palette::default());
    let beats = chronicler.outcome(
        &OutcomeFacts::capture(&initial.observation, &hurt.observation, None),
        &Palette::default(),
    );
    assert!(beats.contains(&Beat::Hp {
        from: 50,
        to: 40,
        max: 50
    }));
}

#[test]
fn item_refusal_sentences_keep_finished_beats_before_discoveries() {
    let facts = OutcomeFacts {
        self_target: view().observation.self_target,
        status: None,
        actor_names: vec![],
        facts: vec![
            OutcomeFact::TrapDiscovered { family: None },
            OutcomeFact::ItemRefused {
                reason: ItemRefusalReason::TooWeak,
            },
            OutcomeFact::CurseBlocked {
                name: Some("mail".into()),
            },
        ],
    };
    assert_eq!(
        Chronicler::default().outcome(&facts, &Palette::default()),
        vec![
            Beat::ItemFinished("You are too weak to use it.".into()),
            Beat::ItemFinished("You cannot remove the mail; it is cursed.".into()),
            Beat::Discovery("You notice a trap.".into()),
        ]
    );
}

#[test]
fn own_action_names_keep_text_case_blank_and_cosmetic_fallback_rules() {
    let item = ItemTarget::from_digest([84; 32]);
    let taken = Event::Taken {
        item,
        result: item,
        quantity: 2,
    };
    let dropped = Event::Dropped {
        item,
        result: item,
        quantity: 2,
    };
    let door = Event::DoorChanged {
        door: DoorTarget::from_digest([85; 32]),
        open: true,
    };
    let name = |text: &str, fallback| OutcomeName {
        text: text.into(),
        fallback,
    };
    assert_eq!(
        own_action_name(
            &taken,
            Some(&name("Copper Token", OutcomeNameFallback::Item))
        ),
        "copper token"
    );
    assert_eq!(
        own_action_name(&taken, Some(&name(" ", OutcomeNameFallback::Item))),
        "thing"
    );
    assert_eq!(own_action_name(&door, None), "door");
    assert_eq!(
        own_action_name(&door, Some(&name("Oak Door", OutcomeNameFallback::Door))),
        "oak door"
    );
    assert_eq!(
        own_action_name(&dropped, Some(&name("", OutcomeNameFallback::Stairs))),
        "stairs"
    );
    assert_eq!(
        own_action_name(&dropped, Some(&name("", OutcomeNameFallback::Object))),
        "object"
    );
    assert_eq!(
        own_action_name(
            &dropped,
            Some(&name("Silver Flask", OutcomeNameFallback::Item))
        ),
        "Silver Flask"
    );
}

#[test]
fn user_messages_are_told_on_their_own_line_with_text_commands() {
    let facts = OutcomeFacts {
        self_target: view().observation.self_target,
        status: None,
        actor_names: vec![],
        facts: vec![
            OutcomeFact::UserMessage {
                style: UserMessageStyle::Tutorial,
                segments: vec![
                    UserMessageSegment::Text("Type ".into()),
                    UserMessageSegment::Control(ControlHint::PickUp),
                    UserMessageSegment::Text(" to pick it up, or ".into()),
                    UserMessageSegment::Control(ControlHint::Move),
                    UserMessageSegment::Text(" to walk on.".into()),
                ],
            },
            OutcomeFact::Displaced,
            OutcomeFact::UserMessage {
                style: UserMessageStyle::Narration,
                segments: vec![UserMessageSegment::Text("The wind drops.".into())],
            },
            OutcomeFact::UserMessage {
                style: UserMessageStyle::Hint,
                segments: vec![
                    UserMessageSegment::Control(ControlHint::Help),
                    UserMessageSegment::Text(" lists every command.".into()),
                ],
            },
        ],
    };
    facts.validate().unwrap();
    let beats = Chronicler::default().outcome(&facts, &Palette::default());
    let passage = super::super::narrate::compose(&super::super::narrate::Record {
        entries: vec![super::super::narrate::Entry::Beats(beats)],
    });
    let lines: Vec<_> = passage.lines().collect();
    assert_eq!(lines.len(), 4, "{passage}");
    assert!(lines[0].to_lowercase().contains("moved against your will"));
    assert_eq!(
        &lines[1..],
        [
            r#"Tutorial: Type "take" to pick it up, or a direction such as "north" to walk on."#,
            "The wind drops.",
            r#"Hint: "help" lists every command."#,
        ]
    );
    for &control in ControlHint::ALL {
        assert!(!crate::controls::command(control).is_empty());
    }
}

#[test]
fn ordered_outcomes_narrate_without_refreshing_the_visual_view() {
    let before = view();
    let mut after = before.clone();
    after.observation.tick += 1;
    after.observation.combat.as_mut().unwrap().hp = 40;
    let event = Event::Moved {
        direction: Direction::North,
    };
    let facts = OutcomeFacts::capture(&before.observation, &after.observation, Some(&event));
    let mut chronicler = Chronicler::default();
    chronicler.learn(&before, &Palette::default());
    let beats = chronicler.outcome(&facts, &Palette::default());
    assert!(beats.contains(&Beat::Stepped(Direction::North)));
    assert!(beats.contains(&Beat::Hp {
        from: 50,
        to: 40,
        max: 50
    }));
    assert_eq!(before.observation.combat.as_ref().unwrap().hp, 50);
}

#[test]
fn outcome_keeps_legacy_combat_wounds_plan_progress_and_objective_order() {
    let mut before = view();
    let subject = before.observation.visible_actors[0].id;
    before
        .observation
        .combat
        .as_mut()
        .unwrap()
        .actors
        .push(CombatActorView {
            actor: subject,
            hostile: true,
            allied: false,
            injury: Injury::Healthy,
        });
    let mut after = before.clone();
    after.observation.tick += 1;
    let me = after.observation.self_target;
    let tick = after.observation.tick;
    let combat = after.observation.combat.as_mut().unwrap();
    combat.events.push(CombatEventView::Ability {
        caster: Some(me),
        target: Some(subject),
        ability: ContentRef {
            id: "test.strike".into(),
            name: "Strike".into(),
        },
        outcome: AbilityOutcome::Applied,
        strike: Some(1),
    });
    combat.actors[0].injury = Injury::Wounded;
    combat.public_plan = Some(PublicPlanObservationView {
        observed_at: tick,
        subject_name: "remembered guard".into(),
        state: PublicPlanStateView::Preparing {
            ability: None,
            aim: ObservedAimView::Unavailable,
            remaining_work: 200,
        },
    });
    combat.sequence = Some(SequenceView {
        completed: 1,
        total: 3,
        next_strike_remaining: 5,
        cancel_recovery_remaining: 35,
    });
    combat.preparation_active = true;
    combat.hp = 40;
    combat.victory = true;
    combat.goal = Some(tor_protocol::GoalView {
        segments: vec![tor_protocol::UserMessageSegment::Text(
            "Bring the seal back to the exit.".into(),
        )],
        cell: None,
    });
    let facts = OutcomeFacts::capture(&before.observation, &after.observation, None);
    let expected = Chronicler::default().observe(&before, &after, None, &Palette::default());
    let mut current = Chronicler::default();
    current.learn(&before, &Palette::default());
    let actual = current.outcome(&facts, &Palette::default());
    assert_eq!(actual, expected);
    assert!(matches!(
        actual.as_slice(),
        [
            Beat::AbilityResolved { .. },
            Beat::Injured { .. },
            Beat::PublicPlan(_),
            Beat::SequenceStatus(_),
            Beat::Hp { .. },
            Beat::Victory { .. },
            Beat::Goal(_),
        ]
    ));
}

fn view() -> StateView {
    serde_json::from_value(serde_json::json!({
            "snapshot":"chronicle-fixture","wizard_game":false,"revision":"1","observation":{
            "actor":"1","self_target":tor_protocol::ActorTarget::from_digest([1; 32]),"tick":"5","position":{"x":0,"y":0,"z":0},"ready":true,
            "places":[],"visible_cells":[{"key":"here","position":{"x":0,"y":0,"z":0},
                "wall":false,"material":"stone","lit":true,"place_hint":false,
                "trap":null,"object_presentation":null,"stairs_up":false,"stairs_down":false,
                "door":{"id":tor_protocol::DoorTarget::from_digest([7; 32]),"name":"Oak Door","description":"","open":false,
                    "reachable":true,"approaches":[]}}],
            "ground_items":[],"inventory":[],
            "charts":[],"linked_holds":[],"public_effects":[],"visible_actors":[{"id":tor_protocol::ActorTarget::from_digest([2; 32]),"name":"ruin scout","description":"",
                "position":{"x":2,"y":0,"z":0}}],
            "combat":{"public_plan":null,"hp":"50","max_hp":"50","sequence":null,"preparation_remaining":null,
                "preparation_active":false,"recovery_remaining":"0","actors":[],
                "events":[],"victory":false,"dead":false,
                "terminal":false}}}))
        .unwrap()
}

fn action(event: Event) -> HistoryEntry {
    HistoryEntry {
        narration: vec![],
        id: EntryId("e".into()),
        branch: BranchId("main".into()),
        actor: ActorId(1),
        tick: 6,
        author: Author::Backend {
            component: "test".into(),
        },
        audience: Audience::Actor,
        content: HistoryContent::Action {
            action: Action::Wait,
            event: Box::new(event),
        },
    }
}

#[test]
fn anonymous_effect_feedback_never_becomes_a_named_ability_beat() {
    let before = view();
    let mut after = before.clone();
    after.observation.tick += 1;
    let target = after.observation.visible_actors[0].id;
    after.observation.combat.as_mut().unwrap().events = vec![CombatEventView::AnonymousAbility {
        caster: Some(after.observation.self_target),
        target: Some(target),
        outcome: AbilityOutcome::Unaffected,
    }];
    let beats = Chronicler::default().observe(&before, &after, None, &Palette::default());
    assert!(beats.contains(&Beat::EscapeHold(
        "You used an unidentified effect on the ruin scout.".into()
    )));
    assert!(!beats
        .iter()
        .any(|beat| matches!(beat, Beat::AbilityResolved { .. })));
}

#[test]
fn ability_starts_and_effects_become_distinct_narrative_beats() {
    let before = view();
    let mut after = view();
    after.observation.tick = 6;
    let target = after.observation.visible_actors[0].id;
    after.observation.combat.as_mut().unwrap().events = vec![CombatEventView::Ability {
        strike: None,
        caster: Some(after.observation.self_target),
        target: Some(target),
        ability: tor_client_common::test_support::content("fear"),
        outcome: AbilityOutcome::Applied,
    }];
    let entry = action(Event::AbilityStarted {
        ability: tor_client_common::test_support::content("fear"),
        target,
    });
    let beats = Chronicler::default().observe(&before, &after, Some(&entry), &Palette::default());
    assert!(beats.contains(&Beat::AbilityBegan("You prepare to use Fear.".into())));
    assert!(beats
        .iter()
        .any(|beat| matches!(beat,Beat::AbilityResolved{ability,..} if ability.id=="fear")));
}

fn armed_view() -> StateView {
    let mut state = view();
    state.observation.combat.as_mut().unwrap().own_stats =
        Some(tor_client_common::test_support::stats());
    let item = ItemTarget::from_digest([8; 32]);
    state.observation.inventory.push(
            serde_json::from_value(serde_json::json!({
                "known_currency":null,"known_supply":null,"quantity":"1","class":"weapon","appearance":"item",
                "identified":true,"identity":"item.weapon.fixture","knowledge":tor_protocol::ItemKnowledge::default(),"id":item,"name":"iron sword","description":""
            }))
            .unwrap(),
        );
    let slot = tor_client_common::test_support::equipment_slot("hand", "in hand", true);
    state.observation.interactions = Some(InteractionView {
        refused: vec![],
        curse_blocked: vec![],
        identification: None,
        completed: vec![],
        slots: vec![slot.clone()],
        preparation: None,
        inventory: vec![ItemInteractionView {
            item,
            slot: Some(slot),
            equipped_slot: Some(0),
            known_cursed: None,
            known_protected: None,
            known_projectile_bonus: None,
            known_equipment: None,
            drinkable: false,
            edible: false,
            readable: false,
            zappable: false,
            portable_light: None,
            device_activation: None,
            charges: None,
        }],
    });
    state
}

#[test]
fn outcome_preparation_prose_uses_disclosed_basic_flag_not_ability_name() {
    for basic_attack in [false, true] {
        let mut before = armed_view();
        let mut disclosed = tor_client_common::test_support::ability("custom.technique");
        disclosed.name = "Renamed Technique".into();
        disclosed.basic_attack = basic_attack;
        before
            .observation
            .combat
            .as_mut()
            .unwrap()
            .own_stats
            .as_mut()
            .unwrap()
            .abilities = vec![disclosed];
        let mut after = before.clone();
        after.observation.tick += 1;
        let event = Event::AbilityStarted {
            ability: ContentRef {
                id: "custom.technique".into(),
                name: "Renamed Technique".into(),
            },
            target: after.observation.visible_actors[0].id,
        };
        let entry = action(event.clone());
        let expected =
            Chronicler::default().observe(&before, &after, Some(&entry), &Palette::default());
        let facts = OutcomeFacts::capture(&before.observation, &after.observation, Some(&event));
        let actual = Chronicler::default().outcome(&facts, &Palette::default());
        assert_eq!(actual, expected);
        assert_eq!(
            actual,
            vec![Beat::AbilityBegan(
                if basic_attack {
                    "You ready yourself to strike."
                } else {
                    "You prepare to use Renamed Technique."
                }
                .into()
            )]
        );
    }
}

#[test]
fn attack_names_and_ids_do_not_override_own_source_semantics() {
    for (id, name) in [("technique", "Attack"), ("basic_melee", "Rattling Blow")] {
        let before = armed_view();
        let mut after = before.clone();
        after.observation.tick += 1;
        let ability = ContentRef {
            id: id.into(),
            name: name.into(),
        };
        let combat = after.observation.combat.as_mut().unwrap();
        let mut disclosed = tor_client_common::test_support::ability(id);
        disclosed.name = name.into();
        disclosed.basic_attack = false;
        combat.own_stats.as_mut().unwrap().abilities = vec![disclosed];
        combat.events = vec![CombatEventView::Ability {
            caster: Some(after.observation.self_target),
            target: Some(after.observation.visible_actors[0].id),
            ability: ability.clone(),
            outcome: AbilityOutcome::Applied,
            strike: None,
        }];
        let entry = action(Event::AbilityStarted {
            ability: ability.clone(),
            target: after.observation.visible_actors[0].id,
        });
        let beats =
            Chronicler::default().observe(&before, &after, Some(&entry), &Palette::default());
        assert!(
            beats.contains(&Beat::AbilityBegan(format!("You prepare to use {name}."))),
            "{beats:?}"
        );
        assert!(
            beats.iter().any(|beat| matches!(beat,
                Beat::AbilityResolved { basic: false, ability: named, .. } if *named == ability
            )),
            "{beats:?}"
        );
    }
}

#[test]
fn a_basic_flag_classifies_only_the_disclosed_source_without_binding_its_weapon() {
    for (caster_is_me, disclose_stats, expected_basic) in [
        (true, true, true),
        (true, false, false),
        (false, true, false),
        (false, false, false),
    ] {
        let before = armed_view();
        let mut after = before.clone();
        after.observation.tick += 1;
        let caster = if caster_is_me {
            after.observation.self_target
        } else {
            after.observation.visible_actors[0].id
        };
        let combat = after.observation.combat.as_mut().unwrap();
        if !disclose_stats {
            combat.own_stats = None;
        }
        combat.events = vec![CombatEventView::Ability {
            caster: Some(caster),
            target: Some(after.observation.self_target),
            ability: ContentRef {
                id: "basic_melee".into(),
                name: "Attack".into(),
            },
            outcome: AbilityOutcome::Applied,
            strike: None,
        }];
        let started = caster_is_me.then(|| {
            action(Event::AbilityStarted {
                ability: ContentRef {
                    id: "basic_melee".into(),
                    name: "Attack".into(),
                },
                target: after.observation.self_target,
            })
        });
        let beats =
            Chronicler::default().observe(&before, &after, started.as_ref(), &Palette::default());
        if expected_basic {
            assert!(beats.contains(&Beat::AbilityBegan("You ready yourself to strike.".into())));
        }
        assert!(
            beats.iter().any(|beat| matches!(beat,
                Beat::AbilityResolved { basic, .. }
                    if *basic == expected_basic
            )),
            "caster_is_me={caster_is_me}, disclose_stats={disclose_stats}: {beats:?}"
        );
    }
}

#[test]
fn a_deadly_blow_names_the_dead_after_they_vanish() {
    let before = view();
    let mut after = view();
    after.observation.tick = 6;
    after.observation.visible_actors.clear();
    let combat = after.observation.combat.as_mut().unwrap();
    combat.hp = 44;
    combat.events = vec![
        CombatEventView::Ability {
            strike: None,
            ability: tor_client_common::test_support::content("basic_melee"),
            caster: Some(tor_protocol::ActorTarget::from_digest([1; 32])),
            target: Some(tor_protocol::ActorTarget::from_digest([2; 32])),
            outcome: AbilityOutcome::Applied,
        },
        CombatEventView::Died {
            actor: tor_protocol::ActorTarget::from_digest([2; 32]),
            cause: DeathCause::Injury,
        },
    ];
    let scout = Figure {
        id: tor_protocol::ActorTarget::from_digest([2; 32]),
        name: "ruin scout".into(),
    };
    assert_eq!(
        Chronicler::default().observe(&before, &after, None, &Palette::default()),
        [
            Beat::AbilityResolved {
                strike: None,
                ability: tor_client_common::test_support::content("basic_melee"),
                basic: false,
                variant: 1,
                attacker: Who::Me,
                target: Who::Figure(scout.clone()),
                outcome: AbilityOutcome::Applied,
            },
            Beat::Died(Who::Figure(scout.clone())),
            Beat::Hp {
                from: 50,
                to: 44,
                max: 50,
            },
            Beat::Vanished(scout),
        ]
    );
}

#[test]
fn own_door_actions_are_not_also_told_as_changes() {
    let before = view();
    let mut after = view();
    after.observation.tick = 6;
    after.observation.visible_cells[0]
        .door
        .as_mut()
        .unwrap()
        .open = true;
    let mut chronicler = Chronicler::default();
    let palette = Palette::default();
    assert_eq!(
        chronicler.observe(
            &before,
            &after,
            Some(&action(Event::DoorChanged {
                door: tor_protocol::DoorTarget::from_digest([7; 32]),
                open: true
            })),
            &palette
        ),
        [Beat::SetDoor {
            name: "oak door".into(),
            open: true
        }]
    );
    assert_eq!(
        chronicler.observe(&before, &after, None, &palette),
        [Beat::DoorChanged {
            name: "oak door".into(),
            whereabouts: "at your feet".into(),
            open: true
        }]
    );
    // An unchanged view tells nothing new.
    assert!(chronicler
        .observe(&after, &after, None, &palette)
        .is_empty());
}
#[test]
fn door_outcomes_preserve_event_time_direction_and_own_action_suppression() {
    let before = view();
    let mut after = before.clone();
    after.observation.tick += 1;
    // The final disclosed projection is the same canonical one selected by
    // the legacy observer, including repeated portal/height projections.
    let mut door_cell = after.observation.visible_cells[0].clone();
    door_cell.position = Position { x: 1, y: 0, z: 0 };
    door_cell.door.as_mut().unwrap().open = true;
    after.observation.visible_cells[0]
        .door
        .as_mut()
        .unwrap()
        .open = true;
    after.observation.visible_cells.push(door_cell);
    let event = Event::DoorChanged {
        door: tor_protocol::DoorTarget::from_digest([7; 32]),
        open: true,
    };
    for own in [None, Some(&event)] {
        let facts = OutcomeFacts::capture(&before.observation, &after.observation, own);
        let entry = own.cloned().map(action);
        let expected =
            Chronicler::default().observe(&before, &after, entry.as_ref(), &Palette::default());
        let actual = Chronicler::default().outcome(&facts, &Palette::default());
        assert_eq!(actual, expected);
        if own.is_none() {
            assert_eq!(
                actual,
                [Beat::DoorChanged {
                    name: "oak door".into(),
                    whereabouts: "to the east".into(),
                    open: true,
                }]
            );
        }
    }
    let facts = OutcomeFacts::capture(&before.observation, &after.observation, None);
    after.observation.visible_cells.clear();
    assert_eq!(
        Chronicler::default().outcome(&facts, &Palette::default()),
        [Beat::DoorChanged {
            name: "oak door".into(),
            whereabouts: "to the east".into(),
            open: true,
        }]
    );
    assert!(Chronicler::default()
        .outcome(
            &OutcomeFacts::capture(&before.observation, &before.observation, None),
            &Palette::default()
        )
        .is_empty());
}

#[test]
fn dropped_object_narration_captures_cosmetic_name_without_true_sibling() {
    let before = view();
    let mut after = before.clone();
    after.observation.tick += 1;
    let id = ItemTarget::from_digest([82; 32]);
    after.observation.ground_items.push(GroundItemView {
        presentation: Some(ObjectPresentation {
            name: "silver flask".into(),
            asset: None,
            kind: ObjectPresentationKind::Item {
                class: ItemClass::Potion,
            },
        }),
        item: ItemView {
            corpse: None,
            class: ItemClass::Misc,
            quantity: 1,
            appearance: "object".into(),
            identified: true,
            identity: "item.misc.fixture".parse().unwrap(),
            knowledge: Default::default(),
            description: String::new(),
            id,
            name: "object".into(),
            asset: None,
            known_supply: None,
            known_currency: None,
        },
        position: after.observation.position,
        reachable: true,
        supply_reachable: false,
    });
    let event = Event::Dropped {
        item: id,
        result: id,
        quantity: 1,
    };
    assert_eq!(
        tor_client_common::narration::action(&event, &after.observation),
        "You drop 1 x silver flask."
    );
    let entry = action(event);
    let beats = Chronicler::default().observe(&before, &after, Some(&entry), &Palette::default());
    assert!(beats.contains(&Beat::Dropped {
        name: "silver flask".into(),
        quantity: 1
    }));
}
