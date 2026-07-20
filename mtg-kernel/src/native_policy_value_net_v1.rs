//! CPU inference reference for Python `KernelPolicyValueNet`.
//!
//! This module intentionally favors a direct, auditable transcription over
//! throughput. It covers `kernel-policy-value-net-8` with the
//! `runner-fixed-v1` initializer. It is not a trainer, optimizer, checkpoint
//! loader, production backend, or cross-libm bit-parity claim.

use sha2::{Digest, Sha256};
use std::error::Error;
use std::fmt::{Display, Formatter};

pub(crate) const MODEL_CONFIG_SCHEMA_VERSION_V1: usize = 5;
pub(crate) const MODEL_ARCHITECTURE_VERSION_V1: &str = "kernel-policy-value-net-8";
pub(crate) const FEATURE_SCHEMA_VERSION_V1: &str = "actor-relative-v5-python-4";
pub(crate) const FEATURE_REGISTRY_VERSION_V1: &str = "rust-observation-v5-action-v5-registry-4";
pub(crate) const FEATURE_CONTRACT_DIGEST_V1: &str =
    "bcc808186e40a1ad6aec679d8a386631cb1226379366a632603f0beb95b47396";
pub(crate) const FEATURE_ENCODING_DIGEST_V1: &str =
    "918e57a0796807e84310026de48d30b500813ef37d939462ea85b7255a39111c";
pub(crate) const MODEL_CONFIG_FINGERPRINT_V1: &str =
    "f3836afa17acc74b4856fe18222345116f27c12fa5ad18c34b4dec3f04855251";
pub(crate) const MODEL_PYTHON_AUTHORITY_SHA256_V1: &str =
    "2e3e830d4212b8c8f8085861b2508c49a6d7192b9621cef087dd396e22d12c59";
pub(crate) const INITIALIZER_RUNNER_FIXED_V1: &str = "runner-fixed-v1";

pub(crate) const STATE_DIM_V1: usize = 219;
pub(crate) const OBJECT_FEATURE_DIM_V1: usize = 98;
pub(crate) const EDGE_FEATURE_DIM_V1: usize = 41;
pub(crate) const ACTION_FEATURE_DIM_V1: usize = 195;
pub(crate) const ACTION_REF_FEATURE_DIM_V1: usize = 25;
pub(crate) const OBJECT_GROUP_COUNT_V1: usize = 20;
pub(crate) const HIDDEN_DIM_V1: usize = 64;
pub(crate) const CARD_EMBEDDING_DIM_V1: usize = 16;
pub(crate) const CARD_VOCAB_SIZE_V1: usize = 65_537;
pub(crate) const PARAMETER_COUNT_V1: usize = 1_230_994;

const OBJECT_ENCODER_INPUT_V1: usize = OBJECT_FEATURE_DIM_V1 + CARD_EMBEDDING_DIM_V1;
const EDGE_ENCODER_INPUT_V1: usize = EDGE_FEATURE_DIM_V1 + HIDDEN_DIM_V1 * 2;
const NODE_UPDATE_INPUT_V1: usize = HIDDEN_DIM_V1 * 2;
const POOLED_OBJECT_DIM_V1: usize = HIDDEN_DIM_V1 * OBJECT_GROUP_COUNT_V1;
const STATE_ENCODER_INPUT_V1: usize = STATE_DIM_V1 + POOLED_OBJECT_DIM_V1;
const ACTION_REF_ENCODER_INPUT_V1: usize = ACTION_REF_FEATURE_DIM_V1 + HIDDEN_DIM_V1;
const ACTION_ENCODER_INPUT_V1: usize = ACTION_FEATURE_DIM_V1 + HIDDEN_DIM_V1;
const SCORER_INPUT_V1: usize = HIDDEN_DIM_V1 * 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct NativePolicyValueModelConfigV1 {
    pub(crate) schema_version: usize,
    pub(crate) model_architecture_version: &'static str,
    pub(crate) feature_schema_version: &'static str,
    pub(crate) feature_registry_version: &'static str,
    pub(crate) feature_contract_digest: &'static str,
    pub(crate) feature_encoding_digest: &'static str,
    pub(crate) card_vocab_size: usize,
    pub(crate) card_embedding_dim: usize,
    pub(crate) hidden_dim: usize,
    pub(crate) state_dim: usize,
    pub(crate) object_feature_dim: usize,
    pub(crate) edge_feature_dim: usize,
    pub(crate) action_feature_dim: usize,
    pub(crate) object_group_count: usize,
    pub(crate) action_ref_feature_dim: usize,
}

impl NativePolicyValueModelConfigV1 {
    pub(crate) const fn contract_v1() -> Self {
        Self {
            schema_version: MODEL_CONFIG_SCHEMA_VERSION_V1,
            model_architecture_version: MODEL_ARCHITECTURE_VERSION_V1,
            feature_schema_version: FEATURE_SCHEMA_VERSION_V1,
            feature_registry_version: FEATURE_REGISTRY_VERSION_V1,
            feature_contract_digest: FEATURE_CONTRACT_DIGEST_V1,
            feature_encoding_digest: FEATURE_ENCODING_DIGEST_V1,
            card_vocab_size: CARD_VOCAB_SIZE_V1,
            card_embedding_dim: CARD_EMBEDDING_DIM_V1,
            hidden_dim: HIDDEN_DIM_V1,
            state_dim: STATE_DIM_V1,
            object_feature_dim: OBJECT_FEATURE_DIM_V1,
            edge_feature_dim: EDGE_FEATURE_DIM_V1,
            action_feature_dim: ACTION_FEATURE_DIM_V1,
            object_group_count: OBJECT_GROUP_COUNT_V1,
            action_ref_feature_dim: ACTION_REF_FEATURE_DIM_V1,
        }
    }

    pub(crate) fn validate(self) -> Result<(), NativePolicyValueErrorV1> {
        let expected = Self::contract_v1();
        macro_rules! exact {
            ($field:ident) => {
                if self.$field != expected.$field {
                    return Err(NativePolicyValueErrorV1::ModelConfigMismatch(stringify!(
                        $field
                    )));
                }
            };
        }
        exact!(schema_version);
        exact!(model_architecture_version);
        exact!(feature_schema_version);
        exact!(feature_registry_version);
        exact!(feature_contract_digest);
        exact!(feature_encoding_digest);
        exact!(card_vocab_size);
        exact!(card_embedding_dim);
        exact!(hidden_dim);
        exact!(state_dim);
        exact!(object_feature_dim);
        exact!(edge_feature_dim);
        exact!(action_feature_dim);
        exact!(object_group_count);
        exact!(action_ref_feature_dim);
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct NativeEncodedDecisionSchemaV1 {
    pub(crate) version: &'static str,
    pub(crate) registry_version: &'static str,
    pub(crate) contract_digest: &'static str,
    pub(crate) encoding_digest: &'static str,
    pub(crate) state_dim: usize,
    pub(crate) object_feature_dim: usize,
    pub(crate) edge_feature_dim: usize,
    pub(crate) action_feature_dim: usize,
    pub(crate) object_group_count: usize,
    pub(crate) action_ref_feature_dim: usize,
}

impl NativeEncodedDecisionSchemaV1 {
    pub(crate) const fn contract_v1() -> Self {
        Self {
            version: FEATURE_SCHEMA_VERSION_V1,
            registry_version: FEATURE_REGISTRY_VERSION_V1,
            contract_digest: FEATURE_CONTRACT_DIGEST_V1,
            encoding_digest: FEATURE_ENCODING_DIGEST_V1,
            state_dim: STATE_DIM_V1,
            object_feature_dim: OBJECT_FEATURE_DIM_V1,
            edge_feature_dim: EDGE_FEATURE_DIM_V1,
            action_feature_dim: ACTION_FEATURE_DIM_V1,
            object_group_count: OBJECT_GROUP_COUNT_V1,
            action_ref_feature_dim: ACTION_REF_FEATURE_DIM_V1,
        }
    }

    fn validate(
        self,
        config: NativePolicyValueModelConfigV1,
    ) -> Result<(), NativePolicyValueErrorV1> {
        macro_rules! exact {
            ($field:ident, $expected:expr) => {
                if self.$field != $expected {
                    return Err(NativePolicyValueErrorV1::SchemaMismatch(stringify!($field)));
                }
            };
        }
        exact!(version, config.feature_schema_version);
        exact!(registry_version, config.feature_registry_version);
        exact!(contract_digest, config.feature_contract_digest);
        exact!(encoding_digest, config.feature_encoding_digest);
        exact!(state_dim, config.state_dim);
        exact!(object_feature_dim, config.object_feature_dim);
        exact!(edge_feature_dim, config.edge_feature_dim);
        exact!(action_feature_dim, config.action_feature_dim);
        exact!(object_group_count, config.object_group_count);
        exact!(action_ref_feature_dim, config.action_ref_feature_dim);
        Ok(())
    }
}

/// Borrowed mirror of Python `EncodedDecision`: schema plus thirteen tensors.
#[derive(Clone, Copy, Debug)]
pub(crate) struct NativeEncodedDecisionViewV1<'a> {
    pub(crate) schema: NativeEncodedDecisionSchemaV1,
    pub(crate) state: &'a [f32],
    pub(crate) object_features: &'a [f32],
    pub(crate) object_card_ids: &'a [i64],
    pub(crate) object_groups: &'a [i64],
    pub(crate) object_node_ids: &'a [i64],
    pub(crate) edge_features: &'a [f32],
    pub(crate) edge_source_indices: &'a [i64],
    pub(crate) edge_target_indices: &'a [i64],
    pub(crate) action_features: &'a [f32],
    pub(crate) action_ref_features: &'a [f32],
    pub(crate) action_ref_card_ids: &'a [i64],
    pub(crate) action_ref_action_indices: &'a [i64],
    pub(crate) action_ref_node_indices: &'a [i64],
}

impl<'a> NativeEncodedDecisionViewV1<'a> {
    #[allow(clippy::too_many_arguments)]
    pub(crate) const fn from_slices_unvalidated(
        schema: NativeEncodedDecisionSchemaV1,
        state: &'a [f32],
        object_features: &'a [f32],
        object_card_ids: &'a [i64],
        object_groups: &'a [i64],
        object_node_ids: &'a [i64],
        edge_features: &'a [f32],
        edge_source_indices: &'a [i64],
        edge_target_indices: &'a [i64],
        action_features: &'a [f32],
        action_ref_features: &'a [f32],
        action_ref_card_ids: &'a [i64],
        action_ref_action_indices: &'a [i64],
        action_ref_node_indices: &'a [i64],
    ) -> Self {
        Self {
            schema,
            state,
            object_features,
            object_card_ids,
            object_groups,
            object_node_ids,
            edge_features,
            edge_source_indices,
            edge_target_indices,
            action_features,
            action_ref_features,
            action_ref_card_ids,
            action_ref_action_indices,
            action_ref_node_indices,
        }
    }

    pub(crate) fn validate(
        self,
        config: NativePolicyValueModelConfigV1,
    ) -> Result<ValidatedCountsV1, NativePolicyValueErrorV1> {
        self.schema.validate(config)?;
        exact_len("state", self.state.len(), config.state_dim)?;
        finite_slice("state", self.state)?;

        let object_count = self.object_card_ids.len();
        if object_count == 0 {
            return Err(NativePolicyValueErrorV1::EmptyRows("object_features"));
        }
        exact_len(
            "object_features",
            self.object_features.len(),
            checked_product(object_count, config.object_feature_dim, "object_features")?,
        )?;
        exact_len("object_groups", self.object_groups.len(), object_count)?;
        exact_len("object_node_ids", self.object_node_ids.len(), object_count)?;
        finite_slice("object_features", self.object_features)?;
        validate_indices(
            "object_card_ids",
            self.object_card_ids,
            config.card_vocab_size,
        )?;
        validate_indices(
            "object_groups",
            self.object_groups,
            config.object_group_count,
        )?;
        for (position, value) in self.object_node_ids.iter().copied().enumerate() {
            if value != position as i64 {
                return Err(NativePolicyValueErrorV1::NonContiguousNodeId { position, value });
            }
        }

        let edge_count = self.edge_source_indices.len();
        exact_len(
            "edge_target_indices",
            self.edge_target_indices.len(),
            edge_count,
        )?;
        exact_len(
            "edge_features",
            self.edge_features.len(),
            checked_product(edge_count, config.edge_feature_dim, "edge_features")?,
        )?;
        finite_slice("edge_features", self.edge_features)?;
        validate_indices(
            "edge_source_indices",
            self.edge_source_indices,
            object_count,
        )?;
        validate_indices(
            "edge_target_indices",
            self.edge_target_indices,
            object_count,
        )?;

        if self.action_features.is_empty()
            || !self
                .action_features
                .len()
                .is_multiple_of(config.action_feature_dim)
        {
            return Err(NativePolicyValueErrorV1::ShapeMismatch {
                field: "action_features",
                expected: config.action_feature_dim,
                actual: self.action_features.len(),
            });
        }
        let action_count = self.action_features.len() / config.action_feature_dim;
        finite_slice("action_features", self.action_features)?;

        let action_ref_count = self.action_ref_card_ids.len();
        exact_len(
            "action_ref_features",
            self.action_ref_features.len(),
            checked_product(
                action_ref_count,
                config.action_ref_feature_dim,
                "action_ref_features",
            )?,
        )?;
        exact_len(
            "action_ref_action_indices",
            self.action_ref_action_indices.len(),
            action_ref_count,
        )?;
        exact_len(
            "action_ref_node_indices",
            self.action_ref_node_indices.len(),
            action_ref_count,
        )?;
        finite_slice("action_ref_features", self.action_ref_features)?;
        validate_indices(
            "action_ref_card_ids",
            self.action_ref_card_ids,
            config.card_vocab_size,
        )?;
        validate_indices(
            "action_ref_action_indices",
            self.action_ref_action_indices,
            action_count,
        )?;
        validate_indices(
            "action_ref_node_indices",
            self.action_ref_node_indices,
            object_count,
        )?;

        Ok(ValidatedCountsV1 {
            object_count,
            edge_count,
            action_count,
            action_ref_count,
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct NativePolicyValueOutputV1 {
    pub(crate) logits: Vec<f32>,
    pub(crate) value: f32,
}

/// Ordered, owned copy of one Torch-compatible named parameter.  The order
/// and `[output, input]` linear-weight layout come from `named_parameters()`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct NativeNamedParameterV1 {
    pub(crate) name: &'static str,
    pub(crate) shape: Vec<usize>,
    pub(crate) values: Vec<f32>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum NativePolicyValueErrorV1 {
    ModelConfigMismatch(&'static str),
    SchemaMismatch(&'static str),
    ShapeMismatch {
        field: &'static str,
        expected: usize,
        actual: usize,
    },
    EmptyRows(&'static str),
    IndexOutOfRange {
        field: &'static str,
        position: usize,
        value: i64,
        upper_exclusive: usize,
    },
    NonContiguousNodeId {
        position: usize,
        value: i64,
    },
    NonFinite {
        field: &'static str,
        position: usize,
    },
    SizeOverflow(&'static str),
    ParameterInvariant(&'static str),
    NonFiniteOutput {
        field: &'static str,
        position: usize,
    },
}

impl Display for NativePolicyValueErrorV1 {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "native policy/value v1 error: {self:?}")
    }
}

impl Error for NativePolicyValueErrorV1 {}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ValidatedCountsV1 {
    pub(crate) object_count: usize,
    pub(crate) edge_count: usize,
    pub(crate) action_count: usize,
    pub(crate) action_ref_count: usize,
}

#[derive(Clone, Debug)]
struct LinearV1 {
    input_dim: usize,
    output_dim: usize,
    // Python nn.Linear layout, contiguous row-major [output, input].
    weight: Vec<f32>,
    bias: Vec<f32>,
}

impl LinearV1 {
    fn runner_fixed_v1(input_dim: usize, output_dim: usize) -> Self {
        Self {
            input_dim,
            output_dim,
            weight: runner_fixed_rank2_v1(input_dim * output_dim),
            bias: runner_fixed_rank1_v1(output_dim),
        }
    }

    fn validate(&self, name: &'static str) -> Result<(), NativePolicyValueErrorV1> {
        if self.weight.len() != self.input_dim * self.output_dim
            || self.bias.len() != self.output_dim
        {
            return Err(NativePolicyValueErrorV1::ParameterInvariant(name));
        }
        if !self
            .weight
            .iter()
            .chain(&self.bias)
            .all(|value| value.is_finite())
        {
            return Err(NativePolicyValueErrorV1::ParameterInvariant(name));
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
struct TwoLayerTanhV1 {
    first: LinearV1,
    second: LinearV1,
}

impl TwoLayerTanhV1 {
    fn runner_fixed_v1(input_dim: usize) -> Self {
        Self {
            first: LinearV1::runner_fixed_v1(input_dim, HIDDEN_DIM_V1),
            second: LinearV1::runner_fixed_v1(HIDDEN_DIM_V1, HIDDEN_DIM_V1),
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct NativePolicyValueNetV1 {
    config: NativePolicyValueModelConfigV1,
    card_embedding: Vec<f32>,
    object_encoder: TwoLayerTanhV1,
    edge_encoder: TwoLayerTanhV1,
    node_update: TwoLayerTanhV1,
    state_encoder: TwoLayerTanhV1,
    action_ref_encoder: TwoLayerTanhV1,
    action_encoder: TwoLayerTanhV1,
    scorer_first: LinearV1,
    scorer_second: LinearV1,
    value_first: LinearV1,
    value_second: LinearV1,
}

impl NativePolicyValueNetV1 {
    pub(crate) fn runner_fixed_v1(
        config: NativePolicyValueModelConfigV1,
    ) -> Result<Self, NativePolicyValueErrorV1> {
        config.validate()?;
        let mut card_embedding = runner_fixed_rank2_v1(checked_product(
            config.card_vocab_size,
            config.card_embedding_dim,
            "card_embedding",
        )?);
        card_embedding[..config.card_embedding_dim].fill(0.0);
        let model = Self {
            config,
            card_embedding,
            object_encoder: TwoLayerTanhV1::runner_fixed_v1(OBJECT_ENCODER_INPUT_V1),
            edge_encoder: TwoLayerTanhV1::runner_fixed_v1(EDGE_ENCODER_INPUT_V1),
            node_update: TwoLayerTanhV1::runner_fixed_v1(NODE_UPDATE_INPUT_V1),
            state_encoder: TwoLayerTanhV1::runner_fixed_v1(STATE_ENCODER_INPUT_V1),
            action_ref_encoder: TwoLayerTanhV1::runner_fixed_v1(ACTION_REF_ENCODER_INPUT_V1),
            action_encoder: TwoLayerTanhV1::runner_fixed_v1(ACTION_ENCODER_INPUT_V1),
            scorer_first: LinearV1::runner_fixed_v1(SCORER_INPUT_V1, HIDDEN_DIM_V1),
            scorer_second: LinearV1::runner_fixed_v1(HIDDEN_DIM_V1, 1),
            value_first: LinearV1::runner_fixed_v1(HIDDEN_DIM_V1, HIDDEN_DIM_V1),
            value_second: LinearV1::runner_fixed_v1(HIDDEN_DIM_V1, 1),
        };
        model.validate_parameters_v1()?;
        if model.parameter_count_v1() != PARAMETER_COUNT_V1 {
            return Err(NativePolicyValueErrorV1::ParameterInvariant(
                "parameter_count",
            ));
        }
        Ok(model)
    }

    pub(crate) fn config_v1(&self) -> NativePolicyValueModelConfigV1 {
        self.config
    }

    /// Computes into private temporaries and returns only after all final
    /// finiteness gates pass.
    pub(crate) fn forward_v1(
        &self,
        encoded: NativeEncodedDecisionViewV1<'_>,
    ) -> Result<NativePolicyValueOutputV1, NativePolicyValueErrorV1> {
        let counts = encoded.validate(self.config)?;

        let mut object_input = Vec::with_capacity(counts.object_count * OBJECT_ENCODER_INPUT_V1);
        for object in 0..counts.object_count {
            let features_begin = object * OBJECT_FEATURE_DIM_V1;
            object_input.extend_from_slice(
                &encoded.object_features[features_begin..features_begin + OBJECT_FEATURE_DIM_V1],
            );
            let token = encoded.object_card_ids[object] as usize;
            let embedding_begin = token * CARD_EMBEDDING_DIM_V1;
            object_input.extend_from_slice(
                &self.card_embedding[embedding_begin..embedding_begin + CARD_EMBEDDING_DIM_V1],
            );
        }
        let object_base_hidden =
            apply_two_layer_tanh_rows_v1(&self.object_encoder, &object_input, counts.object_count);

        let mut edge_pooled = vec![0.0; counts.object_count * HIDDEN_DIM_V1];
        if counts.edge_count > 0 {
            let mut edge_input = Vec::with_capacity(counts.edge_count * EDGE_ENCODER_INPUT_V1);
            for edge in 0..counts.edge_count {
                let feature_begin = edge * EDGE_FEATURE_DIM_V1;
                edge_input.extend_from_slice(
                    &encoded.edge_features[feature_begin..feature_begin + EDGE_FEATURE_DIM_V1],
                );
                let source = encoded.edge_source_indices[edge] as usize;
                let source_begin = source * HIDDEN_DIM_V1;
                edge_input.extend_from_slice(
                    &object_base_hidden[source_begin..source_begin + HIDDEN_DIM_V1],
                );
                let target = encoded.edge_target_indices[edge] as usize;
                let target_begin = target * HIDDEN_DIM_V1;
                edge_input.extend_from_slice(
                    &object_base_hidden[target_begin..target_begin + HIDDEN_DIM_V1],
                );
            }
            let edge_hidden =
                apply_two_layer_tanh_rows_v1(&self.edge_encoder, &edge_input, counts.edge_count);
            // Match Python's two ordered index_add_ calls: all source rows in
            // edge order, followed by all target rows in edge order. A
            // source==target edge therefore contributes twice.
            add_indexed_rows_v1(&mut edge_pooled, &edge_hidden, encoded.edge_source_indices);
            add_indexed_rows_v1(&mut edge_pooled, &edge_hidden, encoded.edge_target_indices);
        }

        let mut node_update_input = Vec::with_capacity(counts.object_count * NODE_UPDATE_INPUT_V1);
        for object in 0..counts.object_count {
            let begin = object * HIDDEN_DIM_V1;
            node_update_input.extend_from_slice(&object_base_hidden[begin..begin + HIDDEN_DIM_V1]);
            node_update_input.extend_from_slice(&edge_pooled[begin..begin + HIDDEN_DIM_V1]);
        }
        let object_hidden = apply_two_layer_tanh_rows_v1(
            &self.node_update,
            &node_update_input,
            counts.object_count,
        );

        let mut pooled_objects = vec![0.0; POOLED_OBJECT_DIM_V1];
        add_indexed_rows_v1(&mut pooled_objects, &object_hidden, encoded.object_groups);
        let mut state_input = Vec::with_capacity(STATE_ENCODER_INPUT_V1);
        state_input.extend_from_slice(encoded.state);
        state_input.extend_from_slice(&pooled_objects);
        let state_hidden = apply_two_layer_tanh_rows_v1(&self.state_encoder, &state_input, 1);

        let mut action_ref_pooled = vec![0.0; counts.action_count * HIDDEN_DIM_V1];
        if counts.action_ref_count > 0 {
            let mut action_ref_input =
                Vec::with_capacity(counts.action_ref_count * ACTION_REF_ENCODER_INPUT_V1);
            for action_ref in 0..counts.action_ref_count {
                let feature_begin = action_ref * ACTION_REF_FEATURE_DIM_V1;
                action_ref_input.extend_from_slice(
                    &encoded.action_ref_features
                        [feature_begin..feature_begin + ACTION_REF_FEATURE_DIM_V1],
                );
                let node = encoded.action_ref_node_indices[action_ref] as usize;
                let node_begin = node * HIDDEN_DIM_V1;
                action_ref_input
                    .extend_from_slice(&object_hidden[node_begin..node_begin + HIDDEN_DIM_V1]);
            }
            let action_ref_hidden = apply_two_layer_tanh_rows_v1(
                &self.action_ref_encoder,
                &action_ref_input,
                counts.action_ref_count,
            );
            add_indexed_rows_v1(
                &mut action_ref_pooled,
                &action_ref_hidden,
                encoded.action_ref_action_indices,
            );
        }
        // action_ref_card_ids is validated above because Python validates it,
        // but kernel-policy-value-net-8 likewise does not consume it here.

        let mut action_input = Vec::with_capacity(counts.action_count * ACTION_ENCODER_INPUT_V1);
        for action in 0..counts.action_count {
            let feature_begin = action * ACTION_FEATURE_DIM_V1;
            action_input.extend_from_slice(
                &encoded.action_features[feature_begin..feature_begin + ACTION_FEATURE_DIM_V1],
            );
            let pooled_begin = action * HIDDEN_DIM_V1;
            action_input
                .extend_from_slice(&action_ref_pooled[pooled_begin..pooled_begin + HIDDEN_DIM_V1]);
        }
        let action_hidden =
            apply_two_layer_tanh_rows_v1(&self.action_encoder, &action_input, counts.action_count);

        let mut scorer_input = Vec::with_capacity(counts.action_count * SCORER_INPUT_V1);
        for action in 0..counts.action_count {
            scorer_input.extend_from_slice(&state_hidden);
            let action_begin = action * HIDDEN_DIM_V1;
            scorer_input
                .extend_from_slice(&action_hidden[action_begin..action_begin + HIDDEN_DIM_V1]);
        }
        let mut scorer_hidden =
            linear_rows_v1(&self.scorer_first, &scorer_input, counts.action_count);
        tanh_in_place_v1(&mut scorer_hidden);
        let logits = linear_rows_v1(&self.scorer_second, &scorer_hidden, counts.action_count);

        let mut value_hidden = linear_rows_v1(&self.value_first, &state_hidden, 1);
        tanh_in_place_v1(&mut value_hidden);
        let value = linear_rows_v1(&self.value_second, &value_hidden, 1)[0];

        for (position, output) in logits.iter().copied().enumerate() {
            if !output.is_finite() {
                return Err(NativePolicyValueErrorV1::NonFiniteOutput {
                    field: "logits",
                    position,
                });
            }
        }
        if !value.is_finite() {
            return Err(NativePolicyValueErrorV1::NonFiniteOutput {
                field: "value",
                position: 0,
            });
        }
        Ok(NativePolicyValueOutputV1 { logits, value })
    }

    /// Transactional caller-owned output adapter. On every error, both
    /// caller-provided outputs retain their original values.
    pub(crate) fn forward_into_v1(
        &self,
        encoded: NativeEncodedDecisionViewV1<'_>,
        logits_output: &mut Vec<f32>,
        value_output: &mut f32,
    ) -> Result<(), NativePolicyValueErrorV1> {
        let output = self.forward_v1(encoded)?;
        *logits_output = output.logits;
        *value_output = output.value;
        Ok(())
    }

    pub(crate) fn parameter_count_v1(&self) -> usize {
        let mut count = 0usize;
        self.visit_parameters_v1(|_, _, values| count += values.len());
        count
    }

    pub(crate) fn parameter_manifest_sha256_raw_v1(&self) -> [u8; 32] {
        let mut digest = Sha256::new();
        self.visit_parameters_v1(|name, shape, values| {
            let name_bytes = name.as_bytes();
            digest.update((name_bytes.len() as u32).to_be_bytes());
            digest.update(name_bytes);
            digest.update((shape.len() as u32).to_be_bytes());
            for dimension in shape {
                digest.update((*dimension as u64).to_be_bytes());
            }
            digest.update((values.len() as u64).to_be_bytes());
            for value in values {
                digest.update(value.to_le_bytes());
            }
        });
        digest.finalize().into()
    }

    pub(crate) fn parameter_manifest_sha256_v1(&self) -> String {
        self.parameter_manifest_sha256_raw_v1()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }

    pub(crate) fn parameter_snapshot_v1(&self) -> Vec<NativeNamedParameterV1> {
        let mut parameters = Vec::new();
        self.visit_parameters_v1(|name, shape, values| {
            parameters.push(NativeNamedParameterV1 {
                name,
                shape: shape.to_vec(),
                values: values.to_vec(),
            });
        });
        parameters
    }

    /// Replaces all named parameters as one transaction.  Names, shapes,
    /// finiteness, parameter count, and the padding embedding row are checked
    /// on a private clone before the live model changes.
    pub(crate) fn replace_parameter_snapshot_v1(
        &mut self,
        replacement: &[NativeNamedParameterV1],
    ) -> Result<(), NativePolicyValueErrorV1> {
        let expected = self.parameter_snapshot_v1();
        if replacement.len() != expected.len()
            || replacement.iter().zip(&expected).any(|(actual, expected)| {
                actual.name != expected.name
                    || actual.shape != expected.shape
                    || actual.values.len() != expected.values.len()
                    || actual.values.iter().any(|value| !value.is_finite())
            })
        {
            return Err(NativePolicyValueErrorV1::ParameterInvariant(
                "replacement_manifest",
            ));
        }

        let mut candidate = self.clone();
        let mut position = 0usize;
        candidate.visit_parameters_mut_v1(|name, shape, values| {
            let source = &replacement[position];
            debug_assert_eq!(source.name, name);
            debug_assert_eq!(source.shape, shape);
            values.copy_from_slice(&source.values);
            position += 1;
        });
        candidate.validate_parameters_v1()?;
        if candidate.parameter_count_v1() != PARAMETER_COUNT_V1 {
            return Err(NativePolicyValueErrorV1::ParameterInvariant(
                "parameter_count",
            ));
        }
        *self = candidate;
        Ok(())
    }

    pub(crate) fn validate_parameters_v1(&self) -> Result<(), NativePolicyValueErrorV1> {
        let expected_embedding = checked_product(
            self.config.card_vocab_size,
            self.config.card_embedding_dim,
            "card_embedding",
        )?;
        if self.card_embedding.len() != expected_embedding
            || !self.card_embedding.iter().all(|value| value.is_finite())
            || self.card_embedding[..self.config.card_embedding_dim]
                .iter()
                .any(|value| value.to_bits() != 0)
        {
            return Err(NativePolicyValueErrorV1::ParameterInvariant(
                "card_embedding.weight",
            ));
        }
        self.object_encoder.first.validate("object_encoder.0")?;
        self.object_encoder.second.validate("object_encoder.2")?;
        self.edge_encoder.first.validate("edge_encoder.0")?;
        self.edge_encoder.second.validate("edge_encoder.2")?;
        self.node_update.first.validate("node_update.0")?;
        self.node_update.second.validate("node_update.2")?;
        self.state_encoder.first.validate("state_encoder.0")?;
        self.state_encoder.second.validate("state_encoder.2")?;
        self.action_ref_encoder
            .first
            .validate("action_ref_encoder.0")?;
        self.action_ref_encoder
            .second
            .validate("action_ref_encoder.2")?;
        self.action_encoder.first.validate("action_encoder.0")?;
        self.action_encoder.second.validate("action_encoder.2")?;
        self.scorer_first.validate("scorer.0")?;
        self.scorer_second.validate("scorer.2")?;
        self.value_first.validate("value_head.0")?;
        self.value_second.validate("value_head.2")?;
        Ok(())
    }

    pub(crate) fn visit_parameters_v1(
        &self,
        mut visitor: impl FnMut(&'static str, &[usize], &[f32]),
    ) {
        visitor(
            "card_embedding.weight",
            &[CARD_VOCAB_SIZE_V1, CARD_EMBEDDING_DIM_V1],
            &self.card_embedding,
        );
        visit_linear_v1(&mut visitor, "object_encoder.0", &self.object_encoder.first);
        visit_linear_v1(
            &mut visitor,
            "object_encoder.2",
            &self.object_encoder.second,
        );
        visit_linear_v1(&mut visitor, "edge_encoder.0", &self.edge_encoder.first);
        visit_linear_v1(&mut visitor, "edge_encoder.2", &self.edge_encoder.second);
        visit_linear_v1(&mut visitor, "node_update.0", &self.node_update.first);
        visit_linear_v1(&mut visitor, "node_update.2", &self.node_update.second);
        visit_linear_v1(&mut visitor, "state_encoder.0", &self.state_encoder.first);
        visit_linear_v1(&mut visitor, "state_encoder.2", &self.state_encoder.second);
        visit_linear_v1(
            &mut visitor,
            "action_ref_encoder.0",
            &self.action_ref_encoder.first,
        );
        visit_linear_v1(
            &mut visitor,
            "action_ref_encoder.2",
            &self.action_ref_encoder.second,
        );
        visit_linear_v1(&mut visitor, "action_encoder.0", &self.action_encoder.first);
        visit_linear_v1(
            &mut visitor,
            "action_encoder.2",
            &self.action_encoder.second,
        );
        visit_linear_v1(&mut visitor, "scorer.0", &self.scorer_first);
        visit_linear_v1(&mut visitor, "scorer.2", &self.scorer_second);
        visit_linear_v1(&mut visitor, "value_head.0", &self.value_first);
        visit_linear_v1(&mut visitor, "value_head.2", &self.value_second);
    }

    fn visit_parameters_mut_v1(
        &mut self,
        mut visitor: impl FnMut(&'static str, &[usize], &mut [f32]),
    ) {
        visitor(
            "card_embedding.weight",
            &[CARD_VOCAB_SIZE_V1, CARD_EMBEDDING_DIM_V1],
            &mut self.card_embedding,
        );
        visit_linear_mut_v1(
            &mut visitor,
            "object_encoder.0",
            &mut self.object_encoder.first,
        );
        visit_linear_mut_v1(
            &mut visitor,
            "object_encoder.2",
            &mut self.object_encoder.second,
        );
        visit_linear_mut_v1(&mut visitor, "edge_encoder.0", &mut self.edge_encoder.first);
        visit_linear_mut_v1(
            &mut visitor,
            "edge_encoder.2",
            &mut self.edge_encoder.second,
        );
        visit_linear_mut_v1(&mut visitor, "node_update.0", &mut self.node_update.first);
        visit_linear_mut_v1(&mut visitor, "node_update.2", &mut self.node_update.second);
        visit_linear_mut_v1(
            &mut visitor,
            "state_encoder.0",
            &mut self.state_encoder.first,
        );
        visit_linear_mut_v1(
            &mut visitor,
            "state_encoder.2",
            &mut self.state_encoder.second,
        );
        visit_linear_mut_v1(
            &mut visitor,
            "action_ref_encoder.0",
            &mut self.action_ref_encoder.first,
        );
        visit_linear_mut_v1(
            &mut visitor,
            "action_ref_encoder.2",
            &mut self.action_ref_encoder.second,
        );
        visit_linear_mut_v1(
            &mut visitor,
            "action_encoder.0",
            &mut self.action_encoder.first,
        );
        visit_linear_mut_v1(
            &mut visitor,
            "action_encoder.2",
            &mut self.action_encoder.second,
        );
        visit_linear_mut_v1(&mut visitor, "scorer.0", &mut self.scorer_first);
        visit_linear_mut_v1(&mut visitor, "scorer.2", &mut self.scorer_second);
        visit_linear_mut_v1(&mut visitor, "value_head.0", &mut self.value_first);
        visit_linear_mut_v1(&mut visitor, "value_head.2", &mut self.value_second);
    }
}

fn visit_linear_v1(
    visitor: &mut impl FnMut(&'static str, &[usize], &[f32]),
    prefix: &'static str,
    linear: &LinearV1,
) {
    let (weight_name, bias_name) = match prefix {
        "object_encoder.0" => ("object_encoder.0.weight", "object_encoder.0.bias"),
        "object_encoder.2" => ("object_encoder.2.weight", "object_encoder.2.bias"),
        "edge_encoder.0" => ("edge_encoder.0.weight", "edge_encoder.0.bias"),
        "edge_encoder.2" => ("edge_encoder.2.weight", "edge_encoder.2.bias"),
        "node_update.0" => ("node_update.0.weight", "node_update.0.bias"),
        "node_update.2" => ("node_update.2.weight", "node_update.2.bias"),
        "state_encoder.0" => ("state_encoder.0.weight", "state_encoder.0.bias"),
        "state_encoder.2" => ("state_encoder.2.weight", "state_encoder.2.bias"),
        "action_ref_encoder.0" => ("action_ref_encoder.0.weight", "action_ref_encoder.0.bias"),
        "action_ref_encoder.2" => ("action_ref_encoder.2.weight", "action_ref_encoder.2.bias"),
        "action_encoder.0" => ("action_encoder.0.weight", "action_encoder.0.bias"),
        "action_encoder.2" => ("action_encoder.2.weight", "action_encoder.2.bias"),
        "scorer.0" => ("scorer.0.weight", "scorer.0.bias"),
        "scorer.2" => ("scorer.2.weight", "scorer.2.bias"),
        "value_head.0" => ("value_head.0.weight", "value_head.0.bias"),
        "value_head.2" => ("value_head.2.weight", "value_head.2.bias"),
        _ => unreachable!("all native model parameters use frozen names"),
    };
    visitor(
        weight_name,
        &[linear.output_dim, linear.input_dim],
        &linear.weight,
    );
    visitor(bias_name, &[linear.output_dim], &linear.bias);
}

fn visit_linear_mut_v1(
    visitor: &mut impl FnMut(&'static str, &[usize], &mut [f32]),
    prefix: &'static str,
    linear: &mut LinearV1,
) {
    let (weight_name, bias_name) = match prefix {
        "object_encoder.0" => ("object_encoder.0.weight", "object_encoder.0.bias"),
        "object_encoder.2" => ("object_encoder.2.weight", "object_encoder.2.bias"),
        "edge_encoder.0" => ("edge_encoder.0.weight", "edge_encoder.0.bias"),
        "edge_encoder.2" => ("edge_encoder.2.weight", "edge_encoder.2.bias"),
        "node_update.0" => ("node_update.0.weight", "node_update.0.bias"),
        "node_update.2" => ("node_update.2.weight", "node_update.2.bias"),
        "state_encoder.0" => ("state_encoder.0.weight", "state_encoder.0.bias"),
        "state_encoder.2" => ("state_encoder.2.weight", "state_encoder.2.bias"),
        "action_ref_encoder.0" => ("action_ref_encoder.0.weight", "action_ref_encoder.0.bias"),
        "action_ref_encoder.2" => ("action_ref_encoder.2.weight", "action_ref_encoder.2.bias"),
        "action_encoder.0" => ("action_encoder.0.weight", "action_encoder.0.bias"),
        "action_encoder.2" => ("action_encoder.2.weight", "action_encoder.2.bias"),
        "scorer.0" => ("scorer.0.weight", "scorer.0.bias"),
        "scorer.2" => ("scorer.2.weight", "scorer.2.bias"),
        "value_head.0" => ("value_head.0.weight", "value_head.0.bias"),
        "value_head.2" => ("value_head.2.weight", "value_head.2.bias"),
        _ => unreachable!("all native model parameters use frozen names"),
    };
    visitor(
        weight_name,
        &[linear.output_dim, linear.input_dim],
        &mut linear.weight,
    );
    visitor(bias_name, &[linear.output_dim], &mut linear.bias);
}

fn apply_two_layer_tanh_rows_v1(encoder: &TwoLayerTanhV1, input: &[f32], rows: usize) -> Vec<f32> {
    let mut hidden = linear_rows_v1(&encoder.first, input, rows);
    tanh_in_place_v1(&mut hidden);
    let mut output = linear_rows_v1(&encoder.second, &hidden, rows);
    tanh_in_place_v1(&mut output);
    output
}

fn linear_rows_v1(linear: &LinearV1, input: &[f32], rows: usize) -> Vec<f32> {
    debug_assert_eq!(input.len(), rows * linear.input_dim);
    let mut output = Vec::with_capacity(rows * linear.output_dim);
    for row in 0..rows {
        let input_row = &input[row * linear.input_dim..(row + 1) * linear.input_dim];
        for output_index in 0..linear.output_dim {
            let weight_row = &linear.weight
                [output_index * linear.input_dim..(output_index + 1) * linear.input_dim];
            let mut value = linear.bias[output_index];
            for input_index in 0..linear.input_dim {
                value += input_row[input_index] * weight_row[input_index];
            }
            output.push(value);
        }
    }
    output
}

fn tanh_in_place_v1(values: &mut [f32]) {
    for value in values {
        *value = value.tanh();
    }
}

fn add_indexed_rows_v1(destination: &mut [f32], source: &[f32], indices: &[i64]) {
    debug_assert_eq!(source.len(), indices.len() * HIDDEN_DIM_V1);
    for (source_row, destination_index) in indices.iter().copied().enumerate() {
        let destination_row = destination_index as usize;
        let source_begin = source_row * HIDDEN_DIM_V1;
        let destination_begin = destination_row * HIDDEN_DIM_V1;
        for column in 0..HIDDEN_DIM_V1 {
            destination[destination_begin + column] += source[source_begin + column];
        }
    }
}

fn exact_len(
    field: &'static str,
    actual: usize,
    expected: usize,
) -> Result<(), NativePolicyValueErrorV1> {
    if actual != expected {
        return Err(NativePolicyValueErrorV1::ShapeMismatch {
            field,
            expected,
            actual,
        });
    }
    Ok(())
}

fn checked_product(
    left: usize,
    right: usize,
    field: &'static str,
) -> Result<usize, NativePolicyValueErrorV1> {
    left.checked_mul(right)
        .ok_or(NativePolicyValueErrorV1::SizeOverflow(field))
}

fn finite_slice(field: &'static str, values: &[f32]) -> Result<(), NativePolicyValueErrorV1> {
    for (position, value) in values.iter().copied().enumerate() {
        if !value.is_finite() {
            return Err(NativePolicyValueErrorV1::NonFinite { field, position });
        }
    }
    Ok(())
}

fn validate_indices(
    field: &'static str,
    values: &[i64],
    upper_exclusive: usize,
) -> Result<(), NativePolicyValueErrorV1> {
    for (position, value) in values.iter().copied().enumerate() {
        if value < 0 || usize::try_from(value).map_or(true, |index| index >= upper_exclusive) {
            return Err(NativePolicyValueErrorV1::IndexOutOfRange {
                field,
                position,
                value,
                upper_exclusive,
            });
        }
    }
    Ok(())
}

const RUNNER_FIXED_RANK2_BITS_V1: [u32; 31] = [
    0xbd99_999a,
    0xbd8f_5c29,
    0xbd85_1eb8,
    0xbd75_c28f,
    0xbd61_47ae,
    0xbd4c_cccd,
    0xbd38_51ec,
    0xbd23_d70a,
    0xbd0f_5c29,
    0xbcf5_c28f,
    0xbccc_cccd,
    0xbca3_d70a,
    0xbc75_c28f,
    0xbc23_d70a,
    0xbba3_d70a,
    0x0000_0000,
    0x3ba3_d70a,
    0x3c23_d70a,
    0x3c75_c28f,
    0x3ca3_d70a,
    0x3ccc_cccd,
    0x3cf5_c28f,
    0x3d0f_5c29,
    0x3d23_d70a,
    0x3d38_51ec,
    0x3d4c_cccd,
    0x3d61_47ae,
    0x3d75_c28f,
    0x3d85_1eb8,
    0x3d8f_5c29,
    0x3d99_999a,
];

const RUNNER_FIXED_RANK1_64_BITS_V1: [u32; 64] = [
    0xbd4c_cccd,
    0xbd46_4c65,
    0xbd3f_cbfd,
    0xbd39_4b95,
    0xbd32_cb2d,
    0xbd2c_4ac5,
    0xbd25_ca5d,
    0xbd1f_49f5,
    0xbd18_c98d,
    0xbd12_4925,
    0xbd0b_c8bc,
    0xbd05_4854,
    0xbcfd_8fd9,
    0xbcf0_8f09,
    0xbce3_8e39,
    0xbcd6_8d69,
    0xbcc9_8c99,
    0xbcbc_8bc9,
    0xbcaf_8af9,
    0xbca2_8a29,
    0xbc95_8958,
    0xbc88_8888,
    0xbc77_0f70,
    0xbc5d_0dd0,
    0xbc43_0c30,
    0xbc29_0a90,
    0xbc0f_08f0,
    0xbbea_0ea0,
    0xbbb6_0b60,
    0xbb82_0820,
    0xbb1c_09c0,
    0xba50_0d00,
    0x3a50_0d00,
    0x3b1c_09c0,
    0x3b82_0820,
    0x3bb6_0b60,
    0x3bea_0ea0,
    0x3c0f_08f0,
    0x3c29_0a90,
    0x3c43_0c30,
    0x3c5d_0dd0,
    0x3c77_0f70,
    0x3c88_8888,
    0x3c95_8958,
    0x3ca2_8a29,
    0x3caf_8af9,
    0x3cbc_8bc9,
    0x3cc9_8c99,
    0x3cd6_8d69,
    0x3ce3_8e39,
    0x3cf0_8f09,
    0x3cfd_8fd9,
    0x3d05_4854,
    0x3d0b_c8bc,
    0x3d12_4925,
    0x3d18_c98d,
    0x3d1f_49f5,
    0x3d25_ca5d,
    0x3d2c_4ac5,
    0x3d32_cb2d,
    0x3d39_4b95,
    0x3d3f_cbfd,
    0x3d46_4c65,
    0x3d4c_cccd,
];

fn runner_fixed_rank2_v1(count: usize) -> Vec<f32> {
    (0..count)
        .map(|index| f32::from_bits(RUNNER_FIXED_RANK2_BITS_V1[index % 31]))
        .collect()
}

fn runner_fixed_rank1_v1(count: usize) -> Vec<f32> {
    match count {
        1 => vec![f32::from_bits(RUNNER_FIXED_RANK1_64_BITS_V1[0])],
        HIDDEN_DIM_V1 => RUNNER_FIXED_RANK1_64_BITS_V1
            .iter()
            .copied()
            .map(f32::from_bits)
            .collect(),
        _ => unreachable!("frozen model has only rank-one lengths 1 and 64"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    const GOLDEN_JSON: &str =
        include_str!("../../data/native_policy_value_net_v1/runner_fixed_forward_goldens_v1.json");
    const MODEL_AUTHORITY: &[u8] = include_bytes!("../../python/mtg_kernel_rl/model.py");

    #[derive(Debug, Deserialize)]
    struct GoldenAuthority {
        absolute_tolerance: f32,
        relative_tolerance: f32,
        sha256: String,
        initializer: String,
        numerical_claim: String,
    }

    #[derive(Debug, Deserialize)]
    struct GoldenParameterManifest {
        sha256: String,
        count: usize,
        ordered: Vec<GoldenParameter>,
    }

    #[derive(Debug, Deserialize)]
    struct GoldenParameter {
        name: String,
        shape: Vec<usize>,
        count: usize,
        first_bits: String,
        last_bits: String,
    }

    #[derive(Debug, Deserialize)]
    struct GoldenCase {
        name: String,
        state: Vec<f32>,
        object_features: Vec<f32>,
        object_card_ids: Vec<i64>,
        object_groups: Vec<i64>,
        object_node_ids: Vec<i64>,
        edge_features: Vec<f32>,
        edge_source_indices: Vec<i64>,
        edge_target_indices: Vec<i64>,
        action_features: Vec<f32>,
        action_ref_features: Vec<f32>,
        action_ref_card_ids: Vec<i64>,
        action_ref_action_indices: Vec<i64>,
        action_ref_node_indices: Vec<i64>,
        torch_logits: Vec<f32>,
        torch_value: f32,
    }

    #[derive(Debug, Deserialize)]
    struct GoldenFixture {
        schema: String,
        authority: GoldenAuthority,
        model_config: serde_json::Value,
        model_config_fingerprint: String,
        parameter_manifest: GoldenParameterManifest,
        cases: Vec<GoldenCase>,
    }

    fn fixture() -> GoldenFixture {
        serde_json::from_str(GOLDEN_JSON).expect("checked golden parses")
    }

    fn view(case: &GoldenCase) -> NativeEncodedDecisionViewV1<'_> {
        NativeEncodedDecisionViewV1::from_slices_unvalidated(
            NativeEncodedDecisionSchemaV1::contract_v1(),
            &case.state,
            &case.object_features,
            &case.object_card_ids,
            &case.object_groups,
            &case.object_node_ids,
            &case.edge_features,
            &case.edge_source_indices,
            &case.edge_target_indices,
            &case.action_features,
            &case.action_ref_features,
            &case.action_ref_card_ids,
            &case.action_ref_action_indices,
            &case.action_ref_node_indices,
        )
    }

    fn assert_close(actual: f32, expected: f32, absolute: f32, relative: f32) {
        let tolerance = absolute + relative * expected.abs();
        assert!(
            (actual - expected).abs() <= tolerance,
            "actual={actual:?} expected={expected:?} delta={:?} tolerance={tolerance:?}",
            (actual - expected).abs()
        );
    }

    #[test]
    fn runner_fixed_parameters_match_torch_named_parameter_manifest() {
        let fixture = fixture();
        assert_eq!(
            fixture.schema,
            "native-policy-value-net-v1-torch-goldens-v1"
        );
        assert_eq!(fixture.authority.initializer, INITIALIZER_RUNNER_FIXED_V1);
        assert!(fixture.authority.numerical_claim.contains("no cross-libm"));
        assert_eq!(
            fixture.model_config_fingerprint,
            MODEL_CONFIG_FINGERPRINT_V1
        );
        assert_eq!(
            fixture.model_config["schema_version"],
            MODEL_CONFIG_SCHEMA_VERSION_V1
        );
        assert_eq!(
            fixture.model_config["model_architecture_version"],
            MODEL_ARCHITECTURE_VERSION_V1
        );
        assert_eq!(
            fixture.model_config["feature_schema_version"],
            FEATURE_SCHEMA_VERSION_V1
        );
        assert_eq!(
            fixture.model_config["feature_registry_version"],
            FEATURE_REGISTRY_VERSION_V1
        );
        assert_eq!(
            fixture.model_config["feature_contract_digest"],
            FEATURE_CONTRACT_DIGEST_V1
        );
        assert_eq!(
            fixture.model_config["feature_encoding_digest"],
            FEATURE_ENCODING_DIGEST_V1
        );
        assert_eq!(fixture.model_config["card_vocab_size"], CARD_VOCAB_SIZE_V1);
        assert_eq!(
            fixture.model_config["card_embedding_dim"],
            CARD_EMBEDDING_DIM_V1
        );
        assert_eq!(fixture.model_config["hidden_dim"], HIDDEN_DIM_V1);
        assert_eq!(fixture.model_config["state_dim"], STATE_DIM_V1);
        assert_eq!(
            fixture.model_config["object_feature_dim"],
            OBJECT_FEATURE_DIM_V1
        );
        assert_eq!(
            fixture.model_config["edge_feature_dim"],
            EDGE_FEATURE_DIM_V1
        );
        assert_eq!(
            fixture.model_config["action_feature_dim"],
            ACTION_FEATURE_DIM_V1
        );
        assert_eq!(
            fixture.model_config["object_group_count"],
            OBJECT_GROUP_COUNT_V1
        );
        assert_eq!(
            fixture.model_config["action_ref_feature_dim"],
            ACTION_REF_FEATURE_DIM_V1
        );
        let authority_sha = format!("{:x}", Sha256::digest(MODEL_AUTHORITY));
        assert_eq!(authority_sha, MODEL_PYTHON_AUTHORITY_SHA256_V1);
        assert_eq!(authority_sha, fixture.authority.sha256);

        let model =
            NativePolicyValueNetV1::runner_fixed_v1(NativePolicyValueModelConfigV1::contract_v1())
                .expect("model builds");
        assert_eq!(
            model.config_v1(),
            NativePolicyValueModelConfigV1::contract_v1()
        );
        assert_eq!(model.parameter_count_v1(), PARAMETER_COUNT_V1);
        assert_eq!(model.parameter_count_v1(), fixture.parameter_manifest.count);
        assert_eq!(
            model.parameter_manifest_sha256_v1(),
            fixture.parameter_manifest.sha256
        );
        assert_eq!(
            model
                .parameter_manifest_sha256_raw_v1()
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>(),
            model.parameter_manifest_sha256_v1()
        );

        let mut actual = Vec::new();
        model.visit_parameters_v1(|name, shape, values| {
            actual.push((
                name.to_owned(),
                shape.to_vec(),
                values.len(),
                format!("0x{:08x}", values[0].to_bits()),
                format!("0x{:08x}", values[values.len() - 1].to_bits()),
            ));
        });
        let expected: Vec<_> = fixture
            .parameter_manifest
            .ordered
            .into_iter()
            .map(|parameter| {
                (
                    parameter.name,
                    parameter.shape,
                    parameter.count,
                    parameter.first_bits,
                    parameter.last_bits,
                )
            })
            .collect();
        assert_eq!(actual, expected);
    }

    #[test]
    fn cpu_forward_reproduces_torch_authority_goldens_with_declared_tolerance() {
        let fixture = fixture();
        let model =
            NativePolicyValueNetV1::runner_fixed_v1(NativePolicyValueModelConfigV1::contract_v1())
                .expect("model builds");
        assert_eq!(fixture.cases.len(), 2);
        for case in &fixture.cases {
            let output = model.forward_v1(view(case)).expect("golden forwards");
            assert_eq!(
                output.logits.len(),
                case.torch_logits.len(),
                "{}",
                case.name
            );
            for (actual, expected) in output.logits.iter().zip(&case.torch_logits) {
                assert_close(
                    *actual,
                    *expected,
                    fixture.authority.absolute_tolerance,
                    fixture.authority.relative_tolerance,
                );
            }
            assert_close(
                output.value,
                case.torch_value,
                fixture.authority.absolute_tolerance,
                fixture.authority.relative_tolerance,
            );
        }
    }

    #[test]
    fn goldens_cover_empty_and_ordered_topologies_and_full_token_boundary() {
        let fixture = fixture();
        let empty = &fixture.cases[0];
        assert!(empty.edge_features.is_empty());
        assert!(empty.action_ref_features.is_empty());
        assert!(empty.object_card_ids.contains(&0));
        assert!(empty.object_card_ids.contains(&65_536));
        assert!(empty.object_groups.contains(&0));
        assert!(empty.object_groups.contains(&19));
        assert_eq!(empty.action_features.len() / ACTION_FEATURE_DIM_V1, 2);

        let ordered = &fixture.cases[1];
        assert_eq!(ordered.edge_source_indices.len(), 3);
        assert_eq!(ordered.action_ref_card_ids.len(), 4);
        assert!(ordered.object_card_ids.contains(&0));
        assert!(ordered.object_card_ids.contains(&65_536));
        assert!(ordered.action_ref_card_ids.contains(&0));
        assert!(ordered.action_ref_card_ids.contains(&65_536));
        assert!(ordered
            .edge_source_indices
            .iter()
            .zip(&ordered.edge_target_indices)
            .any(|(source, target)| source == target));
        assert_eq!(ordered.object_groups[0], ordered.object_groups[1]);
        assert_eq!(ordered.action_features.len() / ACTION_FEATURE_DIM_V1, 3);
    }

    #[test]
    fn malformed_shapes_indices_tokens_groups_and_nonfinite_values_fail_closed() {
        let fixture = fixture();
        let case = &fixture.cases[1];
        let model =
            NativePolicyValueNetV1::runner_fixed_v1(NativePolicyValueModelConfigV1::contract_v1())
                .expect("model builds");

        let mut state = case.state.clone();
        state.pop();
        let bad = NativeEncodedDecisionViewV1 {
            state: &state,
            ..view(case)
        };
        assert!(matches!(
            model.forward_v1(bad),
            Err(NativePolicyValueErrorV1::ShapeMismatch { field: "state", .. })
        ));

        let bad = NativeEncodedDecisionViewV1 {
            object_features: &[],
            object_card_ids: &[],
            object_groups: &[],
            object_node_ids: &[],
            ..view(case)
        };
        assert_eq!(
            model.forward_v1(bad),
            Err(NativePolicyValueErrorV1::EmptyRows("object_features"))
        );

        let bad = NativeEncodedDecisionViewV1 {
            action_features: &[],
            ..view(case)
        };
        assert!(matches!(
            model.forward_v1(bad),
            Err(NativePolicyValueErrorV1::ShapeMismatch {
                field: "action_features",
                ..
            })
        ));

        let bad_tokens = [CARD_VOCAB_SIZE_V1 as i64];
        let bad = NativeEncodedDecisionViewV1 {
            object_card_ids: &bad_tokens,
            object_features: &case.object_features[..OBJECT_FEATURE_DIM_V1],
            object_groups: &case.object_groups[..1],
            object_node_ids: &case.object_node_ids[..1],
            ..view(case)
        };
        assert!(matches!(
            model.forward_v1(bad),
            Err(NativePolicyValueErrorV1::IndexOutOfRange {
                field: "object_card_ids",
                ..
            })
        ));

        let negative_ref_token = [-1];
        let bad = NativeEncodedDecisionViewV1 {
            action_ref_card_ids: &negative_ref_token,
            action_ref_features: &case.action_ref_features[..ACTION_REF_FEATURE_DIM_V1],
            action_ref_action_indices: &case.action_ref_action_indices[..1],
            action_ref_node_indices: &case.action_ref_node_indices[..1],
            ..view(case)
        };
        assert!(matches!(
            model.forward_v1(bad),
            Err(NativePolicyValueErrorV1::IndexOutOfRange {
                field: "action_ref_card_ids",
                ..
            })
        ));

        let bad_group = [OBJECT_GROUP_COUNT_V1 as i64, 3, 11];
        let bad = NativeEncodedDecisionViewV1 {
            object_groups: &bad_group,
            ..view(case)
        };
        assert!(matches!(
            model.forward_v1(bad),
            Err(NativePolicyValueErrorV1::IndexOutOfRange {
                field: "object_groups",
                ..
            })
        ));

        let bad_source = [case.object_card_ids.len() as i64, 2, 1];
        let bad = NativeEncodedDecisionViewV1 {
            edge_source_indices: &bad_source,
            ..view(case)
        };
        assert!(matches!(
            model.forward_v1(bad),
            Err(NativePolicyValueErrorV1::IndexOutOfRange {
                field: "edge_source_indices",
                ..
            })
        ));

        let bad_action_ref_action = [3, 0, 1, 2];
        let bad = NativeEncodedDecisionViewV1 {
            action_ref_action_indices: &bad_action_ref_action,
            ..view(case)
        };
        assert!(matches!(
            model.forward_v1(bad),
            Err(NativePolicyValueErrorV1::IndexOutOfRange {
                field: "action_ref_action_indices",
                ..
            })
        ));

        let bad_action_ref_node = [3, 0, 1, 2];
        let bad = NativeEncodedDecisionViewV1 {
            action_ref_node_indices: &bad_action_ref_node,
            ..view(case)
        };
        assert!(matches!(
            model.forward_v1(bad),
            Err(NativePolicyValueErrorV1::IndexOutOfRange {
                field: "action_ref_node_indices",
                ..
            })
        ));

        let bad_nodes = [0, 2, 1];
        let bad = NativeEncodedDecisionViewV1 {
            object_node_ids: &bad_nodes,
            ..view(case)
        };
        assert!(matches!(
            model.forward_v1(bad),
            Err(NativePolicyValueErrorV1::NonContiguousNodeId { .. })
        ));

        let mut nonfinite = case.action_features.clone();
        nonfinite[7] = f32::NAN;
        let bad = NativeEncodedDecisionViewV1 {
            action_features: &nonfinite,
            ..view(case)
        };
        assert!(matches!(
            model.forward_v1(bad),
            Err(NativePolicyValueErrorV1::NonFinite {
                field: "action_features",
                position: 7
            })
        ));

        let mut bad_schema = NativeEncodedDecisionSchemaV1::contract_v1();
        bad_schema.version = "wrong";
        let bad = NativeEncodedDecisionViewV1 {
            schema: bad_schema,
            ..view(case)
        };
        assert_eq!(
            model.forward_v1(bad),
            Err(NativePolicyValueErrorV1::SchemaMismatch("version"))
        );
    }

    #[test]
    fn caller_owned_outputs_are_transactional_on_input_and_output_failure() {
        let fixture = fixture();
        let case = &fixture.cases[1];
        let mut model =
            NativePolicyValueNetV1::runner_fixed_v1(NativePolicyValueModelConfigV1::contract_v1())
                .expect("model builds");
        let mut logits = vec![91.0, 92.0];
        let mut value = 93.0;

        model
            .forward_into_v1(view(case), &mut logits, &mut value)
            .expect("valid outputs commit together");
        assert_eq!(logits.len(), 3);
        assert_ne!(value, 93.0);
        logits = vec![91.0, 92.0];
        value = 93.0;

        let mut nonfinite = case.state.clone();
        nonfinite[0] = f32::INFINITY;
        let bad = NativeEncodedDecisionViewV1 {
            state: &nonfinite,
            ..view(case)
        };
        assert!(model.forward_into_v1(bad, &mut logits, &mut value).is_err());
        assert_eq!(logits, vec![91.0, 92.0]);
        assert_eq!(value, 93.0);

        model.scorer_second.bias[0] = f32::NAN;
        assert!(matches!(
            model.forward_into_v1(view(case), &mut logits, &mut value),
            Err(NativePolicyValueErrorV1::NonFiniteOutput {
                field: "logits",
                position: 0
            })
        ));
        assert_eq!(logits, vec![91.0, 92.0]);
        assert_eq!(value, 93.0);
    }

    #[test]
    fn parameter_snapshot_replacement_commits_once_and_rejects_corruption_transactionally() {
        fn assert_rejected_without_drift(
            model: &mut NativePolicyValueNetV1,
            replacement: Vec<NativeNamedParameterV1>,
        ) {
            let before_digest = model.parameter_manifest_sha256_v1();
            let before_snapshot = model.parameter_snapshot_v1();
            assert!(model.replace_parameter_snapshot_v1(&replacement).is_err());
            assert_eq!(model.parameter_manifest_sha256_v1(), before_digest);
            assert_eq!(model.parameter_snapshot_v1(), before_snapshot);
        }

        let mut model =
            NativePolicyValueNetV1::runner_fixed_v1(NativePolicyValueModelConfigV1::contract_v1())
                .expect("model builds");
        let initial_digest = model.parameter_manifest_sha256_v1();
        let mut replacement = model.parameter_snapshot_v1();
        for (ordinal, parameter) in replacement.iter_mut().enumerate() {
            let position = if ordinal == 0 {
                CARD_EMBEDDING_DIM_V1
            } else {
                0
            };
            parameter.values[position] += (ordinal + 1) as f32 / 10_000.0;
        }
        model
            .replace_parameter_snapshot_v1(&replacement)
            .expect("complete valid replacement commits");
        assert_eq!(model.parameter_snapshot_v1(), replacement);
        assert_ne!(model.parameter_manifest_sha256_v1(), initial_digest);

        let baseline = model.parameter_snapshot_v1();

        let mut bad_name = baseline.clone();
        bad_name[1].name = "wrong.weight";
        assert_rejected_without_drift(&mut model, bad_name);

        let mut bad_shape = baseline.clone();
        bad_shape[1].shape[0] += 1;
        assert_rejected_without_drift(&mut model, bad_shape);

        let mut bad_count = baseline.clone();
        bad_count[1].values.pop();
        assert_rejected_without_drift(&mut model, bad_count);

        let mut nonfinite = baseline.clone();
        nonfinite[1].values[0] = f32::NAN;
        assert_rejected_without_drift(&mut model, nonfinite);

        let mut nonzero_padding = baseline;
        nonzero_padding[0].values[0] = 1.0;
        assert_rejected_without_drift(&mut model, nonzero_padding);
    }

    #[test]
    fn config_drift_is_rejected_before_parameter_allocation() {
        let mut config = NativePolicyValueModelConfigV1::contract_v1();
        config.hidden_dim += 1;
        assert_eq!(
            NativePolicyValueNetV1::runner_fixed_v1(config).unwrap_err(),
            NativePolicyValueErrorV1::ModelConfigMismatch("hidden_dim")
        );
    }
}
