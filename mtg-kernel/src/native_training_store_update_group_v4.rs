//! v4 update-evidence baseline sidecar and validator.
//!
//! Contract: `docs/native_trainer_terminal_reinforce_value_v4_candidate_v1.md`
//! sections 3-4. This is a PURE validation layer over the existing v1
//! evidence wire shape (`native_training_store_update_group_v1`, frozen,
//! never mutated here) plus a new `baseline_v4` sidecar record: per observed
//! cell, the committed baseline `c_t` used by the batch, the successor
//! `c_{t+1}`, the decision-weighted residual sum that derives it, and the
//! counts. [`validate_update_baseline_v4`] recomputes the entire EMA
//! trajectory and the v4 policy sum bit-exactly from persisted evidence
//! terms, exactly as the v1 validator recomputes today's loss at
//! `native_training_store_update_group_v1.rs:2210-2246`.
//!
//! This module owns no `ValidatedTrainRunV2`/evidence-chain/Store
//! cross-binding. Its evidence input is a minimal borrowed view
//! ([`UpdateBaselineEpisodeViewV4`]/[`UpdateBaselineTermViewV4`]), not v1's
//! private wire types: the Store integration layer adapts one already-valid
//! `UpdateEvidenceWireV1` into this view by slicing the flat
//! `physical_terms` list per episode using each episode's
//! `learner_physical_decision_count` (the same cursor walk
//! `validate_physical_and_loss_v1` performs), attaching `learner_seat` and
//! `opponent_checkpoint_manifest_sha256` from the episode record. That
//! adaptation is out of this module's scope; this module only requires that
//! episodes and their terms arrive in the original batch order.

use crate::canonical_json_v1::{
    from_canonical_json_bytes_v1, to_canonical_json_bytes_v1, CanonicalJsonErrorKindV1,
    CanonicalJsonErrorV1, CanonicalJsonNullPolicyV1,
};
use crate::native_policy_baseline_state_v4::{
    BaselineCellKeyV4, BaselineObservationV4, BaselineRoleV4, NativeBaselineErrorKindV4,
    NativeBaselineStateV4, NATIVE_BASELINE_MAX_CELLS_V4,
};
use crate::native_training_store_digest_v1::{
    lower_hex_raw32_v1, parse_lower_hex_raw32_v1, NativeTrainingStoreDigestErrorV1,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};

pub(crate) const UPDATE_BASELINE_SCHEMA_V4: &str = "mtg_kernel_native_train_update_baseline/v1";
/// Conservative cap: 256 cells (the baseline-state cap) at well under 300
/// wire bytes each, plus the fixed top-level fields.
pub(crate) const UPDATE_BASELINE_RECORD_MAX_BYTES_V4: usize = 512 * 1024;

const U63_MAX_V4: u64 = (1_u64 << 63) - 1;

// ---------------------------------------------------------------------
// Wire shape
// ---------------------------------------------------------------------

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct UpdateBaselineCellWireV4 {
    opponent_checkpoint_manifest_sha256: String,
    role: String,
    c_t_f32_bits: String,
    c_next_f32_bits: String,
    residual_sum_f64_bits: String,
    decision_count: u64,
    episode_count: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct UpdateBaselineWireV4 {
    schema: String,
    update_index: u64,
    update_evidence_sha256: String,
    cells: Vec<UpdateBaselineCellWireV4>,
    declared_policy_sum_f32_bits: String,
}

// ---------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum UpdateBaselineV4ErrorKind {
    RecordTooLarge,
    CanonicalJson(CanonicalJsonErrorKindV1),
    InvalidSchema,
    InvalidDigest,
    InvalidScalar,
    InvalidCellKey(NativeBaselineErrorKindV4),
    InvalidCounts,
    CellOrder,
    CellCapExceeded,
    /// A physical term's `terminal_return_i8` disagrees with its episode's
    /// `learner_return`.
    TermReturnMismatch,
    /// The declared per-cell residual sum (f64 bits) does not match the
    /// value recomputed from evidence terms.
    ResidualSumMismatch,
    /// The declared per-cell `decision_count`/`episode_count` does not match
    /// the counts recomputed from evidence terms.
    CountMismatch,
    /// The set of cells recomputed from the evidence episodes does not equal
    /// the set of cells declared in the record (a missing, extra, or
    /// misattributed cell).
    CellSetMismatch,
    /// A declared `c_t_f32_bits` disagrees with `prior_state.c_for_cell_v4`
    /// (strict-lag violation: the record claims a baseline value the batch
    /// could not actually have used).
    StrictLagMismatch,
    /// `NativeBaselineStateV4::apply_update_v4` rejected the recomputed
    /// observations (non-finite residual, duplicate cell, cap exceeded, or
    /// invalid counts).
    BaselineApply(NativeBaselineErrorKindV4),
    /// The declared `c_next_f32_bits` disagrees with the successor state
    /// `apply_update_v4` actually derives.
    CNextMismatch,
    /// The declared `declared_policy_sum_f32_bits` disagrees with the v4
    /// policy sum recomputed in batch order.
    PolicySumMismatch,
    /// The record's `update_index`/`update_evidence_sha256` disagree with
    /// the caller's expected source update (a replayed or mislabeled
    /// sidecar).
    SourceUpdateMismatch,
}

impl UpdateBaselineV4ErrorKind {
    pub(crate) const fn code(self) -> &'static str {
        match self {
            Self::RecordTooLarge => "native_train_update_baseline_v4_record_too_large",
            Self::CanonicalJson(kind) => kind.code(),
            Self::InvalidSchema => "native_train_update_baseline_v4_invalid_schema",
            Self::InvalidDigest => "native_train_update_baseline_v4_invalid_digest",
            Self::InvalidScalar => "native_train_update_baseline_v4_invalid_scalar",
            Self::InvalidCellKey(_) => "native_train_update_baseline_v4_invalid_cell_key",
            Self::InvalidCounts => "native_train_update_baseline_v4_invalid_counts",
            Self::CellOrder => "native_train_update_baseline_v4_cell_order",
            Self::CellCapExceeded => "native_train_update_baseline_v4_cell_cap_exceeded",
            Self::TermReturnMismatch => "native_train_update_baseline_v4_term_return_mismatch",
            Self::ResidualSumMismatch => "native_train_update_baseline_v4_residual_sum_mismatch",
            Self::CountMismatch => "native_train_update_baseline_v4_count_mismatch",
            Self::CellSetMismatch => "native_train_update_baseline_v4_cell_set_mismatch",
            Self::StrictLagMismatch => "native_train_update_baseline_v4_strict_lag_mismatch",
            Self::BaselineApply(_) => "native_train_update_baseline_v4_baseline_apply_rejected",
            Self::CNextMismatch => "native_train_update_baseline_v4_c_next_mismatch",
            Self::PolicySumMismatch => "native_train_update_baseline_v4_policy_sum_mismatch",
            Self::SourceUpdateMismatch => "native_train_update_baseline_v4_source_update_mismatch",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct UpdateBaselineV4Error {
    kind: UpdateBaselineV4ErrorKind,
}

impl UpdateBaselineV4Error {
    const fn new(kind: UpdateBaselineV4ErrorKind) -> Self {
        Self { kind }
    }

    pub(crate) const fn kind(self) -> UpdateBaselineV4ErrorKind {
        self.kind
    }

    pub(crate) const fn code(self) -> &'static str {
        self.kind.code()
    }
}

impl From<CanonicalJsonErrorV1> for UpdateBaselineV4Error {
    fn from(error: CanonicalJsonErrorV1) -> Self {
        Self::new(UpdateBaselineV4ErrorKind::CanonicalJson(error.kind()))
    }
}

impl Display for UpdateBaselineV4Error {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.code())
    }
}

impl Error for UpdateBaselineV4Error {}

type Result<T> = std::result::Result<T, UpdateBaselineV4Error>;

const fn error_v4(kind: UpdateBaselineV4ErrorKind) -> UpdateBaselineV4Error {
    UpdateBaselineV4Error::new(kind)
}

// ---------------------------------------------------------------------
// Parsed record (post canonical-JSON decode and structural validation)
// ---------------------------------------------------------------------

/// One parsed, structurally validated `baseline_v4` cell: identity/role
/// resolved to a [`BaselineCellKeyV4`] (validating hex-identity format and
/// role spelling), bit-pattern fields parsed and finite.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct UpdateBaselineCellRecordV4 {
    key: BaselineCellKeyV4,
    c_t_bits: u32,
    c_next_bits: u32,
    residual_sum_f64: f64,
    decision_count: u64,
    episode_count: u64,
}

impl UpdateBaselineCellRecordV4 {
    pub(crate) const fn key(&self) -> &BaselineCellKeyV4 {
        &self.key
    }

    pub(crate) const fn c_t_bits(&self) -> u32 {
        self.c_t_bits
    }

    pub(crate) const fn c_next_bits(&self) -> u32 {
        self.c_next_bits
    }

    pub(crate) const fn residual_sum_f64(&self) -> f64 {
        self.residual_sum_f64
    }

    pub(crate) const fn decision_count(&self) -> u64 {
        self.decision_count
    }

    pub(crate) const fn episode_count(&self) -> u64 {
        self.episode_count
    }
}

/// Fully decoded, structurally validated `baseline_v4` sidecar record. No
/// public fields, no `Deserialize`, no unchecked constructor -- only
/// [`build_update_baseline_record_v4`] and [`decode_update_baseline_record_v4`]
/// can produce one. Structural validation only: schema, canonical-JSON
/// round trip, digest/hex formats, cell order/uniqueness/cap, and count
/// well-formedness. Cross-checking against evidence and the prior baseline
/// state is [`validate_update_baseline_v4`]'s job.
#[derive(Clone, Debug)]
pub(crate) struct UpdateBaselineRecordV4 {
    canonical_bytes: Vec<u8>,
    update_index: u64,
    update_evidence_sha256: [u8; 32],
    cells: Vec<UpdateBaselineCellRecordV4>,
    declared_policy_sum_bits: u32,
}

impl UpdateBaselineRecordV4 {
    pub(crate) fn canonical_bytes(&self) -> &[u8] {
        &self.canonical_bytes
    }

    pub(crate) const fn update_index(&self) -> u64 {
        self.update_index
    }

    pub(crate) const fn update_evidence_sha256(&self) -> [u8; 32] {
        self.update_evidence_sha256
    }

    pub(crate) fn cells(&self) -> &[UpdateBaselineCellRecordV4] {
        &self.cells
    }

    pub(crate) const fn declared_policy_sum_bits(&self) -> u32 {
        self.declared_policy_sum_bits
    }
}

/// Inputs to [`build_update_baseline_record_v4`]: the raw parts, pre-JSON.
#[derive(Clone, Debug)]
pub(crate) struct UpdateBaselineCellPartsV4 {
    pub(crate) opponent_checkpoint_manifest_sha256: String,
    pub(crate) role: BaselineRoleV4,
    pub(crate) c_t_bits: u32,
    pub(crate) c_next_bits: u32,
    pub(crate) residual_sum_f64: f64,
    pub(crate) decision_count: u64,
    pub(crate) episode_count: u64,
}

#[derive(Clone, Debug)]
pub(crate) struct UpdateBaselineRecordPartsV4 {
    pub(crate) update_index: u64,
    pub(crate) update_evidence_sha256: [u8; 32],
    pub(crate) cells: Vec<UpdateBaselineCellPartsV4>,
    pub(crate) declared_policy_sum_bits: u32,
}

fn build_wire_v4(parts: &UpdateBaselineRecordPartsV4) -> UpdateBaselineWireV4 {
    UpdateBaselineWireV4 {
        schema: UPDATE_BASELINE_SCHEMA_V4.to_owned(),
        update_index: parts.update_index,
        update_evidence_sha256: lower_hex_raw32_v1(parts.update_evidence_sha256),
        cells: parts
            .cells
            .iter()
            .map(|cell| UpdateBaselineCellWireV4 {
                opponent_checkpoint_manifest_sha256: cell
                    .opponent_checkpoint_manifest_sha256
                    .clone(),
                role: cell.role.wire_v4().to_owned(),
                c_t_f32_bits: format!("{:08x}", cell.c_t_bits),
                c_next_f32_bits: format!("{:08x}", cell.c_next_bits),
                residual_sum_f64_bits: format!("{:016x}", cell.residual_sum_f64.to_bits()),
                decision_count: cell.decision_count,
                episode_count: cell.episode_count,
            })
            .collect(),
        declared_policy_sum_f32_bits: format!("{:08x}", parts.declared_policy_sum_bits),
    }
}

/// Builds a `baseline_v4` record from its parts, canonically encodes it,
/// then immediately round-trips it through [`decode_update_baseline_record_v4`]
/// so a built record can never diverge from what its own decoder accepts.
pub(crate) fn build_update_baseline_record_v4(
    parts: UpdateBaselineRecordPartsV4,
) -> Result<UpdateBaselineRecordV4> {
    let wire = build_wire_v4(&parts);
    let canonical_bytes = to_canonical_json_bytes_v1(&wire, CanonicalJsonNullPolicyV1::Forbid)?;
    decode_update_baseline_record_v4(&canonical_bytes)
}

/// Decodes and structurally validates a `baseline_v4` record: size cap,
/// canonical-JSON round trip (byte-identical re-encode), schema string,
/// digest/hex formats, strict ascending `(identity, role)` cell order with
/// no duplicates, the 256-cell cap, and count well-formedness
/// (`decision_count >= 1`, `episode_count >= 1`, `decision_count >=
/// episode_count`, mirroring `NativeBaselineStateV4::from_wire_v4`).
pub(crate) fn decode_update_baseline_record_v4(record_cj: &[u8]) -> Result<UpdateBaselineRecordV4> {
    if record_cj.len() > UPDATE_BASELINE_RECORD_MAX_BYTES_V4 {
        return Err(error_v4(UpdateBaselineV4ErrorKind::RecordTooLarge));
    }
    let wire: UpdateBaselineWireV4 =
        from_canonical_json_bytes_v1(record_cj, CanonicalJsonNullPolicyV1::Forbid)?;
    let reencoded = to_canonical_json_bytes_v1(&wire, CanonicalJsonNullPolicyV1::Forbid)?;
    if reencoded != record_cj {
        return Err(error_v4(UpdateBaselineV4ErrorKind::CanonicalJson(
            CanonicalJsonErrorKindV1::NonCanonicalBytes,
        )));
    }
    if wire.schema != UPDATE_BASELINE_SCHEMA_V4 {
        return Err(error_v4(UpdateBaselineV4ErrorKind::InvalidSchema));
    }
    if !is_u63_v4(wire.update_index) {
        return Err(error_v4(UpdateBaselineV4ErrorKind::InvalidScalar));
    }
    let update_evidence_sha256 = parse_digest_v4(&wire.update_evidence_sha256)?;
    let declared_policy_sum_bits = parse_f32_hex_v4(&wire.declared_policy_sum_f32_bits)?;

    if wire.cells.len() > NATIVE_BASELINE_MAX_CELLS_V4 {
        return Err(error_v4(UpdateBaselineV4ErrorKind::CellCapExceeded));
    }
    let mut cells = Vec::with_capacity(wire.cells.len());
    let mut previous: Option<BaselineCellKeyV4> = None;
    for entry in &wire.cells {
        let role = BaselineRoleV4::from_wire_v4(&entry.role).map_err(|error| {
            error_v4(UpdateBaselineV4ErrorKind::InvalidCellKey(error.kind_v4()))
        })?;
        let key = BaselineCellKeyV4::new_v4(&entry.opponent_checkpoint_manifest_sha256, role)
            .map_err(|error| {
                error_v4(UpdateBaselineV4ErrorKind::InvalidCellKey(error.kind_v4()))
            })?;
        if let Some(previous_key) = &previous {
            if *previous_key >= key {
                return Err(error_v4(UpdateBaselineV4ErrorKind::CellOrder));
            }
        }
        if entry.decision_count == 0
            || entry.episode_count == 0
            || entry.decision_count < entry.episode_count
        {
            return Err(error_v4(UpdateBaselineV4ErrorKind::InvalidCounts));
        }
        let c_t_bits = parse_f32_hex_v4(&entry.c_t_f32_bits)?;
        let c_next_bits = parse_f32_hex_v4(&entry.c_next_f32_bits)?;
        let residual_sum_f64 = parse_f64_hex_v4(&entry.residual_sum_f64_bits)?;
        previous = Some(key.clone());
        cells.push(UpdateBaselineCellRecordV4 {
            key,
            c_t_bits,
            c_next_bits,
            residual_sum_f64,
            decision_count: entry.decision_count,
            episode_count: entry.episode_count,
        });
    }

    Ok(UpdateBaselineRecordV4 {
        canonical_bytes: reencoded,
        update_index: wire.update_index,
        update_evidence_sha256,
        cells,
        declared_policy_sum_bits,
    })
}

// ---------------------------------------------------------------------
// Evidence view (store integration adapts v1 evidence into this)
// ---------------------------------------------------------------------

/// One learner physical term's evidence, exactly as persisted by v1: the
/// pre-update transported joint log-probability and value bits, and the
/// terminal return the term declares (checked against its episode's
/// `learner_return`).
#[derive(Clone, Copy, Debug)]
pub(crate) struct UpdateBaselineTermViewV4 {
    pub(crate) joint_log_probability_f32_bits: u32,
    pub(crate) value_f32_bits: u32,
    pub(crate) terminal_return_i8: i8,
}

/// One episode's evidence, exactly as needed to label and fold its terms
/// into a cell: the cell key components (`learner_seat`, opponent identity)
/// plus `learner_return` (checked against every term) and the episode's
/// slice of learner physical terms, in original batch order. The Store
/// integration layer builds this slice by walking v1's flat
/// `physical_terms` list `learner_physical_decision_count` terms at a time
/// per episode, in episode order -- the same cursor walk
/// `validate_physical_and_loss_v1` performs.
#[derive(Clone, Copy, Debug)]
pub(crate) struct UpdateBaselineEpisodeViewV4<'a> {
    pub(crate) learner_seat: BaselineRoleV4,
    pub(crate) learner_return: i8,
    pub(crate) opponent_checkpoint_manifest_sha256: &'a str,
    pub(crate) terms: &'a [UpdateBaselineTermViewV4],
}

// ---------------------------------------------------------------------
// Sidecar source (round A: an injected pure source; the launcher-level
// chain-directory reader is round B's job -- see
// `docs/native_cycle4_arm_launcher_v1.md` Sections 2-3)
// ---------------------------------------------------------------------

/// Supplies the raw canonical-JSON bytes of the `baseline_v4` sidecar record
/// for one update index (`docs/native_cycle4_arm_launcher_v1.md` Section 2:
/// `baseline-update-<8-digit index>.record.json`, published atomically
/// right after the Store commits that update and before the next one
/// begins). `None` means no sidecar was found for that index -- the
/// evidence-dispatch caller (`native_training_store_update_group_v1`) fails
/// closed on that, exactly as the doc's "a missing or unbound sidecar fails
/// closed" requires.
///
/// This module, and everything downstream of it, stay pure over the bytes
/// an implementation returns: no file I/O happens in the validation path
/// itself. A production implementation (round B, the launcher-level chain
/// reader) reads the sidecar file for `update_index` and returns its bytes;
/// tests use the blanket closure impl below.
pub(crate) trait BaselineSidecarSourceV4 {
    fn sidecar_record_bytes_v4(&self, update_index: u64) -> Option<Vec<u8>>;
}

impl<F: Fn(u64) -> Option<Vec<u8>>> BaselineSidecarSourceV4 for F {
    fn sidecar_record_bytes_v4(&self, update_index: u64) -> Option<Vec<u8>> {
        self(update_index)
    }
}

/// Round B: the launcher-level chain-directory access threaded through the
/// Store's publish and resume paths so a `trainer_v4_candidate` run's
/// evidence validation can dispatch to the v4 recompute
/// (`docs/native_cycle4_arm_launcher_v1.md` Sections 2-3). Every frozen v3
/// path passes `None` for this and therefore behaves byte for byte as it
/// did before; only a run that declares the v4 trainer ever reaches an
/// implementation.
///
/// The access is deliberately STATELESS with respect to the walk order: a
/// caller asks for the committed state at a checkpoint boundary generation
/// and then folds forward through the in-boundary sidecars itself, so the
/// same segment can be validated any number of times (the publisher
/// revalidates each generation more than once) without a running state
/// drifting.
pub(crate) trait BaselineChainAccessV4: BaselineSidecarSourceV4 {
    /// The committed baseline state at one Store checkpoint boundary
    /// generation. Generation 0 is the pre-training genesis state and is
    /// always the empty state. `None` fails the caller closed.
    fn committed_state_for_generation_v4(
        &self,
        generation_index: u64,
    ) -> Option<NativeBaselineStateV4>;

    /// Registers the Store's own core train-state SHA-256 for one already
    /// validated checkpoint boundary. The Store walk calls this for every
    /// generation it proves, so a launcher-level chain record is only ever
    /// decoded against a hash the Store itself authenticated, never against
    /// a caller's claim. Implementations that need no such registration
    /// leave the default no-op.
    fn observe_store_checkpoint_v4(&self, _generation_index: u64, _core_state_sha256: [u8; 32]) {}

    /// Publishes the per-update sidecar record for `update_index` atomically
    /// into the chain directory. Producer-only: every validation path leaves
    /// this untouched. Returns `false` on any failure so the caller fails
    /// closed rather than continuing with an unpublished sidecar.
    fn publish_sidecar_record_v4(&self, update_index: u64, record_bytes: &[u8]) -> bool;
}

/// Producer-side sibling of [`validate_update_baseline_v4`]: mints the
/// `baseline_v4` sidecar record for one just-built update group from the
/// SAME persisted-evidence walk the validator uses, so the record the
/// producer publishes is by construction the record the validator
/// recomputes.
///
/// `declared_policy_sum_bits` is the evidence's own
/// `loss.policy_sum_f32_bits`; the recomputed v4 policy sum must equal it
/// bit for bit, otherwise the device did not optimize what the evidence
/// declares and the update fails closed here rather than at the next
/// validation.
pub(crate) fn build_update_baseline_record_from_episodes_v4(
    episodes: &[UpdateBaselineEpisodeViewV4<'_>],
    prior_state: &NativeBaselineStateV4,
    update_index: u64,
    update_evidence_sha256: [u8; 32],
    declared_policy_sum_bits: u32,
) -> Result<UpdateBaselineRecordV4> {
    let mut accumulator: BTreeMap<BaselineCellKeyV4, (f64, u64, u64)> = BTreeMap::new();
    let mut policy_sum = 0.0_f32;
    for episode in episodes {
        let key = BaselineCellKeyV4::new_v4(
            episode.opponent_checkpoint_manifest_sha256,
            episode.learner_seat,
        )
        .map_err(|error| error_v4(UpdateBaselineV4ErrorKind::InvalidCellKey(error.kind_v4())))?;
        let c_t = prior_state.c_for_cell_v4(&key);
        let entry = accumulator.entry(key).or_insert((0.0_f64, 0_u64, 0_u64));
        entry.2 = entry
            .2
            .checked_add(1)
            .ok_or_else(|| error_v4(UpdateBaselineV4ErrorKind::InvalidCounts))?;
        for term in episode.terms {
            if term.terminal_return_i8 != episode.learner_return {
                return Err(error_v4(UpdateBaselineV4ErrorKind::TermReturnMismatch));
            }
            let q = f32::from_bits(term.joint_log_probability_f32_bits);
            let value = f32::from_bits(term.value_f32_bits);
            if !q.is_finite() || !value.is_finite() {
                return Err(error_v4(UpdateBaselineV4ErrorKind::InvalidScalar));
            }
            let target = f32::from(term.terminal_return_i8);
            let residual = target - value;
            let advantage = residual - c_t;
            let policy_term = (-q) * advantage;
            policy_sum += policy_term;
            if !residual.is_finite()
                || !advantage.is_finite()
                || !policy_term.is_finite()
                || !policy_sum.is_finite()
            {
                return Err(error_v4(UpdateBaselineV4ErrorKind::InvalidScalar));
            }
            entry.0 += f64::from(residual);
            entry.1 = entry
                .1
                .checked_add(1)
                .ok_or_else(|| error_v4(UpdateBaselineV4ErrorKind::InvalidCounts))?;
            if !entry.0.is_finite() {
                return Err(error_v4(UpdateBaselineV4ErrorKind::InvalidScalar));
            }
        }
    }
    if policy_sum.to_bits() != declared_policy_sum_bits {
        return Err(error_v4(UpdateBaselineV4ErrorKind::PolicySumMismatch));
    }
    let observations = accumulator
        .iter()
        .map(
            |(key, (residual_sum, decision_count, episode_count))| BaselineObservationV4 {
                key: key.clone(),
                residual_sum_f64: *residual_sum,
                decision_count: *decision_count,
                episode_count: *episode_count,
            },
        )
        .collect::<Vec<_>>();
    let successor = prior_state
        .apply_update_v4(&observations)
        .map_err(|error| error_v4(UpdateBaselineV4ErrorKind::BaselineApply(error.kind_v4())))?;
    let cells = observations
        .iter()
        .map(|observation| UpdateBaselineCellPartsV4 {
            opponent_checkpoint_manifest_sha256: observation
                .key
                .opponent_checkpoint_manifest_sha256
                .clone(),
            role: observation.key.role,
            c_t_bits: prior_state.c_for_cell_v4(&observation.key).to_bits(),
            c_next_bits: successor.c_for_cell_v4(&observation.key).to_bits(),
            residual_sum_f64: observation.residual_sum_f64,
            decision_count: observation.decision_count,
            episode_count: observation.episode_count,
        })
        .collect::<Vec<_>>();
    build_update_baseline_record_v4(UpdateBaselineRecordPartsV4 {
        update_index,
        update_evidence_sha256,
        cells,
        declared_policy_sum_bits,
    })
}

// ---------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------

/// Validates one `baseline_v4` record against its evidence and the prior
/// committed baseline state, and returns the successor state on success.
///
/// Contract section 3 transaction, points (a)-(d), enforced in this order:
///
/// (a) Recomputes each cell's residual sum and counts from `episodes` by
///     walking them in batch order (episode, then term within episode) and
///     folding each term into the cell `(opponent_checkpoint_manifest_sha256,
///     learner_seat)` of its episode; requires the recomputed set of cells,
///     residual-sum bits, and counts to exactly equal the record's declared
///     set/bits/counts (a missing, extra, or swapped cell fails closed here).
/// (b) Requires every declared `c_t_f32_bits` to equal
///     `prior_state.c_for_cell_v4` bit-for-bit (strict lag: the record
///     cannot claim a baseline value the batch could not have used).
/// (c) Recomputes the successor state via `NativeBaselineStateV4::apply_update_v4`
///     from the recomputed observations and requires every declared
///     `c_next_f32_bits` to match bit-for-bit.
/// (d) Recomputes the v4 policy sum in the same batch-order pass --
///     `policy_term = (-q) * ((target - value) - c_t)` per term, f32 --
///     and requires bit-equality with `declared_policy_sum_f32_bits`.
///
/// Returns the freshly recomputed successor state (never the record's own
/// declared cells), so a caller can never be handed a state that rode
/// through on a merely-declared-consistent record.
pub(crate) fn validate_update_baseline_v4(
    episodes: &[UpdateBaselineEpisodeViewV4<'_>],
    record: &UpdateBaselineRecordV4,
    prior_state: &NativeBaselineStateV4,
    expected_update_index: u64,
    expected_update_evidence_sha256: [u8; 32],
) -> Result<NativeBaselineStateV4> {
    // Source-update binding (review finding P1): a record replayed against a
    // different update, even one with identical terms and prior state, must
    // fail closed here rather than returning a successor.
    if record.update_index() != expected_update_index
        || record.update_evidence_sha256() != expected_update_evidence_sha256
    {
        return Err(error_v4(UpdateBaselineV4ErrorKind::SourceUpdateMismatch));
    }
    // (b) Strict lag: every declared c_t must equal the prior committed
    // value for that cell (0.0 for a cell the prior state never observed).
    for cell in &record.cells {
        if prior_state.c_for_cell_v4(cell.key()).to_bits() != cell.c_t_bits() {
            return Err(error_v4(UpdateBaselineV4ErrorKind::StrictLagMismatch));
        }
    }

    // (a) Walk episodes/terms in batch order once, folding per-cell residual
    // sums and counts, and (d) accumulating the global v4 policy sum in the
    // same pass (batch order, independent of cell grouping).
    let mut accumulator: BTreeMap<BaselineCellKeyV4, (f64, u64, u64)> = BTreeMap::new();
    let mut policy_sum = 0.0_f32;
    for episode in episodes {
        let key = BaselineCellKeyV4::new_v4(
            episode.opponent_checkpoint_manifest_sha256,
            episode.learner_seat,
        )
        .map_err(|error| error_v4(UpdateBaselineV4ErrorKind::InvalidCellKey(error.kind_v4())))?;
        let c_t = prior_state.c_for_cell_v4(&key);
        let entry = accumulator.entry(key).or_insert((0.0_f64, 0_u64, 0_u64));
        entry.2 = entry
            .2
            .checked_add(1)
            .ok_or_else(|| error_v4(UpdateBaselineV4ErrorKind::InvalidCounts))?;
        for term in episode.terms {
            if term.terminal_return_i8 != episode.learner_return {
                return Err(error_v4(UpdateBaselineV4ErrorKind::TermReturnMismatch));
            }
            let q = f32::from_bits(term.joint_log_probability_f32_bits);
            let value = f32::from_bits(term.value_f32_bits);
            if !q.is_finite() || !value.is_finite() {
                return Err(error_v4(UpdateBaselineV4ErrorKind::InvalidScalar));
            }
            let target = f32::from(term.terminal_return_i8);
            let residual = target - value;
            let advantage = residual - c_t;
            let policy_term = (-q) * advantage;
            policy_sum += policy_term;
            if !residual.is_finite()
                || !advantage.is_finite()
                || !policy_term.is_finite()
                || !policy_sum.is_finite()
            {
                return Err(error_v4(UpdateBaselineV4ErrorKind::InvalidScalar));
            }
            entry.0 += f64::from(residual);
            entry.1 = entry
                .1
                .checked_add(1)
                .ok_or_else(|| error_v4(UpdateBaselineV4ErrorKind::InvalidCounts))?;
            if !entry.0.is_finite() {
                return Err(error_v4(UpdateBaselineV4ErrorKind::InvalidScalar));
            }
        }
    }

    // (a), continued: the recomputed cell set must exactly equal the
    // declared cell set, and per-cell bits/counts must match exactly.
    if accumulator.len() != record.cells.len() {
        return Err(error_v4(UpdateBaselineV4ErrorKind::CellSetMismatch));
    }
    let mut observations = Vec::with_capacity(record.cells.len());
    for cell in &record.cells {
        let (residual_sum, decision_count, episode_count) = accumulator
            .get(cell.key())
            .copied()
            .ok_or_else(|| error_v4(UpdateBaselineV4ErrorKind::CellSetMismatch))?;
        if residual_sum.to_bits() != cell.residual_sum_f64().to_bits() {
            return Err(error_v4(UpdateBaselineV4ErrorKind::ResidualSumMismatch));
        }
        if decision_count != cell.decision_count() || episode_count != cell.episode_count() {
            return Err(error_v4(UpdateBaselineV4ErrorKind::CountMismatch));
        }
        observations.push(BaselineObservationV4 {
            key: cell.key().clone(),
            residual_sum_f64: residual_sum,
            decision_count,
            episode_count,
        });
    }

    // (c) Recompute the successor state end to end and require exact match.
    let successor = prior_state
        .apply_update_v4(&observations)
        .map_err(|error| error_v4(UpdateBaselineV4ErrorKind::BaselineApply(error.kind_v4())))?;
    for cell in &record.cells {
        if successor.c_for_cell_v4(cell.key()).to_bits() != cell.c_next_bits() {
            return Err(error_v4(UpdateBaselineV4ErrorKind::CNextMismatch));
        }
    }

    // (d) The v4 policy sum.
    if policy_sum.to_bits() != record.declared_policy_sum_bits {
        return Err(error_v4(UpdateBaselineV4ErrorKind::PolicySumMismatch));
    }

    Ok(successor)
}

// ---------------------------------------------------------------------
// Parsing helpers (self-contained: v1's equivalents are private to that
// module and never exposed across the frozen boundary).
// ---------------------------------------------------------------------

const fn is_u63_v4(value: u64) -> bool {
    value <= U63_MAX_V4
}

fn parse_digest_v4(value: &str) -> Result<[u8; 32]> {
    parse_lower_hex_raw32_v1(value).map_err(map_digest_error_v4)
}

fn map_digest_error_v4(_error: NativeTrainingStoreDigestErrorV1) -> UpdateBaselineV4Error {
    error_v4(UpdateBaselineV4ErrorKind::InvalidDigest)
}

fn parse_f32_hex_v4(value: &str) -> Result<u32> {
    let bits = parse_fixed_lower_hex_v4(value, 8)?;
    let bits =
        u32::try_from(bits).map_err(|_| error_v4(UpdateBaselineV4ErrorKind::InvalidScalar))?;
    if !f32::from_bits(bits).is_finite() {
        return Err(error_v4(UpdateBaselineV4ErrorKind::InvalidScalar));
    }
    Ok(bits)
}

fn parse_f64_hex_v4(value: &str) -> Result<f64> {
    let bits = parse_fixed_lower_hex_v4(value, 16)?;
    let decoded = f64::from_bits(bits);
    if !decoded.is_finite() {
        return Err(error_v4(UpdateBaselineV4ErrorKind::InvalidScalar));
    }
    Ok(decoded)
}

fn parse_fixed_lower_hex_v4(value: &str, expected_len: usize) -> Result<u64> {
    if value.len() != expected_len
        || !value
            .as_bytes()
            .iter()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
    {
        return Err(error_v4(UpdateBaselineV4ErrorKind::InvalidScalar));
    }
    u64::from_str_radix(value, 16).map_err(|_| error_v4(UpdateBaselineV4ErrorKind::InvalidScalar))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_digest_v4(tag: u8) -> String {
        format!("{tag:02x}").repeat(32)
    }

    fn fake_digest_bytes_v4(tag: u8) -> [u8; 32] {
        [tag; 32]
    }

    /// Two-episode/two-cell synthetic batch: episode 0 is cell
    /// `(digest(1), P0)` with two terms, episode 1 is cell `(digest(2), P1)`
    /// with one term. Both cells are genuinely new (prior state empty).
    fn sample_episodes_v4() -> (Vec<UpdateBaselineTermViewV4>, Vec<UpdateBaselineTermViewV4>) {
        let episode0_terms = vec![
            UpdateBaselineTermViewV4 {
                joint_log_probability_f32_bits: (-0.5_f32).to_bits(),
                value_f32_bits: 0.2_f32.to_bits(),
                terminal_return_i8: 1,
            },
            UpdateBaselineTermViewV4 {
                joint_log_probability_f32_bits: (-0.25_f32).to_bits(),
                value_f32_bits: 0.4_f32.to_bits(),
                terminal_return_i8: 1,
            },
        ];
        let episode1_terms = vec![UpdateBaselineTermViewV4 {
            joint_log_probability_f32_bits: (-1.0_f32).to_bits(),
            value_f32_bits: 0.1_f32.to_bits(),
            terminal_return_i8: -1,
        }];
        (episode0_terms, episode1_terms)
    }

    fn sample_views_v4<'a>(
        episode0_terms: &'a [UpdateBaselineTermViewV4],
        episode1_terms: &'a [UpdateBaselineTermViewV4],
        digest0: &'a str,
        digest1: &'a str,
    ) -> Vec<UpdateBaselineEpisodeViewV4<'a>> {
        vec![
            UpdateBaselineEpisodeViewV4 {
                learner_seat: BaselineRoleV4::P0,
                learner_return: 1,
                opponent_checkpoint_manifest_sha256: digest0,
                terms: episode0_terms,
            },
            UpdateBaselineEpisodeViewV4 {
                learner_seat: BaselineRoleV4::P1,
                learner_return: -1,
                opponent_checkpoint_manifest_sha256: digest1,
                terms: episode1_terms,
            },
        ]
    }

    /// Ground-truth computation mirroring the module under test, used only
    /// to build a genuinely-correct record for the tests to tamper with.
    fn expected_observations_and_policy_sum_v4(
        episodes: &[UpdateBaselineEpisodeViewV4<'_>],
        prior_state: &NativeBaselineStateV4,
    ) -> (Vec<BaselineObservationV4>, f32) {
        let mut accumulator: BTreeMap<BaselineCellKeyV4, (f64, u64, u64)> = BTreeMap::new();
        let mut policy_sum = 0.0_f32;
        for episode in episodes {
            let key = BaselineCellKeyV4::new_v4(
                episode.opponent_checkpoint_manifest_sha256,
                episode.learner_seat,
            )
            .expect("key");
            let c_t = prior_state.c_for_cell_v4(&key);
            let entry = accumulator.entry(key).or_insert((0.0_f64, 0_u64, 0_u64));
            entry.2 += 1;
            for term in episode.terms {
                let q = f32::from_bits(term.joint_log_probability_f32_bits);
                let value = f32::from_bits(term.value_f32_bits);
                let target = f32::from(term.terminal_return_i8);
                let residual = target - value;
                let advantage = residual - c_t;
                policy_sum += (-q) * advantage;
                entry.0 += f64::from(residual);
                entry.1 += 1;
            }
        }
        let observations = accumulator
            .into_iter()
            .map(
                |(key, (residual_sum_f64, decision_count, episode_count))| BaselineObservationV4 {
                    key,
                    residual_sum_f64,
                    decision_count,
                    episode_count,
                },
            )
            .collect();
        (observations, policy_sum)
    }

    fn build_record_from_truth_v4(
        prior_state: &NativeBaselineStateV4,
        observations: &[BaselineObservationV4],
        policy_sum: f32,
        successor: &NativeBaselineStateV4,
    ) -> UpdateBaselineRecordV4 {
        let cells = observations
            .iter()
            .map(|observation| UpdateBaselineCellPartsV4 {
                opponent_checkpoint_manifest_sha256: observation
                    .key
                    .opponent_checkpoint_manifest_sha256
                    .clone(),
                role: observation.key.role,
                c_t_bits: prior_state.c_for_cell_v4(&observation.key).to_bits(),
                c_next_bits: successor.c_for_cell_v4(&observation.key).to_bits(),
                residual_sum_f64: observation.residual_sum_f64,
                decision_count: observation.decision_count,
                episode_count: observation.episode_count,
            })
            .collect();
        build_update_baseline_record_v4(UpdateBaselineRecordPartsV4 {
            update_index: 7,
            update_evidence_sha256: fake_digest_bytes_v4(0xab),
            cells,
            declared_policy_sum_bits: policy_sum.to_bits(),
        })
        .expect("build record")
    }

    #[test]
    fn end_to_end_two_episode_two_cell_batch_validates_v4() {
        let (episode0_terms, episode1_terms) = sample_episodes_v4();
        let digest0 = fake_digest_v4(1);
        let digest1 = fake_digest_v4(2);
        let episodes = sample_views_v4(&episode0_terms, &episode1_terms, &digest0, &digest1);
        let prior_state = NativeBaselineStateV4::empty_v4();
        let (observations, policy_sum) =
            expected_observations_and_policy_sum_v4(&episodes, &prior_state);
        let successor = prior_state.apply_update_v4(&observations).expect("apply");
        let record =
            build_record_from_truth_v4(&prior_state, &observations, policy_sum, &successor);

        let result = validate_update_baseline_v4(
            &episodes,
            &record,
            &prior_state,
            record.update_index(),
            record.update_evidence_sha256(),
        )
        .expect("valid");
        assert_eq!(result, successor);
        assert_eq!(result.cell_count_v4(), 2);
    }

    #[test]
    fn genesis_update_from_empty_prior_state_validates_v4() {
        // Every cell in this batch is genuinely new: prior_state is empty,
        // so every c_t must be exactly 0.0 (checked by the build/validate
        // round trip below).
        let (episode0_terms, episode1_terms) = sample_episodes_v4();
        let digest0 = fake_digest_v4(3);
        let digest1 = fake_digest_v4(4);
        let episodes = sample_views_v4(&episode0_terms, &episode1_terms, &digest0, &digest1);
        let prior_state = NativeBaselineStateV4::empty_v4();
        let (observations, policy_sum) =
            expected_observations_and_policy_sum_v4(&episodes, &prior_state);
        let successor = prior_state.apply_update_v4(&observations).expect("apply");
        let record =
            build_record_from_truth_v4(&prior_state, &observations, policy_sum, &successor);

        for cell in record.cells() {
            assert_eq!(cell.c_t_bits(), 0.0_f32.to_bits());
        }
        let result = validate_update_baseline_v4(
            &episodes,
            &record,
            &prior_state,
            record.update_index(),
            record.update_evidence_sha256(),
        )
        .expect("genesis valid");
        assert_eq!(result.cell_count_v4(), 2);
    }

    #[test]
    fn determinism_two_runs_identical_successor_bytes_v4() {
        let (episode0_terms, episode1_terms) = sample_episodes_v4();
        let digest0 = fake_digest_v4(5);
        let digest1 = fake_digest_v4(6);
        let episodes = sample_views_v4(&episode0_terms, &episode1_terms, &digest0, &digest1);
        let prior_state = NativeBaselineStateV4::empty_v4();
        let (observations, policy_sum) =
            expected_observations_and_policy_sum_v4(&episodes, &prior_state);
        let successor = prior_state.apply_update_v4(&observations).expect("apply");
        let record =
            build_record_from_truth_v4(&prior_state, &observations, policy_sum, &successor);

        let first = validate_update_baseline_v4(
            &episodes,
            &record,
            &prior_state,
            record.update_index(),
            record.update_evidence_sha256(),
        )
        .expect("first");
        let second = validate_update_baseline_v4(
            &episodes,
            &record,
            &prior_state,
            record.update_index(),
            record.update_evidence_sha256(),
        )
        .expect("second");
        assert_eq!(first.canonical_bytes_v4(), second.canonical_bytes_v4());
        let core = [0x11_u8; 32];
        assert_eq!(
            first.compose_train_state_sha256_v4(core),
            second.compose_train_state_sha256_v4(core)
        );
    }

    struct TamperFixtureV4 {
        episodes: Vec<UpdateBaselineEpisodeViewV4<'static>>,
        prior_state: NativeBaselineStateV4,
        record: UpdateBaselineRecordV4,
    }

    /// Leaks the episode term vectors and digest strings so the fixture can
    /// hand out `'static` borrows; test-only, bounded by the fixed small
    /// fixture size used across the tamper-class tests.
    fn tamper_fixture_v4() -> TamperFixtureV4 {
        let (episode0_terms, episode1_terms) = sample_episodes_v4();
        let episode0_terms: &'static [UpdateBaselineTermViewV4] =
            Box::leak(episode0_terms.into_boxed_slice());
        let episode1_terms: &'static [UpdateBaselineTermViewV4] =
            Box::leak(episode1_terms.into_boxed_slice());
        let digest0: &'static str = Box::leak(fake_digest_v4(21).into_boxed_str());
        let digest1: &'static str = Box::leak(fake_digest_v4(22).into_boxed_str());
        let episodes = sample_views_v4(episode0_terms, episode1_terms, digest0, digest1);
        let prior_state = NativeBaselineStateV4::empty_v4();
        let (observations, policy_sum) =
            expected_observations_and_policy_sum_v4(&episodes, &prior_state);
        let successor = prior_state.apply_update_v4(&observations).expect("apply");
        let record =
            build_record_from_truth_v4(&prior_state, &observations, policy_sum, &successor);
        TamperFixtureV4 {
            episodes,
            prior_state,
            record,
        }
    }

    fn tamper_cell_v4(
        record: &UpdateBaselineRecordV4,
        index: usize,
        mutate: impl FnOnce(&mut UpdateBaselineCellPartsV4),
    ) -> UpdateBaselineRecordV4 {
        let mut cells: Vec<UpdateBaselineCellPartsV4> = record
            .cells()
            .iter()
            .map(|cell| UpdateBaselineCellPartsV4 {
                opponent_checkpoint_manifest_sha256: cell
                    .key()
                    .opponent_checkpoint_manifest_sha256
                    .clone(),
                role: cell.key().role,
                c_t_bits: cell.c_t_bits(),
                c_next_bits: cell.c_next_bits(),
                residual_sum_f64: cell.residual_sum_f64(),
                decision_count: cell.decision_count(),
                episode_count: cell.episode_count(),
            })
            .collect();
        mutate(&mut cells[index]);
        build_update_baseline_record_v4(UpdateBaselineRecordPartsV4 {
            update_index: record.update_index(),
            update_evidence_sha256: record.update_evidence_sha256(),
            cells,
            declared_policy_sum_bits: record.declared_policy_sum_bits(),
        })
        .expect("build tampered record")
    }

    #[test]
    fn wrong_residual_bits_fails_closed_v4() {
        let fixture = tamper_fixture_v4();
        let tampered = tamper_cell_v4(&fixture.record, 0, |cell| {
            cell.residual_sum_f64 += 1.0;
        });
        let error = validate_update_baseline_v4(
            &fixture.episodes,
            &tampered,
            &fixture.prior_state,
            tampered.update_index(),
            tampered.update_evidence_sha256(),
        )
        .expect_err("wrong residual");
        assert_eq!(error.kind(), UpdateBaselineV4ErrorKind::ResidualSumMismatch);
    }

    #[test]
    fn wrong_decision_count_fails_closed_v4() {
        let fixture = tamper_fixture_v4();
        let tampered = tamper_cell_v4(&fixture.record, 0, |cell| {
            cell.decision_count += 1;
        });
        let error = validate_update_baseline_v4(
            &fixture.episodes,
            &tampered,
            &fixture.prior_state,
            tampered.update_index(),
            tampered.update_evidence_sha256(),
        )
        .expect_err("wrong count");
        assert_eq!(error.kind(), UpdateBaselineV4ErrorKind::CountMismatch);
    }

    #[test]
    fn wrong_episode_count_fails_closed_v4() {
        let fixture = tamper_fixture_v4();
        let tampered = tamper_cell_v4(&fixture.record, 1, |cell| {
            cell.episode_count += 1;
            // Keep the invariant decision_count >= episode_count so the
            // structural decoder doesn't reject it before the semantic
            // count-mismatch check runs.
            cell.decision_count += 1;
        });
        let error = validate_update_baseline_v4(
            &fixture.episodes,
            &tampered,
            &fixture.prior_state,
            tampered.update_index(),
            tampered.update_evidence_sha256(),
        )
        .expect_err("wrong episode count");
        assert_eq!(error.kind(), UpdateBaselineV4ErrorKind::CountMismatch);
    }

    #[test]
    fn wrong_c_t_lag_violation_fails_closed_v4() {
        let fixture = tamper_fixture_v4();
        let tampered = tamper_cell_v4(&fixture.record, 0, |cell| {
            cell.c_t_bits = 0.5_f32.to_bits();
        });
        let error = validate_update_baseline_v4(
            &fixture.episodes,
            &tampered,
            &fixture.prior_state,
            tampered.update_index(),
            tampered.update_evidence_sha256(),
        )
        .expect_err("lag violation");
        assert_eq!(error.kind(), UpdateBaselineV4ErrorKind::StrictLagMismatch);
    }

    #[test]
    fn wrong_c_next_fails_closed_v4() {
        let fixture = tamper_fixture_v4();
        let tampered = tamper_cell_v4(&fixture.record, 0, |cell| {
            cell.c_next_bits = 0.99_f32.to_bits();
        });
        let error = validate_update_baseline_v4(
            &fixture.episodes,
            &tampered,
            &fixture.prior_state,
            tampered.update_index(),
            tampered.update_evidence_sha256(),
        )
        .expect_err("wrong c_next");
        assert_eq!(error.kind(), UpdateBaselineV4ErrorKind::CNextMismatch);
    }

    #[test]
    fn wrong_policy_sum_fails_closed_v4() {
        let fixture = tamper_fixture_v4();
        let tampered = build_update_baseline_record_v4(UpdateBaselineRecordPartsV4 {
            update_index: fixture.record.update_index(),
            update_evidence_sha256: fixture.record.update_evidence_sha256(),
            cells: fixture
                .record
                .cells()
                .iter()
                .map(|cell| UpdateBaselineCellPartsV4 {
                    opponent_checkpoint_manifest_sha256: cell
                        .key()
                        .opponent_checkpoint_manifest_sha256
                        .clone(),
                    role: cell.key().role,
                    c_t_bits: cell.c_t_bits(),
                    c_next_bits: cell.c_next_bits(),
                    residual_sum_f64: cell.residual_sum_f64(),
                    decision_count: cell.decision_count(),
                    episode_count: cell.episode_count(),
                })
                .collect(),
            declared_policy_sum_bits: 1.2345_f32.to_bits(),
        })
        .expect("build tampered record");
        let error = validate_update_baseline_v4(
            &fixture.episodes,
            &tampered,
            &fixture.prior_state,
            tampered.update_index(),
            tampered.update_evidence_sha256(),
        )
        .expect_err("wrong policy sum");
        assert_eq!(error.kind(), UpdateBaselineV4ErrorKind::PolicySumMismatch);
    }

    #[test]
    fn misattributed_cell_fails_closed_v4() {
        // Swap the two cells' declared residual sums and counts, keeping the
        // keys themselves correct: a term genuinely belonging to cell 0 is
        // now claimed under cell 1's residual/count and vice versa.
        let fixture = tamper_fixture_v4();
        assert_eq!(fixture.record.cells().len(), 2);
        let cell0 = &fixture.record.cells()[0];
        let cell1 = &fixture.record.cells()[1];
        let swapped = vec![
            UpdateBaselineCellPartsV4 {
                opponent_checkpoint_manifest_sha256: cell0
                    .key()
                    .opponent_checkpoint_manifest_sha256
                    .clone(),
                role: cell0.key().role,
                c_t_bits: cell0.c_t_bits(),
                c_next_bits: cell0.c_next_bits(),
                residual_sum_f64: cell1.residual_sum_f64(),
                decision_count: cell1.decision_count(),
                episode_count: cell1.episode_count(),
            },
            UpdateBaselineCellPartsV4 {
                opponent_checkpoint_manifest_sha256: cell1
                    .key()
                    .opponent_checkpoint_manifest_sha256
                    .clone(),
                role: cell1.key().role,
                c_t_bits: cell1.c_t_bits(),
                c_next_bits: cell1.c_next_bits(),
                residual_sum_f64: cell0.residual_sum_f64(),
                decision_count: cell0.decision_count(),
                episode_count: cell0.episode_count(),
            },
        ];
        let tampered = build_update_baseline_record_v4(UpdateBaselineRecordPartsV4 {
            update_index: fixture.record.update_index(),
            update_evidence_sha256: fixture.record.update_evidence_sha256(),
            cells: swapped,
            declared_policy_sum_bits: fixture.record.declared_policy_sum_bits(),
        })
        .expect("build swapped record");
        let error = validate_update_baseline_v4(
            &fixture.episodes,
            &tampered,
            &fixture.prior_state,
            tampered.update_index(),
            tampered.update_evidence_sha256(),
        )
        .expect_err("misattributed cell");
        // Either the residual sum or the counts disagree once swapped
        // (the two cells here have different decision/episode counts).
        assert!(matches!(
            error.kind(),
            UpdateBaselineV4ErrorKind::ResidualSumMismatch
                | UpdateBaselineV4ErrorKind::CountMismatch
        ));
    }

    #[test]
    fn term_return_mismatch_fails_closed_v4() {
        // The record is built from a genuinely-consistent batch (every
        // term's terminal_return_i8 agrees with its episode's
        // learner_return); the evidence VIEW handed to the validator is
        // then tampered so one term disagrees with its episode's return.
        let digest0 = fake_digest_v4(7);
        let digest1 = fake_digest_v4(8);
        let (truth_episode0_terms, truth_episode1_terms) = sample_episodes_v4();
        let truth_episodes = sample_views_v4(
            &truth_episode0_terms,
            &truth_episode1_terms,
            &digest0,
            &digest1,
        );
        let prior_state = NativeBaselineStateV4::empty_v4();
        let (observations, policy_sum) =
            expected_observations_and_policy_sum_v4(&truth_episodes, &prior_state);
        let successor = prior_state.apply_update_v4(&observations).expect("apply");
        let record =
            build_record_from_truth_v4(&prior_state, &observations, policy_sum, &successor);

        let (mut tampered_episode0_terms, tampered_episode1_terms) = sample_episodes_v4();
        tampered_episode0_terms[0].terminal_return_i8 = -1; // disagrees with learner_return = 1
        let tampered_episodes = sample_views_v4(
            &tampered_episode0_terms,
            &tampered_episode1_terms,
            &digest0,
            &digest1,
        );

        let error = validate_update_baseline_v4(
            &tampered_episodes,
            &record,
            &prior_state,
            record.update_index(),
            record.update_evidence_sha256(),
        )
        .expect_err("term/episode return mismatch");
        assert_eq!(error.kind(), UpdateBaselineV4ErrorKind::TermReturnMismatch);
    }

    #[test]
    fn extra_declared_cell_fails_closed_v4() {
        let fixture = tamper_fixture_v4();
        let mut cells: Vec<UpdateBaselineCellPartsV4> = fixture
            .record
            .cells()
            .iter()
            .map(|cell| UpdateBaselineCellPartsV4 {
                opponent_checkpoint_manifest_sha256: cell
                    .key()
                    .opponent_checkpoint_manifest_sha256
                    .clone(),
                role: cell.key().role,
                c_t_bits: cell.c_t_bits(),
                c_next_bits: cell.c_next_bits(),
                residual_sum_f64: cell.residual_sum_f64(),
                decision_count: cell.decision_count(),
                episode_count: cell.episode_count(),
            })
            .collect();
        cells.push(UpdateBaselineCellPartsV4 {
            opponent_checkpoint_manifest_sha256: fake_digest_v4(99),
            role: BaselineRoleV4::P0,
            c_t_bits: 0.0_f32.to_bits(),
            c_next_bits: 0.0_f32.to_bits(),
            residual_sum_f64: 0.0,
            decision_count: 1,
            episode_count: 1,
        });
        let tampered = build_update_baseline_record_v4(UpdateBaselineRecordPartsV4 {
            update_index: fixture.record.update_index(),
            update_evidence_sha256: fixture.record.update_evidence_sha256(),
            cells,
            declared_policy_sum_bits: fixture.record.declared_policy_sum_bits(),
        })
        .expect("build record with extra cell");
        let error = validate_update_baseline_v4(
            &fixture.episodes,
            &tampered,
            &fixture.prior_state,
            tampered.update_index(),
            tampered.update_evidence_sha256(),
        )
        .expect_err("extra cell");
        assert_eq!(error.kind(), UpdateBaselineV4ErrorKind::CellSetMismatch);
    }

    #[test]
    fn non_canonical_bytes_rejected_v4() {
        let fixture = tamper_fixture_v4();
        let mut bytes = fixture.record.canonical_bytes().to_vec();
        bytes.push(b'\n');
        let error = decode_update_baseline_record_v4(&bytes).expect_err("non canonical");
        assert!(matches!(
            error.kind(),
            UpdateBaselineV4ErrorKind::CanonicalJson(_)
        ));
    }

    #[test]
    fn out_of_order_cells_rejected_at_decode_v4() {
        let fixture = tamper_fixture_v4();
        let mut cells: Vec<UpdateBaselineCellPartsV4> = fixture
            .record
            .cells()
            .iter()
            .map(|cell| UpdateBaselineCellPartsV4 {
                opponent_checkpoint_manifest_sha256: cell
                    .key()
                    .opponent_checkpoint_manifest_sha256
                    .clone(),
                role: cell.key().role,
                c_t_bits: cell.c_t_bits(),
                c_next_bits: cell.c_next_bits(),
                residual_sum_f64: cell.residual_sum_f64(),
                decision_count: cell.decision_count(),
                episode_count: cell.episode_count(),
            })
            .collect();
        cells.reverse();
        let error = build_update_baseline_record_v4(UpdateBaselineRecordPartsV4 {
            update_index: fixture.record.update_index(),
            update_evidence_sha256: fixture.record.update_evidence_sha256(),
            cells,
            declared_policy_sum_bits: fixture.record.declared_policy_sum_bits(),
        })
        .expect_err("out of order");
        assert_eq!(error.kind(), UpdateBaselineV4ErrorKind::CellOrder);
    }

    #[test]
    fn schema_mismatch_rejected_at_decode_v4() {
        let fixture = tamper_fixture_v4();
        // Round-trip through JSON with a corrupted schema string.
        let bytes = fixture.record.canonical_bytes().to_vec();
        let mut document: serde_json::Value = serde_json::from_slice(&bytes).expect("parse");
        document["schema"] = serde_json::Value::String("wrong-schema/v1".to_owned());
        let bytes = to_canonical_json_bytes_v1(
            &serde_json::from_value::<UpdateBaselineWireV4>(document).expect("shape"),
            CanonicalJsonNullPolicyV1::Forbid,
        )
        .expect("encode");
        let error = decode_update_baseline_record_v4(&bytes).expect_err("schema");
        assert_eq!(error.kind(), UpdateBaselineV4ErrorKind::InvalidSchema);
    }

    /// Review finding P1: a record replayed against a different source
    /// update fails closed on the index and digest bindings.
    #[test]
    fn source_update_binding_fails_closed_v4() {
        let fixture = tamper_fixture_v4();
        let wrong_index = validate_update_baseline_v4(
            &fixture.episodes,
            &fixture.record,
            &fixture.prior_state,
            fixture.record.update_index() + 1,
            fixture.record.update_evidence_sha256(),
        )
        .expect_err("wrong index");
        assert_eq!(
            wrong_index.kind(),
            UpdateBaselineV4ErrorKind::SourceUpdateMismatch
        );
        let wrong_digest = validate_update_baseline_v4(
            &fixture.episodes,
            &fixture.record,
            &fixture.prior_state,
            fixture.record.update_index(),
            [0xEE_u8; 32],
        )
        .expect_err("wrong digest");
        assert_eq!(
            wrong_digest.kind(),
            UpdateBaselineV4ErrorKind::SourceUpdateMismatch
        );
    }
}
