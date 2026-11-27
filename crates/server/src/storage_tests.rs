use super::*;
use crate::journal::Action;
use crate::{Engine, Scenario};
use tor_protocol::ActorId;

#[test]
fn navigation_heavy_checkpoints_have_compact_storage_payloads() {
    let engine = Engine::memory(Scenario::performance(42, 8, 8).unwrap()).unwrap();
    let checkpoint = Checkpoint::capture(&engine).encode("compression-test", 1);
    let json = serde_json::to_vec(&checkpoint).unwrap();
    assert!(json.len() > 1024);
    let stored = encode_checkpoint(&checkpoint).unwrap();
    assert!(
        stored.len() * 2 < json.len(),
        "stored {} bytes for {} bytes of checkpoint JSON",
        stored.len(),
        json.len()
    );
    assert_eq!(checkpoint_codec::decode(&stored).unwrap(), json);
}

#[test]
fn generated_records_commit_atomically_without_a_checkpoint() {
    let mut conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("CREATE TABLE journal(sequence INTEGER PRIMARY KEY,frame BLOB); CREATE TABLE history(sequence INTEGER PRIMARY KEY,frame BLOB); CREATE TABLE regions(record INTEGER PRIMARY KEY,frame BLOB);").unwrap();
    let entry = Pending {
        attachment: None,
        sequence: 1,
        bytes: vec![1],
        sources: vec![],
        queued: Instant::now(),
        regions: (1..=9).map(|id| (id, vec![id as u8])).collect(),
    };
    let batch = vec![entry];
    assert!(commit_batch(&mut conn, &batch, None, None, |_, stage| {
        if stage == "after_generated_regions" {
            Err(storage_failure())
        } else {
            Ok(())
        }
    })
    .is_err());
    assert_eq!(
        conn.query_row("SELECT count(*) FROM regions", [], |row| row
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        conn.query_row("SELECT count(*) FROM journal", [], |row| row
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    commit_batch(&mut conn, &batch, None, None, |_, _| Ok(())).unwrap();
    commit_batch(&mut conn, &batch, None, None, |_, _| Ok(())).unwrap();
    assert_eq!(
        conn.query_row("SELECT count(*) FROM regions", [], |row| row
            .get::<_, i64>(0))
            .unwrap(),
        9
    );
    let mut conflict = batch;
    conflict[0].regions[0].1 = vec![99];
    assert!(commit_batch(&mut conn, &conflict, None, None, |_, _| Ok(())).is_err());
}

#[test]
fn acquisition_package_chunks_preserve_bounds_order_and_types() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("CREATE TABLE package(chunk INTEGER PRIMARY KEY, bytes BLOB NOT NULL)")
        .unwrap();
    conn.execute(
        "INSERT INTO package VALUES (0, zeroblob(?1))",
        [PACKAGE_CHUNK as i64],
    )
    .unwrap();
    let (count, bytes) = read_package_chunks(&conn).unwrap();
    assert_eq!(count, 1);
    assert_eq!(bytes.len(), PACKAGE_CHUNK);
    for statement in [
        "UPDATE package SET bytes='wrong type'",
        "UPDATE package SET bytes=zeroblob(0)",
        "UPDATE package SET bytes=zeroblob(1),chunk=1",
    ] {
        conn.execute(statement, []).unwrap();
        let before = SAVED_BYTES_COPIED.with(std::cell::Cell::get);
        assert!(read_package_chunks(&conn).is_err(), "{statement}");
        assert_eq!(SAVED_BYTES_COPIED.with(std::cell::Cell::get), before);
    }
    conn.execute("DELETE FROM package", []).unwrap();
    let maximum = (crate::scenario_package::MAX_INDEX_BYTES as usize).div_ceil(PACKAGE_CHUNK);
    for chunk in 0..maximum {
        conn.execute("INSERT INTO package VALUES (?1, x'20')", [chunk as i64])
            .unwrap();
    }
    assert_eq!(
        read_package_chunks(&conn).unwrap(),
        (maximum, vec![b' '; maximum])
    );
    conn.execute("INSERT INTO package VALUES (?1, x'20')", [maximum as i64])
        .unwrap();
    let before = SAVED_BYTES_COPIED.with(std::cell::Cell::get);
    assert!(read_package_chunks(&conn).is_err());
    assert_eq!(
        SAVED_BYTES_COPIED.with(std::cell::Cell::get) - before,
        maximum
    );
}

#[test]
fn acquisition_unknown_copied_region_rejects_without_changing_save() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("unknown-source.db");
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/two-room");
    let engine = Engine::open(
        &path,
        crate::scenario_package::load(&root, 42, None, false).unwrap(),
    )
    .unwrap();
    engine.flush().unwrap();
    drop(engine);
    let conn = connection(&path).unwrap();
    conn.execute("INSERT INTO region_sources VALUES (999999, 'unused')", [])
        .unwrap();
    drop(conn);
    let before = std::fs::read(&path).unwrap();
    assert!(
        load(&path).is_err(),
        "copied regions must belong to the pinned index"
    );
    assert_eq!(std::fs::read(&path).unwrap(), before);
}

#[test]
fn acquisition_retries_reject_oversized_stored_values_before_copying() {
    let mut copied = Vec::new();
    for kind in ["journal", "region", "checkpoint"] {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
                "CREATE TABLE journal(sequence INTEGER PRIMARY KEY, frame BLOB);
                 CREATE TABLE history(sequence INTEGER PRIMARY KEY, frame BLOB);
                 CREATE TABLE regions(record INTEGER PRIMARY KEY, frame BLOB);
                 CREATE TABLE checkpoint(slot INTEGER PRIMARY KEY, sequence INTEGER, payload BLOB, checksum INTEGER);",
            ).unwrap();
        let pending = Pending {
            attachment: None,
            regions: vec![],
            sequence: 1,
            bytes: vec![0],
            sources: vec![],
            queued: Instant::now(),
        };
        let regions = RegionWrite {
            rows: vec![(1, vec![0])],
            keep: BTreeSet::from([1]),
        };
        let before = SAVED_BYTES_COPIED.with(std::cell::Cell::get);
        let result = match kind {
            "journal" => {
                conn.execute(
                    "INSERT INTO journal VALUES (1, zeroblob(?1))",
                    [(MAX_PAYLOAD + 25) as i64],
                )
                .unwrap();
                commit_batch(&mut conn, &[pending], None, None, |_, _| Ok(()))
            }
            "region" => {
                conn.execute(
                    "INSERT INTO regions VALUES (1, zeroblob(?1))",
                    [(codec::MAX_REGION + 25) as i64],
                )
                .unwrap();
                commit_batch(&mut conn, &[], Some((1, &[0])), Some(&regions), |_, _| {
                    Ok(())
                })
            }
            _ => {
                conn.execute(
                    "INSERT INTO checkpoint VALUES (1, 1, zeroblob(?1), 0)",
                    [(MAX_CHECKPOINT + 1) as i64],
                )
                .unwrap();
                commit_batch(&mut conn, &[], Some((1, &[0])), None, |_, _| Ok(()))
            }
        };
        assert!(result.is_err(), "{kind}");
        copied.push((kind, SAVED_BYTES_COPIED.with(std::cell::Cell::get) - before));
    }
    assert_eq!(
        copied,
        vec![("journal", 0), ("region", 0), ("checkpoint", 0)]
    );
}

#[test]
fn acquisition_sources_use_utf8_byte_limits_before_copying() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE region_sources(region INTEGER PRIMARY KEY, source TEXT NOT NULL)",
    )
    .unwrap();
    let mut text = "é".repeat(crate::scenario_package::MAX_REGION_BYTES as usize / 2);
    conn.execute("INSERT INTO region_sources VALUES (1, ?1)", [&text])
        .unwrap();
    let mut reader = SourceReader {
        path: PathBuf::new(),
        conn: Some(conn),
        inputs: Arc::new(Mutex::new(InputOwnership {
            known: BTreeSet::from([1]),
            pending: BTreeMap::new(),
        })),
    };
    assert!(reader.read(2).unwrap().is_none());
    assert_eq!(reader.read(1).unwrap().as_deref(), Some(text.as_str()));
    text.push('!');
    reader
        .conn
        .as_ref()
        .unwrap()
        .execute("UPDATE region_sources SET source=?1", [&text])
        .unwrap();
    let before = SAVED_BYTES_COPIED.with(std::cell::Cell::get);
    assert!(
        reader.read(1).is_err(),
        "source bytes, rather than codepoints, must be bounded"
    );
    assert_eq!(SAVED_BYTES_COPIED.with(std::cell::Cell::get), before);
}

#[test]
fn acquisition_oversized_package_chunk_rejects_before_copying() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("CREATE TABLE package(chunk INTEGER PRIMARY KEY, bytes BLOB NOT NULL)")
        .unwrap();
    conn.execute(
        "INSERT INTO package VALUES (0, zeroblob(?1))",
        [(PACKAGE_CHUNK + 1) as i64],
    )
    .unwrap();
    let before = SAVED_BYTES_COPIED.with(std::cell::Cell::get);
    assert!(read_package_chunks(&conn).is_err());
    assert_eq!(SAVED_BYTES_COPIED.with(std::cell::Cell::get), before);
}

#[test]
fn oversized_region_rows_reject_before_copying_owned_bytes() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("CREATE TABLE regions(record INTEGER PRIMARY KEY, frame BLOB NOT NULL)")
        .unwrap();
    conn.execute(
        "INSERT INTO regions VALUES (1, zeroblob(?1))",
        [(codec::MAX_REGION + 25) as i64],
    )
    .unwrap();
    let before = SAVED_BYTES_COPIED.with(std::cell::Cell::get);
    assert!(read_region_row(
        &conn,
        tor_simulation::RecordId(1),
        crate::engine::diagnostic_content().unwrap()
    )
    .is_err());
    assert_eq!(
        SAVED_BYTES_COPIED.with(std::cell::Cell::get) - before,
        0,
        "an invalid oversized row must not allocate a Rust-owned frame"
    );
}

fn checkpoint_fixture(path: &Path) -> (Vec<u8>, tor_protocol::StateView) {
    let mut engine = Engine::open_with_policy(
        path,
        Scenario::two_room(42),
        SavePolicy {
            checkpoint_interval: 0,
            ..SavePolicy::default()
        },
    )
    .unwrap();
    for index in 0..4 {
        engine
            .command(
                "player",
                "test",
                ActorId(1),
                &format!("{index}"),
                &engine.branch().clone(),
                crate::journal::Command::Act {
                    expected_boundary: engine.causal_boundary(ActorId(1)).unwrap(),
                    action: Action::Wait,
                },
            )
            .unwrap();
    }
    engine.flush().unwrap();
    let (_, _, save_id, _, _, _) = load(path).unwrap();
    let bytes = encode_checkpoint(&Checkpoint::capture(&engine).encode(&save_id, 4)).unwrap();
    (bytes, engine.state(ActorId(1)).unwrap())
}

#[test]
fn region_rows_commit_with_their_checkpoint_and_are_collected_after() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("regions.db");
    let (bytes, _) = checkpoint_fixture(&path);
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/two-room");
    let package = crate::scenario_package::read_package(&root).unwrap();
    let index = package.index(0).unwrap();
    let (record, _) = package.build_region(0, &index, 1).unwrap();
    let row = region_frame(7, &record).unwrap();
    let write = |keep: &[u64], rows: Vec<(u64, Vec<u8>)>| RegionWrite {
        rows,
        keep: keep.iter().copied().collect(),
    };
    let stored = |conn: &Connection| -> Vec<(i64, Vec<u8>)> {
        conn.prepare("SELECT record,frame FROM regions ORDER BY record")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    };
    let mut conn = connection(&path).unwrap();
    let commit = |conn: &mut Connection, regions: &RegionWrite, stage: &str| {
        commit_batch(conn, &[], Some((4, &bytes)), Some(regions), |_, at| {
            if at == stage {
                Err(storage_failure())
            } else {
                Ok(())
            }
        })
    };
    // A failure anywhere in the transaction leaves no row behind.
    let first = write(&[7], vec![(7, row.clone())]);
    for stage in [
        "after_regions",
        "after_checkpoint",
        "after_gc",
        "before_commit",
    ] {
        assert!(commit(&mut conn, &first, stage).is_err(), "{stage}");
        assert!(stored(&conn).is_empty(), "{stage}");
    }
    commit(&mut conn, &first, "").unwrap();
    assert_eq!(stored(&conn), [(7, row.clone())]);
    // A retry must match the stored row exactly; a row is never rewritten.
    commit(&mut conn, &first, "").unwrap();
    let other = region_frame(7, &package.build_region(0, &index, 2).unwrap().0).unwrap();
    assert!(commit(&mut conn, &write(&[7], vec![(7, other)]), "").is_err());
    assert_eq!(stored(&conn), [(7, row.clone())]);
    // Rows decode to their record, and only under their own identity.
    assert_eq!(
        decode_region(&row, 7, package.content().unwrap()).unwrap(),
        record
    );
    assert!(decode_region(&row, 8, package.content().unwrap()).is_err());
    let mut damaged = row;
    *damaged.last_mut().unwrap() ^= 1;
    assert!(decode_region(&damaged, 7, package.content().unwrap()).is_err());
    // Rows the checkpoint still refers to stay without being rewritten;
    // the others are deleted with the checkpoint that stops referring to
    // them, and a failure keeps them.
    commit(&mut conn, &write(&[7], vec![]), "").unwrap();
    assert_eq!(stored(&conn).len(), 1);
    assert!(commit(&mut conn, &write(&[], vec![]), "after_gc").is_err());
    assert_eq!(stored(&conn).len(), 1);
    commit(&mut conn, &write(&[], vec![]), "").unwrap();
    assert!(stored(&conn).is_empty());
}

#[test]
fn checkpoint_transaction_failures_and_uncertain_commits_are_retryable() {
    for stage in [
        "after_append",
        "after_regions",
        "after_checkpoint",
        "after_history",
        "after_rotation",
        "after_gc",
        "before_commit",
        "after_commit",
    ] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("fault.db");
        let (bytes, expected) = checkpoint_fixture(&path);
        let mut conn = connection(&path).unwrap();
        assert!(
            commit_batch(&mut conn, &[], Some((4, &bytes)), None, |_, at| {
                if at == stage {
                    Err(storage_failure())
                } else {
                    Ok(())
                }
            })
            .is_err()
        );
        drop(conn);
        let recovered = Engine::open(&path, Scenario::two_room(0)).unwrap();
        assert_eq!(recovered.state(ActorId(1)).unwrap(), expected, "{stage}");
        assert_eq!(
            recovered.recovery_profile().records_replayed,
            if stage == "after_commit" { 0 } else { 4 }
        );
        drop(recovered);
        let mut conn = connection(&path).unwrap();
        commit_batch(&mut conn, &[], Some((4, &bytes)), None, |_, _| Ok(())).unwrap();
        assert_eq!(
            conn.query_row("SELECT count(*) FROM checkpoint", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert_eq!(
            conn.query_row("SELECT count(*) FROM history", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            4
        );
    }
}

#[test]
fn checkpoint_crash_child() {
    let Ok(path) = std::env::var("TOR_CHECKPOINT_TEST_PATH") else {
        return;
    };
    let stage = std::env::var("TOR_CHECKPOINT_TEST_STAGE").unwrap();
    let path = Path::new(&path);
    let bytes = std::fs::read(path.with_extension("checkpoint")).unwrap();
    // A streaming fixture also passes its region rows and sequence.
    let (sequence, regions) = match std::fs::read(path.with_extension("rows")) {
        Ok(rows) => {
            type Rows = (u64, Vec<(u64, Vec<u8>)>, BTreeSet<u64>);
            let (sequence, rows, keep): Rows = serde_json::from_slice(&rows).unwrap();
            (sequence, Some(RegionWrite { rows, keep }))
        }
        Err(_) => (4, None),
    };
    let mut conn = connection(path).unwrap();
    conn.execute_batch("PRAGMA cache_size=1").unwrap();
    commit_batch(
        &mut conn,
        &[],
        Some((sequence, &bytes)),
        regions.as_ref(),
        |connection, at| {
            if at == stage {
                connection.cache_flush().unwrap();
                std::process::exit(81);
            }
            Ok(())
        },
    )
    .unwrap();
    panic!("crash stage was not reached");
}

#[test]
fn process_death_at_each_checkpoint_transition_recovers_a_complete_prefix() {
    for stage in [
        "after_append",
        "after_regions",
        "after_checkpoint",
        "after_history",
        "after_rotation",
        "after_gc",
        "before_commit",
        "after_commit",
    ] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("crash.db");
        let (bytes, expected) = checkpoint_fixture(&path);
        // A valid level-zero envelope preserves the deliberate page spill
        // and torn-extension stress even when normal checkpoints compress
        // into the database's existing free pages.
        let decoded = checkpoint_codec::decode(&bytes).unwrap();
        let checkpoint: DiskCheckpoint = strict(&decoded).unwrap();
        let bytes = checkpoint_codec::write_uncompressed_json(&checkpoint).unwrap();
        let original_size = std::fs::metadata(&path).unwrap().len();
        std::fs::write(path.with_extension("checkpoint"), bytes).unwrap();
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "storage::tests::checkpoint_crash_child",
                "--nocapture",
            ])
            .env("TOR_CHECKPOINT_TEST_PATH", &path)
            .env("TOR_CHECKPOINT_TEST_STAGE", stage)
            .stdout(std::process::Stdio::null())
            .status()
            .unwrap();
        assert_eq!(status.code(), Some(81));
        if stage == "after_checkpoint" {
            let length = std::fs::metadata(&path).unwrap().len();
            assert!(
                length > original_size,
                "checkpoint must spill new pages before the crash"
            );
            // Model a torn extension while the hot journal still owns the
            // previous durable prefix. Recovery must precede alignment checks.
            std::fs::OpenOptions::new()
                .write(true)
                .open(&path)
                .unwrap()
                .set_len(length - 1)
                .unwrap();
        }
        let mut recovered = Engine::open(&path, Scenario::two_room(0)).unwrap();
        assert_eq!(recovered.state(ActorId(1)).unwrap(), expected, "{stage}");
        assert_eq!(
            recovered.recovery_profile().records_replayed,
            if stage == "after_commit" { 0 } else { 4 }
        );
        assert!(
            recovered
                .command(
                    "player",
                    "test",
                    ActorId(1),
                    "0",
                    &recovered.branch().clone(),
                    crate::journal::Command::Act {
                        expected_boundary: 0,
                        action: Action::Wait
                    }
                )
                .unwrap()
                .duplicate
        );
    }
}
/// Steps from the corridor's start to the middle of hall 4.
pub(super) const TO_HALL_4: usize = 76;

pub(super) fn corridor() -> Scenario {
    let root =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/tests/streaming-corridor");
    let mut scenario = crate::scenario_package::load(&root, 5, None, false).unwrap();
    scenario.streaming = Some(crate::Streaming {
        active_radius: 0,
        load_radius: 0,
    });
    scenario
}

pub(super) fn walk(engine: &mut Engine, direction: crate::journal::Direction, steps: usize) {
    for _ in 0..steps {
        let revision = engine.causal_boundary(ActorId(1)).unwrap();
        let request = format!("walk-{direction:?}-{revision}");
        engine
            .command(
                "player",
                "test",
                ActorId(1),
                &request,
                &engine.branch().clone(),
                crate::journal::Command::Act {
                    expected_boundary: revision,
                    action: Action::Move { direction },
                },
            )
            .unwrap();
    }
}

#[test]
fn flushed_region_sources_release_the_pending_byte_budget() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("source-budget.db");
    let policy = SavePolicy {
        checkpoint_interval: 4,
        ..SavePolicy::default()
    };
    let mut engine = Engine::open_with_policy(&path, corridor(), policy.clone()).unwrap();
    walk(&mut engine, crate::journal::Direction::East, TO_HALL_4);
    let expected = engine.state(ActorId(1)).unwrap();
    engine.flush().unwrap();
    let status = engine.save_status();
    assert_eq!(status.accepted_sequence, status.durable_sequence);
    assert!(status.last_commit_ms <= status.last_batch_ms);
    let source_count: i64 = connection(&path)
        .unwrap()
        .query_row("SELECT COUNT(*) FROM region_sources", [], |row| row.get(0))
        .unwrap();
    assert!(source_count > 1);
    assert_eq!(status.pending_bytes, 0);
    drop(engine);
    let mut restored = Engine::open_with_policy(&path, corridor(), policy).unwrap();
    assert_eq!(restored.state(ActorId(1)).unwrap(), expected);
    walk(&mut restored, crate::journal::Direction::West, 1);
    restored.flush().unwrap();
    assert_eq!(restored.save_status().pending_bytes, 0);
}

fn capture_engine(path: &Path) -> Engine {
    let mut engine = Engine::open_with_policy(
        path,
        Scenario::two_room(42),
        SavePolicy {
            checkpoint_interval: 1,
            ..SavePolicy::default()
        },
    )
    .unwrap();
    engine.enable_wizard().unwrap();
    engine
        .command(
            "player",
            "test",
            ActorId(1),
            "capture-fixture",
            &engine.branch().clone(),
            crate::journal::Command::Act {
                expected_boundary: engine.causal_boundary(ActorId(1)).unwrap(),
                action: crate::journal::Action::Wait,
            },
        )
        .unwrap();
    engine.flush().unwrap();
    engine
}

#[test]
fn checkpoint_capture_does_not_block_status_reads() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("capture-lock.db");
    let engine = capture_engine(&path);
    let store = engine.flush_handle().unwrap();
    let before = store.status();
    let writer = store.clone();
    let reader = store.clone();
    let (entered_tx, entered_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let producer = std::thread::spawn(move || {
        let result = writer.push(
            2,
            &(),
            Vec::new(),
            Vec::new(),
            Some(|| {
                entered_tx.send(()).unwrap();
                release_rx.recv().unwrap();
                Checkpoint::capture(&engine)
            }),
        );
        (engine, result)
    });
    entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    let (status_tx, status_rx) = std::sync::mpsc::channel();
    let observer = std::thread::spawn(move || {
        status_tx.send(reader.status()).unwrap();
    });
    let observed = status_rx.recv_timeout(Duration::from_secs(5));
    // Release and join before asserting, including on the failing baseline.
    release_tx.send(()).unwrap();
    let (engine, result) = producer.join().unwrap();
    observer.join().unwrap();
    result.unwrap();
    store.flush().unwrap();
    let expected = engine.state(ActorId(1)).unwrap();
    drop(engine);
    drop(store);
    let restored = Engine::open(&path, Scenario::two_room(0)).unwrap();
    assert_eq!(restored.state(ActorId(1)).unwrap(), expected);
    let observed = observed.expect("status waited for checkpoint capture");
    assert_eq!(observed.accepted_sequence, before.accepted_sequence);
    assert_eq!(observed.durable_sequence, before.durable_sequence);
    assert_eq!(observed.pending_bytes, 0);
}

#[test]
fn failure_during_checkpoint_capture_rejects_without_consuming_sequence() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("capture-failure.db");
    let engine = capture_engine(&path);
    let store = engine.flush_handle().unwrap();
    let before = store.status();
    let result = store.push(
        2,
        &(),
        Vec::new(),
        Vec::new(),
        Some(|| {
            store.0.shared.state.lock().unwrap().status.error =
                Some("injected worker failure".into());
            Checkpoint::capture(&engine)
        }),
    );
    assert_eq!(
        result.unwrap_err().code,
        tor_protocol::ErrorCode::StorageFailure
    );
    let rejected = store.status();
    assert_eq!(rejected.accepted_sequence, before.accepted_sequence);
    assert_eq!(rejected.durable_sequence, before.durable_sequence);
    assert_eq!(rejected.pending_bytes, 0);
    assert_eq!(rejected.checkpoint_sequence, before.checkpoint_sequence);
    store.request_flush();
    store
        .push(
            2,
            &(),
            Vec::new(),
            Vec::new(),
            Some(|| Checkpoint::capture(&engine)),
        )
        .unwrap();
    store.flush().unwrap();
    assert_eq!(
        store.status().accepted_sequence,
        before.accepted_sequence + 1
    );
    let expected = engine.state(ActorId(1)).unwrap();
    drop(engine);
    drop(store);
    let restored = Engine::open(&path, Scenario::two_room(0)).unwrap();
    assert_eq!(restored.state(ActorId(1)).unwrap(), expected);
}

#[test]
fn queued_execution_rejection_retains_work_and_recovery_preserves_receipts() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("queued-execution.db");
    let mut engine = capture_engine(&path);
    engine.register_input_observer(ActorId(1)).unwrap();
    let branch = engine.branch().clone();
    let command = crate::journal::Command::AdmitIntention {
        expected_boundary: engine.causal_boundary(ActorId(1)).unwrap(),
        action: crate::journal::Action::Move {
            direction: crate::journal::Direction::East,
        },
    };
    let admission = engine
        .command("p", "test", ActorId(1), "queued", &branch, command.clone())
        .unwrap();
    engine.flush().unwrap();
    let store = engine.flush_handle().unwrap();
    let before = engine.state(ActorId(1)).unwrap();
    let counts = engine.profile_counts();
    let input_generation = engine.input_generation(ActorId(1)).unwrap();
    let saved = store.status();
    store.0.shared.state.lock().unwrap().status.error = Some("injected worker failure".into());
    for _ in 0..2 {
        assert_eq!(
            engine.execute_next_intention().unwrap_err().code,
            tor_protocol::ErrorCode::StorageFailure
        );
        assert_eq!(engine.state(ActorId(1)).unwrap(), before);
        assert_eq!(engine.profile_counts(), counts);
        assert_eq!(
            engine.input_generation(ActorId(1)).unwrap(),
            input_generation,
            "failed persistence cannot consume input authority"
        );
        assert_eq!(store.status().accepted_sequence, saved.accepted_sequence);
    }
    store.0.shared.state.lock().unwrap().status.error = None;
    let executed = engine.execute_next_intention().unwrap().unwrap();
    assert!(
        engine.input_generation(ActorId(1)).unwrap() > input_generation,
        "only the successfully admitted execution changes input authority"
    );
    assert!(engine.execute_next_intention().unwrap().is_none());
    let after = engine.state(ActorId(1)).unwrap();
    assert_ne!(after, before);
    engine.flush().unwrap();
    drop(engine);
    drop(store);
    let mut restored = Engine::open(&path, Scenario::two_room(0)).unwrap();
    assert_eq!(restored.state(ActorId(1)).unwrap(), after);
    assert!(restored.execute_next_intention().unwrap().is_none());
    let retry = restored
        .command("p", "test", ActorId(1), "queued", &branch, command)
        .unwrap();
    assert!(retry.duplicate);
    assert_eq!(retry.entry, admission.entry);
    assert!(restored
        .history(ActorId(1), "p", None, 10)
        .unwrap()
        .entries
        .contains(&restored.disclose_entry(&executed.entry).unwrap()));
}

#[test]
fn concurrent_producers_keep_a_contiguous_durable_prefix() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("producer-order.db");
    let engine = capture_engine(&path);
    let store = engine.flush_handle().unwrap();
    let before = store.status();
    let gate = Arc::new(std::sync::Barrier::new(8));
    let producers: Vec<_> = (0..8)
        .map(|_| {
            let writer = store.clone();
            let gate = gate.clone();
            std::thread::spawn(move || {
                gate.wait();
                writer.wizard()
            })
        })
        .collect();
    for producer in producers {
        producer.join().unwrap().unwrap();
    }
    store.flush().unwrap();
    let after = store.status();
    assert_eq!(after.accepted_sequence, before.accepted_sequence + 8);
    assert_eq!(after.durable_sequence, after.accepted_sequence);
    assert_eq!(after.pending_bytes, 0);
    let expected = engine.state(ActorId(1)).unwrap();
    drop(engine);
    drop(store);
    // Opening validates every frame's sequence and the checkpoint prefix.
    let restored = Engine::open(&path, Scenario::two_room(0)).unwrap();
    assert_eq!(restored.state(ActorId(1)).unwrap(), expected);
}

/// A saved corridor game with halls 1 and 2 detached and no checkpoint
/// yet. Journal-only transactions may already have made their rows durable.
fn streaming_fixture(path: &Path) -> (Vec<u8>, RegionWrite, tor_protocol::StateView) {
    let mut engine = Engine::open_with_policy(
        path,
        corridor(),
        SavePolicy {
            checkpoint_interval: 0,
            ..SavePolicy::default()
        },
    )
    .unwrap();
    walk(&mut engine, crate::journal::Direction::East, TO_HALL_4);
    assert_eq!(engine.region_counts().unwrap().detached, 2);
    engine.flush().unwrap();
    let (_, _, save_id, _, _, _) = load(path).unwrap();
    let checkpoint = Checkpoint::capture(&engine);
    let bytes = encode_checkpoint(&checkpoint.encode(&save_id, TO_HALL_4 as u64)).unwrap();
    let mut regions = RegionWrite::encode(&checkpoint).unwrap();
    let conn = connection(path).unwrap();
    let content = corridor().content().unwrap();
    regions.rows = checkpoint
        .referenced()
        .into_iter()
        .map(|id| {
            let record = checkpoint
                .records
                .get(&id)
                .map(|record| (**record).clone())
                .or_else(|| read_region_row(&conn, id, content.clone()).unwrap())
                .unwrap();
            (id.0, region_frame(id.0, &record).unwrap())
        })
        .collect();
    assert_eq!(regions.rows.len(), 2);
    (bytes, regions, engine.state(ActorId(1)).unwrap())
}

/// Killing the process at any stage of a checkpoint that writes region
/// rows leaves either the checkpoint with its rows or neither, and the
/// game continues: detached halls come back from their rows, or from
/// records replay makes again.
#[test]
fn process_death_while_writing_region_rows_recovers_a_complete_prefix() {
    for stage in [
        "after_append",
        "after_regions",
        "after_checkpoint",
        "after_history",
        "after_rotation",
        "after_gc",
        "before_commit",
        "after_commit",
    ] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("crash.db");
        let (bytes, regions, expected) = streaming_fixture(&path);
        let baseline_rows: i64 = connection(&path)
            .unwrap()
            .query_row("SELECT count(*) FROM regions", [], |row| row.get(0))
            .unwrap();
        std::fs::write(path.with_extension("checkpoint"), bytes).unwrap();
        std::fs::write(
            path.with_extension("rows"),
            serde_json::to_vec(&(TO_HALL_4 as u64, &regions.rows, &regions.keep)).unwrap(),
        )
        .unwrap();
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "storage::tests::checkpoint_crash_child",
                "--nocapture",
            ])
            .env("TOR_CHECKPOINT_TEST_PATH", &path)
            .env("TOR_CHECKPOINT_TEST_STAGE", stage)
            .stdout(std::process::Stdio::null())
            .status()
            .unwrap();
        assert_eq!(status.code(), Some(81), "{stage}");
        let committed = stage == "after_commit";
        let rows: i64 = connection(&path)
            .unwrap()
            .query_row("SELECT count(*) FROM regions", [], |r| r.get(0))
            .unwrap();
        assert_eq!(rows, if committed { 2 } else { baseline_rows }, "{stage}");
        let mut recovered = Engine::open(&path, Scenario::two_room(0)).unwrap();
        assert_eq!(recovered.state(ActorId(1)).unwrap(), expected, "{stage}");
        assert_eq!(
            recovered.recovery_profile().records_replayed,
            if committed { 0 } else { TO_HALL_4 },
            "{stage}"
        );
        walk(&mut recovered, crate::journal::Direction::West, TO_HALL_4);
        let counts = recovered.region_counts().unwrap();
        assert_eq!(counts.detached, 4, "{stage}");
        assert_eq!(
            counts.records_read,
            if committed { 2 } else { 0 },
            "{stage}"
        );
    }
}

/// A damaged or missing region row fails closed: the save still opens
/// (rows aren't read then), the command that needs the row is rejected
/// without changing the game, its history or the file, and the game can
/// still be played up to that point.
#[test]
fn damaged_and_missing_region_rows_fail_closed_atomically() {
    for damage in ["flipped", "missing"] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("damaged.db");
        let policy = SavePolicy {
            checkpoint_interval: 4,
            ..SavePolicy::default()
        };
        let mut engine = Engine::open_with_policy(&path, corridor(), policy.clone()).unwrap();
        walk(&mut engine, crate::journal::Direction::East, TO_HALL_4);
        engine.flush().unwrap();
        drop(engine);
        let conn = connection(&path).unwrap();
        let (id, mut frame): (i64, Vec<u8>) = conn
            .query_row(
                "SELECT record,frame FROM regions ORDER BY record",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        match damage {
            "flipped" => {
                *frame.last_mut().unwrap() ^= 1;
                conn.execute(
                    "UPDATE regions SET frame=?1 WHERE record=?2",
                    params![frame, id],
                )
                .unwrap();
            }
            _ => {
                conn.execute("DELETE FROM regions WHERE record=?1", [id])
                    .unwrap();
            }
        }
        drop(conn);

        let mut engine = Engine::open_with_policy(&path, corridor(), policy.clone()).unwrap();
        let mut failure = None;
        for step in 0..TO_HALL_4 {
            let before = engine.state(ActorId(1)).unwrap();
            let before_boundary = engine.causal_boundary(ActorId(1)).unwrap();
            let history = engine.history(ActorId(1), "player", None, 100).unwrap();
            engine.flush().unwrap();
            let file = std::fs::read(&path).unwrap();
            let result = engine.command(
                "player",
                "test",
                ActorId(1),
                &format!("back-{step}"),
                &engine.branch().clone(),
                crate::journal::Command::Act {
                    expected_boundary: before_boundary,
                    action: Action::Move {
                        direction: crate::journal::Direction::West,
                    },
                },
            );
            if let Err(error) = result {
                assert_eq!(
                    error.code,
                    tor_protocol::ErrorCode::StorageFailure,
                    "{damage}"
                );
                assert_eq!(engine.state(ActorId(1)).unwrap(), before, "{damage}");
                assert_eq!(
                    engine.history(ActorId(1), "player", None, 100).unwrap(),
                    history,
                    "{damage}"
                );
                engine.flush().unwrap();
                assert_eq!(std::fs::read(&path).unwrap(), file, "{damage}");
                failure = Some(step);
                break;
            }
        }
        assert!(
            failure.is_some(),
            "{damage}: the damaged row was never needed"
        );
    }
}

/// Reading a region row needs only a shared lock: it succeeds while
/// another connection holds a write reservation, as the save worker does
/// during a commit, without waiting for it.
#[test]
fn region_rows_can_be_read_during_another_connections_write() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("reader.db");
    let mut engine = Engine::open_with_policy(
        &path,
        corridor(),
        SavePolicy {
            checkpoint_interval: 4,
            ..SavePolicy::default()
        },
    )
    .unwrap();
    walk(&mut engine, crate::journal::Direction::East, TO_HALL_4);
    engine.flush().unwrap();
    let id: i64 = connection(&path)
        .unwrap()
        .query_row("SELECT min(record) FROM regions", [], |r| r.get(0))
        .unwrap();
    let writer = connection(&path).unwrap();
    writer.execute_batch("BEGIN IMMEDIATE").unwrap();
    let started = Instant::now();
    let record = engine
        .flush_handle()
        .unwrap()
        .read_region(tor_simulation::RecordId(id as u64));
    assert!(matches!(record, Ok(Some(_))), "{record:?}");
    assert!(started.elapsed() < Duration::from_millis(500));
    writer.execute_batch("ROLLBACK").unwrap();
}

#[test]
fn input_ownership_retires_only_the_committed_admission() {
    let mut inputs = InputOwnership::default();
    let source = vec![(5, Arc::<str>::from("same immutable input"))];
    inputs.admit(1, &source);
    inputs.admit(2, &source);
    inputs.committed(1, &source);
    assert_eq!(inputs.pending.get(&5).unwrap().0, 2);
    assert!(Arc::ptr_eq(
        &inputs.pending.get(&5).unwrap().1,
        &source[0].1
    ));
    inputs.committed(2, &source);
    assert!(inputs.pending.is_empty());
    assert!(inputs.known.contains(&5));
}

#[test]
fn admitted_source_inputs_are_readable_before_sqlite_commit() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("pending-input.db");
    let engine = Engine::open(&path, Scenario::two_room(42)).unwrap();
    let store = engine.flush_handle().unwrap();
    let writer = connection(&path).unwrap();
    // A write reservation makes the absence of a committed source row
    // deterministic regardless of worker scheduling or flush intervals.
    writer.execute_batch("BEGIN IMMEDIATE").unwrap();
    let input: Arc<str> = Arc::from("immutable admitted input");
    store
        .push(
            2,
            &(),
            vec![(999, input.clone())],
            Vec::new(),
            None::<fn() -> Checkpoint>,
        )
        .unwrap();
    let mut reader = store.source_reader();
    let read = reader.read(999);
    let rows: i64 = writer
        .query_row(
            "SELECT count(*) FROM region_sources WHERE region=999",
            [],
            |row| row.get(0),
        )
        .unwrap();
    writer.execute_batch("ROLLBACK").unwrap();
    assert_eq!(rows, 0);
    assert_eq!(read.unwrap().as_deref(), Some(input.as_ref()));
    assert!(reader.read(1000).unwrap().is_none());
    store.flush().unwrap();
    assert!(store.0.shared.inputs.lock().unwrap().pending.is_empty());
    assert_eq!(reader.read(999).unwrap().as_deref(), Some(input.as_ref()));
    writer
        .execute("DELETE FROM region_sources WHERE region=999", [])
        .unwrap();
    assert!(reader
        .read(999)
        .unwrap_err()
        .message
        .contains("compiled region 999 is missing"));
}

#[test]
fn crc_and_frame_corruption() {
    assert_eq!(crc32c(b"123456789".iter().copied()), 0xe3069283);
    let bytes = frame(1, 1, &serde_json::json!({"test":1})).unwrap();
    for index in 0..bytes.len() {
        let mut bad = bytes.clone();
        bad[index] ^= 1;
        assert!(decode(&bad, 1).is_err(), "byte {index}");
    }
    for cut in 0..bytes.len() {
        assert!(decode(&bytes[..cut], 1).is_err());
    }
}
#[test]
fn uncertain_commit_retry_reconciles_bytes_and_never_overwrites() {
    let dir = tempfile::tempdir().unwrap();
    let mut conn = connection(&dir.path().join("retry.db")).unwrap();
    conn.execute_batch(
            "CREATE TABLE journal(sequence INTEGER PRIMARY KEY,frame BLOB NOT NULL) STRICT; CREATE TABLE history(sequence INTEGER PRIMARY KEY,frame BLOB NOT NULL) STRICT;",
        )
        .unwrap();
    let mut batch = vec![Pending {
        attachment: None,
        regions: vec![],
        sources: Vec::new(),
        sequence: 1,
        bytes: vec![1, 2, 3],
        queued: Instant::now(),
    }];
    commit_batch(&mut conn, &batch, None, None, |_, _| Ok(())).unwrap();
    commit_batch(&mut conn, &batch, None, None, |_, _| Ok(())).unwrap();
    assert_eq!(
        conn.query_row("SELECT count(*) FROM journal", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
    batch[0].bytes[0] = 9;
    assert!(commit_batch(&mut conn, &batch, None, None, |_, _| Ok(())).is_err());
    assert_eq!(
        conn.query_row("SELECT frame FROM journal", [], |r| r.get::<_, Vec<u8>>(0))
            .unwrap(),
        vec![1, 2, 3]
    );
}
#[test]
fn idle_opportunities_never_starve_deadline_or_pressure() {
    let p = SavePolicy::default();
    assert!(!p.due(Duration::from_secs(20), Duration::from_secs(2), 0, false));
    assert!(!p.due(Duration::from_secs(40), Duration::ZERO, 0, false));
    assert!(p.due(Duration::from_secs(40), Duration::from_secs(1), 0, false));
    assert!(p.due(Duration::from_secs(60), Duration::ZERO, 0, false));
    assert!(p.due(Duration::ZERO, Duration::ZERO, p.max_pending_bytes, false));
    assert!(p.due(Duration::ZERO, Duration::ZERO, 0, true));
}

/// A journal record at sequence 5 copying region 1's file. Its frame is
/// never decoded here; these tests look at the tables.
fn copying_record(text: &str) -> Pending {
    Pending {
        attachment: None,
        regions: vec![],
        sequence: 5,
        bytes: b"record".to_vec(),
        sources: vec![(1, Arc::from(text))],
        queued: Instant::now(),
    }
}

fn copied(conn: &Connection) -> (i64, Vec<(i64, String)>) {
    let records = conn
        .query_row("SELECT count(*) FROM journal WHERE sequence=5", [], |r| {
            r.get(0)
        })
        .unwrap();
    let sources = conn
        .prepare("SELECT region,source FROM region_sources ORDER BY region")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    (records, sources)
}

#[test]
fn region_files_over_the_queue_limit_still_fit_an_empty_queue() {
    let max = 8 * MAX_PAYLOAD;
    // Region files over the limit still fit an empty queue...
    assert!(fits_queue(0, 100, 9 * MAX_PAYLOAD, max));
    // ...but not one that holds anything.
    assert!(!fits_queue(1, 100, 9 * MAX_PAYLOAD, max));
    assert!(fits_queue(MAX_PAYLOAD, 100, MAX_PAYLOAD, max));
    // The record itself must always fit.
    assert!(!fits_queue(0, max + 1, 0, max));
    assert!(!fits_queue(max, 1, 0, max));
}

#[test]
fn opening_a_save_reads_no_region_file_copies_until_needed() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("copies.db");
    streaming_fixture(&path);
    let (_, archive, _, _, _, saved) = load(&path).unwrap();
    assert!(saved.len() >= 4, "{saved:?}");
    let package = archive.scenario.package.unwrap();
    assert_eq!(package.sources.texts_in_memory(), 0);
}

#[test]
fn region_files_commit_with_the_record_that_needs_them_and_are_written_once() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("sources.db");
    checkpoint_fixture(&path);
    let mut conn = connection(&path).unwrap();
    let commit = |conn: &mut Connection, entry: Pending, stage: &str| {
        commit_batch(conn, &[entry], None, None, |_, at| {
            if at == stage {
                Err(storage_failure())
            } else {
                Ok(())
            }
        })
    };
    // A failure anywhere in the transaction leaves neither behind.
    for stage in ["after_sources", "after_append", "before_commit"] {
        assert!(commit(&mut conn, copying_record("id = 1"), stage).is_err());
        assert_eq!(copied(&conn), (0, vec![]), "{stage}");
    }
    commit(&mut conn, copying_record("id = 1"), "").unwrap();
    assert_eq!(copied(&conn), (1, vec![(1, "id = 1".into())]));
    // A retry must match exactly; a copy is never rewritten.
    commit(&mut conn, copying_record("id = 1"), "").unwrap();
    assert!(commit(&mut conn, copying_record("id = 1 # edited"), "").is_err());
    assert_eq!(copied(&conn), (1, vec![(1, "id = 1".into())]));
}

#[test]
fn sources_crash_child() {
    let Ok(path) = std::env::var("TOR_SOURCES_TEST_PATH") else {
        return;
    };
    let stage = std::env::var("TOR_SOURCES_TEST_STAGE").unwrap();
    let mut conn = connection(Path::new(&path)).unwrap();
    conn.execute_batch("PRAGMA cache_size=1").unwrap();
    commit_batch(
        &mut conn,
        &[copying_record(&"id = 1\n".repeat(4096))],
        None,
        None,
        |connection, at| {
            if at == stage {
                connection.cache_flush().unwrap();
                std::process::exit(81);
            }
            Ok(())
        },
    )
    .unwrap();
    panic!("crash stage was not reached");
}

#[test]
fn process_death_while_copying_region_files_keeps_each_record_with_its_files() {
    for (stage, committed) in [
        ("after_sources", false),
        ("after_append", false),
        ("before_commit", false),
        ("after_commit", true),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("crash.db");
        checkpoint_fixture(&path);
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "storage::tests::sources_crash_child",
                "--nocapture",
            ])
            .env("TOR_SOURCES_TEST_PATH", &path)
            .env("TOR_SOURCES_TEST_STAGE", stage)
            .stdout(std::process::Stdio::null())
            .status()
            .unwrap();
        assert_eq!(status.code(), Some(81), "{stage}");
        let conn = connection(&path).unwrap();
        let (records, sources) = copied(&conn);
        assert_eq!(records, i64::from(committed), "{stage}");
        assert_eq!(sources.len(), usize::from(committed), "{stage}");
    }
}
