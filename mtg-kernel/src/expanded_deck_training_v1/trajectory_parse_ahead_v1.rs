//! Exact parse-ahead of published trajectory files.
//!
//! A native run parses every trajectory file it publishes twice more in the
//! same process: collection validation reads it as a `serde_json::Value` (its
//! episode assignment and learner sampler) and the update reads it as an
//! `ExpandedTrajectoryV1`. Both used to start only after the slowest game of
//! the collection. Now a collection worker with no game left to play starts a
//! helper thread that parses the files already published while the last
//! games run.
//!
//! Each reader still reads and verifies its file exactly as before, then asks
//! for the result of parsing its bytes. A result is kept with the bytes it was
//! parsed from and is returned only to a reader whose bytes are equal, and
//! only successful parses are kept, so a reader receives exactly what its own
//! parse would return. On any miss it parses as before, errors included. A
//! reader whose bytes are being parsed waits for that parse instead of
//! repeating it; one whose bytes are still queued parses them itself.
//!
//! A collection worker that serialized the trajectory itself hands both
//! results over with the bytes instead (`offer_serialized_v1`), since parsing
//! those bytes returns the same values; nothing is left for helpers to parse.
//!
//! The bytes held at once are bounded by an execution setting of the process
//! (`configure_retained_mebibytes_v1`, 512 MiB by default, 0 holds nothing).
//! An entry is charged its record length or its buffer's capacity, whichever
//! is larger, so a batch that fits the update's record-byte bound need not
//! fit a retention of the same size: older entries are then dropped and
//! their readers parse as before. Every entry remembers the file it was
//! published as, and a run drops the entries under an iteration's directory
//! when that iteration ends, failed or not (`release_under_v1`).

use super::*;
use std::collections::{HashMap, VecDeque};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, OnceLock, PoisonError};
use std::thread;

/// The default bound on the bytes held for parsing and comparison at once:
/// one default update batch.
pub(crate) const DEFAULT_RETAINED_MEBIBYTES_V1: usize = 512;
/// The largest admitted bound.
pub(crate) const MAX_RETAINED_MEBIBYTES_V1: usize = 16 * 1024;
const _: () = assert!((DEFAULT_RETAINED_MEBIBYTES_V1 as u64) << 20 == MAX_BATCH_BYTES);

/// The bound in force, in bytes. Older entries are dropped first; their
/// readers parse as before.
static RETAINED_BYTES_V1: AtomicUsize = AtomicUsize::new(DEFAULT_RETAINED_MEBIBYTES_V1 << 20);

/// The bound the process was configured with, if any.
static CONFIGURED_MEBIBYTES_V1: OnceLock<usize> = OnceLock::new();

fn retained_limit_v1() -> usize {
    RETAINED_BYTES_V1.load(Ordering::Relaxed)
}

/// Sets the process's retention bound (in 0..=16384 MiB) once, before any
/// run. It changes only which reads find a result parsed ahead, never what
/// a read returns, so it is an execution setting outside every run
/// identity. Setting the same value again is accepted.
pub(crate) fn configure_retained_mebibytes_v1(mebibytes: usize) -> Result<(), String> {
    ensure(
        mebibytes <= MAX_RETAINED_MEBIBYTES_V1,
        "parse-ahead retention must be in 0..=16384 MiB",
    )?;
    let configured = *CONFIGURED_MEBIBYTES_V1.get_or_init(|| mebibytes);
    ensure(
        configured == mebibytes,
        "parse-ahead retention is already configured",
    )?;
    set_retained_bytes_v1(mebibytes << 20);
    Ok(())
}

/// Whether anything can be held (a retention bound above zero).
pub(crate) fn retains_v1() -> bool {
    retained_limit_v1() > 0
}

/// Puts `bytes` in force and drops the oldest entries above it.
fn set_retained_bytes_v1(bytes: usize) {
    RETAINED_BYTES_V1.store(bytes, Ordering::Relaxed);
    let shared = shared_v1();
    let mut state = lock_v1(shared);
    evict_to_v1(&mut state, bytes);
    drop(state);
    shared.changed.notify_all();
}

/// Helper threads alive at once, across collections.
const PARSE_AHEAD_HELPERS_V1: usize = 64;

/// The collection workers' stack size; a trajectory parse recurses through
/// the decision records.
const PARSE_AHEAD_STACK_BYTES_V1: usize = 8 * 1024 * 1024;

/// The fields collection validation reads from a trajectory document.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct CollectionTrajectoryFieldsV1 {
    pub(crate) episode: Value,
    pub(crate) learner_sampler: Option<String>,
    pub(crate) training_math: Option<String>,
    /// The `input_contract` member as written (`no_state_digest_v1`).
    pub(crate) input_contract: Option<Value>,
}

impl CollectionTrajectoryFieldsV1 {
    /// Validation's reads of a parsed document: a missing episode is `null`,
    /// a learner sampler or training math that is not a string counts as
    /// absent, and an input contract is read as written (an explicit `null`
    /// is not absent).
    pub(crate) fn of_document_v1(document: &Value) -> Self {
        let string = |field| {
            document
                .get(field)
                .and_then(Value::as_str)
                .map(str::to_owned)
        };
        Self {
            episode: document["episode"].clone(),
            learner_sampler: string("learner_sampler"),
            training_math: string("training_math"),
            input_contract: document.get("input_contract").cloned(),
        }
    }

    /// [`Self::of_document_v1`] of the bytes `serde_json::to_vec(trajectory)`
    /// writes, without parsing them: the episode holds only integers,
    /// strings, booleans and paths, so its value tree equals the parsed one,
    /// and the two strings are written exactly when present (training math
    /// as its identity, omitted for the default), as is the input contract,
    /// whose strings, integers and path form the same tree.
    fn of_trajectory_v1(trajectory: &ExpandedTrajectoryV1) -> Result<Self, String> {
        Ok(Self {
            episode: serde_json::to_value(&trajectory.episode).map_err(err)?,
            learner_sampler: trajectory.learner_sampler.clone(),
            training_math: trajectory.training_math.identity_v1().map(str::to_owned),
            input_contract: trajectory
                .input_contract
                .as_ref()
                .map(serde_json::to_value)
                .transpose()
                .map_err(err)?,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum KindV1 {
    CollectionFields = 0,
    Trajectory = 1,
}

enum ParsedV1 {
    CollectionFields(CollectionTrajectoryFieldsV1),
    Trajectory(Box<ExpandedTrajectoryV1>),
}

/// A sealed file (`record_envelope_v1`) is parsed from its opened record
/// bytes, as its readers parse it.
fn parse_v1(kind: KindV1, bytes: &[u8]) -> Option<ParsedV1> {
    let bytes = record_envelope_v1::open_v1(bytes, MAX_FILE_BYTES).ok()?;
    let bytes = bytes.as_ref();
    match kind {
        KindV1::CollectionFields => serde_json::from_slice::<Value>(bytes).ok().map(|document| {
            ParsedV1::CollectionFields(CollectionTrajectoryFieldsV1::of_document_v1(&document))
        }),
        KindV1::Trajectory => serde_json::from_slice::<ExpandedTrajectoryV1>(bytes)
            .ok()
            .map(|trajectory| ParsedV1::Trajectory(Box::new(trajectory))),
    }
}

enum SlotV1 {
    Queued,
    Running,
    Ready(ParsedV1),
    Gone,
}

struct EntryV1 {
    bytes: Arc<Vec<u8>>,
    /// The file the bytes were published as, for `release_under_v1`.
    path: PathBuf,
    /// What the entry counts against the retained-bytes bound: its record
    /// length, which for a sealed file (`record_envelope_v1`) is the opened
    /// length, so held parses stay bounded as before.
    charge: usize,
    slots: [SlotV1; 2],
}

#[derive(Default)]
struct StateV1 {
    /// Keyed by the bytes' SHA-256 (hex); a hit also compares the bytes.
    entries: HashMap<String, EntryV1>,
    /// Live keys in offer order, for dropping the oldest.
    order: VecDeque<String>,
    retained_bytes: usize,
    /// Validation reads first, so its parses run first.
    queues: [VecDeque<String>; 2],
    helpers: usize,
    #[cfg(test)]
    served: std::collections::BTreeSet<(String, u8)>,
}

impl StateV1 {
    fn remove_v1(&mut self, key: &str) {
        if let Some(entry) = self.entries.remove(key) {
            self.retained_bytes -= entry.charge;
            self.order.retain(|live| live != key);
        }
    }

    /// The next queued parse, marked running.
    fn claim_v1(&mut self) -> Option<(String, KindV1, Arc<Vec<u8>>)> {
        for kind in [KindV1::CollectionFields, KindV1::Trajectory] {
            while let Some(key) = self.queues[kind as usize].pop_front() {
                if let Some(entry) = self.entries.get_mut(&key) {
                    let slot = &mut entry.slots[kind as usize];
                    if matches!(slot, SlotV1::Queued) {
                        *slot = SlotV1::Running;
                        let bytes = Arc::clone(&entry.bytes);
                        return Some((key, kind, bytes));
                    }
                }
            }
        }
        None
    }

    /// Stores a finished parse if its entry still waits for it.
    fn finish_v1(
        &mut self,
        key: &str,
        kind: KindV1,
        bytes: &Arc<Vec<u8>>,
        parsed: Option<ParsedV1>,
    ) {
        if let Some(entry) = self
            .entries
            .get_mut(key)
            .filter(|entry| Arc::ptr_eq(&entry.bytes, bytes))
        {
            let slot = &mut entry.slots[kind as usize];
            if matches!(slot, SlotV1::Running) {
                *slot = parsed.map_or(SlotV1::Gone, SlotV1::Ready);
            }
        }
    }
}

struct ParseAheadV1 {
    state: Mutex<StateV1>,
    changed: Condvar,
}

fn shared_v1() -> &'static ParseAheadV1 {
    static SHARED: OnceLock<ParseAheadV1> = OnceLock::new();
    SHARED.get_or_init(|| ParseAheadV1 {
        state: Mutex::new(StateV1::default()),
        changed: Condvar::new(),
    })
}

fn lock_v1(shared: &ParseAheadV1) -> MutexGuard<'_, StateV1> {
    shared.state.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Queues the bytes of a trajectory file just published as `path` under
/// `sha256`.
pub(crate) fn offer_v1(sha256: &str, path: &Path, bytes: impl Into<Arc<Vec<u8>>>) {
    insert_v1(sha256, path, bytes, [SlotV1::Queued, SlotV1::Queued]);
}

/// Whether a collection worker hands over the trajectory it serialized
/// instead of queueing its bytes for parsing: on unless
/// `MTG_KERNEL_PARSE_AHEAD_SEED=0`. Either way readers receive the same values.
fn seeding_enabled_v1() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var("MTG_KERNEL_PARSE_AHEAD_SEED").map_or(true, |value| value != "0")
    })
}

/// `MTG_KERNEL_PARSE_AHEAD_SEED_CHECK=1` also parses every handed-over file
/// and fails the collection if either result differs from the handed-over one.
fn seed_check_enabled_v1() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var("MTG_KERNEL_PARSE_AHEAD_SEED_CHECK").is_ok_and(|value| value == "1")
    })
}

/// [`offer_v1`] for `bytes` that are exactly `serde_json::to_vec(&trajectory)`:
/// both results are stored ready instead of being parsed from the bytes.
/// Every trajectory field is an integer, string, boolean, path or raw binary32
/// bit pattern, so parsing the bytes returns `trajectory` itself. Readers
/// still compare their own bytes with these in full before taking a result.
pub(super) fn offer_serialized_v1(
    sha256: &str,
    path: &Path,
    bytes: impl Into<Arc<Vec<u8>>>,
    trajectory: ExpandedTrajectoryV1,
) -> Result<(), String> {
    let bytes: Arc<Vec<u8>> = bytes.into();
    if !seeding_enabled_v1() {
        offer_v1(sha256, path, bytes);
        return Ok(());
    }
    let fields = CollectionTrajectoryFieldsV1::of_trajectory_v1(&trajectory)?;
    if seed_check_enabled_v1() {
        let parsed_fields = match parse_v1(KindV1::CollectionFields, &bytes) {
            Some(ParsedV1::CollectionFields(parsed)) => Some(parsed),
            _ => None,
        };
        ensure(
            parsed_fields.as_ref() == Some(&fields),
            "parse-ahead seed check: collection fields differ from the parsed file",
        )?;
        let parsed_trajectory = match parse_v1(KindV1::Trajectory, &bytes) {
            Some(ParsedV1::Trajectory(parsed)) => Some(parsed),
            _ => None,
        };
        ensure(
            parsed_trajectory.as_deref() == Some(&trajectory),
            "parse-ahead seed check: trajectory differs from the parsed file",
        )?;
    }
    insert_v1(
        sha256,
        path,
        bytes,
        [
            SlotV1::Ready(ParsedV1::CollectionFields(fields)),
            SlotV1::Ready(ParsedV1::Trajectory(Box::new(trajectory))),
        ],
    );
    Ok(())
}

/// What an entry is charged: its record length, or the capacity of the
/// buffer it keeps alive when that is larger.
fn retained_charge_v1(record_length: usize, capacity: usize) -> usize {
    record_length.max(capacity)
}

fn insert_v1(sha256: &str, path: &Path, bytes: impl Into<Arc<Vec<u8>>>, slots: [SlotV1; 2]) {
    let bytes: Arc<Vec<u8>> = bytes.into();
    // A plain file is charged its length, as before; a sealed one its record
    // length, and one whose header is unreadable is never held. The entry
    // shares the publisher's buffer and keeps its whole allocation alive, so
    // the insertion never charges it less than that allocation's capacity (a
    // plain record's buffer can carry spare capacity from its serializer).
    let Ok(charge) = record_envelope_v1::record_length_v1(&bytes, MAX_FILE_BYTES) else {
        return;
    };
    let shared = shared_v1();
    let limit = retained_limit_v1();
    let entry = (path.to_path_buf(), bytes, charge as usize);
    if insert_locked_v1(&mut lock_v1(shared), sha256, entry, slots, limit) {
        // A reader waiting on a dropped entry parses its bytes itself.
        shared.changed.notify_all();
    }
}

/// Drops the oldest entries until at most `limit` bytes are held.
fn evict_to_v1(state: &mut StateV1, limit: usize) {
    while state.retained_bytes > limit {
        let Some(oldest) = state.order.pop_front() else {
            break;
        };
        if let Some(entry) = state.entries.remove(&oldest) {
            state.retained_bytes -= entry.charge;
        }
    }
}

/// Inserts an entry for the bytes published as a file, with that file's
/// record length (`(path, bytes, record_length)`), into the locked store,
/// charged what it keeps alive (`retained_charge_v1`), dropping the oldest
/// entries to keep at most `limit` bytes. False when it is not held (larger
/// than `limit`, or already there).
fn insert_locked_v1(
    state: &mut StateV1,
    sha256: &str,
    (path, bytes, record_length): (PathBuf, Arc<Vec<u8>>, usize),
    slots: [SlotV1; 2],
    limit: usize,
) -> bool {
    let charge = retained_charge_v1(record_length, bytes.capacity());
    if charge > limit || state.entries.contains_key(sha256) {
        return false;
    }
    evict_to_v1(state, limit - charge);
    state.retained_bytes += charge;
    state.order.push_back(sha256.to_owned());
    for (queue, slot) in state.queues.iter_mut().zip(&slots) {
        if matches!(slot, SlotV1::Queued) {
            queue.push_back(sha256.to_owned());
        }
    }
    state.entries.insert(
        sha256.to_owned(),
        EntryV1 {
            bytes,
            path,
            charge,
            slots,
        },
    );
    true
}

/// Drops every entry whose file lies under `directory`, queued and running
/// parses included: their readers parse as before, a reader waiting on a
/// running parse parses itself, and a parse finishing later finds no entry
/// to fill (a later entry for the same bytes is another allocation).
pub(crate) fn release_under_v1(directory: &Path) {
    let shared = shared_v1();
    let mut state = lock_v1(shared);
    let released: Vec<String> = state
        .entries
        .iter()
        .filter(|(_, entry)| entry.path.starts_with(directory))
        .map(|(key, _)| key.clone())
        .collect();
    if released.is_empty() {
        return;
    }
    for key in &released {
        state.remove_v1(key);
    }
    let StateV1 {
        entries, queues, ..
    } = &mut *state;
    for queue in queues.iter_mut() {
        queue.retain(|key| entries.contains_key(key));
    }
    drop(state);
    shared.changed.notify_all();
}

/// Starts a helper thread for the queued parses, unless none are queued or
/// enough helpers run. A collection worker with no game left calls this, so
/// helpers take the cores the collection leaves idle.
pub(super) fn start_helper_v1() {
    start_helper_below_v1(PARSE_AHEAD_HELPERS_V1);
}

/// Starts helpers for the queued parses until `limit` helpers (at most the
/// process-wide 64) are alive or every queued parse has one.
pub(crate) fn start_helpers_up_to_v1(limit: usize) {
    let limit = limit.min(PARSE_AHEAD_HELPERS_V1);
    loop {
        let wanted = {
            let state = lock_v1(shared_v1());
            let queued: usize = state.queues.iter().map(VecDeque::len).sum();
            queued > state.helpers && state.helpers < limit
        };
        if !wanted || !start_helper_below_v1(limit) {
            return;
        }
    }
}

/// [`start_helper_v1`] with at most `limit` helpers alive; whether one started.
fn start_helper_below_v1(limit: usize) -> bool {
    let shared = shared_v1();
    {
        let mut state = lock_v1(shared);
        if state.queues.iter().all(VecDeque::is_empty) || state.helpers >= limit {
            return false;
        }
        state.helpers += 1;
    }
    let spawned = thread::Builder::new()
        .name("trajectory-parse-ahead".into())
        .stack_size(PARSE_AHEAD_STACK_BYTES_V1)
        .spawn(move || helper_v1(shared));
    if spawned.is_err() {
        lock_v1(shared).helpers -= 1;
        return false;
    }
    true
}

/// Releases a claimed parse even if the helper unwinds, so no reader waits
/// on it forever.
struct ClaimV1 {
    shared: &'static ParseAheadV1,
    key: String,
    kind: KindV1,
    bytes: Arc<Vec<u8>>,
}

impl Drop for ClaimV1 {
    fn drop(&mut self) {
        let mut state = lock_v1(self.shared);
        state.finish_v1(&self.key, self.kind, &self.bytes, None);
        self.shared.changed.notify_all();
    }
}

fn helper_v1(shared: &'static ParseAheadV1) {
    loop {
        let claimed = lock_v1(shared).claim_v1();
        let Some((key, kind, bytes)) = claimed else {
            break;
        };
        let claim = ClaimV1 {
            shared,
            key,
            kind,
            bytes,
        };
        let parsed = catch_unwind(AssertUnwindSafe(|| parse_v1(claim.kind, &claim.bytes)))
            .ok()
            .flatten();
        let mut state = lock_v1(shared);
        state.finish_v1(&claim.key, claim.kind, &claim.bytes, parsed);
        drop(state);
        drop(claim);
    }
    lock_v1(shared).helpers -= 1;
}

/// The parse of exactly `bytes`, if it was parsed ahead; waits while it is
/// being parsed. A queued parse is withdrawn and left to the caller.
fn take_v1(sha256: &str, bytes: &[u8], kind: KindV1) -> Option<ParsedV1> {
    let shared = shared_v1();
    let stored = Arc::clone(&lock_v1(shared).entries.get(sha256)?.bytes);
    // The very allocation the entry holds (a resident read of the bytes the
    // publisher shared) is equal without comparing it.
    let same_allocation = stored.as_ptr() == bytes.as_ptr() && stored.len() == bytes.len();
    if !same_allocation && stored.as_slice() != bytes {
        return None;
    }
    let mut state = lock_v1(shared);
    loop {
        let entry = state
            .entries
            .get_mut(sha256)
            .filter(|entry| Arc::ptr_eq(&entry.bytes, &stored))?;
        let slot = &mut entry.slots[kind as usize];
        if matches!(slot, SlotV1::Running) {
            state = shared
                .changed
                .wait(state)
                .unwrap_or_else(PoisonError::into_inner);
            continue;
        }
        let parsed = match std::mem::replace(slot, SlotV1::Gone) {
            SlotV1::Ready(parsed) => Some(parsed),
            _ => None,
        };
        if entry.slots.iter().all(|slot| matches!(slot, SlotV1::Gone)) {
            state.remove_v1(sha256);
        }
        #[cfg(test)]
        if parsed.is_some() {
            state.served.insert((sha256.to_owned(), kind as u8));
        }
        return parsed;
    }
}

/// Collection validation's fields for exactly `bytes` (pinned as `sha256`),
/// if they were parsed ahead.
pub(crate) fn take_collection_fields_v1(
    sha256: &str,
    bytes: &[u8],
) -> Option<CollectionTrajectoryFieldsV1> {
    match take_v1(sha256, bytes, KindV1::CollectionFields)? {
        ParsedV1::CollectionFields(fields) => Some(fields),
        ParsedV1::Trajectory(_) => None,
    }
}

/// The update's trajectory for exactly `bytes` (pinned as `sha256`), if it was
/// parsed ahead.
pub(super) fn take_trajectory_v1(sha256: &str, bytes: &[u8]) -> Option<ExpandedTrajectoryV1> {
    match take_v1(sha256, bytes, KindV1::Trajectory)? {
        ParsedV1::Trajectory(trajectory) => Some(*trajectory),
        ParsedV1::CollectionFields(_) => None,
    }
}

/// Waits until no parse of `sha256` is queued or running, starting helpers
/// as needed.
#[cfg(test)]
pub(super) fn wait_until_parsed_for_test_v1(sha256: &str) {
    let started = std::time::Instant::now();
    loop {
        let pending = lock_v1(shared_v1())
            .entries
            .get(sha256)
            .is_some_and(|entry| {
                entry
                    .slots
                    .iter()
                    .any(|slot| matches!(slot, SlotV1::Queued | SlotV1::Running))
            });
        if !pending {
            return;
        }
        assert!(
            started.elapsed() < std::time::Duration::from_secs(120),
            "parse-ahead stalled"
        );
        start_helper_v1();
        thread::sleep(std::time::Duration::from_millis(5));
    }
}

/// Whether a reader received a parse-ahead result for `sha256`: collection
/// fields, then the trajectory.
#[cfg(test)]
pub(crate) fn served_for_test_v1(sha256: &str) -> [bool; 2] {
    let state = lock_v1(shared_v1());
    [KindV1::CollectionFields, KindV1::Trajectory]
        .map(|kind| state.served.contains(&(sha256.to_owned(), kind as u8)))
}

/// The entries held for files under `directory`.
#[cfg(test)]
pub(crate) fn held_under_for_test_v1(directory: &Path) -> usize {
    lock_v1(shared_v1())
        .entries
        .values()
        .filter(|entry| entry.path.starts_with(directory))
        .count()
}

/// Forgets that results for `sha256` were served, for a test that runs the
/// same bytes again.
#[cfg(test)]
pub(crate) fn forget_served_for_test_v1(sha256: &str) {
    let mut state = lock_v1(shared_v1());
    for kind in [KindV1::CollectionFields, KindV1::Trajectory] {
        state.served.remove(&(sha256.to_owned(), kind as u8));
    }
}

/// Tests that expect results parsed ahead hold this lock shared, and a test
/// that changes the retention holds it exclusively.
#[cfg(test)]
static RETENTION_TEST_LOCK_V1: std::sync::RwLock<()> = std::sync::RwLock::new(());

/// Held by a test that expects results parsed ahead: no test changes the
/// retention meanwhile.
#[cfg(test)]
pub(crate) fn default_retention_for_test_v1() -> std::sync::RwLockReadGuard<'static, ()> {
    RETENTION_TEST_LOCK_V1
        .read()
        .unwrap_or_else(PoisonError::into_inner)
}

/// A test's exclusive hold on the retention; the default is back in force
/// when it is dropped.
#[cfg(test)]
pub(crate) struct RetentionForTestV1(#[allow(dead_code)] std::sync::RwLockWriteGuard<'static, ()>);

#[cfg(test)]
impl RetentionForTestV1 {
    pub(crate) fn set_v1(&self, bytes: usize) {
        set_retained_bytes_v1(bytes);
    }
}

#[cfg(test)]
impl Drop for RetentionForTestV1 {
    fn drop(&mut self) {
        set_retained_bytes_v1(DEFAULT_RETAINED_MEBIBYTES_V1 << 20);
    }
}

#[cfg(test)]
pub(crate) fn retention_for_test_v1() -> RetentionForTestV1 {
    RetentionForTestV1(
        RETENTION_TEST_LOCK_V1
            .write()
            .unwrap_or_else(PoisonError::into_inner),
    )
}

#[cfg(test)]
mod tests;
