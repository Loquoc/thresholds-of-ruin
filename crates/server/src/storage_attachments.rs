//! Save-owned payloads. Journal descriptors and bytes commit as one transaction.
use super::*;
#[cfg(test)]
use sha2::{Digest, Sha256};
#[cfg(test)]
use tor_protocol::AttachmentMediaType;
use tor_protocol::AttachmentMetadata;

const MAX_BYTES: usize = 8 * 1024 * 1024;

pub(super) type AttachmentWrite = (String, Arc<[u8]>);

fn metadata(record: &Record) -> Result<Option<&AttachmentMetadata>, Failure> {
    match &record.entry.content {
        crate::journal::JournalContent::Annotation { attachments, .. } => {
            match attachments.as_slice() {
                [] => Ok(None),
                [metadata] => Ok(Some(metadata)),
                _ => Err(invalid_archive()),
            }
        }
        _ => Ok(None),
    }
}

/// Admit the journal-declared total before allocating the next payload.
pub(super) fn account_declared(record: &Record, total: &mut usize) -> Result<(), Failure> {
    if let Some(metadata) = metadata(record)? {
        if !tor_protocol::valid_attachment_metadata(metadata) {
            return Err(invalid_archive());
        }
        *total = total
            .checked_add(metadata.byte_len as usize)
            .filter(|sum| *sum <= crate::attachments::save_attachment_limit())
            .ok_or_else(invalid_archive)?;
    }
    Ok(())
}

/// SQLite calculates lengths without handing blob contents to the process.
/// Reject an oversized save before any attachment or checkpoint is decoded.
pub(super) fn preflight_saved_size(conn: &Connection) -> Result<(), Failure> {
    let total: i64 = conn
        .query_row(
            "SELECT coalesce(sum(octet_length(bytes)),0) FROM attachments",
            [],
            |row| row.get(0),
        )
        .map_err(|_| invalid_archive())?;
    if usize::try_from(total)
        .ok()
        .is_none_or(|total| total > crate::attachments::save_attachment_limit())
    {
        return Err(invalid_archive());
    }
    Ok(())
}

pub(super) fn preflight_initial(records: &[Record]) -> Result<(), Failure> {
    let mut total = 0;
    for record in records {
        account_declared(record, &mut total)?;
        // Also refuse descriptor/payload mismatches before creating the file.
        from_record(record)?;
    }
    Ok(())
}

fn validate(metadata: &AttachmentMetadata, bytes: &[u8]) -> Result<(), Failure> {
    crate::attachments::validate_payload(metadata, bytes).map_err(|_| invalid_archive())
}

pub(super) fn from_record(record: &Record) -> Result<Option<AttachmentWrite>, Failure> {
    match (metadata(record)?, &record.attachment) {
        (None, None) => Ok(None),
        (Some(metadata), Some(bytes)) => {
            validate(metadata, bytes)?;
            Ok(Some((record.entry.id.0.clone(), bytes.clone())))
        }
        _ => Err(invalid_archive()),
    }
}

pub(super) fn write(
    conn: &Connection,
    attachment: Option<&AttachmentWrite>,
) -> Result<(), Failure> {
    let Some((entry, bytes)) = attachment else {
        return Ok(());
    };
    if bytes.is_empty() || bytes.len() > MAX_BYTES {
        return Err(invalid_archive());
    }
    let existing = conn.query_row(
        "SELECT CASE WHEN typeof(bytes)='blob' AND octet_length(bytes)<=?2 THEN bytes END FROM attachments WHERE entry=?1",
        params![entry, MAX_BYTES as i64],
        |row| Ok(saved_blob(row, 0, MAX_BYTES)? == bytes.as_ref()),
    ).optional().map_err(saved_row_failure)?;
    match existing {
        Some(true) => Ok(()),
        Some(false) => Err(invalid_archive()),
        None => conn
            .execute(
                "INSERT INTO attachments(entry,bytes) VALUES (?1,?2)",
                params![entry, bytes.as_ref()],
            )
            .map(|_| ())
            .map_err(|_| storage_failure()),
    }
}

pub(super) fn hydrate(conn: &Connection, record: &mut Record) -> Result<(), Failure> {
    let Some(metadata) = metadata(record)? else {
        return Ok(());
    };
    // Check declared bounds before fetching, and actual row bounds inside SQLite
    // before copying. The trusted journal descriptor then verifies the bytes.
    if metadata.byte_len == 0 || metadata.byte_len as usize > MAX_BYTES {
        return Err(invalid_archive());
    }
    let bytes = conn.query_row(
        "SELECT CASE WHEN typeof(bytes)='blob' AND octet_length(bytes)=?2 AND octet_length(bytes)<=?3 THEN bytes END FROM attachments WHERE entry=?1",
        params![record.entry.id.0, metadata.byte_len, MAX_BYTES as i64],
        |row| copy_saved_blob(row, 0, MAX_BYTES),
    ).map_err(|_| invalid_archive())?;
    validate(metadata, &bytes)?;
    record.attachment = Some(bytes.into());
    Ok(())
}

pub(super) fn validate_inventory(conn: &Connection, records: &[Record]) -> Result<(), Failure> {
    let expected: BTreeSet<_> = records
        .iter()
        .filter(|record| record.attachment.is_some())
        .map(|record| record.entry.id.0.as_str())
        .collect();
    let mut statement = conn
        .prepare("SELECT entry FROM attachments")
        .map_err(|_| invalid_archive())?;
    let mut rows = statement.query([]).map_err(|_| invalid_archive())?;
    let mut count = 0;
    while let Some(row) = rows.next().map_err(|_| invalid_archive())? {
        let entry = saved_text(row, 0).map_err(|_| invalid_archive())?;
        if !expected.contains(entry) {
            return Err(invalid_archive());
        }
        count += 1;
    }
    if count != expected.len() {
        return Err(invalid_archive());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn database() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE journal(sequence INTEGER PRIMARY KEY,frame BLOB NOT NULL); CREATE TABLE history(sequence INTEGER PRIMARY KEY,frame BLOB NOT NULL); CREATE TABLE attachments(entry TEXT PRIMARY KEY,bytes BLOB NOT NULL) STRICT;").unwrap();
        conn
    }

    #[test]
    fn payload_and_receipt_rollback_and_retry_together() {
        let mut conn = database();
        let mut batch = vec![Pending {
            attachment: Some(("entry-one".into(), Arc::from(&b"  exact\n\x1b[31mtext"[..]))),
            regions: vec![],
            sequence: 1,
            bytes: vec![7],
            sources: vec![],
            queued: Instant::now(),
        }];
        assert!(commit_batch(&mut conn, &batch, None, None, |_, stage| {
            if stage == "after_append" {
                Err(storage_failure())
            } else {
                Ok(())
            }
        })
        .is_err());
        for table in ["attachments", "journal"] {
            assert_eq!(
                conn.query_row(&format!("SELECT count(*) FROM {table}"), [], |row| row
                    .get::<_, i64>(0))
                    .unwrap(),
                0
            );
        }
        commit_batch(&mut conn, &batch, None, None, |_, _| Ok(())).unwrap();
        commit_batch(&mut conn, &batch, None, None, |_, _| Ok(())).unwrap();
        let bytes: Vec<u8> = conn
            .query_row("SELECT bytes FROM attachments", [], |row| row.get(0))
            .unwrap();
        assert_eq!(bytes, b"  exact\n\x1b[31mtext");
        batch[0].attachment.as_mut().unwrap().1 = Arc::from(&b"changed"[..]);
        assert!(commit_batch(&mut conn, &batch, None, None, |_, _| Ok(())).is_err());
        assert_eq!(
            conn.query_row("SELECT bytes FROM attachments", [], |row| row
                .get::<_, Vec<u8>>(0))
                .unwrap(),
            bytes
        );
    }

    #[test]
    fn invalid_payload_length_checksum_and_text_are_rejected() {
        let text = b" exact\n";
        let metadata = AttachmentMetadata {
            media_type: AttachmentMediaType::Text,
            byte_len: text.len() as u32,
            sha256: Sha256::digest(text)
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>(),
        };
        validate(&metadata, text).unwrap();
        assert!(validate(&metadata, b" exact ").is_err());
        assert!(validate(&metadata, b"exact\n").is_err());
        let bytes = [0xff];
        let metadata = AttachmentMetadata {
            byte_len: 1,
            sha256: Sha256::digest(bytes)
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>(),
            ..metadata
        };
        assert!(validate(&metadata, &bytes).is_err());
    }

    #[test]
    fn oversized_existing_blob_is_not_copied_or_overwritten() {
        let conn = database();
        conn.execute(
            "INSERT INTO attachments VALUES ('entry-one',zeroblob(?1))",
            [MAX_BYTES as i64 + 1],
        )
        .unwrap();
        assert!(write(&conn, Some(&("entry-one".into(), Arc::from(&b"small"[..])))).is_err());
        assert_eq!(
            conn.query_row("SELECT length(bytes) FROM attachments", [], |row| row
                .get::<_, i64>(0))
                .unwrap(),
            MAX_BYTES as i64 + 1
        );
    }
    fn receipt(
        engine: &crate::engine::Engine,
        bytes: &[u8],
    ) -> (crate::engine::Receipt, crate::journal::RecordedAnchor) {
        let snapshot = tor_protocol::SnapshotId("capture-fixture-genesis".into());
        let metadata = AttachmentMetadata {
            media_type: AttachmentMediaType::Text,
            byte_len: bytes.len() as u32,
            sha256: Sha256::digest(bytes)
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>(),
        };
        (
            crate::engine::Receipt {
                user: "player".into(),
                frontend: "storage-capture-test".into(),
                request_id: "capture-request".into(),
                actor: tor_protocol::ActorId(1),
                branch: engine.branch().clone(),
                command: crate::journal::Command::Annotate {
                    anchor: tor_protocol::Anchor::State {
                        snapshot: snapshot.clone(),
                    },
                    text: "Text capture".into(),
                    source: tor_protocol::ClientSource::Frontend,
                    audience: tor_protocol::Audience::Private,
                    category: tor_protocol::AnnotationCategory::Note,
                    attachments: vec![metadata],
                },
            },
            crate::journal::RecordedAnchor::State {
                snapshot,
                boundary: None,
            },
        )
    }

    #[test]
    fn captures_flush_checkpoint_restart_and_copy_without_source_files() {
        use crate::engine::{Engine, Scenario};
        for checkpoint_interval in [0, 1] {
            let directory = tempfile::tempdir().unwrap();
            let source = directory.path().join("source.db");
            let copy = directory.path().join("copy.db");
            let mut engine = Engine::open_with_policy(
                &source,
                Scenario::two_room(42),
                SavePolicy {
                    checkpoint_interval,
                    ..SavePolicy::default()
                },
            )
            .unwrap();
            let before = engine.state(tor_protocol::ActorId(1)).unwrap();
            let bytes: Arc<[u8]> = Arc::from(&b"  left\t right\n\x1b[31mred\x1b[0m\n"[..]);
            let (receipt, anchor) = receipt(&engine, &bytes);
            let result = engine
                .command_with_attachment(&receipt, anchor.clone(), bytes.clone(), None)
                .unwrap();
            assert!(!result.duplicate);
            let repeated = engine
                .command_with_attachment(&receipt, anchor, bytes.clone(), None)
                .unwrap();
            assert!(repeated.duplicate);
            assert_eq!(repeated.entry.id, result.entry.id);
            assert_eq!(engine.state(tor_protocol::ActorId(1)).unwrap(), before);
            engine.flush().unwrap();
            assert_eq!(engine.save_status().pending_bytes, 0);
            assert_eq!(
                engine.save_status().accepted_sequence,
                engine.save_status().durable_sequence
            );
            if checkpoint_interval == 1 {
                assert!(engine.save_status().checkpoint_sequence > 0);
            }
            drop(engine);
            std::fs::copy(&source, &copy).unwrap();
            std::fs::remove_file(&source).unwrap();
            let mut reopened = Engine::open(&copy, Scenario::two_room(0)).unwrap();
            let (_, retained) = reopened
                .attachment_bytes(tor_protocol::ActorId(1), "player", &result.entry.id, 0)
                .unwrap();
            assert_eq!(retained.as_ref(), bytes.as_ref());
            assert_eq!(reopened.state(tor_protocol::ActorId(1)).unwrap(), before);
            let duplicate = reopened
                .command_with_attachment(
                    &receipt,
                    crate::journal::RecordedAnchor::State {
                        snapshot: tor_protocol::SnapshotId("capture-fixture-genesis".into()),
                        boundary: None,
                    },
                    bytes.clone(),
                    None,
                )
                .unwrap();
            assert!(duplicate.duplicate);
            assert_eq!(duplicate.entry.id, result.entry.id);
            assert!(reopened
                .attachment_bytes(tor_protocol::ActorId(1), "other", &result.entry.id, 0)
                .is_err());
            drop(reopened);
            let conn = connection(&copy).unwrap();
            conn.execute("UPDATE attachments SET bytes=zeroblob(length(bytes))", [])
                .unwrap();
            drop(conn);
            assert!(Engine::open(&copy, Scenario::two_room(0)).is_err());
        }
    }

    #[test]
    fn capture_queue_capacity_rejects_atomically_including_payload_bytes() {
        use crate::engine::{Engine, Scenario};
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("capacity.db");
        let mut engine = Engine::open_with_policy(
            &path,
            Scenario::two_room(42),
            SavePolicy {
                checkpoint_interval: 0,
                max_pending_bytes: 1024,
                ..SavePolicy::default()
            },
        )
        .unwrap();
        let before = engine.state(tor_protocol::ActorId(1)).unwrap();
        let bytes: Arc<[u8]> = vec![b'x'; 2048].into();
        let (receipt, anchor) = receipt(&engine, &bytes);
        assert!(engine
            .command_with_attachment(&receipt, anchor, bytes, None)
            .is_err());
        assert_eq!(engine.save_status().accepted_sequence, 0);
        assert_eq!(engine.save_status().pending_bytes, 0);
        assert_eq!(engine.state(tor_protocol::ActorId(1)).unwrap(), before);
        engine.flush().unwrap();
        drop(engine);
        let conn = connection(&path).unwrap();
        assert_eq!(
            conn.query_row("SELECT count(*) FROM attachments", [], |row| row
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
        assert_eq!(
            conn.query_row("SELECT count(*) FROM journal WHERE sequence>0", [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap(),
            0
        );
    }

    #[test]
    fn initial_archive_persists_capture_bytes_and_rejects_missing_or_orphan_rows() {
        use crate::engine::{Engine, Scenario};
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("initial.db");
        let mut engine = Engine::memory(Scenario::two_room(42)).unwrap();
        let bytes: Arc<[u8]> = Arc::from(&b"initial capture\n"[..]);
        let (receipt, anchor) = receipt(&engine, &bytes);
        let entry = engine
            .command_with_attachment(&receipt, anchor, bytes.clone(), None)
            .unwrap()
            .entry;
        let engine = engine.attach_profile_save(&path).unwrap();
        engine.flush().unwrap();
        drop(engine);
        let (_, archive, _, _, _, _) = load(&path).unwrap();
        assert_eq!(
            archive.records[0].attachment.as_deref(),
            Some(bytes.as_ref())
        );
        let reopened = Engine::open(&path, Scenario::two_room(0)).unwrap();
        assert_eq!(
            reopened
                .attachment_bytes(tor_protocol::ActorId(1), "player", &entry.id, 0)
                .unwrap()
                .1
                .as_ref(),
            bytes.as_ref()
        );
        drop(reopened);
        let conn = connection(&path).unwrap();
        conn.execute("INSERT INTO attachments VALUES ('orphan', x'01')", [])
            .unwrap();
        drop(conn);
        assert!(load(&path).is_err());
        let conn = connection(&path).unwrap();
        conn.execute("DELETE FROM attachments WHERE entry='orphan'", [])
            .unwrap();
        conn.execute("DELETE FROM attachments WHERE entry=?1", [&entry.id.0])
            .unwrap();
        drop(conn);
        assert!(load(&path).is_err());
    }

    #[test]
    fn saved_total_is_checked_before_loading_any_payload_and_at_initial_creation() {
        use crate::attachments::SaveAttachmentTestLimit;
        use crate::engine::{Engine, Scenario};
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("bounded.db");
        let refused = directory.path().join("refused.db");
        let bytes: Arc<[u8]> = Arc::from(&b"bounded saved capture\n"[..]);
        let mut engine = Engine::memory(Scenario::two_room(42)).unwrap();
        let (input_receipt, anchor) = receipt(&engine, &bytes);
        engine
            .command_with_attachment(&input_receipt, anchor, bytes.clone(), None)
            .unwrap();
        let engine = engine.attach_profile_save(&path).unwrap();
        engine.flush().unwrap();
        drop(engine);
        {
            let _limit = SaveAttachmentTestLimit::new(bytes.len());
            let (_, archive, _, _, _, _) = load(&path).unwrap();
            let record = &archive.records[0];
            let mut total = 0;
            account_declared(record, &mut total).unwrap();
            assert_eq!(total, bytes.len());
            assert!(account_declared(record, &mut total).is_err());
            assert_eq!(
                total,
                bytes.len(),
                "failed accounting must not change the accepted total"
            );
            let mut overflow = usize::MAX;
            assert!(account_declared(record, &mut overflow).is_err());
            assert_eq!(overflow, usize::MAX);
        }
        {
            let _limit = SaveAttachmentTestLimit::new(bytes.len() - 1);
            SAVED_BYTES_COPIED.with(|copied| copied.set(0));
            assert!(load(&path).is_err());
            assert_eq!(
                SAVED_BYTES_COPIED.with(std::cell::Cell::get),
                0,
                "oversized save must fail before any stored blob is copied"
            );
        }
        let mut engine = Engine::memory(Scenario::two_room(42)).unwrap();
        let (input_receipt, anchor) = receipt(&engine, &bytes);
        engine
            .command_with_attachment(&input_receipt, anchor, bytes.clone(), None)
            .unwrap();
        let _limit = SaveAttachmentTestLimit::new(bytes.len() - 1);
        assert!(engine.attach_profile_save(&refused).is_err());
        assert!(
            !refused.exists(),
            "oversized initial archive must not create a partial database"
        );
    }

    #[test]
    fn maximum_png_payload_is_admitted_flushed_and_reopened_with_default_queue() {
        use crate::engine::{Engine, Receipt, Scenario};
        // Independently generated 1x1 red RGBA PNG, padded with one legal caPt
        // ancillary chunk. Fixed CRC/SHA values are independent fixture oracles.
        const BASE: &[u8] = &[
            137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1,
            8, 6, 0, 0, 0, 31, 21, 196, 137, 0, 0, 0, 13, 73, 68, 65, 84, 120, 156, 99, 248, 207,
            192, 240, 31, 0, 5, 0, 1, 255, 137, 153, 61, 29, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66,
            96, 130,
        ];
        const PADDING: usize = 8388526;
        const PADDING_CRC: u32 = 2224211160;
        let mut bytes = Vec::with_capacity(tor_protocol::MAX_ATTACHMENT_BYTES);
        bytes.extend_from_slice(&BASE[..33]);
        bytes.extend_from_slice(&(PADDING as u32).to_be_bytes());
        bytes.extend_from_slice(b"caPt");
        bytes.resize(bytes.len() + PADDING, 0);
        bytes.extend_from_slice(&PADDING_CRC.to_be_bytes());
        bytes.extend_from_slice(&BASE[33..]);
        assert_eq!(bytes.len(), 8 * 1024 * 1024);
        let metadata = AttachmentMetadata {
            media_type: AttachmentMediaType::Png,
            byte_len: 8 * 1024 * 1024,
            sha256: "a3ec14c1ac26e0cf5bfcfd11bc9b770460f685f96b2956844412ff3dee691ca4".into(),
        };
        crate::attachments::validate_payload(&metadata, &bytes).unwrap();
        let bytes: Arc<[u8]> = bytes.into();
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("maximum-png.db");
        let mut engine = Engine::open(&path, Scenario::two_room(42)).unwrap();
        let before = engine.state(tor_protocol::ActorId(1)).unwrap();
        let snapshot = tor_protocol::SnapshotId("maximum-png-fixture".into());
        let receipt = Receipt {
            user: "player".into(),
            frontend: "test".into(),
            request_id: "maximum-png".into(),
            actor: tor_protocol::ActorId(1),
            branch: engine.branch().clone(),
            command: crate::journal::Command::Annotate {
                anchor: tor_protocol::Anchor::State {
                    snapshot: snapshot.clone(),
                },
                text: "Screenshot".into(),
                source: tor_protocol::ClientSource::Frontend,
                audience: tor_protocol::Audience::Private,
                category: tor_protocol::AnnotationCategory::Note,
                attachments: vec![metadata.clone()],
            },
        };
        let result = engine
            .command_with_attachment(
                &receipt,
                crate::journal::RecordedAnchor::State {
                    snapshot,
                    boundary: None,
                },
                bytes.clone(),
                None,
            )
            .expect("the full advertised PNG limit must fit a default empty save queue");
        assert_eq!(engine.state(tor_protocol::ActorId(1)).unwrap(), before);
        engine.flush().unwrap();
        assert_eq!(
            engine.save_status().accepted_sequence,
            engine.save_status().durable_sequence
        );
        drop(engine);
        let reopened = Engine::open(&path, Scenario::two_room(0)).unwrap();
        let (actual, retained) = reopened
            .attachment_bytes(tor_protocol::ActorId(1), "player", &result.entry.id, 0)
            .unwrap();
        assert_eq!(actual, metadata);
        assert_eq!(retained, bytes);
    }
}
