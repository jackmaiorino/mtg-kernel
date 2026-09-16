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
    NativeFlatDecisionTensorV3, FEATURE_CONTRACT_DIGEST_V3, FEATURE_ENCODING_DIGEST_V3,
};
#[cfg(test)]
use crate::native_flat_tensorizer_v3::{FEATURES_SOURCE_SHA256_V3, FEATURE_DESCRIPTOR_SHA256_V3};
use crate::native_flat_tensorizer_v4::{
    NativeFlatDecisionTensorV4, FEATURE_CONTRACT_DIGEST_V4, FEATURE_ENCODING_DIGEST_V4,
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
    FreshLineageGenerationV1, FrozenPlayObservationTransferV3, FrozenPlayPolicyImportV1,
    FrozenPlayPolicyV1, PlayModelIdentityV1, PlayPolicyOriginV1,
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
const FRESH_TRAJECTORY_SCHEMA: &str = "mtg-kernel-expanded-deck-trajectory/v3";
const CHECKPOINT_SCHEMA: &str = "mtg-kernel-expanded-deck-checkpoint/v1";
const FRESH_CHECKPOINT_SCHEMA: &str = "mtg-kernel-expanded-deck-fresh-checkpoint/v1";
const MAX_FILE_BYTES: u64 = 512 * 1024 * 1024;
const MAX_BATCH_BYTES: u64 = 512 * 1024 * 1024;

mod phase1_parallel_collection;
pub(crate) use phase1_parallel_collection::validate_collection_workers_v1;
mod ordered_update_preparation;
pub(crate) use ordered_update_preparation::validate_preparation_workers_v1;
mod fresh_initialization_source;
mod fresh_registry_transfer_source;
mod registry_transfer_source;
pub use fresh_initialization_source::ExpandedFreshInitializationSourceV1;
pub use registry_transfer_source::{
    ExpandedRegistryTransferScheduleV1, ExpandedRegistryTransferSourceV1,
};

/// Which registry-transfer family produced a `TransferTrainingContextV1`.
/// Both families reuse the exact same context type (imported and fresh
/// registry-transfer sources share resolved-schedule/scalar/cursor logic
/// verbatim); only this tag distinguishes which successor checkpoint schema
/// and origin family a resumed lineage belongs to. Discrimination for the
/// three-way checkpoint schema selection is on this actual transfer context
/// kind, not on a separately threaded flag.
enum TransferContextV1 {
    Imported(registry_transfer_source::TransferTrainingContextV1),
    Fresh(registry_transfer_source::TransferTrainingContextV1),
}

impl TransferContextV1 {
    fn is_fresh_v1(&self) -> bool {
        matches!(self, Self::Fresh(_))
    }
    fn inner(&self) -> &registry_transfer_source::TransferTrainingContextV1 {
        match self {
            Self::Imported(context) | Self::Fresh(context) => context,
        }
    }
    fn validate_scalars(&self, learning_rate: f32, value_coefficient: f32) -> Result<(), String> {
        self.inner().validate_scalars(learning_rate, value_coefficient)
    }
    fn validate_batch(&self, episodes: &[ExpandedEpisodeV1]) -> Result<(), String> {
        self.inner().validate_batch(episodes)
    }
    fn after_update(
        &self,
        adam_step: u64,
    ) -> Result<registry_transfer_source::ExpandedRegistryContinuationV1, String> {
        self.inner().after_update(adam_step)
    }
}

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

/// Selects the backward-pass numerical execution for the ordinary trainer's
/// update (`execute_update_v1`, both `Update` and `UpdatePrepared`) and for
/// the BO3 weighted update (`phase1_bo3_learning_v1::continuation::apply_prepared`).
/// `FixedPartition4` is a config-driven option for the fresh V4 lineage ONLY:
/// `execute_update_v1` rejects it outright for a V3-generation policy with a
/// clear error, so the V3 and imported paths are never reachable with
/// anything but `Sequential` and stay byte-identical. Production always uses
/// `native_policy_train_step_v1::FIXED_BACKWARD_PARTITION_COUNT_V1` (4)
/// workers when this is selected; see `fixed_partition_backward_worker_limit_v1`
/// for the `#[cfg(test)]`-only override that lets tests exercise 1..=4
/// workers through this same real entry point.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpdateBackwardExecutionV1 {
    #[default]
    Sequential,
    #[serde(rename = "fixed_partition_4")]
    FixedPartition4,
}

impl UpdateBackwardExecutionV1 {
    pub(crate) fn is_sequential(&self) -> bool {
        matches!(self, Self::Sequential)
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
    /// Ordinary FrozenPlayPolicyImportV1, or the explicitly versioned
    /// ExpandedRegistryTransferSourceV1. The outer legacy wire shape is fixed.
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
    pub source_import: PlayPolicyOriginV1,
    pub checkpoint_sha256: Option<String>,
    pub model: PlayModelIdentityV1,
    pub state_sha256: String,
    pub adam_step: u64,
    pub feature_schema_version: String,
    pub feature_registry_version: String,
    pub features_source_sha256: String,
    pub feature_descriptor_sha256: String,
}

impl ExpandedInferenceIdentityV1 {
    pub fn has_supported_origin_schema_v1(&self) -> bool {
        self.schema == inference_schema_v1(&self.source_import)
    }
}

fn inference_schema_v1(origin: &PlayPolicyOriginV1) -> &'static str {
    if origin.is_fresh_v1() {
        "mtg-kernel-expanded-deck-inference/v2"
    } else {
        "mtg-kernel-expanded-deck-inference/v1"
    }
}

fn ordinary_checkpoint_schema_v1(origin: &PlayPolicyOriginV1) -> &'static str {
    if origin.is_fresh_v1() {
        FRESH_CHECKPOINT_SCHEMA
    } else {
        CHECKPOINT_SCHEMA
    }
}

/// Reuses the bounded, pinned continuation reader and its exact feature,
/// ancestry, parameter, optimizer and state checks. No artifacts are written,
/// no training runs, and predecessor feature identities are not migrated.
pub fn load_expanded_inference_v1(
    source: &ExpandedModelSourceV1,
) -> Result<(FrozenPlayPolicyV1, ExpandedInferenceIdentityV1), String> {
    // Inference alone admits the explicit BO3 objective. Legacy initialization
    // and update dispatch keep rejecting its descriptor/checkpoint schemas.
    let bytes = read_pinned_bytes(&source.play_import)?;
    let probe: Value = serde_json::from_slice(&bytes).map_err(err)?;
    if probe.get("schema").and_then(Value::as_str)
        == Some(crate::phase1_bo3_learning_v1::BO3_GAMEPLAY_SOURCE_SCHEMA_V1)
    {
        return crate::phase1_bo3_learning_v1::load_bo3_inference_v1(source);
    }
    let (policy, state) = initialize(source)?;
    let receipt = inference_identity_v1(source, &policy, &state)?;
    Ok((policy, receipt))
}

pub(crate) fn inference_identity_v1(
    source: &ExpandedModelSourceV1,
    policy: &FrozenPlayPolicyV1,
    state: &NativePolicyValueTrainStateV1,
) -> Result<ExpandedInferenceIdentityV1, String> {
    // Single accessor, not a hand-picked constant: a fresh-V4 policy must
    // stamp the V4 schema/registry/source/descriptor identity here, never
    // the V3 one, and this is the one place that decides which.
    let feature_identity = policy.feature_identity_v1();
    Ok(ExpandedInferenceIdentityV1 {
        schema: inference_schema_v1(policy.identity_v1()).into(),
        source_import: policy.identity_v1().clone(),
        checkpoint_sha256: source.checkpoint.as_ref().map(|pin| pin.sha256.clone()),
        model: policy.actual_model_identity_v1(),
        state_sha256: hex(&state.state_sha256_v1().map_err(err)?),
        adam_step: state.adam_step_v1(),
        feature_schema_version: feature_identity.feature_schema_version.into(),
        feature_registry_version: feature_identity.feature_registry_version.into(),
        features_source_sha256: feature_identity.features_source_sha256.into(),
        feature_descriptor_sha256: feature_identity.feature_descriptor_sha256.into(),
    })
}

/// Narrow objective-transition seam. An existing game-terminal checkpoint
/// is mandatory; registry/BO3 sources and fresh Adam resets are not admitted.
pub(crate) fn load_ordinary_bo3_parent_v1(
    source: &ExpandedModelSourceV1,
) -> Result<
    (
        FrozenPlayPolicyV1,
        NativePolicyValueTrainStateV1,
        ExpandedInferenceIdentityV1,
        u32,
        u32,
    ),
    String,
> {
    let pin = source
        .checkpoint
        .as_ref()
        .ok_or("BO3 transition requires an existing ordinary checkpoint")?;
    validate_ordinary_source_descriptor_v1(source)?;
    let bytes = read_pinned_bytes(&source.play_import)?;
    let mut policy = load_ordinary_policy_v1(source, &bytes)?;
    let mut model =
        NativePolicyValueNetV1::runner_fixed_v1(NativePolicyValueModelConfigV1::contract_v1())
            .map_err(err)?;
    model
        .replace_parameter_snapshot_v1(&policy.training_parameters_v3())
        .map_err(err)?;
    let saved = read_ordinary_checkpoint_v1(pin, policy.identity_v1())?;
    for bits in [saved.learning_rate_bits, saved.value_coefficient_bits] {
        let value = f32::from_bits(bits);
        ensure(
            value.is_finite() && value > 0.0,
            "ordinary checkpoint scalar is not positive finite",
        )?;
    }
    let state = restore_checkpoint_state_v1(&saved, &mut policy, model)?;
    let identity = inference_identity_v1(source, &policy, &state)?;
    Ok((
        policy,
        state,
        identity,
        saved.learning_rate_bits,
        saved.value_coefficient_bits,
    ))
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
    /// Generic over the shared inner tensor, not `NativeFlatDecisionTensorV3`
    /// specifically: `NativeFlatDecisionTensorV3` and `NativeFlatDecisionTensorV4`
    /// are both byte-identical `{ common: NativeFlatDecisionTensorV2 }`
    /// wrappers, so a caller passes `&tensor.common` regardless of which
    /// generation produced `tensor`, and this type never needs a V4 sibling.
    fn from_tensor(t: &NativeFlatDecisionTensorV2) -> Self {
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
    fn tensor(&self) -> NativeFlatDecisionTensorV2 {
        NativeFlatDecisionTensorV2 {
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
    source_import: PlayPolicyOriginV1,
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
    source_import: PlayPolicyOriginV1,
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
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "registry_transfer_source::deserialize_present_continuation"
    )]
    registry_transfer: Option<registry_transfer_source::ExpandedRegistryContinuationV1>,
}

/// Whole-pair V3-or-V4 admission: a stored trajectory/checkpoint's
/// `(feature_contract_digest, feature_encoding_digest)` pair must match
/// exactly one compiled generation's pair, never a mixed pair (a V3
/// contract digest with a V4 encoding digest, or vice versa) accepted by
/// independently checking each field against "V3 or V4".
fn identity_valid(contract: &str, encoding: &str, cards: &str) -> Result<(), String> {
    ensure(
        (contract == FEATURE_CONTRACT_DIGEST_V3 && encoding == FEATURE_ENCODING_DIGEST_V3)
            || (contract == FEATURE_CONTRACT_DIGEST_V4 && encoding == FEATURE_ENCODING_DIGEST_V4),
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
    let (policy, state, _) = initialize_with_transfer_context(source)?;
    Ok((policy, state))
}

fn initialize_with_transfer_context(
    source: &ExpandedModelSourceV1,
) -> Result<
    (
        FrozenPlayPolicyV1,
        NativePolicyValueTrainStateV1,
        Option<TransferContextV1>,
    ),
    String,
> {
    let bytes = read_pinned_bytes(&source.play_import)?;
    let probe: Value = serde_json::from_slice(&bytes).map_err(err)?;
    if probe.get("schema").and_then(Value::as_str) == Some(registry_transfer_source::SOURCE_SCHEMA)
    {
        let (policy, state, context) = registry_transfer_source::initialize(source, &bytes)?;
        return Ok((policy, state, context.map(TransferContextV1::Imported)));
    }
    if probe.get("schema").and_then(Value::as_str)
        == Some(fresh_registry_transfer_source::SOURCE_SCHEMA)
    {
        let (policy, state, context) = fresh_registry_transfer_source::initialize(source, &bytes)?;
        return Ok((policy, state, context.map(TransferContextV1::Fresh)));
    }
    let mut policy = load_ordinary_policy_v1(source, &bytes)?;
    let mut model =
        NativePolicyValueNetV1::runner_fixed_v1(NativePolicyValueModelConfigV1::contract_v1())
            .map_err(err)?;
    model
        .replace_parameter_snapshot_v1(&policy.training_parameters_v3())
        .map_err(err)?;
    let state = if let Some(pin) = &source.checkpoint {
        let saved = read_ordinary_checkpoint_v1(pin, policy.identity_v1())?;
        restore_checkpoint_state_v1(&saved, &mut policy, model)?
    } else {
        NativePolicyValueTrainStateV1::new_v1(model).map_err(err)?
    };
    Ok((policy, state, None))
}

/// Syntax admission only. Actual checkpoint/parameter/Adam binding is checked
/// by the ordinary parent loader. No general inference dispatch is used here.
pub(crate) fn validate_ordinary_source_descriptor_v1(
    source: &ExpandedModelSourceV1,
) -> Result<(), String> {
    ensure(
        source.checkpoint.is_some(),
        "ordinary parent checkpoint is required",
    )?;
    let bytes = read_pinned_bytes(&source.play_import)?;
    let probe: Value = serde_json::from_slice(&bytes).map_err(err)?;
    if probe.get("schema").and_then(Value::as_str)
        == Some(fresh_initialization_source::SOURCE_SCHEMA)
    {
        fresh_initialization_source::parse_source_v1(&bytes)?;
    } else {
        // Decode original bytes. Malformed fresh descriptors cannot fall back
        // because schema/initialization fields are unknown to the old parser.
        let _: FrozenPlayPolicyImportV1 = serde_json::from_slice(&bytes).map_err(err)?;
    }
    Ok(())
}

fn load_ordinary_policy_v1(
    source: &ExpandedModelSourceV1,
    bytes: &[u8],
) -> Result<FrozenPlayPolicyV1, String> {
    let probe: Value = serde_json::from_slice(bytes).map_err(err)?;
    if probe.get("schema").and_then(Value::as_str)
        == Some(fresh_initialization_source::SOURCE_SCHEMA)
    {
        let descriptor = fresh_initialization_source::parse_source_v1(bytes)?;
        return fresh_initialization_source::load_policy_v1(&descriptor, &source.feature_transfer);
    }
    let import: FrozenPlayPolicyImportV1 = serde_json::from_slice(bytes).map_err(err)?;
    FrozenPlayPolicyV1::load_feature_transfer_v3(&import, &source.feature_transfer)
}

fn read_ordinary_checkpoint_v1(
    pin: &PinnedFileV1,
    origin: &PlayPolicyOriginV1,
) -> Result<ExpandedCheckpointV1, String> {
    let saved = if origin.is_fresh_v1() {
        read_pinned(pin)?
    } else {
        read_legacy_checkpoint_v1(pin)?
    };
    ensure(
        saved.registry_transfer.is_none()
            && saved.schema == ordinary_checkpoint_schema_v1(origin)
            && saved.source_import.is_fresh_v1() == origin.is_fresh_v1(),
        "ordinary checkpoint schema/origin differs",
    )?;
    Ok(saved)
}

fn read_legacy_checkpoint_v1(pin: &PinnedFileV1) -> Result<ExpandedCheckpointV1, String> {
    let saved: ExpandedCheckpointV1 = read_pinned(pin)?;
    // The ordinary checkpoint reader previously rejected this unknown field,
    // including an explicit null. Preserve that strict wire boundary.
    ensure(
        saved.registry_transfer.is_none() && !saved.source_import.is_fresh_v1(),
        "ordinary checkpoint does not admit registry-transfer metadata",
    )?;
    Ok(saved)
}

/// Load and re-export an explicitly fresh origin, without collecting a game
/// or taking an optimizer step. The state file is an inspection artifact,
/// not an ordinary checkpoint or a Store export.
pub fn inspect_fresh_initialization_json_v1(text: &str) -> Result<Value, String> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Request {
        schema: String,
        source: ExpandedModelSourceV1,
        output_directory: PathBuf,
    }
    ensure(
        text.len() <= 1024 * 1024,
        "inspection request exceeds 1 MiB",
    )?;
    crate::rl::parse_strict_json_value(text).map_err(err)?;
    let request: Request = serde_json::from_str(text).map_err(err)?;
    ensure(
        request.schema == "phase1-fresh-initialization-inspection-request/v1",
        "unknown inspection schema",
    )?;
    inspect_fresh_initialization_v1(&request.source, &request.output_directory)
}

pub fn inspect_fresh_initialization_v1(
    source: &ExpandedModelSourceV1,
    output_directory: &Path,
) -> Result<Value, String> {
    ensure(
        source.checkpoint.is_none(),
        "fresh inspection cannot restore a checkpoint",
    )?;
    let bytes = read_pinned_bytes(&source.play_import)?;
    let descriptor = fresh_initialization_source::parse_source_v1(&bytes)?;
    let policy =
        fresh_initialization_source::load_policy_v1(&descriptor, &source.feature_transfer)?;
    let mut model =
        NativePolicyValueNetV1::runner_fixed_v1(NativePolicyValueModelConfigV1::contract_v1())
            .map_err(err)?;
    model
        .replace_parameter_snapshot_v1(&policy.training_parameters_v3())
        .map_err(err)?;
    let state = NativePolicyValueTrainStateV1::new_v1(model).map_err(err)?;
    let snapshot = state.snapshot_v1().map_err(err)?;
    ensure(
        snapshot.adam_step == 0
            && snapshot
                .first_moments
                .iter()
                .chain(&snapshot.second_moments)
                .all(|row| row.values.iter().all(|v| v.to_bits() == 0)),
        "fresh optimizer bootstrap differs",
    )?;
    let mut raw = Vec::new();
    for row in &snapshot.parameters {
        for value in &row.values {
            raw.extend_from_slice(&value.to_bits().to_le_bytes());
        }
    }
    ensure(
        sha(&raw) == policy.actual_model_identity_v1().weights_sha256,
        "re-exported parameter bytes differ from installed policy",
    )?;
    let result = json!({
        "schema":"mtg-kernel-fresh-initialization-inspection/v1",
        "source":source,
        "identity":inference_identity_v1(source, &policy, &state)?,
        "parameters":snapshot.parameters.iter().map(ParameterBitsV1::from_native).collect::<Vec<_>>(),
        "first_moments":snapshot.first_moments.iter().map(ParameterBitsV1::from_native).collect::<Vec<_>>(),
        "second_moments":snapshot.second_moments.iter().map(ParameterBitsV1::from_native).collect::<Vec<_>>(),
        "adam_step":snapshot.adam_step,
        "scorer_bias_anchor_bits":snapshot.scorer_bias_anchor_bits,
        "state_sha256":hex(&snapshot.state_sha256_v1().map_err(err)?),
        "training_started":false,
        "strength_claim":false,
    });
    ensure(
        output_directory.is_absolute() && !output_directory.exists(),
        "fresh absolute inspection directory required",
    )?;
    fs::create_dir_all(output_directory).map_err(err)?;
    let parent = capture_existing_publication_parent_v1(output_directory).map_err(err)?;
    let expected = DurableFileExpectationV1::from_bytes(&raw).map_err(err)?;
    publish_new_file_v1(
        &parent,
        ".parameters.stage",
        "parameters.f32le",
        &raw,
        expected,
    )
    .map_err(err)?;
    let inspection = publish_json(output_directory, "inspection.json", &result)?;
    Ok(
        json!({"schema":"mtg-kernel-fresh-initialization-inspection-result/v1",
        "inspection":inspection,"parameter_sha256":sha(&raw),
        "adam_step":snapshot.adam_step,"state_sha256":hex(&snapshot.state_sha256_v1().map_err(err)?),
        "training_started":false}),
    )
}

fn restore_checkpoint_state_v1(
    saved: &ExpandedCheckpointV1,
    policy: &mut FrozenPlayPolicyV1,
    model: NativePolicyValueNetV1,
) -> Result<NativePolicyValueTrainStateV1, String> {
    ensure(
        saved.schema == ordinary_checkpoint_schema_v1(policy.identity_v1())
            && saved.registry_transfer.is_none(),
        "not an expanded-deck checkpoint",
    )?;
    restore_checkpoint_fields_v1(saved, policy, model)
}

fn restore_checkpoint_fields_v1(
    saved: &ExpandedCheckpointV1,
    policy: &mut FrozenPlayPolicyV1,
    model: NativePolicyValueNetV1,
) -> Result<NativePolicyValueTrainStateV1, String> {
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
    if let Some(other) = &opponent {
        // Strict whole-generation equality (never a flag): a trajectory
        // stamps one feature-contract identity from the learner alone
        // (below), so a learner/opponent generation mismatch would tensorize
        // some rows under a generation the trajectory never declares.
        ensure(
            policy.feature_identity_v1().generation
                == other.policy.feature_identity_v1().generation,
            "learner and opponent use different fresh-lineage feature generations",
        )?;
    }
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
                // Read the digests actually stamped by this loaded policy's
                // generation, never a hardcoded V3 constant: a fresh-V4
                // policy must produce a V4-labeled trajectory.
                let feature_identity = policy.feature_identity_v1();
                let result = ExpandedTrajectoryV1 {
                    schema: trajectory_schema_v1(policy.identity_v1(), seat_behaviors.as_ref())
                        .into(),
                    feature_contract_digest: feature_identity.feature_contract_digest.into(),
                    feature_encoding_digest: feature_identity.feature_encoding_digest.into(),
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
                // Dispatch on the acting policy's own generation: a V4
                // fresh-lineage policy scores and tensorizes through the V4
                // successor, never through the V3 one. `TensorBitsV1` itself
                // is generation-agnostic (both wrappers share the same inner
                // `NativeFlatDecisionTensorV2`), so only this selection point
                // differs.
                let (selected, scores, tensor_bits) =
                    if acting.feature_identity_v1().generation == FreshLineageGenerationV1::V4 {
                        let (selected, scores, tensor) =
                            acting.select_with_training_tensor_v4(&session)?;
                        (selected, scores, TensorBitsV1::from_tensor(&tensor.common))
                    } else {
                        let (selected, scores, tensor) =
                            acting.select_with_training_tensor_v3(&session)?;
                        (selected, scores, TensorBitsV1::from_tensor(&tensor.common))
                    };
                let record = DecisionRecordV1 {
                    step: d.step,
                    physical_decision_id: d.physical_decision_id,
                    substep_index: d.substep_index,
                    substep_count: d.substep_count,
                    actor: seat(d.acting_player),
                    selected,
                    logits: bits(&scores.logits),
                    value: scores.value.to_bits(),
                    tensor: tensor_bits,
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
    ensure(
        t.schema == trajectory_schema_v1(&t.source_import, t.seat_behaviors.as_ref()),
        "trajectory schema/origin differs",
    )?;
    match (&t.episode.opponent, &t.seat_behaviors) {
        (None, None) => Ok(()),
        (Some(opponent), Some(behaviors)) => {
            ensure(
                behaviors
                    .iter()
                    .all(|b| b.identity.has_supported_origin_schema_v1()),
                "behavior inference schema/origin differs",
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

fn trajectory_schema_v1(
    origin: &PlayPolicyOriginV1,
    behaviors: Option<&[ExpandedSeatBehaviorV1; 2]>,
) -> &'static str {
    if origin.is_fresh_v1()
        || behaviors.is_some_and(|rows| rows.iter().any(|b| b.identity.source_import.is_fresh_v1()))
    {
        FRESH_TRAJECTORY_SCHEMA
    } else if behaviors.is_some() {
        POPULATION_TRAJECTORY_SCHEMA
    } else {
        TRAJECTORY_SCHEMA
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

type LearnerTensorGroupV1<'a> = (i8, Vec<(&'a DecisionRecordV1, NativeFlatDecisionTensorV2)>);

/// The trajectory's own recorded whole-pair generation, never a caller flag.
/// `validate_trajectory` (called by every caller of this) already proved the
/// pair is a legitimate V3-or-V4 whole pair via `identity_valid`, so
/// anything that is not the V4 pair here is therefore the V3 pair.
fn trajectory_generation_v1(t: &ExpandedTrajectoryV1) -> FreshLineageGenerationV1 {
    if t.feature_contract_digest == FEATURE_CONTRACT_DIGEST_V4
        && t.feature_encoding_digest == FEATURE_ENCODING_DIGEST_V4
    {
        FreshLineageGenerationV1::V4
    } else {
        FreshLineageGenerationV1::V3
    }
}

/// A `NativeEncodedDecisionViewV1` over the shared common tensor, stamped
/// with exactly the compiled V3 or V4 schema for `generation`. Avoids
/// needing a `NativeFlatDecisionTensorV3`/`V4` wrapper (and the temporary
/// lifetime that would force) just to pick a schema: the wrapper and the
/// generic tensor share the identical field layout, so this reads directly
/// off the generic tensor's own fields.
pub(crate) fn encoded_decision_view_generic_v1(
    t: &NativeFlatDecisionTensorV2,
    generation: FreshLineageGenerationV1,
) -> crate::native_policy_value_net_v1::NativeEncodedDecisionViewV1<'_> {
    let schema = match generation {
        FreshLineageGenerationV1::V3 => crate::native_flat_tensorizer_v3::schema_v3(),
        FreshLineageGenerationV1::V4 => crate::native_flat_tensorizer_v4::schema_v4(),
    };
    crate::native_policy_value_net_v1::NativeEncodedDecisionViewV1::from_slices_unvalidated(
        schema,
        &t.state,
        &t.object_features,
        &t.object_card_ids,
        &t.object_groups,
        &t.object_node_ids,
        &t.edge_features,
        &t.edge_source_indices,
        &t.edge_target_indices,
        &t.action_features,
        &t.action_ref_features,
        &t.action_ref_card_ids,
        &t.action_ref_action_indices,
        &t.action_ref_node_indices,
    )
}

/// Validate every stored actor-visible forward before returning learner-only
/// physical decisions. Opponent tensors never enter the optimizer input.
/// Dispatches the replay/verification score call on the trajectory's own
/// recorded generation (`trajectory_generation_v1`), never a caller flag.
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
    let generation = trajectory_generation_v1(t);
    let mut groups = Vec::new();
    let mut index = 0;
    while index < t.decisions.len() {
        let first = &t.decisions[index];
        let count = first.substep_count as usize;
        let mut group = Vec::new();
        for row in &t.decisions[index..index + count] {
            let common = row.tensor.tensor();
            let acting = if row.actor == t.episode.learner_seat {
                learner
            } else {
                opponent.unwrap_or(learner)
            };
            let output = match generation {
                FreshLineageGenerationV1::V3 => acting.score_training_tensor_v3(
                    &NativeFlatDecisionTensorV3 {
                        common: common.clone(),
                    },
                )?,
                FreshLineageGenerationV1::V4 => acting.score_training_tensor_v4(
                    &NativeFlatDecisionTensorV4 {
                        common: common.clone(),
                    },
                )?,
            };
            ensure(
                bits(&output.logits) == row.logits && output.value.to_bits() == row.value,
                "stored tensor does not reproduce rollout outputs",
            )?;
            if row.actor == t.episode.learner_seat {
                group.push((row, common));
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
    /// Explicit execution-only successor. The legacy Collect command and its
    /// serialized shape remain unchanged. Trajectories join in schedule order.
    CollectParallel {
        source: ExpandedModelSourceV1,
        episodes: Vec<ExpandedEpisodeV1>,
        workers: usize,
        output_directory: PathBuf,
    },
    Update {
        source: ExpandedModelSourceV1,
        trajectories: Vec<PinnedFileV1>,
        learning_rate: f32,
        value_coefficient: f32,
        #[serde(default, skip_serializing_if = "ExpandedUpdateBackendV1::is_cpu")]
        update_backend: ExpandedUpdateBackendV1,
        /// Config-driven; default `Sequential` keeps every existing config and
        /// the V3 lineage byte-identical. See `UpdateBackwardExecutionV1`.
        #[serde(default, skip_serializing_if = "UpdateBackwardExecutionV1::is_sequential")]
        update_backward_execution: UpdateBackwardExecutionV1,
        output_directory: PathBuf,
    },
    /// Explicit execution-only preparation. The old Update wire shape and
    /// sequential reference remain unchanged; backward and Adam are identical.
    UpdatePrepared {
        source: ExpandedModelSourceV1,
        trajectories: Vec<PinnedFileV1>,
        learning_rate: f32,
        value_coefficient: f32,
        #[serde(default, skip_serializing_if = "ExpandedUpdateBackendV1::is_cpu")]
        update_backend: ExpandedUpdateBackendV1,
        #[serde(default, skip_serializing_if = "UpdateBackwardExecutionV1::is_sequential")]
        update_backward_execution: UpdateBackwardExecutionV1,
        preparation_workers: usize,
        output_directory: PathBuf,
    },
}

pub fn execute_v1(command: ExpandedTrainingCommandV1) -> Result<Value, String> {
    match command {
        ExpandedTrainingCommandV1::CollectParallel {
            source,
            episodes,
            workers,
            output_directory,
        } => phase1_parallel_collection::collect_parallel_v1(
            source,
            episodes,
            workers,
            output_directory,
        ),
        ExpandedTrainingCommandV1::Collect {
            source,
            episodes,
            output_directory,
        } => {
            let collection_started = std::time::Instant::now();
            ensure(
                !episodes.is_empty() && episodes.len() <= 1024,
                "invalid collection size",
            )?;
            let mut ids = BTreeSet::new();
            for episode in &episodes {
                episode.configurations()?;
                ensure(ids.insert(&episode.id), "duplicate episode id")?;
            }
            let (mut policy, state, transfer) = initialize_with_transfer_context(&source)?;
            if let Some(context) = &transfer {
                context.validate_batch(&episodes)?;
            }
            let state_hash = hex(&state.state_sha256_v1().map_err(err)?);
            let learner = ExpandedSeatBehaviorV1 {
                source: source.clone(),
                identity: inference_identity_v1(&source, &policy, &state)?,
            };
            drop(state);
            let mut opponent_cache = OpponentCacheV1::default();
            let initialization_seconds = collection_started.elapsed().as_secs_f64();
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
            let result = json!({"schema":"mtg-kernel-expanded-deck-collection/v1", "complete":true, "source": source, "behavior_state_sha256": state_hash, "trajectories": outputs,
                "collection_elapsed_seconds":collection_started.elapsed().as_secs_f64(),
                "collection_initialization_seconds":initialization_seconds});
            publish_json(&output_directory, "collection.json", &result)?;
            Ok(result)
        }
        ExpandedTrainingCommandV1::Update {
            source,
            trajectories,
            learning_rate,
            value_coefficient,
            update_backend,
            update_backward_execution,
            output_directory,
        } => execute_update_v1(
            source,
            trajectories,
            learning_rate,
            value_coefficient,
            update_backend,
            update_backward_execution,
            output_directory,
            None,
        ),
        ExpandedTrainingCommandV1::UpdatePrepared {
            source,
            trajectories,
            learning_rate,
            value_coefficient,
            update_backend,
            update_backward_execution,
            preparation_workers,
            output_directory,
        } => {
            validate_preparation_workers_v1(preparation_workers)?;
            execute_update_v1(
                source,
                trajectories,
                learning_rate,
                value_coefficient,
                update_backend,
                update_backward_execution,
                output_directory,
                Some(preparation_workers),
            )
        }
    }
}

// Per-thread-only test override for `fixed_partition_backward_worker_limit_v1`
// below; see its doc comment.
#[cfg(test)]
thread_local! {
    static BACKWARD_WORKER_LIMIT_OVERRIDE_FOR_TEST_V1: std::cell::Cell<Option<usize>> =
        const { std::cell::Cell::new(None) };
}

#[cfg(test)]
pub(crate) fn set_fixed_partition_backward_worker_limit_override_for_test_v1(
    worker_limit: Option<usize>,
) {
    BACKWARD_WORKER_LIMIT_OVERRIDE_FOR_TEST_V1.with(|cell| cell.set(worker_limit));
}

/// Worker limit `execute_update_v1` uses for `UpdateBackwardExecutionV1::
/// FixedPartition4`. Production always resolves to
/// `FIXED_BACKWARD_PARTITION_COUNT_V1` (4, "up to 4" workers on the 4 frozen
/// logical partitions). Tests may override this on their own thread only, to
/// drive the real two-iteration ordinary-trainer fixture at worker limits
/// 1..=4 through this exact production entry point and prove topology
/// invariance on real data, not just the synthetic-fixture unit test in
/// `native_policy_train_step_v1`.
fn fixed_partition_backward_worker_limit_v1() -> usize {
    #[cfg(test)]
    {
        if let Some(worker_limit) =
            BACKWARD_WORKER_LIMIT_OVERRIDE_FOR_TEST_V1.with(std::cell::Cell::get)
        {
            return worker_limit;
        }
    }
    crate::native_policy_train_step_v1::FIXED_BACKWARD_PARTITION_COUNT_V1
}

fn execute_update_v1(
    source: ExpandedModelSourceV1,
    trajectories: Vec<PinnedFileV1>,
    learning_rate: f32,
    value_coefficient: f32,
    update_backend: ExpandedUpdateBackendV1,
    backward_execution: UpdateBackwardExecutionV1,
    output_directory: PathBuf,
    preparation_workers: Option<usize>,
) -> Result<Value, String> {
    let update_started = std::time::Instant::now();
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
    let (policy, mut state, transfer) = initialize_with_transfer_context(&source)?;
    if let Some(context) = &transfer {
        context.validate_scalars(learning_rate, value_coefficient)?;
    }
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
    if let Some(context) = &transfer {
        let batch: Vec<_> = episodes
            .iter()
            .map(|trajectory| trajectory.episode.clone())
            .collect();
        context.validate_batch(&batch)?;
    }
    let input_read_seconds = update_started.elapsed().as_secs_f64();
    let replay_started = std::time::Instant::now();
    // Recompute all actor-visible rows, including opponent decisions,
    // before constructing learner groups. No private state is decoded.
    let (tensor_groups, preparation_telemetry) = if let Some(workers) = preparation_workers {
        let prepared =
            ordered_update_preparation::prepare_v1(&episodes, &policy, &learner, workers)?;
        (prepared.groups, Some(prepared.telemetry))
    } else {
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
        (tensor_groups, None)
    };
    ensure(!tensor_groups.is_empty(), "no learner decisions")?;
    // One dispatch for the whole update: every trajectory in this batch was
    // already required (above) to share the live policy's own source_import
    // ancestry, which for a fresh origin carries the feature-contract digest
    // pair itself, so they also share its generation transitively. Read from
    // the loaded policy through the same accessor every other stamping site
    // uses, never a hardcoded V3 constant on a fresh-V4 policy.
    let generation = policy.feature_identity_v1().generation;
    // Config-driven fixed-partition backward is a fresh-V4-lineage-only
    // option (`UpdateBackwardExecutionV1`). Any other combination is
    // rejected here, before any learner tensor is built, so a V3-generation
    // or CUDA-backend update can never reach anything but `Sequential`: the
    // V3 and imported paths stay byte-identical by construction, not by
    // convention.
    if !backward_execution.is_sequential() {
        ensure(
            matches!(update_backend, ExpandedUpdateBackendV1::Cpu),
            "fixed-partition backward requires the CPU update backend",
        )?;
        ensure(
            generation == FreshLineageGenerationV1::V4,
            "fixed-partition backward is available only to the V4 fresh lineage; \
             V3 and imported paths must use sequential",
        )?;
    }
    let substeps: Vec<Vec<NativePolicySubstepV1<'_>>> = tensor_groups
        .iter()
        .map(|(_, group)| {
            group
                .iter()
                .map(|(row, t)| NativePolicySubstepV1 {
                    forward: NativePolicyForwardInputV1::Encoded(Box::new(
                        encoded_decision_view_generic_v1(t, generation),
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
    let behavior_replay_seconds = replay_started.elapsed().as_secs_f64();
    let learner_started = std::time::Instant::now();
    let update = match (update_backend, generation) {
        (ExpandedUpdateBackendV1::Cpu, FreshLineageGenerationV1::V3) => state
            .train_step_feature_transfer_v3(&groups, value_coefficient, learning_rate)
            .map_err(err)?,
        (ExpandedUpdateBackendV1::Cpu, FreshLineageGenerationV1::V4) => match backward_execution {
            UpdateBackwardExecutionV1::Sequential => state
                .train_step_feature_transfer_v4(&groups, value_coefficient, learning_rate)
                .map_err(err)?,
            UpdateBackwardExecutionV1::FixedPartition4 => state
                .train_step_feature_transfer_v4_fixed_partition_v1(
                    &groups,
                    value_coefficient,
                    learning_rate,
                    fixed_partition_backward_worker_limit_v1(),
                )
                .map_err(err)?,
        },
        (ExpandedUpdateBackendV1::Cuda { device_ordinal }, FreshLineageGenerationV1::V3) => {
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
        (ExpandedUpdateBackendV1::Cuda { .. }, FreshLineageGenerationV1::V4) => {
            return Err("CUDA update backend has no V4 arm yet".into());
        }
    };
    let learner_update_seconds = learner_started.elapsed().as_secs_f64();
    let checkpoint_started = std::time::Instant::now();
    let snapshot = state.snapshot_v1().map_err(err)?;
    let after = hex(&snapshot.state_sha256_v1().map_err(err)?);
    let checkpoint = ExpandedCheckpointV1 {
        schema: match &transfer {
            Some(context) if context.is_fresh_v1() => {
                fresh_registry_transfer_source::CHECKPOINT_SCHEMA_TRANSFER
            }
            Some(_) => registry_transfer_source::CHECKPOINT_SCHEMA_TRANSFER,
            None => ordinary_checkpoint_schema_v1(policy.identity_v1()),
        }
        .into(),
        // Read from the loaded policy's own generation, never a hardcoded
        // V3 constant on a fresh-V4 policy (the single-accessor mitigation).
        // The update arithmetic above now dispatches on this same
        // `generation` value, so this label matches what actually ran.
        feature_contract_digest: policy.feature_identity_v1().feature_contract_digest.into(),
        feature_encoding_digest: policy.feature_identity_v1().feature_encoding_digest.into(),
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
        registry_transfer: transfer
            .as_ref()
            .map(|context| context.after_update(snapshot.adam_step))
            .transpose()?,
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
    if let Some(telemetry) = preparation_telemetry {
        result["update_preparation"] = serde_json::to_value(telemetry).map_err(err)?;
    }
    result["input_read_seconds"] = json!(input_read_seconds);
    result["behavior_replay_seconds"] = json!(behavior_replay_seconds);
    result["learner_update_seconds"] = json!(learner_update_seconds);
    result["checkpoint_io_seconds"] = json!(checkpoint_started.elapsed().as_secs_f64());
    result["update_elapsed_seconds"] = json!(update_started.elapsed().as_secs_f64());
    // Timings include checkpoint readback, but not this final receipt's
    // publication. They never enter checkpoint parameters or state hashes.
    publish_json(&output_directory, "update.json", &result)?;
    Ok(result)
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
    let bytes = read_pinned_bytes(pin)?;
    serde_json::from_slice(&bytes).map_err(err)
}

fn read_pinned_bytes(pin: &PinnedFileV1) -> Result<Vec<u8>, String> {
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
    Ok(bytes)
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
    pub(super) fn replay_fixture(
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
                tensor: TensorBitsV1::from_tensor(&tensor.common),
                sampler_identity: decision_sampler_identity_v1(scores.logits.len())
                    .map(str::to_owned),
            });
        }
        let configs = episode.configurations().unwrap();
        let seat_behaviors = opponent_behavior.as_ref().map(|other| {
            std::array::from_fn(|actor| {
                if actor == learner_seat as usize {
                    learner_behavior.clone()
                } else {
                    other.clone()
                }
            })
        });
        let trajectory = ExpandedTrajectoryV1 {
            schema: trajectory_schema_v1(learner.identity_v1(), seat_behaviors.as_ref()).into(),
            feature_contract_digest: FEATURE_CONTRACT_DIGEST_V3.into(),
            feature_encoding_digest: FEATURE_ENCODING_DIGEST_V3.into(),
            card_db_hash: format!("{KERNEL_CARDDB_HASH:016x}"),
            source_import: learner.identity_v1().clone(),
            behavior_state_sha256: learner_behavior.identity.state_sha256.clone(),
            seat_behaviors,
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
                let tensor = NativeFlatDecisionTensorV3 {
                    common: row.tensor.tensor(),
                };
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
                            encoded_decision_view_generic_v1(tensor, FreshLineageGenerationV1::V3),
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

    pub(super) fn checkpoint_fixture_v1() -> (
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
            registry_transfer: None,
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
                let PlayPolicyOriginV1::Imported(origin) = &mut s.source_import else {
                    panic!("fixture must retain imported origin")
                };
                origin.weights_sha256 = "0".repeat(64)
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
        // `TensorBitsV1` is generic over the shared inner tensor: exercise it
        // directly against `NativeFlatDecisionTensorV2`, since both the V3
        // and V4 wrappers just forward `.common` to it unchanged.
        let mut t = NativeFlatDecisionTensorV2::default();
        t.state = vec![0.0, -0.0, 1.2345678, f32::MIN_POSITIVE];
        t.object_card_ids = vec![65536];
        let bytes = serde_json::to_vec(&TensorBitsV1::from_tensor(&t)).unwrap();
        let saved: TensorBitsV1 = serde_json::from_slice(&bytes).unwrap();
        let restored = saved.tensor();
        assert_eq!(bits(&t.state), bits(&restored.state));
        assert_eq!(t.object_card_ids, restored.object_card_ids);
    }

    #[test]
    fn identity_valid_admits_pure_v3_and_pure_v4_but_rejects_any_mixed_pair() {
        let cards = format!("{KERNEL_CARDDB_HASH:016x}");
        assert!(identity_valid(FEATURE_CONTRACT_DIGEST_V3, FEATURE_ENCODING_DIGEST_V3, &cards).is_ok());
        assert!(identity_valid(FEATURE_CONTRACT_DIGEST_V4, FEATURE_ENCODING_DIGEST_V4, &cards).is_ok());
        // A mixed pair (a V3 contract digest with a V4 encoding digest, or
        // vice versa) must fail: this is a whole-pair check, not two
        // independent membership checks.
        assert!(identity_valid(FEATURE_CONTRACT_DIGEST_V3, FEATURE_ENCODING_DIGEST_V4, &cards).is_err());
        assert!(identity_valid(FEATURE_CONTRACT_DIGEST_V4, FEATURE_ENCODING_DIGEST_V3, &cards).is_err());
        assert!(identity_valid(FEATURE_CONTRACT_DIGEST_V4, FEATURE_ENCODING_DIGEST_V4, "0".repeat(16).as_str()).is_err());
    }

    /// Real complete game, deliberately opt-in (native engine compute),
    /// mirroring `phase1_parallel_real_episodes_match_serial`'s convention.
    /// Proves the actual "Collect" command path (not a synthetic replay
    /// fixture) for a fresh-V4 policy: the produced trajectory carries the
    /// V4 constants, `validate_trajectory` accepts it, and the same digest
    /// pair, checked by an unmodified pure-V3 equality, is rejected -- no
    /// cross-generation leak.
    #[test]
    #[ignore = "root-owned native qualification: one real complete game"]
    fn collect_episode_v4_real_game_stamps_v4_digests_and_is_rejected_by_a_pure_v3_check() {
        let mut policy = FrozenPlayPolicyV1::training_fixture_v4();
        let learner = test_behavior(&policy, false);
        let registered =
            crate::sideboard::checked_in_pauper_registered_deck_by_id_v1("Burn").unwrap();
        let deck = ExpandedDeckListV1 {
            label: "Burn".into(),
            mainboard: registered.registered_configuration().mainboard().to_vec(),
            sideboard: registered.registered_configuration().sideboard().to_vec(),
        };
        let episode = ExpandedEpisodeV1 {
            id: "fresh-v4-collect-fixture".into(),
            seed: 2_026_091_601,
            starting_player: 0,
            learner_seat: 0,
            opponent: None,
            registered: [deck.clone(), deck.clone()],
            selected: [deck.clone(), deck.clone()],
            postboard: false,
            max_physical_decisions: 100_000,
            max_policy_steps: 1_000_000,
        };
        let trajectory = collect_episode(&mut policy, &learner, None, &episode).unwrap();
        assert_eq!(trajectory.feature_contract_digest, FEATURE_CONTRACT_DIGEST_V4);
        assert_eq!(trajectory.feature_encoding_digest, FEATURE_ENCODING_DIGEST_V4);
        validate_trajectory(&trajectory).unwrap();
        assert!(
            !(trajectory.feature_contract_digest == FEATURE_CONTRACT_DIGEST_V3
                && trajectory.feature_encoding_digest == FEATURE_ENCODING_DIGEST_V3),
            "a V4 trajectory must never satisfy an unmodified pure-V3 identity check"
        );
    }

    /// Multi-seed regression soak for the V4 actor-visible encoder gaps
    /// fixed in `rl_session/flat_action_v4.rs` (see the doc comment on
    /// `ordinary_trainer_two_iteration_v4_fixture_stamps_v4_and_restores_cleanly`
    /// for the original single-seed reproduction; on
    /// `validate_origin_decision_against_reordered_candidates_v4` /
    /// `flat_validate_current_binding_staleness_v4` for the two further
    /// instances an earlier run of this soak found; and on
    /// `FlatResolvedActionObjectV4` for the burst-2
    /// `InvalidActionReference`/`DuplicateCanonicalObject` pair fixed
    /// alongside this extension). 20 real, untrained, unmanipulated V4
    /// self-play games across exactly the seven standard runtime decks
    /// (Wildfire, Rally, Affinity, Elves, Burn, Terror, Faeries -- cycled
    /// as consecutive pairs, so every deck appears as both learner and
    /// opponent mainboard multiple times across the run), each seeded
    /// distinctly. Real complete games (native engine compute), deliberately
    /// opt-in like its single-game sibling above.
    ///
    /// Deliberately excludes `CawGates` (its Gate lands reach
    /// `Decision::ChooseEffectColor`, which `flat_validate_semantic_policy_pair_v1`
    /// rejects with `UnsupportedActionSemantic`/`InvalidDecisionRelation` for
    /// every generation -- confirmed by running the identical seed/decks
    /// through the V3 encoder, which fails identically) and `Spy`/`SpyV2`
    /// (both hit `ActionReferenceShape` in the native tensorizer at a real
    /// decision, again confirmed identical under V3 on the same seed). Both
    /// are pre-existing, generation-agnostic gaps unrelated to the V4-vs-V3
    /// divergence this soak exists to catch; see the fix report for the
    /// residual-risk note. The eight V2 archetype variants (BurnV2,
    /// DelverV2, AffinityV2, RallyV2, WildfireV2, ElvesV2, TerrorV2,
    /// DimirTerrorV2) this soak previously also cycled through are dropped
    /// here to focus exactly on the seven standard decks the task named;
    /// they are not a coverage loss for the two burst-2 defects specifically, since
    /// both are demonstrated and fixed by the real-game regressions above
    /// and the synthetic fixture in `flat_action_v4.rs`, not by this soak.
    /// Extended from 20 to 60 games (more seeds) as part of the
    /// campaign-001 block-1 V4 actor-visible-encoding fix (a card that is
    /// itself a spell on the stack sourcing a `TriggerCondition::CastSelf`/
    /// `home_zone: Zone::Stack` triggered ability of its own, e.g. Writhing
    /// Chrysalis): re-run to completion once against the fixed encoder.
    #[test]
    #[ignore = "root-owned native qualification: 60 real V4 games across the seven standard Pauper decks"]
    fn v4_gameplay_soak_over_standard_decks_completes_every_seed() {
        const DECK_IDS: [&str; 7] = [
            "Wildfire", "Rally", "Affinity", "Elves", "Burn", "Terror", "Faeries",
        ];
        const GAME_COUNT: usize = 60;
        let mut failures = Vec::new();
        for game_index in 0..GAME_COUNT {
            let deck_a_id = DECK_IDS[game_index % DECK_IDS.len()];
            let deck_b_id = DECK_IDS[(game_index + 1) % DECK_IDS.len()];
            let mut policy = FrozenPlayPolicyV1::training_fixture_v4();
            let learner = test_behavior(&policy, false);
            let episode = ExpandedEpisodeV1 {
                id: format!("v4-soak-{game_index}"),
                seed: 2_026_091_601 + game_index as u64,
                starting_player: (game_index % 2) as u8,
                learner_seat: 0,
                opponent: None,
                registered: [list(deck_a_id), list(deck_b_id)],
                selected: [list(deck_a_id), list(deck_b_id)],
                postboard: false,
                max_physical_decisions: 100_000,
                max_policy_steps: 1_000_000,
            };
            if let Err(error) = collect_episode(&mut policy, &learner, None, &episode) {
                failures.push(format!(
                    "game {game_index} seed {} decks ({deck_a_id}, {deck_b_id}): {error}",
                    episode.seed
                ));
            }
        }
        assert!(
            failures.is_empty(),
            "V4 gameplay soak failures:\n{}",
            failures.join("\n")
        );
    }

    /// Loads the real `fresh-initialization-checks-002` inspection-a
    /// weights (the actual Python-producer output the burst-2 WSL run used
    /// when it found both defects below), via the production
    /// `load_policy_v1` loader, so a self-play game driven by these exact
    /// weights reproduces the exact recorded decision step, not merely an
    /// analogous one under `training_fixture_v4()`'s fixed/synthetic
    /// weights (confirmed empirically: the fixed-weight fixture completes
    /// both games below in under 400 decisions without ever reaching
    /// either defect). Depends on
    /// `E:/mtg-kernel-learned-sideboarding-evidence` (the burst-2
    /// preparation's own evidence tree, sha256-pinned below), so both
    /// tests that use it are `#[ignore]`d qualification tests, matching
    /// this module's existing `collect_episode_v4_real_game_...`/
    /// `v4_gameplay_soak_over_standard_decks_...` convention for
    /// real-weight, real-game evidence.
    fn burst2_evidence_fresh_v4_policy_v1() -> FrozenPlayPolicyV1 {
        let root = std::path::Path::new(
            "E:/mtg-kernel-learned-sideboarding-evidence/bo3-post480-preparation-001/phase1-training-qualification-001/fresh-initialization-checks-002/generated-a",
        );
        let source = fresh_initialization_source::ExpandedFreshInitializationSourceV1 {
            schema: fresh_initialization_source::SOURCE_SCHEMA.into(),
            initialization: PinnedFileV1 {
                path: root.join("initialization.json"),
                sha256: "524b5ab409236ace22de794c6512d3adf302b654d15b64bc95a07c389e8cbc2f".into(),
            },
            parameters: PinnedFileV1 {
                path: root.join("parameters.f32le"),
                sha256: "6346acf09188d19c8d320006131fe715da60e88758bf660b8ad5af93c238a001".into(),
            },
        };
        let features = FrozenPlayObservationTransferV3 {
            expected_feature_contract_digest: FEATURE_CONTRACT_DIGEST_V4.into(),
            expected_feature_encoding_digest: FEATURE_ENCODING_DIGEST_V4.into(),
        };
        fresh_initialization_source::load_policy_v1(&source, &features).unwrap()
    }

    /// Card-id arrays taken verbatim from `a-sized-v2.json`'s failing
    /// episode's own "registered"/"selected" fields (iteration 0, episode
    /// index 37), byte-equal to `list("Faeries")`/`list("Affinity")`
    /// (asserted once here rather than at every call site).
    fn burst2_faeries_affinity_decks_v1() -> [ExpandedDeckListV1; 2] {
        let faeries = ExpandedDeckListV1 {
            label: "Faeries".into(),
            mainboard: vec![
                17, 17, 17, 17, 22, 22, 32, 32, 32, 32, 33, 33, 33, 33, 38, 38, 52, 52, 55, 55,
                55, 55, 60, 60, 60, 60, 60, 60, 60, 60, 60, 60, 60, 60, 60, 60, 60, 60, 60, 60,
                75, 75, 75, 75, 80, 80, 80, 80, 82, 82, 82, 82, 99, 106, 106, 106, 110, 110, 110,
                110,
            ],
            sideboard: vec![0, 0, 0, 4, 4, 4, 7, 7, 7, 7, 22, 57, 57, 113, 113],
        };
        let affinity = ExpandedDeckListV1 {
            label: "Affinity".into(),
            mainboard: vec![
                5, 5, 5, 6, 6, 18, 23, 23, 23, 23, 35, 41, 41, 41, 41, 48, 48, 48, 56, 58, 58, 58,
                58, 62, 62, 62, 69, 73, 73, 73, 73, 78, 78, 78, 78, 79, 79, 79, 94, 94, 94, 94,
                96, 96, 96, 96, 102, 102, 103, 103, 114, 117, 117, 117, 117, 121, 121, 125, 125,
                125,
            ],
            sideboard: vec![7, 7, 9, 46, 57, 57, 57, 57, 62, 90, 90, 90, 90, 95, 124],
        };
        assert_eq!(faeries.mainboard, list("Faeries").mainboard, "registry deck differs from evidence");
        assert_eq!(faeries.sideboard, list("Faeries").sideboard, "registry deck differs from evidence");
        assert_eq!(affinity.mainboard, list("Affinity").mainboard, "registry deck differs from evidence");
        assert_eq!(affinity.sideboard, list("Affinity").sideboard, "registry deck differs from evidence");
        [faeries, affinity]
    }

    /// Burst-2 defect 1 regression (`InvalidActionReference`): real V4
    /// fresh-lineage self-play, Elves vs Wildfire, seed
    /// 8736899219446983818, starting player 0 -- byte-identical to the
    /// WSL `native_expanded_training_run_v1` run that crashed at exactly
    /// step 275, `physical_decision_id` 264, actor P1, a `Surface`
    /// decision with `legal_action_count` 2 (confirmed by reproducing the
    /// pre-fix crash with this exact fixture before landing the fix in
    /// `rl_session/flat_action_v4.rs`). Root cause: a `Source`-role
    /// reference to a `PendingEffect`-context historical row (frozen at
    /// the object's battlefield zone/`zone_change_count`) and a later
    /// `TargetObject`-role reference to the same physical `arena_id` (now
    /// a known library card, a different zone/`zone_change_count`) both
    /// default `position` to 0, so the old `(arena_id, position)` dedup
    /// key forced them through the "must resolve identically" check even
    /// though they legitimately differ. Now completes the full real game.
    #[test]
    #[ignore = "root-owned native qualification: real V4 self-play against burst-2 evidence weights"]
    fn burst2_defect_a_elves_vs_wildfire_v4_regression() {
        let mut policy = burst2_evidence_fresh_v4_policy_v1();
        let learner = test_behavior(&policy, false);
        let episode = ExpandedEpisodeV1 {
            id: "burst2-defect-a".into(),
            seed: 8_736_899_219_446_983_818,
            starting_player: 0,
            learner_seat: 0,
            opponent: None,
            registered: [list("Elves"), list("Wildfire")],
            selected: [list("Elves"), list("Wildfire")],
            postboard: false,
            max_physical_decisions: 100_000,
            max_policy_steps: 1_000_000,
        };
        let trajectory = collect_episode(&mut policy, &learner, None, &episode)
            .expect("V4 self-play must complete this real game without an actor-visible encoding error");
        assert!(trajectory.decisions.len() > 275, "must play well past the formerly-crashing step 275");
        validate_trajectory(&trajectory).unwrap();
    }

    /// Burst-2 defect 2 regression (`DuplicateCanonicalObject`): real V4
    /// fresh-lineage self-play, Faeries vs Affinity, seed
    /// 16977991839826055713 (found by a bounded local seed sweep around
    /// the originally reported seed 16977991839826055697, whose exact
    /// game did not reproduce under this evidence policy -- almost
    /// certainly because the WSL run's "current" opponent-pool member was
    /// not byte-identical to this "initial" fresh policy; no preserved
    /// evidence of the exact WSL weights exists to confirm further. This
    /// nearby seed reproduces the same error kind on the same reported
    /// deck pair with direct diagnostic confirmation of the exact
    /// mechanism during root-causing), starting player 1 -- crashed
    /// pre-fix at step 303, actor P1, a `Surface` decision with
    /// `legal_action_count` 6. Root cause: two `Decision::OrderTriggers`
    /// positions (0 and 1) sharing one VISIBLE (not hidden) physical
    /// source both resolve through the position-INSENSITIVE ordinary zone
    /// lookup to the byte-identical row, yet the old dedup key still
    /// treated `position` as significant unconditionally, so the second
    /// occurrence was flagged as a canonical-key collision against the
    /// first. See `rl_session/flat_action_v4.rs`'s
    /// `v4_two_visible_order_triggers_positions_sharing_one_physical_source_share_one_row`
    /// for the same mechanism reproduced as a fast, self-contained unit
    /// fixture (that test, not this one, is the primary regression floor
    /// for CI; this one additionally proves the fix against the real
    /// reported deck pair and real fresh-lineage weights).
    #[test]
    #[ignore = "root-owned native qualification: real V4 self-play against burst-2 evidence weights"]
    fn burst2_defect_b_faeries_vs_affinity_v4_regression() {
        let [faeries, affinity] = burst2_faeries_affinity_decks_v1();
        let mut policy = burst2_evidence_fresh_v4_policy_v1();
        let learner = test_behavior(&policy, false);
        let episode = ExpandedEpisodeV1 {
            id: "burst2-defect-b".into(),
            seed: 16_977_991_839_826_055_713,
            starting_player: 1,
            learner_seat: 0,
            opponent: None,
            registered: [faeries.clone(), affinity.clone()],
            selected: [faeries, affinity],
            postboard: false,
            max_physical_decisions: 100_000,
            max_policy_steps: 1_000_000,
        };
        let trajectory = collect_episode(&mut policy, &learner, None, &episode)
            .expect("V4 self-play must complete this real game without an actor-visible encoding error");
        assert!(trajectory.decisions.len() > 303, "must play well past the formerly-crashing step 303");
        validate_trajectory(&trajectory).unwrap();
    }

    /// The originally reported burst-2 defect-B seed (16977991839826055697,
    /// Faeries vs Affinity, starting player 1) never reproduced the crash
    /// against this evidence policy (see
    /// `burst2_defect_b_faeries_vs_affinity_v4_regression`'s doc comment),
    /// but is still worth a non-regression floor: this exact
    /// originally-reported (seed, deck pair) must keep completing cleanly.
    /// Like its siblings, this is part of the `#[ignore]`d root-owned
    /// qualification suite, not an always-checked test: it depends on the
    /// burst-2 evidence tree (the real evidence-weight policy), not the
    /// synthetic fixtures the always-run suite uses.
    #[test]
    #[ignore = "root-owned native qualification: real V4 self-play against burst-2 evidence weights"]
    fn burst2_originally_reported_defect_b_seed_completes_cleanly() {
        let [faeries, affinity] = burst2_faeries_affinity_decks_v1();
        let mut policy = burst2_evidence_fresh_v4_policy_v1();
        let learner = test_behavior(&policy, false);
        let episode = ExpandedEpisodeV1 {
            id: "burst2-defect-b-original-seed".into(),
            seed: 16_977_991_839_826_055_697,
            starting_player: 1,
            learner_seat: 0,
            opponent: None,
            registered: [faeries.clone(), affinity.clone()],
            selected: [faeries, affinity],
            postboard: false,
            max_physical_decisions: 100_000,
            max_policy_steps: 1_000_000,
        };
        let trajectory = collect_episode(&mut policy, &learner, None, &episode)
            .expect("V4 self-play must complete this real game without an actor-visible encoding error");
        validate_trajectory(&trajectory).unwrap();
    }

    /// Item 1 of the fix procedure: confirm the V3 (frozen) path is
    /// unaffected by playing the identical seeds/deck pairs both burst-2
    /// defects were found on through a real V3 self-play game. `V3` has no
    /// `fresh_successor`/`FreshLineageGenerationV1::V4` concept at all, so
    /// there is no way to force the literal WSL run's V4 weights through
    /// the V3 encoder; this instead uses `training_fixture_v3()`
    /// (self-contained, deterministic, no evidence dependency, the same
    /// generation-comparison idiom `ordinary_trainer_two_iteration_v3_fixture_state_hash_is_unchanged`
    /// already uses alongside its V4 sibling), always-run since it needs
    /// no external weights. `flat_action_v4.rs`'s
    /// `v4_two_visible_order_triggers_positions_sharing_one_physical_source_share_one_row`
    /// additionally proves V3 unaffected on the EXACT synthetic state that
    /// reproduces defect 2's mechanism, by calling the V3 encoder directly
    /// against it.
    #[test]
    fn burst2_defect_seeds_complete_cleanly_through_v3_encoder() {
        for (seed, starting_player, decks) in [
            (8_736_899_219_446_983_818u64, 0u8, [list("Elves"), list("Wildfire")]),
            (16_977_991_839_826_055_713u64, 1u8, burst2_faeries_affinity_decks_v1()),
            (16_977_991_839_826_055_697u64, 1u8, burst2_faeries_affinity_decks_v1()),
        ] {
            let mut policy = FrozenPlayPolicyV1::training_fixture_v3();
            let learner = test_behavior(&policy, false);
            let episode = ExpandedEpisodeV1 {
                id: format!("burst2-v3-check-{seed}"),
                seed,
                starting_player,
                learner_seat: 0,
                opponent: None,
                registered: decks.clone(),
                selected: decks,
                postboard: false,
                max_physical_decisions: 100_000,
                max_policy_steps: 1_000_000,
            };
            let trajectory = collect_episode(&mut policy, &learner, None, &episode).unwrap_or_else(|e| {
                panic!("V3 self-play must never fail on these seeds/decks (seed {seed}): {e}")
            });
            validate_trajectory(&trajectory).unwrap();
        }
    }

    /// Campaign-001 block-1 dry-block regression (`InvalidActionReference`):
    /// real V4 fresh-lineage self-play, Wildfire (seat 0) vs Terror (seat 1,
    /// the learner), seed 3157112932801185221, starting player 0, against
    /// the real `fresh-initialization-checks-002` inspection-a evidence
    /// weights -- byte-identical to the Windows-native
    /// `native_expanded_training_run_v1` campaign run (campaign-001 block 1,
    /// iteration 26 slot 6) that crashed with `V4 actor-visible encoding:
    /// Action(InvalidActionReference)` at step 155, actor P1, decision_kind
    /// Surface, legal_action_count 1 (`CAMPAIGN-001-BLOCK1-DRY-001.md`
    /// section 6a). Root cause: Terror's own `Counterspell`, in the process
    /// of being cast, chose its only legal target -- Wildfire's `Writhing
    /// Chrysalis` spell -- while Writhing Chrysalis's own
    /// `TriggerCondition::CastSelf`/`home_zone: Zone::Stack` triggered
    /// ability (`trigger.rs`'s `WRITHING_CHRYSALIS_TRIGGERS`) also sat on
    /// the stack, sharing the identical physical `source`; see
    /// `flat_stack_action_object_ordinal_v4`'s doc comment for the fix.
    ///
    /// This exact reproduction confirms both fixes: the game no longer
    /// fails at the V4 encoding layer (playing well past the
    /// formerly-crashing step 155), and, once the encoder let the game
    /// actually resolve Counterspell countering Writhing Chrysalis's spell,
    /// it no longer halts resolving Writhing Chrysalis's still-pending cast
    /// trigger with `engine_halted:InvalidEffectContinuation:source:<arena_id>`
    /// either. That halt was a separate, pre-existing, generation-agnostic
    /// engine defect (`engine.rs`'s `UnsupportedMechanic::
    /// InvalidEffectContinuation`, entirely outside `rl_session`/V3/V4):
    /// `validate_spell_sourced_trigger` required a live producing spell
    /// stack item for every spell-sourced (`home_zone: Zone::Stack`) cast
    /// trigger's own resolution, contrary to CR 603.3e (a triggered ability
    /// exists independently of its source once it triggers) and CR 608.2b
    /// (only an ability whose targets are all illegal fizzles; this cast
    /// trigger has none). The fix relaxes that check to CR 603.3e's actual
    /// requirement: once `zone_change_count` proves the producing spell
    /// left the stack at a later generation than this trigger's frozen
    /// contract recorded, the trigger resolves from that last-known
    /// incarnation instead of demanding a live producer. See
    /// `validate_spell_sourced_trigger`'s doc comment in `engine.rs`, and
    /// `writhing_chrysalis_cast_trigger_resolves_after_its_own_spell_is_countered`
    /// there for the fast, self-contained regression floor for the
    /// mechanism itself. The game now reaches natural completion. See also
    /// the always-run, self-contained
    /// `v4_spell_resolves_to_its_own_stack_position_despite_its_own_cast_trigger_sharing_source`
    /// (`flat_action_v4.rs`) for the encoding-layer regression floor.
    #[test]
    #[ignore = "root-owned native qualification: real V4 self-play against burst-2 evidence weights"]
    fn campaign001_block1_wildfire_vs_terror_v4_regression() {
        let mut policy = burst2_evidence_fresh_v4_policy_v1();
        let learner = test_behavior(&policy, false);
        let episode = ExpandedEpisodeV1 {
            id: "campaign001-block1-a-i26-s6".into(),
            seed: 3_157_112_932_801_185_221,
            starting_player: 0,
            learner_seat: 1,
            opponent: None,
            registered: [list("Wildfire"), list("Terror")],
            selected: [list("Wildfire"), list("Terror")],
            postboard: false,
            max_physical_decisions: 100_000,
            max_policy_steps: 1_000_000,
        };
        let trajectory = collect_episode(&mut policy, &learner, None, &episode).unwrap_or_else(|e| {
            panic!(
                "both the V4 encoding defect and the engine InvalidEffectContinuation defect \
                 must be fixed, so this game now completes naturally: got {e}"
            )
        });
        validate_trajectory(&trajectory).unwrap();
    }

    /// V3 check for the campaign-001 block-1 seed/decks above, matching
    /// `burst2_defect_seeds_complete_cleanly_through_v3_encoder`'s idiom
    /// (`training_fixture_v3()`, self-contained, no evidence dependency).
    /// Always-run.
    #[test]
    fn campaign001_block1_seed_completes_cleanly_through_v3_encoder() {
        let mut policy = FrozenPlayPolicyV1::training_fixture_v3();
        let learner = test_behavior(&policy, false);
        let episode = ExpandedEpisodeV1 {
            id: "campaign001-block1-v3-check".into(),
            seed: 3_157_112_932_801_185_221,
            starting_player: 0,
            learner_seat: 1,
            opponent: None,
            registered: [list("Wildfire"), list("Terror")],
            selected: [list("Wildfire"), list("Terror")],
            postboard: false,
            max_physical_decisions: 100_000,
            max_policy_steps: 1_000_000,
        };
        let trajectory = collect_episode(&mut policy, &learner, None, &episode)
            .unwrap_or_else(|e| panic!("V3 self-play must never fail on this seed/decks: {e}"));
        validate_trajectory(&trajectory).unwrap();
    }

    /// Campaign-001 block-1 sweep-offset-26 regression (`InvalidDecisionRelation`):
    /// real V4 fresh-lineage self-play, same decks/policy/starting player as
    /// `campaign001_block1_wildfire_vs_terror_v4_regression`, seed
    /// 3157112932801185247 (offset 26 of the bounded 30-seed local sweep
    /// around the defect-1 seed). Root cause was the identical, already-fixed
    /// `engine::validate_spell_sourced_trigger` defect from defect 1, not a
    /// separate V4-encoder bug: this game's Writhing Chrysalis cast trigger
    /// is left pending on the stack after its own spell is countered
    /// (exactly the defect-1 mechanism), but is not the very next thing to
    /// resolve, so the engine itself never halts on it. Instead, `rl.rs`'s
    /// `stack_source_ref` revalidates every stack item's producer -- this
    /// still-pending one included -- every time ANY later decision's
    /// observation is built, which failed pre-fix well before the trigger
    /// ever reached the top of the stack. Both V3's and V4's actor-visible
    /// encoders call into the identical shared validation chain (see
    /// `rl_session::flat_action_v4::tests::
    /// v3_and_v4_encode_a_later_decision_despite_a_still_pending_departed_producer_trigger`'s
    /// doc comment for the exact call chain, and
    /// `policy_observation_v6::tests::
    /// v6_spell_sourced_trigger_survives_producer_countered_while_still_pending`
    /// for the fast, self-contained mechanism fixture both real-game
    /// regressions below reduce to), so the defect-1 engine fix alone
    /// resolves this too: no V4-only or V3-only file needed a change.
    /// Byte-identical to the real crash this reproduced pre-fix (V4
    /// actor-visible encoding: `Action(InvalidDecisionRelation)`, step 457,
    /// actor P0, decision_kind Surface, legal_action_count 4).
    #[test]
    #[ignore = "root-owned native qualification: real V4 self-play against burst-2 evidence weights"]
    fn campaign001_block1_sweep_offset26_wildfire_vs_terror_v4_regression() {
        let mut policy = burst2_evidence_fresh_v4_policy_v1();
        let learner = test_behavior(&policy, false);
        let episode = ExpandedEpisodeV1 {
            id: "campaign001-block1-sweep-offset26".into(),
            seed: 3_157_112_932_801_185_247,
            starting_player: 0,
            learner_seat: 1,
            opponent: None,
            registered: [list("Wildfire"), list("Terror")],
            selected: [list("Wildfire"), list("Terror")],
            postboard: false,
            max_physical_decisions: 100_000,
            max_policy_steps: 1_000_000,
        };
        let trajectory = collect_episode(&mut policy, &learner, None, &episode).unwrap_or_else(|e| {
            panic!(
                "both the V4 encoding defect (InvalidDecisionRelation) and its root cause \
                 (the engine's validate_spell_sourced_trigger defect) must be fixed, so this \
                 game now completes naturally: got {e}"
            )
        });
        validate_trajectory(&trajectory).unwrap();
    }

    /// V3 check for the campaign-001 block-1 sweep-offset-26 seed/decks
    /// above, matching `campaign001_block1_seed_completes_cleanly_through_v3_encoder`'s
    /// idiom (`training_fixture_v3()`, self-contained, no evidence
    /// dependency). Always-run.
    #[test]
    fn campaign001_block1_sweep_offset26_seed_completes_cleanly_through_v3_encoder() {
        let mut policy = FrozenPlayPolicyV1::training_fixture_v3();
        let learner = test_behavior(&policy, false);
        let episode = ExpandedEpisodeV1 {
            id: "campaign001-block1-sweep-offset26-v3-check".into(),
            seed: 3_157_112_932_801_185_247,
            starting_player: 0,
            learner_seat: 1,
            opponent: None,
            registered: [list("Wildfire"), list("Terror")],
            selected: [list("Wildfire"), list("Terror")],
            postboard: false,
            max_physical_decisions: 100_000,
            max_policy_steps: 1_000_000,
        };
        let trajectory = collect_episode(&mut policy, &learner, None, &episode)
            .unwrap_or_else(|e| panic!("V3 self-play must never fail on this seed/decks: {e}"));
        validate_trajectory(&trajectory).unwrap();
    }

    /// The burst-2 fixture: two full ordinary-trainer iterations (Collect,
    /// Update, Collect, Update) driven through the real `execute_v1` command
    /// entry points against a real, on-disk fresh source, not a direct
    /// `collect_episode`/`execute_update_v1` call. `execute_update_v1`'s own
    /// internal readback (`initialize(&resumed_source)` plus a state-hash
    /// equality check, run every time it publishes a checkpoint) already
    /// proves the first checkpoint restores cleanly; the second iteration's
    /// Collect against that same checkpoint additionally proves the
    /// ordinary Collect path (not just Update's own readback) loads it.
    ///
    /// `mutate`/`decks` let a caller apply an arbitrary weight manipulation
    /// and deck pair. Both the V3 and V4 callers below pass a no-op mutation
    /// and the real constructed-deck pair (Affinity/Terror): the V4 caller
    /// used to need the `compact_board_policy_v4`-style Pass-biased weights
    /// and an all-basic-land opponent to sidestep a real V4 actor-visible
    /// encoder gap on a library-search "Surface" decision
    /// (`Action(InvalidDecisionRelation)`, see the fix at
    /// `rl_session/flat_action_v4.rs`'s
    /// `validate_origin_decision_against_reordered_candidates_v4` and
    /// `flat_visible_action_object_extension_aware_v4`); now that the gap is
    /// fixed, it plays the same real decks as V3, unmanipulated.
    ///
    /// Returns (final checkpoint's feature_contract_digest,
    /// feature_encoding_digest, after_state_sha256).
    fn run_ordinary_two_iteration_fixture_v1(
        feature_identity: crate::sideboard_play_policy_v1::FreshFeatureIdentityV1,
        label: &str,
        mutate: impl FnOnce(&mut Vec<NativeNamedParameterV1>),
        decks: [ExpandedDeckListV1; 2],
        backward_execution: UpdateBackwardExecutionV1,
    ) -> Result<(String, String, String), String> {
        let root = std::env::temp_dir().join(format!(
            "ordinary-two-iteration-{label}-{}",
            std::process::id()
        ));
        let source_struct = fresh_initialization_source::write_synthetic_fresh_source_with_parameters_v1(
            &root.join("source"),
            feature_identity,
            mutate,
        );
        let descriptor_path = root.join("descriptor.json");
        let descriptor_bytes = serde_json::to_vec(&source_struct).unwrap();
        std::fs::write(&descriptor_path, &descriptor_bytes).unwrap();
        let play_import = PinnedFileV1 {
            path: descriptor_path.canonicalize().unwrap(),
            sha256: sha(&descriptor_bytes),
        };
        let feature_transfer = FrozenPlayObservationTransferV3 {
            expected_feature_contract_digest: feature_identity.feature_contract_digest.into(),
            expected_feature_encoding_digest: feature_identity.feature_encoding_digest.into(),
        };
        let mut source = ExpandedModelSourceV1 {
            play_import,
            feature_transfer,
            checkpoint: None,
        };
        let mut final_contract = String::new();
        let mut final_encoding = String::new();
        let mut final_state_sha256 = String::new();
        for iteration in 0..2u64 {
            let episode = ExpandedEpisodeV1 {
                id: format!("{label}-iter-{iteration}"),
                seed: 2_026_091_601 + iteration,
                starting_player: 0,
                learner_seat: 0,
                opponent: None,
                registered: decks.clone(),
                selected: decks.clone(),
                postboard: false,
                max_physical_decisions: 100_000,
                max_policy_steps: 1_000_000,
            };
            let collect_result = execute_v1(ExpandedTrainingCommandV1::Collect {
                source: source.clone(),
                episodes: vec![episode],
                output_directory: root.join(format!("collect-{iteration}")),
            })?;
            let trajectories: Vec<PinnedFileV1> =
                serde_json::from_value(collect_result["trajectories"].clone()).unwrap();
            let update_result = execute_v1(ExpandedTrainingCommandV1::Update {
                source: source.clone(),
                trajectories,
                learning_rate: 0.0003,
                value_coefficient: 0.5,
                update_backend: ExpandedUpdateBackendV1::Cpu,
                update_backward_execution: backward_execution,
                output_directory: root.join(format!("update-{iteration}")),
            })?;
            // Engineering timing note only (never a claim): visible in the
            // test log so `learner_update_seconds` sequential vs.
            // fixed_partition_4 can be read off this fixture directly.
            eprintln!(
                "timing note: label={label} iteration={iteration} backward_execution={backward_execution:?} \
                 learner_update_seconds={}",
                update_result["learner_update_seconds"]
            );
            let checkpoint: PinnedFileV1 =
                serde_json::from_value(update_result["checkpoint"].clone()).unwrap();
            let saved: ExpandedCheckpointV1 = read_pinned(&checkpoint).unwrap();
            final_contract = saved.feature_contract_digest.clone();
            final_encoding = saved.feature_encoding_digest.clone();
            final_state_sha256 = update_result["after_state_sha256"]
                .as_str()
                .unwrap()
                .to_owned();
            source.checkpoint = Some(checkpoint);
        }
        Ok((final_contract, final_encoding, final_state_sha256))
    }

    /// Regression fixture for the V4 actor-visible encoder gap discovered
    /// during the V4 fresh-lineage update-path follow-up: an
    /// untrained/default-weight V4 policy playing two full natural games of
    /// real constructed decks (Affinity/Terror, unmanipulated weights, same
    /// seeds/decks as the V3 sibling below) used to fail deterministically
    /// at seed 2_026_091_601 (this fixture's first iteration), step 42,
    /// physical_decision_id 42, actor P1, a "Surface" decision (a
    /// `Decision::ChooseEffectTargets` library search) with
    /// `Action(InvalidDecisionRelation)`. Root cause: `current.candidates`
    /// is reordered into decision-local-library canonical order by the
    /// shared `flat_action_v3::prepare_and_build_v3` step every physical
    /// decision runs through regardless of scoring generation, but
    /// `encode_current_flat_action_slice_v4` validated the origin decision
    /// directly against that already-reordered list (which
    /// `flat_validate_origin_decision_v1`'s `ChooseEffectTargets` arm
    /// assumes is still in raw engine-surfaced order), and separately never
    /// resolved a decision-local-library target's `Zone::Library` reference
    /// to an action-object row at all. Fixed by
    /// `validate_origin_decision_against_reordered_candidates_v4` (re-derive
    /// the raw order, validate that, then require the live, reordered
    /// `current.candidates` to equal the proven normalization of it -- V3's
    /// own technique, reused) and
    /// `flat_visible_action_object_extension_aware_v4` (resolve a
    /// decision-local-library/historical-PendingEffect reference through the
    /// extension row first, exactly as V3's `build_with_extensions` closure
    /// does, before falling back to the ordinary live zone lookup) in
    /// `rl_session/flat_action_v4.rs`. The V3 path never needed either fix
    /// (`ordinary_trainer_two_iteration_v3_fixture_state_hash_is_unchanged`,
    /// same seeds/decks, has always passed) because `build_with_extensions`
    /// already re-derives and validates the raw order itself.
    #[test]
    fn ordinary_trainer_two_iteration_v4_fixture_stamps_v4_and_restores_cleanly() {
        let (contract, encoding, state) = run_ordinary_two_iteration_fixture_v1(
            crate::sideboard_play_policy_v1::FRESH_FEATURE_IDENTITY_V4,
            "v4",
            |_parameters| {},
            [list("Affinity"), list("Terror")],
            UpdateBackwardExecutionV1::Sequential,
        )
        .unwrap();
        assert_eq!(contract, FEATURE_CONTRACT_DIGEST_V4);
        assert_eq!(encoding, FEATURE_ENCODING_DIGEST_V4);
        // Pinned against the value this fixture actually produced running it
        // post-fix, 2026-09-16 (identical to the V3 sibling's pinned hash
        // below: a fresh/default-weight V3 and V4 policy select the same
        // action at every one of this seed pair's decisions here, so the two
        // real games -- and their post-update state -- are byte-identical;
        // this pin is this test's own independent measurement, not copied
        // from the V3 constant).
        assert_eq!(
            state,
            "28faac998142e07ddc5368238f45ff05e6a0578140d0e490a24daefe95fff878",
            "the V4 ordinary-trainer path over real constructed decks must stay reproducible"
        );
    }

    /// Pinned against the value this exact fixture (same seeds, decks,
    /// learning rate, value coefficient, unmanipulated real weights)
    /// produced by actually running it at commit a94dce64, one commit
    /// before the update-path follow-up that added this test:
    /// `28faac998142e07ddc5368238f45ff05e6a0578140d0e490a24daefe95fff878`.
    /// Proves the V3 ordinary-trainer path is byte-for-byte unchanged by
    /// this follow-up's dispatch/generic-tensor refactor, not merely
    /// "should be unchanged by inspection".
    #[test]
    fn ordinary_trainer_two_iteration_v3_fixture_state_hash_is_unchanged() {
        let (contract, encoding, state) = run_ordinary_two_iteration_fixture_v1(
            crate::sideboard_play_policy_v1::FRESH_FEATURE_IDENTITY_V3,
            "v3",
            |_parameters| {},
            [list("Affinity"), list("Terror")],
            UpdateBackwardExecutionV1::Sequential,
        )
        .unwrap();
        assert_eq!(contract, FEATURE_CONTRACT_DIGEST_V3);
        assert_eq!(encoding, FEATURE_ENCODING_DIGEST_V3);
        assert_eq!(
            state,
            "28faac998142e07ddc5368238f45ff05e6a0578140d0e490a24daefe95fff878",
            "the V3 ordinary-trainer path must be byte-for-byte unchanged by \
             the V4 update-path follow-up"
        );
    }

    /// Real-fixture topology invariance for the config-driven fixed-partition
    /// option: the same two-iteration ordinary-trainer fixture as the tests
    /// above, run end to end through the real `execute_v1`/`execute_update_v1`
    /// production path with `update_backward_execution=fixed_partition_4`, at
    /// backward worker limits 1, 2, 3 and 4 (the
    /// `#[cfg(test)]`-only override installed for exactly this). Complements
    /// `native_policy_train_step_v1::fixed_partition_parallel_backward_is_deterministic_topology_invariant_and_bounded`,
    /// which proves the same property on a synthetic batch directly against
    /// the lower-level primitive; this proves it on real collected games
    /// through the whole config/dispatch path added by this change.
    ///
    /// Timing note (engineering only, not a claim; single CPU, this small
    /// two-iteration fixture, 103 then 88 physical decisions; measured
    /// 2026-09-16 with `cargo test ... -- --nocapture`, reading
    /// `learner_update_seconds` straight from the real `update.json`
    /// receipts): Sequential 2.22-2.42s; fixed_partition_4 at worker_limit=1
    /// 2.26-2.41s (about the same, as expected: same 4-way logical split,
    /// one thread); worker_limit=2 1.93-1.95s; worker_limit=3 1.92-2.03s;
    /// worker_limit=4 1.47-1.50s, roughly 33-38% below Sequential on this
    /// fixture. See `UPDATE-PHASE-STUDY-001.md`/`PARALLEL-REPLAY-SMOKE-001.md`
    /// for the larger, representative fixture this small correctness fixture
    /// does not attempt to reproduce.
    #[test]
    fn ordinary_trainer_two_iteration_v4_fixture_fixed_partition_is_topology_invariant_and_pinned()
    {
        let mut reference: Option<String> = None;
        for worker_limit in [1usize, 2, 3, 4] {
            set_fixed_partition_backward_worker_limit_override_for_test_v1(Some(worker_limit));
            let (contract, encoding, state) = run_ordinary_two_iteration_fixture_v1(
                crate::sideboard_play_policy_v1::FRESH_FEATURE_IDENTITY_V4,
                &format!("v4-fixed-partition-w{worker_limit}"),
                |_parameters| {},
                [list("Affinity"), list("Terror")],
                UpdateBackwardExecutionV1::FixedPartition4,
            )
            .unwrap();
            set_fixed_partition_backward_worker_limit_override_for_test_v1(None);
            assert_eq!(contract, FEATURE_CONTRACT_DIGEST_V4);
            assert_eq!(encoding, FEATURE_ENCODING_DIGEST_V4);
            match &reference {
                None => reference = Some(state),
                Some(reference) => assert_eq!(
                    &state, reference,
                    "fixed_partition_4 must be topology invariant across backward worker limits \
                     on the real two-iteration fixture (worker_limit={worker_limit})"
                ),
            }
        }
        // Pinned against the value this fixture actually produced running it
        // 2026-09-16: the fresh lineage's partitioned-backward reference for
        // this fixture. Deliberately not equal to the Sequential pin above
        // (`28faac99...`); `native_policy_train_step_v1`'s own determinism
        // test already proves fixed-partition and sequential are not
        // expected to be bit-identical to each other.
        assert_eq!(
            reference.unwrap(),
            "2fd271a3aab18a9bcd2501bf92ce8f24664ffcb1269ba885b79159fe743e1bf0",
            "the V4 fixed-partition ordinary-trainer path over real constructed decks \
             must stay reproducible"
        );
    }

    /// V3 must reject `fixed_partition_4` with a clear error, before any
    /// game is even collected: the imported/frozen V3 path cannot change via
    /// this config.
    #[test]
    fn ordinary_trainer_v3_update_rejects_fixed_partition_backward() {
        let error = run_ordinary_two_iteration_fixture_v1(
            crate::sideboard_play_policy_v1::FRESH_FEATURE_IDENTITY_V3,
            "v3-rejects-fixed-partition",
            |_parameters| {},
            [list("Affinity"), list("Terror")],
            UpdateBackwardExecutionV1::FixedPartition4,
        )
        .unwrap_err();
        assert!(
            error.contains("V4 fresh lineage"),
            "error should clearly explain the V3 rejection: {error}"
        );
    }
}
