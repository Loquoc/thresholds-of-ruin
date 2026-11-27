//! Application journal and checkpoints. SQLite supplies atomic batches and recovery.
//!
//! The worker owns background writes after bootstrap. Lazy region/source reads use
//! separate connections on their caller's thread and can wait for SQLite. Producers
//! encode journal records before admission; chart-history growth also checks the
//! complete checkpoint encoding before publication. Neither path holds the worker's
//! shared status lock during encoding or I/O.
use crate::{
    engine::{invalid_archive, storage_failure, Archive, Checkpoint, DiskCheckpoint, Record},
    Failure,
};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

#[path = "checkpoint_codec.rs"]
pub(crate) mod checkpoint_codec;
#[path = "save_codec.rs"]
pub(crate) mod codec;
#[path = "journal_lock.rs"]
mod journal_lock;
pub(crate) use journal_lock::JournalLock;
#[path = "save_schema.rs"]
pub(crate) mod schema;
pub(crate) use codec::frame;
#[path = "storage_attachments.rs"]
mod attachments;
use attachments::AttachmentWrite;
use codec::region_frame;

pub(crate) fn encode_region(
    id: u64,
    record: &tor_simulation::RegionRecord,
) -> Result<Vec<u8>, Failure> {
    region_frame(id, record)
}
#[cfg(test)]
thread_local! {
    static HISTORY_ADMISSION_TEST_LIMIT: std::cell::Cell<Option<usize>> = const { std::cell::Cell::new(None) };
    static HISTORY_ADMISSION_TEST_SIZE: std::cell::Cell<Option<usize>> = const { std::cell::Cell::new(None) };
}
/// Test-only scoped fault on the actual checkpoint encoder, not an alternate
/// storage path. Thread affinity and Drop preserve nested/parallel test isolation.
#[cfg(test)]
pub(crate) struct HistoryAdmissionTestLimit {
    previous: Option<usize>,
    previous_size: Option<usize>,
    _thread: std::marker::PhantomData<std::rc::Rc<()>>,
}
#[cfg(test)]
impl HistoryAdmissionTestLimit {
    pub(crate) fn new(limit: usize) -> Self {
        Self {
            previous: HISTORY_ADMISSION_TEST_LIMIT.with(|value| value.replace(Some(limit))),
            previous_size: HISTORY_ADMISSION_TEST_SIZE.with(|value| value.replace(None)),
            _thread: std::marker::PhantomData,
        }
    }
    pub(crate) fn required_bytes(&self) -> Option<usize> {
        HISTORY_ADMISSION_TEST_SIZE.with(std::cell::Cell::get)
    }
}
#[cfg(test)]
impl Drop for HistoryAdmissionTestLimit {
    fn drop(&mut self) {
        HISTORY_ADMISSION_TEST_LIMIT.with(|value| value.set(self.previous));
        HISTORY_ADMISSION_TEST_SIZE.with(|value| value.set(self.previous_size));
    }
}
/// Measures both sides of the exact raw+compressed encoder ceiling without
/// retaining a second payload. The encoder's last write is its raw-size trailer.
#[cfg(test)]
#[derive(Default)]
struct HistoryAdmissionCounter {
    bytes: usize,
    trailer: [u8; 4],
}
#[cfg(test)]
impl std::io::Write for HistoryAdmissionCounter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.bytes = self
            .bytes
            .checked_add(bytes.len())
            .ok_or_else(|| std::io::Error::other("test size overflow"))?;
        if bytes.len() >= 4 {
            self.trailer.copy_from_slice(&bytes[bytes.len() - 4..]);
        }
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn admit_history_checkpoint(checkpoint: &DiskCheckpoint) -> Result<(), Failure> {
    if !checkpoint
        .record_expansion_within_limit()
        .map_err(|_| storage_failure())?
    {
        return Err(storage_failure());
    }
    #[cfg(test)]
    if let Some(limit) = HISTORY_ADMISSION_TEST_LIMIT.with(std::cell::Cell::get) {
        let size = checkpoint_codec::write_json(checkpoint, HistoryAdmissionCounter::default())
            .map_err(|_| storage_failure())?;
        let required = size.bytes.max(u32::from_le_bytes(size.trailer) as usize);
        HISTORY_ADMISSION_TEST_SIZE.with(|value| value.set(Some(required)));
        return checkpoint_codec::write_json_with_test_limit(checkpoint, std::io::sink(), limit)
            .map(|_| ())
            .map_err(|_| storage_failure());
    }
    checkpoint_codec::write_json(checkpoint, std::io::sink())
        .map(|_| ())
        .map_err(|_| storage_failure())
}
#[cfg(test)]
thread_local! {
    static REGION_ADMISSION_TEST_LIMIT: std::cell::Cell<Option<usize>> = const { std::cell::Cell::new(None) };
    static REGION_ADMISSION_TEST_MEASUREMENTS: std::cell::Cell<(usize,usize)> = const { std::cell::Cell::new((0,0)) };
}
#[cfg(test)]
pub(crate) struct RegionAdmissionTestLimit {
    previous: Option<usize>,
    measurements: (usize, usize),
    _thread: std::marker::PhantomData<std::rc::Rc<()>>,
}
#[cfg(test)]
impl RegionAdmissionTestLimit {
    pub(crate) fn new(limit: usize) -> Self {
        Self {
            previous: REGION_ADMISSION_TEST_LIMIT.with(|v| v.replace(Some(limit))),
            measurements: REGION_ADMISSION_TEST_MEASUREMENTS.with(|v| v.replace((0, 0))),
            _thread: std::marker::PhantomData,
        }
    }
    pub(crate) fn measurements(&self) -> (usize, usize) {
        REGION_ADMISSION_TEST_MEASUREMENTS.with(std::cell::Cell::get)
    }
}
#[cfg(test)]
impl Drop for RegionAdmissionTestLimit {
    fn drop(&mut self) {
        REGION_ADMISSION_TEST_LIMIT.with(|v| v.set(self.previous));
        REGION_ADMISSION_TEST_MEASUREMENTS.with(|v| v.set(self.measurements));
    }
}
/// Actual detached-row JSON ceiling, without allocating a payload copy.
fn admit_region_record(record: &tor_simulation::RegionRecord) -> Result<(), Failure> {
    struct Counter {
        bytes: usize,
        limit: usize,
    }
    impl std::io::Write for Counter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if bytes.len() > self.limit.saturating_sub(self.bytes) {
                return Err(std::io::Error::other("region record exceeds capacity"));
            }
            self.bytes += bytes.len();
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    #[cfg(test)]
    if let Some(limit) = REGION_ADMISSION_TEST_LIMIT.with(std::cell::Cell::get) {
        let mut measured = Counter {
            bytes: 0,
            limit: codec::MAX_REGION,
        };
        serde_json::to_writer(&mut measured, record).map_err(|_| storage_failure())?;
        REGION_ADMISSION_TEST_MEASUREMENTS.with(|v| {
            let (count, max) = v.get();
            v.set((count + 1, max.max(measured.bytes)));
        });
        return serde_json::to_writer(&mut Counter { bytes: 0, limit }, record)
            .map_err(|_| storage_failure());
    }
    serde_json::to_writer(
        &mut Counter {
            bytes: 0,
            limit: codec::MAX_REGION,
        },
        record,
    )
    .map_err(|_| storage_failure())
}

pub use codec::MAX_PAYLOAD;
use codec::{crc32c, decode, decode_region, strict};

const MAX_CHECKPOINT: usize = checkpoint_codec::LIMIT;
const APP_ID: i64 = 0x544f524a;
/// SQLite `user_version`: the save format, as `ARCHIVE_VERSION`.
const SAVE_FORMAT: i64 = crate::engine::ARCHIVE_VERSION as i64;

#[cfg(test)]
thread_local! {
    static SAVED_BYTES_COPIED: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

fn copy_saved_blob(
    row: &rusqlite::Row<'_>,
    column: usize,
    limit: usize,
) -> rusqlite::Result<Vec<u8>> {
    let bytes = saved_blob(row, column, limit)?.to_vec();
    #[cfg(test)]
    SAVED_BYTES_COPIED.with(|count| count.set(count.get() + bytes.len()));
    Ok(bytes)
}

fn saved_blob<'row>(
    row: &'row rusqlite::Row<'_>,
    column: usize,
    limit: usize,
) -> rusqlite::Result<&'row [u8]> {
    saved_bytes(row, column, limit, rusqlite::types::Type::Blob)
}

fn saved_bytes<'row>(
    row: &'row rusqlite::Row<'_>,
    column: usize,
    limit: usize,
    kind: rusqlite::types::Type,
) -> rusqlite::Result<&'row [u8]> {
    let value = row.get_ref(column)?;
    let bytes = match (value, kind) {
        (rusqlite::types::ValueRef::Blob(bytes), rusqlite::types::Type::Blob)
        | (rusqlite::types::ValueRef::Text(bytes), rusqlite::types::Type::Text) => bytes,
        _ => {
            return Err(rusqlite::Error::InvalidColumnType(
                column,
                "saved bytes".into(),
                value.data_type(),
            ))
        }
    };
    if bytes.len() > limit {
        return Err(rusqlite::Error::FromSqlConversionFailure(
            column,
            kind,
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "saved value exceeds its byte limit",
            )),
        ));
    }
    Ok(bytes)
}

fn saved_text<'row>(row: &'row rusqlite::Row<'_>, column: usize) -> rusqlite::Result<&'row str> {
    let bytes = saved_bytes(
        row,
        column,
        crate::scenario_package::MAX_REGION_BYTES as usize,
        rusqlite::types::Type::Text,
    )?;
    std::str::from_utf8(bytes).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            column,
            rusqlite::types::Type::Text,
            Box::new(error),
        )
    })
}

fn copy_saved_text(row: &rusqlite::Row<'_>, column: usize) -> rusqlite::Result<String> {
    let text = saved_text(row, column)?.to_owned();
    #[cfg(test)]
    SAVED_BYTES_COPIED.with(|count| count.set(count.get() + text.len()));
    Ok(text)
}

fn saved_row_failure(error: rusqlite::Error) -> Failure {
    match error {
        rusqlite::Error::FromSqlConversionFailure(..) | rusqlite::Error::InvalidColumnType(..) => {
            invalid_archive()
        }
        _ => storage_failure(),
    }
}

/// Background write scheduling and admission bounds for one open save.
///
/// These are host resource policies, not simulation clocks. A write becomes due
/// at the maximum age, at 75% queue capacity, after the target age and quiet
/// interval, or on an explicit save request. A due write is not yet durable.
/// [`Engine::open_with_policy`](crate::Engine::open_with_policy) validates the
/// policy before opening the save.
#[derive(Clone, Debug)]
pub struct SavePolicy {
    /// Accepted journal entries between periodic checkpoint captures; at most
    /// 1,000,000. Zero disables periodic captures, not pre-publication capacity
    /// checks required when chart history grows.
    pub checkpoint_interval: u64,
    /// Positive age of the oldest unsaved entry before a quiet-time write is due.
    pub target_interval: Duration,
    /// Maximum age before a write is due and session backpressure applies.
    /// Must be at least `target_interval` and at most one day. This is not an
    /// upper bound on completion time for slow or failed storage.
    pub max_unsaved_age: Duration,
    /// Quiet time since the last admission needed for a target-age write.
    /// May be zero; must not exceed `max_unsaved_age`.
    pub idle_interval: Duration,
    /// Encoded admission budget, including pending and in-flight journal,
    /// attachment and copied region bytes; between one byte and 1 GiB.
    ///
    /// The record and attachment must fit. Copied region inputs/rows may exceed
    /// this budget only when the queue is empty, so a large first acquisition can
    /// make progress. Checkpoints and other runtime allocations are separate;
    /// this is not a bound on total process memory.
    pub max_pending_bytes: usize,
}
impl Default for SavePolicy {
    fn default() -> Self {
        Self {
            checkpoint_interval: 1024,
            target_interval: Duration::from_secs(30),
            max_unsaved_age: Duration::from_secs(60),
            idle_interval: Duration::from_millis(750),
            // One maximum PNG plus its bounded journal envelope must fit.
            max_pending_bytes: 16 * MAX_PAYLOAD,
        }
    }
}
impl SavePolicy {
    /// Check the field relationships and supported bounds without opening a save.
    /// Returns [`ErrorCode::InvalidRequest`](tor_protocol::ErrorCode::InvalidRequest)
    /// for an invalid policy.
    pub fn validate(&self) -> Result<(), Failure> {
        if self.checkpoint_interval > 1_000_000
            || self.target_interval.is_zero()
            || self.max_unsaved_age < self.target_interval
            || self.max_unsaved_age > Duration::from_secs(86400)
            || self.max_pending_bytes == 0
            || self.max_pending_bytes > 1024 * MAX_PAYLOAD
            || self.idle_interval > self.max_unsaved_age
        {
            return Err(Failure::new(
                tor_protocol::ErrorCode::InvalidRequest,
                "Invalid background save policy",
            ));
        }
        Ok(())
    }
    fn due(&self, age: Duration, idle: Duration, bytes: usize, forced: bool) -> bool {
        forced
            || age >= self.max_unsaved_age
            || bytes >= self.max_pending_bytes.saturating_mul(3) / 4
            || (age >= self.target_interval && idle >= self.idle_interval)
    }
}
/// A point-in-time view of one save writer, not a durability promise for future work.
///
/// A requested sequence is durable once `durable_sequence` reaches it. Accepted
/// work above that sequence can still be lost after a crash. Counters and timings
/// describe this open writer; they are diagnostics, not persisted gameplay state.
/// An engine without a save returns the default, all-zero status.
#[derive(Clone, Debug, Default, Serialize)]
pub struct SaveStatus {
    /// Sequence covered by the latest committed checkpoint, or zero without one.
    pub checkpoint_sequence: u64,
    /// Checkpoints successfully written by this writer since opening.
    pub checkpoints: u64,
    /// Encoded bytes of the latest written checkpoint and its region rows.
    pub checkpoint_bytes: u64,
    /// Encoding time of that checkpoint and its region rows, in milliseconds.
    pub last_checkpoint_ms: u64,
    /// Highest sequence accepted into this save's in-memory queue.
    pub accepted_sequence: u64,
    /// Highest sequence in the successfully committed database prefix.
    pub durable_sequence: u64,
    /// Encoded admission bytes not yet committed, including the in-flight batch.
    pub pending_bytes: usize,
    /// Age of the oldest uncommitted admission in milliseconds, or zero if empty.
    pub unsaved_age_ms: u64,
    /// Whether the worker currently owns a batch for encoding and writing.
    pub saving: bool,
    /// Whether the oldest uncommitted admission reached the maximum age.
    pub overdue: bool,
    /// Latest checkpoint omitted because it exceeded the encoder capacity.
    /// Journal durability continues independently; a later checkpoint clears it.
    pub checkpoint_deferred_sequence: Option<u64>,
    /// Retained background failure. New mutations are rejected while present.
    /// An explicit save retries journal/serialization failures. Capacity-only
    /// checkpoint deferrals are reported separately and do not block admission.
    pub error: Option<String>,
    /// Successful background batches since opening this writer.
    pub batches: u64,
    /// Committed journal-frame bytes since opening, excluding copied sources,
    /// region rows and attachments. Not physical filesystem writes.
    pub journal_bytes: u64,
    /// Total elapsed time of the last successful batch, in milliseconds.
    pub last_batch_ms: u64,
    /// SQLite commit time for the last successful batch, within last_batch_ms.
    pub last_commit_ms: u64,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope<T> {
    save_id: String,
    generation: u64,
    record: T,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Marker {
    save_id: String,
    generation: u64,
    wizard_game: bool,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Base {
    save_id: String,
    generation: u64,
    archive: Archive,
}

/// Region record rows a checkpoint's transaction writes, and the rows it
/// keeps; it deletes every other row.
pub(crate) struct RegionWrite {
    rows: Vec<(u64, Vec<u8>)>,
    keep: BTreeSet<u64>,
}
impl RegionWrite {
    fn encode(checkpoint: &Checkpoint) -> Result<Self, Failure> {
        Ok(Self {
            rows: checkpoint
                .records
                .iter()
                .map(|(id, record)| Ok((id.0, region_frame(id.0, record)?)))
                .collect::<Result<_, Failure>>()?,
            keep: checkpoint.referenced().into_iter().map(|id| id.0).collect(),
        })
    }
}
fn read_region_row(
    conn: &Connection,
    id: tor_simulation::RecordId,
    content: Arc<tor_simulation::creatures::Content>,
) -> Result<Option<tor_simulation::RegionRecord>, Failure> {
    let bytes: Option<Vec<u8>> = conn
        .query_row(
            "SELECT CASE WHEN typeof(frame)='blob' AND octet_length(frame)<=?2 THEN frame END FROM regions WHERE record=?1",
            params![id.0 as i64, (codec::MAX_REGION + 24) as i64],
            |r| copy_saved_blob(r, 0, codec::MAX_REGION + 24),
        )
        .optional()
        .map_err(saved_row_failure)?;
    bytes
        .map(|bytes| decode_region(&bytes, id.0, content))
        .transpose()
}
/// Test witness for the exact current detached record; uses the production
/// bounded framing/CRC/schema/content decoder, never an alternate JSON reader.
#[cfg(test)]
pub(crate) fn test_read_region_record(
    path: &Path,
    id: tor_simulation::RecordId,
    content: Arc<tor_simulation::creatures::Content>,
) -> Result<Option<tor_simulation::RegionRecord>, Failure> {
    let conn = Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(saved_row_failure)?;
    read_region_row(&conn, id, content)
}

/// Authority for admitted authored inputs, shared without retaining the writer
/// or its checkpoints. Pending bytes are the same Arcs charged to queue capacity
/// and survive worker batch draining. A failed write leaves accepted live inputs
/// readable as pending; SaveStatus still reports failure and blocks admission.
#[derive(Debug, Default)]
struct InputOwnership {
    known: BTreeSet<u64>,
    pending: BTreeMap<u64, (u64, Arc<str>)>,
}
impl InputOwnership {
    fn admit(&mut self, sequence: u64, sources: &[(u64, Arc<str>)]) {
        for (id, input) in sources {
            self.known.insert(*id);
            self.pending.insert(*id, (sequence, input.clone()));
        }
    }
    fn committed(&mut self, sequence: u64, sources: &[(u64, Arc<str>)]) {
        for (id, _) in sources {
            // A later admission of this ID may still be queued or in flight.
            if self
                .pending
                .get(id)
                .is_some_and(|(owner, _)| *owner == sequence)
            {
                self.pending.remove(id);
            }
        }
    }
}

/// Reads admitted immutable input bytes, including pending and in-flight copies.
/// Only IDs never captured by this save may fall back to the compiled artifact.
/// Durable bytes are read with a separate connection and checked by the caller.
#[derive(Debug)]
pub(crate) struct SourceReader {
    path: PathBuf,
    conn: Option<Connection>,
    inputs: Arc<Mutex<InputOwnership>>,
}
impl SourceReader {
    pub(crate) fn independent(&self) -> Self {
        Self {
            path: self.path.clone(),
            conn: None,
            inputs: self.inputs.clone(),
        }
    }
    pub(crate) fn read(&mut self, region: u64) -> Result<Option<Arc<str>>, Failure> {
        {
            let inputs = self.inputs.lock().unwrap();
            if let Some((_, input)) = inputs.pending.get(&region) {
                return Ok(Some(input.clone()));
            }
            if !inputs.known.contains(&region) {
                return Ok(None);
            }
        }
        // Never hold input ownership while waiting for SQLite. A known ID
        // without pending bytes has committed, so absence is corruption.
        if self.conn.is_none() {
            self.conn = Some(connection(&self.path)?);
        }
        let input: Option<String> = self.conn
            .as_ref()
            .unwrap()
            .query_row(
                "SELECT CASE WHEN typeof(source)='text' AND octet_length(source)<=?2 THEN source END FROM region_sources WHERE region=?1",
                params![region as i64, crate::scenario_package::MAX_REGION_BYTES as i64],
                |r| copy_saved_text(r, 0),
            )
            .optional()
            .map_err(saved_row_failure)?;
        input.map(|input| Some(Arc::from(input))).ok_or_else(|| {
            Failure::new(
                tor_protocol::ErrorCode::StorageFailure,
                &format!("The save's compiled region {region} is missing"),
            )
        })
    }
}
/// Reads region rows on a connection of its own, opened on first use.
#[derive(Debug)]
pub(crate) struct RegionReader {
    content: Arc<tor_simulation::creatures::Content>,
    path: PathBuf,
    conn: Option<Connection>,
}
impl RegionReader {
    pub(crate) fn read(
        &mut self,
        id: tor_simulation::RecordId,
    ) -> Result<Option<tor_simulation::RegionRecord>, Failure> {
        if self.conn.is_none() {
            self.conn = Some(connection(&self.path)?);
        }
        read_region_row(self.conn.as_ref().unwrap(), id, self.content.clone())
    }
}
fn connection(path: &Path) -> Result<Connection, Failure> {
    let conn = Connection::open(path).map_err(|_| storage_failure())?;
    conn.busy_timeout(Duration::from_secs(2))
        .map_err(|_| storage_failure())?;
    conn.execute_batch(
        "PRAGMA trusted_schema=OFF; PRAGMA journal_mode=DELETE; PRAGMA synchronous=EXTRA;",
    )
    .map_err(|_| storage_failure())?;
    Ok(conn)
}
/// Copy compiled region inputs into the save. Each is written once; a retry must
/// match it exactly.
fn write_sources(tx: &Connection, sources: &[(u64, Arc<str>)]) -> Result<(), Failure> {
    for (region, text) in sources {
        let existing: Option<bool> = tx
            .query_row(
                "SELECT CASE WHEN typeof(source)='text' AND octet_length(source)<=?2 THEN source END FROM region_sources WHERE region=?1",
                params![*region as i64, crate::scenario_package::MAX_REGION_BYTES as i64],
                |r| Ok(saved_text(r, 0)? == text.as_ref()),
            )
            .optional()
            .map_err(saved_row_failure)?;
        match existing {
            Some(true) => {}
            Some(_) => return Err(invalid_archive()),
            None => {
                tx.execute(
                    "INSERT INTO region_sources(region,source) VALUES (?1,?2)",
                    params![*region as i64, &**text],
                )
                .map_err(|_| storage_failure())?;
            }
        }
    }
    Ok(())
}
/// Whether a record of `record` bytes, copying region files of `copied`
/// bytes, may join a queue already holding `pending`. The record itself must
/// fit. The files it copies may take it over the limit only in an empty
/// queue, which saving then drains at once: otherwise a command whose build
/// copies many large region files could never be saved.
fn fits_queue(pending: usize, record: usize, copied: usize, max: usize) -> bool {
    let with_record = pending.saturating_add(record);
    with_record <= max && (pending == 0 || with_record.saturating_add(copied) <= max)
}
/// The package index is kept in chunks under the frame limit.
const PACKAGE_CHUNK: usize = 512 * 1024;
fn commit_batch(
    conn: &mut Connection,
    entries: &[Pending],
    checkpoint: Option<(u64, &[u8])>,
    regions: Option<&RegionWrite>,
    mut fault: impl FnMut(&Connection, &str) -> Result<(), Failure>,
) -> Result<(), Failure> {
    let tx = conn.transaction().map_err(|_| storage_failure())?;
    for entry in entries {
        attachments::write(&tx, entry.attachment.as_ref())?;
        write_sources(&tx, &entry.sources)?;
        write_region_rows(&tx, &entry.regions)?;
    }
    if entries.iter().any(|entry| !entry.regions.is_empty()) {
        fault(&tx, "after_generated_regions")?;
    }
    if entries.iter().any(|e| !e.sources.is_empty()) {
        fault(&tx, "after_sources")?;
    }
    for entry in entries {
        // Retrying an uncertain commit reconciles exact stored bytes. A conflicting
        // sequence is never overwritten and no request can execute twice.
        let existing: Option<bool> = tx
            .query_row(
                "SELECT CASE WHEN typeof(frame)='blob' AND octet_length(frame)<=?2 THEN frame END FROM journal WHERE sequence=?1
                 UNION ALL SELECT CASE WHEN typeof(frame)='blob' AND octet_length(frame)<=?2 THEN frame END FROM history WHERE sequence=?1",
                params![entry.sequence as i64, (MAX_PAYLOAD + 24) as i64],
                |r| Ok(saved_blob(r, 0, MAX_PAYLOAD + 24)? == entry.bytes.as_slice()),
            )
            .optional()
            .map_err(saved_row_failure)?;
        match existing {
            Some(true) => {}
            Some(_) => return Err(invalid_archive()),
            None => {
                tx.execute(
                    "INSERT INTO journal(sequence,frame) VALUES (?1,?2)",
                    params![entry.sequence as i64, entry.bytes],
                )
                .map_err(|_| storage_failure())?;
            }
        }
    }
    fault(&tx, "after_append")?;
    if let Some((sequence, bytes)) = checkpoint {
        // Rows are written before the checkpoint naming them, in the same
        // transaction. A row is never rewritten: a retry must match it exactly.
        write_region_rows(&tx, regions.map(|r| r.rows.as_slice()).unwrap_or_default())?;
        fault(&tx, "after_regions")?;
        let current: Option<(i64, bool)> = tx
            .query_row(
                "SELECT sequence,CASE WHEN typeof(payload)='blob' AND octet_length(payload)<=?1 THEN payload END FROM checkpoint WHERE slot=1",
                [MAX_CHECKPOINT as i64],
                |r| Ok((r.get(0)?, saved_blob(r, 1, MAX_CHECKPOINT)? == bytes)),
            )
            .optional()
            .map_err(saved_row_failure)?;
        if current
            .as_ref()
            .is_some_and(|(seq, same)| *seq > sequence as i64 || (*seq == sequence as i64 && !same))
        {
            return Err(invalid_archive());
        }
        tx.execute(
            "INSERT OR REPLACE INTO checkpoint VALUES (1,?1,?2,?3)",
            params![
                sequence as i64,
                bytes,
                i64::from(crc32c(bytes.iter().copied()))
            ],
        )
        .map_err(|_| storage_failure())?;
        fault(&tx, "after_checkpoint")?;
        tx.execute(
            "INSERT INTO history SELECT * FROM journal WHERE sequence>0 AND sequence<=?1",
            [sequence as i64],
        )
        .map_err(|_| storage_failure())?;
        fault(&tx, "after_history")?;
        tx.execute(
            "DELETE FROM journal WHERE sequence>0 AND sequence<=?1",
            [sequence as i64],
        )
        .map_err(|_| storage_failure())?;
        fault(&tx, "after_rotation")?;
        // Delete rows the new checkpoint doesn't refer to. Record identities
        // are never reused, so nothing will refer to them again.
        let keep = regions.map(|r| &r.keep);
        let stored: Vec<i64> = tx
            .prepare("SELECT record FROM regions")
            .and_then(|mut statement| {
                statement
                    .query_map([], |r| r.get(0))?
                    .collect::<Result<_, _>>()
            })
            .map_err(|_| storage_failure())?;
        for id in stored {
            if !keep.is_some_and(|keep| keep.contains(&(id as u64)))
                && !entries
                    .iter()
                    .flat_map(|entry| &entry.regions)
                    .any(|(record, _)| *record == id as u64)
            {
                tx.execute("DELETE FROM regions WHERE record=?1", [id])
                    .map_err(|_| storage_failure())?;
            }
        }
        fault(&tx, "after_gc")?;
    }
    fault(&tx, "before_commit")?;
    tx.commit().map_err(|_| storage_failure())?;
    fault(conn, "after_commit")
}

/// Immutable rows use exact retry reconciliation, including journal-only
/// transactions that publish a generated batch before the next checkpoint.
fn write_region_rows(conn: &Connection, rows: &[(u64, Vec<u8>)]) -> Result<(), Failure> {
    for (id, frame) in rows {
        if *id == 0 || *id > i64::MAX as u64 {
            return Err(invalid_archive());
        }
        let existing: Option<bool> = conn.query_row(
            "SELECT CASE WHEN typeof(frame)='blob' AND octet_length(frame)<=?2 THEN frame END FROM regions WHERE record=?1",
            params![*id as i64, (codec::MAX_REGION + 24) as i64],
            |row| Ok(saved_blob(row,0,codec::MAX_REGION+24)? == frame.as_slice()),
        ).optional().map_err(saved_row_failure)?;
        match existing {
            Some(true) => {}
            Some(false) => return Err(invalid_archive()),
            None => {
                conn.execute(
                    "INSERT INTO regions(record,frame) VALUES (?1,?2)",
                    params![*id as i64, frame],
                )
                .map_err(|_| storage_failure())?;
            }
        }
    }
    Ok(())
}
use rusqlite::OptionalExtension;

#[cfg(test)]
fn encode_checkpoint(checkpoint: &DiskCheckpoint) -> Result<Vec<u8>, Failure> {
    if !checkpoint
        .record_expansion_within_limit()
        .map_err(|_| storage_failure())?
    {
        return Err(storage_failure());
    }
    checkpoint_codec::write_json(checkpoint, Vec::new()).map_err(|_| storage_failure())
}

fn read_checkpoint(conn: &Connection) -> Result<Option<DiskCheckpoint>, Failure> {
    let count: i64 = conn
        .query_row("SELECT count(*) FROM checkpoint", [], |r| r.get(0))
        .map_err(|_| invalid_archive())?;
    if count == 0 {
        return Ok(None);
    }
    if count != 1 {
        return Err(invalid_archive());
    }
    let (sequence, bytes, checksum): (i64, Vec<u8>, u32) = conn
        .query_row(
            "SELECT sequence,CASE WHEN typeof(payload)='blob' AND octet_length(payload)<=?1 THEN payload END,checksum FROM checkpoint WHERE slot=1",
            [MAX_CHECKPOINT as i64],
            |r| Ok((r.get(0)?, copy_saved_blob(r, 1, MAX_CHECKPOINT)?, r.get(2)?)),
        )
        .map_err(|_| invalid_archive())?;
    if sequence <= 0 || bytes.is_empty() {
        return Err(invalid_archive());
    }
    if crc32c(bytes.iter().copied()) != checksum {
        return Err(invalid_archive());
    }
    let decoded = checkpoint_codec::decode(&bytes).map_err(|_| invalid_archive())?;
    let checkpoint: DiskCheckpoint = strict(&decoded)?;
    if checkpoint.sequence != sequence as u64 {
        return Err(invalid_archive());
    }
    Ok(Some(checkpoint))
}

/// Read the current-format save's archive as JSON for trusted offline diagnostics.
///
/// This checks the storage frames and selected checkpoint while loading retained
/// records. It does not run the engine's deterministic replay or eagerly read
/// every detached region. Use [`Engine::open`](crate::Engine::open) for gameplay
/// recovery. The physical frame version is independent of the logical save format.
pub fn inspect_save(path: impl AsRef<Path>) -> Result<serde_json::Value, Failure> {
    let (_, archive, _, _, _, _) = load(path.as_ref())?;
    serde_json::to_value(archive).map_err(|_| invalid_archive())
}
type Loaded = (
    Connection,
    Archive,
    String,
    u64,
    Option<DiskCheckpoint>,
    BTreeSet<u64>,
);
fn load(path: &Path) -> Result<Loaded, Failure> {
    // Refuse JSON and other formats without letting SQLite change the input.
    let mut file = std::fs::File::open(path).map_err(|_| storage_failure())?;
    let mut header = [0u8; 18];
    std::io::Read::read_exact(&mut file, &mut header).map_err(|_| invalid_archive())?;
    if &header[..16] != b"SQLite format 3\0" {
        return Err(invalid_archive());
    }
    let page_size = match u16::from_be_bytes([header[16], header[17]]) {
        1 => 65536u64,
        size => u64::from(size),
    };
    if !(512..=65536).contains(&page_size) || !page_size.is_power_of_two() {
        return Err(invalid_archive());
    }
    let conn = connection(path).map_err(|_| invalid_archive())?;
    let version: i64 = conn
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .map_err(|_| invalid_archive())?;
    let app: i64 = conn
        .query_row("PRAGMA application_id", [], |r| r.get(0))
        .map_err(|_| invalid_archive())?;
    if app == APP_ID && version != SAVE_FORMAT {
        return Err(Failure::new(
            tor_protocol::ErrorCode::InvalidArchive,
            &format!(
                "Unsupported or inconsistent game journal: save format {version}; this build requires save format {SAVE_FORMAT}."
            ),
        ));
    }
    // Region record rows aren't checked here, so opening a save doesn't read
    // the whole world; each row's checksum is checked when it's read.
    let integrity = [
        "journal",
        "history",
        "checkpoint",
        "package",
        "region_sources",
        "attachments",
    ]
    .iter()
    .map(|table| {
        conn.query_row(&format!("PRAGMA quick_check({table})"), [], |r| {
            r.get::<_, String>(0)
        })
    })
    .collect::<Result<Vec<_>, _>>()
    .map_err(|_| invalid_archive())?;
    // SQLite must get the first opportunity to recover a hot rollback journal,
    // including a partially extended database page from an interrupted write.
    if app != APP_ID
        || integrity.iter().any(|result| result != "ok")
        || file.metadata().map_err(|_| storage_failure())?.len() % page_size != 0
    {
        return Err(invalid_archive());
    }
    attachments::preflight_saved_size(&conn)?;
    let checkpoint = read_checkpoint(&conn)?;
    let checkpoint_sequence = checkpoint.as_ref().map(|c| c.sequence).unwrap_or(0);
    let mut checkpoint_records = 0;
    let mut statement = conn
        .prepare("SELECT sequence,octet_length(frame),CASE WHEN typeof(frame)='blob' AND octet_length(frame)<=?1 THEN frame END,0 FROM journal
                  UNION ALL SELECT sequence,octet_length(frame),CASE WHEN typeof(frame)='blob' AND octet_length(frame)<=?1 THEN frame END,1 FROM history ORDER BY sequence")
        .map_err(|_| invalid_archive())?;
    let mut rows = statement
        .query([(MAX_PAYLOAD + 24) as i64])
        .map_err(|_| invalid_archive())?;
    let mut archive = None;
    let mut save_id = String::new();
    let mut next = 0u64;
    let mut attachment_bytes = 0usize;
    while let Some(row) = rows.next().map_err(|_| invalid_archive())? {
        let sequence: u64 = u64::try_from(row.get::<_, i64>(0).map_err(|_| invalid_archive())?)
            .map_err(|_| invalid_archive())?;
        let size: usize = usize::try_from(row.get::<_, i64>(1).map_err(|_| invalid_archive())?)
            .map_err(|_| invalid_archive())?;
        if sequence != next || !(24..=MAX_PAYLOAD + 24).contains(&size) {
            return Err(invalid_archive());
        }
        let retained: bool = row.get(3).map_err(|_| invalid_archive())?;
        if retained != (sequence > 0 && sequence <= checkpoint_sequence) {
            return Err(invalid_archive());
        }
        let bytes = copy_saved_blob(row, 2, MAX_PAYLOAD + 24).map_err(|_| invalid_archive())?;
        let (kind, payload) = decode(&bytes, sequence)?;
        match kind {
            0 => {
                let base: Base = strict(payload)?;
                if base.generation != 0
                    || uuid::Uuid::parse_str(&base.save_id).is_err()
                    || !base.archive.records.is_empty()
                {
                    return Err(invalid_archive());
                }
                save_id = base.save_id;
                archive = Some(base.archive);
            }
            1 => {
                let mut envelope: Envelope<Record> = strict(payload)?;
                attachments::account_declared(&envelope.record, &mut attachment_bytes)?;
                attachments::hydrate(&conn, &mut envelope.record)?;
                if envelope.save_id != save_id || envelope.generation != 0 {
                    return Err(invalid_archive());
                }
                let a = archive.as_mut().ok_or_else(invalid_archive)?;
                if matches!(
                    envelope.record.entry.content,
                    crate::journal::JournalContent::Wizard { .. }
                ) && !a.wizard_game
                {
                    return Err(invalid_archive());
                }
                a.records.push(envelope.record);
                if sequence <= checkpoint_sequence {
                    checkpoint_records += 1;
                }
            }
            2 => {
                let marker: Marker = strict(payload)?;
                if marker.save_id != save_id || marker.generation != 0 || !marker.wizard_game {
                    return Err(invalid_archive());
                }
                archive.as_mut().ok_or_else(invalid_archive)?.wizard_game = true;
            }
            _ => return Err(invalid_archive()),
        }
        next = next.checked_add(1).ok_or_else(invalid_archive)?;
    }
    drop(rows);
    drop(statement);
    if checkpoint.as_ref().is_some_and(|c| {
        c.save_id != save_id
            || c.sequence >= next
            || c.sequence == 0
            || c.record_count != checkpoint_records
    }) {
        return Err(invalid_archive());
    }
    let mut archive = archive.ok_or_else(invalid_archive)?;
    attachments::validate_inventory(&conn, &archive.records)?;
    let saved = attach_package(&conn, &mut archive)?;
    Ok((conn, archive, save_id, next - 1, checkpoint, saved))
}

/// Give a loaded archive's package its index, from the `package` table, and
/// the region files copied into the save. Returns the copied regions.
fn read_package_chunks(conn: &Connection) -> Result<(usize, Vec<u8>), Failure> {
    let limit = crate::scenario_package::MAX_INDEX_BYTES as usize;
    let mut statement = conn
        .prepare("SELECT chunk,CASE WHEN typeof(bytes)='blob' AND octet_length(bytes)<=?1 THEN bytes END FROM package ORDER BY chunk LIMIT ?2")
        .map_err(|_| invalid_archive())?;
    let mut rows = statement
        .query(params![
            PACKAGE_CHUNK as i64,
            (limit.div_ceil(PACKAGE_CHUNK) + 1) as i64
        ])
        .map_err(|_| invalid_archive())?;
    let mut count = 0usize;
    let mut bytes = Vec::new();
    while let Some(row) = rows.next().map_err(|_| invalid_archive())? {
        let chunk: i64 = row.get(0).map_err(|_| invalid_archive())?;
        if chunk != count as i64 || count >= limit.div_ceil(PACKAGE_CHUNK) {
            return Err(invalid_archive());
        }
        let payload = saved_blob(row, 1, PACKAGE_CHUNK.min(limit - bytes.len()))
            .map_err(|_| invalid_archive())?;
        if payload.is_empty() {
            return Err(invalid_archive());
        }
        bytes.extend_from_slice(payload);
        #[cfg(test)]
        SAVED_BYTES_COPIED.with(|copied| copied.set(copied.get() + payload.len()));
        count += 1;
    }
    Ok((count, bytes))
}

fn attach_package(conn: &Connection, archive: &mut Archive) -> Result<BTreeSet<u64>, Failure> {
    let (chunks, bytes) = read_package_chunks(conn)?;
    let Some(package) = &mut archive.scenario.package else {
        let has_sources: bool = conn
            .query_row("SELECT EXISTS(SELECT 1 FROM region_sources)", [], |row| {
                row.get(0)
            })
            .map_err(|_| invalid_archive())?;
        return if chunks == 0 && !has_sources {
            Ok(BTreeSet::new())
        } else {
            Err(invalid_archive())
        };
    };
    let index = schema::scenario::decode_index(&bytes)?;
    // Only identities are read here; source text stays lazy. Validate each
    // identity before inserting rather than collecting an unbounded list.
    let mut sources = BTreeSet::new();
    let mut statement = conn
        .prepare("SELECT region FROM region_sources ORDER BY region LIMIT ?1")
        .map_err(|_| invalid_archive())?;
    let mut rows = statement
        .query([(crate::scenario_package::MAX_REGIONS + 1) as i64])
        .map_err(|_| invalid_archive())?;
    while let Some(row) = rows.next().map_err(|_| invalid_archive())? {
        let region = u64::try_from(row.get::<_, i64>(0).map_err(|_| invalid_archive())?)
            .map_err(|_| invalid_archive())?;
        if sources.len() >= crate::scenario_package::MAX_REGIONS
            || index.region(region).is_none()
            || !sources.insert(region)
        {
            return Err(invalid_archive());
        }
    }
    let package = Arc::make_mut(package);
    package.index = Arc::new(index);
    package.sources = Default::default();
    Ok(sources)
}

#[derive(Debug)]
struct Pending {
    attachment: Option<AttachmentWrite>,
    regions: Vec<(u64, Vec<u8>)>,
    sequence: u64,
    bytes: Vec<u8>,
    /// Region files the record's command built regions from, not yet in
    /// the save; written in the same transaction as the record.
    sources: Vec<(u64, Arc<str>)>,
    queued: Instant,
}
/// Only an explicit encoder capacity bound defers optional checkpointing.
/// Other serialization, region and commit failures retain the existing retry path.
#[derive(Clone, Copy, Debug)]
enum SaveFailure {
    CheckpointCapacity { sequence: u64 },
    Retryable,
}

#[cfg(test)]
#[path = "storage_checkpoint_failure_tests.rs"]
mod checkpoint_failure_tests;

fn classify_record_expansion(
    within_limit: Result<bool, tor_simulation::checkpoint::PersistError>,
    sequence: u64,
) -> Result<(), SaveFailure> {
    match within_limit {
        Ok(true) => Ok(()),
        Ok(false) => Err(SaveFailure::CheckpointCapacity { sequence }),
        Err(_) => Err(SaveFailure::Retryable),
    }
}

fn encode_background_checkpoint(
    _shared: &Shared,
    sequence: u64,
    checkpoint: &DiskCheckpoint,
) -> Result<Vec<u8>, SaveFailure> {
    classify_record_expansion(checkpoint.record_expansion_within_limit(), sequence)?;
    #[cfg(test)]
    let encoded = checkpoint_failure_tests::encode(_shared, checkpoint)?;
    #[cfg(not(test))]
    let encoded = match crate::process_diagnostics::checkpoint_byte_limit()
        .map_err(|_| SaveFailure::Retryable)?
    {
        Some(limit) => {
            checkpoint_codec::write_json_classified_with_limit(checkpoint, Vec::new(), limit)
        }
        None => checkpoint_codec::write_json_classified(checkpoint, Vec::new()),
    };
    encoded.map_err(|error| match error {
        checkpoint_codec::EncodeError::CapacityExceeded => {
            SaveFailure::CheckpointCapacity { sequence }
        }
        checkpoint_codec::EncodeError::Other(_) => SaveFailure::Retryable,
    })
}

#[derive(Debug)]
struct State {
    appended: BTreeSet<tor_simulation::RecordId>,
    pending: VecDeque<Pending>,
    checkpoint: Option<(u64, Checkpoint)>,
    last_checkpoint_requested: u64,
    status: SaveStatus,
    oldest: Option<Instant>,
    last_activity: Instant,
    force: u64,
    closing: bool,
    /// After a checkpoint commits: the records on disk, and the identity
    /// watermark at its capture. Taken by the engine.
    written: Option<(BTreeSet<tor_simulation::RecordId>, tor_simulation::RecordId)>,
}
#[derive(Debug)]
struct Shared {
    state: Mutex<State>,
    #[cfg(test)]
    checkpoint_test: checkpoint_failure_tests::CheckpointTest,
    inputs: Arc<Mutex<InputOwnership>>,
    wake: Condvar,
    policy: SavePolicy,
    save_id: String,
}
#[derive(Debug)]
struct Owner {
    content: Arc<tor_simulation::creatures::Content>,
    shared: Arc<Shared>,
    /// Serializes producers without blocking worker progress or status readers.
    admission: Mutex<()>,
    thread: Mutex<Option<JoinHandle<()>>>,
    save_id: String,
    path: PathBuf,
    reader: Mutex<Option<Connection>>,
}
#[derive(Clone, Debug)]
pub(crate) struct Store(Arc<Owner>);
impl Store {
    pub(crate) fn save_id(&self) -> &str {
        &self.0.save_id
    }
    /// Open a save, or create it from `initial`: the archive, and the region
    /// files its regions so far were built from. Also returns the regions
    /// whose files the save holds.
    pub(crate) fn open(
        path: &Path,
        initial: impl FnOnce() -> Result<(Archive, Vec<(u64, Arc<str>)>, Vec<(u64, Vec<u8>)>), Failure>,
        policy: SavePolicy,
        lock: Arc<JournalLock>,
    ) -> Result<(Self, Archive, Option<DiskCheckpoint>, BTreeSet<u64>), Failure> {
        policy.validate()?;
        let (conn, archive, save_id, sequence, checkpoint, saved) = if path.exists() {
            load(path)?
        } else {
            let (initial, sources, records) = initial()?;
            attachments::preflight_initial(&initial.records)?;
            // Every creation route, including detached profiling fixtures, pins
            // the same immutable input closure before the database is opened.
            let sources = if let Some(package) = &initial.scenario.package {
                package.initial_region_inputs(sources)?
            } else {
                sources
            };
            let mut conn = connection(path)?;
            let save_id = uuid::Uuid::new_v4().to_string();
            let mut base = initial.clone();
            base.records.clear();
            let bytes = frame(
                0,
                0,
                &Base {
                    save_id: save_id.clone(),
                    generation: 0,
                    archive: base,
                },
            )?;
            let tx = conn.transaction().map_err(|_| storage_failure())?;
            tx.execute_batch(&format!("PRAGMA application_id=1414484554; PRAGMA user_version={SAVE_FORMAT}; CREATE TABLE journal(sequence INTEGER PRIMARY KEY,frame BLOB NOT NULL) STRICT; CREATE TABLE history(sequence INTEGER PRIMARY KEY,frame BLOB NOT NULL) STRICT; CREATE TABLE checkpoint(slot INTEGER PRIMARY KEY CHECK(slot=1), sequence INTEGER NOT NULL, payload BLOB NOT NULL, checksum INTEGER NOT NULL) STRICT; CREATE TABLE regions(record INTEGER PRIMARY KEY,frame BLOB NOT NULL) STRICT; CREATE TABLE package(chunk INTEGER PRIMARY KEY,bytes BLOB NOT NULL) STRICT; CREATE TABLE region_sources(region INTEGER PRIMARY KEY,source TEXT NOT NULL) STRICT; CREATE TABLE attachments(entry TEXT PRIMARY KEY,bytes BLOB NOT NULL) STRICT;")).map_err(|_| storage_failure())?;
            tx.execute("INSERT INTO journal VALUES (0,?1)", [bytes])
                .map_err(|_| storage_failure())?;
            if let Some(package) = &initial.scenario.package {
                for (chunk, bytes) in schema::scenario::encode_index(&package.index)?
                    .chunks(PACKAGE_CHUNK)
                    .enumerate()
                {
                    tx.execute(
                        "INSERT INTO package VALUES (?1,?2)",
                        params![chunk as i64, bytes],
                    )
                    .map_err(|_| storage_failure())?;
                }
            }
            write_sources(&tx, &sources)?;
            write_region_rows(&tx, &records)?;
            for (index, record) in initial.records.iter().enumerate() {
                let attachment = attachments::from_record(record)?;
                attachments::write(&tx, attachment.as_ref())?;
                let seq = index as u64 + 1;
                let bytes = frame(
                    1,
                    seq,
                    &Envelope {
                        save_id: save_id.clone(),
                        generation: 0,
                        record,
                    },
                )?;
                tx.execute(
                    "INSERT INTO journal VALUES (?1,?2)",
                    params![seq as i64, bytes],
                )
                .map_err(|_| storage_failure())?;
            }
            tx.commit().map_err(|_| storage_failure())?;
            let sequence = initial.records.len() as u64;
            let saved = sources.iter().map(|(region, _)| *region).collect();
            (conn, initial, save_id, sequence, None, saved)
        };
        let checkpoint_sequence = checkpoint.as_ref().map(|c| c.sequence).unwrap_or(0);
        let appended = conn
            .prepare("SELECT record FROM regions ORDER BY record")
            .and_then(|mut statement| {
                statement
                    .query_map([], |row| row.get::<_, i64>(0))?
                    .collect::<Result<Vec<_>, _>>()
            })
            .map_err(saved_row_failure)?
            .into_iter()
            .map(|id| {
                if id > 0 {
                    Ok(tor_simulation::RecordId(id as u64))
                } else {
                    Err(invalid_archive())
                }
            })
            .collect::<Result<_, Failure>>()?;
        let content = archive.scenario.content()?;
        let shared = Arc::new(Shared {
            #[cfg(test)]
            checkpoint_test: checkpoint_failure_tests::CheckpointTest::default(),
            inputs: Arc::new(Mutex::new(InputOwnership {
                known: saved.clone(),
                pending: BTreeMap::new(),
            })),
            save_id: save_id.clone(),
            policy,
            wake: Condvar::new(),
            state: Mutex::new(State {
                appended,
                pending: VecDeque::new(),
                checkpoint: None,
                last_checkpoint_requested: checkpoint_sequence,
                status: SaveStatus {
                    checkpoint_sequence,
                    accepted_sequence: sequence,
                    durable_sequence: sequence,
                    ..SaveStatus::default()
                },
                oldest: None,
                last_activity: Instant::now(),
                force: sequence,
                closing: false,
                written: None,
            }),
        });
        let worker = shared.clone();
        let owned = path.to_owned();
        let path = path.to_owned();
        let thread = std::thread::Builder::new()
            .name("save-journal".into())
            .spawn(move || {
                let _lock = lock;
                run_worker(worker, conn, path);
            })
            .map_err(|_| storage_failure())?;
        Ok((
            Self(Arc::new(Owner {
                content,
                shared,
                admission: Mutex::new(()),
                thread: Mutex::new(Some(thread)),
                save_id,
                path: owned,
                reader: Mutex::new(None),
            })),
            archive,
            checkpoint,
            saved,
        ))
    }
    pub(crate) fn enqueue_regions(
        &self,
        record: &Record,
        sources: Vec<(u64, Arc<str>)>,
        made: &[(
            tor_simulation::RecordId,
            tor_world::Shared<tor_simulation::RegionRecord>,
        )],
        admit_checkpoint: bool,
        capture: impl FnOnce() -> Checkpoint,
    ) -> Result<usize, Failure> {
        let rows = made
            .iter()
            .map(|(id, record)| Ok((id.0, region_frame(id.0, record)?)))
            .collect::<Result<Vec<_>, Failure>>()?;
        let attachment = attachments::from_record(record)?;
        self.push_with_history_admission(
            1,
            record,
            sources,
            rows,
            Some(capture),
            (admit_checkpoint, attachment),
        )
    }
    fn push<T: Serialize>(
        &self,
        kind: u16,
        record: &T,
        sources: Vec<(u64, Arc<str>)>,
        regions: Vec<(u64, Vec<u8>)>,
        capture: Option<impl FnOnce() -> Checkpoint>,
    ) -> Result<usize, Failure> {
        self.push_with_history_admission(kind, record, sources, regions, capture, (false, None))
    }
    fn push_with_history_admission<T: Serialize>(
        &self,
        kind: u16,
        record: &T,
        sources: Vec<(u64, Arc<str>)>,
        regions: Vec<(u64, Vec<u8>)>,
        capture: Option<impl FnOnce() -> Checkpoint>,
        admission: (bool, Option<AttachmentWrite>),
    ) -> Result<usize, Failure> {
        let (admit_checkpoint, attachment) = admission;
        let _admission = self.0.admission.lock().unwrap();
        let shared = &self.0.shared;
        let s = shared.state.lock().unwrap();
        if s.status.error.is_some() || s.closing {
            return Err(storage_failure());
        }
        let sequence = s
            .status
            .accepted_sequence
            .checked_add(1)
            .filter(|v| *v <= i64::MAX as u64)
            .ok_or_else(storage_failure)?;
        let needs_checkpoint = shared.policy.checkpoint_interval > 0
            && sequence.saturating_sub(s.last_checkpoint_requested)
                >= shared.policy.checkpoint_interval;
        drop(s);
        // The producer gate preserves sequence and checkpoint ordering while
        // encoding and capture run outside the worker's shared status lock.
        let bytes = if kind == 2 {
            frame(
                kind,
                sequence,
                &Marker {
                    save_id: self.0.save_id.clone(),
                    generation: 0,
                    wizard_game: true,
                },
            )?
        } else {
            frame(
                kind,
                sequence,
                &Envelope {
                    save_id: self.0.save_id.clone(),
                    generation: 0,
                    record,
                },
            )?
        };
        let copied = sources.iter().map(|(_, text)| text.len()).sum::<usize>()
            + regions.iter().map(|(_, frame)| frame.len()).sum::<usize>();
        let attachment_bytes = attachment.as_ref().map_or(0, |(_, bytes)| bytes.len());
        let mandatory_bytes = bytes
            .len()
            .checked_add(attachment_bytes)
            .ok_or_else(storage_failure)?;
        let len = mandatory_bytes + copied;
        Self::check_admission(
            &mut shared.state.lock().unwrap(),
            shared,
            mandatory_bytes,
            copied,
        )?;
        let checkpoint = if needs_checkpoint || admit_checkpoint {
            capture.map(|capture| capture())
        } else {
            None
        };
        if admit_checkpoint {
            let checkpoint = checkpoint.as_ref().ok_or_else(storage_failure)?;
            // Same exact raw+compressed storage envelope, under the producer
            // gate with the actual sequence, before acceptance/publication.
            admit_history_checkpoint(&checkpoint.encode(&self.0.save_id, sequence))?;
            for record in checkpoint.records.values() {
                admit_region_record(record)?;
            }
        }
        // Admission between scheduled checkpoints must not change checkpoint
        // cadence or enqueue an additional periodic checkpoint.
        let checkpoint = if needs_checkpoint { checkpoint } else { None };
        let mut s = shared.state.lock().unwrap();
        // A worker failure or shutdown during preparation must reject without
        // publishing the prepared record or checkpoint.
        Self::check_admission(&mut s, shared, mandatory_bytes, copied)?;
        debug_assert_eq!(s.status.accepted_sequence.checked_add(1), Some(sequence));
        if let Some(checkpoint) = checkpoint {
            s.checkpoint = Some((sequence, checkpoint));
            s.last_checkpoint_requested = sequence;
        }
        let now = Instant::now();
        // Writer lock order is State then InputOwnership. Readers acquire only
        // InputOwnership, briefly, and never acquire State or perform I/O there.
        shared.inputs.lock().unwrap().admit(sequence, &sources);
        s.pending.push_back(Pending {
            attachment,
            regions,
            sequence,
            bytes,
            sources,
            queued: now,
        });
        s.status.accepted_sequence = sequence;
        s.status.pending_bytes += len;
        s.oldest.get_or_insert(now);
        s.last_activity = now;
        shared.wake.notify_one();
        Ok(len)
    }
    fn check_admission(
        state: &mut State,
        shared: &Shared,
        frame_bytes: usize,
        copied_bytes: usize,
    ) -> Result<(), Failure> {
        if state.status.error.is_some() || state.closing {
            return Err(storage_failure());
        }
        if !fits_queue(
            state.status.pending_bytes,
            frame_bytes,
            copied_bytes,
            shared.policy.max_pending_bytes,
        ) {
            state.force = state.status.accepted_sequence;
            shared.wake.notify_one();
            return Err(Failure::new(
                tor_protocol::ErrorCode::StorageFailure,
                "Save queue is full; wait for saving to finish before retrying",
            ));
        }
        Ok(())
    }
    /// The records on disk after the latest committed checkpoint, once.
    pub(crate) fn take_written(
        &self,
    ) -> Option<(BTreeSet<tor_simulation::RecordId>, tor_simulation::RecordId)> {
        self.0.shared.state.lock().unwrap().written.take()
    }

    pub(crate) fn take_appended(&self) -> BTreeSet<tor_simulation::RecordId> {
        std::mem::take(&mut self.0.shared.state.lock().unwrap().appended)
    }
    /// Read a region record row, on its own connection so the engine never
    /// waits on the worker's lock beyond SQLite's busy timeout.
    pub(crate) fn read_region(
        &self,
        id: tor_simulation::RecordId,
    ) -> Result<Option<tor_simulation::RegionRecord>, Failure> {
        let mut reader = self.0.reader.lock().unwrap();
        if reader.is_none() {
            *reader = Some(connection(&self.0.path)?);
        }
        read_region_row(reader.as_ref().unwrap(), id, self.0.content.clone())
    }
    /// A reader of the save's region file copies, on a connection of its own.
    pub(crate) fn source_reader(&self) -> SourceReader {
        SourceReader {
            path: self.0.path.clone(),
            conn: None,
            inputs: self.0.shared.inputs.clone(),
        }
    }
    /// Whether both handles are the same open save.
    pub(crate) fn same(&self, other: &Store) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
    /// A reader of region rows with its own connection, for another thread.
    pub(crate) fn region_reader(&self) -> RegionReader {
        RegionReader {
            content: self.0.content.clone(),
            path: self.0.path.clone(),
            conn: None,
        }
    }
    pub(crate) fn wizard(&self) -> Result<(), Failure> {
        self.push(2, &(), Vec::new(), Vec::new(), None::<fn() -> Checkpoint>)?;
        Ok(())
    }
    pub(crate) fn status(&self) -> SaveStatus {
        let s = self.0.shared.state.lock().unwrap();
        let mut status = s.status.clone();
        let age = s.oldest.map(|t| t.elapsed()).unwrap_or_default();
        status.unsaved_age_ms = age.as_millis().min(u128::from(u64::MAX)) as u64;
        status.overdue = s.oldest.is_some() && age >= self.0.shared.policy.max_unsaved_age;
        status
    }
    pub(crate) fn request_flush(&self) -> u64 {
        let mut s = self.0.shared.state.lock().unwrap();
        s.force = s.status.accepted_sequence;
        s.status.error = None;
        self.0.shared.wake.notify_one();
        s.force
    }
    pub(crate) fn flush(&self) -> Result<(), Failure> {
        let target = self.request_flush();
        let shared = &self.0.shared;
        let mut s = shared.state.lock().unwrap();
        while s.status.durable_sequence < target {
            if s.status.error.is_some() {
                return Err(storage_failure());
            }
            s = shared.wake.wait(s).unwrap();
        }
        Ok(())
    }
}
impl Drop for Owner {
    fn drop(&mut self) {
        {
            let mut s = self.shared.state.lock().unwrap();
            s.closing = true;
            s.force = s.status.accepted_sequence;
            self.shared.wake.notify_one();
        }
        if let Some(thread) = self.thread.lock().unwrap().take() {
            let _ = thread.join();
        }
    }
}
fn run_worker(shared: Arc<Shared>, conn: Connection, path: PathBuf) {
    let test_barrier = std::env::var_os("TOR_TEST_SAVE_BARRIER").map(PathBuf::from);
    let mut conn = Some(conn);
    loop {
        let mut s = shared.state.lock().unwrap();
        let age = s.oldest.map(|t| t.elapsed()).unwrap_or_default();
        let due = !s.pending.is_empty()
            && shared.policy.due(
                age,
                s.last_activity.elapsed(),
                s.status.pending_bytes,
                s.force > s.status.durable_sequence,
            );
        if s.closing && (s.pending.is_empty() || s.status.error.is_some()) {
            break;
        }
        if !due || s.status.error.is_some() {
            if s.pending.is_empty() || s.status.error.is_some() {
                drop(shared.wake.wait(s).unwrap());
            } else {
                let deadline = shared.policy.max_unsaved_age.saturating_sub(age);
                let opportunity = shared.policy.target_interval.saturating_sub(age).max(
                    shared
                        .policy
                        .idle_interval
                        .saturating_sub(s.last_activity.elapsed()),
                );
                drop(
                    shared
                        .wake
                        .wait_timeout(s, deadline.min(opportunity).max(Duration::from_millis(1)))
                        .unwrap(),
                );
            }
            continue;
        }
        let batch: Vec<_> = s.pending.drain(..).collect();
        let mut checkpoint = s.checkpoint.take();
        s.status.saving = true;
        drop(s);
        let start = Instant::now();
        let checkpoint_started = Instant::now();
        let encoded = checkpoint
            .as_ref()
            .map(|(seq, state)| {
                Ok((
                    encode_background_checkpoint(
                        &shared,
                        *seq,
                        &state.encode(&shared.save_id, *seq),
                    )?,
                    RegionWrite::encode(state).map_err(|_| SaveFailure::Retryable)?,
                ))
            })
            .transpose();
        // A checkpoint is optional compaction, not the durability boundary.
        // Discard the oversized capture (never retry it) and commit the same
        // admitted journal/source/region batch against the last good checkpoint.
        // Newer captures concurrently queued in State remain eligible.
        let (encoded, deferred) = match encoded {
            Err(SaveFailure::CheckpointCapacity { sequence }) => {
                checkpoint = None;
                (Ok(None), Some(sequence))
            }
            other => (other, None),
        };
        let checkpoint_ms = checkpoint_started
            .elapsed()
            .as_millis()
            .min(u128::from(u64::MAX)) as u64;
        let checkpoint_bytes = encoded
            .as_ref()
            .ok()
            .and_then(|v| v.as_ref())
            .map(|(b, regions)| {
                (b.len() + regions.rows.iter().map(|r| r.1.len()).sum::<usize>()) as u64
            })
            .unwrap_or(0);
        let mut commit_started = None;
        let mut commit_ms = 0;
        let outcome = encoded.and_then(|encoded| {
            crate::process_diagnostics::save_barrier(
                test_barrier.as_deref(),
                Duration::from_secs(30),
            )
            .map_err(|_| SaveFailure::Retryable)?;
            if conn.is_none() {
                conn = Some(connection(&path).map_err(|_| SaveFailure::Retryable)?);
            }
            commit_batch(
                conn.as_mut().unwrap(),
                &batch,
                checkpoint
                    .as_ref()
                    .zip(encoded.as_ref())
                    .map(|((seq, _), (bytes, _))| (*seq, bytes.as_slice())),
                encoded.as_ref().map(|(_, regions)| regions),
                |_, stage| {
                    if stage == "before_commit" {
                        commit_started = Some(Instant::now());
                    } else if stage == "after_commit" {
                        commit_ms = commit_started
                            .expect("commit has started")
                            .elapsed()
                            .as_millis()
                            .min(u128::from(u64::MAX)) as u64;
                    }
                    Ok(())
                },
            )
            .map_err(|_| SaveFailure::Retryable)
        });
        let mut s = shared.state.lock().unwrap();
        s.status.saving = false;
        if let Some(sequence) = deferred {
            s.status.checkpoint_deferred_sequence = Some(sequence);
        }
        match outcome {
            Ok(()) => {
                {
                    let mut inputs = shared.inputs.lock().unwrap();
                    for entry in &batch {
                        inputs.committed(entry.sequence, &entry.sources);
                    }
                }
                if let Some((seq, state)) = &checkpoint {
                    s.appended.clear();
                    s.written = Some((state.referenced(), state.watermark));
                    s.status.checkpoint_sequence = *seq;
                    s.status.checkpoint_deferred_sequence = None;
                    s.status.checkpoints += 1;
                    s.status.checkpoint_bytes = checkpoint_bytes;
                    s.status.last_checkpoint_ms = checkpoint_ms;
                }
                let journal_bytes = batch.iter().map(|p| p.bytes.len()).sum::<usize>();
                let source_bytes = batch
                    .iter()
                    .flat_map(|p| &p.sources)
                    .map(|(_, text)| text.len())
                    .sum::<usize>();
                let region_bytes = batch
                    .iter()
                    .flat_map(|entry| &entry.regions)
                    .map(|(_, frame)| frame.len())
                    .sum::<usize>();
                s.appended.extend(
                    batch
                        .iter()
                        .flat_map(|entry| &entry.regions)
                        .map(|(id, _)| tor_simulation::RecordId(*id)),
                );
                // Both frames and copied sources consume admission capacity.
                // Release it only after their shared transaction commits;
                // journal_bytes continues to describe journal frames alone.
                let attachment_bytes = batch
                    .iter()
                    .filter_map(|p| p.attachment.as_ref())
                    .map(|(_, bytes)| bytes.len())
                    .sum::<usize>();
                s.status.pending_bytes -=
                    journal_bytes + source_bytes + region_bytes + attachment_bytes;
                s.status.journal_bytes += journal_bytes as u64;
                s.status.durable_sequence = batch.last().unwrap().sequence;
                s.status.batches += 1;
                s.status.last_batch_ms =
                    start.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
                s.status.last_commit_ms = commit_ms;
                s.oldest = s.pending.front().map(|p| p.queued);
            }
            Err(_) => {
                if s.checkpoint.is_none() {
                    s.checkpoint = checkpoint;
                }
                for entry in batch.into_iter().rev() {
                    s.pending.push_front(entry);
                }
                s.status.error = Some(
                    "Background save failed; recent play is not saved. Use save to retry.".into(),
                );
                // Discard uncertain connection state before an explicit retry.
                drop(s);
                conn = None;
                s = shared.state.lock().unwrap();
            }
        }
        shared.wake.notify_all();
        drop(s);
    }
}

#[cfg(test)]
#[path = "storage_tests.rs"]
mod tests;

#[cfg(test)]
mod historical_admission_guard_tests {
    use super::*;
    use crate::engine::{Engine, Scenario};
    #[test]
    fn no_history_change_does_not_capture_or_encode_between_checkpoint_intervals() {
        let directory = tempfile::tempdir().unwrap();
        let engine = Engine::open_with_policy(
            directory.path().join("no-history.db"),
            Scenario::two_room(42),
            SavePolicy {
                checkpoint_interval: 0,
                ..SavePolicy::default()
            },
        )
        .unwrap();
        let store = engine.flush_handle().unwrap();
        HISTORY_ADMISSION_TEST_LIMIT.with(|limit| limit.set(Some(0)));
        let result = store.push_with_history_admission(
            2,
            &(),
            vec![],
            vec![],
            Some(|| -> Checkpoint { panic!("unchanged history must not capture a checkpoint") }),
            (false, None),
        );
        HISTORY_ADMISSION_TEST_LIMIT.with(|limit| limit.set(None));
        assert!(result.is_ok());
        store.flush().unwrap();
    }
    #[test]
    fn changed_history_capacity_failure_does_not_accept_or_publish_between_intervals() {
        let directory = tempfile::tempdir().unwrap();
        let engine = Engine::open_with_policy(
            directory.path().join("history-capacity.db"),
            Scenario::two_room(42),
            SavePolicy {
                checkpoint_interval: 0,
                ..SavePolicy::default()
            },
        )
        .unwrap();
        let store = engine.flush_handle().unwrap();
        let status = store.status();
        let before = engine.state(tor_protocol::ActorId(1)).unwrap();
        let captures = std::cell::Cell::new(0);
        // Deliberate test-only capacity injection through the exact production
        // encoder path; large-lawful-envelope controls are a separate cohort.
        HISTORY_ADMISSION_TEST_LIMIT.with(|limit| limit.set(Some(0)));
        let result = store.push_with_history_admission(
            2,
            &(),
            vec![],
            vec![],
            Some(|| {
                captures.set(captures.get() + 1);
                Checkpoint::capture(&engine)
            }),
            (true, None),
        );
        HISTORY_ADMISSION_TEST_LIMIT.with(|limit| limit.set(None));
        assert_eq!(
            result.unwrap_err().code,
            tor_protocol::ErrorCode::StorageFailure
        );
        assert_eq!(captures.get(), 1);
        let after = store.status();
        assert_eq!(after.accepted_sequence, status.accepted_sequence);
        assert_eq!(after.pending_bytes, status.pending_bytes);
        assert_eq!(after.checkpoint_sequence, status.checkpoint_sequence);
        assert_eq!(engine.state(tor_protocol::ActorId(1)).unwrap(), before);
    }
}
