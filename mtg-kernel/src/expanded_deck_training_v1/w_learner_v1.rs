//! The wide net-8 learner in RT1 (`kernel-policy-value-net-8w128`, collab
//! LANES/w128-20261113/DESIGN.md): an `ExpandedModelSourceV1` with
//! `w_learner` collects, updates and resumes with the wide net in place of
//! net-8, the way W5's native S learner (`s_learner_v1`) does.
//!
//! **Source.** The play import and feature transfer load the plain FlatV4
//! shell (a fresh-initialization import, as every FlatV4 lineage's): its
//! encoder, tensorizer, samplers and RNG streams are used unchanged, and the
//! wide net is installed on it (`install_wide_model_v1`), so every forward of
//! the learner is the wide net's. `w_learner.initial` pins the lineage's
//! starting train state, a [`W_STATE_MAGIC_V1`] container. Unlike S's,
//! it is a whole train state, used as is: parameters, both Adam moments,
//! `adam_step` and the scorer-bias anchor (a widened net-8 state keeps its
//! old coordinates' optimizer history; a fresh start carries zero moments).
//! With a checkpoint, `checkpoint` pins a wide checkpoint record
//! ([`WLearnerCheckpointV1`]) that names the update's container and binds
//! its inputs.
//!
//! **Opponents.** Net-8 opponents load and play exactly as in a net-8 run;
//! a wide source as an opponent (the current or a segment-start learner)
//! loads through [`load_inference_v1`].
//!
//! **Update.** The ordinary CPU GAE update (`gae_advantage_value/v1`,
//! entropy 0) with the wide train state (`NativePolicyValueTrainStateWideV1`,
//! the generic core at `Net8W128DimsV1`): the same trajectory checks and
//! replay as net-8's (serial or prepared), the same GAE targets, and the core
//! checks every learner row's collected logits and value against its own
//! forward. The receipt has the net-8 update's fields, so the run driver
//! validates and chains it unchanged.
//!
//! **Platform libm only** (the wide net has no other math). **Refused**
//! (explicit errors): portable math, line (b), E supervision, SQ1, TP1
//! search targets, EX1 and W5 search, the CUDA backend, fixed-partition
//! backward, the terminal and recovery losses, checkpoint record format v2,
//! verified replay, FlatV7 and FlatV8, the no-state-digest contract,
//! registry-transfer and registry-evolution shells, the line (b) collection
//! sampler, and every net-8-only command.

use super::{
    compute_gae_targets_of_values_v1, ensure, err, inference_schema_v1, ordered_update_preparation,
    publish_json, publish_json_bytes_v1, read_pinned_bytes, read_trajectory_batch_v1,
    replay_learner_groups_v1, validate_actual_behaviors_v1, validate_trajectory,
    CheckpointInputExpectationV1, CheckpointRecordFormatV1, CollectionSamplerV1,
    ExpandedInferenceIdentityV1, ExpandedLossSelectionV1, ExpandedModelSourceV1,
    ExpandedSeatBehaviorV1, ExpandedUpdateBackendV1, OpponentCacheV1, PinnedFileV1,
    UpdateBackwardExecutionV1, GAE_ADVANTAGE_VALUE_LOSS_IDENTITY_V1, MAX_BATCH_BYTES,
};
use crate::card_def::KERNEL_CARDDB_HASH;
use crate::native_flat_tensorizer_v4::{FEATURE_CONTRACT_DIGEST_V4, FEATURE_ENCODING_DIGEST_V4};
use crate::native_policy_train_step_v1::{
    NativePolicyForwardInputV1, NativePolicyPhysicalDecisionV1, NativePolicySubstepV1,
    NativePolicyValueTrainStateWideV1,
};
use crate::native_policy_value_net_v1::{
    NativePolicyValueModelConfigV1, NativePolicyValueNetWideV1, W_MODEL_ARCHITECTURE_VERSION_V1,
};
use crate::native_train_state_payload_v1::{
    decode_native_train_state_payload_wide_v1, encode_native_train_state_payload_wide_v1,
};
use crate::native_training_phase_diagnostic_v1::NativeTrainingPhaseRecorderV1;
use crate::sideboard_play_policy_v1::{
    FreshLineageGenerationV1, FrozenPlayPolicyV1, PlayPolicyOriginV1, FRESH_FEATURE_IDENTITY_V4,
};
use crate::training_math_v1::TrainingMathV1;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::fs;
use std::sync::Arc;

/// `WLearnerSourceV1::schema`.
pub const W_LEARNER_SOURCE_SCHEMA_V1: &str = "w128-learner/v1";
/// The wide checkpoint record's schema.
pub const W_LEARNER_CHECKPOINT_SCHEMA_V1: &str = "w128-learner-checkpoint/v1";
/// The wide container file each update publishes beside its record.
pub const W_LEARNER_STATE_FILE_V1: &str = "w128-train-state.bin";
/// The net-8 loader's refusal of a wide source.
pub(super) const W_LEARNER_NET8_REFUSAL_V1: &str =
    "a wide (net-8w128) learner source loads only through the wide learner paths (Collect, \
     CollectParallel, Update, UpdatePrepared and inference); this command is net-8's";

/// The wide train-state container: this magic, then `u32` version 1, the
/// `u32` scorer-bias anchor bits, the `u64` Adam step (all little-endian),
/// then the wide payload (`encode_native_train_state_payload_wide_v1`:
/// parameters, first and second moments, f32le, no header).
pub const W_STATE_MAGIC_V1: &[u8; 8] = b"MTGW128\0";
const W_STATE_VERSION_V1: u32 = 1;
const W_STATE_HEADER_BYTES_V1: usize = 24;

/// Encodes a wide train state as its container.
pub(crate) fn encode_w_state_v1(
    state: &NativePolicyValueTrainStateWideV1,
) -> Result<Vec<u8>, String> {
    let snapshot = state.snapshot_v1().map_err(err)?;
    let payload = encode_native_train_state_payload_wide_v1(&snapshot).map_err(err)?;
    let mut bytes = Vec::with_capacity(W_STATE_HEADER_BYTES_V1 + payload.bytes.len());
    bytes.extend_from_slice(W_STATE_MAGIC_V1);
    bytes.extend_from_slice(&W_STATE_VERSION_V1.to_le_bytes());
    bytes.extend_from_slice(&snapshot.scorer_bias_anchor_bits.to_le_bytes());
    bytes.extend_from_slice(&snapshot.adam_step.to_le_bytes());
    bytes.extend_from_slice(&payload.bytes);
    Ok(bytes)
}

/// Decodes a container written by [`encode_w_state_v1`] into a validated
/// wide train state. Any other bytes are refused, and the bytes must be the
/// state's own encoding.
pub(crate) fn decode_w_state_v1(bytes: &[u8]) -> Result<NativePolicyValueTrainStateWideV1, String> {
    ensure(
        bytes.len() > W_STATE_HEADER_BYTES_V1 && &bytes[..8] == W_STATE_MAGIC_V1,
        "not a wide (net-8w128) train-state container",
    )?;
    let word = |at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
    ensure(
        word(8) == W_STATE_VERSION_V1,
        "unknown wide train-state container version",
    )?;
    let anchor = word(12);
    let adam_step = u64::from_le_bytes(bytes[16..24].try_into().unwrap());
    let decoded = decode_native_train_state_payload_wide_v1(
        &bytes[W_STATE_HEADER_BYTES_V1..],
        adam_step,
        anchor,
    )
    .map_err(err)?;
    let template = NativePolicyValueNetWideV1::runner_fixed_wide_v1(
        NativePolicyValueModelConfigV1::contract_wide_v1(),
    )
    .map_err(err)?;
    let state =
        NativePolicyValueTrainStateWideV1::from_snapshot_wide_v1(template, &decoded.snapshot)
            .map_err(err)?;
    ensure(
        encode_w_state_v1(&state)? == bytes,
        "wide train-state container is not its state's encoding",
    )?;
    Ok(state)
}

/// The hex `state_sha256` of a wide train state.
pub(crate) fn w_state_sha256_v1(
    state: &NativePolicyValueTrainStateWideV1,
) -> Result<String, String> {
    Ok(hex_v1(&state.state_sha256_v1().map_err(err)?))
}

fn hex_v1(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// The `w_learner` member of a source.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WLearnerSourceV1 {
    pub schema: String,
    /// The lineage's starting train state, a [`W_STATE_MAGIC_V1`]
    /// container, used whole (parameters and optimizer state).
    pub initial: PinnedFileV1,
}

/// The record `checkpoint` pins in a wide source: the container and the
/// update that produced it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WLearnerCheckpointV1 {
    pub schema: String,
    pub architecture: String,
    /// `w_learner.initial`'s sha256: the lineage's start.
    pub initial_sha256: String,
    /// The shell's import identity.
    pub source_import: PlayPolicyOriginV1,
    pub feature_contract_digest: String,
    pub feature_encoding_digest: String,
    pub card_db_hash: String,
    /// The wide container.
    pub state: PinnedFileV1,
    pub state_sha256: String,
    pub adam_step: u64,
    pub trajectories: Vec<PinnedFileV1>,
    pub loss_identity: String,
    pub gamma_bits: u32,
    pub gae_lambda_bits: u32,
    pub entropy_coefficient_bits: u32,
    pub learning_rate_bits: u32,
    pub value_coefficient_bits: u32,
}

/// A loaded wide learner: the shell with the wide net installed, the wide
/// train state, and the checkpoint record it resumed from.
pub(super) struct LoadedWV1 {
    pub(super) policy: FrozenPlayPolicyV1,
    pub(super) state: NativePolicyValueTrainStateWideV1,
    pub(super) checkpoint: Option<WLearnerCheckpointV1>,
}

fn w_member_v1(source: &ExpandedModelSourceV1) -> Result<&WLearnerSourceV1, String> {
    let member = source
        .w_learner
        .as_deref()
        .ok_or("the source has no wide learner")?;
    ensure(
        member.schema == W_LEARNER_SOURCE_SCHEMA_V1,
        "unknown wide learner source schema",
    )?;
    ensure(
        source.s_learner.is_none(),
        "a source has at most one of the S and wide learners",
    )?;
    Ok(member)
}

/// Loads `source`'s shell, wide train state and checkpoint record. With
/// `inputs`, the record must bind exactly those trajectories and scalars.
pub(super) fn load_w_v1(
    source: &ExpandedModelSourceV1,
    inputs: Option<&CheckpointInputExpectationV1<'_>>,
) -> Result<LoadedWV1, String> {
    let member = w_member_v1(source)?;
    ensure(
        source.feature_transfer.expected_feature_contract_digest == FEATURE_CONTRACT_DIGEST_V4
            && source.feature_transfer.expected_feature_encoding_digest
                == FEATURE_ENCODING_DIGEST_V4,
        "a wide learner runs on the plain FlatV4 contract; FlatV7 and FlatV8 are refused",
    )?;
    let bytes = read_pinned_bytes(&source.play_import)?;
    let probe: Value = serde_json::from_slice(&bytes).map_err(err)?;
    let schema = probe.get("schema").and_then(Value::as_str);
    ensure(
        schema == Some(super::fresh_initialization_source::SOURCE_SCHEMA),
        "a wide learner's shell is a fresh-initialization (FlatV4) import; ordinary V3, \
         transfer, registry-evolution, BO3 and no-state-digest imports are refused",
    )?;
    let mut policy = super::load_ordinary_policy_v1(source, &bytes)?;
    ensure(
        policy.feature_identity_v1() == FRESH_FEATURE_IDENTITY_V4
            && policy.input_contract_v1().is_none(),
        "a wide learner's shell must load with the plain FlatV4 identity",
    )?;
    let initial = decode_w_state_v1(&read_pinned_bytes(&member.initial)?)?;
    let (state, checkpoint) = match &source.checkpoint {
        None => {
            ensure(inputs.is_none(), "update requires checkpoint")?;
            (initial, None)
        }
        Some(pin) => {
            let record: WLearnerCheckpointV1 =
                serde_json::from_slice(&read_pinned_bytes(pin)?).map_err(err)?;
            let identity = policy.feature_identity_v1();
            ensure(
                record.schema == W_LEARNER_CHECKPOINT_SCHEMA_V1
                    && record.architecture == W_MODEL_ARCHITECTURE_VERSION_V1
                    && record.initial_sha256 == member.initial.sha256
                    && record.source_import == *policy.identity_v1()
                    && record.feature_contract_digest == identity.feature_contract_digest
                    && record.feature_encoding_digest == identity.feature_encoding_digest
                    && record.card_db_hash == format!("{KERNEL_CARDDB_HASH:016x}")
                    && record.loss_identity == GAE_ADVANTAGE_VALUE_LOSS_IDENTITY_V1,
                "wide checkpoint record does not belong to this source",
            )?;
            if let Some(inputs) = inputs {
                inputs.validate_v1(
                    &record.trajectories,
                    record.learning_rate_bits,
                    record.value_coefficient_bits,
                )?;
            }
            let state = decode_w_state_v1(&read_pinned_bytes(&record.state)?)?;
            ensure(
                w_state_sha256_v1(&state)? == record.state_sha256
                    && state.adam_step_v1() == record.adam_step,
                "wide checkpoint record and container differ",
            )?;
            (state, Some(record))
        }
    };
    ensure(
        policy.training_math_v1().is_platform_libm(),
        "a wide learner runs under the platform libm only",
    )?;
    policy.install_wide_model_v1(Arc::new(state.model_v1().clone()))?;
    Ok(LoadedWV1 {
        policy,
        state,
        checkpoint,
    })
}

/// The inference identity of a wide learner: the shell's ancestry and
/// feature identity, the wide net's model identity, state hash and Adam step.
pub(super) fn w_identity_v1(
    source: &ExpandedModelSourceV1,
    policy: &FrozenPlayPolicyV1,
    state: &NativePolicyValueTrainStateWideV1,
) -> Result<ExpandedInferenceIdentityV1, String> {
    let feature_identity = policy.feature_identity_v1();
    Ok(ExpandedInferenceIdentityV1 {
        schema: inference_schema_v1(policy.identity_v1()).into(),
        source_import: policy.identity_v1().clone(),
        checkpoint_sha256: source.checkpoint.as_ref().map(|pin| pin.sha256.clone()),
        model: policy.actual_model_identity_v1(),
        state_sha256: w_state_sha256_v1(state)?,
        adam_step: state.adam_step_v1(),
        feature_schema_version: feature_identity.feature_schema_version.into(),
        feature_registry_version: feature_identity.feature_registry_version.into(),
        features_source_sha256: feature_identity.features_source_sha256.into(),
        feature_descriptor_sha256: feature_identity.feature_descriptor_sha256.into(),
    })
}

/// `load_expanded_inference_any_math_v1` for a wide source: the policy plays
/// under the platform libm.
pub(super) fn load_inference_v1(
    source: &ExpandedModelSourceV1,
) -> Result<(FrozenPlayPolicyV1, ExpandedInferenceIdentityV1), String> {
    let loaded = load_w_v1(source, None)?;
    let identity = w_identity_v1(source, &loaded.policy, &loaded.state)?;
    Ok((loaded.policy, identity))
}

/// `load_expanded_update_inference_v1` for a wide source.
pub(super) fn load_update_inference_v1(
    source: &ExpandedModelSourceV1,
    inputs: &CheckpointInputExpectationV1<'_>,
) -> Result<ExpandedInferenceIdentityV1, String> {
    let loaded = load_w_v1(source, Some(inputs))?;
    w_identity_v1(source, &loaded.policy, &loaded.state)
}

/// The run driver's recovery check of a wide checkpoint's GAE settings: the
/// record must carry exactly the run's gamma, lambda and zero entropy. A
/// source without the wide learner passes unchanged.
pub(crate) fn validate_update_loss_v1(
    source: &ExpandedModelSourceV1,
    loss_selection: &ExpandedLossSelectionV1,
) -> Result<(), String> {
    if source.w_learner.is_none() {
        return Ok(());
    }
    let pin = source
        .checkpoint
        .as_ref()
        .ok_or("a wide update requires its checkpoint")?;
    let record: WLearnerCheckpointV1 =
        serde_json::from_slice(&read_pinned_bytes(pin)?).map_err(err)?;
    let ExpandedLossSelectionV1::GaeAdvantageValueV1 {
        gamma,
        lambda,
        entropy_coefficient,
    } = loss_selection
    else {
        return Err("a wide learner trains only the GAE loss (gae_advantage_value/v1)".into());
    };
    ensure(
        record.gamma_bits == gamma.to_bits()
            && record.gae_lambda_bits == lambda.to_bits()
            && entropy_coefficient.to_bits() == 0
            && record.entropy_coefficient_bits == 0,
        "wide checkpoint GAE settings differ from the run's",
    )
}

/// The learner of a collection: the policy, its behavior and state hash.
pub(super) fn collection_learner_v1(
    source: &ExpandedModelSourceV1,
    training_math: TrainingMathV1,
    collection_sampler: CollectionSamplerV1,
) -> Result<(FrozenPlayPolicyV1, ExpandedSeatBehaviorV1, String), String> {
    ensure(
        collection_sampler.is_legacy(),
        "a wide learner collects with the legacy sampler; the line (b) sampler is refused",
    )?;
    ensure(
        training_math.is_platform_libm(),
        "a wide learner runs under the platform libm only",
    )?;
    let loaded = load_w_v1(source, None)?;
    let identity = w_identity_v1(source, &loaded.policy, &loaded.state)?;
    let state_hash = identity.state_sha256.clone();
    Ok((
        loaded.policy,
        ExpandedSeatBehaviorV1 {
            source: source.clone(),
            identity,
        },
        state_hash,
    ))
}

/// Update options a wide learner refuses, as flags.
pub(super) struct WUnsupportedOptionsV1 {
    pub(super) line_b: bool,
    pub(super) e_supported: bool,
    pub(super) soft_q: bool,
    pub(super) search_targets: bool,
    pub(super) ex1: bool,
    pub(super) verified_replay: bool,
}

/// The update command's inputs.
pub(super) struct WUpdateInputsV1 {
    pub(super) source: ExpandedModelSourceV1,
    pub(super) trajectories: Vec<PinnedFileV1>,
    pub(super) learning_rate: f32,
    pub(super) value_coefficient: f32,
    pub(super) update_backend: ExpandedUpdateBackendV1,
    pub(super) backward_execution: UpdateBackwardExecutionV1,
    pub(super) loss_selection: ExpandedLossSelectionV1,
    pub(super) output_directory: std::path::PathBuf,
    pub(super) preparation_workers: Option<usize>,
    pub(super) max_prepared_tensor_mebibytes: usize,
    pub(super) unsupported: WUnsupportedOptionsV1,
    pub(super) training_math: TrainingMathV1,
    pub(super) record_format: CheckpointRecordFormatV1,
    pub(super) started: std::time::Instant,
}

/// The refusals of a wide update, before any input is read.
fn refuse_unsupported_v1(inputs: &WUpdateInputsV1) -> Result<(f32, f32), String> {
    let u = &inputs.unsupported;
    ensure(
        !u.line_b,
        "a wide learner refuses line (b) options (net-8's head-only mask and teacher)",
    )?;
    ensure(
        !u.e_supported && !u.soft_q && !u.search_targets,
        "a wide learner refuses E, SQ1 and TP1 search-target supervision",
    )?;
    ensure(!u.ex1, "a wide learner refuses EX1 and W5 search updates")?;
    ensure(!u.verified_replay, "a wide learner refuses verified replay")?;
    ensure(
        inputs.training_math.is_platform_libm(),
        "a wide learner runs under the platform libm only",
    )?;
    ensure(
        matches!(inputs.update_backend, ExpandedUpdateBackendV1::Cpu),
        "a wide learner updates on the CPU; the CUDA backend is refused",
    )?;
    ensure(
        inputs.backward_execution.is_sequential(),
        "a wide learner refuses fixed-partition backward",
    )?;
    ensure(
        !inputs.record_format.is_v2(),
        "a wide learner writes its own checkpoint record and container; record format v2 is \
         refused",
    )?;
    match inputs.loss_selection {
        ExpandedLossSelectionV1::GaeAdvantageValueV1 {
            gamma,
            lambda,
            entropy_coefficient,
        } => {
            ensure(
                entropy_coefficient.to_bits() == 0,
                "a wide learner's GAE loss has no entropy term",
            )?;
            Ok((gamma, lambda))
        }
        _ => Err(
            "a wide learner trains only the GAE loss (gae_advantage_value/v1); the terminal and \
             recovery losses are refused"
                .into(),
        ),
    }
}

/// The wide update (see the module documentation). The caller has checked
/// the batch size, the optimizer scalars and that the output does not exist.
pub(super) fn execute_w_update_v1(inputs: WUpdateInputsV1) -> Result<Value, String> {
    let (gamma, lambda) = refuse_unsupported_v1(&inputs)?;
    let WUpdateInputsV1 {
        source,
        trajectories,
        learning_rate,
        value_coefficient,
        output_directory,
        preparation_workers,
        max_prepared_tensor_mebibytes,
        training_math,
        started,
        ..
    } = inputs;
    let LoadedWV1 {
        policy,
        mut state,
        checkpoint,
    } = load_w_v1(&source, None)?;
    if let Some(record) = &checkpoint {
        ensure(
            record.gamma_bits == gamma.to_bits() && record.gae_lambda_bits == lambda.to_bits(),
            "wide continuation must keep the lineage's gamma and lambda",
        )?;
    }
    let identity = w_identity_v1(&source, &policy, &state)?;
    let before = identity.state_sha256.clone();
    let learner = ExpandedSeatBehaviorV1 {
        source: source.clone(),
        identity,
    };
    let mut hashes = BTreeSet::new();
    let mut total_bytes = 0u64;
    for pin in &trajectories {
        ensure(hashes.insert(&pin.sha256), "duplicate trajectory bytes")?;
        total_bytes = total_bytes
            .checked_add(fs::metadata(&pin.path).map_err(err)?.len())
            .ok_or("batch size overflow")?;
        ensure(
            total_bytes <= MAX_BATCH_BYTES,
            "trajectory batch exceeds 512 MiB CPU ingestion bound",
        )?;
    }
    let episodes = read_trajectory_batch_v1(&trajectories, preparation_workers.unwrap_or(1))?;
    let mut ids = BTreeSet::new();
    for episode in &episodes {
        validate_trajectory(episode)?;
        ensure(
            episode.search_played.is_none(),
            "a wide learner refuses search-played rows",
        )?;
        ensure(
            ids.insert(episode.episode.id.clone()),
            "duplicate episode id",
        )?;
        ensure(
            episode.behavior_state_sha256 == before,
            "stale trajectory: behavior optimizer/model state differs",
        )?;
        ensure(
            episode.source_import == *policy.identity_v1(),
            "trajectory source import differs",
        )?;
        ensure(
            episode.training_math == training_math,
            "trajectory training math differs from the command's",
        )?;
        ensure(
            episode.input_contract.is_none(),
            "a wide learner refuses the no-state-digest contract",
        )?;
    }
    let input_read_seconds = started.elapsed().as_secs_f64();
    let replay_started = std::time::Instant::now();
    // The replay recomputes every actor-visible row: the learner's with the
    // wide net, a net-8 opponent's with net-8.
    let (tensor_groups, preparation_telemetry) = if let Some(workers) = preparation_workers {
        let prepared = ordered_update_preparation::prepare_v1(
            &episodes,
            &policy,
            &learner,
            workers,
            max_prepared_tensor_mebibytes,
            training_math,
        )?;
        (prepared.groups, Some(prepared.telemetry))
    } else {
        let mut tensor_groups = Vec::new();
        let mut opponent_cache = OpponentCacheV1::default().with_training_math_v1(training_math);
        for (episode_ordinal, episode) in episodes.iter().enumerate() {
            let opponent = episode
                .episode
                .opponent
                .as_ref()
                .map(|s| opponent_cache.load(s))
                .transpose()?;
            validate_actual_behaviors_v1(
                episode,
                &learner,
                opponent.as_ref().map(|o| &o.behavior),
            )?;
            tensor_groups.extend(replay_learner_groups_v1(
                episode_ordinal,
                episode,
                &policy,
                opponent.as_ref().map(|o| &o.policy),
            )?);
        }
        (tensor_groups, None)
    };
    ensure(!tensor_groups.is_empty(), "no learner decisions")?;
    let mut values = Vec::with_capacity(tensor_groups.len());
    for (index, (_, _, rows)) in tensor_groups.iter().enumerate() {
        let (first, _) = rows
            .first()
            .ok_or_else(|| format!("gae group {index} has no substeps"))?;
        values.push(f32::from_bits(first.value));
    }
    let terminal_returns: Vec<i8> = tensor_groups.iter().map(|group| group.0).collect();
    let episode_ordinals: Vec<usize> = tensor_groups.iter().map(|group| group.1).collect();
    let gae = compute_gae_targets_of_values_v1(
        &values,
        &terminal_returns,
        &episode_ordinals,
        gamma,
        lambda,
    )?;
    let substeps: Vec<Vec<NativePolicySubstepV1<'_>>> = tensor_groups
        .iter()
        .map(|(_, _, group)| {
            group
                .iter()
                .map(|(row, t)| NativePolicySubstepV1 {
                    forward: NativePolicyForwardInputV1::Encoded(Box::new(
                        super::encoded_decision_view_generic_v1(t, FreshLineageGenerationV1::V4),
                    )),
                    selected_action_index: row.selected as usize,
                    expected_raw_action_logit_bits: &row.logits,
                    expected_value_bits: row.value,
                })
                .collect()
        })
        .collect();
    let groups: Vec<NativePolicyPhysicalDecisionV1<'_>> = substeps
        .iter()
        .zip(&tensor_groups)
        .map(
            |(substeps, (terminal_return, _, _))| NativePolicyPhysicalDecisionV1 {
                substeps,
                terminal_return: *terminal_return,
                baseline_bits: 0,
            },
        )
        .collect();
    let policy_substeps: usize = groups.iter().map(|group| group.substeps.len()).sum();
    let behavior_replay_seconds = replay_started.elapsed().as_secs_f64();
    eprintln!(
        "native wide update: {} episodes, {} learner physical decisions",
        episodes.len(),
        groups.len()
    );
    let learner_started = std::time::Instant::now();
    let learner_workers = preparation_workers.unwrap_or(1);
    let step = state
        .train_step_gae_feature_transfer_profiled_dims_v1(
            FreshLineageGenerationV1::V4,
            &groups,
            &gae.value_targets,
            &gae.advantages,
            value_coefficient,
            learning_rate,
            None,
            learner_workers,
            &mut NativeTrainingPhaseRecorderV1::disabled_v1(),
        )
        .map_err(err)?;
    let learner_update_seconds = learner_started.elapsed().as_secs_f64();
    drop(groups);
    drop(substeps);
    let after = w_state_sha256_v1(&state)?;
    ensure(
        step.after_state_sha256 == after,
        "wide step's after-state differs from the state's hash",
    )?;
    let checkpoint_started = std::time::Instant::now();
    fs::create_dir(&output_directory).map_err(err)?;
    let container = publish_json_bytes_v1(
        &output_directory,
        W_LEARNER_STATE_FILE_V1,
        &encode_w_state_v1(&state)?,
    )?;
    let feature_identity = policy.feature_identity_v1();
    let record = WLearnerCheckpointV1 {
        schema: W_LEARNER_CHECKPOINT_SCHEMA_V1.into(),
        architecture: W_MODEL_ARCHITECTURE_VERSION_V1.into(),
        initial_sha256: w_member_v1(&source)?.initial.sha256.clone(),
        source_import: policy.identity_v1().clone(),
        feature_contract_digest: feature_identity.feature_contract_digest.into(),
        feature_encoding_digest: feature_identity.feature_encoding_digest.into(),
        card_db_hash: format!("{KERNEL_CARDDB_HASH:016x}"),
        state: container.clone(),
        state_sha256: after.clone(),
        adam_step: state.adam_step_v1(),
        trajectories: trajectories.clone(),
        loss_identity: GAE_ADVANTAGE_VALUE_LOSS_IDENTITY_V1.into(),
        gamma_bits: gamma.to_bits(),
        gae_lambda_bits: lambda.to_bits(),
        entropy_coefficient_bits: 0.0f32.to_bits(),
        learning_rate_bits: learning_rate.to_bits(),
        value_coefficient_bits: value_coefficient.to_bits(),
    };
    let checkpoint_pin = publish_json(&output_directory, "checkpoint.json", &record)?;
    // Readback through the continuation loader, with this update's inputs.
    let resumed = ExpandedModelSourceV1 {
        checkpoint: Some(checkpoint_pin.clone()),
        ..source.clone()
    };
    let expectation = CheckpointInputExpectationV1 {
        trajectories: &trajectories,
        learning_rate_bits: learning_rate.to_bits(),
        value_coefficient_bits: value_coefficient.to_bits(),
    };
    let restored = load_w_v1(&resumed, Some(&expectation))?;
    ensure(
        w_state_sha256_v1(&restored.state)? == after
            && restored.state.snapshot_v1().map_err(err)? == state.snapshot_v1().map_err(err)?,
        "published wide checkpoint round trip differs",
    )?;
    drop(restored);
    let model = state.model_v1();
    let mut result = json!({"schema":"mtg-kernel-expanded-deck-update/v1", "complete":true,
        "source":source, "trajectories":trajectories, "checkpoint":checkpoint_pin,
        "before_state_sha256":before, "after_state_sha256":after,
        "adam_step":state.adam_step_v1(), "physical_decisions":step.loss.group_count,
        "policy_substeps":policy_substeps, "loss":step.loss.loss,
        "policy_sum":step.loss.policy_sum, "value_sum":step.loss.value_sum,
        "checkpoint_readback":true, "numerical_backend":"native-cpu-sequential",
        "loss_identity":GAE_ADVANTAGE_VALUE_LOSS_IDENTITY_V1,
        "claim":"engineering update only; no playing-strength or production-throughput claim",
        "w_learner":{"architecture":W_MODEL_ARCHITECTURE_VERSION_V1,
            "parameter_manifest_sha256":model.parameter_manifest_sha256_wide_v1(),
            "state":container}});
    training_math.record_v1(&mut result);
    result["advantage_statistics"] = json!(gae.statistics);
    if let Some(telemetry) = preparation_telemetry {
        result["update_preparation"] = serde_json::to_value(telemetry).map_err(err)?;
    }
    result["input_read_seconds"] = json!(input_read_seconds);
    result["behavior_replay_seconds"] = json!(behavior_replay_seconds);
    result["learner_update_seconds"] = json!(learner_update_seconds);
    if learner_workers > 1 {
        result["w_learner_workers_requested"] = json!(learner_workers);
    }
    result["checkpoint_io_seconds"] = json!(checkpoint_started.elapsed().as_secs_f64());
    result["update_elapsed_seconds"] = json!(started.elapsed().as_secs_f64());
    publish_json(&output_directory, "update.json", &result)?;
    Ok(result)
}
