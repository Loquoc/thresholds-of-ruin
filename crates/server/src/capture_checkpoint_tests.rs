//! Checkpoint recovery must preserve the admission rules of capture replay.
use super::*;
use sha2::{Digest, Sha256};

fn note(engine: &mut Engine, user: &str, request: &str) -> EntryId {
    let snapshot = SnapshotId(format!("{request}-snapshot"));
    let receipt = Receipt {
        user: user.into(),
        frontend: "capture-checkpoint-test".into(),
        request_id: request.into(),
        actor: ActorId(1),
        branch: engine.branch().clone(),
        command: Command::Annotate {
            anchor: Anchor::State {
                snapshot: snapshot.clone(),
            },
            text: request.into(),
            source: ClientSource::User,
            audience: Audience::Private,
            category: AnnotationCategory::Note,
            attachments: Vec::new(),
        },
    };
    engine
        .command_with_annotation(
            &receipt,
            crate::journal::RecordedAnchor::State {
                snapshot,
                boundary: None,
            },
        )
        .unwrap()
        .entry
        .id
}

fn fixture() -> Engine {
    let mut engine = Engine::memory(Scenario::two_room(42)).unwrap();
    let visible = note(&mut engine, "owner", "visible");
    note(&mut engine, "other", "foreign-private");
    let bytes: Arc<[u8]> = Arc::from(&b"captured text\n"[..]);
    let metadata = AttachmentMetadata {
        media_type: AttachmentMediaType::Text,
        byte_len: bytes.len() as u32,
        sha256: Sha256::digest(&bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect(),
    };
    let receipt = Receipt {
        user: "owner".into(),
        frontend: "capture-checkpoint-test".into(),
        request_id: "capture".into(),
        actor: ActorId(1),
        branch: engine.branch().clone(),
        command: Command::Annotate {
            anchor: Anchor::Entry {
                id: visible.clone(),
            },
            text: "Text capture".into(),
            source: ClientSource::Frontend,
            audience: Audience::Private,
            category: AnnotationCategory::Note,
            attachments: vec![metadata],
        },
    };
    engine
        .command_with_attachment(
            &receipt,
            crate::journal::RecordedAnchor::Entry { id: visible },
            bytes,
            None,
        )
        .unwrap();
    note(&mut engine, "owner", "future");
    engine
}

fn checkpoint(engine: &Engine) -> DiskCheckpoint {
    Checkpoint::capture(engine).encode(
        "f706bd2e-41a0-400f-971d-7d92bc7aad3f",
        engine.archive.records.len() as u64,
    )
}

fn assert_valid(engine: &Engine) {
    let id = &engine.archive.records[2].entry.id;
    for checkpoint in [None, Some(checkpoint(engine))] {
        let restored = Engine::replay(engine.archive.clone(), checkpoint, None).unwrap();
        assert_eq!(
            restored
                .attachment_bytes(ActorId(1), "owner", id, 0)
                .unwrap()
                .1
                .as_ref(),
            b"captured text\n"
        );
        assert!(restored
            .attachment_bytes(ActorId(1), "other", id, 0)
            .is_err());
    }
}

#[test]
fn captured_entry_anchors_reject_self_future_and_foreign_private_on_both_recovery_paths() {
    let engine = fixture();
    assert_valid(&engine);
    for (name, position) in [("self", 2), ("future", 3), ("foreign-private", 1)] {
        let mut archive = engine.archive.clone();
        let target = archive.records[position].entry.id.clone();
        let record = &mut archive.records[2];
        let JournalContent::Annotation { anchor, .. } = &mut record.entry.content else {
            panic!("capture fixture")
        };
        *anchor = crate::journal::RecordedAnchor::Entry { id: target.clone() };
        let Command::Annotate { anchor, .. } = &mut record.receipt.as_mut().unwrap().command else {
            panic!("capture receipt fixture")
        };
        *anchor = Anchor::Entry { id: target };
        assert!(
            Engine::replay(archive.clone(), None, None).is_err(),
            "{name}: full replay accepted invalid capture anchor"
        );
        assert!(
            Engine::replay(archive, Some(checkpoint(&engine)), None).is_err(),
            "{name}: checkpoint accepted invalid capture anchor"
        );
    }
}

#[test]
fn capture_receipt_branch_must_match_entry_on_both_recovery_paths() {
    let engine = fixture();
    assert_valid(&engine);
    let mut archive = engine.archive.clone();
    archive.records[2].receipt.as_mut().unwrap().branch =
        BranchId("79f24a6f-80fc-4e98-bbc5-d609730b04de".into());
    assert!(Engine::replay(archive.clone(), None, None).is_err());
    assert!(Engine::replay(archive, Some(checkpoint(&engine)), None).is_err());
}
