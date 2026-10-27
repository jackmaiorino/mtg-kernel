//! Function-preserving widening of a net-8 train state into net-8w128, and
//! the w32 sensitivity control (collab
//! LANES/throughput-architecture-20261020/WIDE-BACKWARD-DESIGN.md v2.1,
//! section 3 PR 4a, sections 5.6, 7.6 and 7.7; the construction is the B
//! study memo's: new hidden units get random fan-in and zero fan-out, new
//! embedding columns are zero with random fan-out).
//!
//! Every wide coordinate falls in exactly one [`WideCoordinateClassV1`]:
//! - `Old`: the image of a net-8 coordinate under [`narrow_to_wide_index_v1`];
//!   its parameter and both Adam moments are copied bit for bit.
//! - `NewFanIn`: a weight in a new output row (a new hidden unit's incoming
//!   weights, from old and new inputs alike). Drawn from the init law.
//! - `NewBias`: a new hidden unit's bias. +0.
//! - `NewFanOut`: an old row's weight from a new hidden unit. +0, so no old
//!   unit sees a new one, which preserves the function exactly.
//! - `NewEmbeddingColumn`: a new card-embedding column (every row, including
//!   the padding row). +0.
//! - `NewEmbeddingFanOut`: an old object-encoder row's weight from a new
//!   embedding column. Drawn from the init law; the column is +0, so it
//!   contributes nothing until the column moves.
//!
//! A weight that is both a new unit's fan-out and a new unit's fan-in (new
//! row, new column) is `NewFanIn`: the receiving unit is new, so drawing it
//! cannot change any old output. Every `New*` moment is +0, and `adam_step`
//! and the gauge anchor are kept.
//!
//! **Init law and draw order.** The only law is `torch_default_uniform_v1`,
//! Torch's default `nn.Linear` weight bound, `U(-1/sqrt(fan_in),
//! 1/sqrt(fan_in))`, where `fan_in` is the wide layer's input width. One
//! SplitMix64 stream, seeded with the recorded seed, is drawn in parameter
//! ordinal order (0 to 32), then row-major wide index order within a tensor,
//! once per `NewFanIn` or `NewEmbeddingFanOut` coordinate and never for any
//! other. One draw is `u = (next_u64 >> 40) / 2^24` in `[0, 1)`, then
//! `((2u - 1) * (1 / sqrt(fan_in)))` in f64, rounded once to f32.
//!
//! The screen driver and the widened RT1 source (PR 4b) call these entry
//! points; until they land only tests do.
#![cfg_attr(not(test), allow(dead_code))]

use super::*;
use crate::state::SplitMix64;
use serde::Serialize;

/// Version of the widening construction; part of its provenance.
pub(crate) const WIDENING_CONSTRUCTION_VERSION_V1: &str =
    "net8-to-net8w128-function-preserving-widening-v1";
/// Version of the w32 control construction; part of its provenance.
pub(crate) const W32_CONTROL_CONSTRUCTION_VERSION_V1: &str =
    "net8-w32-upper-half-fan-out-zeroed-control-v1";
/// The w32 rule, recorded verbatim in its provenance.
pub(crate) const W32_CONTROL_RULE_V1: &str = "in every hidden space of net-8 the upper half of \
     the units (32 to 63) has every fan-out weight and both Adam moments of those weights set \
     to +0; every other parameter, moment, adam_step and the gauge anchor are kept bit for bit";
/// The name of the one accepted init law (design section 7.7).
pub(crate) const TORCH_DEFAULT_UNIFORM_V1: &str = "torch_default_uniform_v1";

/// The widening init law. B's design fixes the law; until it does, this is
/// the only value accepted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WideningInitLawV1 {
    TorchDefaultUniformV1,
}

impl WideningInitLawV1 {
    pub(crate) fn from_name_v1(name: &str) -> Result<Self, WideningErrorV1> {
        match name {
            TORCH_DEFAULT_UNIFORM_V1 => Ok(Self::TorchDefaultUniformV1),
            other => Err(WideningErrorV1::UnsupportedInitLaw {
                requested: other.to_owned(),
            }),
        }
    }

    pub(crate) fn name_v1(self) -> &'static str {
        match self {
            Self::TorchDefaultUniformV1 => TORCH_DEFAULT_UNIFORM_V1,
        }
    }

    /// The law's bound for a layer of input width `fan_in`.
    pub(crate) fn bound_v1(self, fan_in: usize) -> f64 {
        match self {
            Self::TorchDefaultUniformV1 => 1.0 / (fan_in as f64).sqrt(),
        }
    }

    /// One draw, as the module documentation defines it.
    pub(crate) fn draw_v1(self, rng: &mut SplitMix64, fan_in: usize) -> f32 {
        let unit = (rng.next_u64() >> 40) as f64 / (1u64 << 24) as f64;
        ((2.0 * unit - 1.0) * self.bound_v1(fan_in)) as f32
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum WideningErrorV1 {
    /// Only net-8w128 is a widening target.
    UnsupportedTargetArchitecture { requested: String },
    /// Only `torch_default_uniform_v1` is accepted.
    UnsupportedInitLaw { requested: String },
    /// The parent snapshot is not a valid net-8 train state.
    ParentNotNet8(NativePolicyTrainErrorV1),
    /// The constructed state failed its own validation (a construction bug).
    Construction(NativePolicyTrainErrorV1),
}

impl Display for WideningErrorV1 {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "native widening v1 error: {self:?}")
    }
}

impl Error for WideningErrorV1 {}

// ---------------------------------------------------------------------------
// Narrow-to-wide layout map (design sections 2.4 and 5.2).
// ---------------------------------------------------------------------------

/// One concatenation segment of a layer input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SegmentV1 {
    Fixed(usize),
    Hidden,
    Embedding,
}

/// The input segments of each tensor's columns; biases have none.
pub(crate) fn column_segments_v1(tensor: usize) -> Vec<SegmentV1> {
    use SegmentV1::{Embedding, Fixed, Hidden};
    match tensor {
        CARD_EMBEDDING => vec![Embedding],
        OBJECT_FIRST_WEIGHT => vec![Fixed(OBJECT_FEATURE_DIM_V1), Embedding],
        EDGE_FIRST_WEIGHT => vec![Fixed(EDGE_FEATURE_DIM_V1), Hidden, Hidden],
        NODE_FIRST_WEIGHT | SCORER_FIRST_WEIGHT => vec![Hidden, Hidden],
        STATE_FIRST_WEIGHT => std::iter::once(Fixed(STATE_DIM_V1))
            .chain(std::iter::repeat_n(Hidden, OBJECT_GROUP_COUNT_V1))
            .collect(),
        ACTION_REF_FIRST_WEIGHT => vec![Fixed(ACTION_REF_FEATURE_DIM_V1), Hidden],
        ACTION_FIRST_WEIGHT => vec![Fixed(ACTION_FEATURE_DIM_V1), Hidden],
        tensor if EXPECTED_PARAMETER_SHAPES[tensor].len() == 2 => vec![Hidden],
        _ => Vec::new(),
    }
}

pub(crate) fn segment_width_v1(segment: SegmentV1, hidden: usize, embedding: usize) -> usize {
    match segment {
        SegmentV1::Fixed(width) => width,
        SegmentV1::Hidden => hidden,
        SegmentV1::Embedding => embedding,
    }
}

/// Narrow-to-wide layout map (design section 5.2, reused here): the wide
/// element a narrow element occupies when net-8 is embedded in net-8w128.
pub(crate) fn narrow_to_wide_index_v1(tensor: usize, index: usize) -> usize {
    let narrow_shape = EXPECTED_PARAMETER_SHAPES[tensor];
    let wide_shape = W_EXPECTED_PARAMETER_SHAPES[tensor];
    if narrow_shape.len() == 1 {
        return index;
    }
    let (row, mut column) = (index / narrow_shape[1], index % narrow_shape[1]);
    let mut wide_offset = 0;
    for segment in column_segments_v1(tensor) {
        let narrow_width = segment_width_v1(segment, HIDDEN_DIM_V1, CARD_EMBEDDING_DIM_V1);
        if column < narrow_width {
            return row * wide_shape[1] + wide_offset + column;
        }
        column -= narrow_width;
        wide_offset += segment_width_v1(segment, W_HIDDEN_DIM_V1, W_CARD_EMBEDDING_DIM_V1);
    }
    unreachable!("column inside the narrow layout")
}

/// What one wide coordinate is in the widening (module documentation).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum WideCoordinateClassV1 {
    Old,
    NewFanIn,
    NewBias,
    NewFanOut,
    NewEmbeddingColumn,
    NewEmbeddingFanOut,
}

impl WideCoordinateClassV1 {
    pub(crate) const ALL: [Self; 6] = [
        Self::Old,
        Self::NewFanIn,
        Self::NewBias,
        Self::NewFanOut,
        Self::NewEmbeddingColumn,
        Self::NewEmbeddingFanOut,
    ];

    pub(crate) fn name_v1(self) -> &'static str {
        match self {
            Self::Old => "old",
            Self::NewFanIn => "new_fan_in",
            Self::NewBias => "new_bias",
            Self::NewFanOut => "new_fan_out",
            Self::NewEmbeddingColumn => "new_embedding_column",
            Self::NewEmbeddingFanOut => "new_embedding_fan_out",
        }
    }

    /// Whether the init law draws this coordinate.
    pub(crate) fn is_drawn_v1(self) -> bool {
        matches!(self, Self::NewFanIn | Self::NewEmbeddingFanOut)
    }
}

/// The class of every wide coordinate of `tensor`, in row-major order.
pub(crate) fn wide_coordinate_classes_v1(tensor: usize) -> Vec<WideCoordinateClassV1> {
    use WideCoordinateClassV1::*;
    let narrow_shape = EXPECTED_PARAMETER_SHAPES[tensor];
    let wide_shape = W_EXPECTED_PARAMETER_SHAPES[tensor];
    if wide_shape.len() == 1 {
        return (0..wide_shape[0])
            .map(|index| {
                if index < narrow_shape[0] {
                    Old
                } else {
                    NewBias
                }
            })
            .collect();
    }
    let columns: Vec<WideCoordinateClassV1> = column_segments_v1(tensor)
        .into_iter()
        .flat_map(|segment| {
            let narrow_width = segment_width_v1(segment, HIDDEN_DIM_V1, CARD_EMBEDDING_DIM_V1);
            let wide_width = segment_width_v1(segment, W_HIDDEN_DIM_V1, W_CARD_EMBEDDING_DIM_V1);
            let new = match (tensor, segment) {
                (CARD_EMBEDDING, _) => NewEmbeddingColumn,
                (_, SegmentV1::Embedding) => NewEmbeddingFanOut,
                (_, SegmentV1::Hidden) => NewFanOut,
                (_, SegmentV1::Fixed(_)) => unreachable!("fixed segments do not widen"),
            };
            (0..wide_width).map(move |offset| if offset < narrow_width { Old } else { new })
        })
        .collect();
    debug_assert_eq!(columns.len(), wide_shape[1]);
    (0..wide_shape[0])
        .flat_map(|row| {
            let new_row = tensor != CARD_EMBEDDING && row >= narrow_shape[0];
            columns
                .iter()
                .map(move |&column| if new_row { NewFanIn } else { column })
        })
        .collect()
}

// ---------------------------------------------------------------------------
// The widening.
// ---------------------------------------------------------------------------

/// Provenance of one widening, recorded with the widened state.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct WideningProvenanceV1 {
    pub(crate) construction_version: &'static str,
    pub(crate) parent_architecture: &'static str,
    pub(crate) child_architecture: &'static str,
    /// Hex `state_sha256` of the net-8 parent snapshot.
    pub(crate) parent_state_sha256: String,
    /// Hex `state_sha256` of the widened state.
    pub(crate) child_state_sha256: String,
    pub(crate) init_law: &'static str,
    pub(crate) seed: u64,
    /// Kept from the parent.
    pub(crate) adam_step: u64,
    /// SplitMix64 draws made (one per drawn coordinate).
    pub(crate) drawn_coordinate_count: u64,
}

#[derive(Clone, Debug)]
pub(crate) struct WidenedTrainStateV1 {
    pub(crate) state: NativePolicyValueTrainStateWideV1,
    pub(crate) provenance: WideningProvenanceV1,
}

pub(crate) fn hex_sha256_v1(digest: [u8; 32]) -> String {
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// The widened snapshot of a net-8 snapshot, without validation, and the
/// number of draws made.
pub(crate) fn widened_snapshot_v1(
    parent: &NativePolicyValueTrainSnapshotV1,
    law: WideningInitLawV1,
    seed: u64,
) -> (NativePolicyValueTrainSnapshotV1, u64) {
    let mut rng = SplitMix64::seed(seed);
    let mut drawn = 0u64;
    let mut parameters = Vec::with_capacity(PARAMETER_TENSOR_COUNT);
    let mut first_moments = Vec::with_capacity(PARAMETER_TENSOR_COUNT);
    let mut second_moments = Vec::with_capacity(PARAMETER_TENSOR_COUNT);
    for tensor in 0..PARAMETER_TENSOR_COUNT {
        let wide_shape = W_EXPECTED_PARAMETER_SHAPES[tensor];
        let fan_in = if wide_shape.len() == 2 {
            wide_shape[1]
        } else {
            0
        };
        let classes = wide_coordinate_classes_v1(tensor);
        let mut values = vec![0.0f32; classes.len()];
        for (value, class) in values.iter_mut().zip(&classes) {
            if class.is_drawn_v1() {
                *value = law.draw_v1(&mut rng, fan_in);
                drawn += 1;
            }
        }
        let mut first = vec![0.0f32; classes.len()];
        let mut second = vec![0.0f32; classes.len()];
        for (index, &value) in parent.parameters[tensor].values.iter().enumerate() {
            let wide = narrow_to_wide_index_v1(tensor, index);
            values[wide] = value;
            first[wide] = parent.first_moments[tensor].values[index];
            second[wide] = parent.second_moments[tensor].values[index];
        }
        let named = |values: Vec<f32>| NativeNamedParameterV1 {
            name: EXPECTED_PARAMETER_NAMES[tensor],
            shape: wide_shape.to_vec(),
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

/// Widens a net-8 train snapshot into a net-8w128 train state by the
/// versioned construction of the module documentation. `target_architecture`
/// must be `kernel-policy-value-net-8w128` and `init_law`
/// `torch_default_uniform_v1`; the parent must validate as net-8.
pub(crate) fn widen_net8_snapshot_v1(
    parent: &NativePolicyValueTrainSnapshotV1,
    target_architecture: &str,
    init_law: &str,
    seed: u64,
) -> Result<WidenedTrainStateV1, WideningErrorV1> {
    if target_architecture != Net8W128DimsV1::ARCHITECTURE {
        return Err(WideningErrorV1::UnsupportedTargetArchitecture {
            requested: target_architecture.to_owned(),
        });
    }
    let law = WideningInitLawV1::from_name_v1(init_law)?;
    let parent_state_sha256 = parent
        .state_sha256_v1()
        .map_err(WideningErrorV1::ParentNotNet8)?;
    let (snapshot, drawn_coordinate_count) = widened_snapshot_v1(parent, law, seed);
    let mut template = NativePolicyValueNetWideV1::runner_fixed_wide_v1(
        NativePolicyValueModelConfigV1::contract_wide_v1(),
    )
    .map_err(|error| WideningErrorV1::Construction(error.into()))?;
    template
        .replace_parameter_snapshot_wide_v1(&snapshot.parameters)
        .map_err(|error| WideningErrorV1::Construction(error.into()))?;
    let state = NativePolicyValueTrainStateWideV1::from_snapshot_wide_v1(template, &snapshot)
        .map_err(WideningErrorV1::Construction)?;
    let child_state_sha256 = state
        .state_sha256_v1()
        .map_err(WideningErrorV1::Construction)?;
    Ok(WidenedTrainStateV1 {
        provenance: WideningProvenanceV1 {
            construction_version: WIDENING_CONSTRUCTION_VERSION_V1,
            parent_architecture: Net8DimsV1::ARCHITECTURE,
            child_architecture: Net8W128DimsV1::ARCHITECTURE,
            parent_state_sha256: hex_sha256_v1(parent_state_sha256),
            child_state_sha256: hex_sha256_v1(child_state_sha256),
            init_law: law.name_v1(),
            seed,
            adam_step: parent.adam_step,
            drawn_coordinate_count,
        },
        state,
    })
}

// ---------------------------------------------------------------------------
// The w32 control.
// ---------------------------------------------------------------------------

/// One hidden space of net-8: its units and the `(tensor, segment ordinal)`
/// input segments that read them (the units' fan-out).
pub(crate) struct HiddenSpaceV1 {
    pub(crate) name: &'static str,
    pub(crate) fan_out: &'static [(usize, usize)],
}

/// Every hidden space of net-8 (python/mtg_kernel_rl/model.py): each
/// two-layer module's inner layer (after `.0` and tanh) and its output
/// (after `.2` and tanh), plus the scorer and value-head inner layers.
pub(crate) const HIDDEN_SPACES_V1: [HiddenSpaceV1; 14] = [
    HiddenSpaceV1 {
        name: "object_encoder.hidden",
        fan_out: &[(OBJECT_SECOND_WEIGHT, 0)],
    },
    HiddenSpaceV1 {
        name: "object_encoder.output",
        fan_out: &[
            (EDGE_FIRST_WEIGHT, 1),
            (EDGE_FIRST_WEIGHT, 2),
            (NODE_FIRST_WEIGHT, 0),
        ],
    },
    HiddenSpaceV1 {
        name: "edge_encoder.hidden",
        fan_out: &[(EDGE_SECOND_WEIGHT, 0)],
    },
    HiddenSpaceV1 {
        name: "edge_encoder.output",
        fan_out: &[(NODE_FIRST_WEIGHT, 1)],
    },
    HiddenSpaceV1 {
        name: "node_update.hidden",
        fan_out: &[(NODE_SECOND_WEIGHT, 0)],
    },
    HiddenSpaceV1 {
        name: "node_update.output",
        fan_out: &[
            (STATE_FIRST_WEIGHT, 1),
            (STATE_FIRST_WEIGHT, 2),
            (STATE_FIRST_WEIGHT, 3),
            (STATE_FIRST_WEIGHT, 4),
            (STATE_FIRST_WEIGHT, 5),
            (STATE_FIRST_WEIGHT, 6),
            (STATE_FIRST_WEIGHT, 7),
            (STATE_FIRST_WEIGHT, 8),
            (STATE_FIRST_WEIGHT, 9),
            (STATE_FIRST_WEIGHT, 10),
            (STATE_FIRST_WEIGHT, 11),
            (STATE_FIRST_WEIGHT, 12),
            (STATE_FIRST_WEIGHT, 13),
            (STATE_FIRST_WEIGHT, 14),
            (STATE_FIRST_WEIGHT, 15),
            (STATE_FIRST_WEIGHT, 16),
            (STATE_FIRST_WEIGHT, 17),
            (STATE_FIRST_WEIGHT, 18),
            (STATE_FIRST_WEIGHT, 19),
            (STATE_FIRST_WEIGHT, 20),
            (ACTION_REF_FIRST_WEIGHT, 1),
        ],
    },
    HiddenSpaceV1 {
        name: "state_encoder.hidden",
        fan_out: &[(STATE_SECOND_WEIGHT, 0)],
    },
    HiddenSpaceV1 {
        name: "state_encoder.output",
        fan_out: &[(SCORER_FIRST_WEIGHT, 0), (VALUE_FIRST_WEIGHT, 0)],
    },
    HiddenSpaceV1 {
        name: "action_ref_encoder.hidden",
        fan_out: &[(ACTION_REF_SECOND_WEIGHT, 0)],
    },
    HiddenSpaceV1 {
        name: "action_ref_encoder.output",
        fan_out: &[(ACTION_FIRST_WEIGHT, 1)],
    },
    HiddenSpaceV1 {
        name: "action_encoder.hidden",
        fan_out: &[(ACTION_SECOND_WEIGHT, 0)],
    },
    HiddenSpaceV1 {
        name: "action_encoder.output",
        fan_out: &[(SCORER_FIRST_WEIGHT, 1)],
    },
    HiddenSpaceV1 {
        name: "scorer.hidden",
        fan_out: &[(SCORER_SECOND_WEIGHT, 0)],
    },
    HiddenSpaceV1 {
        name: "value_head.hidden",
        fan_out: &[(VALUE_SECOND_WEIGHT, 0)],
    },
];

/// The narrow column of `unit` in segment `segment` of `tensor`.
pub(crate) fn narrow_segment_column_v1(tensor: usize, segment: usize, unit: usize) -> usize {
    let segments = column_segments_v1(tensor);
    assert!(
        segments[segment] == SegmentV1::Hidden && unit < HIDDEN_DIM_V1,
        "unit inside a hidden segment"
    );
    segments[..segment]
        .iter()
        .map(|&segment| segment_width_v1(segment, HIDDEN_DIM_V1, CARD_EMBEDDING_DIM_V1))
        .sum::<usize>()
        + unit
}

/// The units the w32 rule zeroes in every hidden space: the upper half.
pub(crate) fn w32_zeroed_units_v1() -> std::ops::Range<usize> {
    HIDDEN_DIM_V1 / 2..HIDDEN_DIM_V1
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct W32ZeroedSpaceV1 {
    pub(crate) space: &'static str,
    pub(crate) units: Vec<usize>,
    pub(crate) zeroed_weights: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct W32ControlProvenanceV1 {
    pub(crate) construction_version: &'static str,
    pub(crate) architecture: &'static str,
    pub(crate) rule: &'static str,
    pub(crate) parent_state_sha256: String,
    pub(crate) child_state_sha256: String,
    pub(crate) adam_step: u64,
    pub(crate) zeroed: Vec<W32ZeroedSpaceV1>,
    pub(crate) zeroed_weight_count: u64,
}

#[derive(Clone, Debug)]
pub(crate) struct W32ControlV1 {
    pub(crate) state: NativePolicyValueTrainStateV1,
    pub(crate) provenance: W32ControlProvenanceV1,
}

/// The w32 control of a net-8 snapshot (B memo: b512 with half its hidden
/// units' fan-out zeroed), by [`W32_CONTROL_RULE_V1`]. It changes the
/// function on purpose.
pub(crate) fn w32_control_from_net8_snapshot_v1(
    parent: &NativePolicyValueTrainSnapshotV1,
) -> Result<W32ControlV1, WideningErrorV1> {
    let parent_state_sha256 = parent
        .state_sha256_v1()
        .map_err(WideningErrorV1::ParentNotNet8)?;
    let mut snapshot = parent.clone();
    let units: Vec<usize> = w32_zeroed_units_v1().collect();
    let mut zeroed = Vec::with_capacity(HIDDEN_SPACES_V1.len());
    let mut zeroed_weight_count = 0u64;
    for space in &HIDDEN_SPACES_V1 {
        let mut zeroed_weights = 0u64;
        for &(tensor, segment) in space.fan_out {
            let columns = EXPECTED_PARAMETER_SHAPES[tensor][1];
            let rows = EXPECTED_PARAMETER_SHAPES[tensor][0];
            for &unit in &units {
                let column = narrow_segment_column_v1(tensor, segment, unit);
                for row in 0..rows {
                    let index = row * columns + column;
                    snapshot.parameters[tensor].values[index] = 0.0;
                    snapshot.first_moments[tensor].values[index] = 0.0;
                    snapshot.second_moments[tensor].values[index] = 0.0;
                    zeroed_weights += 1;
                }
            }
        }
        zeroed_weight_count += zeroed_weights;
        zeroed.push(W32ZeroedSpaceV1 {
            space: space.name,
            units: units.clone(),
            zeroed_weights,
        });
    }
    let mut template =
        NativePolicyValueNetV1::runner_fixed_v1(NativePolicyValueModelConfigV1::contract_v1())
            .map_err(|error| WideningErrorV1::Construction(error.into()))?;
    template
        .replace_parameter_snapshot_v1(&snapshot.parameters)
        .map_err(|error| WideningErrorV1::Construction(error.into()))?;
    let state = NativePolicyValueTrainStateV1::from_snapshot_v1(template, &snapshot)
        .map_err(WideningErrorV1::Construction)?;
    let child_state_sha256 = state
        .state_sha256_v1()
        .map_err(WideningErrorV1::Construction)?;
    Ok(W32ControlV1 {
        provenance: W32ControlProvenanceV1 {
            construction_version: W32_CONTROL_CONSTRUCTION_VERSION_V1,
            architecture: Net8DimsV1::ARCHITECTURE,
            rule: W32_CONTROL_RULE_V1,
            parent_state_sha256: hex_sha256_v1(parent_state_sha256),
            child_state_sha256: hex_sha256_v1(child_state_sha256),
            adam_step: parent.adam_step,
            zeroed,
            zeroed_weight_count,
        },
        state,
    })
}
