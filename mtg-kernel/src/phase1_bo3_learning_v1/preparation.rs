use super::require;
use super::*;
use crate::expanded_deck_training_v1::{
    ExpandedSeatBehaviorV1, PinnedFileV1, load_expanded_inference_v1,
};
use crate::fast_sampler::{WIDE_CATEGORICAL_SAMPLER_VERSION_V1, WideCategoricalScratchV1};
use crate::learned_bo3_v1::Bo3OpeningProtocolV1;
use crate::native_flat_tensorizer_v3::*;
use crate::native_policy_train_step_v1::{
    NativePolicyForwardInputV1, NativePolicyPhysicalDecisionV1, NativePolicySubstepV1,
};
use crate::paired_bo1_harness_v1::paired_policy_seeds_v1;
use crate::phase1_agent_v1::*;
use crate::phase1_bo3_collection_v1::{BO3_COLLECTION_RESULT_SCHEMA_V1, Bo3CollectionConfigV1};
use crate::rl::{PlayerSeatV1, TerminalClassificationV1};
use crate::rl_session::RlSessionTerminalV1;
use crate::sideboard_play_policy_v1::{FrozenPlayPolicyImportV1, FrozenPlayPolicyV1};
use crate::state::SplitMix64;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::io::Read;
use std::path::PathBuf;

pub const BO3_PREPARATION_REQUEST_SCHEMA_V1: &str = "mtg-kernel-bo3-gameplay-preparation/v1";
pub const MAX_BO3_PREPARATION_REQUEST_BYTES_V1: usize = 1024 * 1024;
const MAX_INPUT_BYTES: u64 = 512 * 1024 * 1024;
const MAX_PREPARED_BYTES: u64 = 256 * 1024 * 1024;
const MAX_GROUPS: usize = 65_536;
const MAX_SUBSTEPS: usize = 100_000;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Bo3PreparationLimitsV1 {
    pub max_input_bytes: u64,
    pub max_prepared_payload_bytes: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Bo3AttemptInputV1 {
    pub request: PinnedFileV1,
    pub result: PinnedFileV1,
    pub learner_seat: PlayerSeatV1,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Bo3GameplayPreparationRequestV1 {
    pub schema: String,
    pub learner: ExpandedSeatBehaviorV1,
    /// A fixed ordered list of attempted matches, not a request to backfill
    /// incomplete results until a desired number of winners/completions exists.
    pub attempts: Vec<Bo3AttemptInputV1>,
    pub limits: Bo3PreparationLimitsV1,
}
impl Bo3GameplayPreparationRequestV1 {
    /// Bounded strict JSON shape parsing only. Actual source/producer identity,
    /// attempt eligibility and native replay remain preparation's responsibility.
    pub fn from_json_v1(text: &str) -> Result<Self, String> {
        require(
            text.len() <= MAX_BO3_PREPARATION_REQUEST_BYTES_V1,
            "BO3 preparation request exceeds 1 MiB",
        )?;
        strict_json(text.as_bytes())
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Bo3PreparationDispositionV1 {
    Ready,
    NoUpdate,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Bo3AttemptPreparationReportV1 {
    pub match_id: String,
    pub deck_ids: [String; 2],
    pub learner_seat: PlayerSeatV1,
    pub complete: bool,
    pub eligible: bool,
    pub exclusion_reason: Option<String>,
    pub physical_learner_groups: usize,
    pub captured_substeps: usize,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Bo3GameplayPreparationReportV1 {
    pub disposition: Bo3PreparationDispositionV1,
    pub attempts: Vec<Bo3AttemptPreparationReportV1>,
    pub eligible_matches: usize,
    pub complete_matches: usize,
    pub incomplete_matches: usize,
    pub prepared_payload_bytes: u64,
    pub learner_groups: usize,
    pub learner_substeps: usize,
    pub weight_bits: Vec<u32>,
    pub claim: String,
}
pub(crate) struct PreparedBo3SubstepV1 {
    pub(crate) tensor: NativeFlatDecisionTensorV3,
    pub(crate) logits: Vec<u32>,
    pub(crate) value: u32,
    pub(crate) selected: usize,
}
pub(crate) struct PreparedBo3GroupV1 {
    pub(crate) substeps: Vec<PreparedBo3SubstepV1>,
    pub(crate) match_return: i8,
}
/// Only successful preparation constructs the owned groups. It never mutates
/// an optimizer. NoUpdate must bypass Adam, including its moment decay.
pub struct PreparedBo3GameplayBatchV1 {
    report: Bo3GameplayPreparationReportV1,
    learner: ExpandedSeatBehaviorV1,
    attempts: Vec<Bo3AttemptInputV1>,
    attempt_identities: Vec<[String; 3]>,
    pub(crate) groups: Vec<PreparedBo3GroupV1>,
    weights: Vec<f32>,
}
impl PreparedBo3GameplayBatchV1 {
    pub fn report_v1(&self) -> &Bo3GameplayPreparationReportV1 {
        &self.report
    }
    pub fn learner_v1(&self) -> &ExpandedSeatBehaviorV1 {
        &self.learner
    }
    pub fn attempts_v1(&self) -> &[Bo3AttemptInputV1] {
        &self.attempts
    }
    pub(crate) fn attempt_identities_v1(&self) -> &[[String; 3]] {
        &self.attempt_identities
    }
    pub(crate) fn with_native_groups_v1<T>(
        &self,
        use_groups: impl FnOnce(&[NativePolicyPhysicalDecisionV1<'_>], &[f32]) -> T,
    ) -> T {
        let substeps: Vec<Vec<_>> = self
            .groups
            .iter()
            .map(|group| {
                group
                    .substeps
                    .iter()
                    .map(|step| NativePolicySubstepV1 {
                        forward: NativePolicyForwardInputV1::Encoded(Box::new(
                            encoded_decision_view_v3(&step.tensor),
                        )),
                        selected_action_index: step.selected,
                        expected_raw_action_logit_bits: &step.logits,
                        expected_value_bits: step.value,
                    })
                    .collect()
            })
            .collect();
        let groups: Vec<_> = self
            .groups
            .iter()
            .zip(&substeps)
            .map(|(group, steps)| NativePolicyPhysicalDecisionV1 {
                substeps: steps,
                terminal_return: group.match_return,
                baseline_bits: 0,
            })
            .collect();
        use_groups(&groups, &self.weights)
    }
}

// Reader DTOs deliberately do not deserialize CurrentAgentRuntimeV1. A
// producer's recorded claims are not a certificate for this updater process.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProducerRuntimeClaimV1 {
    executable_path: PathBuf,
    executable_sha256: String,
    engine_commit: String,
    tracked_tree_sha256: String,
    tracked_tree_contract: String,
    toolchain_sha256: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct GameDiagnosticDto {
    game_index: u8,
    environment_seed: Option<u64>,
    observed_terminal: Option<RlSessionTerminalV1>,
    discarded_pending_selections: u64,
    error: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CollectedDto {
    trajectory: Bo3TrainingTrajectoryV1,
    trajectory_sha256: String,
    committed_decision_records: u64,
    committed_decision_json_bytes: u64,
    games: Vec<GameDiagnosticDto>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CollectionResultDto {
    schema: String,
    config: Bo3CollectionConfigV1,
    packages: [CompleteAgentPackageV1; 2],
    current_runtimes: [ProducerRuntimeClaimV1; 2],
    collected: CollectedDto,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct TrainableResultDto {
    schema: String,
    request: TrainableBo3RequestV1,
    result: CollectionResultDto,
    native_capture: Bo3NativeCaptureV1,
}
fn seat(actor: PlayerSeatV1) -> usize {
    if actor == PlayerSeatV1::P0 { 0 } else { 1 }
}
fn pin_shape(pin: &PinnedFileV1) -> Result<(), String> {
    require(
        pin.path.is_absolute()
            && pin.sha256.len() == 64
            && pin
                .sha256
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
        "absolute artifact path and lowercase SHA256 required",
    )
}
pub(super) fn read_bytes(pin: &PinnedFileV1, maximum: u64) -> Result<Vec<u8>, String> {
    pin_shape(pin)?;
    let file = std::fs::File::open(&pin.path).map_err(|e| e.to_string())?;
    require(
        file.metadata().map_err(|e| e.to_string())?.len() <= maximum,
        "pinned file exceeds input bound",
    )?;
    let mut bytes = Vec::new();
    file.take(maximum + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    require(
        bytes.len() as u64 <= maximum,
        "pinned file grew beyond input bound",
    )?;
    require(
        format!("{:x}", Sha256::digest(&bytes)) == pin.sha256,
        "artifact hash differs",
    )?;
    Ok(bytes)
}
fn verify_file(pin: &PinnedFileV1) -> Result<(), String> {
    pin_shape(pin)?;
    let mut file = std::fs::File::open(&pin.path).map_err(|e| e.to_string())?;
    require(
        file.metadata().map_err(|e| e.to_string())?.len() <= MAX_INPUT_BYTES,
        "producer artifact exceeds 512 MiB bound",
    )?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 65536];
    loop {
        let n = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }
    require(
        format!("{:x}", hasher.finalize()) == pin.sha256,
        "producer artifact hash differs",
    )
}
pub(super) fn strict_json<T: DeserializeOwned + Serialize>(bytes: &[u8]) -> Result<T, String> {
    let text = std::str::from_utf8(bytes).map_err(|e| e.to_string())?;
    let raw = crate::rl::parse_strict_json_value(text).map_err(|e| e.to_string())?;
    let result: T = serde_json::from_str(text).map_err(|e| e.to_string())?;
    // Also reject ignored fields in reused legacy identity structs that do
    // not themselves carry deny_unknown_fields. Producer output is canonical.
    let typed = serde_json::to_value(&result).map_err(|e| e.to_string())?;
    fn known_fields(raw: &serde_json::Value, typed: &serde_json::Value) -> bool {
        match (raw, typed) {
            (serde_json::Value::Object(a), serde_json::Value::Object(b)) => a
                .iter()
                .all(|(key, value)| b.get(key).is_some_and(|other| known_fields(value, other))),
            (serde_json::Value::Array(a), serde_json::Value::Array(b)) => {
                a.len() == b.len() && a.iter().zip(b).all(|(a, b)| known_fields(a, b))
            }
            _ => true,
        }
    }
    // Compare field topology, not Value's f32-to-f64 number representation.
    require(known_fields(&raw, &typed), "unknown field in producer JSON")?;
    Ok(result)
}
pub(crate) fn ordinary_source(behavior: &ExpandedSeatBehaviorV1) -> Result<(), String> {
    require(
        behavior.source.checkpoint.is_some(),
        "BO3 preparation requires an ordinary existing checkpoint",
    )?;
    let bytes = read_bytes(&behavior.source.play_import, 4 * 1024 * 1024)?;
    let _: FrozenPlayPolicyImportV1 = strict_json(&bytes)
        .map_err(|e| format!("ordinary import required; registry-transfer continuation needs an explicit BO3 schedule transition: {e}"))?;
    Ok(())
}
pub(crate) fn admitted_source(behavior: &ExpandedSeatBehaviorV1) -> Result<(), String> {
    let bytes = read_bytes(&behavior.source.play_import, 4 * 1024 * 1024)?;
    let probe: serde_json::Value = strict_json(&bytes)?;
    if probe.get("schema").and_then(serde_json::Value::as_str)
        == Some(super::BO3_GAMEPLAY_SOURCE_SCHEMA_V1)
    {
        super::continuation::validate_bo3_source_admission_v1(&behavior.source, &bytes)
    } else {
        ordinary_source(behavior)
    }
}
fn load_actual(behavior: &ExpandedSeatBehaviorV1) -> Result<FrozenPlayPolicyV1, String> {
    admitted_source(behavior)?;
    let (policy, actual) = load_expanded_inference_v1(&behavior.source)?;
    require(
        actual == behavior.identity
            && policy.runtime_sampler_identity_v1() == WIDE_CATEGORICAL_SAMPLER_VERSION_V1,
        "actual checkpoint/model/Adam identity differs from expected behavior",
    )?;
    Ok(policy)
}
fn verify_producer(result: &CollectionResultDto) -> Result<(), String> {
    for (package, claim) in result.packages.iter().zip(&result.current_runtimes) {
        package.validate_metadata_v1()?;
        let runtime = &package.runtime;
        verify_file(&runtime.executable)?;
        verify_file(&runtime.toolchain)?;
        verify_file(&PinnedFileV1 {
            path: claim.executable_path.clone(),
            sha256: claim.executable_sha256.clone(),
        })?;
        require(
            runtime.build_git_clean
                && claim.executable_sha256 == runtime.executable.sha256
                && claim.engine_commit == runtime.engine_commit
                && claim.tracked_tree_sha256 == runtime.tracked_tree_sha256
                && claim.tracked_tree_contract == runtime.tracked_tree_contract
                && claim.toolchain_sha256 == runtime.toolchain.sha256,
            "producer runtime claims differ from pinned package",
        )?;
        require(
            runtime.feature_contract_digest == FEATURE_CONTRACT_DIGEST_V3
                && runtime.feature_encoding_digest == FEATURE_ENCODING_DIGEST_V3
                && runtime.features_source_sha256 == FEATURES_SOURCE_SHA256_V3
                && runtime.feature_descriptor_sha256 == FEATURE_DESCRIPTOR_SHA256_V3
                && runtime.card_db_hash == format!("{:016x}", crate::card_def::KERNEL_CARDDB_HASH)
                && runtime.card_registry_sha256
                    == format!(
                        "{:x}",
                        Sha256::digest(include_bytes!("../../../data/cards_v1.json"))
                    ),
            "producer feature/registry differs from preparation runtime",
        )?;
    }
    Ok(())
}
fn hash_json<T: Serialize>(value: &T) -> Result<String, String> {
    struct Sink(Sha256);
    impl std::io::Write for Sink {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.update(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut sink = Sink(Sha256::new());
    serde_json::to_writer(&mut sink, value).map_err(|e| e.to_string())?;
    Ok(format!("{:x}", sink.0.finalize()))
}

/// Deliberately separate from package/provenance identity. In this fixed
/// Keep-only slice labels, caps, summary tags, transport paths, ancestry and
/// Adam history cannot turn the same seeded gameplay into a fresh sample.
fn physical_match_identity(
    config: &Bo3CollectionConfigV1,
    packages: [&CompleteAgentPackageV1; 2],
) -> Result<String, String> {
    #[derive(Serialize)]
    struct SeatBehavior<'a> {
        model: &'a crate::sideboard_play_policy_v1::PlayModelIdentityV1,
        card_registry_sha256: &'a str,
        feature_schema_version: &'a str,
        feature_registry_version: &'a str,
        features_source_sha256: &'a str,
        feature_descriptor_sha256: &'a str,
        sampler: &'a str,
        opening: &'a AgentOpeningPolicyV1,
        play_draw: &'a AgentPlayDrawPolicyV1,
        sideboard: &'a AgentSideboardPolicyV1,
        search: &'a AgentSearchPolicyV1,
    }
    for package in packages {
        require(
            matches!(
                package.opening,
                AgentOpeningPolicyV1::Existing {
                    protocol: Bo3OpeningProtocolV1::KeepSevenV2
                }
            ) && matches!(package.play_draw, AgentPlayDrawPolicyV1::Fixed { .. })
                && matches!(package.sideboard, AgentSideboardPolicyV1::Keep)
                && matches!(package.search, AgentSearchPolicyV1::Disabled),
            "physical identity only supports the fixed Keep-only behavior scope",
        )?;
    }
    let behaviors = packages.map(|package| {
        let identity = &package.gameplay.identity;
        SeatBehavior {
            model: &identity.model,
            card_registry_sha256: &package.runtime.card_registry_sha256,
            feature_schema_version: &identity.feature_schema_version,
            feature_registry_version: &identity.feature_registry_version,
            features_source_sha256: &identity.features_source_sha256,
            feature_descriptor_sha256: &identity.feature_descriptor_sha256,
            sampler: &package.gameplay_sampler_identity,
            opening: &package.opening,
            play_draw: &package.play_draw,
            sideboard: &package.sideboard,
            search: &package.search,
        }
    });
    hash_json(&(
        "mtg-kernel-bo3-keep-physical-sample/v1",
        config.seed,
        config.initial_chooser,
        &config.registrations,
        behaviors,
    ))
}

/// Returns owned learner-only physical groups after exact native forward and
/// sampler replay. This is not engine replay or proof of an external producer's
/// execution: callers retain the trusted pinned original-collector boundary.
pub fn prepare_bo3_gameplay_batch_v1(
    request: Bo3GameplayPreparationRequestV1,
) -> Result<PreparedBo3GameplayBatchV1, String> {
    require(
        request.schema == BO3_PREPARATION_REQUEST_SCHEMA_V1
            && (1..=32).contains(&request.attempts.len()),
        "preparation schema or attempt count differs",
    )?;
    require(
        (1..=MAX_INPUT_BYTES).contains(&request.limits.max_input_bytes)
            && (1..=MAX_PREPARED_BYTES).contains(&request.limits.max_prepared_payload_bytes),
        "preparation byte limit differs",
    )?;
    super::canonical_size(&request, 1024 * 1024)?;
    let mut declared_requests = BTreeSet::new();
    let mut declared_results = BTreeSet::new();
    for attempt in &request.attempts {
        pin_shape(&attempt.request)?;
        pin_shape(&attempt.result)?;
        require(
            declared_requests.insert(&attempt.request.sha256)
                && declared_results.insert(&attempt.result.sha256),
            "duplicate attempted artifact",
        )?;
    }
    let learner = load_actual(&request.learner)?;
    let mut opponent_cache: Option<(ExpandedSeatBehaviorV1, FrozenPlayPolicyV1)> = None;
    let mut reports = Vec::new();
    let mut groups = Vec::new();
    let mut ids = BTreeSet::new();
    let mut result_hashes = BTreeSet::new();
    let mut request_hashes = BTreeSet::new();
    let mut identities = BTreeSet::new();
    let mut attempt_identities = Vec::new();
    let mut consumed_bytes = 0_u64;
    let mut prepared_bytes = 0_u64;
    let mut substep_count = 0_usize;
    for attempt in &request.attempts {
        require(
            result_hashes.insert(attempt.result.sha256.clone())
                && request_hashes.insert(attempt.request.sha256.clone()),
            "duplicate attempted artifact",
        )?;
        let input = read_bytes(&attempt.request, 4 * 1024 * 1024)?;
        consumed_bytes = consumed_bytes
            .checked_add(input.len() as u64)
            .ok_or("input size overflow")?;
        require(
            consumed_bytes <= request.limits.max_input_bytes,
            "cumulative input bound exceeded",
        )?;
        let source_request: TrainableBo3RequestV1 = strict_json(&input)?;
        source_request.validate()?;
        drop(input);
        let input = read_bytes(
            &attempt.result,
            request.limits.max_input_bytes - consumed_bytes,
        )?;
        consumed_bytes += input.len() as u64;
        let result: TrainableResultDto = strict_json(&input)?;
        drop(input);
        require(
            result.schema == TRAINABLE_BO3_RESULT_SCHEMA_V1
                && result.request == source_request
                && result.result.schema == BO3_COLLECTION_RESULT_SCHEMA_V1
                && result.result.config == source_request.config
                && result.result.packages == source_request.packages,
            "trainable result/request binding differs",
        )?;
        verify_producer(&result.result)?;
        require(
            ids.insert(result.result.config.match_id.clone()),
            "duplicate match id",
        )?;
        let physical_identity =
            physical_match_identity(&result.result.config, result.result.packages.each_ref())?;
        require(
            identities.insert(physical_identity.clone()),
            "duplicate physical match identity",
        )?;
        attempt_identities.push([
            attempt.request.sha256.clone(),
            attempt.result.sha256.clone(),
            physical_identity,
        ]);
        let actor = seat(attempt.learner_seat);
        require(
            result.result.packages[actor].gameplay == request.learner,
            "stale or different learner source/model/Adam state",
        )?;
        let other = &result.result.packages[1 - actor].gameplay;
        if opponent_cache
            .as_ref()
            .is_none_or(|(behavior, _)| behavior != other)
        {
            drop(opponent_cache.take());
            opponent_cache = Some((other.clone(), load_actual(other)?));
        }
        let opponent = &opponent_cache.as_ref().unwrap().1;
        let policies = if actor == 0 {
            [&learner, opponent]
        } else {
            [opponent, &learner]
        };
        let prepared = replay_attempt(
            result,
            attempt.learner_seat,
            policies,
            request
                .limits
                .max_prepared_payload_bytes
                .saturating_sub(prepared_bytes),
        )?;
        substep_count = substep_count
            .checked_add(prepared.substeps)
            .ok_or("substep count overflow")?;
        require(
            substep_count <= MAX_SUBSTEPS
                && groups.len().saturating_add(prepared.groups.len()) <= MAX_GROUPS,
            "prepared group/substep count bound exceeded",
        )?;
        prepared_bytes += prepared.payload;
        reports.push(prepared.report);
        groups.extend(prepared.groups);
    }
    let mut prepared = finish_prepared(
        request.learner,
        reports,
        groups,
        prepared_bytes,
        substep_count,
    )?;
    prepared.attempts = request.attempts;
    prepared.attempt_identities = attempt_identities;
    Ok(prepared)
}

fn finish_prepared(
    learner: ExpandedSeatBehaviorV1,
    reports: Vec<Bo3AttemptPreparationReportV1>,
    groups: Vec<PreparedBo3GroupV1>,
    prepared_bytes: u64,
    substep_count: usize,
) -> Result<PreparedBo3GameplayBatchV1, String> {
    let match_group_counts: Vec<_> = reports
        .iter()
        .filter(|r| r.eligible)
        .map(|r| r.physical_learner_groups)
        .collect();
    require(
        match_group_counts.iter().all(|&count| count > 0),
        "eligible match lacks groups",
    )?;
    let eligible = match_group_counts.len();
    let weights: Vec<f32> = match_group_counts
        .iter()
        .flat_map(|&count| {
            // Versioned order: f64 1/N/G, then one cast to f32. Persist these bits.
            std::iter::repeat_n((1.0_f64 / eligible as f64 / count as f64) as f32, count)
        })
        .collect();
    require(
        weights.iter().all(|w| w.is_finite() && *w > 0.0) && weights.len() == groups.len(),
        "derived physical-group weights invalid",
    )?;
    let complete_matches = reports.iter().filter(|r| r.complete).count();
    let report = Bo3GameplayPreparationReportV1 {
        disposition: if eligible == 0 { Bo3PreparationDispositionV1::NoUpdate } else { Bo3PreparationDispositionV1::Ready },
        incomplete_matches: reports.len() - complete_matches, complete_matches, attempts: reports,
        eligible_matches: eligible, prepared_payload_bytes: prepared_bytes, learner_groups: groups.len(),
        learner_substeps: substep_count, weight_bits: weights.iter().map(|w| w.to_bits()).collect(),
        claim: "Exact original-tensor forward/sampler replay and conditional-on-completion gameplay preparation only; no optimizer, engine-legality proof, or strength claim".into(),
    };
    Ok(PreparedBo3GameplayBatchV1 {
        report,
        learner,
        attempts: Vec::new(),
        attempt_identities: Vec::new(),
        groups,
        weights,
    })
}

struct AttemptPrepared {
    report: Bo3AttemptPreparationReportV1,
    groups: Vec<PreparedBo3GroupV1>,
    payload: u64,
    substeps: usize,
}

fn replay_attempt(
    result: TrainableResultDto,
    learner: PlayerSeatV1,
    policies: [&FrozenPlayPolicyV1; 2],
    remaining_bytes: u64,
) -> Result<AttemptPrepared, String> {
    let config = &result.result.config;
    let collected = &result.result.collected;
    require(
        collected.trajectory.match_id == config.match_id
            && collected.trajectory.initial_chooser == config.initial_chooser
            && collected.trajectory.registrations_by_seat == config.registrations,
        "trajectory match id, chooser or registrations differ from requested config",
    )?;
    for (package, policy) in result.result.packages.iter().zip(policies) {
        require(
            package.gameplay_sampler_identity == policy.runtime_sampler_identity_v1()
                && package.gameplay_sampler_identity == WIDE_CATEGORICAL_SAMPLER_VERSION_V1,
            "package sampler differs from actual loaded WIDE behavior",
        )?;
    }
    let validated = collected
        .trajectory
        .validate_v1(result.result.packages.each_ref())?;
    let complete = validated.is_complete_v1();
    let reward = validated.match_return_v1(learner).map(|r| r as i8);
    require(
        hash_json(&collected.trajectory)? == collected.trajectory_sha256,
        "trajectory digest differs",
    )?;
    require(
        result.native_capture.schema == BO3_NATIVE_CAPTURE_SCHEMA_V1
            && collected.games.len() == collected.trajectory.games.len(),
        "capture schema or diagnostic count differs",
    )?;
    let mut captures = result.native_capture.records.into_iter();
    let mut groups = Vec::new();
    let mut payload = 0_u64;
    let mut total_payload = 0_u64;
    let mut total_json = 0_u64;
    let mut decision_json = 0_u64;
    let mut decisions = 0_u64;
    let mut native_count = 0;
    let mut prepared_substeps = 0;
    let mut seed_stream = SplitMix64::seed(config.seed);
    for (game_ordinal, (game, diagnostic)) in collected
        .trajectory
        .games
        .iter()
        .zip(&collected.games)
        .enumerate()
    {
        require(
            diagnostic.game_index == game.game_index,
            "game diagnostic ordinal differs",
        )?;
        let expected_seed = game.start.map(|_| seed_stream.next_u64());
        require(
            diagnostic.environment_seed == expected_seed,
            "recorded environment seed differs from match stream",
        )?;
        let mut rng = paired_policy_seeds_v1(expected_seed.unwrap_or(0)).map(SplitMix64::seed);
        let mut sampler = WideCategoricalScratchV1::default();
        if complete {
            require(
                diagnostic.error.is_none() && diagnostic.discarded_pending_selections == 0,
                "complete match contains failed/discarded selections",
            )?;
        }
        if let Some(terminal) = &game.terminal {
            let observed = diagnostic
                .observed_terminal
                .as_ref()
                .ok_or("game terminal lacks producer diagnostic")?;
            require(
                observed.episode_id == u64::from(game.game_index)
                    && observed.terminal_classification == terminal.classification
                    && observed.terminal_outcome == terminal.outcome
                    && observed.policy_step_count == terminal.gameplay_decision_count,
                "observed/core terminal differs",
            )?;
            require(
                observed.schema_version == crate::rl_session::RL_SESSION_SCHEMA_VERSION
                    && observed.deck_ids == config.deck_ids
                    && observed.deck_hashes
                        == config
                            .registrations
                            .each_ref()
                            .map(|deck| crate::rl_session::explicit_deck_hash_v1(&deck.mainboard))
                    && observed.terminal_code == crate::rl::TerminalSafeCodeV2::NaturalGameOver
                    && crate::rl::terminal_tuple_is_valid_v1(
                        observed.terminal_outcome,
                        observed.terminal_classification,
                        observed.winner,
                        observed.terminal_reward,
                    ),
                "producer terminal schema/deck/reward tuple differs",
            )?;
        }
        let mut physical_id = 0_u64;
        let mut active_actor = None;
        let mut expected_substeps = 0_u32;
        let mut active_substeps = 0_u32;
        let mut active = Vec::new();
        for decision in &game.decisions {
            decisions += 1;
            decision_json = decision_json
                .checked_add(super::canonical_size(
                    decision,
                    config.max_decision_json_bytes.saturating_sub(decision_json),
                )?)
                .ok_or("decision size overflow")?;
            let ActorVisibleDecisionV1::Gameplay {
                observation,
                ordered_actions,
            } = &decision.visible
            else {
                continue;
            };
            let captured = captures.next().ok_or("gameplay row lacks native witness")?;
            native_count += 1;
            require(
                native_count <= MAX_SUBSTEPS && captured.decision_index == decision.decision_index,
                "native witness is missing, duplicated, reordered or assigned to an auxiliary",
            )?;
            let size = captured.payload_bytes()?;
            require(
                size <= 16 * 1024 * 1024,
                "native record payload exceeds bound",
            )?;
            total_payload = total_payload
                .checked_add(size)
                .ok_or("capture payload overflow")?;
            total_json = total_json
                .checked_add(super::canonical_size(
                    &captured,
                    result
                        .request
                        .capture_limits
                        .max_json_bytes
                        .saturating_sub(total_json),
                )?)
                .ok_or("capture JSON overflow")?;
            require(
                total_payload <= result.request.capture_limits.max_payload_bytes,
                "native capture payload exceeds request",
            )?;
            if active_substeps == 0 {
                require(
                    observation.physical_decision_id == physical_id
                        && observation.substep_index == 0,
                    "physical group start differs",
                )?;
                expected_substeps = observation.substep_count;
                active_actor = Some(decision.actor);
            }
            require(
                observation.physical_decision_id == physical_id
                    && observation.substep_index == active_substeps
                    && observation.substep_count == expected_substeps
                    && active_actor == Some(decision.actor),
                "physical group actor/substep sequence differs",
            )?;
            let selected = decision.behavior.selected_index_v1();
            require(
                captured.raw_logit_bits.len() == ordered_actions.len(),
                "native/visible action width differs",
            )?;
            let tensor = captured.tensor_bits.into_tensor();
            let scores = policies[seat(decision.actor)].score_training_tensor_v3(&tensor)?;
            require(
                scores.value.to_bits() == captured.raw_value_bits
                    && scores
                        .logits
                        .iter()
                        .map(|v| v.to_bits())
                        .collect::<Vec<_>>()
                        == captured.raw_logit_bits,
                "captured tensor does not reproduce original model outputs",
            )?;
            let expected_behavior =
                BehaviorDistributionV1::hamilton_from_logits_v1(&scores.logits, selected as u32)?;
            require(
                decision.behavior == expected_behavior,
                "recorded Hamilton masses differ from original logits",
            )?;
            require(
                sampler
                    .sample(&scores.logits, rng[seat(decision.actor)].next_u64())
                    .map_err(|e| e.to_string())?
                    == selected,
                "recorded selection differs from actual per-seat sampler stream",
            )?;
            if complete && decision.actor == learner {
                payload = payload
                    .checked_add(size)
                    .ok_or("prepared payload overflow")?;
                require(
                    payload <= remaining_bytes,
                    "cumulative prepared native payload limit exceeded",
                )?;
                prepared_substeps += 1;
                active.push(PreparedBo3SubstepV1 {
                    tensor,
                    logits: captured.raw_logit_bits,
                    value: captured.raw_value_bits,
                    selected,
                });
            }
            active_substeps += 1;
            if active_substeps == expected_substeps {
                if complete && decision.actor == learner {
                    groups.push(PreparedBo3GroupV1 {
                        substeps: std::mem::take(&mut active),
                        match_return: reward.unwrap(),
                    });
                    require(
                        groups.len() <= MAX_GROUPS,
                        "prepared physical groups exceed bound",
                    )?;
                }
                physical_id += 1;
                active_substeps = 0;
            }
        }
        require(
            active_substeps == 0
                || (!complete
                    && game_ordinal + 1 == collected.trajectory.games.len()
                    && game.terminal.is_none()),
            "complete physical group is missing substeps",
        )?;
        if let Some(terminal) = &game.terminal {
            require(
                terminal.classification == TerminalClassificationV1::Natural,
                "trainable collector cannot publish a capped game terminal",
            )?;
            require(
                diagnostic
                    .observed_terminal
                    .as_ref()
                    .unwrap()
                    .physical_decision_count
                    == physical_id,
                "observed physical decision count differs",
            )?;
        }
    }
    require(
        captures.next().is_none(),
        "native witnesses remain without gameplay rows",
    )?;
    require(
        total_payload == result.native_capture.committed_payload_bytes
            && total_json == result.native_capture.committed_json_bytes
            && decisions == collected.committed_decision_records
            && decision_json == collected.committed_decision_json_bytes
            && decisions <= config.max_decision_records,
        "native/core accounting differs",
    )?;
    let eligible = complete && !groups.is_empty();
    let exclusion = if complete {
        (!eligible).then(|| "no learner gameplay groups".into())
    } else {
        Some(format!("{:?}", collected.trajectory.ending))
    };
    Ok(AttemptPrepared {
        report: Bo3AttemptPreparationReportV1 {
            match_id: config.match_id.clone(),
            deck_ids: config.deck_ids.clone(),
            learner_seat: learner,
            complete,
            eligible,
            exclusion_reason: exclusion,
            physical_learner_groups: groups.len(),
            captured_substeps: native_count,
        },
        groups,
        payload,
        substeps: prepared_substeps,
    })
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
