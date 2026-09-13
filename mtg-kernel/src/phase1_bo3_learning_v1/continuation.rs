//! Explicit CPU BO3 objective transition and immutable attempted-batch progress.
use super::preparation::{read_bytes, strict_json};
use super::{
    Bo3GameplayPreparationRequestV1, PreparedBo3GameplayBatchV1, prepare_bo3_gameplay_batch_v1,
    require,
};
use crate::durable_publication_v1::{
    DurableFileExpectationV1, capture_existing_publication_parent_v1, publish_new_file_v1,
    verify_existing_publication_v1,
};
use crate::expanded_deck_training_v1::{
    ExpandedInferenceIdentityV1, ExpandedModelSourceV1, ExpandedSeatBehaviorV1, PinnedFileV1,
    inference_identity_v1, load_ordinary_bo3_parent_v1,
};
use crate::native_policy_train_step_v1::{
    NativePolicyValueTrainSnapshotV1, NativePolicyValueTrainStateV1,
};
use crate::native_policy_value_net_v1::NativeNamedParameterV1;
use crate::rl::PlayerSeatV1;
use crate::sideboard_play_policy_v1::{FrozenPlayObservationTransferV3, FrozenPlayPolicyV1};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::io::Write;
use std::path::{Path, PathBuf};

pub const BO3_GAMEPLAY_SOURCE_SCHEMA_V1: &str = "mtg-kernel-bo3-gameplay-source/v1";
pub const BO3_GAMEPLAY_CHECKPOINT_SCHEMA_V1: &str = "mtg-kernel-bo3-gameplay-checkpoint/v1";
pub const BO3_GAMEPLAY_UPDATE_SCHEMA_V1: &str = "mtg-kernel-bo3-gameplay-update-request/v1";
pub const BO3_GAMEPLAY_OBJECTIVE_V1: &str = "bo3_match_return_weighted_gameplay/v1";
const NORMALIZATION: &str = "equal_eligible_match_groups_f64_div_cast_f32/v1";
const PROGRESS_SCHEMA: &str = "mtg-kernel-bo3-gameplay-progress/v1";
const MAX_REQUEST_BYTES: u64 = 1024 * 1024;
const MAX_PROGRESS_BYTES: u64 = 32 * 1024 * 1024;
const MAX_CHECKPOINT_BYTES: u64 = 512 * 1024 * 1024;
const MAX_ATTEMPTS: usize = 65_536;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Bo3GameplayUpdateInputV1 {
    OrdinaryCheckpointTransition { learner: ExpandedSeatBehaviorV1 },
    Bo3Checkpoint { learner: ExpandedSeatBehaviorV1 },
}
impl Bo3GameplayUpdateInputV1 {
    pub fn learner_v1(&self) -> &ExpandedSeatBehaviorV1 {
        match self {
            Self::OrdinaryCheckpointTransition { learner } | Self::Bo3Checkpoint { learner } => {
                learner
            }
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Bo3GameplayUpdateRequestV1 {
    pub schema: String,
    pub input: Bo3GameplayUpdateInputV1,
    pub preparation_request: PinnedFileV1,
    pub learning_rate_bits: u32,
    pub value_coefficient_bits: u32,
    /// The caller supplies the latest immutable tip, including after NoUpdate.
    /// None opens a deliberate new ordinary-origin chain, not a global claim.
    pub previous_progress: Option<PinnedFileV1>,
    pub output_directory: PathBuf,
}
impl Bo3GameplayUpdateRequestV1 {
    pub fn from_json_v1(text: &str) -> Result<Self, String> {
        require(
            text.len() as u64 <= MAX_REQUEST_BYTES,
            "BO3 update request exceeds 1 MiB",
        )?;
        strict_json(text.as_bytes())
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Bo3GameplaySourceV1 {
    pub schema: String,
    pub objective: String,
    pub normalization: String,
    pub ordinary_origin: ExpandedSeatBehaviorV1,
    pub feature_transfer: FrozenPlayObservationTransferV3,
    pub learning_rate_bits: u32,
    pub value_coefficient_bits: u32,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ParameterBits {
    name: String,
    shape: Vec<usize>,
    values: Vec<u32>,
}
impl ParameterBits {
    fn from_native(p: &NativeNamedParameterV1) -> Self {
        Self {
            name: p.name.into(),
            shape: p.shape.clone(),
            values: p.values.iter().map(|v| v.to_bits()).collect(),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StateBits {
    state_sha256: String,
    adam_step: u64,
    scorer_bias_anchor_bits: u32,
    parameters: Vec<ParameterBits>,
    first_moments: Vec<ParameterBits>,
    second_moments: Vec<ParameterBits>,
}
impl StateBits {
    fn capture(state: &NativePolicyValueTrainStateV1) -> Result<Self, String> {
        let snapshot = state.snapshot_v1().map_err(err)?;
        Ok(Self {
            state_sha256: hex(&snapshot.state_sha256_v1().map_err(err)?),
            adam_step: snapshot.adam_step,
            scorer_bias_anchor_bits: snapshot.scorer_bias_anchor_bits,
            parameters: snapshot
                .parameters
                .iter()
                .map(ParameterBits::from_native)
                .collect(),
            first_moments: snapshot
                .first_moments
                .iter()
                .map(ParameterBits::from_native)
                .collect(),
            second_moments: snapshot
                .second_moments
                .iter()
                .map(ParameterBits::from_native)
                .collect(),
        })
    }
    fn restore(
        &self,
        origin: &NativePolicyValueTrainStateV1,
    ) -> Result<NativePolicyValueTrainStateV1, String> {
        let template = origin.model_v1().parameter_snapshot_v1();
        let restore = |saved: &[ParameterBits]| -> Result<Vec<NativeNamedParameterV1>, String> {
            require(saved.len() == template.len(), "BO3 parameter count differs")?;
            saved
                .iter()
                .zip(&template)
                .map(|(s, t)| {
                    require(
                        s.name == t.name && s.shape == t.shape && s.values.len() == t.values.len(),
                        "BO3 named parameter layout differs",
                    )?;
                    Ok(NativeNamedParameterV1 {
                        name: t.name,
                        shape: t.shape.clone(),
                        values: s.values.iter().map(|b| f32::from_bits(*b)).collect(),
                    })
                })
                .collect()
        };
        let snapshot = NativePolicyValueTrainSnapshotV1 {
            adam_step: self.adam_step,
            scorer_bias_anchor_bits: self.scorer_bias_anchor_bits,
            parameters: restore(&self.parameters)?,
            first_moments: restore(&self.first_moments)?,
            second_moments: restore(&self.second_moments)?,
        };
        require(
            hex(&snapshot.state_sha256_v1().map_err(err)?) == self.state_sha256,
            "BO3 full state hash differs",
        )?;
        NativePolicyValueTrainStateV1::from_snapshot_v1(origin.model_v1().clone(), &snapshot)
            .map_err(err)
    }
}

// Private reader DTOs preserve the existing preparation-report output without
// adding deserialization authority or fields to its public type.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Disposition {
    Ready,
    NoUpdate,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct AttemptReport {
    match_id: String,
    deck_ids: [String; 2],
    learner_seat: PlayerSeatV1,
    complete: bool,
    eligible: bool,
    exclusion_reason: Option<String>,
    physical_learner_groups: usize,
    captured_substeps: usize,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PreparationReport {
    disposition: Disposition,
    attempts: Vec<AttemptReport>,
    eligible_matches: usize,
    complete_matches: usize,
    incomplete_matches: usize,
    prepared_payload_bytes: u64,
    learner_groups: usize,
    learner_substeps: usize,
    weight_bits: Vec<u32>,
    claim: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct AttemptProgress {
    attempted_batches: u64,
    /// Each fixed triple is request SHA, result SHA, semantic physical SHA.
    /// Includes incomplete attempts. Strings are exactly 64 lowercase hex.
    ledger: Vec<[String; 3]>,
    preparation: PreparationReport,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Bo3GameplayCheckpointV1 {
    schema: String,
    objective: String,
    normalization: String,
    source_descriptor: PinnedFileV1,
    ordinary_origin: ExpandedSeatBehaviorV1,
    operation_request: PinnedFileV1,
    request: Bo3GameplayUpdateRequestV1,
    completed_bo3_updates: u64,
    learning_rate_bits: u32,
    value_coefficient_bits: u32,
    state: StateBits,
    attempt_progress: AttemptProgress,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Progress {
    schema: String,
    operation_request: PinnedFileV1,
    request: Bo3GameplayUpdateRequestV1,
    ordinary_origin: ExpandedSeatBehaviorV1,
    completed_bo3_updates: u64,
    attempt_progress: AttemptProgress,
    result: ExpandedSeatBehaviorV1,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Bo3GameplayUpdateResultV1 {
    pub progress: PinnedFileV1,
    pub learner: ExpandedSeatBehaviorV1,
    pub completed_bo3_updates: u64,
    pub attempted_batches: u64,
    pub attempted_matches: usize,
    pub optimizer_updated: bool,
}
struct Loaded {
    policy: FrozenPlayPolicyV1,
    state: NativePolicyValueTrainStateV1,
    origin: ExpandedSeatBehaviorV1,
    completed: u64,
    learning_rate_bits: u32,
    value_coefficient_bits: u32,
}
fn err(error: impl std::fmt::Display) -> String {
    error.to_string()
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn digest_valid(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn read<T: DeserializeOwned + Serialize>(pin: &PinnedFileV1, bound: u64) -> Result<T, String> {
    strict_json(&read_bytes(pin, bound)?)
}
fn scalar_valid(bits: u32) -> bool {
    let x = f32::from_bits(bits);
    x.is_finite() && x > 0.0
}
fn json<T: Serialize>(value: &T, maximum: u64) -> Result<Vec<u8>, String> {
    struct Buffer {
        bytes: Vec<u8>,
        maximum: u64,
    }
    impl Write for Buffer {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            let next = self
                .bytes
                .len()
                .checked_add(bytes.len())
                .filter(|n| *n as u64 <= self.maximum)
                .ok_or_else(|| std::io::Error::other("BO3 artifact byte bound exceeded"))?;
            self.bytes
                .try_reserve(next - self.bytes.len())
                .map_err(std::io::Error::other)?;
            self.bytes.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut buffer = Buffer {
        bytes: Vec::new(),
        maximum,
    };
    serde_json::to_writer(&mut buffer, value).map_err(err)?;
    Ok(buffer.bytes)
}
fn pin_for(path: PathBuf, bytes: &[u8]) -> PinnedFileV1 {
    PinnedFileV1 {
        path,
        sha256: format!("{:x}", Sha256::digest(bytes)),
    }
}
fn publish<T: Serialize>(
    directory: &Path,
    name: &str,
    value: &T,
    bound: u64,
) -> Result<PinnedFileV1, String> {
    let bytes = json(value, bound)?;
    let parent = capture_existing_publication_parent_v1(directory).map_err(err)?;
    let expected = DurableFileExpectationV1::from_bytes(&bytes).map_err(err)?;
    let stage = format!(".{name}.stage");
    if directory.join(name).try_exists().map_err(err)? {
        verify_existing_publication_v1(&parent, name, expected).map_err(err)?;
        if directory.join(&stage).try_exists().map_err(err)? {
            verify_existing_publication_v1(&parent, &stage, expected).map_err(err)?;
        }
    } else {
        publish_new_file_v1(&parent, &stage, name, &bytes, expected).map_err(err)?;
    }
    Ok(pin_for(directory.join(name), &bytes))
}
fn existing_pin(path: &Path, maximum: u64) -> Result<PinnedFileV1, String> {
    let file = std::fs::File::open(path).map_err(err)?;
    let metadata = file.metadata().map_err(err)?;
    require(
        metadata.is_file() && metadata.len() <= maximum,
        "existing artifact is not a bounded file",
    )?;
    let mut bytes = Vec::new();
    std::io::Read::read_to_end(&mut std::io::Read::take(file, maximum + 1), &mut bytes)
        .map_err(err)?;
    require(
        bytes.len() as u64 <= maximum,
        "existing artifact grew beyond bound",
    )?;
    Ok(pin_for(path.to_path_buf(), &bytes))
}
fn read_preparation(
    request: &Bo3GameplayUpdateRequestV1,
) -> Result<Bo3GameplayPreparationRequestV1, String> {
    let input: Bo3GameplayPreparationRequestV1 =
        read(&request.preparation_request, MAX_REQUEST_BYTES)?;
    require(
        input.learner == *request.input.learner_v1(),
        "preparation learner differs from exact update parent",
    )?;
    require(
        input.schema == super::BO3_PREPARATION_REQUEST_SCHEMA_V1
            && (1..=32).contains(&input.attempts.len())
            && (1..=512 * 1024 * 1024).contains(&input.limits.max_input_bytes)
            && (1..=256 * 1024 * 1024).contains(&input.limits.max_prepared_payload_bytes),
        "invalid preparation schema, bounds or attempted batch size",
    )?;
    for attempt in &input.attempts {
        for pin in [&attempt.request, &attempt.result] {
            require(
                pin.path.is_absolute() && digest_valid(&pin.sha256),
                "attempt pin shape differs",
            )?;
        }
    }
    Ok(input)
}
pub(crate) fn validate_bo3_source_admission_v1(
    source: &ExpandedModelSourceV1,
    bytes: &[u8],
) -> Result<(), String> {
    let descriptor: Bo3GameplaySourceV1 = strict_json(bytes)?;
    require(
        descriptor.schema == BO3_GAMEPLAY_SOURCE_SCHEMA_V1
            && descriptor.objective == BO3_GAMEPLAY_OBJECTIVE_V1
            && descriptor.normalization == NORMALIZATION
            && source.checkpoint.is_some()
            && source.feature_transfer == descriptor.feature_transfer
            && descriptor.feature_transfer == descriptor.ordinary_origin.source.feature_transfer
            && scalar_valid(descriptor.learning_rate_bits)
            && scalar_valid(descriptor.value_coefficient_bits),
        "BO3 source objective, checkpoint, features or scalars differ",
    )
}
fn ordinary(behavior: &ExpandedSeatBehaviorV1) -> Result<Loaded, String> {
    let (policy, state, actual, lr, value) = load_ordinary_bo3_parent_v1(&behavior.source)?;
    require(
        actual == behavior.identity,
        "ordinary origin or parent state differs",
    )?;
    Ok(Loaded {
        policy,
        state,
        origin: behavior.clone(),
        completed: 0,
        learning_rate_bits: lr,
        value_coefficient_bits: value,
    })
}
fn validate_report(
    report: &PreparationReport,
    preparation: &Bo3GameplayPreparationRequestV1,
) -> Result<(), String> {
    require(
        report.attempts.len() == preparation.attempts.len()
            && report.learner_groups == report.weight_bits.len()
            && report.learner_groups <= 65_536
            && report.learner_substeps <= 100_000
            && report.prepared_payload_bytes <= preparation.limits.max_prepared_payload_bytes,
        "recorded preparation counts or bounds differ",
    )?;
    let complete = report.attempts.iter().filter(|a| a.complete).count();
    let eligible: Vec<_> = report.attempts.iter().filter(|a| a.eligible).collect();
    require(
        report.complete_matches == complete
            && report.incomplete_matches == report.attempts.len() - complete
            && report.eligible_matches == eligible.len(),
        "recorded match counts differ",
    )?;
    let mut group_count = 0_usize;
    for (a, declared) in report.attempts.iter().zip(&preparation.attempts) {
        group_count = group_count
            .checked_add(a.physical_learner_groups)
            .ok_or("group count overflow")?;
        require(
            a.learner_seat == declared.learner_seat
                && (!a.eligible
                    || (a.complete
                        && a.physical_learner_groups > 0
                        && a.exclusion_reason.is_none()))
                && (a.eligible || a.physical_learner_groups == 0)
                && group_count <= 65_536
                && a.captured_substeps <= 100_000,
            "recorded attempt eligibility or count differs",
        )?;
    }
    require(
        group_count == report.learner_groups,
        "recorded total group count differs",
    )?;
    let weights: Vec<u32> = eligible
        .iter()
        .flat_map(|a| {
            std::iter::repeat_n(
                ((1.0_f64 / eligible.len() as f64 / a.physical_learner_groups as f64) as f32)
                    .to_bits(),
                a.physical_learner_groups,
            )
        })
        .collect();
    require(
        weights == report.weight_bits
            && report.learner_substeps >= report.learner_groups
            && ((report.disposition == Disposition::Ready) == !eligible.is_empty())
            && (!eligible.is_empty()
                || (report.learner_groups == 0
                    && report.learner_substeps == 0
                    && report.prepared_payload_bytes == 0)),
        "recorded group weights or no-update accounting differ",
    )
}
fn validate_ledger(ledger: &[[String; 3]]) -> Result<(), String> {
    require(
        ledger.len() <= MAX_ATTEMPTS,
        "BO3 exact attempt ledger exhausted; explicit extension required",
    )?;
    let mut seen = [BTreeSet::new(), BTreeSet::new(), BTreeSet::new()];
    for triple in ledger {
        for (i, hash) in triple.iter().enumerate() {
            require(
                digest_valid(hash) && seen[i].insert(hash.as_str()),
                "duplicate or invalid historical request/result/physical attempt identity",
            )?;
        }
    }
    Ok(())
}
fn prior(
    request: &Bo3GameplayUpdateRequestV1,
    origin: &ExpandedSeatBehaviorV1,
    completed: u64,
) -> Result<Option<Progress>, String> {
    prior_fields(
        &request.input,
        request.previous_progress.as_ref(),
        request.learning_rate_bits,
        request.value_coefficient_bits,
        origin,
        completed,
    )
}
fn prior_fields(
    input: &Bo3GameplayUpdateInputV1,
    previous_progress: Option<&PinnedFileV1>,
    learning_rate_bits: u32,
    value_coefficient_bits: u32,
    origin: &ExpandedSeatBehaviorV1,
    completed: u64,
) -> Result<Option<Progress>, String> {
    if let Some(pin) = previous_progress {
        let progress: Progress = read(pin, MAX_PROGRESS_BYTES)?;
        require(
            progress.schema == PROGRESS_SCHEMA
                && progress.request.schema == BO3_GAMEPLAY_UPDATE_SCHEMA_V1
                && progress.request.output_directory.is_absolute()
                && progress.request.learning_rate_bits == learning_rate_bits
                && progress.request.value_coefficient_bits == value_coefficient_bits
                && progress.result == *input.learner_v1()
                && progress.ordinary_origin == *origin
                && progress.completed_bo3_updates == completed
                && progress.attempt_progress.attempted_batches > 0
                && progress.attempt_progress.attempted_batches
                    <= progress.attempt_progress.ledger.len() as u64,
            "prior attempted progress does not bind current parent/origin/counters",
        )?;
        validate_ledger(&progress.attempt_progress.ledger)?;
        let bytes = json(&progress.request, MAX_REQUEST_BYTES)?;
        require(
            progress.operation_request
                == pin_for(
                    progress.request.output_directory.join("request.json"),
                    &bytes,
                )
                && pin.path == progress.request.output_directory.join("progress.json"),
            "prior progress/request location differs",
        )?;
        let saved: Bo3GameplayUpdateRequestV1 =
            read(&progress.operation_request, MAX_REQUEST_BYTES)?;
        require(saved == progress.request, "prior operation request differs")?;
        // Inspect the immediate recorded prefix, not a recursive ancestry replay.
        let predecessor: Option<Progress> = progress
            .request
            .previous_progress
            .as_ref()
            .map(|pin| read(pin, MAX_PROGRESS_BYTES))
            .transpose()?;
        if let Some(before) = &predecessor {
            require(
                before.schema == PROGRESS_SCHEMA
                    && before.result == *progress.request.input.learner_v1()
                    && before.ordinary_origin == progress.ordinary_origin,
                "prior progress predecessor differs",
            )?;
        }
        validate_progress(
            &progress.attempt_progress,
            &progress.request,
            &read_preparation(&progress.request)?,
            predecessor.as_ref(),
        )?;
        if progress.attempt_progress.preparation.disposition == Disposition::NoUpdate {
            require(
                progress.result == *progress.request.input.learner_v1()
                    && progress.completed_bo3_updates
                        == predecessor.as_ref().map_or(0, |p| p.completed_bo3_updates),
                "prior NoUpdate progress changed source or optimizer count",
            )?;
        } else {
            let checkpoint: Bo3GameplayCheckpointV1 = read(
                progress
                    .result
                    .source
                    .checkpoint
                    .as_ref()
                    .ok_or("prior Ready progress lacks checkpoint")?,
                MAX_CHECKPOINT_BYTES,
            )?;
            require(
                checkpoint.request == progress.request
                    && checkpoint.operation_request == progress.operation_request
                    && checkpoint.attempt_progress == progress.attempt_progress
                    && checkpoint.completed_bo3_updates == progress.completed_bo3_updates,
                "prior Ready progress differs from current parent checkpoint",
            )?;
        }
        Ok(Some(progress))
    } else {
        require(
            completed == 0
                && matches!(
                    input,
                    Bo3GameplayUpdateInputV1::OrdinaryCheckpointTransition { .. }
                ),
            "BO3 continuation requires previous attempted progress",
        )?;
        Ok(None)
    }
}

/// Read-only scheduler admission. The ledger is internal state, not a new wire
/// protocol or a claim that the caller owns the globally latest chain tip.
pub(super) struct ValidatedBo3TipV1 {
    pub completed_bo3_updates: u64,
    pub attempted_batches: u64,
    pub attempted_matches: usize,
    pub consumed_physical_keys: BTreeSet<String>,
}
pub(super) fn validate_bo3_tip_v1(
    input: &Bo3GameplayUpdateInputV1,
    previous_progress: Option<&PinnedFileV1>,
    learning_rate_bits: u32,
    value_coefficient_bits: u32,
) -> Result<ValidatedBo3TipV1, String> {
    let parent = load_parent(input)?;
    require(
        learning_rate_bits == parent.learning_rate_bits
            && value_coefficient_bits == parent.value_coefficient_bits,
        "BO3 tip must preserve exact parent scalar bits",
    )?;
    let previous = prior_fields(
        input,
        previous_progress,
        learning_rate_bits,
        value_coefficient_bits,
        &parent.origin,
        parent.completed,
    )?;
    Ok(ValidatedBo3TipV1 {
        completed_bo3_updates: parent.completed,
        attempted_batches: previous
            .as_ref()
            .map_or(0, |p| p.attempt_progress.attempted_batches),
        attempted_matches: previous
            .as_ref()
            .map_or(0, |p| p.attempt_progress.ledger.len()),
        consumed_physical_keys: previous.map_or_else(BTreeSet::new, |p| {
            p.attempt_progress
                .ledger
                .into_iter()
                .map(|triple| triple[2].clone())
                .collect()
        }),
    })
}

/// Checks a compact saved operation result against its original progress DTO.
/// The runner verifies the bytes' pin; this does not load historical tensors or
/// substitute for full validation of the latest continuation tip.
pub(super) fn validate_recorded_bo3_result_v1(
    request: &Bo3GameplayUpdateRequestV1,
    expected: &Bo3GameplayUpdateResultV1,
    bytes: &[u8],
    expected_ledger: Option<&[[String; 3]]>,
) -> Result<(), String> {
    require(
        bytes.len() as u64 <= MAX_PROGRESS_BYTES,
        "recorded progress exceeds bound",
    )?;
    let progress: Progress = strict_json(bytes)?;
    validate_recorded_progress(request, expected, &progress, expected_ledger)
}
fn validate_recorded_progress(
    request: &Bo3GameplayUpdateRequestV1,
    expected: &Bo3GameplayUpdateResultV1,
    progress: &Progress,
    expected_ledger: Option<&[[String; 3]]>,
) -> Result<(), String> {
    require(
        progress.schema == PROGRESS_SCHEMA
            && progress.request == *request
            && expected.progress.path == request.output_directory.join("progress.json")
            && progress.operation_request
                == pin_for(
                    request.output_directory.join("request.json"),
                    &json(request, MAX_REQUEST_BYTES)?,
                ),
        "recorded result does not bind original progress/request",
    )?;
    validate_ledger(&progress.attempt_progress.ledger)?;
    require(
        expected_ledger.is_none_or(|ledger| ledger == progress.attempt_progress.ledger.as_slice()),
        "recorded progress ledger differs from exact ordered schedule prefix",
    )?;
    require(
        result(&progress, expected.progress.clone()) == *expected,
        "recorded result differs from progress learner/counters/disposition",
    )
}

/// Compact initial-prefix anchor, using the same saved progress DTO as native
/// continuation. Caller verifies its exact pin; full learned state is validated
/// at the reconstructed latest tip, not repeatedly for historical checkpoints.
pub(super) fn recorded_bo3_tip_v1(
    input: &Bo3GameplayUpdateInputV1,
    pin: &PinnedFileV1,
    bytes: &[u8],
    learning_rate_bits: u32,
    value_coefficient_bits: u32,
) -> Result<
    (
        Bo3GameplayUpdateResultV1,
        Bo3GameplayUpdateRequestV1,
        Vec<[String; 3]>,
    ),
    String,
> {
    require(
        bytes.len() as u64 <= MAX_PROGRESS_BYTES,
        "recorded tip exceeds bound",
    )?;
    let progress: Progress = strict_json(bytes)?;
    let summary = result(&progress, pin.clone());
    validate_recorded_progress(&progress.request, &summary, &progress, None)?;
    require(
        progress.result == *input.learner_v1()
            && progress.request.learning_rate_bits == learning_rate_bits
            && progress.request.value_coefficient_bits == value_coefficient_bits
            && progress.attempt_progress.attempted_batches > 0
            && progress.attempt_progress.attempted_batches
                <= progress.attempt_progress.ledger.len() as u64
            && match input {
                Bo3GameplayUpdateInputV1::OrdinaryCheckpointTransition { .. } => {
                    progress.completed_bo3_updates == 0
                }
                Bo3GameplayUpdateInputV1::Bo3Checkpoint { .. } => {
                    progress.completed_bo3_updates > 0
                }
            },
        "initial recorded tip does not bind exact input/scalars/counters",
    )?;
    Ok((summary, progress.request, progress.attempt_progress.ledger))
}
fn validate_progress(
    plan: &AttemptProgress,
    request: &Bo3GameplayUpdateRequestV1,
    preparation: &Bo3GameplayPreparationRequestV1,
    previous: Option<&Progress>,
) -> Result<(), String> {
    validate_ledger(&plan.ledger)?;
    let prefix = previous
        .map(|p| p.attempt_progress.ledger.as_slice())
        .unwrap_or(&[]);
    let batches = previous
        .map_or(0, |p| p.attempt_progress.attempted_batches)
        .checked_add(1)
        .ok_or("attempt batch counter overflow")?;
    require(
        plan.attempted_batches == batches
            && plan.ledger.len() == prefix.len() + preparation.attempts.len()
            && plan.ledger[..prefix.len()] == *prefix,
        "attempt progress prefix/counters differ",
    )?;
    for (triple, attempt) in plan.ledger[prefix.len()..]
        .iter()
        .zip(&preparation.attempts)
    {
        require(
            triple[0] == attempt.request.sha256 && triple[1] == attempt.result.sha256,
            "attempt progress suffix differs from exact declared inputs",
        )?;
    }
    require(
        previous.is_some() == request.previous_progress.is_some(),
        "prior progress presence differs",
    )?;
    validate_report(&plan.preparation, preparation)
}

fn load_state(source: &ExpandedModelSourceV1) -> Result<(Loaded, Bo3GameplayCheckpointV1), String> {
    let bytes = read_bytes(&source.play_import, 4 * 1024 * 1024)?;
    validate_bo3_source_admission_v1(source, &bytes)?;
    let descriptor: Bo3GameplaySourceV1 = strict_json(&bytes)?;
    let mut origin = ordinary(&descriptor.ordinary_origin)?;
    require(
        origin.learning_rate_bits == descriptor.learning_rate_bits
            && origin.value_coefficient_bits == descriptor.value_coefficient_bits,
        "BO3 descriptor changed ordinary scalar bits",
    )?;
    let checkpoint: Bo3GameplayCheckpointV1 =
        read(source.checkpoint.as_ref().unwrap(), MAX_CHECKPOINT_BYTES)?;
    require(
        checkpoint.schema == BO3_GAMEPLAY_CHECKPOINT_SCHEMA_V1
            && checkpoint.objective == BO3_GAMEPLAY_OBJECTIVE_V1
            && checkpoint.normalization == NORMALIZATION
            && checkpoint.source_descriptor == source.play_import
            && checkpoint.ordinary_origin == descriptor.ordinary_origin
            && checkpoint.completed_bo3_updates > 0
            && checkpoint.learning_rate_bits == descriptor.learning_rate_bits
            && checkpoint.value_coefficient_bits == descriptor.value_coefficient_bits
            && checkpoint.attempt_progress.preparation.disposition == Disposition::Ready,
        "BO3 checkpoint objective/origin/scalars differ",
    )?;
    let before_count = checkpoint.completed_bo3_updates - 1;
    let expected_parent_step = descriptor
        .ordinary_origin
        .identity
        .adam_step
        .checked_add(before_count)
        .ok_or("origin/update count overflow")?;
    require(
        checkpoint.request.input.learner_v1().identity.adam_step == expected_parent_step
            && checkpoint.state.adam_step
                == expected_parent_step
                    .checked_add(1)
                    .ok_or("Adam counter overflow")?
            && checkpoint.request.learning_rate_bits == descriptor.learning_rate_bits
            && checkpoint.request.value_coefficient_bits == descriptor.value_coefficient_bits,
        "BO3 parent/update/Adam counter or scalar differs",
    )?;
    match &checkpoint.request.input {
        Bo3GameplayUpdateInputV1::OrdinaryCheckpointTransition { learner } => require(
            before_count == 0 && *learner == descriptor.ordinary_origin,
            "first BO3 checkpoint must preserve its exact ordinary parent",
        )?,
        Bo3GameplayUpdateInputV1::Bo3Checkpoint { learner } => require(
            before_count > 0
                && learner.source.play_import == source.play_import
                && learner.source.feature_transfer == source.feature_transfer
                && learner.source.checkpoint.is_some(),
            "BO3 parent descriptor differs",
        )?,
    }
    let operation: Bo3GameplayUpdateRequestV1 =
        read(&checkpoint.operation_request, MAX_REQUEST_BYTES)?;
    require(
        operation.schema == BO3_GAMEPLAY_UPDATE_SCHEMA_V1
            && operation.output_directory.is_absolute()
            && operation == checkpoint.request
            && source.checkpoint.as_ref().unwrap().path
                == operation.output_directory.join("checkpoint.json")
            && checkpoint.operation_request.path == operation.output_directory.join("request.json"),
        "BO3 checkpoint does not bind recorded operation/location",
    )?;
    let previous = prior(&operation, &descriptor.ordinary_origin, before_count)?;
    validate_progress(
        &checkpoint.attempt_progress,
        &operation,
        &read_preparation(&operation)?,
        previous.as_ref(),
    )?;
    let state = checkpoint.state.restore(&origin.state)?;
    origin
        .policy
        .replace_training_parameters_v3(&state.model_v1().parameter_snapshot_v1())?;
    origin.state = state;
    origin.completed = checkpoint.completed_bo3_updates;
    Ok((origin, checkpoint))
}
pub(crate) fn load_bo3_inference_v1(
    source: &ExpandedModelSourceV1,
) -> Result<(FrozenPlayPolicyV1, ExpandedInferenceIdentityV1), String> {
    let (loaded, _) = load_state(source)?;
    let identity = inference_identity_v1(source, &loaded.policy, &loaded.state)?;
    Ok((loaded.policy, identity))
}
fn load_parent(input: &Bo3GameplayUpdateInputV1) -> Result<Loaded, String> {
    match input {
        Bo3GameplayUpdateInputV1::OrdinaryCheckpointTransition { learner } => ordinary(learner),
        Bo3GameplayUpdateInputV1::Bo3Checkpoint { learner } => {
            let (loaded, _) = load_state(&learner.source)?;
            require(
                inference_identity_v1(&learner.source, &loaded.policy, &loaded.state)?
                    == learner.identity,
                "BO3 parent actual state/model differs",
            )?;
            Ok(loaded)
        }
    }
}
#[cfg(test)]
thread_local! {
    static UPDATE_CALLS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static PREPARATION_CALLS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static STOP_BEFORE_PROGRESS: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}
fn apply_prepared(
    state: &mut NativePolicyValueTrainStateV1,
    batch: &PreparedBo3GameplayBatchV1,
    lr: u32,
    value: u32,
) -> Result<(), String> {
    batch
        .with_native_groups_v1(|groups, weights| {
            state.train_step_weighted_feature_transfer_v3(
                groups,
                weights,
                f32::from_bits(value),
                f32::from_bits(lr),
            )
        })
        .map_err(err)?;
    #[cfg(test)]
    UPDATE_CALLS.with(|n| n.set(n.get() + 1));
    Ok(())
}
fn result(progress: &Progress, pin: PinnedFileV1) -> Bo3GameplayUpdateResultV1 {
    Bo3GameplayUpdateResultV1 {
        progress: pin,
        learner: progress.result.clone(),
        completed_bo3_updates: progress.completed_bo3_updates,
        attempted_batches: progress.attempt_progress.attempted_batches,
        attempted_matches: progress.attempt_progress.ledger.len(),
        optimizer_updated: progress.attempt_progress.preparation.disposition == Disposition::Ready,
    }
}
fn complete(
    request: &Bo3GameplayUpdateRequestV1,
    request_pin: PinnedFileV1,
    origin: ExpandedSeatBehaviorV1,
    count: u64,
    plan: AttemptProgress,
    behavior: ExpandedSeatBehaviorV1,
) -> Result<Bo3GameplayUpdateResultV1, String> {
    let progress = Progress {
        schema: PROGRESS_SCHEMA.into(),
        operation_request: request_pin,
        request: request.clone(),
        ordinary_origin: origin,
        completed_bo3_updates: count,
        attempt_progress: plan,
        result: behavior,
    };
    #[cfg(test)]
    if STOP_BEFORE_PROGRESS.with(|flag| flag.replace(false)) {
        return Err("injected stop after checkpoint readback before progress publication".into());
    }
    let pin = publish(
        &request.output_directory,
        "progress.json",
        &progress,
        MAX_PROGRESS_BYTES,
    )?;
    Ok(result(&progress, pin))
}

/// One explicitly requested CPU operation. The caller owns the latest progress
/// tip and exclusive directory; this API is not a scheduler or global CAS.
pub fn update_bo3_gameplay_v1(
    request: Bo3GameplayUpdateRequestV1,
) -> Result<Bo3GameplayUpdateResultV1, String> {
    require(
        request.schema == BO3_GAMEPLAY_UPDATE_SCHEMA_V1 && request.output_directory.is_absolute(),
        "BO3 update schema or absolute output differs",
    )?;
    let request_bytes = json(&request, MAX_REQUEST_BYTES)?;
    let mut parent = load_parent(&request.input)?;
    require(
        request.learning_rate_bits == parent.learning_rate_bits
            && request.value_coefficient_bits == parent.value_coefficient_bits,
        "BO3 transition/continuation must preserve exact parent scalar bits",
    )?;
    let preparation = read_preparation(&request)?;
    let previous = prior(&request, &parent.origin, parent.completed)?;
    if let Some(previous) = &previous {
        require(
            previous
                .attempt_progress
                .ledger
                .len()
                .checked_add(preparation.attempts.len())
                .is_some_and(|n| n <= MAX_ATTEMPTS),
            "BO3 exact attempt ledger exhausted; explicit extension required",
        )?;
        let requests: BTreeSet<_> = previous
            .attempt_progress
            .ledger
            .iter()
            .map(|x| x[0].as_str())
            .collect();
        let results: BTreeSet<_> = previous
            .attempt_progress
            .ledger
            .iter()
            .map(|x| x[1].as_str())
            .collect();
        for attempt in &preparation.attempts {
            require(
                !requests.contains(attempt.request.sha256.as_str())
                    && !results.contains(attempt.result.sha256.as_str()),
                "attempt request/result already consumed by prior progress",
            )?;
        }
    }
    let request_path = request.output_directory.join("request.json");
    if request.output_directory.try_exists().map_err(err)? {
        require(
            request_path.try_exists().map_err(err)?
                || std::fs::read_dir(&request.output_directory)
                    .map_err(err)?
                    .next()
                    .is_none(),
            "existing nonempty operation directory has no committed request",
        )?;
    } else {
        std::fs::create_dir(&request.output_directory).map_err(err)?;
    }
    let request_pin = publish(
        &request.output_directory,
        "request.json",
        &request,
        MAX_REQUEST_BYTES,
    )?;
    require(
        request_pin == pin_for(request_path, &request_bytes),
        "published request identity differs",
    )?;
    let progress_path = request.output_directory.join("progress.json");
    if progress_path.try_exists().map_err(err)? {
        let pin = existing_pin(&progress_path, MAX_PROGRESS_BYTES)?;
        let progress: Progress = read(&pin, MAX_PROGRESS_BYTES)?;
        require(
            progress.schema == PROGRESS_SCHEMA
                && progress.operation_request == request_pin
                && progress.request == request
                && progress.ordinary_origin == parent.origin,
            "existing progress differs from exact operation",
        )?;
        validate_progress(
            &progress.attempt_progress,
            &request,
            &preparation,
            previous.as_ref(),
        )?;
        if progress.attempt_progress.preparation.disposition == Disposition::NoUpdate {
            require(
                progress.result == *request.input.learner_v1()
                    && progress.completed_bo3_updates == parent.completed,
                "NoUpdate progress changed source/state",
            )?;
        } else {
            let (readback, checkpoint) = load_state(&progress.result.source)?;
            require(
                checkpoint.request == request
                    && checkpoint.operation_request == request_pin
                    && checkpoint.attempt_progress == progress.attempt_progress
                    && readback.completed
                        == parent
                            .completed
                            .checked_add(1)
                            .ok_or("update counter overflow")?
                    && progress.completed_bo3_updates == readback.completed
                    && inference_identity_v1(
                        &progress.result.source,
                        &readback.policy,
                        &readback.state,
                    )? == progress.result.identity,
                "existing progress/checkpoint readback differs",
            )?;
        }
        let verified = publish(
            &request.output_directory,
            "progress.json",
            &progress,
            MAX_PROGRESS_BYTES,
        )?;
        return Ok(result(&progress, verified));
    }
    let checkpoint_path = request.output_directory.join("checkpoint.json");
    if checkpoint_path.try_exists().map_err(err)? {
        let descriptor = match &request.input {
            Bo3GameplayUpdateInputV1::OrdinaryCheckpointTransition { .. } => existing_pin(
                &request.output_directory.join("source.json"),
                MAX_REQUEST_BYTES,
            )?,
            Bo3GameplayUpdateInputV1::Bo3Checkpoint { learner } => {
                learner.source.play_import.clone()
            }
        };
        let source = ExpandedModelSourceV1 {
            play_import: descriptor,
            checkpoint: Some(existing_pin(&checkpoint_path, MAX_CHECKPOINT_BYTES)?),
            feature_transfer: parent.origin.source.feature_transfer.clone(),
        };
        let (readback, checkpoint) = load_state(&source)?;
        require(
            checkpoint.request == request
                && checkpoint.operation_request == request_pin
                && readback.completed
                    == parent
                        .completed
                        .checked_add(1)
                        .ok_or("update counter overflow")?,
            "existing checkpoint differs from exact recovery request",
        )?;
        validate_progress(
            &checkpoint.attempt_progress,
            &request,
            &preparation,
            previous.as_ref(),
        )?;
        let behavior = ExpandedSeatBehaviorV1 {
            identity: inference_identity_v1(&source, &readback.policy, &readback.state)?,
            source,
        };
        return complete(
            &request,
            request_pin,
            parent.origin,
            readback.completed,
            checkpoint.attempt_progress,
            behavior,
        );
    }
    #[cfg(test)]
    PREPARATION_CALLS.with(|n| n.set(n.get() + 1));
    let prepared = prepare_bo3_gameplay_batch_v1(preparation.clone())?;
    let mut ledger = previous
        .as_ref()
        .map(|p| p.attempt_progress.ledger.clone())
        .unwrap_or_default();
    ledger.extend_from_slice(prepared.attempt_identities_v1());
    let report: PreparationReport = strict_json(&json(prepared.report_v1(), 4 * 1024 * 1024)?)?;
    let plan = AttemptProgress {
        attempted_batches: previous
            .as_ref()
            .map_or(0, |p| p.attempt_progress.attempted_batches)
            .checked_add(1)
            .ok_or("attempt counter overflow")?,
        ledger,
        preparation: report,
    };
    validate_progress(&plan, &request, &preparation, previous.as_ref())?;
    if plan.preparation.disposition == Disposition::NoUpdate {
        drop(prepared);
        return complete(
            &request,
            request_pin,
            parent.origin,
            parent.completed,
            plan,
            request.input.learner_v1().clone(),
        );
    }
    let completed = parent
        .completed
        .checked_add(1)
        .ok_or("BO3 update counter overflow")?;
    let expected_step = parent
        .origin
        .identity
        .adam_step
        .checked_add(completed)
        .ok_or("Adam origin counter overflow")?;
    apply_prepared(
        &mut parent.state,
        &prepared,
        request.learning_rate_bits,
        request.value_coefficient_bits,
    )?;
    drop(prepared);
    require(
        parent.state.adam_step_v1() == expected_step,
        "weighted update Adam counter differs",
    )?;
    let descriptor_pin = match &request.input {
        Bo3GameplayUpdateInputV1::OrdinaryCheckpointTransition { .. } => publish(
            &request.output_directory,
            "source.json",
            &Bo3GameplaySourceV1 {
                schema: BO3_GAMEPLAY_SOURCE_SCHEMA_V1.into(),
                objective: BO3_GAMEPLAY_OBJECTIVE_V1.into(),
                normalization: NORMALIZATION.into(),
                ordinary_origin: parent.origin.clone(),
                feature_transfer: parent.origin.source.feature_transfer.clone(),
                learning_rate_bits: request.learning_rate_bits,
                value_coefficient_bits: request.value_coefficient_bits,
            },
            MAX_REQUEST_BYTES,
        )?,
        Bo3GameplayUpdateInputV1::Bo3Checkpoint { learner } => learner.source.play_import.clone(),
    };
    let checkpoint = Bo3GameplayCheckpointV1 {
        schema: BO3_GAMEPLAY_CHECKPOINT_SCHEMA_V1.into(),
        objective: BO3_GAMEPLAY_OBJECTIVE_V1.into(),
        normalization: NORMALIZATION.into(),
        source_descriptor: descriptor_pin.clone(),
        ordinary_origin: parent.origin.clone(),
        operation_request: request_pin.clone(),
        request: request.clone(),
        completed_bo3_updates: completed,
        learning_rate_bits: request.learning_rate_bits,
        value_coefficient_bits: request.value_coefficient_bits,
        state: StateBits::capture(&parent.state)?,
        attempt_progress: plan,
    };
    let checkpoint_pin = publish(
        &request.output_directory,
        "checkpoint.json",
        &checkpoint,
        MAX_CHECKPOINT_BYTES,
    )?;
    let source = ExpandedModelSourceV1 {
        play_import: descriptor_pin,
        checkpoint: Some(checkpoint_pin),
        feature_transfer: parent.origin.source.feature_transfer.clone(),
    };
    let (readback, restored) = load_state(&source)?;
    require(
        restored == checkpoint && StateBits::capture(&readback.state)? == checkpoint.state,
        "BO3 checkpoint full-bit readback differs",
    )?;
    let behavior = ExpandedSeatBehaviorV1 {
        identity: inference_identity_v1(&source, &readback.policy, &readback.state)?,
        source,
    };
    complete(
        &request,
        request_pin,
        parent.origin,
        completed,
        checkpoint.attempt_progress,
        behavior,
    )
}

#[cfg(test)]
#[path = "continuation_tests.rs"]
mod tests;
