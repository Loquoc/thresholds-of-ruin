use super::*;
use crate::Scenario;
use tokio::sync::mpsc;

#[test]
fn an_explicit_save_answers_durable_failed_or_behind_within_its_wait() {
    let mut status = crate::SaveStatus {
        durable_sequence: 7,
        ..Default::default()
    };
    let short = SAVE_REPLY_WAIT / 2;
    assert_eq!(
        save_reply(&status, 7, short, SAVE_REPLY_WAIT),
        SaveReply::Durable
    );
    assert_eq!(
        save_reply(&status, 7, SAVE_REPLY_WAIT * 2, SAVE_REPLY_WAIT),
        SaveReply::Durable
    );
    assert_eq!(
        save_reply(&status, 9, short, SAVE_REPLY_WAIT),
        SaveReply::Wait
    );
    // Never "outcome unknown": a slow writer gets a definite answer in time.
    assert_eq!(
        save_reply(&status, 9, SAVE_REPLY_WAIT, SAVE_REPLY_WAIT),
        SaveReply::Behind
    );
    assert!(SAVE_REPLY_WAIT < std::time::Duration::from_secs(10));
    status.error = Some("disk full".into());
    assert_eq!(
        save_reply(&status, 9, short, SAVE_REPLY_WAIT),
        SaveReply::Failed("disk full".into())
    );
    status.error = None;
    let configured = std::time::Duration::from_millis(200);
    assert_eq!(
        save_reply(&status, 9, configured, configured),
        SaveReply::Behind
    );
    let mut service = Service::new(Engine::memory(Scenario::two_room(42)).unwrap());
    assert!(service
        .set_save_reply_wait(std::time::Duration::ZERO)
        .is_err());
    assert!(service
        .set_save_reply_wait(std::time::Duration::from_secs(61))
        .is_err());
    service.set_save_reply_wait(configured).unwrap();
    assert_eq!(service.save_reply_wait, configured);
}

#[test]
fn observer_capture_failure_retains_every_pending_pause_until_retry() {
    let actors = [ActorId(1), ActorId(2)];
    let mut service =
        Service::new(Engine::memory(Scenario::performance(42, 2, 2).unwrap()).unwrap());
    let account = Account {
        role: AccessRole::Spectator,
        user: "observer".into(),
        token: "test".into(),
        actors: actors.into_iter().collect(),
    };
    let mut connection = service.connect(&account, "pause-fault".into()).unwrap();
    service.handle(
        connection.id,
        "attach".into(),
        Request::Attach { actor: actors[0] },
    );
    while connection.messages.try_recv().is_ok() {}
    for actor in actors {
        service
            .engine
            .command(
                "p",
                "test",
                actor,
                &format!("queued-{}", actor.0),
                &service.engine.branch().clone(),
                crate::journal::Command::AdmitIntention {
                    expected_boundary: service.engine.causal_boundary(actor).unwrap(),
                    action: crate::journal::Action::Wait,
                },
            )
            .unwrap();
        service.pending_pauses.insert(actor);
    }
    let before = service.engine.causal_boundary(actors[0]).unwrap();
    // A failed required observer projection must not drain pause ownership.
    service.clients.get_mut(&connection.id).unwrap().actor = Some(ActorId(u64::MAX));
    assert!(service.semantic_states().is_err());
    assert!(matches!(service.step_inner(), Step::Blocked));
    assert_eq!(service.pending_pauses, actors.into_iter().collect());
    assert_eq!(service.engine.causal_boundary(actors[0]).unwrap(), before);
    for actor in actors {
        assert_eq!(
            service.engine.pending_intentions(actor)[0].phase,
            IntentionPhase::Queued
        );
    }
    service.clients.get_mut(&connection.id).unwrap().actor = Some(actors[0]);
    service.apply_pending_pauses();
    assert!(service.pending_pauses.is_empty());
    for actor in actors {
        assert_eq!(
            service.engine.pending_intentions(actor)[0].phase,
            IntentionPhase::Suspended
        );
    }
}

#[test]
fn ordered_outcomes_survive_multiple_actions_without_visual_publication() {
    let actor = ActorId(1);
    let mut service = Service::new(Engine::memory(Scenario::two_room(42)).unwrap());
    let account = Account {
        role: AccessRole::Spectator,
        user: "observer".into(),
        token: "test".into(),
        actors: BTreeSet::from([actor]),
    };
    let mut connection = service.connect(&account, "paced".into()).unwrap();
    service.handle(connection.id, "attach".into(), Request::Attach { actor });
    while connection.messages.try_recv().is_ok() {}
    let original = service.clients[&connection.id]
        .last_observation
        .as_ref()
        .unwrap()
        .state
        .clone();
    let mut expected = Vec::new();
    for index in 0..2 {
        let before = service.semantic_states().unwrap();
        let result = service
            .engine
            .command(
                "backend",
                "test",
                actor,
                &format!("ordered-{index}"),
                &service.engine.branch().clone(),
                crate::journal::Command::Act {
                    expected_boundary: service.engine.causal_boundary(actor).unwrap(),
                    action: crate::journal::Action::Wait,
                },
            )
            .unwrap();
        expected.push(result.entry.id.clone());
        service.action_result_update(&before, &result).unwrap();
    }
    let messages: Vec<_> = std::iter::from_fn(|| connection.messages.try_recv().ok()).collect();
    let outcomes: Vec<_> = messages
        .iter()
        .filter_map(|message| match message {
            ServerMessage::Update { update } => match &update.body {
                UpdateBody::Outcome {
                    event: Some(entry),
                    facts,
                    ..
                } => {
                    assert!(facts
                        .facts
                        .iter()
                        .any(|fact| matches!(fact, OutcomeFact::OwnAction { .. })));
                    Some((update.cursor.sequence, entry.id.clone()))
                }
                UpdateBody::Observation { .. } | UpdateBody::ObservationDelta { .. } => {
                    panic!("unsolicited picture")
                }
                _ => None,
            },
            _ => None,
        })
        .collect();
    assert_eq!(
        outcomes
            .iter()
            .map(|(_, id)| id.clone())
            .collect::<Vec<_>>(),
        expected
    );
    assert!(outcomes[0].0 < outcomes[1].0);
    assert_eq!(
        service.clients[&connection.id]
            .last_observation
            .as_ref()
            .unwrap()
            .state
            .snapshot,
        original.snapshot
    );
    service.handle(connection.id, "picture".into(), Request::Observe);
    let current = &service.clients[&connection.id]
        .last_observation
        .as_ref()
        .unwrap()
        .state;
    assert_ne!(current.snapshot, original.snapshot);
    assert_eq!(current.revision, original.revision + 1);
    assert!(current.observation.tick > original.observation.tick);
    let visual: Vec<_> = std::iter::from_fn(|| connection.messages.try_recv().ok()).collect();
    assert!(visual.iter().all(|message| !matches!(message, ServerMessage::Update { update } if matches!(update.body, UpdateBody::Outcome { .. }))));
}

#[test]
fn invalid_committed_outcome_closes_stream_instead_of_skipping_delivery() {
    let actor = ActorId(1);
    let mut service = Service::new(Engine::memory(Scenario::two_room(42)).unwrap());
    let account = Account {
        role: AccessRole::Spectator,
        user: "observer".into(),
        token: "test".into(),
        actors: BTreeSet::from([actor]),
    };
    let mut connection = service.connect(&account, "bounded".into()).unwrap();
    service.handle(connection.id, "attach".into(), Request::Attach { actor });
    while connection.messages.try_recv().is_ok() {}
    let mut before = service.semantic_states().unwrap();
    let result = service
        .engine
        .command(
            "backend",
            "test",
            actor,
            "committed",
            &service.engine.branch().clone(),
            crate::journal::Command::Act {
                expected_boundary: service.engine.causal_boundary(actor).unwrap(),
                action: crate::journal::Action::Wait,
            },
        )
        .unwrap();
    before.events = service
        .engine
        .event_history_boundary_at(&service.engine.presentation_boundary())
        .unwrap();
    before.events.through += 1; // impossible provenance is rejected, never silently truncated
    service.save_warning = Some("existing storage warning".into());
    service.action_result_update(&before, &result).unwrap();
    assert!(!service.clients.contains_key(&connection.id));
    assert!(connection.close.borrow().is_some());
    assert_eq!(
        service.save_warning.as_deref(),
        Some("existing storage warning")
    );
    assert!(
        service.engine.causal_boundary(actor).unwrap() > 0,
        "committed action is not rolled back by output failure"
    );
}

#[test]
fn hidden_clock_change_does_not_publish_an_outcome_or_change_display_identity() {
    let actor = ActorId(1);
    let mut service = Service::new(Engine::memory(Scenario::two_room(42)).unwrap());
    let account = Account {
        role: AccessRole::Spectator,
        user: "observer".into(),
        token: "test".into(),
        actors: BTreeSet::from([actor]),
    };
    let mut connection = service.connect(&account, "private".into()).unwrap();
    service.handle(connection.id, "attach".into(), Request::Attach { actor });
    while connection.messages.try_recv().is_ok() {}
    let identity = service.clients[&connection.id]
        .last_observation
        .as_ref()
        .unwrap()
        .state
        .snapshot
        .clone();
    let mut before = service.semantic_states().unwrap();
    Arc::make_mut(before.views.get_mut(&actor).unwrap())
        .facts
        .tick += 1;
    service.action_update(&before, None, None).unwrap();
    assert!(connection.messages.try_recv().is_err());
    assert_eq!(
        service.clients[&connection.id]
            .last_observation
            .as_ref()
            .unwrap()
            .state
            .snapshot,
        identity
    );
    service.observe(connection.id).unwrap();
    let first = service.clients[&connection.id]
        .last_observation
        .as_ref()
        .unwrap()
        .state
        .clone();
    service.observe(connection.id).unwrap();
    let second = &service.clients[&connection.id]
        .last_observation
        .as_ref()
        .unwrap()
        .state;
    assert_ne!(
        first.snapshot, second.snapshot,
        "identity follows demand, not hidden activity"
    );
    assert_eq!(second.revision, first.revision + 1);
    assert_eq!(first.observation, second.observation);
}

#[test]
fn intentional_detach_drains_its_explanation_but_resource_disconnect_aborts() {
    let mut service = Service::new(Engine::memory(Scenario::two_room(42)).unwrap());
    let account = Account {
        role: AccessRole::Spectator,
        user: "observer".into(),
        token: "test-only".into(),
        actors: BTreeSet::from([ActorId(1)]),
    };
    let mut detached = service.connect(&account, "headless".into()).unwrap();
    detached.messages.try_recv().unwrap();
    service.detach_unloaded(detached.id);
    assert_eq!(*detached.close.borrow(), Some(DisconnectMode::Drain));
    assert!(matches!(
        detached.messages.try_recv().unwrap(),
        ServerMessage::Error {
            code: ErrorCode::NotAttached,
            ..
        }
    ));
    let failed = service.connect(&account, "headless".into()).unwrap();
    service.disconnect(failed.id);
    assert_eq!(*failed.close.borrow(), Some(DisconnectMode::Abort));
}

#[test]
fn narrowing_a_view_chooses_the_smaller_complete_encoded_update() {
    let actor = ActorId(1);
    let mut service = Service::new(Engine::memory(Scenario::two_room(42)).unwrap());
    let account = Account {
        role: AccessRole::Spectator,
        user: "encoding-observer".into(),
        token: "test-only".into(),
        actors: BTreeSet::from([actor]),
    };
    let mut client = service.connect(&account, "headless".into()).unwrap();
    service.handle(client.id, "attach".into(), Request::Attach { actor });
    while client.messages.try_recv_frame().is_ok() {}
    let mut base = service.engine.state(actor).unwrap();
    // This encoding-size witness operates on the spectator's actual base,
    // not the richer owner picture supplied to the role projection.
    restrict_item_disclosure(&mut base, AccessRole::Spectator);
    base.revision += 1;
    base.observation.tick += 1;
    base.observation.visible_cells.truncate(5);
    assert_eq!(base.observation.visible_cells.len(), 5);
    base.observation.visible_cells[0].door = Some(DoorView {
        asset: None,
        id: service.engine.target_scope(actor).door(77),
        name: "visible door".into(),
        description: "An already disclosed elaborate carving. ".repeat(128),
        open: false,
        reachable: false,
        approaches: Vec::new(),
    });
    base.validate().unwrap();
    service.update(
        client.id,
        UpdateBody::Observation {
            state: base.clone().into(),
            event: None,
        },
    );
    while client.messages.try_recv_frame().is_ok() {}
    let disclosed = service.clients[&client.id]
        .last_observation
        .as_ref()
        .unwrap();
    let previous = disclosed.base.clone();
    assert_eq!(*disclosed.state, base);
    let mut next = base.clone();
    next.snapshot = SnapshotId("next-issued-test-picture".into());
    next.revision += 1;
    next.observation.tick += 1;
    next.observation.visible_cells.truncate(1);
    next.validate().unwrap();
    // A concrete valid delta is a witness that counting removed cells is
    // insufficient: the retained cell holds most of the full payload.
    let o = &next.observation;
    let witness = StateDelta {
        base_snapshot: base.snapshot.clone(),
        snapshot: next.snapshot.clone(),
        charts: o.charts.clone(),
        level_name: None,
        known_item_uses: o.known_item_uses.clone(),
        plane: None,
        self_appearance: o.self_appearance.clone(),
        interactions: None,
        base_revision: base.revision,
        wizard_game: next.wizard_game,
        revision: next.revision,
        combat: o.combat.clone(),
        motion: o.motion.clone(),
        places: Vec::new(),
        actor: o.actor,
        self_target: o.self_target,
        linked_holds: o.linked_holds.clone(),
        public_effects: o.public_effects.clone(),
        tick: o.tick,
        position: o.position,
        cells: CellChanges {
            shift: Position { x: 0, y: 0, z: 0 },
            removed: base.observation.visible_cells[1..]
                .iter()
                .map(|cell| cell.position)
                .collect(),
            changed: Vec::new(),
        },
        ground_items: Vec::new(),
        inventory: Vec::new(),
        visible_actors: Vec::new(),
        ready: o.ready,
    };
    assert_eq!(witness.clone().apply(&base).unwrap(), next);
    let client_state = &service.clients[&client.id];
    let envelope = |body| ServerMessage::Update {
        update: Box::new(StreamUpdate {
            context: client_state.context.clone().unwrap(),
            actor,
            branch: service.engine.branch().clone(),
            cursor: StreamCursor {
                sequence: client_state.sequence + 1,
                tick: next.observation.tick,
            },
            body,
        }),
    };
    let full_bytes = serde_json::to_vec(&envelope(UpdateBody::Observation {
        state: next.clone().into(),
        event: None,
    }))
    .unwrap()
    .len();
    let delta_bytes = serde_json::to_vec(&envelope(UpdateBody::ObservationDelta {
        base: previous.clone(),
        state: Box::new(witness),
        event: None,
    }))
    .unwrap()
    .len();
    assert!(
        delta_bytes < full_bytes,
        "the witness must actually save bytes"
    );
    service.update(
        client.id,
        UpdateBody::Observation {
            state: next.clone().into(),
            event: None,
        },
    );
    let frame = client.messages.try_recv_frame().unwrap();
    let ServerMessage::Update { update } = decode_response(&frame.text).unwrap() else {
        panic!("expected actual encoded observation");
    };
    let restored = match update.body {
        UpdateBody::Observation { state, .. } => Arc::unwrap_or_clone(state),
        UpdateBody::ObservationDelta {
            base: actual_base,
            state,
            ..
        } => {
            assert_eq!(actual_base, previous);
            state.apply(&base).unwrap()
        }
        _ => panic!("expected observation update"),
    };
    assert_eq!(restored, next);
    assert!(frame.text.len() <= delta_bytes,
            "actual server chose {} bytes, but a complete equivalent delta needs {delta_bytes} (full {full_bytes})",
            frame.text.len());
}

#[test]
fn published_disabled_admission_rejects_fresh_gameplay_without_mutation() {
    for pause in [false, true] {
        for command in [
            Command::Act {
                action: Action::Wait,
            },
            Command::Travel {
                expected_revision: 1,
                destination: "other-room".into(),
            },
        ] {
            let actor = ActorId(1);
            let mut service = Service::new(Engine::memory(Scenario::two_room(42)).unwrap());
            let account = Account {
                role: AccessRole::Player,
                user: "p".into(),
                token: "test".into(),
                actors: BTreeSet::from([actor]),
            };
            let mut client = service.connect(&account, "headless".into()).unwrap();
            service.handle(client.id, "attach".into(), Request::Attach { actor });
            service.handle(client.id, "control".into(), Request::AcquireControl);
            // Another actor's unsettled host work masks gameplay globally.
            if pause {
                service.pending_pauses.insert(ActorId(2));
            } else {
                service.pending_travel_cancellations.insert(ActorId(2));
            }
            service.refresh_readiness();
            while client.messages.try_recv().is_ok() {}
            assert!(
                !service.clients[&client.id]
                    .readiness
                    .as_ref()
                    .unwrap()
                    .permissions
                    .admission
            );
            let before = service.engine.state(actor).unwrap();
            let branch = service.engine.branch().clone();
            service.handle(
                client.id,
                "disabled".into(),
                Request::Command {
                    context: service.input_context(client.id).unwrap(),
                    branch: branch.clone(),
                    command,
                },
            );
            let messages: Vec<_> = std::iter::from_fn(|| client.messages.try_recv().ok()).collect();
            assert!(
                messages.iter().any(|message| matches!(message,
                    ServerMessage::Error { request_id: Some(id), code: ErrorCode::ActorBusy, .. }
                    if id == "disabled")),
                "disabled admission must reject: {messages:?}"
            );
            assert!(!messages
                .iter()
                .any(|message| matches!(message, ServerMessage::Ack { .. })));
            assert!(!service.engine.has_pending_intention(actor));
            assert!(!service.travels.contains_key(&actor));
            assert_eq!(service.engine.state(actor).unwrap(), before);
            assert_eq!(service.engine.branch(), &branch);
        }
    }
}

#[test]
fn published_disabled_recovery_rejects_resume_and_cancel_without_mutation() {
    for resume in [false, true] {
        let actor = ActorId(1);
        let mut engine = Engine::memory(Scenario::two_room(42)).unwrap();
        let admitted = engine
            .command(
                "p",
                "test",
                actor,
                "original",
                &engine.branch().clone(),
                crate::journal::Command::AdmitIntention {
                    expected_boundary: 0,
                    action: crate::journal::Action::Wait,
                },
            )
            .unwrap();
        let mut service = Service::new(engine);
        let account = Account {
            role: AccessRole::Player,
            user: "p".into(),
            token: "test".into(),
            actors: BTreeSet::from([actor]),
        };
        let mut client = service.connect(&account, "headless".into()).unwrap();
        service.handle(client.id, "attach".into(), Request::Attach { actor });
        service.handle(client.id, "control".into(), Request::AcquireControl);
        let intention = IntentionId(admitted.entry.id.0.clone());
        let permissions = &service.clients[&client.id]
            .readiness
            .as_ref()
            .unwrap()
            .permissions;
        assert!(permissions.resume.contains(&intention));
        assert!(permissions.cancel.contains(&intention));
        service.pending_pauses.insert(ActorId(2));
        service.refresh_readiness();
        while client.messages.try_recv().is_ok() {}
        let before = service.engine.state(actor).unwrap();
        let pending = service.engine.pending_intentions(actor);
        let command = if resume {
            Command::ResumeIntention { intention }
        } else {
            Command::CancelIntention {
                step: None,
                intention,
            }
        };
        service.handle(
            client.id,
            "disabled".into(),
            Request::Command {
                context: service.input_context(client.id).unwrap(),
                branch: service.engine.branch().clone(),
                command: command.clone(),
            },
        );
        let messages: Vec<_> = std::iter::from_fn(|| client.messages.try_recv().ok()).collect();
        assert!(
            messages.iter().any(|message| matches!(message,
                ServerMessage::Error { request_id: Some(id), code: ErrorCode::InvalidAction, .. }
                if id == "disabled")),
            "disabled recovery must reject: {messages:?}"
        );
        assert!(!messages
            .iter()
            .any(|message| matches!(message, ServerMessage::Ack { .. })));
        assert_eq!(service.engine.state(actor).unwrap(), before);
        assert_eq!(service.engine.pending_intentions(actor), pending);
        // The rejection leaves the same work usable after policy settles.
        service.pending_pauses.clear();
        service.refresh_readiness();
        while client.messages.try_recv().is_ok() {}
        service.handle(
            client.id,
            "enabled".into(),
            Request::Command {
                context: service.input_context(client.id).unwrap(),
                branch: service.engine.branch().clone(),
                command,
            },
        );
        let messages: Vec<_> = std::iter::from_fn(|| client.messages.try_recv().ok()).collect();
        assert!(
            messages.iter().any(|message| matches!(message,
                ServerMessage::Ack { request_id, .. } if request_id == "enabled")),
            "enabled recovery must succeed: {messages:?}"
        );
        assert_eq!(service.engine.state(actor).unwrap(), before);
    }
}

#[test]
fn query_replies_follow_changed_permissions_without_running_simulation() {
    for request in [
        Request::History {
            before: None,
            limit: 8,
        },
        Request::Palette,
    ] {
        let actor = ActorId(1);
        let mut service = Service::new(Engine::memory(Scenario::two_room(42)).unwrap());
        let account = Account {
            role: AccessRole::Player,
            user: "p".into(),
            token: "test".into(),
            actors: BTreeSet::from([actor]),
        };
        let mut client = service.connect(&account, "headless".into()).unwrap();
        while client.messages.try_recv().is_ok() {}
        service.handle(client.id, "attach".into(), Request::Attach { actor });
        while client.messages.try_recv().is_ok() {}
        service.handle(client.id, "control".into(), Request::AcquireControl);
        while client.messages.try_recv().is_ok() {}
        let before = service.engine.state(actor).unwrap();
        // A suspension awaiting persistence disables new input globally.
        service.pending_pauses.insert(actor);
        service.handle(client.id, "query".into(), request);
        let messages: Vec<_> = std::iter::from_fn(|| client.messages.try_recv().ok()).collect();
        let readiness = messages
            .iter()
            .position(|message| {
                matches!(message,
                ServerMessage::Update { update } if matches!(&update.body,
                    UpdateBody::Readiness { readiness } if !readiness.admission))
            })
            .unwrap();
        let reply = messages.iter().position(|message| matches!(message,
                ServerMessage::History { request_id, .. } if request_id == "query") || matches!(message,
                ServerMessage::Palette { request_id: Some(request_id), .. } if request_id == "query")).unwrap();
        assert!(
            readiness < reply,
            "query completion must follow current permissions: {messages:?}"
        );
        let ServerMessage::Update { update } = &messages[readiness] else {
            unreachable!()
        };
        let context = match &messages[reply] {
            ServerMessage::History { context, .. } | ServerMessage::Palette { context, .. } => {
                context
            }
            _ => unreachable!(),
        };
        let UpdateBody::Readiness { readiness } = &update.body else {
            unreachable!()
        };
        assert_eq!(context.input.stream, update.context);
        assert_eq!(context.input.readiness_revision, readiness.revision);
        assert_eq!(context.actor, actor);
        assert_eq!(context.branch, update.branch);
        assert_eq!(context.cursor, update.cursor);
        assert_eq!(
            context.revision,
            service.clients[&client.id]
                .last_observation
                .as_ref()
                .unwrap()
                .state
                .revision
        );
        assert_eq!(service.engine.state(actor).unwrap(), before);
    }
}

#[test]
fn rejected_reply_disconnection_publishes_permissions_to_surviving_controllers() {
    let mut service =
        Service::new(Engine::memory(Scenario::performance(42, 4, 2).unwrap()).unwrap());
    let account = Account {
        role: AccessRole::Player,
        user: "p".into(),
        token: "test".into(),
        actors: BTreeSet::from([ActorId(1), ActorId(2)]),
    };
    let mut clients = Vec::new();
    for actor in [ActorId(1), ActorId(2)] {
        let mut client = service.connect(&account, "headless".into()).unwrap();
        while client.messages.try_recv().is_ok() {}
        service.handle(client.id, "attach".into(), Request::Attach { actor });
        while client.messages.try_recv().is_ok() {}
        service.handle(client.id, "control".into(), Request::AcquireControl);
        while client.messages.try_recv().is_ok() {}
        clients.push(client);
    }
    let slow = clients[0].id;
    let survivor = clients[1].id;
    assert!(
        service.clients[&survivor]
            .readiness
            .as_ref()
            .unwrap()
            .permissions
            .admission
    );
    for _ in 0..QUEUE {
        service.send(slow, ServerMessage::Waiting { on: Waiting::You });
    }
    assert!(service.clients.contains_key(&slow));
    service.handle(slow, String::new(), Request::Continue);
    assert!(!service.clients.contains_key(&slow));
    let messages: Vec<_> = std::iter::from_fn(|| clients[1].messages.try_recv().ok()).collect();
    assert!(messages.iter().any(|message| matches!(message,
            ServerMessage::Update { update } if matches!(&update.body,
                UpdateBody::Readiness { readiness } if !readiness.admission))),
            "controller removal must publish disabled input before the next simulation step: {messages:?}");
}

#[test]
fn readiness_tracks_control_and_queue_lifecycle_without_changing_observation_bases() {
    let actor = ActorId(1);
    let mut service = Service::new(Engine::memory(Scenario::two_room(42)).unwrap());
    let account = Account {
        role: AccessRole::Player,
        user: "p".into(),
        token: "test".into(),
        actors: BTreeSet::from([actor]),
    };
    let mut client = service.connect(&account, "headless".into()).unwrap();
    while client.messages.try_recv().is_ok() {}
    service.handle(client.id, "attach".into(), Request::Attach { actor });
    let snapshot = loop {
        if let ServerMessage::Snapshot { snapshot, .. } = client.messages.try_recv().unwrap() {
            break *snapshot;
        }
    };
    assert_eq!(snapshot.readiness.revision, 0);
    assert!(!snapshot.readiness.admission);
    let mut model = tor_client_common::ClientState::from_snapshot(snapshot).unwrap();
    while client.messages.try_recv().is_ok() {}
    let initial_base = model.observation_base();
    for (request_id, request, expected_revision, admission) in [
        ("acquire", Request::AcquireControl, 1, true),
        ("release", Request::ReleaseControl, 2, false),
        ("reacquire", Request::AcquireControl, 3, true),
    ] {
        service.handle(client.id, request_id.into(), request);
        let mut readiness_seen = false;
        while let Ok(message) = client.messages.try_recv() {
            match message {
                ServerMessage::Update { update } => {
                    readiness_seen |= matches!(update.body, UpdateBody::Readiness { .. });
                    model.apply(*update).unwrap();
                }
                ServerMessage::Ack {
                    context,
                    request_id: answered,
                    ..
                } => {
                    model.validate_reply_context(&context).unwrap();
                    assert_eq!(answered, request_id);
                    assert!(readiness_seen, "permissions must precede acknowledgement");
                }
                _ => {}
            }
        }
        assert_eq!(model.readiness().revision, expected_revision);
        assert_eq!(model.readiness().admission, admission);
        assert_eq!(model.observation_base(), initial_base);
    }
    service.handle(
        client.id,
        "act".into(),
        Request::Command {
            context: service.input_context(client.id).unwrap(),
            branch: service.engine.branch().clone(),
            command: Command::Act {
                action: Action::Move {
                    direction: Direction::East,
                },
            },
        },
    );
    while let Ok(message) = client.messages.try_recv() {
        if let ServerMessage::Update { update } = message {
            model.apply(*update).unwrap();
        }
    }
    assert_eq!(model.readiness().revision, 4);
    assert!(!model.readiness().admission);
    assert_eq!(
        model.readiness().cancel,
        vec![model.intentions()[0].intention.clone()]
    );
    assert_eq!(model.observation_base(), initial_base);
    assert!(matches!(service.step(), Step::Progress));
    while let Ok(message) = client.messages.try_recv() {
        if let ServerMessage::Update { update } = message {
            model.apply(*update).unwrap();
        }
    }
    assert_eq!(model.readiness().revision, 5);
    assert!(model.readiness().admission);
    assert!(model.readiness().cancel.is_empty());
    assert!(model.cursor().sequence > initial_base.cursor.sequence);
    assert_eq!(model.observation_base(), initial_base);
    assert!(!model.observation_current());
    service.handle(client.id, "demand-picture".into(), Request::Observe);
    while let Ok(message) = client.messages.try_recv() {
        if let ServerMessage::Update { update } = message {
            model.apply(*update).unwrap();
        }
    }
    assert!(model.observation_base().cursor.sequence > initial_base.cursor.sequence);
    assert!(model.observation_current());
}

#[test]
fn a_new_command_cannot_reuse_authority_from_before_control_loss_or_snapshot_reset() {
    let actor = ActorId(1);
    let mut service = Service::new(Engine::memory(Scenario::two_room(42)).unwrap());
    let account = Account {
        role: AccessRole::Player,
        user: "p".into(),
        token: "test".into(),
        actors: BTreeSet::from([actor]),
    };
    let mut client = service.connect(&account, "headless".into()).unwrap();
    service.handle(client.id, "attach".into(), Request::Attach { actor });
    service.handle(client.id, "acquire".into(), Request::AcquireControl);
    let before = service.engine.state(actor).unwrap();
    let old_authority = service.input_context(client.id).unwrap();
    service.handle(client.id, "release".into(), Request::ReleaseControl);
    service.handle(client.id, "reacquire".into(), Request::AcquireControl);
    let old_epoch = service.input_context(client.id).unwrap();
    service.handle(client.id, "reset".into(), Request::Snapshot);
    let current = service.input_context(client.id).unwrap();
    let mut foreign_stream = current.clone();
    foreign_stream.stream.stream = StreamId("another-attachment".into());
    while client.messages.try_recv().is_ok() {}
    for (index, context) in [old_authority, old_epoch, foreign_stream]
        .into_iter()
        .enumerate()
    {
        let request_id = format!("stale-{index}");
        service.handle(
            client.id,
            request_id.clone(),
            Request::Command {
                context,
                branch: service.engine.branch().clone(),
                command: Command::Act {
                    action: Action::Wait,
                },
            },
        );
        let messages: Vec<_> = std::iter::from_fn(|| client.messages.try_recv().ok()).collect();
        assert!(messages.iter().any(|message| matches!(message,
                ServerMessage::Error { request_id: Some(id), code: ErrorCode::StaleContext, .. } if id == &request_id)),
                "stale transport authority must reject before admission: {messages:?}");
        assert!(!messages
            .iter()
            .any(|message| matches!(message, ServerMessage::Ack { .. })));
        assert_eq!(service.engine.state(actor).unwrap(), before);
        assert!(!service.engine.has_pending_intention(actor));
    }
    let request = Request::Command {
        context: current,
        branch: service.engine.branch().clone(),
        command: Command::Act {
            action: Action::Wait,
        },
    };
    service.handle(client.id, "original".into(), request.clone());
    assert!(service.engine.has_pending_intention(actor));
    while client.messages.try_recv().is_ok() {}
    assert!(matches!(service.step(), Step::Progress));
    let after = service.engine.state(actor).unwrap();
    service.handle(client.id, "release-final".into(), Request::ReleaseControl);
    service.handle(client.id, "reset-final".into(), Request::Snapshot);
    while client.messages.try_recv().is_ok() {}
    service.handle(client.id, "original".into(), request);
    let messages: Vec<_> = std::iter::from_fn(|| client.messages.try_recv().ok()).collect();
    assert!(messages.iter().any(|message| matches!(message,
            ServerMessage::Ack { request_id, receipt: RequestReceipt::Admitted { phase: IntentionPhase::Resolved, .. }, .. }
                if request_id == "original")), "original receipt must win over abandoned input context: {messages:?}");
    assert_eq!(service.engine.state(actor).unwrap(), after);
    assert!(!service.engine.has_pending_intention(actor));
}

#[test]
fn a_command_cannot_predict_an_unpublished_readiness_generation() {
    let actor = ActorId(1);
    let mut service = Service::new(Engine::memory(Scenario::two_room(42)).unwrap());
    let account = Account {
        role: AccessRole::Player,
        user: "p".into(),
        token: "test".into(),
        actors: BTreeSet::from([actor]),
    };
    let mut client = service.connect(&account, "headless".into()).unwrap();
    service.handle(client.id, "attach".into(), Request::Attach { actor });
    service.handle(client.id, "acquire".into(), Request::AcquireControl);
    while client.messages.try_recv().is_ok() {}
    let before = service.engine.state(actor).unwrap();
    let mut guessed = service.input_context(client.id).unwrap();
    guessed.readiness_revision += 1;
    // Pending host work changed permissions but has not published the new
    // generation. Neither the old generation nor a predicted one is fresh.
    service.pending_travel_cancellations.insert(actor);
    service.handle(
        client.id,
        "predicted".into(),
        Request::Command {
            context: guessed,
            branch: service.engine.branch().clone(),
            command: Command::Act {
                action: Action::Wait,
            },
        },
    );
    let messages: Vec<_> = std::iter::from_fn(|| client.messages.try_recv().ok()).collect();
    assert!(
        messages.iter().any(|message| matches!(
            message,
            ServerMessage::Error {
                code: ErrorCode::StaleContext,
                ..
            }
        )),
        "unpublished generation must reject: {messages:?}"
    );
    assert!(!messages
        .iter()
        .any(|message| matches!(message, ServerMessage::Ack { .. })));
    assert!(!service.engine.has_pending_intention(actor));
    assert_eq!(service.engine.state(actor).unwrap(), before);
}

#[test]
fn control_changes_invalidate_readiness_even_when_all_permissions_stay_disabled() {
    let actor = ActorId(1);
    let mut service = Service::new(Engine::memory(Scenario::two_room(42)).unwrap());
    let account = Account {
        role: AccessRole::Player,
        user: "p".into(),
        token: "test".into(),
        actors: BTreeSet::from([actor]),
    };
    let mut client = service.connect(&account, "headless".into()).unwrap();
    service.handle(client.id, "attach".into(), Request::Attach { actor });
    while client.messages.try_recv().is_ok() {}
    let before = service.engine.state(actor).unwrap();
    // Unsettled cancellation prevents gameplay permission publication. Control
    // can still transfer while the disclosed permission vectors remain empty.
    service.pending_travel_cancellations.insert(actor);
    for (request, revision) in [(Request::AcquireControl, 1), (Request::ReleaseControl, 2)] {
        service.handle(client.id, format!("control-{revision}"), request);
        let mut published = None;
        while let Ok(message) = client.messages.try_recv() {
            if let ServerMessage::Update { update } = message {
                if let UpdateBody::Readiness { readiness } = update.body {
                    assert!(published.replace(readiness).is_none());
                }
            }
        }
        let readiness = published
            .expect("ownership changes need a fresh generation even with identical permissions");
        assert_eq!(
            readiness,
            Readiness {
                revision,
                admission: false,
                resume: vec![],
                cancel: vec![]
            }
        );
        assert_eq!(service.engine.state(actor).unwrap(), before);
    }
}

#[test]
fn snapshot_admission_establishes_the_exact_reset_base() {
    let actor = ActorId(1);
    let mut service = Service::new(Engine::memory(Scenario::two_room(42)).unwrap());
    let account = Account {
        role: AccessRole::Spectator,
        user: "snapshot-observer".into(),
        token: "test-only".into(),
        actors: BTreeSet::from([actor]),
    };
    let mut connection = service.connect(&account, "headless".into()).unwrap();
    service.handle(connection.id, "attach".into(), Request::Attach { actor });
    while connection.messages.try_recv().is_ok() {}
    let previous = service.clients[&connection.id].context.clone().unwrap();
    service.snapshot(connection.id, "reset").unwrap();
    let ServerMessage::Snapshot {
        request_id,
        snapshot,
    } = connection.messages.try_recv().unwrap()
    else {
        panic!("reset must publish a snapshot first");
    };
    assert_eq!(request_id, "reset");
    assert_eq!(snapshot.context, previous.next_reset().unwrap());
    snapshot.state.validate().unwrap();
    let client = &service.clients[&connection.id];
    let disclosed = client.last_observation.as_ref().unwrap();
    assert_eq!(client.context.as_ref(), Some(&snapshot.context));
    assert_eq!(client.observation_tick, snapshot.cursor.tick);
    assert_eq!(disclosed.branch, snapshot.branch);
    assert_eq!(
        disclosed.base,
        ObservationBase {
            snapshot: snapshot.state.snapshot.clone(),
            cursor: snapshot.cursor,
            revision: snapshot.state.revision,
        }
    );
    assert_eq!(disclosed.state, snapshot.state);
    assert_eq!(client.waiting, None);
}

#[test]
fn recovery_preserves_a_complete_snapshot_when_metadata_exceeds_one_frame() {
    let actor = ActorId(1);
    let account = Account {
        role: AccessRole::Spectator,
        user: "recovery-observer".into(),
        token: "test-only".into(),
        actors: BTreeSet::from([actor]),
    };
    // Size the host limit from a real initial response, not guessed padding
    // for every possible future history or queue envelope.
    let frame_bytes = {
        let mut probe = Service::new(Engine::memory(Scenario::two_room(42)).unwrap());
        let mut peer = probe.connect(&account, "headless".into()).unwrap();
        probe.handle(peer.id, "attach".into(), Request::Attach { actor });
        let mut snapshot_bytes = None;
        while let Ok(frame) = peer.messages.try_recv_frame() {
            if matches!(
                decode_response(&frame.text).unwrap(),
                ServerMessage::Snapshot { .. }
            ) {
                snapshot_bytes = Some(frame.text.len());
            }
        }
        snapshot_bytes.expect("initial snapshot") + 256
    };
    let mut service = Service::with_outbound_limits(
        Engine::memory(Scenario::two_room(42)).unwrap(),
        crate::OutboundLimits {
            frame_bytes,
            client_bytes: 4 * frame_bytes,
            total_bytes: 16 * frame_bytes,
        },
    )
    .unwrap();
    let mut peer = service.connect(&account, "headless".into()).unwrap();
    service.handle(peer.id, "attach".into(), Request::Attach { actor });
    while peer.messages.try_recv_frame().is_ok() {}
    assert!(service.clients.contains_key(&peer.id));
    for _ in 0..4 {
        service
            .annotate_backend(
                actor,
                "recovery-test",
                Anchor::State {
                    snapshot: SnapshotId("trusted-recovery-note".into()),
                },
                AnnotationCategory::Note,
                &"x".repeat(256),
            )
            .unwrap();
        while let Ok(frame) = peer.messages.try_recv_frame() {
            assert!(frame.text.len() <= frame_bytes);
        }
        assert!(
            service.clients.contains_key(&peer.id),
            "individual updates fit"
        );
    }
    let expected_state = service.engine.state(actor).unwrap();
    assert!(serde_json::to_vec(&expected_state).unwrap().len() < frame_bytes);
    let expected_history = service
        .engine
        .history(actor, &account.user, None, MAX_HISTORY_PAGE)
        .unwrap();
    service.snapshot(peer.id, "recover").unwrap();
    assert!(
        service.clients.contains_key(&peer.id),
        "a bounded state must remain recoverable when its complete snapshot spans frames"
    );
    let ServerMessage::Snapshot {
        request_id,
        snapshot,
    } = peer.messages.try_recv().unwrap()
    else {
        panic!("recovery must publish one complete logical snapshot");
    };
    assert_eq!(request_id, "recover");
    crate::assert_spectator_disclosed_state_eq(&snapshot.state, &expected_state);
    assert_eq!(
        snapshot.history, expected_history,
        "recovery cannot prune history to fit"
    );
}

#[test]
fn rejected_snapshot_output_disconnects_without_affecting_a_healthy_peer() {
    let actor = ActorId(1);
    for closed in [false, true] {
        let mut service = Service::new(Engine::memory(Scenario::two_room(42)).unwrap());
        let account = Account {
            role: AccessRole::Spectator,
            user: "snapshot-observer".into(),
            token: "test-only".into(),
            actors: BTreeSet::from([actor]),
        };
        let mut connection = service.connect(&account, "headless".into()).unwrap();
        let mut healthy = service.connect(&account, "headless".into()).unwrap();
        for peer in [&mut connection, &mut healthy] {
            service.handle(peer.id, "attach".into(), Request::Attach { actor });
            while peer.messages.try_recv().is_ok() {}
        }
        let before = service.engine.state(actor).unwrap();
        let id = connection.id;
        if closed {
            drop(connection.messages);
        } else {
            for _ in 0..QUEUE {
                service.clients[&id]
                    .messages
                    .try_send(ServerMessage::Waiting { on: Waiting::You })
                    .unwrap();
            }
        }
        service.snapshot(id, "reset").unwrap();
        assert!(!service.clients.contains_key(&id));
        assert!(connection.close.borrow().is_some());
        assert_eq!(service.engine.state(actor).unwrap(), before);
        service.snapshot(healthy.id, "healthy-reset").unwrap();
        let ServerMessage::Snapshot {
            request_id,
            snapshot,
        } = healthy.messages.try_recv().unwrap()
        else {
            panic!("healthy peer must receive its snapshot");
        };
        assert_eq!(request_id, "healthy-reset");
        crate::assert_spectator_disclosed_state_eq(&snapshot.state, &before);
    }
}

#[test]
fn a_snapshot_with_exhausted_readiness_disconnects_instead_of_reusing_authority() {
    let actor = ActorId(1);
    let mut service = Service::new(Engine::memory(Scenario::two_room(42)).unwrap());
    let account = Account {
        role: AccessRole::Player,
        user: "p".into(),
        token: "test".into(),
        actors: BTreeSet::from([actor]),
    };
    let mut client = service.connect(&account, "headless".into()).unwrap();
    service.handle(client.id, "attach".into(), Request::Attach { actor });
    while client.messages.try_recv().is_ok() {}
    let previous_context = service.clients[&client.id].context.clone();
    let readiness = service
        .clients
        .get_mut(&client.id)
        .unwrap()
        .readiness
        .as_mut()
        .unwrap();
    readiness.permissions.revision = u64::MAX;
    // A stale cache requires a fresh authority generation, which cannot wrap.
    readiness.permissions.admission = true;
    assert!(service.snapshot(client.id, "reset").is_err());
    assert!(
        !service.clients.contains_key(&client.id),
        "exhausted authority must disconnect"
    );
    assert!(previous_context.is_some());
    assert!(
        client.messages.try_recv().is_err(),
        "no invalid snapshot may be published"
    );
}

#[test]
fn recovered_queued_gameplay_is_suspended_before_control_can_restart_it() {
    let actor = ActorId(1);
    let mut engine = Engine::memory(Scenario::two_room(42)).unwrap();
    let admitted = engine
        .command(
            "p",
            "test",
            actor,
            "queued",
            &engine.branch().clone(),
            crate::journal::Command::AdmitIntention {
                expected_boundary: 0,
                action: crate::journal::Action::Move {
                    direction: crate::journal::Direction::East,
                },
            },
        )
        .unwrap();
    let state = engine.state(actor).unwrap();
    let mut service = Service::new(engine);
    let status = service.engine.pending_intentions(actor);
    assert_eq!(status[0].phase, IntentionPhase::Suspended);
    assert_eq!(status[0].entry_id, admitted.entry.id);
    assert_eq!(service.engine.state(actor).unwrap(), state);
    let account = Account {
        role: AccessRole::Player,
        user: "p".into(),
        token: "test".into(),
        actors: BTreeSet::from([actor]),
    };
    let mut client = service.connect(&account, "test".into()).unwrap();
    while client.messages.try_recv().is_ok() {}
    service.handle(client.id, "attach".into(), Request::Attach { actor });
    while client.messages.try_recv().is_ok() {}
    service.handle(client.id, "control".into(), Request::AcquireControl);
    while client.messages.try_recv().is_ok() {}
    assert!(matches!(service.step(), Step::Blocked));
    assert_eq!(service.engine.state(actor).unwrap(), state);
    service.handle(
        client.id,
        "cancel".into(),
        Request::Command {
            context: service.input_context(client.id).unwrap(),
            branch: service.engine.branch().clone(),
            command: Command::CancelIntention {
                step: None,
                intention: IntentionId(admitted.entry.id.0),
            },
        },
    );
    assert!(
        !service.autonomous_enabled,
        "cancellation must not restart unrelated simulation work"
    );
    assert!(service.engine.pending_intentions(actor).is_empty());
    assert_eq!(service.engine.state(actor).unwrap(), state);
}

#[test]
fn client_action_is_admitted_before_simulation_applies_its_effect() {
    let mut service = Service::new(Engine::memory(Scenario::two_room(42)).unwrap());
    let account = Account {
        role: AccessRole::Player,
        user: "p".into(),
        token: "test".into(),
        actors: BTreeSet::from([ActorId(1)]),
    };
    let mut client = service.connect(&account, "headless".into()).unwrap();
    while client.messages.try_recv().is_ok() {}
    service.handle(
        client.id,
        "attach".into(),
        Request::Attach { actor: ActorId(1) },
    );
    while client.messages.try_recv().is_ok() {}
    service.handle(client.id, "control".into(), Request::AcquireControl);
    while client.messages.try_recv().is_ok() {}
    let before = service.engine.state(ActorId(1)).unwrap();
    let request = Request::Command {
        context: service.input_context(client.id).unwrap(),
        branch: service.engine.branch().clone(),
        command: Command::Act {
            action: Action::Move {
                direction: Direction::East,
            },
        },
    };
    service.handle(client.id, "queued".into(), request.clone());
    assert!(
        service.engine.state(ActorId(1)).unwrap() == before,
        "handling a gameplay request admits work without executing it"
    );
    let mut accepted = false;
    let mut identity = None;
    while let Ok(message) = client.messages.try_recv() {
        if let ServerMessage::Ack {
            request_id,
            receipt,
            ..
        } = message
        {
            assert_eq!(request_id, "queued");
            assert!(matches!(
                &receipt,
                RequestReceipt::Admitted {
                    actor: ActorId(1),
                    phase: IntentionPhase::Queued,
                    ..
                }
            ));
            if let RequestReceipt::Admitted {
                intention,
                entry_id,
                ..
            } = receipt
            {
                identity = Some((intention, entry_id));
            }
            accepted = true;
        }
    }
    assert!(accepted);
    assert!(matches!(service.step(), Step::Progress));
    let after = service.engine.state(ActorId(1)).unwrap();
    assert!(
        after != before,
        "simulation step applies the admitted action"
    );
    while client.messages.try_recv().is_ok() {}
    service.handle(client.id, "queued".into(), request);
    let mut retried = false;
    while let Ok(message) = client.messages.try_recv() {
        if let ServerMessage::Ack {
            request_id,
            receipt,
            ..
        } = message
        {
            assert_eq!(request_id, "queued");
            assert!(
                matches!(
                    &receipt,
                    RequestReceipt::Admitted {
                        phase: IntentionPhase::Resolved,
                        ..
                    }
                ),
                "a retry must resolve the original admitted intention"
            );
            if let RequestReceipt::Admitted {
                intention,
                entry_id,
                ..
            } = receipt
            {
                assert_eq!(Some((intention, entry_id)), identity);
            }
            retried = true;
        }
    }
    assert!(retried);
    assert!(
        service.engine.state(ActorId(1)).unwrap() == after,
        "retry must not execute again"
    );
}

#[test]
fn byte_backlog_disconnects_before_message_slots_fill_and_other_clients_continue() {
    let mut service = Service::new(Engine::memory(Scenario::two_room(0)).unwrap());
    let account = Account {
        role: AccessRole::Spectator,
        user: "watcher".into(),
        token: "test-only".into(),
        actors: BTreeSet::from([ActorId(1)]),
    };
    let mut slow = service.connect(&account, "ascii".into()).unwrap();
    slow.messages.try_recv().unwrap();
    let mut healthy = service.connect(&account, "headless".into()).unwrap();
    healthy.messages.try_recv().unwrap();
    let branch = service.engine.branch().clone();
    let revision = service.engine.causal_boundary(ActorId(1)).unwrap();
    // These frames fit the existing native client's frame ceiling. Their
    // backlog exceeds 64 MiB while still far below the 256 message slots.
    for _ in 0..65 {
        service.send(
            slow.id,
            ServerMessage::Error {
                scope: service.error_scope(slow.id),
                request_id: None,
                code: ErrorCode::InvalidRequest,
                message: "x".repeat(1024 * 1024),
            },
        );
    }
    assert!(
        slow.close.borrow().is_some(),
        "byte pressure must disconnect a slow stream"
    );
    assert!(healthy.close.borrow().is_none());
    service.handle(
        healthy.id,
        "attach".into(),
        Request::Attach { actor: ActorId(1) },
    );
    assert!(matches!(
        healthy.messages.try_recv().unwrap(),
        ServerMessage::Snapshot { .. }
    ));
    assert_eq!(service.engine.branch(), &branch);
    assert_eq!(
        service.engine.causal_boundary(ActorId(1)).unwrap(),
        revision
    );
}

#[test]
fn paid_identification_and_instance_counts_are_owner_only_on_encoded_updates() {
    let mut service = Service::new(Engine::memory(Scenario::two_room(0)).unwrap());
    let item = ItemTarget::from_digest([72; 32]);
    let mut state = service.engine.state(ActorId(1)).unwrap();
    state.observation.inventory.push(serde_json::from_value(serde_json::json!({
            "id":item,"class":"wand","quantity":"1","appearance":"ivory wand","identified":true,"identity":"item.unknown","knowledge":ItemKnowledge::default(),"name":"Known wand","known_currency":null,"known_supply":null
        })).unwrap());
    let coin = ItemTarget::from_digest([84; 32]);
    state.observation.inventory.push(
            serde_json::from_value(serde_json::json!({
                "id":coin,"class":"coin","quantity":"9007199254740993",
                "appearance":"unmarked discs","identified":true,"identity":"item.unknown","knowledge":ItemKnowledge::default(),"name":"Known tender",
                "known_currency":"gold","known_supply":null
            }))
            .unwrap(),
        );
    restrict_item_disclosure(&mut state, AccessRole::Spectator);
    let public = Arc::new(state.clone());
    for role in [AccessRole::Player, AccessRole::Spectator] {
        assert!(Arc::ptr_eq(
            &service.item_observation(Arc::clone(&public), role),
            &public
        ));
    }
    state.observation.interactions = Some(InteractionView {
        curse_blocked: vec![],
        completed: vec![],
        refused: vec![],
        slots: vec![],
        preparation: Some(PreparationView {
            action: Action::Zap {
                item,
                aim: tor_protocol::DeviceAim::Direction {
                    direction: Direction::North,
                },
            },
            remaining: 1,
            active: true,
        }),
        identification: Some(IdentificationChoiceView {
            choice: IntentionId("private-paid-choice".into()),
            classes: vec![ItemClass::Wand],
            items: vec![item],
        }),
        inventory: vec![ItemInteractionView {
            portable_light: None,
            item,
            slot: None,
            equipped_slot: None,
            known_cursed: None,
            known_protected: None,
            known_projectile_bonus: Some(ProjectileBonusView { hit: -2, damage: 3 }),
            known_equipment: None,
            drinkable: false,
            edible: false,
            readable: false,
            zappable: true,
            device_activation: Some(tor_protocol::DeviceActivation::Direction),
            charges: Some(7),
        }],
    });
    state.revision += 1;
    let shared = Arc::new(state.clone());
    let mut retained_projections = BTreeMap::new();
    for role in [
        AccessRole::Player,
        AccessRole::Spectator,
        AccessRole::Spectator,
        AccessRole::Player,
    ] {
        let account = Account {
            role,
            user: format!("{role:?}"),
            token: "test-only".into(),
            actors: BTreeSet::from([ActorId(1)]),
        };
        let mut connection = service.connect(&account, "headless".into()).unwrap();
        connection.messages.try_recv().unwrap();
        service.handle(
            connection.id,
            "attach".into(),
            Request::Attach { actor: ActorId(1) },
        );
        while connection.messages.try_recv().is_ok() {}
        service.update(
            connection.id,
            UpdateBody::Observation {
                state: Arc::clone(&shared),
                event: None,
            },
        );
        let frame = connection.messages.try_recv().unwrap();
        let text = serde_json::to_string(&frame).unwrap();
        assert_eq!(
            text.contains("private-paid-choice"),
            role == AccessRole::Player
        );
        let retained = Arc::clone(
            &service.clients[&connection.id]
                .last_observation
                .as_ref()
                .unwrap()
                .state,
        );
        let owner = role == AccessRole::Player;
        assert_eq!(Arc::ptr_eq(&retained, &shared), owner);
        if let Some(previous) = retained_projections.insert(owner, Arc::clone(&retained)) {
            assert_eq!(
                previous, retained,
                "equal role projections retain exact disclosed content"
            );
        }
        let disclosed = &retained.observation;
        let tender = disclosed
            .inventory
            .iter()
            .find(|row| row.id == coin)
            .unwrap();
        assert_eq!(tender.known_currency, Some(CurrencyKind::Gold));
        assert_eq!(tender.quantity, 9_007_199_254_740_993);
        assert!(text.contains("\"known_currency\":\"gold\""));
        let interaction = disclosed.interactions.as_ref().unwrap();
        assert_eq!(
            interaction.identification.is_some(),
            role == AccessRole::Player
        );
        assert_eq!(
            interaction.inventory[0].charges,
            (role == AccessRole::Player).then_some(7)
        );
        assert!(
            interaction.preparation.is_some(),
            "ordinary preparation remains visible"
        );
        assert!(interaction.inventory[0].zappable);
        assert_eq!(
            interaction.inventory[0].known_projectile_bonus,
            owner.then_some(ProjectileBonusView { hit: -2, damage: 3 })
        );
        // Bonus alone must trigger the shared-observation sanitation fast path.
        let mut bonus_only = state.clone();
        let facts = bonus_only.observation.interactions.as_mut().unwrap();
        facts.identification = None;
        facts.inventory[0].charges = None;
        let projected = service.item_observation(Arc::new(bonus_only), role);
        assert_eq!(
            projected
                .observation
                .interactions
                .as_ref()
                .unwrap()
                .inventory[0]
                .known_projectile_bonus,
            owner.then_some(ProjectileBonusView { hit: -2, damage: 3 })
        );
        // The same restriction owns snapshots as well as delta bases.
        let mut snapshot_state = state.clone();
        restrict_item_disclosure(&mut snapshot_state, role);
        assert_eq!(snapshot_state.observation, *disclosed);
    }
    assert_eq!(
        shared.as_ref(),
        &state,
        "spectator projection cannot mutate shared owner state"
    );
}

#[test]
fn semantic_item_facts_require_projection_even_without_interactions() {
    let service = Service::new(Engine::memory(Scenario::two_room(0)).unwrap());
    for ground in [false, true] {
        for fact in 0..4 {
            let mut state = service.engine.state(ActorId(1)).unwrap();
            state.observation.inventory.clear();
            state.observation.ground_items.clear();
            state.observation.interactions = None;
            let mut item: ItemView = serde_json::from_value(serde_json::json!({
                "id":ItemTarget::from_digest([79; 32]), "class":"ring",
                "quantity":"1", "appearance":"silver ring", "identified":true,
                "identity":"item.unknown", "knowledge":ItemKnowledge::default(),
                "name":"Known ring", "known_currency":null, "known_supply":null
            }))
            .unwrap();
            match fact {
                0 => item.identity = "item.ring.fixture".parse().unwrap(),
                1 => item.knowledge.condition.protected = Some(false),
                2 => item.knowledge.enchantments.hit = Some(0),
                _ => item.knowledge.enchantments.potency = Some(-1),
            }
            if ground {
                state.observation.ground_items.push(GroundItemView {
                    presentation: None,
                    reachable: true,
                    supply_reachable: false,
                    position: Position { x: 0, y: 0, z: 0 },
                    item,
                });
            } else {
                state.observation.inventory.push(item);
            }
            let owner = Arc::new(state);
            assert!(Arc::ptr_eq(
                &owner,
                &service.item_observation(Arc::clone(&owner), AccessRole::Player)
            ));
            let spectator = service.item_observation(Arc::clone(&owner), AccessRole::Spectator);
            assert!(
                !Arc::ptr_eq(&owner, &spectator),
                "ground={ground}, fact={fact}"
            );
            let mut expected = owner.as_ref().clone();
            let item = if ground {
                &mut expected.observation.ground_items[0].item
            } else {
                &mut expected.observation.inventory[0]
            };
            item.identity = ItemClass::Misc.unknown_identity();
            item.knowledge = ItemKnowledge::default();
            assert_eq!(spectator.as_ref(), &expected);
            // A previously authorized attachment cannot reuse its old owner
            // projection after its role changes; public projections can share.
            assert_eq!(
                service.item_observation(Arc::clone(&owner), AccessRole::Spectator),
                spectator
            );
            assert!(Arc::ptr_eq(
                &spectator,
                &service.item_observation(Arc::clone(&spectator), AccessRole::Spectator)
            ));
            assert!(has_private_item_disclosure(&owner));
            // Read authority follows AccessRole, not command ownership.
            // A Player without any controller lease still reads the source;
            // returning from a spectator projection must not poison it.
            assert!(service.controllers.is_empty());
            let player_again = service.item_observation(Arc::clone(&owner), AccessRole::Player);
            assert!(Arc::ptr_eq(&player_again, &owner));
            assert!(has_private_item_disclosure(&player_again));
            let second_observer =
                service.item_observation(Arc::clone(&owner), AccessRole::Spectator);
            assert_eq!(second_observer, spectator);
            assert!(!Arc::ptr_eq(&second_observer, &owner));
        }
    }
}

#[test]
fn each_private_equipment_fact_is_sanitized_without_losing_public_arc_sharing() {
    let mut service = Service::new(Engine::memory(Scenario::two_room(0)).unwrap());
    let item = ItemTarget::from_digest([73; 32]);
    let mut public = service.engine.state(ActorId(1)).unwrap();
    public.observation.inventory.push(
            serde_json::from_value(serde_json::json!({
                "id":item,"class":"ring","quantity":"1","appearance":"silver ring",
                "identified":true,"identity":"item.unknown","knowledge":ItemKnowledge::default(),"name":"Known ring","known_currency":null,"known_supply":null
            }))
            .unwrap(),
        );
    public.observation.interactions = Some(InteractionView {
        curse_blocked: vec![],
        completed: vec![],
        refused: vec![],
        slots: vec![],
        preparation: None,
        identification: None,
        inventory: vec![ItemInteractionView {
            portable_light: None,
            item,
            slot: Some(EquipmentSlot {
                id: "fixture_ring".into(),
                name: "Ring".into(),
                equipped_phrase: "worn".into(),
                requires_attack: false,
            }),
            equipped_slot: None,
            known_equipment: None,
            known_projectile_bonus: None,
            known_cursed: None,
            known_protected: None,
            drinkable: false,
            edible: false,
            readable: false,
            zappable: false,
            device_activation: None,
            charges: None,
        }],
    });
    restrict_item_disclosure(&mut public, AccessRole::Spectator);
    public.revision += 1;
    assert!(public.validate().is_ok());
    let public = Arc::new(public);
    for role in [AccessRole::Player, AccessRole::Spectator] {
        assert!(
            Arc::ptr_eq(
                &public,
                &service.item_observation(Arc::clone(&public), role)
            ),
            "truly public updates must preserve shared storage"
        );
    }
    for fact in 0..9 {
        let mut owner = public.as_ref().clone();
        let interaction = owner.observation.interactions.as_mut().unwrap();
        match fact {
            0 => {
                interaction.inventory[0].known_equipment = Some(EquipmentView {
                    attack: None,
                    modifiers: vec![],
                })
            }
            1 => interaction.inventory[0].known_cursed = Some(true),
            2 => interaction.inventory[0].known_protected = Some(true),
            3 => interaction.curse_blocked.push(item),
            4 => interaction.inventory[0].known_cursed = Some(false),
            5 => interaction.inventory[0].known_protected = Some(false),
            6 => {
                owner.observation.inventory.last_mut().unwrap().identity =
                    "item.ring.fixture".parse().unwrap()
            }
            7 => {
                owner
                    .observation
                    .inventory
                    .last_mut()
                    .unwrap()
                    .knowledge
                    .condition
                    .protected = Some(false)
            }
            _ => {
                owner
                    .observation
                    .inventory
                    .last_mut()
                    .unwrap()
                    .knowledge
                    .enchantments
                    .potency = Some(-1)
            }
        }
        assert!(owner.validate().is_ok(), "fact {fact}");
        let owner = Arc::new(owner);
        let owner_projection = service.item_observation(Arc::clone(&owner), AccessRole::Player);
        assert!(Arc::ptr_eq(&owner, &owner_projection));
        let projected = service.item_observation(Arc::clone(&owner), AccessRole::Spectator);
        assert_eq!(
            projected.as_ref(),
            public.as_ref(),
            "private fact {fact} leaked through the fast path"
        );
        assert!(!Arc::ptr_eq(&owner, &projected));
        for role in [AccessRole::Player, AccessRole::Spectator] {
            let account = Account {
                role,
                user: format!("{role:?}-{fact}"),
                token: "test-only".into(),
                actors: BTreeSet::from([ActorId(1)]),
            };
            let mut connection = service.connect(&account, "headless".into()).unwrap();
            connection.messages.try_recv().unwrap();
            service.handle(
                connection.id,
                "attach".into(),
                Request::Attach { actor: ActorId(1) },
            );
            while connection.messages.try_recv().is_ok() {}
            let base = Arc::clone(
                &service.clients[&connection.id]
                    .last_observation
                    .as_ref()
                    .unwrap()
                    .state,
            );
            service.update(
                connection.id,
                UpdateBody::Observation {
                    state: Arc::clone(&owner),
                    event: None,
                },
            );
            let frame = connection.messages.try_recv_frame().unwrap();
            let ServerMessage::Update { update } = decode_response(&frame.text).unwrap() else {
                panic!("observation update expected")
            };
            let received = match update.body {
                UpdateBody::Observation { state, .. } => state.as_ref().clone(),
                UpdateBody::ObservationDelta { state, .. } => state.apply(&base).unwrap(),
                _ => panic!("observation body expected"),
            };
            let retained = &service.clients[&connection.id]
                .last_observation
                .as_ref()
                .unwrap()
                .state;
            let expected = if role == AccessRole::Spectator {
                &public
            } else {
                &owner
            };
            assert_eq!(
                retained.as_ref(),
                expected.as_ref(),
                "retained update role {role:?}, fact {fact}"
            );
            assert_eq!(
                &received,
                expected.as_ref(),
                "decoded update role {role:?}, fact {fact}"
            );
            let mut snapshot = owner.as_ref().clone();
            restrict_item_disclosure(&mut snapshot, role);
            assert_eq!(&snapshot, expected.as_ref());
            // Each fact is an independent projection case, not a test of
            // accumulating live connections beyond the host's finite cap.
            service.disconnect(connection.id);
            drop(connection);
        }
    }
}

#[test]
fn rejected_connection_preserves_identity_and_waits_for_transport_release() {
    let frame = tor_protocol::MAX_RESPONSE_BYTES;
    let mut service = Service::with_outbound_limits(
        Engine::memory(Scenario::two_room(0)).unwrap(),
        crate::OutboundLimits {
            frame_bytes: frame,
            client_bytes: frame,
            total_bytes: frame,
        },
    )
    .unwrap();
    let account = Account {
        role: AccessRole::Spectator,
        user: "observer".into(),
        token: "test-only".into(),
        actors: BTreeSet::from([ActorId(1)]),
    };
    let connection = service.connect(&account, "first".into()).unwrap();
    service.disconnect(connection.id);
    let next = service.next_client;
    assert_eq!(
        service
            .connect(&account, "rejected".into())
            .err()
            .unwrap()
            .code,
        ErrorCode::ResourceLimit
    );
    assert_eq!(service.next_client, next);
    drop(connection);
    let replacement = service.connect(&account, "replacement".into()).unwrap();
    assert_eq!(replacement.id, next);
}

#[test]
fn action_broadcast_observes_once_per_actor_with_independent_client_streams() {
    for watchers in [1, 8, 32] {
        let mut limits = crate::OutboundLimits::default();
        limits.total_bytes = limits.total_bytes.max(watchers * limits.frame_bytes);
        let mut service =
            Service::with_outbound_limits(Engine::memory(Scenario::two_room(0)).unwrap(), limits)
                .unwrap();
        let account = Account {
            role: AccessRole::Spectator,
            user: "observer".into(),
            token: "test-only".into(),
            actors: BTreeSet::from([ActorId(1)]),
        };
        let mut connections = Vec::new();
        for index in 0..watchers {
            let mut client = service
                .connect(&account, format!("observer-{index}"))
                .unwrap();
            client.messages.try_recv().unwrap();
            service.handle(
                client.id,
                "attach".into(),
                Request::Attach { actor: ActorId(1) },
            );
            let ServerMessage::Snapshot { snapshot, .. } = client.messages.try_recv().unwrap()
            else {
                panic!("initial snapshot")
            };
            connections.push((client, snapshot));
        }
        assert_eq!(
            connections
                .iter()
                .map(|(_, snapshot)| snapshot.context.stream.0.clone())
                .collect::<BTreeSet<_>>()
                .len(),
            watchers
        );
        assert_eq!(
            connections
                .iter()
                .map(|(_, snapshot)| snapshot.state.snapshot.clone())
                .collect::<BTreeSet<_>>()
                .len(),
            watchers,
            "capabilities belong to attachments, not shared display counters"
        );
        // The test transport decodes serialized messages, so the received
        // snapshot has an independent Arc. Preserve the server allocations
        // separately to prove that causal updates never replace them.
        let retained_pictures: BTreeMap<_, _> = connections
            .iter()
            .map(|(client, _)| {
                (
                    client.id,
                    Arc::clone(
                        &service.clients[&client.id]
                            .last_observation
                            .as_ref()
                            .unwrap()
                            .state,
                    ),
                )
            })
            .collect();
        for (index, action) in [
            crate::journal::Action::Wait,
            crate::journal::Action::Move {
                direction: crate::journal::Direction::East,
            },
            crate::journal::Action::Move {
                direction: crate::journal::Direction::West,
            },
            crate::journal::Action::Wait,
        ]
        .into_iter()
        .enumerate()
        {
            let before_states = service.semantic_states().unwrap();
            let result = service
                .engine
                .command(
                    "test",
                    "test",
                    ActorId(1),
                    &format!("wait-{index}"),
                    &service.engine.branch().clone(),
                    crate::journal::Command::Act {
                        expected_boundary: service.engine.causal_boundary(ActorId(1)).unwrap(),
                        action,
                    },
                )
                .unwrap();
            let before = tor_simulation::diagnostics::work_counts();
            let pictures_before = Engine::picture_adaptation_count();
            service
                .action_result_update(&before_states, &result)
                .unwrap();
            let after = tor_simulation::diagnostics::work_counts();
            assert_eq!(
                Engine::picture_adaptation_count() - pictures_before,
                if index == 1 || index == 2 { 2 } else { 0 },
                "canonical fallback is once per actor, not per {watchers} watchers"
            );
            assert_eq!(
                after.observations - before.observations,
                1,
                "watchers: {watchers}"
            );
            for (client, snapshot) in &mut connections {
                let ServerMessage::Update { update } = client.messages.try_recv().unwrap() else {
                    panic!("outcome update")
                };
                assert_eq!(
                    update.cursor.sequence,
                    snapshot.cursor.sequence + index as u64 + 1
                );
                assert_eq!(update.branch, snapshot.branch);
                let UpdateBody::Outcome {
                    event: Some(event),
                    facts,
                    ..
                } = update.body
                else {
                    panic!("ordered outcome")
                };
                assert_eq!(event.id, result.entry.id);
                assert!(facts
                    .facts
                    .iter()
                    .any(|fact| matches!(fact, OutcomeFact::OwnAction { .. })));
                let retained = &service.clients[&client.id]
                    .last_observation
                    .as_ref()
                    .unwrap()
                    .state;
                assert!(
                    Arc::ptr_eq(retained, &retained_pictures[&client.id]),
                    "causal updates do not mutate or replace retained pictures"
                );
                assert_eq!(retained.as_ref(), snapshot.state.as_ref());
            }
        }
    }
}

#[test]
fn slow_controller_during_rewind_cannot_send_control_into_the_old_branch() {
    let mut engine = Engine::memory(Scenario::two_room(0)).unwrap();
    engine.enable_wizard().unwrap();
    let mut service = Service::new(engine);
    let account = Account {
        role: AccessRole::Wizard,
        user: "wizard".into(),
        token: "test".into(),
        actors: BTreeSet::from([ActorId(1)]),
    };
    let mut controller = service.connect(&account, "text".into()).unwrap();
    let mut observer = service.connect(&account, "ascii".into()).unwrap();
    for c in [&mut controller, &mut observer] {
        c.messages.try_recv().unwrap();
        service.handle(c.id, "attach".into(), Request::Attach { actor: ActorId(1) });
        c.messages.try_recv().unwrap();
    }
    service.handle(controller.id, "control".into(), Request::AcquireControl);
    assert!(matches!(controller.messages.try_recv().unwrap(),
            ServerMessage::Update { update } if matches!(update.body, UpdateBody::Control { has_control: true })));
    assert!(matches!(controller.messages.try_recv().unwrap(),
            ServerMessage::Update { update } if matches!(&update.body, UpdateBody::Readiness { readiness } if readiness.admission)));
    assert!(matches!(controller.messages.try_recv().unwrap(),
            ServerMessage::Ack { request_id, .. } if request_id == "control"));
    while observer.messages.try_recv().is_ok() {}
    for i in 0..QUEUE {
        service.handle(controller.id, format!("snapshot-{i}"), Request::Snapshot);
    }
    service.autonomous_enabled = true;
    let branch = service.engine.branch().clone();
    service.handle(
        observer.id,
        "rewind".into(),
        Request::Command {
            context: service.input_context(observer.id).unwrap(),
            branch: branch.clone(),
            command: Command::Wizard {
                expected_revision: service.clients[&observer.id]
                    .last_observation
                    .as_ref()
                    .unwrap()
                    .state
                    .revision,
                operation: "rewind initial".into(),
            },
        },
    );
    assert!(!service.autonomous_enabled);
    assert!(controller.close.borrow().is_some());
    let ServerMessage::Snapshot { snapshot, .. } = observer.messages.try_recv().unwrap() else {
        panic!("snapshot must precede control transition")
    };
    assert_ne!(snapshot.branch, branch);
    let ServerMessage::Update { update } = observer.messages.try_recv().unwrap() else {
        panic!("control")
    };
    assert_eq!(update.branch, snapshot.branch);
    assert!(matches!(
        update.body,
        UpdateBody::Control { has_control: false }
    ));
}

#[test]
fn disconnect_during_admission_broadcast_does_not_execute_queued_work() {
    let mut service = Service::new(Engine::memory(Scenario::two_room(0)).unwrap());
    let account = Account {
        role: tor_protocol::AccessRole::Player,
        user: "alice".into(),
        token: "test-only".into(),
        actors: BTreeSet::from([ActorId(1)]),
    };
    let mut controller = service.connect(&account, "text".into()).unwrap();
    let mut observer = service.connect(&account, "ascii".into()).unwrap();
    for client in [&mut controller, &mut observer] {
        client.messages.try_recv().unwrap();
        service.handle(
            client.id,
            "attach".into(),
            Request::Attach { actor: ActorId(1) },
        );
        client.messages.try_recv().unwrap();
    }
    service.handle(controller.id, "control".into(), Request::AcquireControl);
    assert!(matches!(controller.messages.try_recv().unwrap(),
            ServerMessage::Update { update } if matches!(update.body, UpdateBody::Control { has_control: true })));
    assert!(matches!(controller.messages.try_recv().unwrap(),
            ServerMessage::Update { update } if matches!(&update.body, UpdateBody::Readiness { readiness } if readiness.admission)));
    assert!(matches!(controller.messages.try_recv().unwrap(),
            ServerMessage::Ack { request_id, .. } if request_id == "control"));
    while observer.messages.try_recv().is_ok() {}
    for i in 0..QUEUE {
        service.handle(controller.id, format!("snapshot-{i}"), Request::Snapshot);
    }
    service.handle(
        controller.id,
        "wait".into(),
        Request::Command {
            context: service.input_context(controller.id).unwrap(),
            branch: service.engine.branch().clone(),
            command: Command::Act {
                action: Action::Wait,
            },
        },
    );
    assert!(controller.close.borrow().is_some());
    let ServerMessage::Update { update } = observer.messages.try_recv().unwrap() else {
        panic!("control update")
    };
    assert!(matches!(
        update.body,
        UpdateBody::Control { has_control: false }
    ));
    assert_eq!(update.cursor.tick, 0);
    let ServerMessage::Update { update } = observer.messages.try_recv().unwrap() else {
        panic!("intention update")
    };
    assert!(matches!(
        update.body,
        UpdateBody::Intention {
            status: IntentionStatus {
                phase: IntentionPhase::Queued,
                ..
            },
            ..
        }
    ));
    assert_eq!(update.cursor.tick, 0);
    assert!(service.engine.has_pending_intention(ActorId(1)));
    assert!(matches!(service.step(), Step::Blocked));
    assert_eq!(
        service.engine.state(ActorId(1)).unwrap().observation.tick,
        0
    );
}

#[test]
fn action_broadcast_survives_cascaded_backpressure_disconnects() {
    let mut service = Service::new(Engine::memory(Scenario::two_room(0)).unwrap());
    let account = Account {
        role: AccessRole::Player,
        user: "owner".into(),
        token: "test-only".into(),
        actors: BTreeSet::from([ActorId(1)]),
    };
    let mut clients = Vec::new();
    for index in 0..3 {
        let mut client = service
            .connect(&account, format!("reader-{index}"))
            .unwrap();
        while client.messages.try_recv().is_ok() {}
        service.handle(
            client.id,
            "attach".into(),
            Request::Attach { actor: ActorId(1) },
        );
        while client.messages.try_recv().is_ok() {}
        clients.push(client);
    }
    service.handle(clients[0].id, "control".into(), Request::AcquireControl);
    for client in &mut clients {
        while client.messages.try_recv().is_ok() {}
    }
    for client in &clients[..2] {
        for _ in 0..QUEUE {
            service.send(client.id, ServerMessage::Waiting { on: Waiting::You });
        }
        assert!(service.clients.contains_key(&client.id));
    }
    let before_states = service.semantic_states().unwrap();
    let result = service
        .engine
        .command(
            "test",
            "test",
            ActorId(1),
            "wait",
            &service.engine.branch().clone(),
            crate::journal::Command::Act {
                expected_boundary: 0,
                action: crate::journal::Action::Wait,
            },
        )
        .unwrap();
    service
        .action_result_update(&before_states, &result)
        .unwrap();
    for client in &clients[..2] {
        assert!(client.close.borrow().is_some());
        assert!(!service.clients.contains_key(&client.id));
    }
    assert!(!service.controllers.contains_key(&ActorId(1)));
    let survivor = &mut clients[2];
    assert!(service.clients.contains_key(&survivor.id));
    let messages: Vec<_> = std::iter::from_fn(|| survivor.messages.try_recv().ok()).collect();
    assert!(messages.iter().any(|message| matches!(message,
            ServerMessage::Update { update } if matches!(update.body,
                UpdateBody::Outcome { .. }))));
}

#[test]
fn slow_client_is_disconnected_and_releases_control_instead_of_losing_updates() {
    let mut service = Service::new(Engine::memory(Scenario::two_room(0)).unwrap());
    let account = Account {
        role: tor_protocol::AccessRole::Player,
        user: "alice".into(),
        token: "test-only".into(),
        actors: BTreeSet::from([ActorId(1)]),
    };
    let mut connection = service.connect(&account, "text".into()).unwrap();
    connection.messages.try_recv().unwrap();
    service.handle(
        connection.id,
        "attach".into(),
        Request::Attach { actor: ActorId(1) },
    );
    connection.messages.try_recv().unwrap();
    service.handle(connection.id, "control".into(), Request::AcquireControl);
    connection.messages.try_recv().unwrap();
    connection.messages.try_recv().unwrap();
    for i in 0..QUEUE + 1 {
        service.handle(connection.id, format!("snapshot-{i}"), Request::Snapshot);
    }
    assert!(connection.close.borrow().is_some());
    assert!(!service.clients.contains_key(&connection.id));
    assert!(!service.controllers.contains_key(&ActorId(1)));
    let mut replacement = service.connect(&account, "ascii".into()).unwrap();
    replacement.messages.try_recv().unwrap();
    service.handle(
        replacement.id,
        "attach".into(),
        Request::Attach { actor: ActorId(1) },
    );
    let ServerMessage::Snapshot { snapshot, .. } = replacement.messages.try_recv().unwrap() else {
        panic!("snapshot")
    };
    assert_eq!(snapshot.cursor.sequence, 0);
    service.handle(replacement.id, "control".into(), Request::AcquireControl);
    assert_eq!(service.controllers.get(&ActorId(1)), Some(&replacement.id));
}

/// A dungeon where, after the character waits, the guard is the next actor.
fn character_waiting_on_a_guard() -> (Service, Connection) {
    let root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/tests/dungeon-loop");
    let mut engine =
        Engine::memory(crate::scenario_package::load(&root, 1, None, false).unwrap()).unwrap();
    engine.enable_wizard().unwrap();
    let mut service = Service::new(engine);
    let account = Account {
        role: AccessRole::Wizard,
        user: "wizard".into(),
        token: "test".into(),
        actors: BTreeSet::from([ActorId(1)]),
    };
    let mut client = service.connect(&account, "headless".into()).unwrap();
    client.messages.try_recv().unwrap();
    service.handle(
        client.id,
        "attach".into(),
        Request::Attach { actor: ActorId(1) },
    );
    while client.messages.try_recv().is_ok() {}
    service.handle(client.id, "control".into(), Request::AcquireControl);
    while client.messages.try_recv().is_ok() {}
    service.handle(
        client.id,
        "wait".into(),
        Request::Command {
            context: service.input_context(client.id).unwrap(),
            branch: service.engine.branch().clone(),
            command: Command::Act {
                action: Action::Wait,
            },
        },
    );
    while client.messages.try_recv().is_ok() {}
    assert!(service.engine.has_pending_intention(ActorId(1)));
    assert!(matches!(service.step(), Step::Progress));
    while client.messages.try_recv().is_ok() {}
    assert!(
        service.engine.next_ai_action().is_some(),
        "the guard must be next"
    );
    (service, client)
}

#[test]
fn ai_turns_run_until_a_controlled_actor_is_due() {
    let (mut service, _client) = character_waiting_on_a_guard();
    let before = service.engine.causal_boundary(ActorId(1)).unwrap();
    service.run_until_blocked();
    assert_eq!(service.engine.next_actor(), Some(ActorId(1)));
    assert_ne!(service.engine.causal_boundary(ActorId(1)).unwrap(), before);
    assert!(matches!(service.step(), Step::Blocked));
}

#[test]
fn an_actions_before_capture_reuses_the_previous_after_capture_exactly() {
    // Each AI turn is admitted then executed; each round's before-capture
    // is the previous round's after-capture of the same commit. Reusing it
    // must equal a fresh capture taken at that moment.
    let (mut service, mut client) = character_waiting_on_a_guard();
    let reused = REUSED_SEMANTIC_CAPTURES.with(std::cell::Cell::get);
    let carried = crate::engine::REUSED_SEMANTIC_FACTS.with(std::cell::Cell::get);
    let pictured = crate::engine::REUSED_SEMANTIC_PICTURES.with(std::cell::Cell::get);
    let keyed = crate::adapt::CellKeys::reused_count();
    let mut compared = 0;
    loop {
        let boundary = service.engine.presentation_boundary();
        if let Some(settled) = &service.settled_views {
            for (actor, view) in &settled.views {
                if !view.taken_at(&boundary) {
                    continue;
                }
                let fresh = service.engine.semantic_observation(*actor).unwrap();
                assert_eq!(
                    view.facts, fresh.facts,
                    "reused facts equal a fresh capture"
                );
                assert_eq!(
                    service.engine.semantic_picture(view).unwrap(),
                    service.engine.semantic_picture(&fresh).unwrap(),
                    "reused picture equals a fresh capture"
                );
                compared += 1;
            }
        }
        let step = service.step();
        while client.messages.try_recv().is_ok() {}
        if !matches!(step, Step::Progress) {
            break;
        }
    }
    assert!(
        compared > 0,
        "positive fixture: a capture was available for reuse"
    );
    assert!(
        REUSED_SEMANTIC_CAPTURES.with(std::cell::Cell::get) > reused,
        "an action reused the previous outcome's capture"
    );
    // An AI admission only queues a decision: the observer's retained view
    // survives it, so its after-capture carries the facts over (each reuse
    // is checked against freshly derived facts in test builds).
    assert!(
        crate::engine::REUSED_SEMANTIC_FACTS.with(std::cell::Cell::get) > carried,
        "an action that left the observer's view unchanged reused its facts"
    );
    // A picture of the same retained view and branch is adapted once
    // (each memoized picture is checked against a fresh one in test builds).
    assert!(
        crate::engine::REUSED_SEMANTIC_PICTURES.with(std::cell::Cell::get) > pictured,
        "a second picture of the same retained view reused the first"
    );
    // A changed view derives its facts and picture again, but cells still
    // in sight keep their salted keys (each reused key is checked against
    // a fresh digest in test builds; verifications are not counted).
    assert!(
        crate::adapt::CellKeys::reused_count() > keyed,
        "a new derivation reused the keys of cells still in sight"
    );
    // Anything a client asks for may change state outside an outcome.
    assert!(service.settled_views.is_some());
    service.handle(client.id, "look".into(), Request::Observe);
    assert!(
        service.settled_views.is_none(),
        "a request clears the reusable capture"
    );
}

#[test]
fn ai_does_not_play_when_nobody_controls_an_actor() {
    let (mut service, client) = character_waiting_on_a_guard();
    service.handle(client.id, "release".into(), Request::ReleaseControl);
    service.run_until_blocked();
    assert!(
        service.engine.next_ai_action().is_some(),
        "the guard must still be waiting for its turn"
    );
}

#[test]
fn a_full_client_queue_pauses_the_run_until_the_client_reads() {
    let (mut service, mut client) = character_waiting_on_a_guard();
    for i in 0..QUEUE - HEADROOM + 1 {
        service.handle(client.id, format!("fill-{i}"), Request::Snapshot);
    }
    let before = service.engine.causal_boundary(ActorId(1)).unwrap();
    let Step::Full(full) = service.step() else {
        panic!("the run must wait for the client");
    };
    assert_eq!(
        full.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
        [client.id]
    );
    assert_eq!(service.engine.causal_boundary(ActorId(1)).unwrap(), before);
    assert!(client.close.borrow().is_none());
    while client.messages.try_recv().is_ok() {}
    service.run_until_blocked();
    assert_eq!(service.engine.next_actor(), Some(ActorId(1)));
    assert_ne!(service.engine.causal_boundary(ActorId(1)).unwrap(), before);
}

#[tokio::test]
async fn replenished_mailbox_cannot_starve_a_due_simulation_action() {
    use crate::runner::Mail;
    use std::sync::{Arc, Mutex};
    use tokio::sync::oneshot;

    fn next_mail(
        sender: mpsc::Sender<Mail>,
        remaining: usize,
        observed: Arc<Mutex<Vec<Option<ActorId>>>>,
        stopped: oneshot::Sender<Option<crate::storage::Store>>,
    ) -> Mail {
        Mail::Call(Box::new(move |service| {
            observed.lock().unwrap().push(service.engine.next_actor());
            let next = if remaining == 0 {
                Mail::Shutdown(stopped)
            } else {
                next_mail(sender.clone(), remaining - 1, observed, stopped)
            };
            // One replacement always fits, so this reproduces a mailbox
            // that never empties without relying on threads or wall time.
            assert!(sender.try_send(next).is_ok());
        }))
    }

    for capacity in [4, 16, 256] {
        let (service, _client) = character_waiting_on_a_guard();
        let (mail, mailbox) = mpsc::channel(capacity);
        let observed = Arc::new(Mutex::new(Vec::new()));
        let (reply, stopped) = oneshot::channel();
        assert!(mail
            .try_send(next_mail(
                mail.clone(),
                capacity * 3,
                observed.clone(),
                reply
            ))
            .is_ok());
        crate::runner::run(service, mailbox, crate::runner::STALL).await;
        stopped.await.unwrap();
        let observed = observed.lock().unwrap();
        assert_ne!(observed[0], Some(ActorId(1)));
        assert_eq!(
            observed[capacity],
            Some(ActorId(1)),
            "due AI was starved by replenished mail at capacity {capacity}"
        );
    }
}

#[tokio::test]
async fn a_closed_full_mailbox_stops_before_the_next_simulation_action() {
    let (service, _client) = character_waiting_on_a_guard();
    let before = service.engine.next_actor();
    let (mail, mailbox) = mpsc::channel(4);
    for _ in 0..4 {
        assert!(mail
            .try_send(crate::runner::Mail::Call(Box::new(|_| {})))
            .is_ok());
    }
    drop(mail);
    let service = crate::runner::run(service, mailbox, crate::runner::STALL).await;
    assert_eq!(service.engine.next_actor(), before);
}

/// Mail that is already waiting, here a rewind against the current
/// revision, is handled before the next action can change that revision.
#[tokio::test]
async fn mail_waiting_in_the_mailbox_is_handled_before_the_next_action() {
    let (service, mut client) = character_waiting_on_a_guard();
    let revision = service.clients[&client.id]
        .last_observation
        .as_ref()
        .unwrap()
        .state
        .revision;
    let branch = service.engine.branch().clone();
    let (mail, mailbox) = mpsc::channel(8);
    mail.send(crate::runner::Mail::Request {
        client: client.id,
        request_id: "rewind".into(),
        request: Box::new(Request::Command {
            context: service.input_context(client.id).unwrap(),
            branch: branch.clone(),
            command: Command::Wizard {
                expected_revision: revision,
                operation: "rewind initial".into(),
            },
        }),
        timing: None,
    })
    .await
    .unwrap();
    let (reply, stopped) = tokio::sync::oneshot::channel();
    mail.send(crate::runner::Mail::Shutdown(reply))
        .await
        .unwrap();
    crate::runner::run(service, mailbox, crate::runner::STALL).await;
    stopped.await.unwrap();
    let mut messages = Vec::new();
    while let Ok(message) = client.messages.try_recv() {
        messages.push(message);
    }
    assert!(
        messages.iter().all(|message| !matches!(
            message,
            ServerMessage::Error {
                code: ErrorCode::StaleRevision,
                ..
            }
        )),
        "waiting command was rejected: {messages:?}"
    );
    assert!(
        messages.iter().any(|message| matches!(
            message,
            ServerMessage::Snapshot { snapshot, .. } if snapshot.branch != branch
        )),
        "rewind did not fork: {messages:?}"
    );
}

#[test]
fn wizard_condition_publishes_private_receipt_without_resetting_other_streams() {
    let root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/rogue-exploration");
    let scenario = crate::scenario_package::load(&root, 42, Some(1), true).unwrap();
    let mut engine = Engine::memory(scenario).unwrap();
    engine.enable_wizard().unwrap();
    let mut service = Service::new(engine);
    let mut clients = Vec::new();
    for (user, role) in [
        ("wizard", AccessRole::Wizard),
        ("observer", AccessRole::Spectator),
    ] {
        let mut client = service
            .connect(
                &Account {
                    role,
                    user: user.into(),
                    token: "test".into(),
                    actors: BTreeSet::from([ActorId(1)]),
                },
                "headless".into(),
            )
            .unwrap();
        while client.messages.try_recv().is_ok() {}
        service.handle(
            client.id,
            "attach".into(),
            Request::Attach { actor: ActorId(1) },
        );
        while client.messages.try_recv().is_ok() {}
        clients.push(client);
    }
    let private = clients[0].id;
    let observer = clients[1].id;
    let observer_stream = service.clients[&observer].context.clone();
    let request = |service: &Service, id| Request::Command {
        context: service.input_context(id).unwrap(),
        branch: service.engine.branch().clone(),
        command: Command::Wizard {
            expected_revision: service.clients[&id]
                .last_observation
                .as_ref()
                .unwrap()
                .state
                .revision,
            operation: "condition apply self rogue.frozen 200".into(),
        },
    };
    service.handle(observer, "denied".into(), request(&service, observer));
    assert!(
        std::iter::from_fn(|| clients[1].messages.try_recv().ok()).any(|message| matches!(
            message,
            ServerMessage::Error {
                code: ErrorCode::Unauthorized,
                ..
            }
        ))
    );
    service.autonomous_enabled = true;
    service.handle(
        private,
        "apply-condition".into(),
        request(&service, private),
    );
    assert!(service.autonomous_enabled);
    assert_eq!(service.clients[&observer].context, observer_stream);
    let private_messages: Vec<_> =
        std::iter::from_fn(|| clients[0].messages.try_recv().ok()).collect();
    assert!(
        private_messages.iter().any(|message| matches!(message,
            ServerMessage::Snapshot { snapshot, .. } if snapshot.history.entries.iter().any(|entry|
                matches!(&entry.content, HistoryContent::Wizard { summary, .. }
                    if summary == "Actor 1 condition rogue.frozen: applied."))
        )),
        "{private_messages:?}"
    );
    let observer_messages: Vec<_> =
        std::iter::from_fn(|| clients[1].messages.try_recv().ok()).collect();
    assert!(!observer_messages.iter().any(|message| matches!(message,
            ServerMessage::Update { update, .. } if matches!(&update.body,
                UpdateBody::Outcome { event: Some(entry), .. } if matches!(entry.content, HistoryContent::Wizard { .. }))
        )));
}

#[test]
fn combat_diagnostics_query_is_private_and_rejects_stale_controls() {
    let mut engine = Engine::memory(crate::Scenario::two_room(42)).unwrap();
    engine.enable_wizard().unwrap();
    let mut service = Service::new(engine);
    let mut clients = Vec::new();
    for (user, role) in [
        ("wizard", AccessRole::Wizard),
        ("player", AccessRole::Player),
    ] {
        let mut client = service
            .connect(
                &Account {
                    role,
                    user: user.into(),
                    token: "test".into(),
                    actors: BTreeSet::from([ActorId(1)]),
                },
                "headless".into(),
            )
            .unwrap();
        while client.messages.try_recv().is_ok() {}
        service.handle(
            client.id,
            "attach".into(),
            Request::Attach { actor: ActorId(1) },
        );
        while client.messages.try_recv().is_ok() {}
        clients.push(client);
    }
    let before = service.engine.state(ActorId(1)).unwrap();
    let history = service
        .engine
        .history(ActorId(1), "wizard", None, 100)
        .unwrap();
    for (index, case, operation, expected) in [
        (1, 0, "combat capture on", Some(ErrorCode::Unauthorized)),
        (1, 0, "combat inspect bad", Some(ErrorCode::Unauthorized)),
        (0, 1, "combat capture on", Some(ErrorCode::StaleContext)),
        (0, 2, "combat capture on", Some(ErrorCode::WrongBranch)),
        (0, 3, "combat capture on", Some(ErrorCode::StaleContext)),
        (0, 0, "combat capture on", None),
        (0, 1, "combat capture off", Some(ErrorCode::StaleContext)),
        (0, 0, "combat inspect", None),
        (0, 0, "combat inspect 1", Some(ErrorCode::InvalidAction)),
        (0, 0, "combat capture off", None),
    ] {
        let enabled_before = service
            .engine
            .inspect_combat_diagnostics(None)
            .unwrap()
            .enabled;
        let id = clients[index].id;
        let mut context = service.input_context(id).unwrap();
        if case == 3 {
            context.readiness_revision += 1;
        }
        service.handle(
            id,
            "diagnostics".into(),
            Request::Command {
                context,
                branch: if case == 2 {
                    BranchId("absent".into())
                } else {
                    service.engine.branch().clone()
                },
                command: Command::Wizard {
                    expected_revision: service.clients[&id]
                        .last_observation
                        .as_ref()
                        .unwrap()
                        .state
                        .revision
                        + u64::from(case == 1),
                    operation: operation.into(),
                },
            },
        );
        let messages: Vec<_> =
            std::iter::from_fn(|| clients[index].messages.try_recv().ok()).collect();
        if let Some(code) = expected {
            assert!(
                messages.iter().any(|message| matches!(message,
                    ServerMessage::Error { code: actual, .. } if *actual == code)),
                "{messages:?}"
            );
            assert!(!messages
                .iter()
                .any(|message| matches!(message, ServerMessage::CombatDiagnostics { .. })));
            assert_eq!(
                service
                    .engine
                    .inspect_combat_diagnostics(None)
                    .unwrap()
                    .enabled,
                enabled_before
            );
        } else {
            let (context, report) = messages
                .iter()
                .find_map(|message| match message {
                    ServerMessage::CombatDiagnostics {
                        context,
                        request_id,
                        report,
                    } if request_id == "diagnostics" => Some((context, report)),
                    _ => None,
                })
                .expect("private diagnostics reply");
            assert!(report.validate().is_ok());
            assert_eq!(
                report.enabled,
                operation != "combat capture off",
                "{operation}"
            );
            assert_eq!(context.actor, ActorId(1));
            assert_eq!(context.input, service.input_context(id).unwrap());
        }
        assert!(clients[1 - index].messages.try_recv().is_err());
        assert_eq!(service.engine.state(ActorId(1)).unwrap(), before);
        assert_eq!(
            service
                .engine
                .history(ActorId(1), "wizard", None, 100)
                .unwrap(),
            history
        );
    }
}

#[test]
fn creature_inspection_query_is_private_readonly_and_contextual() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/mob-arena");
    let mut engine =
        Engine::memory(crate::scenario_package::load(&root, 42, None, false).unwrap()).unwrap();
    engine.enable_wizard().unwrap();
    let mut service = Service::new(engine);
    let mut clients = Vec::new();
    for (user, role) in [
        ("wizard", AccessRole::Wizard),
        ("player", AccessRole::Player),
    ] {
        let mut client = service
            .connect(
                &Account {
                    role,
                    user: user.into(),
                    token: "test".into(),
                    actors: BTreeSet::from([ActorId(1)]),
                },
                "headless".into(),
            )
            .unwrap();
        while client.messages.try_recv().is_ok() {}
        service.handle(
            client.id,
            "attach".into(),
            Request::Attach { actor: ActorId(1) },
        );
        while client.messages.try_recv().is_ok() {}
        clients.push(client);
    }
    let before = service.engine.state(ActorId(1)).unwrap();
    let history = service
        .engine
        .history(ActorId(1), "wizard", None, 100)
        .unwrap();
    for (index, target) in [(0, 2), (1, 2), (1, 999)] {
        let id = clients[index].id;
        service.handle(
            id,
            "inspect".into(),
            Request::Command {
                context: service.input_context(id).unwrap(),
                branch: service.engine.branch().clone(),
                command: Command::Wizard {
                    expected_revision: service.clients[&id]
                        .last_observation
                        .as_ref()
                        .unwrap()
                        .state
                        .revision,
                    operation: format!("creature inspect {target}"),
                },
            },
        );
        let messages: Vec<_> =
            std::iter::from_fn(|| clients[index].messages.try_recv().ok()).collect();
        if index == 0 {
            let (context, report) = messages
                .iter()
                .find_map(|message| match message {
                    ServerMessage::CreatureInspection {
                        context,
                        request_id,
                        report,
                    } if request_id == "inspect" => Some((context, report)),
                    _ => None,
                })
                .expect("private inspection reply");
            assert_eq!(report.actor, ActorId(2));
            assert_eq!(report.name, "blue sentinel");
            assert!(report.validate().is_ok());
            assert_eq!(context.actor, ActorId(1));
            assert_eq!(context.input, service.input_context(id).unwrap());
            assert!(clients[1].messages.try_recv().is_err());
        } else {
            assert!(messages.iter().any(|message| matches!(
                message,
                ServerMessage::Error {
                    code: ErrorCode::Unauthorized,
                    ..
                }
            )));
            assert!(!messages
                .iter()
                .any(|message| matches!(message, ServerMessage::CreatureInspection { .. })));
        }
        assert_eq!(service.engine.state(ActorId(1)).unwrap(), before);
        assert_eq!(
            service
                .engine
                .history(ActorId(1), "wizard", None, 100)
                .unwrap(),
            history
        );
    }
    for (case, code) in [
        (0, ErrorCode::StaleContext),
        (1, ErrorCode::WrongBranch),
        (2, ErrorCode::StaleContext),
        (3, ErrorCode::InvalidAction),
    ] {
        let id = clients[0].id;
        let mut context = service.input_context(id).unwrap();
        if case == 2 {
            context.readiness_revision += 1;
        }
        service.handle(
            id,
            format!("invalid-{case}"),
            Request::Command {
                context,
                branch: if case == 1 {
                    BranchId("absent".into())
                } else {
                    service.engine.branch().clone()
                },
                command: Command::Wizard {
                    expected_revision: service.clients[&id]
                        .last_observation
                        .as_ref()
                        .unwrap()
                        .state
                        .revision
                        + u64::from(case == 0),
                    operation: format!("creature inspect {}", if case == 3 { 999 } else { 2 }),
                },
            },
        );
        let messages: Vec<_> = std::iter::from_fn(|| clients[0].messages.try_recv().ok()).collect();
        assert!(messages.iter().any(|message| matches!(message, ServerMessage::Error { code: actual, .. } if *actual == code)), "{messages:?}");
        assert!(!messages
            .iter()
            .any(|message| matches!(message, ServerMessage::CreatureInspection { .. })));
        assert_eq!(service.engine.state(ActorId(1)).unwrap(), before);
    }
}
