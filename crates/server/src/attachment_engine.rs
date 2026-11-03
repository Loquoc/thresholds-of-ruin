//! Save-owned attachment annotations share normal author and anchor validation.
use super::*;
impl Engine {
    pub(crate) fn attachment_retry(
        &self,
        user: &str,
        actor: ActorId,
        upload_id: &str,
    ) -> Result<Option<CommandResult>, Failure> {
        let Some(&position) = self.receipts.get(&(user.into(), upload_id.into())) else {
            return Ok(None);
        };
        let record = &self.archive.records[position];
        if record.entry.actor != actor
            || !matches!(&record.entry.content,JournalContent::Annotation{attachments,..} if attachments.len()==1)
        {
            return Err(Failure::new(
                ErrorCode::RequestConflict,
                "Upload identity was used for another command",
            ));
        }
        Ok(Some(CommandResult {
            entry: record.entry.clone(),
            duplicate: true,
        }))
    }

    pub(crate) fn validate_attachment_anchor(
        &self,
        actor: ActorId,
        user: &str,
        anchor: &crate::journal::RecordedAnchor,
    ) -> Result<(), Failure> {
        self.validate_note(actor, user, Audience::Private, anchor, "Capture")
    }
    pub(crate) fn command_with_attachment(
        &mut self,
        receipt: &Receipt,
        anchor: crate::journal::RecordedAnchor,
        bytes: Arc<[u8]>,
        recorded_id: Option<EntryId>,
    ) -> Result<CommandResult, Failure> {
        if let Some(result) = self.resolve_receipt(receipt)? {
            return Ok(result);
        }
        let total = self
            .archive
            .records
            .iter()
            .filter_map(|record| record.attachment.as_ref())
            .try_fold(bytes.len(), |total, payload| {
                total.checked_add(payload.len())
            });
        if total.is_none_or(|total| total > crate::attachments::save_attachment_limit()) {
            return Err(Failure::new(
                ErrorCode::ResourceLimit,
                "Save attachment capacity is exhausted",
            ));
        }
        self.check_request(receipt)?
            .with_annotation_anchor(anchor.clone())?;
        let Command::Annotate {
            text,
            source: ClientSource::Frontend,
            audience: Audience::Private,
            category: AnnotationCategory::Note,
            attachments,
            ..
        } = &receipt.command
        else {
            return Err(invalid_archive());
        };
        if attachments.len() != 1 {
            return Err(invalid_archive());
        }
        crate::attachments::validate_payload(&attachments[0], &bytes)?;
        let title = match attachments[0].media_type {
            tor_protocol::AttachmentMediaType::Png => "Screenshot",
            tor_protocol::AttachmentMediaType::Text => "Text capture",
        };
        if text != title {
            return Err(invalid_archive());
        }
        self.validate_note(
            receipt.actor,
            &receipt.user,
            Audience::Private,
            &anchor,
            text,
        )?;
        let entry = JournalEntry {
            intention_suspensions: Vec::new(),
            intention_ends: Vec::new(),
            id: recorded_id.unwrap_or_else(new_id),
            branch: receipt.branch.clone(),
            actor: receipt.actor,
            tick: self.game.tick(),
            author: Author::Frontend {
                user: receipt.user.clone(),
                component: receipt.frontend.clone(),
            },
            audience: Audience::Private,
            content: JournalContent::Annotation {
                anchor,
                category: AnnotationCategory::Note,
                text: text.clone(),
                attachments: attachments.clone(),
            },
        };
        let record = Record {
            attachment: Some(bytes),
            retrieval_before: RetrievalBoundaryState::capture(&self.game),
            retrieval_after: RetrievalBoundaryState::capture(&self.game),
            before_tick: self.game.tick(),
            after_tick: self.game.tick(),
            entry: entry.clone(),
            receipt: Some(receipt.clone()),
            action_resolution: ActionResolution::Unchanged,
            activations: Vec::new(),
            retrievals: Vec::new(),
        };
        let copied = self.admit(&record, &Candidate::capture(self), None)?;
        self.mark_saved(copied);
        self.append_record(record);
        Ok(CommandResult {
            entry,
            duplicate: false,
        })
    }
    pub(crate) fn attachment_bytes(
        &self,
        actor: ActorId,
        user: &str,
        entry: &EntryId,
        index: u16,
    ) -> Result<(tor_protocol::AttachmentMetadata, Arc<[u8]>), Failure> {
        let position = self.history_index.find(entry).ok_or_else(invalid_anchor)?;
        let record = &self.archive.records[position];
        if !record.entry.visible_to(actor, user) || index != 0 {
            return Err(invalid_anchor());
        }
        let JournalContent::Annotation { attachments, .. } = &record.entry.content else {
            return Err(invalid_anchor());
        };
        let meta = attachments.first().ok_or_else(invalid_anchor)?;
        Ok((
            meta.clone(),
            record.attachment.clone().ok_or_else(invalid_archive)?,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};
    #[test]
    fn total_capture_capacity_rejects_atomically_but_preserves_retry_and_old_bytes() {
        let _limit = crate::attachments::SaveAttachmentTestLimit::new(5);
        let mut engine = Engine::memory(Scenario::two_room(42)).unwrap();
        let before = engine.state(ActorId(1)).unwrap();
        let bytes: Arc<[u8]> = Arc::from(&b"abc"[..]);
        let metadata = AttachmentMetadata {
            media_type: AttachmentMediaType::Text,
            byte_len: 3,
            sha256: Sha256::digest(&bytes)
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect(),
        };
        let snapshot = SnapshotId("capture-test".into());
        let anchor = crate::journal::RecordedAnchor::State {
            snapshot: snapshot.clone(),
            boundary: None,
        };
        let mut receipt = Receipt {
            user: "owner".into(),
            frontend: "test".into(),
            request_id: "first".into(),
            actor: ActorId(1),
            branch: engine.branch().clone(),
            command: Command::Annotate {
                anchor: Anchor::State { snapshot },
                text: "Text capture".into(),
                source: ClientSource::Frontend,
                audience: Audience::Private,
                category: AnnotationCategory::Note,
                attachments: vec![metadata],
            },
        };
        let first = engine
            .command_with_attachment(&receipt, anchor.clone(), bytes.clone(), None)
            .unwrap();
        receipt.request_id = "second".into();
        assert_eq!(
            engine
                .command_with_attachment(&receipt, anchor.clone(), bytes.clone(), None)
                .unwrap_err()
                .code,
            ErrorCode::ResourceLimit
        );
        assert_eq!(
            engine
                .history(ActorId(1), "owner", None, 10)
                .unwrap()
                .entries
                .len(),
            1
        );
        assert_eq!(
            engine
                .attachment_bytes(ActorId(1), "owner", &first.entry.id, 0)
                .unwrap()
                .1,
            bytes
        );
        assert_eq!(engine.state(ActorId(1)).unwrap(), before);
        receipt.request_id = "first".into();
        assert!(
            engine
                .command_with_attachment(&receipt, anchor, bytes, None)
                .unwrap()
                .duplicate
        );
    }
}
