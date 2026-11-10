//! W5's native S learner in RT1 (collab LANES/lead/W5-DESIGN.md v1.3,
//! sections 2 and 6.2, item E3): an `ExpandedModelSourceV1` with
//! `s_learner` collects, updates and resumes with S (`s_transformer_v1`) in
//! place of net-8.
//!
//! **Source.** The play import and feature transfer load the plain FlatV4
//! shell (a fresh-initialization import, as every FlatV4 lineage's): its encoder,
//! tensorizer, samplers and RNG streams are used unchanged, and S is
//! installed on it (`install_s_model_v1`), so every forward of the learner
//! is S's. `s_learner.initial` pins S's starting train state (S0 for XS).
//! Without a checkpoint the learner starts from that state's parameters with
//! a fresh optimizer (zero moments, step 0: W5 ruling 6, XS's switch to
//! native RL). With one, `checkpoint` pins an S checkpoint record
//! ([`SLearnerCheckpointV1`]) that names the S container
//! (`s_transformer_v1::encode_checkpoint_v1`) and binds the update's inputs.
//!
//! **Opponents.** Net-8 opponents load and play exactly as in a net-8 run,
//! under the command's training math (the cross-family path: one FlatV4
//! feature identity, one math, no input contract). An S source as an
//! opponent (the current or initial learner) loads through
//! [`load_inference_v1`].
//!
//! **Update.** The ordinary CPU GAE update (`gae_advantage_value/v1`,
//! entropy 0) with full Adam: the same trajectory checks and replay as
//! net-8's (serial or prepared), the same GAE targets and advantage
//! normalization, then one S step. Prepared ordinary GAE updates also use
//! the preparation worker count for S's forward and backward passes.
//! The receipt has the net-8 update's fields, so the run driver validates and
//! chains it unchanged.
//!
//! **EX1 and W5** (`Ex1Update`, `ex1_update_v1`): the collection hook runs
//! with S as the learner (S's own search prior and SELF rollouts), and the
//! update adds net-8's terms on S
//! (`STrainStateV1::train_step_gae_auxiliary_v1`): the hook's rows as
//! forward-KL auxiliary rows, `c / 128` each, authenticated against S's
//! training forward; under W5 the search-played substeps' policy gradient
//! masked and the policy term normalized over the unmasked groups; the cap;
//! step telemetry (every EX1 update, one W5 update in 16, and any update
//! with no rows). The checkpoint record carries the EX1 continuation (and
//! W5's next global update), and the receipt and `ex1-update.json` have
//! net-8's fields, so the run driver validates an S chain as a net-8 one.
//!
//! **Refused** (explicit errors): line (b), E supervision, SQ1, TP1 search
//! targets, the CUDA
//! backend, fixed-partition backward, the terminal and recovery losses,
//! checkpoint record format v2, FlatV7 and FlatV8, the no-state-digest
//! contract, registry-transfer and registry-evolution shells, the line (b)
//! collection sampler, and every net-8-only command (they refuse an S
//! source in the net-8 loader).

use super::ex1_update_v1::{self, Ex1UpdateOptionsV1};
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
use crate::native_flat_tensorizer_v2::NativeFlatDecisionTensorV2;
use crate::native_flat_tensorizer_v4::{FEATURE_CONTRACT_DIGEST_V4, FEATURE_ENCODING_DIGEST_V4};
use crate::s_transformer_v1::{
    decode_checkpoint_v1, encode_checkpoint_v1, SGroupV1, SInputV1, SModelV1, SStepV1, SSubstepV1,
    STapeV1, STrainStateV1, SWorkV1, S_ARCHITECTURE_V1,
};
use crate::sideboard_play_policy_v1::{
    FrozenPlayPolicyV1, PlayPolicyOriginV1, FRESH_FEATURE_IDENTITY_V4,
};
use crate::training_math_v1::TrainingMathV1;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

#[cfg(test)]
mod verified_replay_v1;

/// `SLearnerSourceV1::schema`.
pub const S_LEARNER_SOURCE_SCHEMA_V1: &str = "w5-s-learner/v1";
/// The S checkpoint record's schema.
pub const S_LEARNER_CHECKPOINT_SCHEMA_V1: &str = "w5-s-learner-checkpoint/v1";
/// The S container file each update publishes beside its record.
pub const S_LEARNER_STATE_FILE_V1: &str = "s-train-state.bin";
pub(super) const S_PARALLEL_UPDATE_BACKEND_V1: &str = "native-cpu-exact-parallel-s";
/// The net-8 loader's refusal of an S source.
pub(super) const S_LEARNER_NET8_REFUSAL_V1: &str =
    "an S learner source loads only through the S learner paths (Collect, CollectParallel, \
     Update, UpdatePrepared and inference); this command is net-8's";

pub(super) fn record_parallel_update_execution_v1(document: &mut Value, workers: usize) {
    document["numerical_backend"] = json!(S_PARALLEL_UPDATE_BACKEND_V1);
    document["s_learner_workers_requested"] = json!(workers);
}

/// The `s_learner` member of a source.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SLearnerSourceV1 {
    pub schema: String,
    /// S's starting train state, an `s_transformer_v1` container. Only its
    /// parameters are used: a source without a checkpoint starts a fresh
    /// optimizer.
    pub initial: PinnedFileV1,
}

/// The record `checkpoint` pins in an S source: the S container and the
/// update that produced it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SLearnerCheckpointV1 {
    pub schema: String,
    pub architecture: String,
    /// `s_learner.initial`'s sha256: the lineage's start.
    pub initial_sha256: String,
    /// The shell's import identity.
    pub source_import: PlayPolicyOriginV1,
    pub feature_contract_digest: String,
    pub feature_encoding_digest: String,
    pub card_db_hash: String,
    /// The S container.
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
    /// The command's math, which the net-8 opponents played under; omitted
    /// for the platform's libm.
    #[serde(default, skip_serializing_if = "TrainingMathV1::is_platform_libm")]
    pub training_math: TrainingMathV1,
    /// An EX1 or W5 update's continuation, collection and receipt (and W5's
    /// next global update), as net-8's checkpoint records them; omitted for
    /// an ordinary update.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ex1: Option<Value>,
}

/// A loaded S learner: the shell with S installed, S's train state, and the
/// checkpoint record it resumed from.
pub(super) struct LoadedSV1 {
    pub(super) policy: FrozenPlayPolicyV1,
    pub(super) state: STrainStateV1,
    pub(super) checkpoint: Option<SLearnerCheckpointV1>,
}

fn s_member_v1(source: &ExpandedModelSourceV1) -> Result<&SLearnerSourceV1, String> {
    let member = source
        .s_learner
        .as_deref()
        .ok_or("the source has no S learner")?;
    ensure(
        member.schema == S_LEARNER_SOURCE_SCHEMA_V1,
        "unknown S learner source schema",
    )?;
    Ok(member)
}

/// Loads `source`'s shell, S train state and checkpoint record. With
/// `inputs`, the record must bind exactly those trajectories and scalars.
pub(super) fn load_s_v1(
    source: &ExpandedModelSourceV1,
    inputs: Option<&CheckpointInputExpectationV1<'_>>,
) -> Result<LoadedSV1, String> {
    let member = s_member_v1(source)?;
    ensure(
        source.feature_transfer.expected_feature_contract_digest == FEATURE_CONTRACT_DIGEST_V4
            && source.feature_transfer.expected_feature_encoding_digest
                == FEATURE_ENCODING_DIGEST_V4,
        "an S learner runs on the plain FlatV4 contract; FlatV7 and FlatV8 are refused",
    )?;
    let bytes = read_pinned_bytes(&source.play_import)?;
    let probe: Value = serde_json::from_slice(&bytes).map_err(err)?;
    let schema = probe.get("schema").and_then(Value::as_str);
    ensure(
        schema == Some(super::fresh_initialization_source::SOURCE_SCHEMA),
        "an S learner's shell is a fresh-initialization (FlatV4) import; ordinary V3, \
         transfer, registry-evolution, BO3 and no-state-digest imports are refused",
    )?;
    let mut policy = super::load_ordinary_policy_v1(source, &bytes)?;
    ensure(
        policy.feature_identity_v1() == FRESH_FEATURE_IDENTITY_V4
            && policy.input_contract_v1().is_none(),
        "an S learner's shell must load with the plain FlatV4 identity",
    )?;
    let initial = decode_checkpoint_v1(&read_pinned_bytes(&member.initial)?)?;
    let (state, checkpoint) = match &source.checkpoint {
        None => {
            ensure(inputs.is_none(), "update requires checkpoint")?;
            (
                STrainStateV1::fresh_optimizer_v1(initial.model_v1().clone()),
                None,
            )
        }
        Some(pin) => {
            let record: SLearnerCheckpointV1 =
                serde_json::from_slice(&read_pinned_bytes(pin)?).map_err(err)?;
            let identity = policy.feature_identity_v1();
            ensure(
                record.schema == S_LEARNER_CHECKPOINT_SCHEMA_V1
                    && record.architecture == S_ARCHITECTURE_V1
                    && record.initial_sha256 == member.initial.sha256
                    && record.source_import == *policy.identity_v1()
                    && record.feature_contract_digest == identity.feature_contract_digest
                    && record.feature_encoding_digest == identity.feature_encoding_digest
                    && record.card_db_hash == format!("{KERNEL_CARDDB_HASH:016x}")
                    && record.loss_identity == GAE_ADVANTAGE_VALUE_LOSS_IDENTITY_V1,
                "S checkpoint record does not belong to this source",
            )?;
            if let Some(inputs) = inputs {
                inputs.validate_v1(
                    &record.trajectories,
                    record.learning_rate_bits,
                    record.value_coefficient_bits,
                )?;
            }
            let state = decode_checkpoint_v1(&read_pinned_bytes(&record.state)?)?;
            ensure(
                state.state_sha256_v1() == record.state_sha256
                    && state.adam_step_v1() == record.adam_step,
                "S checkpoint record and container differ",
            )?;
            ensure(
                state.model_v1().config_v1() == initial.model_v1().config_v1(),
                "S checkpoint config differs from the initial state's",
            )?;
            policy.set_training_math_v1(record.training_math);
            (state, Some(record))
        }
    };
    policy.install_s_model_v1(Arc::new(state.model_v1().clone()))?;
    Ok(LoadedSV1 {
        policy,
        state,
        checkpoint,
    })
}

/// The inference identity of an S learner: the shell's ancestry and feature
/// identity, S's model identity, state hash and Adam step.
pub(super) fn s_identity_v1(
    source: &ExpandedModelSourceV1,
    policy: &FrozenPlayPolicyV1,
    state: &STrainStateV1,
) -> ExpandedInferenceIdentityV1 {
    let feature_identity = policy.feature_identity_v1();
    ExpandedInferenceIdentityV1 {
        schema: inference_schema_v1(policy.identity_v1()).into(),
        source_import: policy.identity_v1().clone(),
        checkpoint_sha256: source.checkpoint.as_ref().map(|pin| pin.sha256.clone()),
        model: policy.actual_model_identity_v1(),
        state_sha256: state.state_sha256_v1(),
        adam_step: state.adam_step_v1(),
        feature_schema_version: feature_identity.feature_schema_version.into(),
        feature_registry_version: feature_identity.feature_registry_version.into(),
        features_source_sha256: feature_identity.features_source_sha256.into(),
        feature_descriptor_sha256: feature_identity.feature_descriptor_sha256.into(),
    }
}

/// `load_expanded_inference_any_math_v1` for an S source: the policy plays
/// under its checkpoint's math.
pub(super) fn load_inference_v1(
    source: &ExpandedModelSourceV1,
) -> Result<(FrozenPlayPolicyV1, ExpandedInferenceIdentityV1), String> {
    let loaded = load_s_v1(source, None)?;
    let identity = s_identity_v1(source, &loaded.policy, &loaded.state);
    Ok((loaded.policy, identity))
}

/// `load_expanded_update_inference_v1` for an S source.
pub(super) fn load_update_inference_v1(
    source: &ExpandedModelSourceV1,
    inputs: &CheckpointInputExpectationV1<'_>,
) -> Result<ExpandedInferenceIdentityV1, String> {
    let loaded = load_s_v1(source, Some(inputs))?;
    Ok(s_identity_v1(source, &loaded.policy, &loaded.state))
}

/// The run driver's recovery check of an S checkpoint's GAE settings: the
/// record must carry exactly the run's gamma, lambda and zero entropy. A
/// net-8 source passes unchanged.
pub(crate) fn validate_update_loss_v1(
    source: &ExpandedModelSourceV1,
    loss_selection: &ExpandedLossSelectionV1,
) -> Result<(), String> {
    if source.s_learner.is_none() {
        return Ok(());
    }
    let pin = source
        .checkpoint
        .as_ref()
        .ok_or("an S update requires its checkpoint")?;
    let record: SLearnerCheckpointV1 =
        serde_json::from_slice(&read_pinned_bytes(pin)?).map_err(err)?;
    let ExpandedLossSelectionV1::GaeAdvantageValueV1 {
        gamma,
        lambda,
        entropy_coefficient,
    } = loss_selection
    else {
        return Err("an S learner trains only the GAE loss (gae_advantage_value/v1)".into());
    };
    ensure(
        record.gamma_bits == gamma.to_bits()
            && record.gae_lambda_bits == lambda.to_bits()
            && entropy_coefficient.to_bits() == 0
            && record.entropy_coefficient_bits == 0,
        "S checkpoint GAE settings differ from the run's",
    )
}

/// Binds the command's math: a checkpoint must record it.
fn bind_math_v1(loaded: &mut LoadedSV1, math: TrainingMathV1) -> Result<(), String> {
    if let Some(record) = &loaded.checkpoint {
        ensure(
            record.training_math == math,
            "checkpoint training math differs from the command's",
        )?;
    }
    loaded.policy.set_training_math_v1(math);
    Ok(())
}

/// The learner of a collection: the policy, its behavior and state hash.
pub(super) fn collection_learner_v1(
    source: &ExpandedModelSourceV1,
    training_math: TrainingMathV1,
    collection_sampler: CollectionSamplerV1,
) -> Result<(FrozenPlayPolicyV1, ExpandedSeatBehaviorV1, String), String> {
    ensure(
        collection_sampler.is_legacy(),
        "an S learner collects with the legacy sampler; the line (b) sampler is refused",
    )?;
    let mut loaded = load_s_v1(source, None)?;
    bind_math_v1(&mut loaded, training_math)?;
    let identity = s_identity_v1(source, &loaded.policy, &loaded.state);
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

/// Update options an S learner refuses, as flags.
pub(super) struct SUnsupportedOptionsV1 {
    pub(super) line_b: bool,
    pub(super) e_supported: bool,
    pub(super) soft_q: bool,
    pub(super) search_targets: bool,
}

/// The update command's inputs.
pub(super) struct SUpdateInputsV1 {
    pub(super) source: ExpandedModelSourceV1,
    pub(super) trajectories: Vec<PinnedFileV1>,
    pub(super) learning_rate: f32,
    pub(super) value_coefficient: f32,
    pub(super) update_backend: ExpandedUpdateBackendV1,
    pub(super) backward_execution: UpdateBackwardExecutionV1,
    pub(super) loss_selection: ExpandedLossSelectionV1,
    pub(super) output_directory: PathBuf,
    pub(super) preparation_workers: Option<usize>,
    pub(super) max_prepared_tensor_mebibytes: usize,
    #[cfg(test)]
    pub(super) verified_tape_cap: Option<usize>,
    pub(super) unsupported: SUnsupportedOptionsV1,
    /// EX1 or W5 (`Ex1Update`); the caller validated the options and their
    /// combination.
    pub(super) ex1: Option<Ex1UpdateOptionsV1>,
    pub(super) training_math: TrainingMathV1,
    pub(super) record_format: CheckpointRecordFormatV1,
    pub(super) started: std::time::Instant,
}

struct LearnedSV1 {
    step: SStepV1,
    gae_statistics: super::bootstrapped_advantage_v1::AdvantageStatisticsV1,
    policy_substeps: usize,
    learner_workers: usize,
    preparation_telemetry: Option<ordered_update_preparation::PreparationTelemetryV1>,
    teacher_receipt: Option<Value>,
    ex1_exposure: Option<Value>,
    teacher_seconds: f64,
    behavior_replay_seconds: f64,
    learner_update_seconds: f64,
}

/// The refusals of an S update, before any input is read.
fn refuse_unsupported_v1(inputs: &SUpdateInputsV1) -> Result<(f32, f32), String> {
    let u = &inputs.unsupported;
    ensure(
        !u.line_b,
        "an S learner refuses line (b) options (net-8's head-only mask and teacher)",
    )?;
    ensure(
        !u.e_supported && !u.soft_q && !u.search_targets,
        "an S learner refuses E, SQ1 and TP1 search-target supervision",
    )?;
    ensure(
        matches!(inputs.update_backend, ExpandedUpdateBackendV1::Cpu),
        "an S learner updates on the CPU; the CUDA backend is refused",
    )?;
    ensure(
        inputs.backward_execution.is_sequential(),
        "an S learner refuses fixed-partition backward",
    )?;
    ensure(
        !inputs.record_format.is_v2(),
        "an S learner writes its own checkpoint record and container; record format v2 is \
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
                "an S learner's GAE loss has no entropy term",
            )?;
            Ok((gamma, lambda))
        }
        _ => Err(
            "an S learner trains only the GAE loss (gae_advantage_value/v1); the terminal and \
             recovery losses are refused"
                .into(),
        ),
    }
}

/// The S update (see the module documentation). The caller has checked the
/// batch size, the optimizer scalars and that the output does not exist.
pub(super) fn execute_s_update_v1(inputs: SUpdateInputsV1) -> Result<Value, String> {
    let (gamma, lambda) = refuse_unsupported_v1(&inputs)?;
    let SUpdateInputsV1 {
        source,
        trajectories,
        learning_rate,
        value_coefficient,
        output_directory,
        preparation_workers,
        max_prepared_tensor_mebibytes,
        #[cfg(test)]
        verified_tape_cap,
        ex1,
        training_math,
        started,
        ..
    } = inputs;
    let mut loaded = load_s_v1(&source, None)?;
    bind_math_v1(&mut loaded, training_math)?;
    if let Some(record) = &loaded.checkpoint {
        ensure(
            record.gamma_bits == gamma.to_bits() && record.gae_lambda_bits == lambda.to_bits(),
            "S continuation must keep the lineage's gamma and lambda",
        )?;
        // An EX1 or W5 lineage continues only with its own identity (and
        // W5's next global update); an ordinary lineage admits EX1.
        ex1_update_v1::check_continuation_v1(record.ex1.as_ref(), ex1.as_ref())?;
    }
    let LoadedSV1 {
        policy, mut state, ..
    } = loaded;
    let identity = s_identity_v1(&source, &policy, &state);
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
        // Rows the search played are authenticated and masked only by a
        // W5 update.
        ensure(
            episode.search_played.is_none() || ex1.as_ref().is_some_and(|o| o.w5.is_some()),
            "search-played rows need a w5 update",
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
            "an S learner refuses the no-state-digest contract",
        )?;
    }
    let input_read_seconds = started.elapsed().as_secs_f64();
    #[cfg(test)]
    let (candidate, verified_replay) = match verified_tape_cap {
        Some(tape_cap) => {
            use crate::s_transformer_v1::verified_row_v1::ReplayErrorV1;

            ensure(
                preparation_workers.is_none() && ex1.is_none(),
                "verified replay requires ordinary serial S",
            )?;
            let started = std::time::Instant::now();
            let candidate = verified_replay_v1::gradient_v1(
                &episodes,
                &policy,
                &learner,
                state.model_v1(),
                verified_replay_v1::OptionsV1 {
                    gamma,
                    lambda,
                    coefficient: value_coefficient,
                    tape_cap,
                    training_math,
                },
            );
            let replay_and_gradient_seconds = started.elapsed().as_secs_f64();
            match candidate {
                Ok(candidate) => {
                    let learner_started = std::time::Instant::now();
                    state.adam_step_with_gradient_v1(&candidate.gradient, learning_rate)?;
                    let step = SStepV1 {
                        loss: candidate.loss,
                        adam_step: state.adam_step_v1(),
                        after_state_sha256: state.state_sha256_v1(),
                    };
                    (
                        Some(LearnedSV1 {
                            step,
                            gae_statistics: candidate.gae.statistics,
                            policy_substeps: candidate.policy_substeps,
                            learner_workers: 1,
                            preparation_telemetry: None,
                            teacher_receipt: None,
                            ex1_exposure: None,
                            teacher_seconds: 0.0,
                            behavior_replay_seconds: replay_and_gradient_seconds,
                            learner_update_seconds: learner_started.elapsed().as_secs_f64(),
                        }),
                        Some(json!({"used":true, "tape_cap_bytes":tape_cap,
                            "peak_tape_bytes":candidate.peak_tape_bytes,
                            "replay_and_gradient_seconds":replay_and_gradient_seconds})),
                    )
                }
                Err(ReplayErrorV1::Replay(error)) => return Err(error),
                Err(error) => {
                    let reason = match error {
                        ReplayErrorV1::Capacity => "tape_capacity",
                        ReplayErrorV1::CallerMathState => "caller_math_state",
                        ReplayErrorV1::UnsupportedGeneration => "unsupported_generation",
                        ReplayErrorV1::Replay(_) => unreachable!(),
                    };
                    (
                        None,
                        Some(
                            json!({"used":false, "fallback":reason, "tape_cap_bytes":tape_cap,
                            "replay_and_gradient_seconds":replay_and_gradient_seconds}),
                        ),
                    )
                }
            }
        }
        None => (None, None),
    };
    #[cfg(not(test))]
    let candidate = None;
    let run_reference = |state: &mut STrainStateV1| -> Result<LearnedSV1, String> {
        let replay_started = std::time::Instant::now();
        // The replay recomputes every actor-visible row: the learner's with S,
        // a net-8 opponent's with net-8 under the command's math.
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
            let mut opponent_cache =
                OpponentCacheV1::default().with_training_math_v1(training_math);
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
        // EX1: the hook's rows, authenticated against S's training forward at
        // the update-start parameters.
        let teacher_started = std::time::Instant::now();
        let ex1_teacher = ex1
            .as_ref()
            .map(|options| {
                let model = state.model_v1();
                let mut tape = STapeV1::default();
                ex1_update_v1::ex1_teach_with_v1(options, &trajectories, &episodes, &mut |tensor| {
                    model
                        .forward_v1(SInputV1::of_tensor_v1(tensor), &mut tape)
                        .map(|output| output.logits)
                })
            })
            .transpose()?;
        let policy_mask = ex1_teacher
            .as_ref()
            .map(|(receipt, _)| ex1_update_v1::w5_policy_mask_v1(receipt, &tensor_groups))
            .transpose()?
            .unwrap_or_default();
        let teacher_seconds = teacher_started.elapsed().as_secs_f64();
        let terminal_returns: Vec<i8> = tensor_groups.iter().map(|group| group.0).collect();
        let episode_ordinals: Vec<usize> = tensor_groups.iter().map(|group| group.1).collect();
        let gae = compute_gae_targets_of_values_v1(
            &values,
            &terminal_returns,
            &episode_ordinals,
            gamma,
            lambda,
        )?;
        let groups: Vec<SGroupV1<'_>> = tensor_groups
            .iter()
            .zip(gae.advantages.iter().zip(&gae.value_targets))
            .map(|((_, _, rows), (advantage, value_target))| SGroupV1 {
                substeps: rows
                    .iter()
                    .map(|(row, tensor)| SSubstepV1 {
                        input: SInputV1::of_tensor_v1(tensor),
                        selected: row.selected as usize,
                    })
                    .collect(),
                advantage: *advantage,
                value_target: *value_target,
            })
            .collect();
        let policy_substeps: usize = groups.iter().map(|group| group.substeps.len()).sum();
        let behavior_replay_seconds = replay_started.elapsed().as_secs_f64();
        eprintln!(
            "native S update: {} episodes, {} learner physical decisions",
            episodes.len(),
            groups.len()
        );
        let learner_started = std::time::Instant::now();
        let learner_workers = if ex1.is_none() {
            preparation_workers.unwrap_or(1)
        } else {
            1
        };
        let (step, ex1_exposure) = match (&ex1_teacher, &ex1) {
            (Some((_, rows)), Some(options)) => {
                let masked_rows = policy_mask.len();
                let input = ex1_update_v1::ex1_s_auxiliary_input_v1(rows, options, policy_mask);
                let (step, auxiliary) = state.train_step_gae_auxiliary_v1(
                    &groups,
                    value_coefficient,
                    learning_rate,
                    &input,
                    &mut SWorkV1::default(),
                )?;
                let movement =
                    s_movement_v1(rows, state.model_v1(), auxiliary.ordinary_only.as_ref())?;
                let mut exposure = json!({"rows":rows.len(),
                    "denominator":crate::ex1_v1::EX1_DENOMINATOR_V1,
                    "coefficient":options.coefficient,
                    "kl_before":auxiliary.divergences_before,
                    "auxiliary_loss":auxiliary.auxiliary_loss,
                    "total_loss":f64::from(step.loss.loss) + auxiliary.auxiliary_loss,
                    "gradient_components":auxiliary.gradient_components,
                    "auxiliary_head_l2":auxiliary.auxiliary_head_l2,
                    "ordinary_head_l2":auxiliary.ordinary_head_l2,
                    "auxiliary_bias_residual":auxiliary.auxiliary_bias_residual,
                    "cap":auxiliary.cap,"adam_steps":auxiliary.adam_steps,
                    "movement":movement,"thresholds":ex1_update_v1::ex1_thresholds_v1(),
                    "learner":S_ARCHITECTURE_V1});
                if options.w5.is_some() {
                    exposure["pg_masked_rows"] = json!(masked_rows);
                    exposure["policy_groups"] = json!(auxiliary.policy_groups);
                    exposure["step_telemetry"] = json!(input.step_telemetry);
                }
                (step, Some(exposure))
            }
            (None, None) => (
                state.train_step_gae_with_workers_v1(
                    &groups,
                    value_coefficient,
                    learning_rate,
                    &mut SWorkV1::default(),
                    learner_workers,
                )?,
                None,
            ),
            _ => return Err("an EX1 update lacks its teacher".into()),
        };
        let learner_update_seconds = learner_started.elapsed().as_secs_f64();
        drop(groups);
        Ok(LearnedSV1 {
            step,
            gae_statistics: gae.statistics,
            policy_substeps,
            learner_workers,
            preparation_telemetry,
            teacher_receipt: ex1_teacher.map(|(receipt, _)| receipt),
            ex1_exposure,
            teacher_seconds,
            behavior_replay_seconds,
            learner_update_seconds,
        })
    };
    let LearnedSV1 {
        step,
        gae_statistics,
        policy_substeps,
        learner_workers,
        preparation_telemetry,
        teacher_receipt,
        ex1_exposure,
        teacher_seconds,
        behavior_replay_seconds,
        learner_update_seconds,
    } = match candidate {
        Some(learned) => learned,
        None => run_reference(&mut state)?,
    };
    let checkpoint_started = std::time::Instant::now();
    fs::create_dir(&output_directory).map_err(err)?;
    let container = publish_json_bytes_v1(
        &output_directory,
        S_LEARNER_STATE_FILE_V1,
        &encode_checkpoint_v1(&state),
    )?;
    let ex1_pin = match (&teacher_receipt, &ex1_exposure) {
        (Some(receipt), Some(exposure)) => {
            let mut document = receipt.clone();
            document["schema"] = json!("ex1-update/v1");
            document["gradient_exposure"] = exposure.clone();
            Some(publish_json(
                &output_directory,
                "ex1-update.json",
                &document,
            )?)
        }
        _ => None,
    };
    let feature_identity = policy.feature_identity_v1();
    let record = SLearnerCheckpointV1 {
        schema: S_LEARNER_CHECKPOINT_SCHEMA_V1.into(),
        architecture: S_ARCHITECTURE_V1.into(),
        initial_sha256: s_member_v1(&source)?.initial.sha256.clone(),
        source_import: policy.identity_v1().clone(),
        feature_contract_digest: feature_identity.feature_contract_digest.into(),
        feature_encoding_digest: feature_identity.feature_encoding_digest.into(),
        card_db_hash: format!("{KERNEL_CARDDB_HASH:016x}"),
        state: container.clone(),
        state_sha256: step.after_state_sha256.clone(),
        adam_step: step.adam_step,
        trajectories: trajectories.clone(),
        loss_identity: GAE_ADVANTAGE_VALUE_LOSS_IDENTITY_V1.into(),
        gamma_bits: gamma.to_bits(),
        gae_lambda_bits: lambda.to_bits(),
        entropy_coefficient_bits: 0.0f32.to_bits(),
        learning_rate_bits: learning_rate.to_bits(),
        value_coefficient_bits: value_coefficient.to_bits(),
        training_math,
        ex1: ex1.as_ref().map(|options| {
            let mut saved = json!({
                "continuation":ex1_update_v1::continuation_identity(options),
                "collection":options.collection,"receipt":ex1_pin});
            if options.w5.is_some() {
                saved["w5_next_update"] = json!(options.update + 1);
            }
            saved
        }),
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
    let restored = load_s_v1(&resumed, Some(&expectation))?;
    ensure(
        restored.state == state
            && restored.state.state_sha256_v1() == step.after_state_sha256
            && restored.policy.training_math_v1() == training_math,
        "published S checkpoint round trip differs",
    )?;
    drop(restored);
    let model = state.model_v1();
    let mut result = json!({"schema":"mtg-kernel-expanded-deck-update/v1", "complete":true,
        "source":source, "trajectories":trajectories, "checkpoint":checkpoint_pin,
        "before_state_sha256":before, "after_state_sha256":step.after_state_sha256,
        "adam_step":step.adam_step, "physical_decisions":step.loss.group_count,
        "policy_substeps":policy_substeps, "loss":step.loss.loss,
        "policy_sum":step.loss.policy_sum, "value_sum":step.loss.value_sum,
        "checkpoint_readback":true, "numerical_backend":"native-cpu-sequential",
        "loss_identity":GAE_ADVANTAGE_VALUE_LOSS_IDENTITY_V1,
        "claim":"engineering update only; no playing-strength or production-throughput claim",
        "s_learner":{"architecture":S_ARCHITECTURE_V1,"config":model.config_v1(),
            "parameter_sha256":model.parameter_sha256_v1(),"state":container,
            "policy_term":step.loss.policy_term,"value_term":step.loss.value_term}});
    training_math.record_v1(&mut result);
    result["advantage_statistics"] = json!(gae_statistics);
    if let Some(telemetry) = preparation_telemetry {
        result["update_preparation"] = serde_json::to_value(telemetry).map_err(err)?;
    }
    if let (Some(receipt), Some(options), Some(exposure)) = (&teacher_receipt, &ex1, &ex1_exposure)
    {
        result["ex1"] = json!({"method":ex1_update_v1::EX1_METHOD_V1,"receipt":ex1_pin,
            "collection":options.collection,"rows":receipt["rows"],
            "rows_dropped":receipt["rows_dropped"],"searches":receipt["searches"],
            "searches_failed":receipt["searches_failed"],"cap":exposure["cap"],
            "adam_steps":exposure["adam_steps"],"teacher_seconds":teacher_seconds});
    }
    result["input_read_seconds"] = json!(input_read_seconds);
    result["behavior_replay_seconds"] = json!(behavior_replay_seconds);
    result["learner_update_seconds"] = json!(learner_update_seconds);
    if learner_workers > 1 {
        record_parallel_update_execution_v1(&mut result, learner_workers);
    }
    #[cfg(test)]
    if let Some(verified) = verified_replay {
        result["verified_replay"] = verified;
    }
    result["checkpoint_io_seconds"] = json!(checkpoint_started.elapsed().as_secs_f64());
    result["update_elapsed_seconds"] = json!(started.elapsed().as_secs_f64());
    publish_json(&output_directory, "update.json", &result)?;
    Ok(result)
}

/// EX1's movement rows on S: each row's log p after the combined step and
/// after the ordinary-only step (when telemetry ran).
fn s_movement_v1(
    rows: &[ex1_update_v1::Ex1TaughtRowV1],
    after: &SModelV1,
    alone: Option<&SModelV1>,
) -> Result<Value, String> {
    let forward = |model: &SModelV1, tensor: &NativeFlatDecisionTensorV2| {
        model
            .forward_v1(SInputV1::of_tensor_v1(tensor), &mut STapeV1::default())
            .map(|output| output.logits)
    };
    let mut alone =
        alone.map(|model| move |tensor: &NativeFlatDecisionTensorV2| forward(model, tensor));
    ex1_update_v1::ex1_movement_with_v1(
        rows,
        &mut |tensor| forward(after, tensor),
        alone.as_mut().map(|f| f as ex1_update_v1::Ex1ForwardV1<'_>),
    )
}

#[cfg(test)]
mod tests;
