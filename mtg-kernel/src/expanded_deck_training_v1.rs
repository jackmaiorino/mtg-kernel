//! Explicit V6/V3 expanded-deck rollouts, immutable trajectories and native
//! CPU collection and explicitly selected updates. This successor starts from an inference export with fresh Adam
//! or resumes its own checkpoint. It never reads or writes a legacy Store.

use crate::card_def::KERNEL_CARDDB_HASH;
use crate::durable_publication_v1::{
    capture_existing_publication_parent_v1, publish_new_file_v1, DurableFileExpectationV1,
};
use crate::fast_sampler::{
    WideCategoricalScratchV1, FAST_CATEGORICAL_MAX_ACTIONS, WIDE_CATEGORICAL_SAMPLER_VERSION_V1,
};
use crate::ids::PlayerId;
use crate::native_flat_tensorizer_v2::NativeFlatDecisionTensorV2;
use crate::native_flat_tensorizer_v3::{
    encoded_decision_view_v3, NativeFlatDecisionTensorV3, FEATURES_SOURCE_SHA256_V3,
    FEATURE_CONTRACT_DIGEST_V3, FEATURE_DESCRIPTOR_SHA256_V3, FEATURE_ENCODING_DIGEST_V3,
    FEATURE_REGISTRY_VERSION_V3, FEATURE_SCHEMA_VERSION_V3,
};
use crate::native_policy_train_step_v1::{
    NativePolicyForwardInputV1, NativePolicyPhysicalDecisionV1, NativePolicySubstepV1,
    NativePolicyValueTrainSnapshotV1, NativePolicyValueTrainStateV1,
};
use crate::native_policy_value_net_v1::{
    NativeNamedParameterV1, NativePolicyValueModelConfigV1, NativePolicyValueNetV1,
};
use crate::paired_bo1_harness_v1::paired_policy_seeds_v1;
use crate::rl::{
    terminal_tuple_is_valid_v1, PlayerSeatV1, TerminalClassificationV1, TerminalSafeCodeV2,
};
use crate::rl_session::{
    explicit_deck_hash_v1, FastActorResponseV1, FastActorSessionV1, RlSessionTerminalV1,
    RL_SESSION_SCHEMA_VERSION,
};
use crate::sideboard::{DeckConfigurationV1, RegisteredDeckV1};
use crate::sideboard_play_policy_v1::{
    FrozenPlayObservationTransferV3, FrozenPlayPolicyIdentityV1, FrozenPlayPolicyImportV1,
    FrozenPlayPolicyV1, PlayModelIdentityV1,
};
use crate::state::SplitMix64;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

const TRAJECTORY_SCHEMA: &str = "mtg-kernel-expanded-deck-trajectory/v1";
const POPULATION_TRAJECTORY_SCHEMA: &str = "mtg-kernel-expanded-deck-trajectory/v2";
const CHECKPOINT_SCHEMA: &str = "mtg-kernel-expanded-deck-checkpoint/v1";
const MAX_FILE_BYTES: u64 = 512 * 1024 * 1024;
const MAX_BATCH_BYTES: u64 = 512 * 1024 * 1024;

/// Collection and exact behavior replay remain CPU-based. This selection
/// changes only the learner's recomputation/backward/Adam implementation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ExpandedUpdateBackendV1 {
    #[default]
    Cpu,
    Cuda {
        device_ordinal: usize,
    },
}

impl ExpandedUpdateBackendV1 {
    pub(crate) fn is_cpu(&self) -> bool {
        matches!(self, Self::Cpu)
    }

    pub(crate) fn validate_v1(&self) -> Result<(), String> {
        if let Self::Cuda { device_ordinal } = self {
            ensure(
                *device_ordinal <= i32::MAX as usize,
                "CUDA ordinal exceeds driver index range",
            )?;
        }
        Ok(())
    }

    /// No device is opened here. Refuse an unavailable compiled backend before
    /// creating run artifacts or collecting episodes, without a CPU fallback.
    pub(crate) fn require_compiled_v1(&self) -> Result<(), String> {
        self.validate_v1()?;
        ensure(
            self.is_cpu() || cfg!(feature = "experimental-burn-net8-packed-cuda-v1"),
            "CUDA update backend was not compiled; explicit CUDA feature required",
        )
    }

    pub(crate) fn record_run_execution_v1(&self, document: &mut Value) {
        if let Self::Cuda { device_ordinal } = self {
            document["device"] = json!("cpu-collection-cuda-update");
            document["gpu_ordinal"] = json!(device_ordinal);
            document["update_backend"] = json!(self);
            document["collection_backend"] = json!("native-cpu-sequential");
        }
    }

    fn record_update_execution_v1(&self, document: &mut Value) {
        if let Self::Cuda { device_ordinal } = self {
            document["schema"] = json!("mtg-kernel-expanded-deck-update/v2");
            document["numerical_backend"] = json!("cuda-burn-dense-feature-transfer-v3");
            document["update_backend"] = json!(self);
            document["device"] = json!("cuda");
            document["gpu_ordinal"] = json!(device_ordinal);
            document["behavior_backend"] = json!("native-cpu-sequential");
            document["replay_backend"] = json!("native-cpu-sequential");
            document["reported_loss_source"] = json!("transported-cpu-outputs");
        }
    }

    pub(crate) fn validate_update_execution_v1(&self, document: &Value) -> Result<(), String> {
        self.validate_v1()?;
        let mut expected = json!({
            "schema":"mtg-kernel-expanded-deck-update/v1",
            "numerical_backend":"native-cpu-sequential"
        });
        self.record_update_execution_v1(&mut expected);
        for key in [
            "schema",
            "numerical_backend",
            "update_backend",
            "device",
            "gpu_ordinal",
            "behavior_backend",
            "replay_backend",
            "reported_loss_source",
        ] {
            ensure(
                document.get(key) == expected.get(key),
                &format!("completed update execution differs at {key}"),
            )?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PinnedFileV1 {
    pub path: PathBuf,
    pub sha256: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExpandedModelSourceV1 {
    pub play_import: PinnedFileV1,
    pub feature_transfer: FrozenPlayObservationTransferV3,
    /// None means an explicit fresh-optimizer warm start. A successor
    /// checkpoint resumes both model and Adam, with the same feature identity.
    pub checkpoint: Option<PinnedFileV1>,
}

/// Inference identity after exact current-feature model/optimizer validation.
/// Adam state is checked for checkpoint integrity but is not used by inference.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExpandedInferenceIdentityV1 {
    pub schema: String,
    pub source_import: FrozenPlayPolicyIdentityV1,
    pub checkpoint_sha256: Option<String>,
    pub model: PlayModelIdentityV1,
    pub state_sha256: String,
    pub adam_step: u64,
    pub feature_schema_version: String,
    pub feature_registry_version: String,
    pub features_source_sha256: String,
    pub feature_descriptor_sha256: String,
}

/// Reuses the bounded, pinned continuation reader and its exact feature,
/// ancestry, parameter, optimizer and state checks. No artifacts are written,
/// no training runs, and predecessor feature identities are not migrated.
pub fn load_expanded_inference_v1(
    source: &ExpandedModelSourceV1,
) -> Result<(FrozenPlayPolicyV1, ExpandedInferenceIdentityV1), String> {
    let (policy, state) = initialize(source)?;
    let receipt = inference_identity_v1(source, &policy, &state)?;
    Ok((policy, receipt))
}

fn inference_identity_v1(
    source: &ExpandedModelSourceV1,
    policy: &FrozenPlayPolicyV1,
    state: &NativePolicyValueTrainStateV1,
) -> Result<ExpandedInferenceIdentityV1, String> {
    Ok(ExpandedInferenceIdentityV1 {
        schema: "mtg-kernel-expanded-deck-inference/v1".into(),
        source_import: policy.identity_v1().clone(),
        checkpoint_sha256: source.checkpoint.as_ref().map(|pin| pin.sha256.clone()),
        model: policy.actual_model_identity_v1(),
        state_sha256: hex(&state.state_sha256_v1().map_err(err)?),
        adam_step: state.adam_step_v1(),
        feature_schema_version: FEATURE_SCHEMA_VERSION_V3.into(),
        feature_registry_version: FEATURE_REGISTRY_VERSION_V3.into(),
        features_source_sha256: FEATURES_SOURCE_SHA256_V3.into(),
        feature_descriptor_sha256: FEATURE_DESCRIPTOR_SHA256_V3.into(),
    })
}

/// Physical-seat behavior provenance, independent of deck registration.
/// `source_import` remains ancestry; `identity.model` binds installed weights.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExpandedSeatBehaviorV1 {
    pub source: ExpandedModelSourceV1,
    pub identity: ExpandedInferenceIdentityV1,
}

struct LoadedOpponentV1 {
    policy: FrozenPlayPolicyV1,
    behavior: ExpandedSeatBehaviorV1,
}

/// Keep only the most recently used opponent. Evict before loading a new
/// source so a population roster cannot retain unbounded model/Adam copies.
#[derive(Default)]
struct OpponentCacheV1 {
    entry: Option<LoadedOpponentV1>,
}

impl OpponentCacheV1 {
    fn load(&mut self, source: &ExpandedModelSourceV1) -> Result<&mut LoadedOpponentV1, String> {
        if !self
            .entry
            .as_ref()
            .is_some_and(|e| e.behavior.source == *source)
        {
            self.entry = None;
            let (policy, identity) = load_expanded_inference_v1(source)?;
            self.entry = Some(LoadedOpponentV1 {
                policy,
                behavior: ExpandedSeatBehaviorV1 {
                    source: source.clone(),
                    identity,
                },
            });
        }
        self.entry
            .as_mut()
            .ok_or_else(|| "opponent cache is empty".into())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExpandedDeckListV1 {
    pub label: String,
    pub mainboard: Vec<u16>,
    pub sideboard: Vec<u16>,
}

impl ExpandedDeckListV1 {
    fn validated(&self) -> Result<RegisteredDeckV1, String> {
        RegisteredDeckV1::new_executable_v1(
            &self.label,
            self.mainboard.clone(),
            self.sideboard.clone(),
        )
        .map_err(err)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExpandedEpisodeV1 {
    pub id: String,
    pub seed: u64,
    pub starting_player: u8,
    pub learner_seat: u8,
    /// None retains common-model self-play. Some pins the other physical
    /// seat's independently loaded behavior, including an optional checkpoint.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opponent: Option<ExpandedModelSourceV1>,
    /// Configuration identity is metadata only, never opponent model input.
    pub registered: [ExpandedDeckListV1; 2],
    pub selected: [ExpandedDeckListV1; 2],
    pub postboard: bool,
    pub max_physical_decisions: u64,
    pub max_policy_steps: u64,
}

impl ExpandedEpisodeV1 {
    fn configurations(&self) -> Result<[DeckConfigurationV1; 2], String> {
        ensure(
            !self.id.is_empty() && self.id.len() <= 128,
            "invalid episode id",
        )?;
        ensure(
            self.starting_player < 2 && self.learner_seat < 2,
            "invalid player seat",
        )?;
        ensure(
            (1..=100_000).contains(&self.max_physical_decisions)
                && (1..=1_000_000).contains(&self.max_policy_steps),
            "invalid episode limits",
        )?;
        let mut configs = Vec::new();
        for seat in 0..2 {
            let registered = self.registered[seat].validated()?;
            let selected = self.selected[seat].validated()?;
            let initial = registered.registered_configuration();
            let configuration = selected.registered_configuration();
            ensure(
                initial.combined_card_counts_v1() == configuration.combined_card_counts_v1(),
                "postboard configuration changes the registered 75",
            )?;
            ensure(
                self.postboard || initial == configuration,
                "preboard game differs from registration",
            )?;
            configs.push(configuration.clone());
        }
        Ok([configs.remove(0), configs.remove(0)])
    }
}

/// Raw binary32 bits keep JSON round trips exact, including signed zero.
/// All fields are actor-visible tensors. No private bindings or GameState.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct TensorBitsV1 {
    state: Vec<u32>,
    object_features: Vec<u32>,
    object_card_ids: Vec<i64>,
    object_groups: Vec<i64>,
    object_node_ids: Vec<i64>,
    edge_features: Vec<u32>,
    edge_source_indices: Vec<i64>,
    edge_target_indices: Vec<i64>,
    action_features: Vec<u32>,
    action_ref_features: Vec<u32>,
    action_ref_card_ids: Vec<i64>,
    action_ref_action_indices: Vec<i64>,
    action_ref_node_indices: Vec<i64>,
}

fn bits(values: &[f32]) -> Vec<u32> {
    values.iter().map(|x| x.to_bits()).collect()
}
fn floats(values: &[u32]) -> Vec<f32> {
    values.iter().map(|x| f32::from_bits(*x)).collect()
}
impl TensorBitsV1 {
    fn from_tensor(t: &NativeFlatDecisionTensorV3) -> Self {
        let t = &t.common;
        Self {
            state: bits(&t.state),
            object_features: bits(&t.object_features),
            object_card_ids: t.object_card_ids.clone(),
            object_groups: t.object_groups.clone(),
            object_node_ids: t.object_node_ids.clone(),
            edge_features: bits(&t.edge_features),
            edge_source_indices: t.edge_source_indices.clone(),
            edge_target_indices: t.edge_target_indices.clone(),
            action_features: bits(&t.action_features),
            action_ref_features: bits(&t.action_ref_features),
            action_ref_card_ids: t.action_ref_card_ids.clone(),
            action_ref_action_indices: t.action_ref_action_indices.clone(),
            action_ref_node_indices: t.action_ref_node_indices.clone(),
        }
    }
    fn tensor(&self) -> NativeFlatDecisionTensorV3 {
        NativeFlatDecisionTensorV3 {
            common: NativeFlatDecisionTensorV2 {
                state: floats(&self.state),
                object_features: floats(&self.object_features),
                object_card_ids: self.object_card_ids.clone(),
                object_groups: self.object_groups.clone(),
                object_node_ids: self.object_node_ids.clone(),
                edge_features: floats(&self.edge_features),
                edge_source_indices: self.edge_source_indices.clone(),
                edge_target_indices: self.edge_target_indices.clone(),
                action_features: floats(&self.action_features),
                action_ref_features: floats(&self.action_ref_features),
                action_ref_card_ids: self.action_ref_card_ids.clone(),
                action_ref_action_indices: self.action_ref_action_indices.clone(),
                action_ref_node_indices: self.action_ref_node_indices.clone(),
            },
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DecisionRecordV1 {
    step: u64,
    physical_decision_id: u64,
    substep_index: u32,
    substep_count: u32,
    actor: u8,
    selected: u32,
    logits: Vec<u32>,
    value: u32,
    tensor: TensorBitsV1,
    /// Absent for the frozen narrow behavior, preserving archived JSON bytes.
    /// Wide menus require an explicit runtime receipt, not an ancestry edit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    sampler_identity: Option<String>,
}

fn decision_sampler_identity_v1(width: usize) -> Option<&'static str> {
    (width > FAST_CATEGORICAL_MAX_ACTIONS).then_some(WIDE_CATEGORICAL_SAMPLER_VERSION_V1)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ExpandedTrajectoryV1 {
    schema: String,
    feature_contract_digest: String,
    feature_encoding_digest: String,
    card_db_hash: String,
    source_import: FrozenPlayPolicyIdentityV1,
    behavior_state_sha256: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    seat_behaviors: Option<[ExpandedSeatBehaviorV1; 2]>,
    episode: ExpandedEpisodeV1,
    configuration_sha256: [String; 2],
    decisions: Vec<DecisionRecordV1>,
    terminal: RlSessionTerminalV1,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ParameterBitsV1 {
    name: String,
    shape: Vec<usize>,
    values: Vec<u32>,
}

impl ParameterBitsV1 {
    fn from_native(p: &NativeNamedParameterV1) -> Self {
        Self {
            name: p.name.into(),
            shape: p.shape.clone(),
            values: bits(&p.values),
        }
    }
}

fn restore_parameters(
    saved: &[ParameterBitsV1],
    template: &[NativeNamedParameterV1],
) -> Result<Vec<NativeNamedParameterV1>, String> {
    ensure(
        saved.len() == template.len(),
        "checkpoint parameter count differs",
    )?;
    saved
        .iter()
        .zip(template)
        .map(|(s, t)| {
            ensure(
                s.name == t.name && s.shape == t.shape && s.values.len() == t.values.len(),
                "checkpoint parameter layout differs",
            )?;
            Ok(NativeNamedParameterV1 {
                name: t.name,
                shape: t.shape.clone(),
                values: floats(&s.values),
            })
        })
        .collect()
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ExpandedCheckpointV1 {
    schema: String,
    feature_contract_digest: String,
    feature_encoding_digest: String,
    card_db_hash: String,
    source_import: FrozenPlayPolicyIdentityV1,
    state_sha256: String,
    adam_step: u64,
    scorer_bias_anchor_bits: u32,
    parameters: Vec<ParameterBitsV1>,
    first_moments: Vec<ParameterBitsV1>,
    second_moments: Vec<ParameterBitsV1>,
    trajectories: Vec<PinnedFileV1>,
    loss_identity: String,
    learning_rate_bits: u32,
    value_coefficient_bits: u32,
}

fn identity_valid(contract: &str, encoding: &str, cards: &str) -> Result<(), String> {
    ensure(
        contract == FEATURE_CONTRACT_DIGEST_V3 && encoding == FEATURE_ENCODING_DIGEST_V3,
        "successor feature identity differs",
    )?;
    ensure(
        cards == format!("{KERNEL_CARDDB_HASH:016x}"),
        "successor card database differs",
    )
}

fn initialize(
    source: &ExpandedModelSourceV1,
) -> Result<(FrozenPlayPolicyV1, NativePolicyValueTrainStateV1), String> {
    let import: FrozenPlayPolicyImportV1 = read_pinned(&source.play_import)?;
    let mut policy =
        FrozenPlayPolicyV1::load_feature_transfer_v3(&import, &source.feature_transfer)?;
    let mut model =
        NativePolicyValueNetV1::runner_fixed_v1(NativePolicyValueModelConfigV1::contract_v1())
            .map_err(err)?;
    model
        .replace_parameter_snapshot_v1(&policy.training_parameters_v3())
        .map_err(err)?;
    let state = if let Some(pin) = &source.checkpoint {
        let saved: ExpandedCheckpointV1 = read_pinned(pin)?;
        restore_checkpoint_state_v1(&saved, &mut policy, model)?
    } else {
        NativePolicyValueTrainStateV1::new_v1(model).map_err(err)?
    };
    Ok((policy, state))
}

fn restore_checkpoint_state_v1(
    saved: &ExpandedCheckpointV1,
    policy: &mut FrozenPlayPolicyV1,
    model: NativePolicyValueNetV1,
) -> Result<NativePolicyValueTrainStateV1, String> {
    ensure(
        saved.schema == CHECKPOINT_SCHEMA,
        "not an expanded-deck checkpoint",
    )?;
    identity_valid(
        &saved.feature_contract_digest,
        &saved.feature_encoding_digest,
        &saved.card_db_hash,
    )?;
    ensure(
        saved.source_import == *policy.identity_v1(),
        "checkpoint warm-start provenance differs",
    )?;
    ensure(
        saved.loss_identity == "terminal_reinforce_value/v3",
        "checkpoint loss differs",
    )?;
    let template = model.parameter_snapshot_v1();
    let snapshot = NativePolicyValueTrainSnapshotV1 {
        adam_step: saved.adam_step,
        scorer_bias_anchor_bits: saved.scorer_bias_anchor_bits,
        parameters: restore_parameters(&saved.parameters, &template)?,
        first_moments: restore_parameters(&saved.first_moments, &template)?,
        second_moments: restore_parameters(&saved.second_moments, &template)?,
    };
    ensure(
        hex(&snapshot.state_sha256_v1().map_err(err)?) == saved.state_sha256,
        "checkpoint state hash differs",
    )?;
    // Validate the model's gauge anchor and full optimizer state before the
    // live policy or copied sideboard embeddings are replaced.
    let state = NativePolicyValueTrainStateV1::from_snapshot_v1(model, &snapshot).map_err(err)?;
    policy.replace_training_parameters_v3(&snapshot.parameters)?;
    Ok(state)
}

fn collect_episode(
    policy: &mut FrozenPlayPolicyV1,
    learner: &ExpandedSeatBehaviorV1,
    mut opponent: Option<&mut LoadedOpponentV1>,
    episode: &ExpandedEpisodeV1,
) -> Result<ExpandedTrajectoryV1, String> {
    let configs = episode.configurations()?;
    ensure(
        episode.opponent.is_some() == opponent.is_some(),
        "opponent dispatch source missing",
    )?;
    let seat_behaviors = opponent.as_ref().map(|other| {
        std::array::from_fn(|actor| {
            if actor == episode.learner_seat as usize {
                learner.clone()
            } else {
                other.behavior.clone()
            }
        })
    });
    let config_hashes = configs.each_ref().map(|c| hex(&c.mainboard_sha256_v1()));
    let mut session = FastActorSessionV1::reset_with_explicit_decks_and_limits_flat_action_v3_environment_v2_with_starting_player_v1(
        1, episode.seed, episode.max_physical_decisions, episode.max_policy_steps,
        episode.selected.each_ref().map(|d| d.label.clone()), configs.each_ref().map(|d| d.mainboard().to_vec()), PlayerId(episode.starting_player)).map_err(err)?;
    let seeds = paired_policy_seeds_v1(episode.seed);
    policy.reset_sampling_v1(seeds);
    if let Some(other) = opponent.as_mut() {
        // Both policies use physical-seat streams, never learner/opponent
        // roles. Only the acting policy consumes its actor's next sample.
        other.policy.reset_sampling_v1(seeds);
    }
    let mut decisions = Vec::new();
    loop {
        match session.current_response() {
            FastActorResponseV1::Terminal(terminal) => {
                ensure(
                    terminal.terminal_classification == TerminalClassificationV1::Natural,
                    "only naturally completed games may become training trajectories",
                )?;
                let result = ExpandedTrajectoryV1 {
                    schema: if opponent.is_some() {
                        POPULATION_TRAJECTORY_SCHEMA
                    } else {
                        TRAJECTORY_SCHEMA
                    }
                    .into(),
                    feature_contract_digest: FEATURE_CONTRACT_DIGEST_V3.into(),
                    feature_encoding_digest: FEATURE_ENCODING_DIGEST_V3.into(),
                    card_db_hash: format!("{KERNEL_CARDDB_HASH:016x}"),
                    source_import: policy.identity_v1().clone(),
                    behavior_state_sha256: learner.identity.state_sha256.clone(),
                    seat_behaviors,
                    episode: episode.clone(),
                    configuration_sha256: config_hashes,
                    decisions,
                    terminal,
                };
                validate_trajectory(&result)?;
                return Ok(result);
            }
            FastActorResponseV1::Decision(d) => {
                ensure(
                    decisions.len() < episode.max_policy_steps as usize,
                    "policy step limit",
                )?;
                let acting = acting_policy_v1(
                    policy,
                    opponent.as_mut().map(|other| &mut other.policy),
                    episode.learner_seat,
                    seat(d.acting_player),
                )?;
                let (selected, scores, tensor) = acting.select_with_training_tensor_v3(&session)?;
                let record = DecisionRecordV1 {
                    step: d.step,
                    physical_decision_id: d.physical_decision_id,
                    substep_index: d.substep_index,
                    substep_count: d.substep_count,
                    actor: seat(d.acting_player),
                    selected,
                    logits: bits(&scores.logits),
                    value: scores.value.to_bits(),
                    tensor: TensorBitsV1::from_tensor(&tensor),
                    sampler_identity: decision_sampler_identity_v1(scores.logits.len())
                        .map(str::to_owned),
                };
                session.step(d.episode_id, d.step, selected).map_err(err)?;
                decisions.push(record);
            }
        }
    }
}

fn acting_policy_v1<'a>(
    learner: &'a mut FrozenPlayPolicyV1,
    opponent: Option<&'a mut FrozenPlayPolicyV1>,
    learner_seat: u8,
    actor: u8,
) -> Result<&'a mut FrozenPlayPolicyV1, String> {
    ensure(learner_seat < 2 && actor < 2, "invalid behavior seat")?;
    if actor == learner_seat {
        Ok(learner)
    } else {
        Ok(opponent.unwrap_or(learner))
    }
}

fn validate_behavior_shape_v1(t: &ExpandedTrajectoryV1) -> Result<(), String> {
    match (&t.episode.opponent, &t.seat_behaviors) {
        (None, None) => ensure(
            t.schema == TRAJECTORY_SCHEMA,
            "self-play trajectory schema differs",
        ),
        (Some(opponent), Some(behaviors)) => {
            ensure(
                t.schema == POPULATION_TRAJECTORY_SCHEMA,
                "population trajectory schema differs",
            )?;
            ensure(t.episode.learner_seat < 2, "invalid learner seat")?;
            let learner_seat = t.episode.learner_seat as usize;
            ensure(
                behaviors[1 - learner_seat].source == *opponent,
                "opponent source differs from episode",
            )?;
            ensure(
                behaviors[learner_seat].identity.state_sha256 == t.behavior_state_sha256
                    && behaviors[learner_seat].identity.source_import == t.source_import,
                "learner behavior identity differs from trajectory",
            )
        }
        _ => {
            Err("opponent source and per-seat behavior identities must be present together".into())
        }
    }
}

fn validate_actual_behaviors_v1(
    t: &ExpandedTrajectoryV1,
    learner: &ExpandedSeatBehaviorV1,
    opponent: Option<&ExpandedSeatBehaviorV1>,
) -> Result<(), String> {
    validate_behavior_shape_v1(t)?;
    ensure(
        t.behavior_state_sha256 == learner.identity.state_sha256,
        "stale trajectory: behavior optimizer/model state differs",
    )?;
    ensure(
        t.source_import == learner.identity.source_import,
        "trajectory source import differs",
    )?;
    match (&t.seat_behaviors, opponent) {
        (None, None) => Ok(()),
        (Some(recorded), Some(other)) => {
            let learner_seat = t.episode.learner_seat as usize;
            ensure(
                recorded[learner_seat] == *learner,
                "actual learner behavior identity differs",
            )?;
            ensure(
                recorded[1 - learner_seat] == *other,
                "actual opponent behavior identity differs",
            )
        }
        _ => Err("actual opponent source missing or unexpected".into()),
    }
}

fn validate_trajectory(t: &ExpandedTrajectoryV1) -> Result<(), String> {
    validate_behavior_shape_v1(t)?;
    identity_valid(
        &t.feature_contract_digest,
        &t.feature_encoding_digest,
        &t.card_db_hash,
    )?;
    let configs = t.episode.configurations()?;
    ensure(
        t.configuration_sha256 == configs.each_ref().map(|c| hex(&c.mainboard_sha256_v1())),
        "selected deck hash differs",
    )?;
    ensure(
        t.terminal.terminal_classification == TerminalClassificationV1::Natural,
        "incomplete or halted trajectory",
    )?;
    ensure(
        t.terminal.terminal_code == TerminalSafeCodeV2::NaturalGameOver
            && terminal_tuple_is_valid_v1(
                t.terminal.terminal_outcome,
                t.terminal.terminal_classification,
                t.terminal.winner,
                t.terminal.terminal_reward,
            ),
        "terminal outcome, code and reward disagree",
    )?;
    ensure(
        t.terminal.schema_version == RL_SESSION_SCHEMA_VERSION && t.terminal.episode_id == 1,
        "terminal schema or episode binding differs",
    )?;
    ensure(
        t.terminal.deck_ids == t.episode.selected.each_ref().map(|d| d.label.clone())
            && t.terminal.deck_hashes
                == configs
                    .each_ref()
                    .map(|c| explicit_deck_hash_v1(c.mainboard())),
        "terminal deck binding differs",
    )?;
    ensure(
        t.terminal.policy_step_count == t.decisions.len() as u64 && !t.decisions.is_empty(),
        "trajectory step count differs",
    )?;
    let expected_rewards = match t.terminal.winner {
        Some(PlayerSeatV1::P0) => [1, -1],
        Some(PlayerSeatV1::P1) => [-1, 1],
        None => [0, 0],
    };
    ensure(
        t.terminal.terminal_reward == expected_rewards,
        "terminal rewards contradict winner",
    )?;
    let mut physical = 0u64;
    let mut index = 0usize;
    let mut rng = paired_policy_seeds_v1(t.episode.seed).map(SplitMix64::seed);
    let mut sampler = WideCategoricalScratchV1::default();
    while index < t.decisions.len() {
        let first = &t.decisions[index];
        ensure(
            first.physical_decision_id == physical
                && first.substep_index == 0
                && first.substep_count > 0
                && first.actor < 2,
            "invalid physical decision start",
        )?;
        let end = index
            .checked_add(first.substep_count as usize)
            .ok_or("group length overflow")?;
        ensure(end <= t.decisions.len(), "truncated physical decision")?;
        for (substep, row) in t.decisions[index..end].iter().enumerate() {
            ensure(
                row.step == (index + substep) as u64
                    && row.physical_decision_id == physical
                    && row.substep_count == first.substep_count
                    && row.substep_index == substep as u32
                    && row.actor == first.actor,
                "physical decision grouping or step sequence differs",
            )?;
            ensure(
                !row.logits.is_empty()
                    && (row.selected as usize) < row.logits.len()
                    && f32::from_bits(row.value).is_finite()
                    && row.logits.iter().all(|v| f32::from_bits(*v).is_finite()),
                "invalid captured outputs",
            )?;
            ensure(
                row.sampler_identity.as_deref() == decision_sampler_identity_v1(row.logits.len()),
                "stored decision sampler identity differs from action width",
            )?;
            let selected = sampler
                .sample(&floats(&row.logits), rng[row.actor as usize].next_u64())
                .map_err(err)?;
            ensure(
                selected == row.selected as usize,
                "stored action differs from recorded behavior sampler",
            )?;
        }
        physical += 1;
        index = end;
    }
    ensure(
        physical == t.terminal.physical_decision_count,
        "terminal physical count differs",
    )
}

type LearnerTensorGroupV1<'a> = (i8, Vec<(&'a DecisionRecordV1, NativeFlatDecisionTensorV3)>);

/// Validate every stored actor-visible forward before returning learner-only
/// physical decisions. Opponent tensors never enter the optimizer input.
fn replay_learner_groups_v1<'a>(
    t: &'a ExpandedTrajectoryV1,
    learner: &FrozenPlayPolicyV1,
    opponent: Option<&FrozenPlayPolicyV1>,
) -> Result<Vec<LearnerTensorGroupV1<'a>>, String> {
    validate_trajectory(t)?;
    ensure(
        t.episode.opponent.is_some() == opponent.is_some(),
        "opponent replay source missing",
    )?;
    let mut groups = Vec::new();
    let mut index = 0;
    while index < t.decisions.len() {
        let first = &t.decisions[index];
        let count = first.substep_count as usize;
        let mut group = Vec::new();
        for row in &t.decisions[index..index + count] {
            let tensor = row.tensor.tensor();
            let acting = if row.actor == t.episode.learner_seat {
                learner
            } else {
                opponent.unwrap_or(learner)
            };
            let output = acting.score_training_tensor_v3(&tensor)?;
            ensure(
                bits(&output.logits) == row.logits && output.value.to_bits() == row.value,
                "stored tensor does not reproduce rollout outputs",
            )?;
            if row.actor == t.episode.learner_seat {
                group.push((row, tensor));
            }
        }
        if !group.is_empty() {
            groups.push((
                t.terminal.terminal_reward[first.actor as usize] as i8,
                group,
            ));
        }
        index += count;
    }
    Ok(groups)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
pub enum ExpandedTrainingCommandV1 {
    Collect {
        source: ExpandedModelSourceV1,
        episodes: Vec<ExpandedEpisodeV1>,
        output_directory: PathBuf,
    },
    Update {
        source: ExpandedModelSourceV1,
        trajectories: Vec<PinnedFileV1>,
        learning_rate: f32,
        value_coefficient: f32,
        #[serde(default, skip_serializing_if = "ExpandedUpdateBackendV1::is_cpu")]
        update_backend: ExpandedUpdateBackendV1,
        output_directory: PathBuf,
    },
}

pub fn execute_v1(command: ExpandedTrainingCommandV1) -> Result<Value, String> {
    match command {
        ExpandedTrainingCommandV1::Collect {
            source,
            episodes,
            output_directory,
        } => {
            ensure(
                !episodes.is_empty() && episodes.len() <= 1024,
                "invalid collection size",
            )?;
            let mut ids = BTreeSet::new();
            for episode in &episodes {
                episode.configurations()?;
                ensure(ids.insert(&episode.id), "duplicate episode id")?;
            }
            let (mut policy, state) = initialize(&source)?;
            let state_hash = hex(&state.state_sha256_v1().map_err(err)?);
            let learner = ExpandedSeatBehaviorV1 {
                source: source.clone(),
                identity: inference_identity_v1(&source, &policy, &state)?,
            };
            drop(state);
            let mut opponent_cache = OpponentCacheV1::default();
            fs::create_dir(&output_directory).map_err(err)?;
            let mut outputs = Vec::new();
            for (index, episode) in episodes.iter().enumerate() {
                eprintln!(
                    "collect episode {}/{} {}",
                    index + 1,
                    episodes.len(),
                    episode.id
                );
                let opponent = episode
                    .opponent
                    .as_ref()
                    .map(|s| opponent_cache.load(s))
                    .transpose()?;
                let trajectory = collect_episode(&mut policy, &learner, opponent, episode)?;
                let name = format!("episode-{index:04}.json");
                outputs.push(publish_json(&output_directory, &name, &trajectory)?);
            }
            let result = json!({"schema":"mtg-kernel-expanded-deck-collection/v1", "complete":true, "source": source, "behavior_state_sha256": state_hash, "trajectories": outputs});
            publish_json(&output_directory, "collection.json", &result)?;
            Ok(result)
        }
        ExpandedTrainingCommandV1::Update {
            source,
            trajectories,
            learning_rate,
            value_coefficient,
            update_backend,
            output_directory,
        } => {
            update_backend.require_compiled_v1()?;
            ensure(
                !trajectories.is_empty() && trajectories.len() <= 1024,
                "invalid update size",
            )?;
            ensure(
                learning_rate.is_finite()
                    && learning_rate > 0.0
                    && value_coefficient.is_finite()
                    && value_coefficient > 0.0,
                "invalid optimizer configuration",
            )?;
            ensure(
                !output_directory.exists(),
                "output directory already exists",
            )?;
            let (policy, mut state) = initialize(&source)?;
            let before = hex(&state.state_sha256_v1().map_err(err)?);
            let learner = ExpandedSeatBehaviorV1 {
                source: source.clone(),
                identity: inference_identity_v1(&source, &policy, &state)?,
            };
            let mut episodes = Vec::new();
            let mut ids = BTreeSet::new();
            let mut hashes = BTreeSet::new();
            let mut total_bytes = 0u64;
            for pin in &trajectories {
                total_bytes = total_bytes
                    .checked_add(fs::metadata(&pin.path).map_err(err)?.len())
                    .ok_or("batch size overflow")?;
                ensure(
                    total_bytes <= MAX_BATCH_BYTES,
                    "trajectory batch exceeds 512 MiB CPU ingestion bound",
                )?;
            }
            for pin in &trajectories {
                ensure(hashes.insert(&pin.sha256), "duplicate trajectory bytes")?;
                let episode: ExpandedTrajectoryV1 = read_pinned(pin)?;
                validate_trajectory(&episode)?;
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
                episodes.push(episode);
            }
            // Recompute all actor-visible rows, including opponent decisions,
            // before constructing learner groups. No private state is decoded.
            let mut tensor_groups = Vec::new();
            let mut opponent_cache = OpponentCacheV1::default();
            for episode in &episodes {
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
                    episode,
                    &policy,
                    opponent.as_ref().map(|o| &o.policy),
                )?);
            }
            drop(opponent_cache);
            ensure(!tensor_groups.is_empty(), "no learner decisions")?;
            let substeps: Vec<Vec<NativePolicySubstepV1<'_>>> = tensor_groups
                .iter()
                .map(|(_, group)| {
                    group
                        .iter()
                        .map(|(row, t)| NativePolicySubstepV1 {
                            forward: NativePolicyForwardInputV1::Encoded(Box::new(
                                encoded_decision_view_v3(t),
                            )),
                            selected_action_index: row.selected as usize,
                            expected_raw_action_logit_bits: &row.logits,
                            expected_value_bits: row.value,
                        })
                        .collect()
                })
                .collect();
            let groups: Vec<_> = substeps
                .iter()
                .zip(&tensor_groups)
                .map(
                    |(substeps, (terminal_return, _))| NativePolicyPhysicalDecisionV1 {
                        substeps,
                        terminal_return: *terminal_return,
                        baseline_bits: 0,
                    },
                )
                .collect();
            eprintln!(
                "native {:?} update: {} episodes, {} learner physical decisions",
                update_backend,
                episodes.len(),
                groups.len()
            );
            let update = match update_backend {
                ExpandedUpdateBackendV1::Cpu => state
                    .train_step_feature_transfer_v3(&groups, value_coefficient, learning_rate)
                    .map_err(err)?,
                ExpandedUpdateBackendV1::Cuda { device_ordinal } => {
                    #[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
                    {
                        state
                            .train_step_cuda_feature_transfer_v3(
                                &groups,
                                value_coefficient,
                                learning_rate,
                                device_ordinal,
                            )
                            .map_err(err)?
                    }
                    #[cfg(not(feature = "experimental-burn-net8-packed-cuda-v1"))]
                    {
                        let _ = device_ordinal;
                        return Err("CUDA update backend was not compiled".into());
                    }
                }
            };
            let snapshot = state.snapshot_v1().map_err(err)?;
            let after = hex(&snapshot.state_sha256_v1().map_err(err)?);
            let checkpoint = ExpandedCheckpointV1 {
                schema: CHECKPOINT_SCHEMA.into(),
                feature_contract_digest: FEATURE_CONTRACT_DIGEST_V3.into(),
                feature_encoding_digest: FEATURE_ENCODING_DIGEST_V3.into(),
                card_db_hash: format!("{KERNEL_CARDDB_HASH:016x}"),
                source_import: policy.identity_v1().clone(),
                state_sha256: after.clone(),
                adam_step: snapshot.adam_step,
                scorer_bias_anchor_bits: snapshot.scorer_bias_anchor_bits,
                parameters: snapshot
                    .parameters
                    .iter()
                    .map(ParameterBitsV1::from_native)
                    .collect(),
                first_moments: snapshot
                    .first_moments
                    .iter()
                    .map(ParameterBitsV1::from_native)
                    .collect(),
                second_moments: snapshot
                    .second_moments
                    .iter()
                    .map(ParameterBitsV1::from_native)
                    .collect(),
                trajectories: trajectories.clone(),
                loss_identity: "terminal_reinforce_value/v3".into(),
                learning_rate_bits: learning_rate.to_bits(),
                value_coefficient_bits: value_coefficient.to_bits(),
            };
            fs::create_dir(&output_directory).map_err(err)?;
            let checkpoint_pin = publish_json(&output_directory, "checkpoint.json", &checkpoint)?;
            // Readback exercises the real same-format continuation loader.
            let resumed_source = ExpandedModelSourceV1 {
                checkpoint: Some(checkpoint_pin.clone()),
                ..source.clone()
            };
            let (_, restored) = initialize(&resumed_source)?;
            ensure(
                hex(&restored.state_sha256_v1().map_err(err)?) == after,
                "published checkpoint round trip differs",
            )?;
            let mut result = json!({"schema":"mtg-kernel-expanded-deck-update/v1", "complete":true, "source":source, "trajectories":trajectories, "checkpoint":checkpoint_pin, "before_state_sha256":before, "after_state_sha256":after, "adam_step":update.adam_step, "physical_decisions":groups.len(), "policy_substeps":update.selected_outputs.len(), "loss":update.loss, "policy_sum":update.policy_sum, "value_sum":update.value_sum, "checkpoint_readback":true, "numerical_backend":"native-cpu-sequential", "loss_identity":"terminal_reinforce_value/v3", "claim":"engineering update only; no playing-strength or production-throughput claim"});
            update_backend.record_update_execution_v1(&mut result);
            publish_json(&output_directory, "update.json", &result)?;
            Ok(result)
        }
    }
}

fn seat(p: PlayerSeatV1) -> u8 {
    match p {
        PlayerSeatV1::P0 => 0,
        PlayerSeatV1::P1 => 1,
    }
}
fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}
fn ensure(ok: bool, message: &str) -> Result<(), String> {
    if ok {
        Ok(())
    } else {
        Err(message.into())
    }
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn sha(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

fn read_pinned<T: for<'de> Deserialize<'de>>(pin: &PinnedFileV1) -> Result<T, String> {
    let metadata = fs::metadata(&pin.path).map_err(err)?;
    ensure(
        metadata.is_file() && metadata.len() <= MAX_FILE_BYTES,
        "input is not a bounded regular file",
    )?;
    let bytes = fs::read(&pin.path).map_err(err)?;
    ensure(
        bytes.len() as u64 <= MAX_FILE_BYTES && sha(&bytes) == pin.sha256,
        "input file SHA differs",
    )?;
    serde_json::from_slice(&bytes).map_err(err)
}

fn publish_json<T: Serialize>(
    directory: &Path,
    name: &str,
    value: &T,
) -> Result<PinnedFileV1, String> {
    let bytes = serde_json::to_vec(value).map_err(err)?;
    ensure(
        bytes.len() as u64 <= MAX_FILE_BYTES,
        "output exceeds file size limit",
    )?;
    let parent = capture_existing_publication_parent_v1(directory).map_err(err)?;
    let expected = DurableFileExpectationV1::from_bytes(&bytes).map_err(err)?;
    publish_new_file_v1(&parent, format!(".{name}.stage"), name, &bytes, expected).map_err(err)?;
    Ok(PinnedFileV1 {
        path: directory.join(name),
        sha256: sha(&bytes),
    })
}

#[cfg(test)]
mod cuda_probe_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sideboard::checked_in_pauper_registered_deck_by_id_v1;

    #[test]
    fn wide_trajectory_sampler_receipt_is_required_only_for_wide_decisions() {
        let mut policy = FrozenPlayPolicyV1::training_fixture_v3();
        let (mut trajectory, _, _) = replay_fixture(&mut policy, None, 0, &[0, 1, 0, 1, 1, 0]);
        let narrow_bytes = serde_json::to_vec(&trajectory).unwrap();
        for row in &trajectory.decisions {
            assert!(row.sampler_identity.is_none());
            assert!(serde_json::to_value(row)
                .unwrap()
                .get("sampler_identity")
                .is_none());
        }
        let roundtrip: ExpandedTrajectoryV1 = serde_json::from_slice(&narrow_bytes).unwrap();
        assert_eq!(serde_json::to_vec(&roundtrip).unwrap(), narrow_bytes);
        validate_trajectory(&roundtrip).unwrap();

        // Exercise the stored sampling envelope independently of tensor/model
        // replay. These synthetic logits are not claimed as real forwards.
        let mut rng = paired_policy_seeds_v1(trajectory.episode.seed).map(SplitMix64::seed);
        for (row, width) in trajectory
            .decisions
            .iter_mut()
            .zip([64, 120, 257, 5_040, 3, 65])
        {
            row.logits = vec![0.0f32.to_bits(); width];
            row.sampler_identity = decision_sampler_identity_v1(width).map(str::to_owned);
            let draw = u128::from(crate::fast_sampler::splitmix64_first(
                rng[row.actor as usize].next_u64(),
            ));
            let total = 1u128 << 64;
            let quotient = total / width as u128;
            let residual = total % width as u128;
            let prefix = (quotient + 1) * residual;
            row.selected = if draw < prefix {
                draw / (quotient + 1)
            } else {
                residual + (draw - prefix) / quotient
            } as u32;
        }
        validate_trajectory(&trajectory).unwrap();
        let wide_bytes = serde_json::to_vec(&trajectory).unwrap();
        let roundtrip: ExpandedTrajectoryV1 = serde_json::from_slice(&wide_bytes).unwrap();
        validate_trajectory(&roundtrip).unwrap();
        for receipt in [None, Some("wrong-sampler".into())] {
            let mut changed = trajectory.clone();
            changed.decisions[1].sampler_identity = receipt;
            assert!(validate_trajectory(&changed)
                .unwrap_err()
                .contains("sampler identity"));
        }
        let mut changed = trajectory.clone();
        changed.decisions[0].sampler_identity = Some(WIDE_CATEGORICAL_SAMPLER_VERSION_V1.into());
        assert!(validate_trajectory(&changed)
            .unwrap_err()
            .contains("sampler identity"));
        let mut changed = trajectory;
        changed.decisions[2].selected = (changed.decisions[2].selected + 1) % 257;
        assert!(validate_trajectory(&changed)
            .unwrap_err()
            .contains("recorded behavior sampler"));
    }

    #[test]
    fn update_receipt_requires_declared_backend_device_and_loss_provenance() {
        let cpu = ExpandedUpdateBackendV1::Cpu;
        let cuda = ExpandedUpdateBackendV1::Cuda { device_ordinal: 1 };
        let base = json!({"schema":"mtg-kernel-expanded-deck-update/v1",
            "numerical_backend":"native-cpu-sequential"});
        let mut legacy = base.clone();
        cpu.record_update_execution_v1(&mut legacy);
        assert_eq!(legacy, base);
        cpu.validate_update_execution_v1(&legacy).unwrap();
        assert!(cuda.validate_update_execution_v1(&legacy).is_err());
        let mut device = base.clone();
        cuda.record_update_execution_v1(&mut device);
        cuda.validate_update_execution_v1(&device).unwrap();
        assert!(cpu.validate_update_execution_v1(&device).is_err());
        assert_eq!(device["reported_loss_source"], "transported-cpu-outputs");
        for key in [
            "schema",
            "numerical_backend",
            "update_backend",
            "device",
            "gpu_ordinal",
            "behavior_backend",
            "replay_backend",
            "reported_loss_source",
        ] {
            let mut missing = device.clone();
            missing.as_object_mut().unwrap().remove(key);
            assert!(
                cuda.validate_update_execution_v1(&missing).is_err(),
                "{key}"
            );
            let mut wrong = device.clone();
            wrong[key] = json!("incorrect");
            assert!(cuda.validate_update_execution_v1(&wrong).is_err(), "{key}");
        }
        let other = ExpandedUpdateBackendV1::Cuda { device_ordinal: 2 };
        assert!(other.validate_update_execution_v1(&device).is_err());
        let mut misleading_cpu = legacy;
        misleading_cpu["gpu_ordinal"] = json!(1);
        assert!(cpu.validate_update_execution_v1(&misleading_cpu).is_err());
    }

    #[test]
    fn update_backend_parser_requires_device_and_preserves_legacy_command_shape() {
        for invalid in [
            json!({"kind":"cuda"}),
            json!({"kind":"cuda", "device_ordinal":-1}),
            json!({"kind":"unavailable"}),
            json!({"kind":"cuda", "device_ordinal":1, "fallback":"cpu"}),
        ] {
            assert!(serde_json::from_value::<ExpandedUpdateBackendV1>(invalid).is_err());
        }
        let source = json!({"play_import":{"path":"test-import.json", "sha256":"a".repeat(64)},
            "feature_transfer":{"expected_feature_contract_digest":FEATURE_CONTRACT_DIGEST_V3,
                "expected_feature_encoding_digest":FEATURE_ENCODING_DIGEST_V3}, "checkpoint":null});
        let command = json!({"mode":"update", "source":source, "trajectories":[],
            "learning_rate":0.00001_f32, "value_coefficient":0.5_f32,
            "output_directory":std::env::temp_dir().join("unused-update-output")});
        let decoded: ExpandedTrainingCommandV1 = serde_json::from_value(command.clone()).unwrap();
        assert_eq!(serde_json::to_value(&decoded).unwrap(), command);
        let mut explicit = command;
        explicit["update_backend"] = json!({"kind":"cuda", "device_ordinal":1});
        let decoded: ExpandedTrainingCommandV1 = serde_json::from_value(explicit.clone()).unwrap();
        assert_eq!(serde_json::to_value(&decoded).unwrap(), explicit);
    }

    fn test_state(policy: &FrozenPlayPolicyV1) -> NativePolicyValueTrainStateV1 {
        let mut model =
            NativePolicyValueNetV1::runner_fixed_v1(NativePolicyValueModelConfigV1::contract_v1())
                .unwrap();
        model
            .replace_parameter_snapshot_v1(&policy.training_parameters_v3())
            .unwrap();
        NativePolicyValueTrainStateV1::new_v1(model).unwrap()
    }

    fn test_behavior(policy: &FrozenPlayPolicyV1, checkpoint: bool) -> ExpandedSeatBehaviorV1 {
        let source = ExpandedModelSourceV1 {
            play_import: PinnedFileV1 {
                path: "test-import.json".into(),
                sha256: "a".repeat(64),
            },
            feature_transfer: FrozenPlayObservationTransferV3 {
                expected_feature_contract_digest: FEATURE_CONTRACT_DIGEST_V3.into(),
                expected_feature_encoding_digest: FEATURE_ENCODING_DIGEST_V3.into(),
            },
            checkpoint: checkpoint.then(|| PinnedFileV1 {
                path: "test-opponent.json".into(),
                sha256: "b".repeat(64),
            }),
        };
        ExpandedSeatBehaviorV1 {
            identity: inference_identity_v1(&source, policy, &test_state(policy)).unwrap(),
            source,
        }
    }

    fn distinct_opponent() -> FrozenPlayPolicyV1 {
        let mut policy = FrozenPlayPolicyV1::training_fixture_v3();
        let mut parameters = policy.training_parameters_v3();
        for value in &mut parameters
            .iter_mut()
            .find(|p| p.name == "scorer.2.weight")
            .unwrap()
            .values
        {
            *value = -*value * 2.0;
        }
        parameters
            .iter_mut()
            .find(|p| p.name == "value_head.2.bias")
            .unwrap()
            .values[0] += 0.25;
        policy.replace_training_parameters_v3(&parameters).unwrap();
        policy
    }

    fn actor_session(actor: u8) -> FastActorSessionV1 {
        use crate::mana::ManaColor;
        use crate::policy_observation_v6::tests::{put, ready_state};
        use crate::state::Zone;
        let mut state = ready_state();
        state.active_player = PlayerId(actor);
        state.priority_player = PlayerId(actor);
        put(&mut state, PlayerId(actor), "Lightning Bolt", Zone::Hand);
        state.players[actor as usize].mana_pool[ManaColor::R.pool_index()] = 1;
        for player in [PlayerId::P0, PlayerId::P1] {
            put(&mut state, player, "Forest", Zone::Library);
        }
        let session = FastActorSessionV1::from_v3_fixture_state(state);
        let FastActorResponseV1::Decision(d) = session.current_response() else {
            panic!("live fixture")
        };
        assert_eq!(seat(d.acting_player), actor);
        assert!(d.legal_action_count > 1);
        session
    }

    fn test_episode(
        learner_seat: u8,
        opponent: Option<ExpandedModelSourceV1>,
    ) -> ExpandedEpisodeV1 {
        ExpandedEpisodeV1 {
            id: "population-test".into(),
            seed: 918_337,
            starting_player: 0,
            learner_seat,
            opponent,
            registered: [list("Affinity"), list("Terror")],
            selected: [list("Affinity"), list("Terror")],
            postboard: false,
            max_physical_decisions: 100,
            max_policy_steps: 1000,
        }
    }

    /// Real actor-visible decision tensors sampled in a test-only grouping
    /// container. The synthetic terminal supplies reward/shape metadata only;
    /// this fixture is not a completed game or playing-strength measurement.
    fn replay_fixture(
        learner: &mut FrozenPlayPolicyV1,
        mut opponent: Option<&mut FrozenPlayPolicyV1>,
        learner_seat: u8,
        actors: &[u8],
    ) -> (
        ExpandedTrajectoryV1,
        ExpandedSeatBehaviorV1,
        Option<ExpandedSeatBehaviorV1>,
    ) {
        let learner_behavior = test_behavior(learner, false);
        let opponent_behavior = opponent.as_ref().map(|p| test_behavior(p, true));
        let episode = test_episode(
            learner_seat,
            opponent_behavior.as_ref().map(|b| b.source.clone()),
        );
        let seeds = paired_policy_seeds_v1(episode.seed);
        learner.reset_sampling_v1(seeds);
        if let Some(other) = opponent.as_mut() {
            other.reset_sampling_v1(seeds);
        }
        let sessions = [actor_session(0), actor_session(1)];
        let mut decisions = Vec::new();
        for (index, &actor) in actors.iter().enumerate() {
            let acting =
                acting_policy_v1(learner, opponent.as_deref_mut(), learner_seat, actor).unwrap();
            let (selected, scores, tensor) = acting
                .select_with_training_tensor_v3(&sessions[actor as usize])
                .unwrap();
            decisions.push(DecisionRecordV1 {
                step: index as u64,
                physical_decision_id: index as u64,
                substep_index: 0,
                substep_count: 1,
                actor,
                selected,
                logits: bits(&scores.logits),
                value: scores.value.to_bits(),
                tensor: TensorBitsV1::from_tensor(&tensor),
                sampler_identity: decision_sampler_identity_v1(scores.logits.len())
                    .map(str::to_owned),
            });
        }
        let configs = episode.configurations().unwrap();
        let trajectory = ExpandedTrajectoryV1 {
            schema: if opponent.is_some() {
                POPULATION_TRAJECTORY_SCHEMA
            } else {
                TRAJECTORY_SCHEMA
            }
            .into(),
            feature_contract_digest: FEATURE_CONTRACT_DIGEST_V3.into(),
            feature_encoding_digest: FEATURE_ENCODING_DIGEST_V3.into(),
            card_db_hash: format!("{KERNEL_CARDDB_HASH:016x}"),
            source_import: learner.identity_v1().clone(),
            behavior_state_sha256: learner_behavior.identity.state_sha256.clone(),
            seat_behaviors: opponent_behavior.as_ref().map(|other| {
                std::array::from_fn(|actor| {
                    if actor == learner_seat as usize {
                        learner_behavior.clone()
                    } else {
                        other.clone()
                    }
                })
            }),
            configuration_sha256: configs.each_ref().map(|c| hex(&c.mainboard_sha256_v1())),
            terminal: RlSessionTerminalV1 {
                schema_version: RL_SESSION_SCHEMA_VERSION,
                deck_ids: episode.selected.each_ref().map(|d| d.label.clone()),
                deck_hashes: configs
                    .each_ref()
                    .map(|c| explicit_deck_hash_v1(c.mainboard())),
                episode_id: 1,
                terminal_outcome: crate::rl::TerminalOutcomeV1::P0Win,
                terminal_classification: TerminalClassificationV1::Natural,
                terminal_code: TerminalSafeCodeV2::NaturalGameOver,
                winner: Some(PlayerSeatV1::P0),
                terminal_reward: [1, -1],
                terminal_reason: "synthetic unit-test grouping fixture".into(),
                policy_step_count: decisions.len() as u64,
                physical_decision_count: decisions.len() as u64,
            },
            episode,
            decisions,
        };
        (trajectory, learner_behavior, opponent_behavior)
    }

    #[test]
    fn population_routes_distinct_parameters_and_physical_seat_rng_for_both_learner_seats() {
        for learner_seat in [0, 1] {
            let mut learner = FrozenPlayPolicyV1::training_fixture_v3();
            let mut opponent = distinct_opponent();
            assert_eq!(learner.identity_v1(), opponent.identity_v1());
            assert_ne!(
                learner.actual_model_identity_v1(),
                opponent.actual_model_identity_v1()
            );
            let actors: Vec<u8> = [0, 1, 1, 0, 1, 0, 0, 1].repeat(4);
            let (trajectory, learner_behavior, opponent_behavior) =
                replay_fixture(&mut learner, Some(&mut opponent), learner_seat, &actors);
            validate_actual_behaviors_v1(
                &trajectory,
                &learner_behavior,
                opponent_behavior.as_ref(),
            )
            .unwrap();
            validate_trajectory(&trajectory).unwrap();
            let mut rng = paired_policy_seeds_v1(trajectory.episode.seed).map(SplitMix64::seed);
            let mut sampler = WideCategoricalScratchV1::default();
            for row in &trajectory.decisions {
                let tensor = row.tensor.tensor();
                let expected = if row.actor == learner_seat {
                    &learner
                } else {
                    &opponent
                }
                .score_training_tensor_v3(&tensor)
                .unwrap();
                let wrong = if row.actor == learner_seat {
                    &opponent
                } else {
                    &learner
                }
                .score_training_tensor_v3(&tensor)
                .unwrap();
                assert_eq!(row.logits, bits(&expected.logits));
                assert_eq!(row.value, expected.value.to_bits());
                assert_ne!(row.logits, bits(&wrong.logits));
                assert_ne!(row.value, wrong.value.to_bits());
                assert_eq!(
                    row.selected as usize,
                    sampler
                        .sample(&expected.logits, rng[row.actor as usize].next_u64())
                        .unwrap()
                );
            }
        }
    }

    #[test]
    fn population_replay_checks_opponent_identity_outputs_and_tensors_before_learner_filter() {
        for learner_seat in [0, 1] {
            let mut learner = FrozenPlayPolicyV1::training_fixture_v3();
            let mut opponent = distinct_opponent();
            let (trajectory, learner_behavior, opponent_behavior) =
                replay_fixture(&mut learner, Some(&mut opponent), learner_seat, &[0, 1]);
            let groups = replay_learner_groups_v1(&trajectory, &learner, Some(&opponent)).unwrap();
            assert_eq!(groups.len(), 1);
            assert_eq!(groups[0].0, if learner_seat == 0 { 1 } else { -1 });
            assert!(groups[0].1.iter().all(|(row, _)| row.actor == learner_seat));
            assert!(replay_learner_groups_v1(&trajectory, &learner, Some(&learner)).is_err());
            let mut changed = trajectory.clone();
            let other = (1 - learner_seat) as usize;
            changed.seat_behaviors.as_mut().unwrap()[other]
                .identity
                .model
                .weights_sha256 = "0".repeat(64);
            assert_eq!(
                validate_actual_behaviors_v1(
                    &changed,
                    &learner_behavior,
                    opponent_behavior.as_ref()
                )
                .unwrap_err(),
                "actual opponent behavior identity differs"
            );
            changed = trajectory.clone();
            changed.seat_behaviors.as_mut().unwrap()[other]
                .source
                .checkpoint
                .as_mut()
                .unwrap()
                .sha256 = "0".repeat(64);
            assert!(validate_actual_behaviors_v1(
                &changed,
                &learner_behavior,
                opponent_behavior.as_ref()
            )
            .is_err());
            changed = trajectory.clone();
            changed.decisions[other].tensor.state[0] = f32::NAN.to_bits();
            assert!(replay_learner_groups_v1(&changed, &learner, Some(&opponent)).is_err());
            changed = trajectory.clone();
            changed.decisions[other].value ^= 1;
            assert!(replay_learner_groups_v1(&changed, &learner, Some(&opponent)).is_err());
            changed = trajectory.clone();
            let row = &mut changed.decisions[other];
            row.selected = (row.selected + 1) % row.logits.len() as u32;
            assert!(validate_trajectory(&changed).is_err());
        }
    }

    #[test]
    fn population_gradient_ignores_opponent_rows_and_updated_learner_rejects_stale_data() {
        for learner_seat in [0, 1] {
            let mut learner = FrozenPlayPolicyV1::training_fixture_v3();
            let mut opponent = distinct_opponent();
            let (short, _, _) =
                replay_fixture(&mut learner, Some(&mut opponent), learner_seat, &[0, 1]);
            let actors = if learner_seat == 0 {
                vec![0, 1, 1]
            } else {
                vec![0, 0, 1]
            };
            let (long, learner_behavior, opponent_behavior) =
                replay_fixture(&mut learner, Some(&mut opponent), learner_seat, &actors);
            let opponent_before = opponent.actual_model_identity_v1();
            let mut updated_hashes = Vec::new();
            for trajectory in [&short, &long] {
                let captured =
                    replay_learner_groups_v1(trajectory, &learner, Some(&opponent)).unwrap();
                assert_eq!(captured.len(), 1);
                let steps: Vec<_> = captured[0]
                    .1
                    .iter()
                    .map(|(row, tensor)| NativePolicySubstepV1 {
                        forward: NativePolicyForwardInputV1::Encoded(Box::new(
                            encoded_decision_view_v3(tensor),
                        )),
                        selected_action_index: row.selected as usize,
                        expected_raw_action_logit_bits: &row.logits,
                        expected_value_bits: row.value,
                    })
                    .collect();
                let groups = [NativePolicyPhysicalDecisionV1 {
                    substeps: &steps,
                    terminal_return: captured[0].0,
                    baseline_bits: 0,
                }];
                let mut state = test_state(&learner);
                state
                    .train_step_feature_transfer_v3(&groups, 0.5, 0.001)
                    .unwrap();
                let after = hex(&state.state_sha256_v1().unwrap());
                assert_ne!(after, learner_behavior.identity.state_sha256);
                let mut updated = learner_behavior.clone();
                updated.identity.state_sha256 = after.clone();
                assert_eq!(
                    validate_actual_behaviors_v1(trajectory, &updated, opponent_behavior.as_ref())
                        .unwrap_err(),
                    "stale trajectory: behavior optimizer/model state differs"
                );
                updated_hashes.push(after);
            }
            assert_eq!(updated_hashes[0], updated_hashes[1]);
            assert_eq!(opponent.actual_model_identity_v1(), opponent_before);
        }
    }

    #[test]
    fn legacy_self_play_defaults_omit_new_fields_and_keep_both_seat_streams() {
        let mut learner = FrozenPlayPolicyV1::training_fixture_v3();
        let (trajectory, behavior, _) = replay_fixture(&mut learner, None, 1, &[0, 1, 1, 0]);
        let value = serde_json::to_value(&trajectory).unwrap();
        assert!(value.get("seat_behaviors").is_none());
        assert!(value["episode"].get("opponent").is_none());
        assert_eq!(value["schema"], TRAJECTORY_SCHEMA);
        let decoded: ExpandedTrajectoryV1 = serde_json::from_value(value.clone()).unwrap();
        assert!(decoded.episode.opponent.is_none());
        assert!(decoded.seat_behaviors.is_none());
        assert_eq!(serde_json::to_value(&decoded).unwrap(), value);
        validate_actual_behaviors_v1(&decoded, &behavior, None).unwrap();
        assert_eq!(
            replay_learner_groups_v1(&decoded, &learner, None)
                .unwrap()
                .len(),
            2
        );
        let mut changed = decoded.clone();
        changed.schema = POPULATION_TRAJECTORY_SCHEMA.into();
        assert!(validate_trajectory(&changed).is_err());
        changed = decoded;
        changed.episode.opponent = Some(behavior.source);
        assert!(validate_trajectory(&changed).is_err());
    }

    fn checkpoint_fixture_v1() -> (
        FrozenPlayPolicyV1,
        NativePolicyValueNetV1,
        ExpandedCheckpointV1,
    ) {
        let policy = FrozenPlayPolicyV1::training_fixture_v3();
        let mut model =
            NativePolicyValueNetV1::runner_fixed_v1(NativePolicyValueModelConfigV1::contract_v1())
                .unwrap();
        model
            .replace_parameter_snapshot_v1(&policy.training_parameters_v3())
            .unwrap();
        let state = NativePolicyValueTrainStateV1::new_v1(model.clone()).unwrap();
        let mut snapshot = state.snapshot_v1().unwrap();
        snapshot
            .parameters
            .iter_mut()
            .find(|p| p.name == "card_embedding.weight")
            .unwrap()
            .values[16] = 0.4375;
        let saved = ExpandedCheckpointV1 {
            schema: CHECKPOINT_SCHEMA.into(),
            feature_contract_digest: FEATURE_CONTRACT_DIGEST_V3.into(),
            feature_encoding_digest: FEATURE_ENCODING_DIGEST_V3.into(),
            card_db_hash: format!("{KERNEL_CARDDB_HASH:016x}"),
            source_import: policy.identity_v1().clone(),
            state_sha256: hex(&snapshot.state_sha256_v1().unwrap()),
            adam_step: snapshot.adam_step,
            scorer_bias_anchor_bits: snapshot.scorer_bias_anchor_bits,
            parameters: snapshot
                .parameters
                .iter()
                .map(ParameterBitsV1::from_native)
                .collect(),
            first_moments: snapshot
                .first_moments
                .iter()
                .map(ParameterBitsV1::from_native)
                .collect(),
            second_moments: snapshot
                .second_moments
                .iter()
                .map(ParameterBitsV1::from_native)
                .collect(),
            trajectories: Vec::new(),
            loss_identity: "terminal_reinforce_value/v3".into(),
            learning_rate_bits: 0.001_f32.to_bits(),
            value_coefficient_bits: 0.5_f32.to_bits(),
        };
        (policy, model, saved)
    }

    #[test]
    fn inference_restore_preserves_exact_successor_parameters_embeddings_and_ancestry() {
        let (mut policy, model, saved) = checkpoint_fixture_v1();
        let ancestry = policy.identity_v1().clone();
        let original = policy.actual_model_identity_v1();
        let restored = restore_checkpoint_state_v1(&saved, &mut policy, model).unwrap();
        let actual = policy.actual_model_identity_v1();
        assert_eq!(
            hex(&restored.state_sha256_v1().unwrap()),
            saved.state_sha256
        );
        assert_eq!(
            actual.model_parameter_sha256,
            restored.model_v1().parameter_manifest_sha256_v1()
        );
        assert_ne!(
            actual.model_parameter_sha256,
            original.model_parameter_sha256
        );
        assert_ne!(
            actual.embedding_table_sha256,
            original.embedding_table_sha256
        );
        assert_eq!(policy.identity_v1(), &ancestry);
        assert_eq!(
            bits(policy.embedding_rows_v1()),
            saved
                .parameters
                .iter()
                .find(|p| p.name == "card_embedding.weight")
                .unwrap()
                .values
        );
        let parameters = policy.training_parameters_v3();
        assert_eq!(parameters.len(), saved.parameters.len());
        for (actual, expected) in parameters.iter().zip(&saved.parameters) {
            assert_eq!(actual.name, expected.name);
            assert_eq!(actual.shape, expected.shape);
            assert_eq!(bits(&actual.values), expected.values);
        }
    }

    #[test]
    fn inference_restore_rejects_stale_features_malformed_state_and_ancestry() {
        let (mut policy, model, saved) = checkpoint_fixture_v1();
        let original = policy.actual_model_identity_v1();
        let ancestry = policy.identity_v1().clone();
        let cases: [(&str, fn(&mut ExpandedCheckpointV1)); 8] = [
            ("revision2", |s| {
                s.feature_contract_digest =
                    "527db1125fa760076c2e751bb70be74cab21aa4dcb7d558a0b804a597ef8adfd".into();
                s.feature_encoding_digest =
                    "3ccfd79fe8c4c3916043e931fcb251aed2c3cd8c9c13f31c1112c2af3b855feb".into();
            }),
            ("card database", |s| s.card_db_hash = "0".repeat(16)),
            ("ancestry", |s| {
                s.source_import.weights_sha256 = "0".repeat(64)
            }),
            ("loss", |s| s.loss_identity = "another-loss".into()),
            ("state digest", |s| s.state_sha256 = "0".repeat(64)),
            ("parameter layout", |s| s.parameters[0].shape[0] += 1),
            ("parameter nonfinite", |s| {
                s.parameters[0].values[16] = f32::NAN.to_bits()
            }),
            ("optimizer nonfinite", |s| {
                s.first_moments[0].values[16] = f32::NAN.to_bits()
            }),
        ];
        for (name, mutate) in cases {
            let mut changed = saved.clone();
            mutate(&mut changed);
            assert!(
                restore_checkpoint_state_v1(&changed, &mut policy, model.clone()).is_err(),
                "{name}"
            );
            assert_eq!(policy.actual_model_identity_v1(), original, "{name}");
            assert_eq!(policy.identity_v1(), &ancestry, "{name}");
        }
    }

    #[test]
    fn inference_entrypoint_uses_existing_pinned_reader_before_import_decode() {
        let path = std::env::temp_dir().join(format!(
            "expanded-inference-pin-{}-{}.json",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let bytes = b"{\"not_an_import\":true}";
        fs::write(&path, bytes).unwrap();
        let mut source = ExpandedModelSourceV1 {
            play_import: PinnedFileV1 {
                path: path.clone(),
                sha256: "0".repeat(64),
            },
            feature_transfer: FrozenPlayObservationTransferV3 {
                expected_feature_contract_digest: FEATURE_CONTRACT_DIGEST_V3.into(),
                expected_feature_encoding_digest: FEATURE_ENCODING_DIGEST_V3.into(),
            },
            checkpoint: None,
        };
        let error = match load_expanded_inference_v1(&source) {
            Ok(_) => panic!("mismatched source pin accepted"),
            Err(error) => error,
        };
        assert_eq!(error, "input file SHA differs");
        source.play_import.sha256 = sha(bytes);
        assert!(load_expanded_inference_v1(&source).is_err());
        fs::remove_file(path).unwrap();
    }

    fn list(id: &str) -> ExpandedDeckListV1 {
        let d = checked_in_pauper_registered_deck_by_id_v1(id).unwrap();
        let c = d.registered_configuration();
        ExpandedDeckListV1 {
            label: id.into(),
            mainboard: c.mainboard().to_vec(),
            sideboard: c.sideboard().to_vec(),
        }
    }
    #[test]
    fn explicit_configuration_conserves_75_and_accepts_uncatalogued_label() {
        let mut deck = list("Affinity");
        deck.label = "candidate-brew".into();
        let mut e = ExpandedEpisodeV1 {
            id: "test".into(),
            seed: 1,
            starting_player: 0,
            learner_seat: 0,
            opponent: None,
            registered: [deck.clone(), list("Terror")],
            selected: [deck, list("Terror")],
            postboard: false,
            max_physical_decisions: 100,
            max_policy_steps: 1000,
        };
        assert!(e.configurations().is_ok());
        let swap = e.selected[0].sideboard[0];
        e.selected[0].sideboard[0] = e.selected[0].mainboard[0];
        e.selected[0].mainboard[0] = swap;
        assert!(e.configurations().is_err());
        e.postboard = true;
        assert!(e.configurations().is_ok());
        e.selected[0].mainboard.pop();
        assert!(e.configurations().is_err());
    }
    #[test]
    fn frozen_feature_identity_cannot_be_relabelled_as_successor() {
        assert!(identity_valid(
            crate::native_policy_value_net_v1::FEATURE_CONTRACT_DIGEST_V1,
            FEATURE_ENCODING_DIGEST_V3,
            &format!("{KERNEL_CARDDB_HASH:016x}")
        )
        .is_err());
    }
    #[test]
    fn persisted_tensor_preserves_every_bit() {
        let mut t = NativeFlatDecisionTensorV3::default();
        t.common.state = vec![0.0, -0.0, 1.2345678, f32::MIN_POSITIVE];
        t.common.object_card_ids = vec![65536];
        let bytes = serde_json::to_vec(&TensorBitsV1::from_tensor(&t)).unwrap();
        let saved: TensorBitsV1 = serde_json::from_slice(&bytes).unwrap();
        let restored = saved.tensor();
        assert_eq!(bits(&t.common.state), bits(&restored.common.state));
        assert_eq!(t.common.object_card_ids, restored.common.object_card_ids);
    }
}
