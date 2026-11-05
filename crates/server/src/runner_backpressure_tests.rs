use super::*;
use tor_protocol::{AccessRole, ActorId, ErrorCode, ServerMessage};

/// A real SQLite writer cannot commit the admitted record until the test
/// releases this lock. An overdue status is established before exercising ingress.
fn stalled_service() -> (tempfile::TempDir, rusqlite::Connection, Service) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("stalled.db");
    let mut engine = crate::Engine::open_with_policy(
        &path,
        crate::Scenario::two_room(42),
        crate::SavePolicy {
            target_interval: Duration::from_millis(1),
            max_unsaved_age: Duration::from_millis(1),
            idle_interval: Duration::ZERO,
            checkpoint_interval: 0,
            ..crate::SavePolicy::default()
        },
    )
    .unwrap();
    let lock = rusqlite::Connection::open(path).unwrap();
    lock.execute_batch("BEGIN IMMEDIATE").unwrap();
    engine
        .command(
            "seed",
            "test",
            ActorId(1),
            "pending",
            &engine.branch().clone(),
            crate::journal::Command::Act {
                expected_boundary: engine.causal_boundary(ActorId(1)).unwrap(),
                action: crate::journal::Action::Wait,
            },
        )
        .unwrap();
    std::thread::sleep(Duration::from_millis(5));
    assert!(engine.save_status().overdue);
    assert_eq!(engine.save_status().durable_sequence, 0);
    (dir, lock, Service::new(engine))
}

fn connect(service: &mut Service, user: &str) -> Connection {
    let mut client = service
        .connect(
            &Account {
                user: user.into(),
                token: "test".into(),
                role: AccessRole::Player,
                actors: [ActorId(1)].into(),
            },
            "test".into(),
        )
        .unwrap();
    assert!(matches!(
        client.messages.try_recv().unwrap(),
        ServerMessage::Welcome { .. }
    ));
    client
}

fn commit(client: u64, request_id: impl Into<String>) -> Mail {
    Mail::Request {
        client,
        request_id: request_id.into(),
        request: Request::CommitAttachment {
            upload_id: "pending-upload".into(),
        },
        timing: None,
    }
}

#[test]
fn stalled_ingress_is_bounded_and_overflow_only_disconnects_the_sender() {
    let (_dir, lock, mut service) = stalled_service();
    let flood = connect(&mut service, "flood");
    let mut peer = connect(&mut service, "peer");
    let mut held = VecDeque::new();
    assert!(deliver(&mut service, commit(peer.id, "first"), &mut held));
    for index in 0..1024 {
        assert!(deliver(
            &mut service,
            commit(flood.id, format!("flood-{index}")),
            &mut held,
        ));
        assert!(held.len() <= 256, "retained {} requests", held.len());
    }
    assert_eq!(
        *flood.close.borrow(),
        Some(crate::session::DisconnectMode::Abort)
    );
    assert!(peer.close.borrow().is_none());
    assert_eq!(
        held.len(),
        1,
        "disconnected requests must release their storage"
    );
    assert!(
        matches!(held.front(), Some(Mail::Request { request_id, .. }) if request_id == "first")
    );
    assert!(deliver(
        &mut service,
        Mail::Request {
            client: peer.id,
            request_id: "query".into(),
            request: Request::Snapshot,
            timing: None,
        },
        &mut held,
    ));
    assert!(
        matches!(peer.messages.try_recv().unwrap(), ServerMessage::Error {
        request_id: Some(id), code: ErrorCode::NotAttached, ..
    } if id == "query")
    );
    let (reply, mut stopped) = oneshot::channel();
    assert!(!deliver(&mut service, Mail::Shutdown(reply), &mut held));
    assert!(stopped.try_recv().unwrap().is_some());
    lock.execute_batch("ROLLBACK").unwrap();
}

#[test]
fn disconnect_reclaims_held_requests_without_reordering_the_survivors() {
    let (_dir, lock, mut service) = stalled_service();
    let departed = connect(&mut service, "departed");
    let peer = connect(&mut service, "peer");
    let mut held = VecDeque::new();
    for (client, request) in [(peer.id, "first"), (departed.id, "gone"), (peer.id, "last")] {
        assert!(deliver(&mut service, commit(client, request), &mut held));
    }
    assert!(deliver(
        &mut service,
        Mail::Disconnect(departed.id),
        &mut held
    ));
    let requests: Vec<_> = held
        .iter()
        .map(|mail| match mail {
            Mail::Request { request_id, .. } => request_id.as_str(),
            _ => panic!("only requests are deferred"),
        })
        .collect();
    assert_eq!(requests, ["first", "last"]);
    // A request already waiting in the mailbox cannot retain a dead connection.
    assert!(deliver(
        &mut service,
        commit(departed.id, "late"),
        &mut held
    ));
    assert_eq!(held.len(), 2);
    lock.execute_batch("ROLLBACK").unwrap();
}

#[tokio::test]
async fn real_transport_overflow_closes_flooding_stream_but_keeps_queries_available() {
    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::{connect_async, tungstenite::Message};
    use tor_protocol::{ClientMessage, PROTOCOL_VERSION};

    let (_dir, lock, service) = stalled_service();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = format!("ws://{}", listener.local_addr().unwrap());
    let (stop, shutdown) = oneshot::channel();
    let server = tokio::spawn(crate::serve(
        listener,
        Simulation::start(service),
        vec![Account {
            user: "test".into(),
            token: "test".into(),
            role: AccessRole::Player,
            actors: [ActorId(1)].into(),
        }],
        async {
            let _ = shutdown.await;
        },
    ));
    let outcome = tokio::time::timeout(Duration::from_secs(10), async {
        let mut clients = Vec::new();
        for _ in 0..2 {
            let (mut socket, _) = connect_async(&address).await.unwrap();
            socket
                .send(Message::Text(
                    serde_json::to_string(&ClientMessage::Hello {
                        protocol: PROTOCOL_VERSION,
                        token: "test".into(),
                        frontend: "test".into(),
                    })
                    .unwrap()
                    .into(),
                ))
                .await
                .unwrap();
            let frame = socket.next().await.unwrap().unwrap();
            assert!(matches!(
                serde_json::from_str::<ServerMessage>(frame.to_text().unwrap()).unwrap(),
                ServerMessage::Welcome { .. }
            ));
            clients.push(socket);
        }
        let mut peer = clients.pop().unwrap();
        let mut flood = clients.pop().unwrap();
        for index in 0..257 {
            let request = ClientMessage::Request {
                request_id: format!("flood-{index}"),
                request: Box::new(Request::CommitAttachment {
                    upload_id: "missing".into(),
                }),
            };
            // The final send may race the required transport reset.
            if flood
                .send(Message::Text(
                    serde_json::to_string(&request).unwrap().into(),
                ))
                .await
                .is_err()
            {
                break;
            }
        }
        loop {
            match flood.next().await {
                None | Some(Err(_)) | Some(Ok(Message::Close(_))) => break,
                Some(Ok(Message::Text(text))) => {
                    // Saving warnings may precede closure, but no fabricated
                    // resource-limit reply can deny an earlier receipt.
                    assert!(matches!(
                        serde_json::from_str::<ServerMessage>(&text).unwrap(),
                        ServerMessage::Error {
                            request_id: None,
                            code: ErrorCode::StorageFailure,
                            ..
                        }
                    ));
                }
                other => panic!("unexpected flood output: {other:?}"),
            }
        }
        peer.send(Message::Text(
            serde_json::to_string(&ClientMessage::Request {
                request_id: "query".into(),
                request: Box::new(Request::Snapshot),
            })
            .unwrap()
            .into(),
        ))
        .await
        .unwrap();
        loop {
            let frame = peer.next().await.unwrap().unwrap();
            let message: ServerMessage = serde_json::from_str(frame.to_text().unwrap()).unwrap();
            if matches!(message, ServerMessage::Error {
                request_id: Some(ref id), code: ErrorCode::NotAttached, ..
            } if id == "query")
            {
                break;
            }
            assert!(matches!(
                message,
                ServerMessage::Error {
                    request_id: None,
                    ..
                }
            ));
        }
    })
    .await;
    // Always release storage before asking the real server to flush and stop.
    lock.execute_batch("ROLLBACK").unwrap();
    stop.send(()).unwrap();
    server.await.unwrap().unwrap();
    outcome.expect("bounded ingress must close overload and answer the healthy peer");
}

fn attach(service: &mut Service, client: &mut Connection) -> tor_protocol::Snapshot {
    service.handle(
        client.id,
        "attach".into(),
        Request::Attach { actor: ActorId(1) },
    );
    let mut snapshot = None;
    while let Ok(message) = client.messages.try_recv() {
        if let ServerMessage::Snapshot {
            snapshot: value, ..
        } = message
        {
            snapshot = Some(*value);
        }
    }
    snapshot.expect("attached snapshot")
}

fn annotation(snapshot: &tor_protocol::Snapshot, text: &str) -> Request {
    Request::Command {
        context: snapshot.reply_context().input,
        branch: snapshot.branch.clone(),
        command: tor_protocol::Command::Annotate {
            anchor: tor_protocol::Anchor::State {
                snapshot: snapshot.state.snapshot.clone(),
            },
            text: text.into(),
            source: tor_protocol::ClientSource::User,
            audience: tor_protocol::Audience::Private,
            category: tor_protocol::AnnotationCategory::Note,
        },
    }
}

#[tokio::test]
async fn recovery_preserves_surviving_fifo_and_receipts_after_overload_reconnect() {
    let (_dir, lock, mut service) = stalled_service();
    let mut original = connect(&mut service, "original");
    let snapshot = attach(&mut service, &mut original);
    let accepted = annotation(&snapshot, "accepted before overload");
    service.handle(original.id, "accepted".into(), accepted.clone());
    let receipt = std::iter::from_fn(|| original.messages.try_recv().ok())
        .find_map(|message| match message {
            ServerMessage::Ack {
                receipt,
                request_id,
                ..
            } if request_id == "accepted" => Some(receipt),
            _ => None,
        })
        .expect("positive accepted receipt before overload");
    let mut peer = connect(&mut service, "peer");
    let peer_snapshot = attach(&mut service, &mut peer);
    let store = service.flush_handle().unwrap();
    let (sender, mailbox) = mpsc::channel(16);
    let driver = async move {
        for request_id in ["first", "second"] {
            sender
                .send(Mail::Request {
                    client: peer.id,
                    request_id: request_id.into(),
                    request: annotation(&peer_snapshot, request_id),
                    timing: None,
                })
                .await
                .unwrap();
        }
        for index in 0..300 {
            sender
                .send(commit(original.id, format!("flood-{index}")))
                .await
                .unwrap();
        }
        let (reply, handled) = oneshot::channel();
        sender
            .send(Mail::Call(Box::new(move |_| {
                let _ = reply.send(());
            })))
            .await
            .unwrap();
        handled.await.unwrap();
        assert_eq!(
            *original.close.borrow(),
            Some(crate::session::DisconnectMode::Abort)
        );
        lock.execute_batch("ROLLBACK").unwrap();
        store.flush().unwrap();
        let mut order = Vec::new();
        while order.len() < 2 {
            match tokio::time::timeout(Duration::from_secs(5), peer.messages.recv())
                .await
                .unwrap()
                .unwrap()
            {
                ServerMessage::Ack { request_id, .. } => order.push(request_id),
                ServerMessage::Error {
                    request_id: Some(id),
                    code,
                    ..
                } => panic!("{id}: {code:?}"),
                _ => {}
            }
        }
        assert_eq!(order, ["first", "second"]);
        let (reply, connected) = oneshot::channel();
        sender
            .send(Mail::Connect {
                account: Account {
                    user: "original".into(),
                    token: "test".into(),
                    role: AccessRole::Player,
                    actors: [ActorId(1)].into(),
                },
                frontend: "reconnected".into(),
                reply,
            })
            .await
            .unwrap();
        let mut reconnect = connected.await.unwrap().unwrap();
        sender
            .send(Mail::Request {
                client: reconnect.id,
                request_id: "attach".into(),
                request: Request::Attach { actor: ActorId(1) },
                timing: None,
            })
            .await
            .unwrap();
        loop {
            if matches!(
                reconnect.messages.recv().await.unwrap(),
                ServerMessage::Snapshot { .. }
            ) {
                break;
            }
        }
        sender
            .send(Mail::Request {
                client: reconnect.id,
                request_id: "accepted".into(),
                request: accepted,
                timing: None,
            })
            .await
            .unwrap();
        loop {
            match tokio::time::timeout(Duration::from_secs(5), reconnect.messages.recv())
                .await
                .unwrap()
                .unwrap()
            {
                ServerMessage::Ack {
                    request_id,
                    receipt: replayed,
                    ..
                } => {
                    assert_eq!(request_id, "accepted");
                    assert_eq!(replayed, receipt);
                    break;
                }
                ServerMessage::Error {
                    request_id: Some(id),
                    code,
                    ..
                } => panic!("{id}: {code:?}"),
                _ => {}
            }
        }
        let (reply, stopped) = oneshot::channel();
        sender.send(Mail::Shutdown(reply)).await.unwrap();
        stopped.await.unwrap();
    };
    let (_, service) = tokio::join!(driver, run(service, mailbox, STALL));
    // The seed, original annotation and two surviving annotations each occur once.
    assert_eq!(
        service.flush_handle().unwrap().status().accepted_sequence,
        4
    );
}
