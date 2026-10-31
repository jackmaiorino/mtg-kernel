//! Function-preserving conversion of a net-8 train state into the V8
//! inclusion net (collab LANES/w4-combat-20261030/DESIGN.md v5.1, section 5;
//! the net is `native_policy_value_net_v1::v8_inclusion_v1`), on the
//! widening precedent (`widening_v1.rs`).
//!
//! Every V8 coordinate falls in exactly one [`V8CoordinateClassV1`]:
//! - `Old`: the image of a net-8 coordinate. Its parameter and both Adam
//!   moments are copied bit for bit. In `action_encoder.0.weight` (`[64,
//!   259]` to `[64, 260]`) old column `c < 195` stays at `c` and the pooled
//!   action-ref columns `c >= 195` move to `c + 1`.
//! - `NewIncludeColumn`: column 195 of `action_encoder.0.weight`, which reads
//!   the include bit. +0.
//! - `NewPairingFanIn`: `pairing_encoder.0.weight`. Drawn from the init law,
//!   so the pairing path's first layer has a gradient once its output
//!   projection moves.
//! - `NewPairingBias`: both pairing biases. +0.
//! - `NewPairingOutput`: the output projection `pairing_encoder.2.weight`.
//!   +0, so every pairing message is +0 and the guarded add leaves every old
//!   value unchanged: the converted net computes the parent's function.
//!
//! Every new moment is +0, and `adam_step` and the gauge anchor are kept: the
//! optimizer is unchanged, so a new coordinate's first update is Adam's with
//! zero moments at the inherited step (widening_v1's step-age handling,
//! measured by the `#[ignore]`d probe in the tests).
//!
//! **Init law and draw order.** The only law is `torch_default_uniform_v1`
//! (`widening_v1::WideningInitLawV1`), with `fan_in` 128, the pairing input
//! width. One SplitMix64 stream, seeded with the recorded seed, is drawn once
//! per `NewPairingFanIn` coordinate in row-major order and never otherwise.

use super::widening_v1::{hex_sha256_v1, WideningErrorV1, WideningInitLawV1};
use super::*;
use crate::native_policy_value_net_v1::v8_inclusion_v1::{
    V8_INCLUDE_COLUMN_V1, V8_PAIRING_INPUT_DIM_V1,
};
use crate::state::SplitMix64;
use serde::Serialize;

/// Version of the V8 conversion; part of its provenance.
pub(crate) const V8_CONVERSION_CONSTRUCTION_VERSION_V1: &str =
    "net8-to-net8-v8-inclusion-function-preserving-conversion-v1";

/// What one V8 coordinate is in the conversion (module documentation).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub(crate) enum V8CoordinateClassV1 {
    Old,
    NewIncludeColumn,
    NewPairingFanIn,
    NewPairingBias,
    NewPairingOutput,
}

impl V8CoordinateClassV1 {
    pub(crate) const ALL: [Self; 5] = [
        Self::Old,
        Self::NewIncludeColumn,
        Self::NewPairingFanIn,
        Self::NewPairingBias,
        Self::NewPairingOutput,
    ];

    pub(crate) fn name_v1(self) -> &'static str {
        match self {
            Self::Old => "old",
            Self::NewIncludeColumn => "new_include_column",
            Self::NewPairingFanIn => "new_pairing_fan_in",
            Self::NewPairingBias => "new_pairing_bias",
            Self::NewPairingOutput => "new_pairing_output",
        }
    }
}

/// The class of every coordinate of V8 tensor `tensor` (0 to 36), in
/// row-major order.
pub(crate) fn v8_coordinate_classes_v1(tensor: usize) -> Vec<V8CoordinateClassV1> {
    use V8CoordinateClassV1::*;
    let layout = parameter_layout_dims_v1::<Net8DimsV1>(true);
    let count = layout[tensor].1.iter().product::<usize>();
    let class = match tensor {
        PAIRING_FIRST_WEIGHT_V1 => NewPairingFanIn,
        PAIRING_FIRST_BIAS_V1 | PAIRING_SECOND_BIAS_V1 => NewPairingBias,
        PAIRING_SECOND_WEIGHT_V1 => NewPairingOutput,
        ACTION_FIRST_WEIGHT => {
            let width = V8_ACTION_FIRST_WEIGHT_SHAPE_V1[1];
            return (0..count)
                .map(|index| {
                    if index % width == V8_INCLUDE_COLUMN_V1 {
                        NewIncludeColumn
                    } else {
                        Old
                    }
                })
                .collect();
        }
        _ => Old,
    };
    vec![class; count]
}

/// The V8 element a net-8 element occupies.
pub(crate) fn net8_to_v8_index_v1(tensor: usize, index: usize) -> usize {
    if tensor != ACTION_FIRST_WEIGHT {
        return index;
    }
    let width = EXPECTED_PARAMETER_SHAPES[ACTION_FIRST_WEIGHT][1];
    let (row, column) = (index / width, index % width);
    let column = if column < V8_INCLUDE_COLUMN_V1 {
        column
    } else {
        column + 1
    };
    row * V8_ACTION_FIRST_WEIGHT_SHAPE_V1[1] + column
}

/// Coordinate counts of one V8 tensor by class.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct V8TensorClassesV1 {
    pub(crate) name: &'static str,
    pub(crate) shape: Vec<usize>,
    pub(crate) counts: std::collections::BTreeMap<&'static str, u64>,
}

/// Provenance of one conversion, recorded with the converted state.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct V8ConversionProvenanceV1 {
    pub(crate) construction_version: &'static str,
    pub(crate) parent_architecture: &'static str,
    pub(crate) child_architecture: &'static str,
    /// Hex `state_sha256` of the net-8 parent snapshot.
    pub(crate) parent_state_sha256: String,
    /// Hex `state_sha256` of the converted state.
    pub(crate) child_state_sha256: String,
    pub(crate) init_law: &'static str,
    pub(crate) seed: u64,
    /// Kept from the parent.
    pub(crate) adam_step: u64,
    /// SplitMix64 draws made (one per `NewPairingFanIn` coordinate).
    pub(crate) drawn_coordinate_count: u64,
    pub(crate) tensors: Vec<V8TensorClassesV1>,
}

#[derive(Clone, Debug)]
pub(crate) struct ConvertedV8TrainStateV1 {
    pub(crate) state: NativePolicyValueTrainStateV1,
    pub(crate) provenance: V8ConversionProvenanceV1,
}

/// The converted snapshot of a net-8 snapshot, without validation, and the
/// number of draws made.
pub(crate) fn v8_snapshot_v1(
    parent: &NativePolicyValueTrainSnapshotV1,
    law: WideningInitLawV1,
    seed: u64,
) -> (NativePolicyValueTrainSnapshotV1, u64) {
    let mut rng = SplitMix64::seed(seed);
    let mut drawn = 0u64;
    let layout = parameter_layout_dims_v1::<Net8DimsV1>(true);
    let mut parameters = Vec::with_capacity(layout.len());
    let mut first_moments = Vec::with_capacity(layout.len());
    let mut second_moments = Vec::with_capacity(layout.len());
    for (tensor, &(name, shape)) in layout.iter().enumerate() {
        let classes = v8_coordinate_classes_v1(tensor);
        let mut values = vec![0.0f32; classes.len()];
        for (value, class) in values.iter_mut().zip(&classes) {
            if *class == V8CoordinateClassV1::NewPairingFanIn {
                *value = law.draw_v1(&mut rng, V8_PAIRING_INPUT_DIM_V1);
                drawn += 1;
            }
        }
        let mut first = vec![0.0f32; classes.len()];
        let mut second = vec![0.0f32; classes.len()];
        if tensor < PARAMETER_TENSOR_COUNT {
            for (index, &value) in parent.parameters[tensor].values.iter().enumerate() {
                let target = net8_to_v8_index_v1(tensor, index);
                values[target] = value;
                first[target] = parent.first_moments[tensor].values[index];
                second[target] = parent.second_moments[tensor].values[index];
            }
        }
        let named = |values: Vec<f32>| NativeNamedParameterV1 {
            name,
            shape: shape.to_vec(),
            values,
        };
        parameters.push(named(values));
        first_moments.push(named(first));
        second_moments.push(named(second));
    }
    (
        NativePolicyValueTrainSnapshotV1 {
            adam_step: parent.adam_step,
            scorer_bias_anchor_bits: parent.scorer_bias_anchor_bits,
            parameters,
            first_moments,
            second_moments,
        },
        drawn,
    )
}

/// Converts a net-8 train snapshot into a V8 inclusion train state by the
/// versioned construction of the module documentation. `init_law` must be
/// `torch_default_uniform_v1`; the parent must validate as net-8.
pub(crate) fn convert_net8_snapshot_to_v8_v1(
    parent: &NativePolicyValueTrainSnapshotV1,
    init_law: &str,
    seed: u64,
) -> Result<ConvertedV8TrainStateV1, WideningErrorV1> {
    let law = WideningInitLawV1::from_name_v1(init_law)?;
    if parent.parameters.len() != PARAMETER_TENSOR_COUNT {
        return Err(WideningErrorV1::ParentNotNet8(
            NativePolicyTrainErrorV1::ParameterManifest,
        ));
    }
    let parent_state_sha256 = parent
        .state_sha256_v1()
        .map_err(WideningErrorV1::ParentNotNet8)?;
    let (snapshot, drawn_coordinate_count) = v8_snapshot_v1(parent, law, seed);
    let mut template = NativePolicyValueNetV1::v8_inclusion_template_v1()
        .map_err(|error| WideningErrorV1::Construction(error.into()))?;
    template
        .replace_parameter_snapshot_v1(&snapshot.parameters)
        .map_err(|error| WideningErrorV1::Construction(error.into()))?;
    let state = NativePolicyValueTrainStateV1::from_snapshot_v1(template, &snapshot)
        .map_err(WideningErrorV1::Construction)?;
    let child_state_sha256 = state
        .state_sha256_v1()
        .map_err(WideningErrorV1::Construction)?;
    let tensors = snapshot
        .parameters
        .iter()
        .enumerate()
        .map(|(tensor, parameter)| {
            let mut counts = std::collections::BTreeMap::new();
            for class in v8_coordinate_classes_v1(tensor) {
                *counts.entry(class.name_v1()).or_insert(0u64) += 1;
            }
            V8TensorClassesV1 {
                name: parameter.name,
                shape: parameter.shape.clone(),
                counts,
            }
        })
        .collect();
    Ok(ConvertedV8TrainStateV1 {
        provenance: V8ConversionProvenanceV1 {
            construction_version: V8_CONVERSION_CONSTRUCTION_VERSION_V1,
            parent_architecture: Net8DimsV1::ARCHITECTURE,
            child_architecture: V8_MODEL_ARCHITECTURE_VERSION_V1,
            parent_state_sha256: hex_sha256_v1(parent_state_sha256),
            child_state_sha256: hex_sha256_v1(child_state_sha256),
            init_law: law.name_v1(),
            seed,
            adam_step: parent.adam_step,
            drawn_coordinate_count,
            tensors,
        },
        state,
    })
}
