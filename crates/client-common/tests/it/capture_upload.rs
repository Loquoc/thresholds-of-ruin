//! Scripted wire witnesses for the real shared upload driver.
use super::*;
use tor_client_common::captures::{self, CapturedAttachment};

async fn request(server: &mut Server) -> (String, Request) {
    match receive(server).await {
        ClientMessage::Request {
            request_id,
            request,
        } => (request_id, *request),
        other => panic!("Expected upload request, got {other:?}"),
    }
}

#[tokio::test]
async fn capture_upload_keeps_exact_bytes_anchor_and_live_messages() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut server = accept_async(stream).await.unwrap();
        let initial = super::super::validation::snapshot(0);
        attach_scripted(&mut server, &initial).await;
        let (id, begin) = request(&mut server).await;
        let Request::BeginAttachment {
            context,
            branch,
            anchor,
            metadata,
        } = begin
        else {
            panic!("not begin");
        };
        assert_eq!(context, initial.reply_context().input);
        assert_eq!(branch, initial.branch);
        assert_eq!(
            anchor,
            Anchor::State {
                snapshot: initial.state.snapshot.clone()
            }
        );
        assert_eq!(metadata.media_type, AttachmentMediaType::Text);
        assert_eq!(metadata.byte_len, 8197);
        let note = HistoryEntry {
            id: EntryId("live-note".into()),
            branch: initial.branch.clone(),
            actor: initial.actor,
            tick: 0,
            author: Author::User {
                user: "test".into(),
            },
            audience: Audience::Private,
            content: HistoryContent::Annotation {
                anchor,
                category: AnnotationCategory::Note,
                text: "live during upload".into(),
                attachments: vec![],
            },
        };
        send(
            &mut server,
            ServerMessage::Update {
                update: Box::new(StreamUpdate {
                    context: initial.context.clone(),
                    actor: initial.actor,
                    branch: initial.branch.clone(),
                    cursor: StreamCursor {
                        sequence: 1,
                        tick: 0,
                    },
                    body: UpdateBody::Annotation {
                        entry: Box::new(note),
                    },
                }),
            },
        )
        .await;
        let mut reply = initial.reply_context();
        reply.cursor.sequence = 1;
        send(
            &mut server,
            ServerMessage::AttachmentUpload {
                context: reply.clone(),
                request_id: id,
                upload_id: "upload-one".into(),
                next_offset: 0,
            },
        )
        .await;
        let mut bytes = Vec::new();
        for expected in [8192, 8197] {
            let (id, next) = request(&mut server).await;
            let Request::AttachmentChunk {
                upload_id,
                offset,
                data,
            } = next
            else {
                panic!("not chunk");
            };
            assert_eq!(upload_id, "upload-one");
            assert_eq!(offset as usize, bytes.len());
            bytes.extend(attachment_base64_decode(&data).unwrap());
            assert_eq!(bytes.len(), expected);
            send(
                &mut server,
                ServerMessage::AttachmentUpload {
                    context: reply.clone(),
                    request_id: id,
                    upload_id,
                    next_offset: expected as u32,
                },
            )
            .await;
        }
        assert_eq!(bytes, vec![b'x'; 8197]);
        let (id, next) = request(&mut server).await;
        assert!(
            matches!(next, Request::CommitAttachment { upload_id } if upload_id == "upload-one")
        );
        send(
            &mut server,
            ServerMessage::Ack {
                context: reply,
                request_id: id,
                receipt: RequestReceipt::Immediate {
                    actor: initial.actor,
                    branch: initial.branch,
                    entry_id: Some(EntryId("capture-entry".into())),
                },
            },
        )
        .await;
    });
    let mut client = Connection::connect(address, "token".into(), ActorId(1), "capture-test")
        .await
        .unwrap();
    let capture = CapturedAttachment::new(
        client.state.input_context(),
        client.state.branch().clone(),
        client.state.state().snapshot.clone(),
        AttachmentMediaType::Text,
        vec![b'x'; 8197],
    )
    .unwrap();
    let mut live = 0;
    captures::upload(&mut client, capture, |connection, message| {
        if matches!(message, ServerMessage::Update { .. }) {
            live += 1;
            assert_eq!(connection.state.history().last().unwrap().id.0, "live-note");
        }
        Ok(())
    })
    .await
    .unwrap();
    assert_eq!(live, 1);
    server.await.unwrap();
}

#[tokio::test]
async fn capture_rejection_is_not_a_success_or_an_automatic_retry() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut server = accept_async(stream).await.unwrap();
        let initial = super::super::validation::snapshot(0);
        attach_scripted(&mut server, &initial).await;
        let (id, begin) = request(&mut server).await;
        assert!(matches!(begin, Request::BeginAttachment { .. }));
        send(
            &mut server,
            ServerMessage::Error {
                scope: ErrorScope::Attached {
                    context: initial.reply_context(),
                },
                request_id: Some(id),
                code: ErrorCode::InvalidRequest,
                message: "Capture rejected".into(),
            },
        )
        .await;
        assert!(
            timeout(Duration::from_millis(50), next_non_map_frame(&mut server))
                .await
                .is_err()
        );
    });
    let mut client = Connection::connect(address, "token".into(), ActorId(1), "capture-test")
        .await
        .unwrap();
    let capture = CapturedAttachment::new(
        client.state.input_context(),
        client.state.branch().clone(),
        client.state.state().snapshot.clone(),
        AttachmentMediaType::Text,
        b"abc".to_vec(),
    )
    .unwrap();
    let error = captures::upload(&mut client, capture, |_, _| Ok(()))
        .await
        .unwrap_err();
    assert!(error.to_string().contains("Capture rejected"));
    server.await.unwrap();
}

#[tokio::test]
async fn capture_chunk_rejection_aborts_only_uncommitted_staging() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut server = accept_async(stream).await.unwrap();
        let initial = super::super::validation::snapshot(0);
        attach_scripted(&mut server, &initial).await;
        let (id, begin) = request(&mut server).await;
        assert!(matches!(begin, Request::BeginAttachment { .. }));
        send(
            &mut server,
            ServerMessage::AttachmentUpload {
                context: initial.reply_context(),
                request_id: id,
                upload_id: "reject-upload".into(),
                next_offset: 0,
            },
        )
        .await;
        let (id, chunk) = request(&mut server).await;
        assert!(matches!(chunk, Request::AttachmentChunk { .. }));
        send(
            &mut server,
            ServerMessage::Error {
                scope: ErrorScope::Attached {
                    context: initial.reply_context(),
                },
                request_id: Some(id),
                code: ErrorCode::InvalidRequest,
                message: "Chunk rejected".into(),
            },
        )
        .await;
        let (id, abort) = request(&mut server).await;
        assert!(
            matches!(abort, Request::AbortAttachment { upload_id } if upload_id == "reject-upload")
        );
        send(
            &mut server,
            ServerMessage::Ack {
                context: initial.reply_context(),
                request_id: id,
                receipt: RequestReceipt::Immediate {
                    actor: initial.actor,
                    branch: initial.branch,
                    entry_id: None,
                },
            },
        )
        .await;
        assert!(
            timeout(Duration::from_millis(50), next_non_map_frame(&mut server))
                .await
                .is_err()
        );
    });
    let mut client = Connection::connect(address, "token".into(), ActorId(1), "capture-test")
        .await
        .unwrap();
    let capture = CapturedAttachment::new(
        client.state.input_context(),
        client.state.branch().clone(),
        client.state.state().snapshot.clone(),
        AttachmentMediaType::Text,
        b"abc".to_vec(),
    )
    .unwrap();
    let error = captures::upload(&mut client, capture, |_, _| Ok(()))
        .await
        .unwrap_err();
    assert!(error.to_string().contains("Chunk rejected"));
    server.await.unwrap();
}

#[tokio::test]
async fn capture_uses_current_readiness_but_never_replaces_the_picture_anchor() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut server = accept_async(stream).await.unwrap();
        let initial = super::super::validation::snapshot(0);
        attach_scripted(&mut server, &initial).await;
        send(
            &mut server,
            ServerMessage::Update {
                update: Box::new(StreamUpdate {
                    context: initial.context.clone(),
                    actor: initial.actor,
                    branch: initial.branch.clone(),
                    cursor: StreamCursor {
                        sequence: 1,
                        tick: 0,
                    },
                    body: UpdateBody::Readiness {
                        readiness: Readiness {
                            revision: 1,
                            admission: false,
                            resume: vec![],
                            cancel: vec![],
                        },
                    },
                }),
            },
        )
        .await;
        let (id, begin) = request(&mut server).await;
        assert!(
            matches!(begin, Request::BeginAttachment { context, anchor: Anchor::State { snapshot }, .. }
            if context.readiness_revision == 1 && snapshot == initial.state.snapshot)
        );
        let mut context = initial.reply_context();
        context.cursor.sequence = 1;
        context.input.readiness_revision = 1;
        send(
            &mut server,
            ServerMessage::Error {
                scope: ErrorScope::Attached { context },
                request_id: Some(id),
                code: ErrorCode::ResourceLimit,
                message: "fixture stop after verified begin".into(),
            },
        )
        .await;
    });
    let mut client = Connection::connect(address, "token".into(), ActorId(1), "capture-test")
        .await
        .unwrap();
    let capture = CapturedAttachment::new(
        client.state.input_context(),
        client.state.branch().clone(),
        client.state.state().snapshot.clone(),
        AttachmentMediaType::Text,
        b"abc".to_vec(),
    )
    .unwrap();
    assert!(matches!(
        client.next().await.unwrap(),
        ServerMessage::Update { .. }
    ));
    let error = captures::upload(&mut client, capture, |_, _| Ok(()))
        .await
        .unwrap_err();
    assert!(error.to_string().contains("fixture stop"));
    server.await.unwrap();
}

#[tokio::test]
async fn rejected_commit_releases_staging_and_allows_a_fresh_capture() {
    commit_failure_script(false, 0).await;
}

#[tokio::test]
async fn rejected_commit_preserves_original_error_if_abort_is_rejected() {
    commit_failure_script(true, 0).await;
}

#[tokio::test]
async fn disconnected_commit_is_not_aborted_or_automatically_retried() {
    commit_failure_script(false, 1).await;
}

#[tokio::test]
async fn malformed_commit_reply_is_not_aborted_or_automatically_retried() {
    commit_failure_script(false, 2).await;
}

async fn commit_failure_script(reject_abort: bool, unknown_commit: u8) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut server = accept_async(stream).await.unwrap();
        let initial = super::super::validation::snapshot(0);
        attach_scripted(&mut server, &initial).await;
        // A second capture is submitted explicitly by the caller only after a
        // known rejection has released the first upload, never as an auto-retry.
        let attempts = if reject_abort || unknown_commit != 0 {
            1
        } else {
            2
        };
        for attempt in 0..attempts {
            let upload_id = format!("commit-upload-{attempt}");
            let (id, begin) = request(&mut server).await;
            assert!(matches!(begin, Request::BeginAttachment { .. }));
            send(
                &mut server,
                ServerMessage::AttachmentUpload {
                    context: initial.reply_context(),
                    request_id: id,
                    upload_id: upload_id.clone(),
                    next_offset: 0,
                },
            )
            .await;
            let (id, chunk) = request(&mut server).await;
            assert!(
                matches!(chunk, Request::AttachmentChunk { upload_id: ref received, offset: 0, ref data }
                if received == &upload_id && attachment_base64_decode(data).unwrap() == b"abc")
            );
            send(
                &mut server,
                ServerMessage::AttachmentUpload {
                    context: initial.reply_context(),
                    request_id: id,
                    upload_id: upload_id.clone(),
                    next_offset: 3,
                },
            )
            .await;
            let (id, commit) = request(&mut server).await;
            assert!(
                matches!(commit, Request::CommitAttachment { upload_id: ref received } if received == &upload_id)
            );
            if unknown_commit == 1 {
                server.close(None).await.unwrap();
                // The client may observe this only as an unknown outcome. No
                // second connection or retry is allowed by the upload driver.
                assert!(timeout(Duration::from_millis(50), listener.accept())
                    .await
                    .is_err());
                return;
            }
            if unknown_commit == 2 {
                // Even a correlated message is not a confirmed rejection when
                // its type is invalid for a commit. Leave its outcome unknown.
                send(
                    &mut server,
                    ServerMessage::AttachmentUpload {
                        context: initial.reply_context(),
                        request_id: id,
                        upload_id: upload_id.clone(),
                        next_offset: 3,
                    },
                )
                .await;
                assert!(
                    timeout(Duration::from_millis(50), next_non_map_frame(&mut server))
                        .await
                        .is_err()
                );
                return;
            }
            if attempt == 1 {
                send(
                    &mut server,
                    ServerMessage::Ack {
                        context: initial.reply_context(),
                        request_id: id,
                        receipt: RequestReceipt::Immediate {
                            actor: initial.actor,
                            branch: initial.branch.clone(),
                            entry_id: Some(EntryId("second-capture".into())),
                        },
                    },
                )
                .await;
                continue;
            }
            send(
                &mut server,
                ServerMessage::Error {
                    scope: ErrorScope::Attached {
                        context: initial.reply_context(),
                    },
                    request_id: Some(id),
                    code: ErrorCode::ResourceLimit,
                    message: "Commit rejected: queue full".into(),
                },
            )
            .await;
            let next = timeout(Duration::from_secs(2), request(&mut server)).await;
            assert!(
                next.is_ok(),
                "A known rejected commit must release its staging identity promptly"
            );
            let (id, abort) = next.unwrap();
            assert!(
                matches!(abort, Request::AbortAttachment { upload_id: ref received } if received == &upload_id)
            );
            let reply = if reject_abort {
                ServerMessage::Error {
                    scope: ErrorScope::Attached {
                        context: initial.reply_context(),
                    },
                    request_id: Some(id),
                    code: ErrorCode::InvalidRequest,
                    message: "Abort rejected".into(),
                }
            } else {
                ServerMessage::Ack {
                    context: initial.reply_context(),
                    request_id: id,
                    receipt: RequestReceipt::Immediate {
                        actor: initial.actor,
                        branch: initial.branch.clone(),
                        entry_id: None,
                    },
                }
            };
            send(&mut server, reply).await;
        }
        assert!(
            timeout(Duration::from_millis(50), next_non_map_frame(&mut server))
                .await
                .is_err()
        );
    });
    let mut client = Connection::connect(address, "token".into(), ActorId(1), "capture-test")
        .await
        .unwrap();
    let capture = CapturedAttachment::new(
        client.state.input_context(),
        client.state.branch().clone(),
        client.state.state().snapshot.clone(),
        AttachmentMediaType::Text,
        b"abc".to_vec(),
    )
    .unwrap();
    let error = captures::upload(&mut client, capture.clone(), |_, _| Ok(()))
        .await
        .unwrap_err();
    if unknown_commit == 0 {
        assert_eq!(error.to_string(), "Commit rejected: queue full");
        if !reject_abort {
            captures::upload(&mut client, capture, |_, _| Ok(()))
                .await
                .unwrap();
        }
    }
    server.await.unwrap();
}
