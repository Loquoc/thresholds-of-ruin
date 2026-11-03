//! Connection-owned staging is bounded and never grants simulation authority.
use super::*;
use std::time::{Duration, Instant};
pub(super) struct Upload {
    begin_request: String,
    context: InputContext,
    branch: BranchId,
    anchor: crate::journal::RecordedAnchor,
    metadata: tor_protocol::AttachmentMetadata,
    upload_id: String,
    bytes: Vec<u8>,
    touched: Instant,
}
impl Service {
    pub(super) fn expire_attachments(&mut self) {
        self.attachments
            .retain(|_, u| u.touched.elapsed() < Duration::from_secs(60));
    }
    fn upload_reply(
        &mut self,
        id: u64,
        request_id: &str,
        upload_id: String,
        next_offset: u32,
    ) -> Result<ProcessedRequest, Failure> {
        let context = self
            .reply_context(id)
            .ok_or_else(|| Failure::new(ErrorCode::NotAttached, "Attach an actor first"))?;
        self.send(
            id,
            ServerMessage::AttachmentUpload {
                context,
                request_id: request_id.into(),
                upload_id,
                next_offset,
            },
        );
        Ok(ProcessedRequest::Published)
    }
    pub(super) fn process_attachment(
        &mut self,
        id: u64,
        request_id: &str,
        request: Request,
    ) -> Result<ProcessedRequest, Failure> {
        let client = &self.clients[&id];
        let actor = client
            .actor
            .ok_or_else(|| Failure::new(ErrorCode::NotAttached, "Attach an actor first"))?;
        let user = client.user.clone();
        let frontend = client.frontend.clone();
        let invalid = || {
            Failure::new(
                ErrorCode::InvalidRequest,
                "Attachment upload is unavailable",
            )
        };
        match request {
            Request::BeginAttachment {
                context,
                branch,
                anchor,
                metadata,
            } => {
                if let Some(upload) = self.attachments.get(&id) {
                    if upload.begin_request == request_id
                        && upload.context == context
                        && upload.branch == branch
                        && upload.anchor.to_public() == anchor
                        && upload.metadata == metadata
                    {
                        return self.upload_reply(
                            id,
                            request_id,
                            upload.upload_id.clone(),
                            upload.bytes.len() as u32,
                        );
                    }
                    return Err(Failure::new(
                        ErrorCode::RequestConflict,
                        "An attachment upload is already pending",
                    ));
                }
                if !tor_protocol::valid_attachment_metadata(&metadata) {
                    return Err(invalid());
                }
                let current = self.current_readiness(id).ok_or_else(|| {
                    Failure::new(ErrorCode::StaleContext, "Input context changed")
                })?;
                if self.input_context(id).as_ref() != Some(&context)
                    || current.permissions.revision != context.readiness_revision
                {
                    return Err(Failure::new(
                        ErrorCode::StaleContext,
                        "Input context changed",
                    ));
                }
                if &branch != self.engine.branch() {
                    return Err(Failure::new(
                        ErrorCode::WrongBranch,
                        "The game branch changed",
                    ));
                }
                let anchor = match anchor {
                    Anchor::State { snapshot } => {
                        let issued = self.presentation_by_snapshot(id, &snapshot)?;
                        crate::journal::RecordedAnchor::State {
                            snapshot,
                            boundary: issued.boundary.entry().cloned(),
                        }
                    }
                    Anchor::Entry { id } => crate::journal::RecordedAnchor::Entry { id },
                };
                self.engine
                    .validate_attachment_anchor(actor, &user, &anchor)?;
                let reserved: usize = self
                    .attachments
                    .values()
                    .map(|u| u.metadata.byte_len as usize)
                    .sum();
                if reserved + metadata.byte_len as usize > 32 * 1024 * 1024 {
                    return Err(Failure::new(
                        ErrorCode::ResourceLimit,
                        "Attachment staging is full",
                    ));
                }
                let mut bytes = Vec::new();
                bytes
                    .try_reserve_exact(metadata.byte_len as usize)
                    .map_err(|_| {
                        Failure::new(
                            ErrorCode::ResourceLimit,
                            "Attachment staging allocation failed",
                        )
                    })?;
                let upload_id = uuid::Uuid::new_v4().to_string();
                self.attachments.insert(
                    id,
                    Upload {
                        begin_request: request_id.into(),
                        context,
                        branch,
                        anchor,
                        metadata,
                        upload_id: upload_id.clone(),
                        bytes,
                        touched: Instant::now(),
                    },
                );
                self.upload_reply(id, request_id, upload_id, 0)
            }
            Request::AttachmentChunk {
                upload_id,
                offset,
                data,
            } => {
                let bytes = tor_protocol::attachment_base64_decode(&data).ok_or_else(invalid)?;
                let upload = self
                    .attachments
                    .get_mut(&id)
                    .filter(|u| u.upload_id == upload_id)
                    .ok_or_else(invalid)?;
                let offset = offset as usize;
                let end = offset.checked_add(bytes.len()).ok_or_else(invalid)?;
                if end > upload.metadata.byte_len as usize || offset > upload.bytes.len() {
                    return Err(invalid());
                }
                if offset < upload.bytes.len() {
                    if upload.bytes.get(offset..end) != Some(bytes.as_slice()) {
                        return Err(Failure::new(
                            ErrorCode::RequestConflict,
                            "Attachment chunk differs from accepted bytes",
                        ));
                    }
                } else {
                    upload.bytes.extend_from_slice(&bytes);
                }
                upload.touched = Instant::now();
                let next = upload.bytes.len() as u32;
                self.upload_reply(id, request_id, upload_id, next)
            }
            Request::AbortAttachment { upload_id } => {
                if self
                    .attachments
                    .get(&id)
                    .is_some_and(|u| u.upload_id == upload_id)
                {
                    self.attachments.remove(&id);
                }
                Ok(ProcessedRequest::Reply(RequestReply::Receipt(
                    self.session_receipt(id).ok_or_else(invalid)?,
                )))
            }
            Request::CommitAttachment { upload_id } => {
                if let Some(result) = self.engine.attachment_retry(&user, actor, &upload_id)? {
                    return Ok(ProcessedRequest::Reply(RequestReply::Receipt(
                        self.engine.request_receipt(&result),
                    )));
                }
                let upload = self
                    .attachments
                    .get(&id)
                    .filter(|u| u.upload_id == upload_id)
                    .ok_or_else(invalid)?;
                if &upload.branch != self.engine.branch() {
                    return Err(Failure::new(
                        ErrorCode::WrongBranch,
                        "The game branch changed",
                    ));
                }
                crate::attachments::validate_payload(&upload.metadata, &upload.bytes)?;
                let text = match upload.metadata.media_type {
                    tor_protocol::AttachmentMediaType::Png => "Screenshot",
                    tor_protocol::AttachmentMediaType::Text => "Text capture",
                };
                let mut upload = self.attachments.remove(&id).expect("validated upload");
                let bytes: Arc<[u8]> = Arc::from(std::mem::take(&mut upload.bytes));
                let receipt = crate::engine::Receipt {
                    user,
                    frontend,
                    request_id: upload_id,
                    actor,
                    branch: upload.branch.clone(),
                    command: crate::journal::Command::Annotate {
                        anchor: upload.anchor.to_public(),
                        text: text.into(),
                        source: ClientSource::Frontend,
                        audience: Audience::Private,
                        category: AnnotationCategory::Note,
                        attachments: vec![upload.metadata.clone()],
                    },
                };
                let result = match self.engine.command_with_attachment(
                    &receipt,
                    upload.anchor.clone(),
                    bytes.clone(),
                    None,
                ) {
                    Ok(result) => result,
                    Err(error) => {
                        upload.bytes = bytes.as_ref().to_vec();
                        self.attachments.insert(id, upload);
                        return Err(error);
                    }
                };
                if let Some(entry) = self.engine.disclose_entry(&result.entry) {
                    self.annotation_update(&entry);
                }
                Ok(ProcessedRequest::Reply(RequestReply::Receipt(
                    self.engine.request_receipt(&result),
                )))
            }
            Request::ReadAttachment {
                entry,
                index,
                offset,
            } => {
                let (metadata, bytes) =
                    self.engine.attachment_bytes(actor, &user, &entry, index)?;
                let start = offset as usize;
                if start > bytes.len() {
                    return Err(invalid());
                }
                let context = self.reply_context(id).ok_or_else(invalid)?;
                let limit = self.outbound.capabilities().max_response_bytes as usize;
                let message = fit_data_reply(
                    context,
                    request_id,
                    entry,
                    metadata,
                    offset,
                    &bytes[start..],
                    limit,
                )?;
                self.send(id, message);
                Ok(ProcessedRequest::Published)
            }
            _ => unreachable!("attachment request dispatch"),
        }
    }
}

fn fit_data_reply(
    context: ReplyContext,
    request_id: &str,
    entry: EntryId,
    metadata: AttachmentMetadata,
    offset: u32,
    remaining: &[u8],
    limit: usize,
) -> Result<ServerMessage, Failure> {
    let resource = || {
        Failure::new(
            ErrorCode::ResourceLimit,
            "Attachment reply cannot fit the response budget",
        )
    };
    let make = |length| ServerMessage::AttachmentData {
        context: context.clone(),
        request_id: request_id.into(),
        entry: entry.clone(),
        index: 0,
        metadata: metadata.clone(),
        offset,
        data: attachment_base64_encode(&remaining[..length]),
    };
    let fits = |message: &ServerMessage| {
        tor_protocol::encoded_length(message, limit)
            .map(|length| length.is_some())
            .map_err(|_| resource())
    };
    if !fits(&make(0))? {
        return Err(resource());
    }
    let mut low = 0;
    let mut high = remaining.len().min(ATTACHMENT_CHUNK_BYTES);
    while low < high {
        let middle = low + (high - low).div_ceil(2);
        if fits(&make(middle))? {
            low = middle;
        } else {
            high = middle - 1;
        }
    }
    if low == 0 && !remaining.is_empty() {
        return Err(resource());
    }
    Ok(make(low))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};
    fn setup() -> (Service, Connection) {
        setup_with_frame(MAX_RESPONSE_BYTES)
    }
    fn setup_with_frame(frame_bytes: usize) -> (Service, Connection) {
        let mut s = Service::with_outbound_limits(
            Engine::memory(crate::Scenario::two_room(42)).unwrap(),
            crate::OutboundLimits {
                frame_bytes,
                ..crate::OutboundLimits::default()
            },
        )
        .unwrap();
        let account = Account {
            user: "owner".into(),
            token: "test".into(),
            role: AccessRole::Player,
            actors: BTreeSet::from([ActorId(1)]),
        };
        let client = s.connect(&account, "test".into()).unwrap();
        let id = client.id;
        s.handle(id, "attach".into(), Request::Attach { actor: ActorId(1) });
        (s, client)
    }
    fn begin(s: &mut Service, id: u64, bytes: &[u8]) -> String {
        let context = s.input_context(id).unwrap();
        let anchor = Anchor::State {
            snapshot: s.reply_context(id).unwrap().snapshot,
        };
        let branch = s.engine.branch().clone();
        let metadata = AttachmentMetadata {
            media_type: AttachmentMediaType::Text,
            byte_len: bytes.len() as u32,
            sha256: Sha256::digest(bytes)
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>(),
        };
        s.process(
            id,
            "begin",
            Request::BeginAttachment {
                context,
                branch,
                anchor,
                metadata,
            },
        )
        .unwrap();
        s.attachments[&id].upload_id.clone()
    }
    fn chunk(
        s: &mut Service,
        id: u64,
        upload_id: &str,
        offset: u32,
        bytes: &[u8],
    ) -> Result<ProcessedRequest, Failure> {
        s.process(
            id,
            "chunk",
            Request::AttachmentChunk {
                upload_id: upload_id.into(),
                offset,
                data: attachment_base64_encode(bytes),
            },
        )
    }
    #[test]
    fn exact_capture_retries_preserve_bytes_anchor_and_world_without_control() {
        let (mut s, mut client) = setup();
        let id = client.id;
        while client.messages.try_recv().is_ok() {}
        let before = s.engine.state(ActorId(1)).unwrap();
        let bytes = b"  exact\t\n\x1b[31mred\x1b[0m\n";
        let upload = begin(&mut s, id, bytes);
        let anchor = s.attachments[&id].anchor.clone();
        chunk(&mut s, id, &upload, 0, bytes).unwrap();
        chunk(&mut s, id, &upload, 0, bytes).unwrap();
        assert!(chunk(&mut s, id, &upload, 0, b"different").is_err());
        // Evict issued snapshots after begin; the normalized capture still commits.
        for n in 0..140 {
            s.handle(id, format!("observe-{n}"), Request::Observe);
            while client.messages.try_recv().is_ok() {}
        }
        let result = s
            .process(
                id,
                "commit",
                Request::CommitAttachment {
                    upload_id: upload.clone(),
                },
            )
            .unwrap();
        assert!(matches!(
            result,
            ProcessedRequest::Reply(RequestReply::Receipt(_))
        ));
        assert!(s.attachments.is_empty());
        s.process(id, "retry", Request::CommitAttachment { upload_id: upload })
            .unwrap();
        let history = s.engine.history(ActorId(1), "owner", None, 10).unwrap();
        assert_eq!(history.entries.len(), 1);
        let entry = &history.entries[0];
        assert_eq!(entry.audience, Audience::Private);
        assert!(
            matches!(&entry.content,HistoryContent::Annotation{anchor:actual,attachments,..} if actual==&anchor.to_public() && attachments.len()==1)
        );
        assert_eq!(
            s.engine
                .attachment_bytes(ActorId(1), "owner", &entry.id, 0)
                .unwrap()
                .1
                .as_ref(),
            bytes
        );
        assert!(s
            .engine
            .attachment_bytes(ActorId(1), "other", &entry.id, 0)
            .is_err());
        assert!(s
            .engine
            .attachment_bytes(ActorId(2), "owner", &entry.id, 0)
            .is_err());
        assert_eq!(s.engine.state(ActorId(1)).unwrap(), before);
    }
    #[test]
    fn interrupted_expired_invalid_and_spectator_uploads_never_create_notes() {
        let (mut s, mut client) = setup();
        let id = client.id;
        while client.messages.try_recv().is_ok() {}
        let upload = begin(&mut s, id, b"hello");
        chunk(&mut s, id, &upload, 0, b"hel").unwrap();
        assert!(s
            .process(
                id,
                "commit",
                Request::CommitAttachment {
                    upload_id: upload.clone()
                }
            )
            .is_err());
        s.attachments.get_mut(&id).unwrap().touched = Instant::now() - Duration::from_secs(61);
        assert!(s
            .process(
                id,
                "chunk",
                Request::AttachmentChunk {
                    upload_id: upload,
                    offset: 3,
                    data: attachment_base64_encode(b"lo")
                }
            )
            .is_err());
        assert!(s.attachments.is_empty());
        let upload = begin(&mut s, id, b"hello");
        chunk(&mut s, id, &upload, 0, b"wrong").unwrap();
        assert!(s
            .process(
                id,
                "commit",
                Request::CommitAttachment { upload_id: upload }
            )
            .is_err());
        assert!(s
            .engine
            .history(ActorId(1), "owner", None, 10)
            .unwrap()
            .entries
            .is_empty());
        s.disconnect(id);
        assert!(s.attachments.is_empty());
        let (mut s, mut client) = setup();
        let id = client.id;
        while client.messages.try_recv().is_ok() {}
        s.clients.get_mut(&id).unwrap().role = AccessRole::Spectator;
        let request = Request::BeginAttachment {
            context: s.input_context(id).unwrap(),
            branch: s.engine.branch().clone(),
            anchor: Anchor::State {
                snapshot: s.reply_context(id).unwrap().snapshot,
            },
            metadata: AttachmentMetadata {
                media_type: AttachmentMediaType::Text,
                byte_len: 1,
                sha256: Sha256::digest(b"x")
                    .iter()
                    .map(|byte| format!("{byte:02x}"))
                    .collect::<String>(),
            },
        };
        assert!(matches!(
            s.process(id, "spectator", request),
            Err(Failure {
                code: ErrorCode::Unauthorized,
                ..
            })
        ));
        assert!(s.attachments.is_empty());
    }
    #[test]
    fn staging_limits_and_foreign_snapshot_are_rejected_before_allocation() {
        let (mut s, mut client) = setup();
        let id = client.id;
        while client.messages.try_recv().is_ok() {}
        let metadata = AttachmentMetadata {
            media_type: AttachmentMediaType::Text,
            byte_len: (MAX_TEXT_ATTACHMENT_BYTES + 1) as u32,
            sha256: "0".repeat(64),
        };
        let request = Request::BeginAttachment {
            context: s.input_context(id).unwrap(),
            branch: s.engine.branch().clone(),
            anchor: Anchor::State {
                snapshot: s.reply_context(id).unwrap().snapshot,
            },
            metadata,
        };
        assert!(s.process(id, "large", request).is_err());
        assert!(s.attachments.is_empty());
        let request = Request::BeginAttachment {
            context: s.input_context(id).unwrap(),
            branch: s.engine.branch().clone(),
            anchor: Anchor::State {
                snapshot: SnapshotId("foreign".into()),
            },
            metadata: AttachmentMetadata {
                media_type: AttachmentMediaType::Text,
                byte_len: 1,
                sha256: Sha256::digest(b"x")
                    .iter()
                    .map(|byte| format!("{byte:02x}"))
                    .collect::<String>(),
            },
        };
        assert!(s.process(id, "foreign", request).is_err());
        assert!(s.attachments.is_empty());
    }
    #[test]
    fn global_staging_reserves_declared_bytes_and_disconnect_releases_capacity() {
        let mut s = Service::new(Engine::memory(crate::Scenario::two_room(42)).unwrap());
        let account = Account {
            user: "owner".into(),
            token: "test".into(),
            role: AccessRole::Player,
            actors: BTreeSet::from([ActorId(1)]),
        };
        let mut clients = Vec::new();
        for n in 0..5 {
            let mut client = s.connect(&account, "test".into()).unwrap();
            let id = client.id;
            s.handle(
                id,
                format!("attach-{n}"),
                Request::Attach { actor: ActorId(1) },
            );
            while client.messages.try_recv().is_ok() {}
            let request = Request::BeginAttachment {
                context: s.input_context(id).unwrap(),
                branch: s.engine.branch().clone(),
                anchor: Anchor::State {
                    snapshot: s.reply_context(id).unwrap().snapshot,
                },
                metadata: AttachmentMetadata {
                    media_type: AttachmentMediaType::Png,
                    byte_len: MAX_ATTACHMENT_BYTES as u32,
                    sha256: "0".repeat(64),
                },
            };
            let result = s.process(id, "begin", request.clone());
            if n < 4 {
                assert!(result.is_ok());
            } else {
                assert!(matches!(
                    result,
                    Err(Failure {
                        code: ErrorCode::ResourceLimit,
                        ..
                    })
                ));
                assert_eq!(s.attachments.len(), 4);
                assert!(s.attachments.values().all(|upload| upload.bytes.is_empty()));
                let first = s.attachments.keys().next().copied().unwrap();
                s.disconnect(first);
                assert!(s.process(id, "retry", request).is_ok());
            }
            clients.push(client);
        }
    }
    #[test]
    fn small_frame_retrieval_reconstructs_exact_payload_without_disconnect() {
        let (mut s, mut client) = setup_with_frame(1024);
        let id = client.id;
        while client.messages.try_recv_frame().is_ok() {}
        let bytes: Vec<u8> = (0..4000).map(|n| b'a' + (n % 26) as u8).collect();
        let upload = begin(&mut s, id, &bytes);
        chunk(&mut s, id, &upload, 0, &bytes).unwrap();
        s.process(
            id,
            "commit",
            Request::CommitAttachment { upload_id: upload },
        )
        .unwrap();
        while client.messages.try_recv_frame().is_ok() {}
        let entry = s
            .engine
            .history(ActorId(1), "owner", None, 10)
            .unwrap()
            .entries[0]
            .id
            .clone();
        let mut received = Vec::new();
        let mut chunks = 0;
        while received.len() < bytes.len() {
            s.process(
                id,
                "read",
                Request::ReadAttachment {
                    entry: entry.clone(),
                    index: 0,
                    offset: received.len() as u32,
                },
            )
            .unwrap();
            let frame = client.messages.try_recv_frame().unwrap();
            assert!(frame.text.len() <= 1024);
            let message: ServerMessage = serde_json::from_str(&frame.text).unwrap();
            let ServerMessage::AttachmentData { data, offset, .. } = message else {
                panic!("expected attachment data");
            };
            assert_eq!(offset as usize, received.len());
            received.extend(attachment_base64_decode(&data).unwrap());
            chunks += 1;
        }
        assert!(chunks > 1);
        assert_eq!(received, bytes);
        assert!(s.clients.contains_key(&id));
        let context = s.reply_context(id).unwrap();
        let (metadata, bytes) = s
            .engine
            .attachment_bytes(ActorId(1), "owner", &entry, 0)
            .unwrap();
        assert!(matches!(
            fit_data_reply(context, "read", entry, metadata, 0, &bytes, 1),
            Err(Failure {
                code: ErrorCode::ResourceLimit,
                ..
            })
        ));
    }
}
