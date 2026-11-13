//! Accounting of model-source loads (#595): how often each caller role
//! loads a source and for how long, what the loaded-source cache
//! (`loaded_source_cache_v1`) did for it, and how many bytes of train state
//! and policy each cache hit or kept entry copied. It only counts, with
//! relaxed atomics, and no result depends on it; the counters leave the
//! process only through the opt-in diagnostics file (`cache_diagnostics_v1`).
//!
//! A role is the purpose of one load, set around it on the calling thread by
//! a [`RoleScopeV1`]. A load on a thread without a scope counts as
//! `unattributed`; nested scopes count the inner role, and each scope's wall
//! time is inclusive of anything it encloses. Bytes are the `f32` buffers a
//! copy duplicates, four bytes each: for a state, the parameters, derived
//! transposed weights and both Adam moments; for a policy, the embedding
//! table and any boxed no-state-digest check model (a policy duplicate
//! shares its model).
//!
//! Counters are process-wide; unit tests run in parallel threads, so there
//! each test thread keeps its own, as the cache itself does.

use serde_json::{json, Map, Value};
use std::cell::Cell;
use std::time::Instant;

pub(crate) const REPORT_SCHEMA_V1: &str = "mtg-kernel-loaded-source-profile/v1";

/// Why a source is loaded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LoadRoleV1 {
    /// The run's initial source, for its inference identity.
    RunInitial,
    /// A fixed opponent's contract check before the first collection.
    RunFixedOpponent,
    /// The collection learner: a policy to play and its state's digest.
    CollectLearner,
    /// An opponent's policy, for collection or for the update's replay.
    Opponent,
    /// The update learner: the train state the step mutates.
    UpdateLearner,
    /// The update's readback of the checkpoint it just published.
    UpdateReadback,
    /// The run loop's validation of a published update.
    UpdateValidation,
    /// A load outside any scope.
    Unattributed,
}

const ROLES_V1: [LoadRoleV1; 8] = [
    LoadRoleV1::RunInitial,
    LoadRoleV1::RunFixedOpponent,
    LoadRoleV1::CollectLearner,
    LoadRoleV1::Opponent,
    LoadRoleV1::UpdateLearner,
    LoadRoleV1::UpdateReadback,
    LoadRoleV1::UpdateValidation,
    LoadRoleV1::Unattributed,
];

impl LoadRoleV1 {
    fn index_v1(self) -> usize {
        self as usize
    }

    fn name_v1(self) -> &'static str {
        match self {
            Self::RunInitial => "run_initial",
            Self::RunFixedOpponent => "run_fixed_opponent",
            Self::CollectLearner => "collect_learner",
            Self::Opponent => "opponent",
            Self::UpdateLearner => "update_learner",
            Self::UpdateReadback => "update_readback",
            Self::UpdateValidation => "update_validation",
            Self::Unattributed => "unattributed",
        }
    }
}

/// One role's counters, in report order.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct RoleCountsV1 {
    /// Scoped loads, and their inclusive wall time.
    pub(crate) scoped_loads: u64,
    pub(crate) scoped_nanos: u64,
    /// Cache lookups that searched the entries; each ends as a hit, a miss
    /// (the caller loads in full) or an error (a reread file failed).
    pub(crate) cache_lookups: u64,
    pub(crate) cache_hits: u64,
    pub(crate) cache_misses: u64,
    /// Full loads kept as a new entry.
    pub(crate) cache_kept: u64,
    /// Bytes copied out of an entry on hits.
    pub(crate) served_state_bytes: u64,
    pub(crate) served_policy_bytes: u64,
    /// Bytes copied into kept entries.
    pub(crate) kept_state_bytes: u64,
    pub(crate) kept_policy_bytes: u64,
}

impl RoleCountsV1 {
    fn fields_v1(&self) -> [(&'static str, u64); 10] {
        [
            ("scoped_loads", self.scoped_loads),
            ("scoped_nanos", self.scoped_nanos),
            ("cache_lookups", self.cache_lookups),
            ("cache_hits", self.cache_hits),
            ("cache_misses", self.cache_misses),
            ("cache_kept", self.cache_kept),
            ("served_state_bytes", self.served_state_bytes),
            ("served_policy_bytes", self.served_policy_bytes),
            ("kept_state_bytes", self.kept_state_bytes),
            ("kept_policy_bytes", self.kept_policy_bytes),
        ]
    }

    fn add_v1(&mut self, other: &Self) {
        self.scoped_loads += other.scoped_loads;
        self.scoped_nanos += other.scoped_nanos;
        self.cache_lookups += other.cache_lookups;
        self.cache_hits += other.cache_hits;
        self.cache_misses += other.cache_misses;
        self.cache_kept += other.cache_kept;
        self.served_state_bytes += other.served_state_bytes;
        self.served_policy_bytes += other.served_policy_bytes;
        self.kept_state_bytes += other.kept_state_bytes;
        self.kept_policy_bytes += other.kept_policy_bytes;
    }
}

#[cfg(not(test))]
mod counters_v1 {
    use super::{RoleCountsV1, ROLES_V1};
    use std::sync::atomic::{AtomicU64, Ordering::Relaxed};

    static COUNTS: [[AtomicU64; 10]; ROLES_V1.len()] =
        [const { [const { AtomicU64::new(0) }; 10] }; ROLES_V1.len()];

    pub(super) fn add_delta_v1(role: usize, delta: &RoleCountsV1) {
        for (counter, (_, value)) in COUNTS[role].iter().zip(delta.fields_v1()) {
            if value != 0 {
                counter.fetch_add(value, Relaxed);
            }
        }
    }

    pub(super) fn read_v1(role: usize) -> RoleCountsV1 {
        let [scoped_loads, scoped_nanos, cache_lookups, cache_hits, cache_misses, cache_kept, served_state_bytes, served_policy_bytes, kept_state_bytes, kept_policy_bytes] =
            std::array::from_fn(|index| COUNTS[role][index].load(Relaxed));
        RoleCountsV1 {
            scoped_loads,
            scoped_nanos,
            cache_lookups,
            cache_hits,
            cache_misses,
            cache_kept,
            served_state_bytes,
            served_policy_bytes,
            kept_state_bytes,
            kept_policy_bytes,
        }
    }
}

#[cfg(test)]
mod counters_v1 {
    use super::{RoleCountsV1, ROLES_V1};
    use std::cell::RefCell;

    thread_local! {
        static COUNTS: RefCell<[RoleCountsV1; ROLES_V1.len()]> = RefCell::default();
    }

    pub(super) fn add_delta_v1(role: usize, delta: &RoleCountsV1) {
        COUNTS.with(|counts| counts.borrow_mut()[role].add_v1(delta));
    }

    pub(super) fn read_v1(role: usize) -> RoleCountsV1 {
        COUNTS.with(|counts| counts.borrow()[role])
    }

    pub(super) fn reset_v1() {
        COUNTS.with(|counts| *counts.borrow_mut() = Default::default());
    }
}

/// Adds the counts `set` writes on a zeroed delta to `role`'s.
fn record_v1(role: usize, set: impl FnOnce(&mut RoleCountsV1)) {
    let mut delta = RoleCountsV1::default();
    set(&mut delta);
    counters_v1::add_delta_v1(role, &delta);
}

thread_local! {
    static ROLE: Cell<LoadRoleV1> = const { Cell::new(LoadRoleV1::Unattributed) };
}

fn current_role_v1() -> usize {
    ROLE.with(Cell::get).index_v1()
}

/// Counts the loads made on this thread while it lives as `role`'s.
pub(crate) struct RoleScopeV1 {
    role: LoadRoleV1,
    previous: LoadRoleV1,
    started: Instant,
}

impl RoleScopeV1 {
    pub(crate) fn enter(role: LoadRoleV1) -> Self {
        let previous = ROLE.with(|current| current.replace(role));
        Self {
            role,
            previous,
            started: Instant::now(),
        }
    }
}

impl Drop for RoleScopeV1 {
    fn drop(&mut self) {
        let nanos = u64::try_from(self.started.elapsed().as_nanos()).unwrap_or(u64::MAX);
        record_v1(self.role.index_v1(), |counts| {
            counts.scoped_loads = 1;
            counts.scoped_nanos = nanos;
        });
        ROLE.with(|current| current.set(self.previous));
    }
}

/// Runs `load` on this thread as one load of `role`.
pub(crate) fn scoped_v1<T>(role: LoadRoleV1, load: impl FnOnce() -> T) -> T {
    let _scope = RoleScopeV1::enter(role);
    load()
}

/// A cache lookup that searched the entries.
pub(crate) fn record_lookup_v1() {
    record_v1(current_role_v1(), |counts| counts.cache_lookups = 1);
}

/// A lookup served copies of `state_bytes` and `policy_bytes`.
pub(crate) fn record_hit_v1(state_bytes: u64, policy_bytes: u64) {
    record_v1(current_role_v1(), |counts| {
        counts.cache_hits = 1;
        counts.served_state_bytes = state_bytes;
        counts.served_policy_bytes = policy_bytes;
    });
}

/// A lookup the caller must load in full.
pub(crate) fn record_miss_v1() {
    record_v1(current_role_v1(), |counts| counts.cache_misses = 1);
}

/// A full load kept as an entry, copying `state_bytes` and `policy_bytes`.
pub(crate) fn record_kept_v1(state_bytes: u64, policy_bytes: u64) {
    record_v1(current_role_v1(), |counts| {
        counts.cache_kept = 1;
        counts.kept_state_bytes = state_bytes;
        counts.kept_policy_bytes = policy_bytes;
    });
}

/// Bytes of `f32` buffers holding `len` values.
pub(crate) fn f32_bytes_v1(len: usize) -> u64 {
    (len as u64).saturating_mul(4)
}

/// This process's counters (this thread's in unit tests).
pub(crate) fn counts_v1(role: LoadRoleV1) -> RoleCountsV1 {
    counters_v1::read_v1(role.index_v1())
}

/// Every role's counters and their total, as one JSON object.
pub(crate) fn report_v1() -> Value {
    let mut roles = Map::new();
    let mut total = RoleCountsV1::default();
    for role in ROLES_V1 {
        let counts = counts_v1(role);
        total.add_v1(&counts);
        roles.insert(role.name_v1().into(), counts_json_v1(&counts));
    }
    json!({"schema": REPORT_SCHEMA_V1, "roles": roles, "total": counts_json_v1(&total)})
}

fn counts_json_v1(counts: &RoleCountsV1) -> Value {
    Value::Object(
        counts
            .fields_v1()
            .into_iter()
            .map(|(name, value)| (name.to_owned(), json!(value)))
            .collect(),
    )
}

/// Test-only: zeroes this thread's counters.
#[cfg(test)]
pub(crate) fn reset_for_test_v1() {
    counters_v1::reset_v1();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scopes_attribute_cache_events_to_the_innermost_role_and_restore_the_outer() {
        reset_for_test_v1();
        record_lookup_v1();
        {
            let _outer = RoleScopeV1::enter(LoadRoleV1::UpdateLearner);
            record_lookup_v1();
            record_hit_v1(12, 4);
            {
                let _inner = RoleScopeV1::enter(LoadRoleV1::Opponent);
                record_lookup_v1();
                record_miss_v1();
                record_kept_v1(8, 2);
            }
            record_lookup_v1();
            record_hit_v1(12, 4);
        }
        record_miss_v1();
        let learner = counts_v1(LoadRoleV1::UpdateLearner);
        assert_eq!(
            (
                learner.scoped_loads,
                learner.cache_lookups,
                learner.cache_hits
            ),
            (1, 2, 2)
        );
        assert_eq!(
            (learner.served_state_bytes, learner.served_policy_bytes),
            (24, 8)
        );
        assert_eq!(learner.cache_misses + learner.cache_kept, 0);
        let opponent = counts_v1(LoadRoleV1::Opponent);
        assert_eq!(
            (
                opponent.scoped_loads,
                opponent.cache_lookups,
                opponent.cache_misses,
                opponent.cache_kept,
                opponent.kept_state_bytes,
                opponent.kept_policy_bytes
            ),
            (1, 1, 1, 1, 8, 2)
        );
        let outside = counts_v1(LoadRoleV1::Unattributed);
        assert_eq!(
            (
                outside.scoped_loads,
                outside.cache_lookups,
                outside.cache_misses
            ),
            (0, 1, 1)
        );
        // The inner scope's time is part of the outer one's.
        assert!(learner.scoped_nanos >= opponent.scoped_nanos);
        let report = report_v1();
        assert_eq!(report["schema"], REPORT_SCHEMA_V1);
        assert_eq!(report["roles"]["update_learner"]["cache_hits"], 2);
        assert_eq!(report["roles"]["opponent"]["kept_state_bytes"], 8);
        assert_eq!(report["total"]["cache_lookups"], 4);
        assert_eq!(report["total"]["cache_misses"], 2);
        assert_eq!(
            report["roles"].as_object().unwrap().len(),
            ROLES_V1.len(),
            "every role is reported, zero or not"
        );
        assert!(ROLES_V1
            .iter()
            .enumerate()
            .all(|(index, role)| role.index_v1() == index));
    }
}
