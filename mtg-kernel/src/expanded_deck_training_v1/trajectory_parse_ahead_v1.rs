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

use super::*;
use std::collections::{HashMap, VecDeque};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, OnceLock, PoisonError};
use std::thread;

/// Published bytes held for parsing and comparison at once: one full update
/// batch. Older entries are dropped first; their readers parse as before.
const PARSE_AHEAD_RETAINED_BYTES_V1: usize = MAX_BATCH_BYTES as usize;

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
}

impl CollectionTrajectoryFieldsV1 {
    /// Validation's reads of a parsed document: a missing episode is `null`,
    /// and a learner sampler or training math that is not a string counts as
    /// absent.
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
        }
    }

    /// [`Self::of_document_v1`] of the bytes `serde_json::to_vec(trajectory)`
    /// writes, without parsing them: the episode holds only integers,
    /// strings, booleans and paths, so its value tree equals the parsed one,
    /// and the two strings are written exactly when present (training math
    /// as its identity, omitted for the default).
    fn of_trajectory_v1(trajectory: &ExpandedTrajectoryV1) -> Result<Self, String> {
        Ok(Self {
            episode: serde_json::to_value(&trajectory.episode).map_err(err)?,
            learner_sampler: trajectory.learner_sampler.clone(),
            training_math: trajectory.training_math.identity_v1().map(str::to_owned),
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

fn parse_v1(kind: KindV1, bytes: &[u8]) -> Option<ParsedV1> {
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
            self.retained_bytes -= entry.bytes.len();
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

/// Queues the bytes of a trajectory file just published under `sha256`.
pub(super) fn offer_v1(sha256: &str, bytes: Vec<u8>) {
    insert_v1(sha256, bytes, [SlotV1::Queued, SlotV1::Queued]);
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
    bytes: Vec<u8>,
    trajectory: ExpandedTrajectoryV1,
) -> Result<(), String> {
    if !seeding_enabled_v1() {
        offer_v1(sha256, bytes);
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
        bytes,
        [
            SlotV1::Ready(ParsedV1::CollectionFields(fields)),
            SlotV1::Ready(ParsedV1::Trajectory(Box::new(trajectory))),
        ],
    );
    Ok(())
}

fn insert_v1(sha256: &str, bytes: Vec<u8>, slots: [SlotV1; 2]) {
    let shared = shared_v1();
    let mut state = lock_v1(shared);
    if bytes.len() > PARSE_AHEAD_RETAINED_BYTES_V1 || state.entries.contains_key(sha256) {
        return;
    }
    while state.retained_bytes + bytes.len() > PARSE_AHEAD_RETAINED_BYTES_V1 {
        let Some(oldest) = state.order.pop_front() else {
            break;
        };
        if let Some(entry) = state.entries.remove(&oldest) {
            state.retained_bytes -= entry.bytes.len();
        }
    }
    state.retained_bytes += bytes.len();
    state.order.push_back(sha256.to_owned());
    for (queue, slot) in state.queues.iter_mut().zip(&slots) {
        if matches!(slot, SlotV1::Queued) {
            queue.push_back(sha256.to_owned());
        }
    }
    state.entries.insert(
        sha256.to_owned(),
        EntryV1 {
            bytes: Arc::new(bytes),
            slots,
        },
    );
    // A reader waiting on a dropped entry parses its bytes itself.
    shared.changed.notify_all();
}

/// Starts a helper thread for the queued parses, unless none are queued or
/// enough helpers run. A collection worker with no game left calls this, so
/// helpers take the cores the collection leaves idle.
pub(super) fn start_helper_v1() {
    let shared = shared_v1();
    {
        let mut state = lock_v1(shared);
        if state.queues.iter().all(VecDeque::is_empty) || state.helpers >= PARSE_AHEAD_HELPERS_V1 {
            return;
        }
        state.helpers += 1;
    }
    let spawned = thread::Builder::new()
        .name("trajectory-parse-ahead".into())
        .stack_size(PARSE_AHEAD_STACK_BYTES_V1)
        .spawn(move || helper_v1(shared));
    if spawned.is_err() {
        lock_v1(shared).helpers -= 1;
    }
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
    if stored.as_slice() != bytes {
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
pub(super) fn served_for_test_v1(sha256: &str) -> [bool; 2] {
    let state = lock_v1(shared_v1());
    [KindV1::CollectionFields, KindV1::Trajectory]
        .map(|kind| state.served.contains(&(sha256.to_owned(), kind as u8)))
}

#[cfg(test)]
mod tests;
