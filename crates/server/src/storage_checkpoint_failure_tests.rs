//! Per-store faults exercise the real worker/encoder without global environment
//! overrides. A reduced test limit is not evidence that gameplay reaches the cap.
use super::*;
use crate::journal::{Action, Command};
use crate::{Engine, Scenario};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use tor_protocol::ActorId;

#[derive(Debug)]
struct Pause {
    entered: Sender<()>,
    resume: Receiver<()>,
}
#[derive(Debug)]
pub(super) struct CheckpointTest {
    limit: AtomicUsize,
    attempts: AtomicUsize,
    pause: Mutex<Option<Pause>>,
}
impl Default for CheckpointTest {
    fn default() -> Self {
        Self {
            limit: AtomicUsize::new(checkpoint_codec::LIMIT),
            attempts: AtomicUsize::new(0),
            pause: Mutex::new(None),
        }
    }
}
pub(super) fn encode(
    shared: &Shared,
    checkpoint: &DiskCheckpoint,
) -> Result<Result<Vec<u8>, checkpoint_codec::EncodeError>, SaveFailure> {
    let test = &shared.checkpoint_test;
    test.attempts.fetch_add(1, Ordering::SeqCst);
    let limit = test.limit.load(Ordering::SeqCst);
    let pause = test.pause.lock().unwrap().take();
    if let Some(pause) = pause {
        pause.entered.send(()).map_err(|_| SaveFailure::Retryable)?;
        pause
            .resume
            .recv_timeout(Duration::from_secs(10))
            .map_err(|_| SaveFailure::Retryable)?;
    }
    Ok(checkpoint_codec::write_json_classified_with_test_limit(
        checkpoint,
        Vec::new(),
        limit,
    ))
}

fn fixture(path: &Path) -> Engine {
    Engine::open_with_policy(
        path,
        Scenario::two_room(42),
        SavePolicy {
            target_interval: Duration::from_secs(3_600),
            idle_interval: Duration::ZERO,
            max_unsaved_age: Duration::from_secs(3_600),
            checkpoint_interval: 1,
            ..SavePolicy::default()
        },
    )
    .unwrap()
}
fn accept_wait(engine: &mut Engine, request: &str) -> Result<(), Failure> {
    engine
        .command(
            "player",
            "test",
            ActorId(1),
            request,
            &engine.branch().clone(),
            Command::Act {
                expected_boundary: engine.causal_boundary(ActorId(1)).unwrap(),
                action: Action::Wait,
            },
        )
        .map(|_| ())
}
#[derive(Debug, PartialEq, Eq)]
struct PendingWitness {
    sequence: u64,
    bytes: Vec<u8>,
    regions: Vec<(u64, Vec<u8>)>,
    sources: Vec<(u64, Arc<str>)>,
}
#[derive(Debug, PartialEq, Eq)]
struct RetainedWork {
    pending: Vec<PendingWitness>,
    checkpoint: Option<(u64, Vec<u8>)>,
    accepted: u64,
    durable: u64,
    pending_bytes: usize,
}
fn retained(store: &Store) -> RetainedWork {
    let state = store.0.shared.state.lock().unwrap();
    RetainedWork {
        pending: state
            .pending
            .iter()
            .map(|entry| PendingWitness {
                sequence: entry.sequence,
                bytes: entry.bytes.clone(),
                regions: entry.regions.clone(),
                sources: entry.sources.clone(),
            })
            .collect(),
        checkpoint: state.checkpoint.as_ref().map(|(seq, checkpoint)| {
            (
                *seq,
                serde_json::to_vec(&checkpoint.encode(&store.0.save_id, *seq)).unwrap(),
            )
        }),
        accepted: state.status.accepted_sequence,
        durable: state.status.durable_sequence,
        pending_bytes: state.status.pending_bytes,
    }
}
fn attempts(store: &Store) -> usize {
    store
        .0
        .shared
        .checkpoint_test
        .attempts
        .load(Ordering::SeqCst)
}
#[test]
fn retryable_database_failure_keeps_exact_batch_and_reopens_after_save_retry() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("locked.db");
    let mut engine = fixture(&path);
    let store = engine.flush_handle().unwrap();
    let blocker = connection(&path).unwrap();
    blocker.execute_batch("BEGIN EXCLUSIVE").unwrap();
    accept_wait(&mut engine, "accepted").unwrap();
    let work = retained(&store);
    let expected = engine.state(ActorId(1)).unwrap();
    assert!(store.flush().is_err());
    assert_eq!(attempts(&store), 1);
    assert_eq!(retained(&store), work);
    assert!(store
        .status()
        .error
        .unwrap()
        .ends_with("Use save to retry."));
    assert!(accept_wait(&mut engine, "blocked").is_err());
    assert_eq!(engine.state(ActorId(1)).unwrap(), expected);
    blocker.execute_batch("ROLLBACK").unwrap();
    store.flush().unwrap();
    assert_eq!(attempts(&store), 2);
    let status = store.status();
    assert_eq!(status.accepted_sequence, 1);
    assert_eq!(status.durable_sequence, 1);
    assert_eq!(status.checkpoint_sequence, 1);
    assert_eq!(status.pending_bytes, 0);
    assert!(status.error.is_none());
    let disk = connection(&path).unwrap();
    let frame: Vec<u8> = disk
        .query_row("SELECT frame FROM history WHERE sequence=1", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(frame, work.pending[0].bytes);
    drop(disk);
    drop(blocker);
    drop(store);
    drop(engine);
    let restored = Engine::open(&path, Scenario::two_room(0)).unwrap();
    assert_eq!(restored.state(ActorId(1)).unwrap(), expected);
}

#[test]
fn capacity_deferral_of_older_snapshot_does_not_block_a_newer_retained_checkpoint() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("superseded.db");
    let mut engine = fixture(&path);
    let store = engine.flush_handle().unwrap();
    let (entered, observed) = mpsc::channel();
    let (resume, continuation) = mpsc::channel();
    *store.0.shared.checkpoint_test.pause.lock().unwrap() = Some(Pause {
        entered,
        resume: continuation,
    });
    store
        .0
        .shared
        .checkpoint_test
        .limit
        .store(0, Ordering::SeqCst);
    accept_wait(&mut engine, "first").unwrap();
    store.request_flush();
    observed.recv_timeout(Duration::from_secs(10)).unwrap();
    accept_wait(&mut engine, "second").unwrap();
    let expected = engine.state(ActorId(1)).unwrap();
    store
        .0
        .shared
        .checkpoint_test
        .limit
        .store(checkpoint_codec::LIMIT, Ordering::SeqCst);
    resume.send(()).unwrap();
    store.flush().unwrap();
    assert_eq!(attempts(&store), 2);
    assert_eq!(store.status().durable_sequence, 2);
    assert_eq!(store.status().checkpoint_sequence, 2);
    assert_eq!(store.status().pending_bytes, 0);
    drop(store);
    drop(engine);
    let restored = Engine::open(&path, Scenario::two_room(0)).unwrap();
    assert_eq!(restored.state(ActorId(1)).unwrap(), expected);
}

#[test]
fn capacity_deferral_keeps_journal_durable_with_and_without_prior_checkpoint() {
    for prior_checkpoint in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("deferred.db");
        let mut engine = fixture(&path);
        let store = engine.flush_handle().unwrap();
        let baseline = if prior_checkpoint {
            accept_wait(&mut engine, "baseline").unwrap();
            store.flush().unwrap();
            1
        } else {
            0
        };
        store
            .0
            .shared
            .checkpoint_test
            .limit
            .store(0, Ordering::SeqCst);
        accept_wait(&mut engine, "oversized").unwrap();
        store
            .flush()
            .expect("checkpoint overflow must not prevent journal durability");
        assert_eq!(store.status().durable_sequence, baseline + 1);
        assert_eq!(store.status().checkpoint_sequence, baseline);
        assert_eq!(store.status().pending_bytes, 0);
        assert!(store.status().error.is_none());
        assert_eq!(
            store.status().checkpoint_deferred_sequence,
            Some(baseline + 1)
        );
        let encoded_attempts = attempts(&store);
        store.flush().unwrap();
        assert_eq!(attempts(&store), encoded_attempts);
        accept_wait(&mut engine, "continued while deferred").unwrap();
        store.flush().unwrap();
        assert_eq!(store.status().durable_sequence, baseline + 2);
        assert_eq!(store.status().checkpoint_sequence, baseline);
        let expected = engine.state(ActorId(1)).unwrap();
        drop(store);
        drop(engine);
        let mut restored = fixture(&path);
        assert_eq!(restored.state(ActorId(1)).unwrap(), expected);
        accept_wait(&mut restored, "continue").unwrap();
        restored.flush_handle().unwrap().flush().unwrap();
        assert_eq!(restored.save_status().checkpoint_sequence, baseline + 3);
    }
}

#[test]
fn capacity_deferral_does_not_hide_disk_failure_or_retry_discarded_snapshot() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("deferred-locked.db");
    let mut engine = fixture(&path);
    let store = engine.flush_handle().unwrap();
    store
        .0
        .shared
        .checkpoint_test
        .limit
        .store(0, Ordering::SeqCst);
    let blocker = connection(&path).unwrap();
    blocker.execute_batch("BEGIN EXCLUSIVE").unwrap();
    accept_wait(&mut engine, "accepted").unwrap();
    let before = retained(&store);
    assert!(store.flush().is_err());
    let after = retained(&store);
    assert_eq!(after.pending, before.pending);
    assert_eq!(after.pending_bytes, before.pending_bytes);
    assert_eq!(after.durable, 0);
    assert!(
        after.checkpoint.is_none(),
        "oversized capture must be dropped"
    );
    assert_eq!(attempts(&store), 1);
    assert!(store.status().error.is_some());
    assert!(accept_wait(&mut engine, "blocked").is_err());
    blocker.execute_batch("ROLLBACK").unwrap();
    store.flush().unwrap();
    assert_eq!(attempts(&store), 1);
    assert_eq!(store.status().durable_sequence, 1);
    assert_eq!(store.status().pending_bytes, 0);
    assert_eq!(store.status().checkpoint_sequence, 0);
    assert!(store.status().error.is_none());
    assert_eq!(store.status().checkpoint_deferred_sequence, Some(1));
    store
        .0
        .shared
        .checkpoint_test
        .limit
        .store(checkpoint_codec::LIMIT, Ordering::SeqCst);
    accept_wait(&mut engine, "valid checkpoint").unwrap();
    store.flush().unwrap();
    assert_eq!(store.status().checkpoint_sequence, 2);
    assert_eq!(store.status().checkpoint_deferred_sequence, None);
    let expected = engine.state(ActorId(1)).unwrap();
    drop(blocker);
    drop(store);
    drop(engine);
    let restored = Engine::open(&path, Scenario::two_room(0)).unwrap();
    assert_eq!(restored.state(ActorId(1)).unwrap(), expected);
}

#[test]
fn deferred_checkpoint_transaction_preserves_old_selection_and_atomic_tail_inputs() {
    for stage in [
        "after_generated_regions",
        "after_sources",
        "after_append",
        "before_commit",
        "after_commit",
    ] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("atomic-tail.db");
        let mut engine = fixture(&path);
        accept_wait(&mut engine, "baseline").unwrap();
        engine.flush_handle().unwrap().flush().unwrap();
        drop(engine);
        let mut conn = connection(&path).unwrap();
        let old_checkpoint: (i64, Vec<u8>) = conn
            .query_row("SELECT sequence,payload FROM checkpoint", [], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })
            .unwrap();
        let old_history: i64 = conn
            .query_row("SELECT count(*) FROM history", [], |row| row.get(0))
            .unwrap();
        // A row retained by the old selected checkpoint must not be collected
        // simply because the optional replacement could not be encoded.
        conn.execute("INSERT INTO regions VALUES (900, x'010203')", [])
            .unwrap();
        let entry = Pending {
            attachment: None,
            sequence: 2,
            bytes: vec![4, 5, 6],
            regions: vec![(901, vec![7, 8, 9])],
            sources: vec![(902, Arc::from("copied tail source"))],
            queued: Instant::now(),
        };
        assert!(commit_batch(&mut conn, &[entry], None, None, |_, actual| {
            if actual == stage {
                Err(storage_failure())
            } else {
                Ok(())
            }
        })
        .is_err());
        let committed = i64::from(stage == "after_commit");
        for query in [
            "SELECT count(*) FROM journal WHERE sequence=2",
            "SELECT count(*) FROM regions WHERE record=901",
            "SELECT count(*) FROM region_sources WHERE region=902",
        ] {
            assert_eq!(
                conn.query_row(query, [], |row| row.get::<_, i64>(0))
                    .unwrap(),
                committed,
                "{stage}: {query}"
            );
        }
        assert_eq!(
            conn.query_row("SELECT count(*) FROM regions WHERE record=900", [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap(),
            1
        );
        assert_eq!(
            conn.query_row("SELECT count(*) FROM history", [], |row| row
                .get::<_, i64>(0))
                .unwrap(),
            old_history
        );
        assert_eq!(
            conn.query_row("SELECT sequence,payload FROM checkpoint", [], |row| Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, Vec<u8>>(1)?
            )))
            .unwrap(),
            old_checkpoint
        );
    }
}

#[test]
fn deferred_checkpoint_replays_streamed_sources_and_rewinds_detached_halls() {
    use super::tests::{corridor, TO_HALL_4};
    use crate::journal::{Direction, Event, JournalContent, WizardOperation};

    fn moved(engine: &mut Engine, direction: Direction, at: &mut crate::journal::Position) {
        // A streamed NPC may become ready before the player even while the
        // player remains in an earlier hall. Run the ordinary scheduler work;
        // direct Command::Act does not pump other actors automatically.
        for _ in 0..16 {
            if engine.next_actor() == Some(ActorId(1)) {
                break;
            }
            let next = engine.next_actor().unwrap_or_else(|| {
                panic!(
                    "no ready actor before {direction:?} at {at:?}: {:?}",
                    engine.region_counts()
                )
            });
            assert!(
                engine.is_ai(next),
                "unexpected next actor {next:?} at {at:?}"
            );
            engine.advance_ai(next).unwrap_or_else(|error| {
                panic!("AI {next:?} before {direction:?} at {at:?}: {error:?}")
            });
        }
        assert_eq!(
            engine.next_actor(),
            Some(ActorId(1)),
            "NPC work did not yield before {direction:?} at {at:?}: {:?}",
            engine.region_counts()
        );
        let revision = engine.causal_boundary(ActorId(1)).unwrap();
        let result = engine
            .command(
                "player",
                "test",
                ActorId(1),
                &format!("checked-walk-{direction:?}-{revision}"),
                &engine.branch().clone(),
                Command::Act {
                    expected_boundary: revision,
                    action: Action::Move { direction },
                },
            )
            .unwrap_or_else(|error| {
                panic!(
                    "move {direction:?} from {at:?} at revision {revision}: {error:?}; next={:?}, state={:?}, counts={:?}",
                    engine.next_actor(), engine.state(ActorId(1)), engine.region_counts()
                )
            });
        assert!(!result.duplicate);
        assert_eq!(engine.causal_boundary(ActorId(1)).unwrap(), revision + 1);
        let JournalContent::Action {
            event: Event::Moved { from, to },
            ..
        } = result.entry.content
        else {
            panic!("movement did not execute: {:?}", result.entry);
        };
        assert_ne!(from, to);
        assert_eq!(
            from, *at,
            "movement must continue from the actual prior location"
        );
        *at = to;
    }
    fn source_rows(conn: &rusqlite::Connection) -> Vec<(i64, Vec<u8>)> {
        conn.prepare("SELECT region,CAST(source AS BLOB) FROM region_sources ORDER BY region")
            .unwrap()
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
    }
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("streamed-deferral.db");
    let policy = SavePolicy {
        target_interval: Duration::from_secs(3_600),
        idle_interval: Duration::ZERO,
        max_unsaved_age: Duration::from_secs(3_600),
        checkpoint_interval: 1,
        ..SavePolicy::default()
    };
    let mut engine = Engine::open_with_policy(&path, corridor(), policy.clone()).unwrap();
    engine.enable_wizard().unwrap();
    // The authored start anchor. Public observation coordinates are always
    // observer-relative zero, so use executed movement events for this oracle.
    let origin = crate::journal::Position {
        region: 1,
        x: 2,
        y: 1,
        z: 0,
    };
    let mut at = origin;
    // Sight makes neighboring regions active, and their physical-link pins
    // can retain another hall beyond radius zero. Stop on actual detachment,
    // not an assumed hall/step threshold; hall 4 is the established safe bound.
    let mut old_steps = 0;
    while engine.region_counts().unwrap().detached == 0 && old_steps < TO_HALL_4 {
        moved(&mut engine, Direction::East, &mut at);
        assert!(at.region <= 4, "initial leg reached {at:?}");
        old_steps += 1;
    }
    assert!(
        engine.region_counts().unwrap().detached > 0,
        "no detachment after {old_steps} moves: position={:?}, counts={:?}",
        at,
        engine.region_counts()
    );
    let marker = engine
        .command(
            "player",
            "test",
            ActorId(1),
            "streaming-rewind-marker",
            &engine.branch().clone(),
            Command::Act {
                expected_boundary: engine.causal_boundary(ActorId(1)).unwrap(),
                action: Action::Wait,
            },
        )
        .unwrap();
    let marker_revision = engine.causal_boundary(ActorId(1)).unwrap();
    let marker_location = at;
    let marker_observation = engine.state(ActorId(1)).unwrap().observation;
    engine.flush().unwrap();
    let store = engine.flush_handle().unwrap();
    let old_sequence = store.status().checkpoint_sequence;
    assert_eq!(old_sequence, store.status().accepted_sequence);
    let old_references = Checkpoint::capture(&engine).referenced();
    assert!(!old_references.is_empty());
    let disk = connection(&path).unwrap();
    let old_checkpoint: (i64, Vec<u8>, i64) = disk
        .query_row(
            "SELECT sequence,payload,checksum FROM checkpoint",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    let old_rows: Vec<_> = old_references
        .iter()
        .map(|id| {
            let frame: Vec<u8> = disk
                .query_row(
                    "SELECT frame FROM regions WHERE record=?1",
                    [id.0 as i64],
                    |row| row.get(0),
                )
                .unwrap();
            (id.0, frame)
        })
        .collect();
    let old_sources = source_rows(&disk);
    assert_eq!(
        old_sources.len(),
        7,
        "the authored corridor has seven sources"
    );
    let old_detached = engine.region_counts().unwrap().detached;
    drop(disk);
    store
        .0
        .shared
        .checkpoint_test
        .limit
        .store(0, Ordering::SeqCst);
    // This finite authored fixture already persisted every source. It proves
    // preservation/replay of real streamed rows, not new source admission.
    // deferred_checkpoint_transaction_preserves_old_selection_and_atomic_tail_inputs
    // separately faults the transaction around new source/generated-row writes.
    assert!(old_steps < TO_HALL_4);
    for _ in old_steps..TO_HALL_4 {
        moved(&mut engine, Direction::East, &mut at);
        assert!(at.region <= 4, "fallback leg left hall 4: {at:?}");
    }
    assert!(
        engine.region_counts().unwrap().detached > old_detached,
        "continued play must detach further halls: at={at:?}, counts={:?}",
        engine.region_counts()
    );
    assert!(
        engine.causal_boundary(ActorId(1)).unwrap() - marker_revision < 128,
        "rewind marker must remain selectable, including NPC records"
    );
    let expected = engine.state(ActorId(1)).unwrap();
    store.flush().unwrap();
    let accepted = store.status().accepted_sequence;
    assert_eq!(store.status().durable_sequence, accepted);
    assert_eq!(store.status().pending_bytes, 0);
    assert_eq!(store.status().checkpoint_sequence, old_sequence);
    assert!(store.status().checkpoint_deferred_sequence.is_some());
    assert!(store.0.shared.inputs.lock().unwrap().pending.is_empty());
    let disk = connection(&path).unwrap();
    assert_eq!(
        disk.query_row(
            "SELECT sequence,payload,checksum FROM checkpoint",
            [],
            |row| Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, Vec<u8>>(1)?,
                row.get::<_, i64>(2)?
            ))
        )
        .unwrap(),
        old_checkpoint
    );
    for (id, frame) in old_rows {
        assert_eq!(
            disk.query_row(
                "SELECT frame FROM regions WHERE record=?1",
                [id as i64],
                |row| row.get::<_, Vec<u8>>(0)
            )
            .unwrap(),
            frame
        );
    }
    assert_eq!(
        source_rows(&disk),
        old_sources,
        "fallback must preserve every original source byte-for-byte"
    );
    assert_eq!(
        disk.query_row("SELECT max(sequence) FROM journal", [], |row| row
            .get::<_, i64>(0))
            .unwrap(),
        accepted as i64
    );
    drop(disk);
    drop(store);
    drop(engine); // Drops every live cache; recovery must use the durable save.
    let mut resumed = Engine::open_with_policy(&path, Scenario::two_room(0), policy).unwrap();
    assert_eq!(resumed.state(ActorId(1)).unwrap(), expected);
    assert_eq!(resumed.recovery_profile().checkpoint_sequence, old_sequence);
    assert!(resumed.recovery_profile().records_replayed > 0);
    resumed.enable_wizard().unwrap();
    resumed
        .command(
            "player",
            "test",
            ActorId(1),
            "rewind-streamed-deferral",
            &resumed.branch().clone(),
            Command::Wizard {
                display_basis: None,
                expected_boundary: resumed.causal_boundary(ActorId(1)).unwrap(),
                operation: WizardOperation::Rewind {
                    target: Some(marker.entry.id),
                },
            },
        )
        .unwrap();
    assert_eq!(
        resumed.state(ActorId(1)).unwrap().observation,
        marker_observation
    );
    at = marker_location;
    for _ in 0..old_steps {
        moved(&mut resumed, Direction::West, &mut at);
    }
    assert_eq!(at, origin);
    assert!(
        resumed.region_counts().unwrap().records_read > 0,
        "earlier detached records must be restored from disk"
    );
    resumed.flush().unwrap();
    assert_eq!(resumed.save_status().checkpoint_deferred_sequence, None);
    let returned = resumed.state(ActorId(1)).unwrap();
    drop(resumed);
    let reopened = Engine::open(&path, Scenario::two_room(0)).unwrap();
    assert_eq!(reopened.state(ActorId(1)).unwrap(), returned);
}

#[test]
fn only_explicit_record_expansion_capacity_defers_checkpointing() {
    assert!(classify_record_expansion(Ok(true), 42).is_ok());
    assert!(matches!(
        classify_record_expansion(Ok(false), 42),
        Err(SaveFailure::CheckpointCapacity { sequence: 42 })
    ));
    assert!(matches!(
        classify_record_expansion(
            Err(tor_simulation::checkpoint::PersistError::RecordPool),
            42
        ),
        Err(SaveFailure::Retryable)
    ));
}
#[test]
fn malformed_pool_reference_is_not_a_checkpoint_capacity_deferral() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("invalid-reference.db");
    let engine = fixture(&path);
    let store = engine.flush_handle().unwrap();
    let checkpoint = Checkpoint::capture(&engine).encode(&store.0.save_id, 1);
    let mut encoded = serde_json::to_value(checkpoint).unwrap();
    let actors = encoded["game"]["actors"].as_u64().unwrap() as usize;
    encoded["shared"]["actors"][actors]["1"] = serde_json::json!(usize::MAX);
    let invalid: DiskCheckpoint = serde_json::from_value(encoded).unwrap();
    assert!(matches!(
        encode_background_checkpoint(&store.0.shared, 1, &invalid),
        Err(SaveFailure::Retryable)
    ));
    assert_eq!(
        attempts(&store),
        0,
        "invalid references are rejected before encoding"
    );
}
