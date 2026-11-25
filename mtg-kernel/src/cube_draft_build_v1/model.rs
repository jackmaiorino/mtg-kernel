//! M4 task D2: the draft-and-build model's parameters, its seeded
//! initialiser and its forward passes (the pick head, the four-step build
//! head and the value).
//!
//! The forward is f32 throughout and bit-identical on every run, thread count
//! and x86-64 host:
//! - every layer starts each output at its bias (or at +0.0 without one) and
//!   adds `W[i][j] * x[j]` for j ascending, one rounded multiply and one
//!   rounded add per term, never fused (the play network's rule,
//!   `accumulate_linear_rows_v1`);
//! - every `tanh` is `portable-math-v1`'s
//!   ([`TrainingMathV1::PortableV1`]);
//! - every set mean adds card-hidden rows in ascending index row, then
//!   multiplies by `1.0 / n`; an empty set gives +0.0;
//! - parallelism is only over the independent rows of the card-hidden table,
//!   assembled in index order.
//!
//! At initialisation the last layer of every head is +0.0, so every logit and
//! the value are exactly +0.0 and every first decision is uniform. Nothing in
//! the model concerns lands except the data it reads.
//!
//! The masked softmax and the integer-count sampler that consume these logits
//! are not here (tasks D2a and D3). Logits come out as f32, in the canonical
//! orders those routines walk, with exactly the real candidates.

use std::fmt;
use std::ops::Range;

use sha2::{Digest, Sha256};

use crate::cube_draft_v1::{DECK_CARDS, PACK_SIZE, POOL_CARDS, ROUNDS};
use crate::state::SplitMix64;
use crate::training_math_v1::TrainingMathV1;

/// Width of a card's id row `E[r]` and of `P s(r)`.
pub const EMBEDDING_WIDTH_V1: usize = 16;
/// Width of every hidden layer, of the card hidden `h` and of each set mean.
pub const HIDDEN_WIDTH_V1: usize = 64;
/// Draft steps 15r + p, 0 to 44.
pub const STEP_COUNT_V1: usize = ROUNDS * PACK_SIZE;
/// Width of the pick state input x: three set means, the step one-hot and
/// the direction.
pub const STATE_INPUT_WIDTH_V1: usize = 3 * HIDDEN_WIDTH_V1 + STEP_COUNT_V1 + 1;
/// Columns of x that hold the history mean. They start at +0.0 in
/// `pick.state_encoder.0` and in `value.0`.
pub const HISTORY_COLUMNS_V1: Range<usize> = 2 * HIDDEN_WIDTH_V1..3 * HIDDEN_WIDTH_V1;
/// Column of x where the step one-hot begins.
pub const STEP_COLUMN_V1: usize = 3 * HIDDEN_WIDTH_V1;
/// Column of x that holds the direction.
pub const DIRECTION_COLUMN_V1: usize = STEP_COLUMN_V1 + STEP_COUNT_V1;
/// Largest seen-not-taken history: 276 cards seen by round 2, pick 8, minus
/// 38 picks.
pub const MAX_HISTORY_CARDS_V1: usize = 238;
/// Cards in a build's pool.
pub const BUILD_POOL_CARDS_V1: usize = POOL_CARDS;
/// Cards in a built deck.
pub const DECK_CARDS_V1: usize = DECK_CARDS as usize;
/// Values of the basics count B, 0 to 40.
pub const BASICS_COUNT_OPTIONS_V1: usize = DECK_CARDS_V1 + 1;
/// Basic land types W, U, B, R, G, type bit `1 << i` for the i-th.
pub const BASIC_TYPE_COUNT_V1: usize = 5;
/// Non-empty sets of basic land types, by 5-bit mask 1 to 31.
pub const TYPE_SET_COUNT_V1: usize = (1 << BASIC_TYPE_COUNT_V1) - 1;
/// Fan-in used for the uniform bound of both card embeddings E (bound
/// 1/sqrt(16) = 0.25). The design does not state an embedding's fan-in; this
/// is an open point for D0 to confirm.
pub const EMBEDDING_INIT_FAN_IN_V1: usize = EMBEDDING_WIDTH_V1;

const H: usize = HIDDEN_WIDTH_V1;
/// [q_j, h(c)] and [r, h(basic)]: the pointer, pick and split scorers.
const SCORER_INPUT_WIDTH: usize = 2 * H;
/// [g, m_j, B/40, j/40].
const POINTER_CONTEXT_INPUT_WIDTH: usize = 2 * H + 2;
/// [g, m, B/40].
const TYPESET_CONTEXT_INPUT_WIDTH: usize = 2 * H + 1;
/// [r, e_T, |T|/5].
const TYPESET_SCORER_INPUT_WIDTH: usize = 2 * H + 1;

const MATH: TrainingMathV1 = TrainingMathV1::PortableV1;

/// Why a model, table, state or build step was refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ModelErrorV1 {
    /// Widths, table binding or thread count.
    Shape(String),
    /// A pick state that is not a valid seat view.
    PickState(String),
    /// A build step's inputs that are not a valid point of the build.
    Build(String),
}

impl fmt::Display for ModelErrorV1 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Shape(message) => write!(f, "cube draft-build model shape: {message}"),
            Self::PickState(message) => write!(f, "cube draft-build pick state: {message}"),
            Self::Build(message) => write!(f, "cube draft-build build step: {message}"),
        }
    }
}

impl std::error::Error for ModelErrorV1 {}

fn shape_error(message: impl Into<String>) -> ModelErrorV1 {
    ModelErrorV1::Shape(message.into())
}

fn state_error(message: impl Into<String>) -> ModelErrorV1 {
    ModelErrorV1::PickState(message.into())
}

fn build_error(message: impl Into<String>) -> ModelErrorV1 {
    ModelErrorV1::Build(message.into())
}

/// The three parameter sets. Each has its own losses and Adam moments (D4).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ParameterSetV1 {
    Build,
    Pick,
    Value,
}

/// Which card trunk a card-hidden table is computed from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TrunkV1 {
    Build,
    Pick,
}

/// How a tensor is initialised.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InitRuleV1 {
    /// Every value +0.0; draws nothing from the stream.
    Zero,
    /// One draw per element in row-major order, uniform in plus or minus
    /// 1/sqrt(`fan_in`). The listed input columns of every row are then set
    /// to +0.0, after their draws, so the draw count never depends on them.
    Uniform {
        fan_in: usize,
        zero_columns: Option<Range<usize>>,
    },
}

/// One tensor of the layout: name, set, shape (`[out, in]` row-major for a
/// weight, `[out]` for a bias) and initialiser.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TensorSpecV1 {
    pub name: String,
    pub set: ParameterSetV1,
    pub shape: Vec<usize>,
    pub init: InitRuleV1,
}

impl TensorSpecV1 {
    pub fn element_count(&self) -> usize {
        self.shape.iter().product()
    }
}

/// A read-only view of one tensor, as the visitor gives it.
#[derive(Clone, Copy, Debug)]
pub struct TensorViewV1<'a> {
    pub name: &'a str,
    pub set: ParameterSetV1,
    pub shape: &'a [usize],
    pub values: &'a [f32],
}

/// The tensors in layout order. The discriminant is the position in the
/// layout; [`TensorIdV1::spec`] gives each one's name, set, shape and
/// initialiser, so the order is written once.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(usize)]
enum TensorIdV1 {
    BuildEmbedding,
    BuildProjection,
    BuildMlp0Weight,
    BuildMlp0Bias,
    BuildMlp2Weight,
    BuildMlp2Bias,
    PoolEncoder0Weight,
    PoolEncoder0Bias,
    PoolEncoder2Weight,
    PoolEncoder2Bias,
    CountHeadWeight,
    CountHeadBias,
    PointerContextWeight,
    PointerContextBias,
    PointerScorer0Weight,
    PointerScorer0Bias,
    PointerScorer2Weight,
    TypesetContextWeight,
    TypesetContextBias,
    TypesetScorer0Weight,
    TypesetScorer0Bias,
    TypesetScorer2Weight,
    SplitScorer0Weight,
    SplitScorer0Bias,
    SplitScorer2Weight,
    PickEmbedding,
    PickProjection,
    PickMlp0Weight,
    PickMlp0Bias,
    PickMlp2Weight,
    PickMlp2Bias,
    StateEncoder0Weight,
    StateEncoder0Bias,
    StateEncoder2Weight,
    StateEncoder2Bias,
    PickScorer0Weight,
    PickScorer0Bias,
    PickScorer2Weight,
    Value0Weight,
    Value0Bias,
    Value2Weight,
    Value2Bias,
    ValueOutWeight,
    ValueOutBias,
}

impl TensorIdV1 {
    const ALL: [TensorIdV1; 44] = [
        Self::BuildEmbedding,
        Self::BuildProjection,
        Self::BuildMlp0Weight,
        Self::BuildMlp0Bias,
        Self::BuildMlp2Weight,
        Self::BuildMlp2Bias,
        Self::PoolEncoder0Weight,
        Self::PoolEncoder0Bias,
        Self::PoolEncoder2Weight,
        Self::PoolEncoder2Bias,
        Self::CountHeadWeight,
        Self::CountHeadBias,
        Self::PointerContextWeight,
        Self::PointerContextBias,
        Self::PointerScorer0Weight,
        Self::PointerScorer0Bias,
        Self::PointerScorer2Weight,
        Self::TypesetContextWeight,
        Self::TypesetContextBias,
        Self::TypesetScorer0Weight,
        Self::TypesetScorer0Bias,
        Self::TypesetScorer2Weight,
        Self::SplitScorer0Weight,
        Self::SplitScorer0Bias,
        Self::SplitScorer2Weight,
        Self::PickEmbedding,
        Self::PickProjection,
        Self::PickMlp0Weight,
        Self::PickMlp0Bias,
        Self::PickMlp2Weight,
        Self::PickMlp2Bias,
        Self::StateEncoder0Weight,
        Self::StateEncoder0Bias,
        Self::StateEncoder2Weight,
        Self::StateEncoder2Bias,
        Self::PickScorer0Weight,
        Self::PickScorer0Bias,
        Self::PickScorer2Weight,
        Self::Value0Weight,
        Self::Value0Bias,
        Self::Value2Weight,
        Self::Value2Bias,
        Self::ValueOutWeight,
        Self::ValueOutBias,
    ];

    /// The four trunk tensors of each set (weights and biases), in the same
    /// order for both trunks.
    const BUILD_TRUNK: [TensorIdV1; 6] = [
        Self::BuildEmbedding,
        Self::BuildProjection,
        Self::BuildMlp0Weight,
        Self::BuildMlp0Bias,
        Self::BuildMlp2Weight,
        Self::BuildMlp2Bias,
    ];
    const PICK_TRUNK: [TensorIdV1; 6] = [
        Self::PickEmbedding,
        Self::PickProjection,
        Self::PickMlp0Weight,
        Self::PickMlp0Bias,
        Self::PickMlp2Weight,
        Self::PickMlp2Bias,
    ];

    fn index(self) -> usize {
        self as usize
    }

    fn spec(self, index_rows: usize, table_width: usize, dense_width: usize) -> TensorSpecV1 {
        use ParameterSetV1::{Build, Pick, Value};
        let card_input = EMBEDDING_WIDTH_V1 + dense_width;
        let uniform = |fan_in: usize| InitRuleV1::Uniform {
            fan_in,
            zero_columns: None,
        };
        let history_zeroed = InitRuleV1::Uniform {
            fan_in: STATE_INPUT_WIDTH_V1,
            zero_columns: Some(HISTORY_COLUMNS_V1),
        };
        let weight = |out: usize, input: usize| vec![out, input];
        let (name, set, shape, init) = match self {
            Self::BuildEmbedding => (
                "build_trunk.card_embedding.weight",
                Build,
                weight(index_rows, EMBEDDING_WIDTH_V1),
                uniform(EMBEDDING_INIT_FAN_IN_V1),
            ),
            Self::BuildProjection => (
                "build_trunk.feature_projection.weight",
                Build,
                weight(EMBEDDING_WIDTH_V1, table_width),
                InitRuleV1::Zero,
            ),
            Self::BuildMlp0Weight => (
                "build_trunk.card_mlp.0.weight",
                Build,
                weight(H, card_input),
                uniform(card_input),
            ),
            Self::BuildMlp0Bias => (
                "build_trunk.card_mlp.0.bias",
                Build,
                vec![H],
                InitRuleV1::Zero,
            ),
            Self::BuildMlp2Weight => (
                "build_trunk.card_mlp.2.weight",
                Build,
                weight(H, H),
                uniform(H),
            ),
            Self::BuildMlp2Bias => (
                "build_trunk.card_mlp.2.bias",
                Build,
                vec![H],
                InitRuleV1::Zero,
            ),
            Self::PoolEncoder0Weight => (
                "build.pool_encoder.0.weight",
                Build,
                weight(H, H),
                uniform(H),
            ),
            Self::PoolEncoder0Bias => (
                "build.pool_encoder.0.bias",
                Build,
                vec![H],
                InitRuleV1::Zero,
            ),
            Self::PoolEncoder2Weight => (
                "build.pool_encoder.2.weight",
                Build,
                weight(H, H),
                uniform(H),
            ),
            Self::PoolEncoder2Bias => (
                "build.pool_encoder.2.bias",
                Build,
                vec![H],
                InitRuleV1::Zero,
            ),
            Self::CountHeadWeight => (
                "build.count_head.weight",
                Build,
                weight(BASICS_COUNT_OPTIONS_V1, H),
                InitRuleV1::Zero,
            ),
            Self::CountHeadBias => (
                "build.count_head.bias",
                Build,
                vec![BASICS_COUNT_OPTIONS_V1],
                InitRuleV1::Zero,
            ),
            Self::PointerContextWeight => (
                "build.pointer_context.weight",
                Build,
                weight(H, POINTER_CONTEXT_INPUT_WIDTH),
                uniform(POINTER_CONTEXT_INPUT_WIDTH),
            ),
            Self::PointerContextBias => (
                "build.pointer_context.bias",
                Build,
                vec![H],
                InitRuleV1::Zero,
            ),
            Self::PointerScorer0Weight => (
                "build.pointer_scorer.0.weight",
                Build,
                weight(H, SCORER_INPUT_WIDTH),
                uniform(SCORER_INPUT_WIDTH),
            ),
            Self::PointerScorer0Bias => (
                "build.pointer_scorer.0.bias",
                Build,
                vec![H],
                InitRuleV1::Zero,
            ),
            Self::PointerScorer2Weight => (
                "build.pointer_scorer.2.weight",
                Build,
                weight(1, H),
                InitRuleV1::Zero,
            ),
            Self::TypesetContextWeight => (
                "build.typeset_context.weight",
                Build,
                weight(H, TYPESET_CONTEXT_INPUT_WIDTH),
                uniform(TYPESET_CONTEXT_INPUT_WIDTH),
            ),
            Self::TypesetContextBias => (
                "build.typeset_context.bias",
                Build,
                vec![H],
                InitRuleV1::Zero,
            ),
            Self::TypesetScorer0Weight => (
                "build.typeset_scorer.0.weight",
                Build,
                weight(H, TYPESET_SCORER_INPUT_WIDTH),
                uniform(TYPESET_SCORER_INPUT_WIDTH),
            ),
            Self::TypesetScorer0Bias => (
                "build.typeset_scorer.0.bias",
                Build,
                vec![H],
                InitRuleV1::Zero,
            ),
            Self::TypesetScorer2Weight => (
                "build.typeset_scorer.2.weight",
                Build,
                weight(1, H),
                InitRuleV1::Zero,
            ),
            Self::SplitScorer0Weight => (
                "build.split_scorer.0.weight",
                Build,
                weight(H, SCORER_INPUT_WIDTH),
                uniform(SCORER_INPUT_WIDTH),
            ),
            Self::SplitScorer0Bias => (
                "build.split_scorer.0.bias",
                Build,
                vec![H],
                InitRuleV1::Zero,
            ),
            Self::SplitScorer2Weight => (
                "build.split_scorer.2.weight",
                Build,
                weight(1, H),
                InitRuleV1::Zero,
            ),
            Self::PickEmbedding => (
                "pick_trunk.card_embedding.weight",
                Pick,
                weight(index_rows, EMBEDDING_WIDTH_V1),
                uniform(EMBEDDING_INIT_FAN_IN_V1),
            ),
            Self::PickProjection => (
                "pick_trunk.feature_projection.weight",
                Pick,
                weight(EMBEDDING_WIDTH_V1, table_width),
                InitRuleV1::Zero,
            ),
            Self::PickMlp0Weight => (
                "pick_trunk.card_mlp.0.weight",
                Pick,
                weight(H, card_input),
                uniform(card_input),
            ),
            Self::PickMlp0Bias => (
                "pick_trunk.card_mlp.0.bias",
                Pick,
                vec![H],
                InitRuleV1::Zero,
            ),
            Self::PickMlp2Weight => (
                "pick_trunk.card_mlp.2.weight",
                Pick,
                weight(H, H),
                uniform(H),
            ),
            Self::PickMlp2Bias => (
                "pick_trunk.card_mlp.2.bias",
                Pick,
                vec![H],
                InitRuleV1::Zero,
            ),
            Self::StateEncoder0Weight => (
                "pick.state_encoder.0.weight",
                Pick,
                weight(H, STATE_INPUT_WIDTH_V1),
                history_zeroed,
            ),
            Self::StateEncoder0Bias => {
                ("pick.state_encoder.0.bias", Pick, vec![H], InitRuleV1::Zero)
            }
            Self::StateEncoder2Weight => (
                "pick.state_encoder.2.weight",
                Pick,
                weight(H, H),
                uniform(H),
            ),
            Self::StateEncoder2Bias => {
                ("pick.state_encoder.2.bias", Pick, vec![H], InitRuleV1::Zero)
            }
            Self::PickScorer0Weight => (
                "pick.scorer.0.weight",
                Pick,
                weight(H, SCORER_INPUT_WIDTH),
                uniform(SCORER_INPUT_WIDTH),
            ),
            Self::PickScorer0Bias => ("pick.scorer.0.bias", Pick, vec![H], InitRuleV1::Zero),
            Self::PickScorer2Weight => {
                ("pick.scorer.2.weight", Pick, weight(1, H), InitRuleV1::Zero)
            }
            Self::Value0Weight => (
                "value.0.weight",
                Value,
                weight(H, STATE_INPUT_WIDTH_V1),
                history_zeroed,
            ),
            Self::Value0Bias => ("value.0.bias", Value, vec![H], InitRuleV1::Zero),
            Self::Value2Weight => ("value.2.weight", Value, weight(H, H), uniform(H)),
            Self::Value2Bias => ("value.2.bias", Value, vec![H], InitRuleV1::Zero),
            Self::ValueOutWeight => ("value.out.weight", Value, weight(1, H), InitRuleV1::Zero),
            Self::ValueOutBias => ("value.out.bias", Value, vec![1], InitRuleV1::Zero),
        };
        TensorSpecV1 {
            name: name.to_owned(),
            set,
            shape,
            init,
        }
    }
}

/// The parameter layout for `index_rows` card rows, `table_width` table
/// columns and `dense_width` dense columns, in layout order.
pub fn parameter_layout_v1(
    index_rows: usize,
    table_width: usize,
    dense_width: usize,
) -> Result<Vec<TensorSpecV1>, ModelErrorV1> {
    if index_rows == 0 || table_width == 0 {
        return Err(shape_error(format!(
            "the index needs at least one row and the table at least one column \
             (rows {index_rows}, table width {table_width})"
        )));
    }
    if dense_width > table_width {
        return Err(shape_error(format!(
            "the dense width {dense_width} exceeds the table width {table_width}"
        )));
    }
    Ok(TensorIdV1::ALL
        .iter()
        .map(|id| id.spec(index_rows, table_width, dense_width))
        .collect())
}

fn update_tensor_header_v1(digest: &mut Sha256, name: &str, shape: &[usize]) {
    let name_bytes = name.as_bytes();
    digest.update((name_bytes.len() as u32).to_be_bytes());
    digest.update(name_bytes);
    digest.update((shape.len() as u32).to_be_bytes());
    for dimension in shape {
        digest.update((*dimension as u64).to_be_bytes());
    }
    let count: usize = shape.iter().product();
    digest.update((count as u64).to_be_bytes());
}

/// SHA-256 of a layout's names and shapes, without values: per tensor in
/// order, the name's byte length (u32 BE), the name, the rank (u32 BE), each
/// dimension (u64 BE) and the element count (u64 BE). The play network's
/// `parameter_manifest_sha256_raw_v1` minus its values.
pub fn layout_sha256_v1(specs: &[TensorSpecV1]) -> [u8; 32] {
    let mut digest = Sha256::new();
    for spec in specs {
        update_tensor_header_v1(&mut digest, &spec.name, &spec.shape);
    }
    digest.finalize().into()
}

/// The uniform bound 1/sqrt(`fan_in`), computed in f64 and rounded once.
pub fn uniform_bound_v1(fan_in: usize) -> f32 {
    (1.0f64 / (fan_in as f64).sqrt()) as f32
}

/// One initial weight from one stream value: the top 24 bits as u in [0, 1)
/// (exact), t = 2u - 1 (exact), then one f32 multiply by the bound.
pub fn uniform_draw_v1(stream_value: u64, bound: f32) -> f32 {
    let u = ((stream_value >> 40) as u32) as f32 / 16_777_216.0;
    let t = 2.0f32 * u - 1.0f32;
    t * bound
}

/// Initial values of `specs`, in order, from one `SplitMix64::seed(seed)`
/// stream. A zero tensor draws nothing; a drawn tensor draws one value per
/// element in row-major order, including its zeroed columns.
pub fn initial_values_v1(specs: &[TensorSpecV1], seed: u64) -> Vec<Vec<f32>> {
    let mut stream = SplitMix64::seed(seed);
    specs
        .iter()
        .map(|spec| {
            let count = spec.element_count();
            match &spec.init {
                InitRuleV1::Zero => vec![0.0f32; count],
                InitRuleV1::Uniform {
                    fan_in,
                    zero_columns,
                } => {
                    let bound = uniform_bound_v1(*fan_in);
                    let mut values: Vec<f32> = (0..count)
                        .map(|_| uniform_draw_v1(stream.next_u64(), bound))
                        .collect();
                    if let Some(columns) = zero_columns {
                        let in_width = spec.shape[spec.shape.len() - 1];
                        for row in values.chunks_exact_mut(in_width) {
                            row[columns.clone()].fill(0.0f32);
                        }
                    }
                    values
                }
            }
        })
        .collect()
}

/// The card rows the model reads, one per index row: the index's registry
/// ids in ascending order, the scaled table S (`rows` by `table_width`), the
/// dense rows D (`rows` by `dense_width`) and the rows of the five basics.
///
/// D1's card index, loader and binding produce these; this type only checks
/// their shapes and holds them for the forward.
#[derive(Clone, Debug)]
pub struct ModelCardRowsV1 {
    ids: Vec<u16>,
    table_width: usize,
    dense_width: usize,
    scaled: Vec<f32>,
    dense: Vec<f32>,
    basic_rows: [usize; BASIC_TYPE_COUNT_V1],
}

impl ModelCardRowsV1 {
    /// `basic_ids` are the registry ids of Plains, Island, Swamp, Mountain
    /// and Forest, in that order (type bits W=1, U=2, B=4, R=8, G=16).
    pub fn new_v1(
        ids: Vec<u16>,
        table_width: usize,
        scaled: Vec<f32>,
        dense_width: usize,
        dense: Vec<f32>,
        basic_ids: [u16; BASIC_TYPE_COUNT_V1],
    ) -> Result<Self, ModelErrorV1> {
        if ids.is_empty() || table_width == 0 {
            return Err(shape_error(
                "the card rows need at least one row and one column",
            ));
        }
        if let Some(pair) = ids.windows(2).find(|pair| pair[0] >= pair[1]) {
            return Err(shape_error(format!(
                "index ids are not strictly ascending ({} then {})",
                pair[0], pair[1]
            )));
        }
        if dense_width > table_width {
            return Err(shape_error(format!(
                "the dense width {dense_width} exceeds the table width {table_width}"
            )));
        }
        let rows = ids.len();
        if scaled.len() != rows * table_width {
            return Err(shape_error(format!(
                "the scaled table has {} values, not {rows} x {table_width}",
                scaled.len()
            )));
        }
        if dense.len() != rows * dense_width {
            return Err(shape_error(format!(
                "the dense rows have {} values, not {rows} x {dense_width}",
                dense.len()
            )));
        }
        if scaled
            .iter()
            .chain(dense.iter())
            .any(|value| !value.is_finite())
        {
            return Err(shape_error("the card rows hold a non-finite value"));
        }
        let mut basic_rows = [0usize; BASIC_TYPE_COUNT_V1];
        for (slot, id) in basic_rows.iter_mut().zip(basic_ids) {
            *slot = ids
                .binary_search(&id)
                .map_err(|_| shape_error(format!("basic land {id} is not in the index")))?;
        }
        let mut sorted_basics = basic_rows;
        sorted_basics.sort_unstable();
        if sorted_basics.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(shape_error("the five basic land ids are not distinct"));
        }
        Ok(Self {
            ids,
            table_width,
            dense_width,
            scaled,
            dense,
            basic_rows,
        })
    }

    /// [`Self::new_v1`] with the registry's basics,
    /// [`crate::cube_draft_v1::basic_land_ids`].
    pub fn with_registry_basics_v1(
        ids: Vec<u16>,
        table_width: usize,
        scaled: Vec<f32>,
        dense_width: usize,
        dense: Vec<f32>,
    ) -> Result<Self, ModelErrorV1> {
        let basics = crate::cube_draft_v1::basic_land_ids()
            .map_err(|error| shape_error(format!("basic land ids: {error}")))?;
        Self::new_v1(ids, table_width, scaled, dense_width, dense, basics)
    }

    pub fn rows(&self) -> usize {
        self.ids.len()
    }

    pub fn table_width(&self) -> usize {
        self.table_width
    }

    pub fn dense_width(&self) -> usize {
        self.dense_width
    }

    pub fn ids(&self) -> &[u16] {
        &self.ids
    }

    /// The index row of a registry id.
    pub fn row_of(&self, id: u16) -> Option<usize> {
        self.ids.binary_search(&id).ok()
    }

    pub fn scaled_row(&self, row: usize) -> &[f32] {
        &self.scaled[row * self.table_width..(row + 1) * self.table_width]
    }

    pub fn dense_row(&self, row: usize) -> &[f32] {
        &self.dense[row * self.dense_width..(row + 1) * self.dense_width]
    }

    /// Index rows of the basics, W, U, B, R, G.
    pub fn basic_rows(&self) -> [usize; BASIC_TYPE_COUNT_V1] {
        self.basic_rows
    }
}

/// H: one trunk's card hidden h(r) for every index row, `rows` by 64.
#[derive(Clone, Debug)]
pub struct CardHiddenTableV1 {
    trunk: TrunkV1,
    rows: usize,
    values: Vec<f32>,
}

impl CardHiddenTableV1 {
    pub fn trunk(&self) -> TrunkV1 {
        self.trunk
    }

    pub fn rows(&self) -> usize {
        self.rows
    }

    pub fn row(&self, row: usize) -> &[f32] {
        &self.values[row * H..(row + 1) * H]
    }

    pub fn values(&self) -> &[f32] {
        &self.values
    }
}

/// The model: three parameter sets in one layout.
#[derive(Clone, Debug)]
pub struct ModelV1 {
    index_rows: usize,
    table_width: usize,
    dense_width: usize,
    specs: Vec<TensorSpecV1>,
    values: Vec<Vec<f32>>,
}

impl ModelV1 {
    /// A fresh model from `init_seed` (see [`initial_values_v1`]). Every
    /// head's last layer, every bias, both projections P and the history
    /// columns start at +0.0.
    pub fn initialise_v1(
        index_rows: usize,
        table_width: usize,
        dense_width: usize,
        init_seed: u64,
    ) -> Result<Self, ModelErrorV1> {
        let specs = parameter_layout_v1(index_rows, table_width, dense_width)?;
        let values = initial_values_v1(&specs, init_seed);
        Ok(Self {
            index_rows,
            table_width,
            dense_width,
            specs,
            values,
        })
    }

    pub fn index_rows(&self) -> usize {
        self.index_rows
    }

    pub fn table_width(&self) -> usize {
        self.table_width
    }

    pub fn dense_width(&self) -> usize {
        self.dense_width
    }

    pub fn layout(&self) -> &[TensorSpecV1] {
        &self.specs
    }

    pub fn parameter_count(&self) -> usize {
        self.specs.iter().map(TensorSpecV1::element_count).sum()
    }

    pub fn parameter_count_in_set(&self, set: ParameterSetV1) -> usize {
        self.specs
            .iter()
            .filter(|spec| spec.set == set)
            .map(TensorSpecV1::element_count)
            .sum()
    }

    /// See [`layout_sha256_v1`]. Values do not enter it.
    pub fn parameter_layout_sha256(&self) -> [u8; 32] {
        layout_sha256_v1(&self.specs)
    }

    /// Per tensor in layout order, the layout hash's bytes for it followed by
    /// its values as f32 bits little-endian.
    pub fn parameter_sha256(&self) -> [u8; 32] {
        let mut digest = Sha256::new();
        for (spec, values) in self.specs.iter().zip(&self.values) {
            update_tensor_header_v1(&mut digest, &spec.name, &spec.shape);
            for value in values {
                digest.update(value.to_bits().to_le_bytes());
            }
        }
        digest.finalize().into()
    }

    /// Calls `visit` on every tensor in layout order.
    pub fn visit_tensors_v1(&self, mut visit: impl FnMut(TensorViewV1<'_>)) {
        for (spec, values) in self.specs.iter().zip(&self.values) {
            visit(TensorViewV1 {
                name: &spec.name,
                set: spec.set,
                shape: &spec.shape,
                values,
            });
        }
    }

    /// Calls `visit` on every tensor in layout order with its values mutable
    /// (name, set, shape, values). Shapes cannot change.
    pub fn visit_tensors_mut_v1(
        &mut self,
        mut visit: impl FnMut(&str, ParameterSetV1, &[usize], &mut [f32]),
    ) {
        for (spec, values) in self.specs.iter().zip(self.values.iter_mut()) {
            visit(&spec.name, spec.set, &spec.shape, values);
        }
    }

    /// The values of the tensor named `name`.
    pub fn tensor_v1(&self, name: &str) -> Option<&[f32]> {
        let position = self.specs.iter().position(|spec| spec.name == name)?;
        Some(&self.values[position])
    }

    /// The values of the tensor named `name`, mutable.
    pub fn tensor_mut_v1(&mut self, name: &str) -> Option<&mut [f32]> {
        let position = self.specs.iter().position(|spec| spec.name == name)?;
        Some(&mut self.values[position])
    }

    /// Copies the build trunk's four tensors (E, P, `card_mlp.0` and
    /// `card_mlp.2`, weights and biases) into the pick trunk, bit for bit.
    /// Nothing else changes.
    pub fn copy_build_trunk_to_pick_trunk(&mut self) {
        for (from, to) in TensorIdV1::BUILD_TRUNK.iter().zip(TensorIdV1::PICK_TRUNK) {
            let source = self.values[from.index()].clone();
            self.values[to.index()].copy_from_slice(&source);
        }
    }

    fn t(&self, id: TensorIdV1) -> &[f32] {
        &self.values[id.index()]
    }

    fn layer(&self, weight: TensorIdV1, bias: Option<TensorIdV1>) -> LayerV1<'_> {
        let shape = &self.specs[weight.index()].shape;
        LayerV1 {
            weight: self.t(weight),
            bias: bias.map(|id| self.t(id)),
            out_width: shape[0],
            in_width: shape[1],
        }
    }

    fn check_rows(&self, rows: &ModelCardRowsV1) -> Result<(), ModelErrorV1> {
        if rows.rows() != self.index_rows
            || rows.table_width() != self.table_width
            || rows.dense_width() != self.dense_width
        {
            return Err(shape_error(format!(
                "the card rows are {} x {} with {} dense columns; the model is {} x {} with {}",
                rows.rows(),
                rows.table_width(),
                rows.dense_width(),
                self.index_rows,
                self.table_width,
                self.dense_width
            )));
        }
        Ok(())
    }

    fn card_input(
        &self,
        rows: &ModelCardRowsV1,
        row: usize,
        embedding: TensorIdV1,
        projection: TensorIdV1,
    ) -> Vec<f32> {
        let embedding_row =
            &self.t(embedding)[row * EMBEDDING_WIDTH_V1..(row + 1) * EMBEDDING_WIDTH_V1];
        let projection = self.t(projection);
        let scaled = rows.scaled_row(row);
        let mut card_vector = vec![0.0f32; EMBEDDING_WIDTH_V1 + self.dense_width];
        for (k, slot) in card_vector[..EMBEDDING_WIDTH_V1].iter_mut().enumerate() {
            // c_k = E[r][k] + sum over j ascending of P[k][j] s_j.
            let projection_row = &projection[k * self.table_width..(k + 1) * self.table_width];
            let mut sum = embedding_row[k];
            for (weight, value) in projection_row.iter().zip(scaled) {
                sum += weight * value;
            }
            *slot = sum;
        }
        card_vector[EMBEDDING_WIDTH_V1..].copy_from_slice(rows.dense_row(row));
        card_vector
    }

    /// h(r) of one index row through `trunk`.
    fn card_hidden_row(&self, trunk: TrunkV1, rows: &ModelCardRowsV1, row: usize, out: &mut [f32]) {
        let ids = match trunk {
            TrunkV1::Build => TensorIdV1::BUILD_TRUNK,
            TrunkV1::Pick => TensorIdV1::PICK_TRUNK,
        };
        let [embedding, projection, mlp0_weight, mlp0_bias, mlp2_weight, mlp2_bias] = ids;
        let card_vector = self.card_input(rows, row, embedding, projection);
        let mut first = [0.0f32; H];
        self.layer(mlp0_weight, Some(mlp0_bias))
            .forward(&card_vector, &mut first);
        MATH.tanh_in_place_v1(&mut first);
        self.layer(mlp2_weight, Some(mlp2_bias))
            .forward(&first, out);
        MATH.tanh_in_place_v1(out);
    }

    /// H for `trunk`: every index row's h, rows split over `threads` threads
    /// and assembled in index order. Each row's arithmetic is fixed, so the
    /// result does not depend on `threads`.
    pub fn card_hidden_table(
        &self,
        rows: &ModelCardRowsV1,
        trunk: TrunkV1,
        threads: usize,
    ) -> Result<CardHiddenTableV1, ModelErrorV1> {
        self.check_rows(rows)?;
        if threads == 0 {
            return Err(shape_error("the thread count must be at least 1"));
        }
        let row_count = self.index_rows;
        let mut values = vec![0.0f32; row_count * H];
        let threads = threads.min(row_count);
        if threads == 1 {
            for (row, out) in values.chunks_exact_mut(H).enumerate() {
                self.card_hidden_row(trunk, rows, row, out);
            }
        } else {
            let rows_per_thread = row_count.div_ceil(threads);
            std::thread::scope(|scope| {
                for (chunk_index, chunk) in values.chunks_mut(rows_per_thread * H).enumerate() {
                    scope.spawn(move || {
                        for (offset, out) in chunk.chunks_exact_mut(H).enumerate() {
                            let row = chunk_index * rows_per_thread + offset;
                            self.card_hidden_row(trunk, rows, row, out);
                        }
                    });
                }
            });
        }
        Ok(CardHiddenTableV1 {
            trunk,
            rows: row_count,
            values,
        })
    }

    /// The pick head and the value over the pick trunk's H, computed here
    /// once. The forward borrows the model, so no parameter can change while
    /// its H is in use. The forward is the cache: keep one per parameter
    /// version for all of that version's decisions, since every call here
    /// recomputes H.
    pub fn pick_forward_v1<'m>(
        &'m self,
        rows: &'m ModelCardRowsV1,
        threads: usize,
    ) -> Result<PickForwardV1<'m>, ModelErrorV1> {
        let hidden = self.card_hidden_table(rows, TrunkV1::Pick, threads)?;
        Ok(PickForwardV1 {
            model: self,
            rows,
            hidden: std::borrow::Cow::Owned(hidden),
        })
    }

    /// Borrows H from a caller that holds this immutable model and its card rows.
    #[cfg(feature = "eternal-fixtures")]
    pub(crate) fn pick_forward_with_hidden<'m>(
        &'m self,
        rows: &'m ModelCardRowsV1,
        hidden: &'m CardHiddenTableV1,
    ) -> PickForwardV1<'m> {
        debug_assert_eq!(hidden.trunk(), TrunkV1::Pick);
        PickForwardV1 {
            model: self,
            rows,
            hidden: std::borrow::Cow::Borrowed(hidden),
        }
    }

    /// The build head over the build trunk's H, computed here once.
    pub fn build_forward_v1<'m>(
        &'m self,
        rows: &'m ModelCardRowsV1,
        threads: usize,
    ) -> Result<BuildForwardV1<'m>, ModelErrorV1> {
        let hidden = self.card_hidden_table(rows, TrunkV1::Build, threads)?;
        Ok(BuildForwardV1 {
            model: self,
            rows,
            hidden: std::borrow::Cow::Owned(hidden),
        })
    }

    /// Borrows H from a caller that holds this immutable model and its card rows.
    #[cfg(feature = "eternal-fixtures")]
    pub(crate) fn build_forward_with_hidden<'m>(
        &'m self,
        rows: &'m ModelCardRowsV1,
        hidden: &'m CardHiddenTableV1,
    ) -> BuildForwardV1<'m> {
        debug_assert_eq!(hidden.trunk(), TrunkV1::Build);
        BuildForwardV1 {
            model: self,
            rows,
            hidden: std::borrow::Cow::Borrowed(hidden),
        }
    }
}

/// One layer's weights `[out, in]` and optional bias.
#[derive(Clone, Copy)]
struct LayerV1<'a> {
    weight: &'a [f32],
    bias: Option<&'a [f32]>,
    out_width: usize,
    in_width: usize,
}

impl LayerV1<'_> {
    /// Each output starts at its bias, or at +0.0 without one.
    fn start(&self) -> Vec<f32> {
        match self.bias {
            Some(bias) => bias.to_vec(),
            None => vec![0.0f32; self.out_width],
        }
    }

    /// Adds `W[i][first + k] * input[k]` to `accumulator[i]` for k
    /// ascending: one rounded multiply and one rounded add per term, no FMA.
    /// Calling it on consecutive column ranges in order gives the same bits
    /// as one call on the concatenated input.
    fn accumulate(&self, first: usize, input: &[f32], accumulator: &mut [f32]) {
        debug_assert_eq!(accumulator.len(), self.out_width);
        debug_assert!(first + input.len() <= self.in_width);
        for (i, slot) in accumulator.iter_mut().enumerate() {
            let start = i * self.in_width + first;
            let row = &self.weight[start..start + input.len()];
            let mut sum = *slot;
            for (weight, value) in row.iter().zip(input) {
                sum += weight * value;
            }
            *slot = sum;
        }
    }

    /// y = b + W x over the full input.
    fn forward(&self, input: &[f32], output: &mut [f32]) {
        debug_assert_eq!(input.len(), self.in_width);
        match self.bias {
            Some(bias) => output.copy_from_slice(bias),
            None => output.fill(0.0f32),
        }
        self.accumulate(0, input, output);
    }
}

/// Two tanh layers, `inner` then `outer`.
fn two_tanh_layers(inner: LayerV1<'_>, outer: LayerV1<'_>, input: &[f32]) -> [f32; H] {
    let mut first = [0.0f32; H];
    inner.forward(input, &mut first);
    MATH.tanh_in_place_v1(&mut first);
    let mut second = [0.0f32; H];
    outer.forward(&first, &mut second);
    MATH.tanh_in_place_v1(&mut second);
    second
}

/// The final no-bias scorer row: +0.0 plus `w[k] * hidden[k]`, k ascending.
fn score(last: LayerV1<'_>, hidden: &[f32]) -> f32 {
    let mut out = [0.0f32];
    last.forward(hidden, &mut out);
    out[0]
}

/// The mean of H over `sorted_rows` (ascending index rows): add the rows in
/// order from +0.0, then multiply by 1/n. An empty set gives +0.0.
fn set_mean(hidden: &CardHiddenTableV1, sorted_rows: &[usize]) -> [f32; H] {
    let mut mean = [0.0f32; H];
    if sorted_rows.is_empty() {
        return mean;
    }
    for &row in sorted_rows {
        for (slot, value) in mean.iter_mut().zip(hidden.row(row)) {
            *slot += *value;
        }
    }
    let scale = 1.0f32 / (sorted_rows.len() as f32);
    for slot in &mut mean {
        *slot *= scale;
    }
    mean
}

/// One correctly rounded f32 division.
fn ratio(numerator: usize, denominator: f32) -> f32 {
    (numerator as f32) / denominator
}

/// A candidate list resolved against the index: per candidate, sorted by
/// index row (so by card id), its row, its id and its input position.
struct CandidatesV1 {
    rows: Vec<usize>,
    ids: Vec<u16>,
    positions: Vec<usize>,
}

/// Resolves `ids` to index rows in ascending order, refusing an id outside
/// the index and a repeated id.
fn resolve_set(
    rows: &ModelCardRowsV1,
    ids: &[u16],
    what: &str,
    error: fn(String) -> ModelErrorV1,
) -> Result<CandidatesV1, ModelErrorV1> {
    let mut entries = Vec::with_capacity(ids.len());
    for (position, &id) in ids.iter().enumerate() {
        let row = rows
            .row_of(id)
            .ok_or_else(|| error(format!("{what} card {id} is not in the index")))?;
        entries.push((row, id, position));
    }
    entries.sort_unstable();
    if let Some(pair) = entries.windows(2).find(|pair| pair[0].0 == pair[1].0) {
        return Err(error(format!("{what} holds card {} twice", pair[0].1)));
    }
    Ok(CandidatesV1 {
        rows: entries.iter().map(|entry| entry.0).collect(),
        ids: entries.iter().map(|entry| entry.1).collect(),
        positions: entries.iter().map(|entry| entry.2).collect(),
    })
}

fn sorted_intersect(left: &[usize], right: &[usize]) -> bool {
    let (mut i, mut j) = (0, 0);
    while i < left.len() && j < right.len() {
        match left[i].cmp(&right[j]) {
            std::cmp::Ordering::Less => i += 1,
            std::cmp::Ordering::Greater => j += 1,
            std::cmp::Ordering::Equal => return true,
        }
    }
    false
}

/// Logits of a candidate list (a pack, or the remaining pool cards of a
/// pointer step), in card id ascending.
#[derive(Clone, Debug, PartialEq)]
pub struct CandidateLogitsV1 {
    /// Candidate card ids, strictly ascending.
    pub ids: Vec<u16>,
    /// `input_positions[k]` is the position of `ids[k]` in the input list.
    pub input_positions: Vec<usize>,
    /// `logits[k]` is the logit of `ids[k]`.
    pub logits: Vec<f32>,
}

/// What a seat knows at a pick, and nothing else.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PickStateV1 {
    /// The 15 - p cards in front of the seat.
    pub pack: Vec<u16>,
    /// The seat's own 15r + p picks so far, as a set.
    pub pool: Vec<u16>,
    /// Every card the seat saw at an earlier pick of this draft and did not
    /// take, as a set.
    pub history: Vec<u16>,
    /// 15r + p, 0 to 44.
    pub step: u8,
    /// +1 for left (rounds 0 and 2), -1 for right (round 1).
    pub direction: i8,
}

struct ResolvedPickV1 {
    pack: CandidatesV1,
    pool_rows: Vec<usize>,
    history_rows: Vec<usize>,
}

/// Checks a pick state against the index and against the draft's own
/// structure, and resolves its ids to sorted index rows.
fn resolve_pick_state(
    rows: &ModelCardRowsV1,
    state: &PickStateV1,
) -> Result<ResolvedPickV1, ModelErrorV1> {
    let step = usize::from(state.step);
    if step >= STEP_COUNT_V1 {
        return Err(state_error(format!(
            "step {step} is not under {STEP_COUNT_V1}"
        )));
    }
    if state.pack.is_empty() || state.pack.len() > PACK_SIZE {
        return Err(state_error(format!(
            "a pack holds 1 to {PACK_SIZE} cards, not {}",
            state.pack.len()
        )));
    }
    if state.direction != 1 && state.direction != -1 {
        return Err(state_error(format!(
            "direction {} is not +1 or -1",
            state.direction
        )));
    }
    let round = step / PACK_SIZE;
    let pick = step % PACK_SIZE;
    if state.pack.len() != PACK_SIZE - pick {
        return Err(state_error(format!(
            "at step {step} the pack holds {} cards, not {}",
            state.pack.len(),
            PACK_SIZE - pick
        )));
    }
    let expected_direction = if round == 1 { -1 } else { 1 };
    if state.direction != expected_direction {
        return Err(state_error(format!(
            "round {round} passes {expected_direction:+}, not {:+}",
            state.direction
        )));
    }
    if state.pool.len() != step {
        return Err(state_error(format!(
            "at step {step} the pool holds {} cards, not {step}",
            state.pool.len()
        )));
    }
    if state.history.len() > MAX_HISTORY_CARDS_V1 {
        return Err(state_error(format!(
            "the history holds {} cards, more than {MAX_HISTORY_CARDS_V1}",
            state.history.len()
        )));
    }
    let pack = resolve_set(rows, &state.pack, "the pack", ModelErrorV1::PickState)?;
    let pool = resolve_set(rows, &state.pool, "the pool", ModelErrorV1::PickState)?;
    let history = resolve_set(rows, &state.history, "the history", ModelErrorV1::PickState)?;
    if sorted_intersect(&pack.rows, &pool.rows) {
        return Err(state_error("a pack card is already in the pool"));
    }
    if sorted_intersect(&pool.rows, &history.rows) {
        return Err(state_error("a pool card is in the history"));
    }
    Ok(ResolvedPickV1 {
        pack,
        pool_rows: pool.rows,
        history_rows: history.rows,
    })
}

/// The pick head and the value over one parameter version's pick trunk H.
pub struct PickForwardV1<'m> {
    model: &'m ModelV1,
    rows: &'m ModelCardRowsV1,
    hidden: std::borrow::Cow<'m, CardHiddenTableV1>,
}

impl PickForwardV1<'_> {
    pub fn hidden_table(&self) -> &CardHiddenTableV1 {
        &self.hidden
    }

    fn state_input(&self, resolved: &ResolvedPickV1, state: &PickStateV1) -> Vec<f32> {
        let mut input = vec![0.0f32; STATE_INPUT_WIDTH_V1];
        input[0..H].copy_from_slice(&set_mean(&self.hidden, &resolved.pack.rows));
        input[H..2 * H].copy_from_slice(&set_mean(&self.hidden, &resolved.pool_rows));
        input[HISTORY_COLUMNS_V1].copy_from_slice(&set_mean(&self.hidden, &resolved.history_rows));
        input[STEP_COLUMN_V1 + usize::from(state.step)] = 1.0f32;
        input[DIRECTION_COLUMN_V1] = f32::from(state.direction);
        input
    }

    /// x, the 238-wide state input: pack mean, pool mean, history mean, the
    /// step one-hot and the direction.
    pub fn state_input_v1(&self, state: &PickStateV1) -> Result<Vec<f32>, ModelErrorV1> {
        let resolved = resolve_pick_state(self.rows, state)?;
        Ok(self.state_input(&resolved, state))
    }

    /// One logit per pack card, in card id ascending.
    pub fn pick_logits_v1(&self, state: &PickStateV1) -> Result<CandidateLogitsV1, ModelErrorV1> {
        use TensorIdV1 as T;
        let resolved = resolve_pick_state(self.rows, state)?;
        let input = self.state_input(&resolved, state);
        let model = self.model;
        let state_hidden = two_tanh_layers(
            model.layer(T::StateEncoder0Weight, Some(T::StateEncoder0Bias)),
            model.layer(T::StateEncoder2Weight, Some(T::StateEncoder2Bias)),
            &input,
        );
        let scorer = model.layer(T::PickScorer0Weight, Some(T::PickScorer0Bias));
        let last = model.layer(T::PickScorer2Weight, None);
        // The u columns come first in every candidate's sum, so their partial
        // sums are shared; the h columns continue each sum in order.
        let mut shared = scorer.start();
        scorer.accumulate(0, &state_hidden, &mut shared);
        let logits = resolved
            .pack
            .rows
            .iter()
            .map(|&row| {
                let mut pre = shared.clone();
                scorer.accumulate(H, self.hidden.row(row), &mut pre);
                MATH.tanh_in_place_v1(&mut pre);
                score(last, &pre)
            })
            .collect();
        Ok(CandidateLogitsV1 {
            ids: resolved.pack.ids,
            input_positions: resolved.pack.positions,
            logits,
        })
    }

    /// V(x), on the same state input as the pick head.
    pub fn value_v1(&self, state: &PickStateV1) -> Result<f32, ModelErrorV1> {
        use TensorIdV1 as T;
        let resolved = resolve_pick_state(self.rows, state)?;
        let input = self.state_input(&resolved, state);
        let model = self.model;
        let hidden = two_tanh_layers(
            model.layer(T::Value0Weight, Some(T::Value0Bias)),
            model.layer(T::Value2Weight, Some(T::Value2Bias)),
            &input,
        );
        let mut out = [0.0f32];
        model
            .layer(T::ValueOutWeight, Some(T::ValueOutBias))
            .forward(&hidden, &mut out);
        Ok(out[0])
    }
}

/// The build head over one parameter version's build trunk H.
pub struct BuildForwardV1<'m> {
    model: &'m ModelV1,
    rows: &'m ModelCardRowsV1,
    hidden: std::borrow::Cow<'m, CardHiddenTableV1>,
}

impl BuildForwardV1<'_> {
    pub fn hidden_table(&self) -> &CardHiddenTableV1 {
        &self.hidden
    }

    /// The pool context g of a 45-card pool, and what the later steps need.
    pub fn build_context_v1(&self, pool: &[u16]) -> Result<BuildContextV1<'_>, ModelErrorV1> {
        use TensorIdV1 as T;
        if pool.len() != BUILD_POOL_CARDS_V1 {
            return Err(build_error(format!(
                "a build pool holds {BUILD_POOL_CARDS_V1} cards, not {}",
                pool.len()
            )));
        }
        let pool_rows = resolve_set(self.rows, pool, "the pool", ModelErrorV1::Build)?.rows;
        let context = two_tanh_layers(
            self.model
                .layer(T::PoolEncoder0Weight, Some(T::PoolEncoder0Bias)),
            self.model
                .layer(T::PoolEncoder2Weight, Some(T::PoolEncoder2Bias)),
            &set_mean(&self.hidden, &pool_rows),
        );
        Ok(BuildContextV1 {
            model: self.model,
            rows: self.rows,
            hidden: &self.hidden,
            pool_rows,
            context,
        })
    }
}

/// One pool's build: its context g and the steps that read it.
pub struct BuildContextV1<'f> {
    model: &'f ModelV1,
    rows: &'f ModelCardRowsV1,
    hidden: &'f CardHiddenTableV1,
    pool_rows: Vec<usize>,
    context: [f32; H],
}

impl<'f> BuildContextV1<'f> {
    /// g.
    pub fn pool_context(&self) -> &[f32] {
        &self.context
    }

    /// Step 1: 41 logits, index b for B = b.
    pub fn count_logits_v1(&self) -> Vec<f32> {
        use TensorIdV1 as T;
        let mut logits = vec![0.0f32; BASICS_COUNT_OPTIONS_V1];
        self.model
            .layer(T::CountHeadWeight, Some(T::CountHeadBias))
            .forward(&self.context, &mut logits);
        logits
    }

    fn check_basics_count(&self, basics_count: usize) -> Result<(), ModelErrorV1> {
        if basics_count > DECK_CARDS_V1 {
            return Err(build_error(format!(
                "the basics count {basics_count} is over {DECK_CARDS_V1}"
            )));
        }
        Ok(())
    }

    /// Resolves a list of pool cards, refusing a card outside the pool.
    fn resolve_pool_subset(&self, ids: &[u16], what: &str) -> Result<CandidatesV1, ModelErrorV1> {
        let resolved = resolve_set(self.rows, ids, what, ModelErrorV1::Build)?;
        if let Some(position) = resolved
            .rows
            .iter()
            .position(|row| self.pool_rows.binary_search(row).is_err())
        {
            return Err(build_error(format!(
                "{what} card {} is not in the pool",
                resolved.ids[position]
            )));
        }
        Ok(resolved)
    }

    /// Step 2, pointer step j = `chosen.len()`: logits for the `remaining`
    /// pool cards, in card id ascending. `chosen` and `remaining` must
    /// partition the pool, and j must be under 40 - B.
    pub fn pointer_logits_v1(
        &self,
        basics_count: usize,
        chosen: &[u16],
        remaining: &[u16],
    ) -> Result<CandidateLogitsV1, ModelErrorV1> {
        use TensorIdV1 as T;
        self.check_basics_count(basics_count)?;
        let step = chosen.len();
        let steps = DECK_CARDS_V1 - basics_count;
        if step >= steps {
            return Err(build_error(format!(
                "pointer step {step} is not under 40 - B = {steps}"
            )));
        }
        let chosen = self.resolve_pool_subset(chosen, "the chosen list")?;
        let remaining = self.resolve_pool_subset(remaining, "the remaining list")?;
        if sorted_intersect(&chosen.rows, &remaining.rows) {
            return Err(build_error("a card is both chosen and remaining"));
        }
        if chosen.rows.len() + remaining.rows.len() != self.pool_rows.len() {
            return Err(build_error(
                "the chosen and remaining lists do not cover the pool",
            ));
        }
        let mut context_input = Vec::with_capacity(POINTER_CONTEXT_INPUT_WIDTH);
        context_input.extend_from_slice(&self.context);
        context_input.extend_from_slice(&set_mean(self.hidden, &chosen.rows));
        context_input.push(ratio(basics_count, 40.0));
        context_input.push(ratio(step, 40.0));
        let mut query = [0.0f32; H];
        self.model
            .layer(T::PointerContextWeight, Some(T::PointerContextBias))
            .forward(&context_input, &mut query);
        MATH.tanh_in_place_v1(&mut query);
        let scorer = self
            .model
            .layer(T::PointerScorer0Weight, Some(T::PointerScorer0Bias));
        let last = self.model.layer(T::PointerScorer2Weight, None);
        let mut shared = scorer.start();
        scorer.accumulate(0, &query, &mut shared);
        let logits = remaining
            .rows
            .iter()
            .map(|&row| {
                let mut pre = shared.clone();
                scorer.accumulate(H, self.hidden.row(row), &mut pre);
                MATH.tanh_in_place_v1(&mut pre);
                score(last, &pre)
            })
            .collect();
        Ok(CandidateLogitsV1 {
            ids: remaining.ids,
            input_positions: remaining.positions,
            logits,
        })
    }

    /// The context r of steps 3 and 4, once the 40 - B pool cards are
    /// chosen. B must be 1 to 40 (D3 skips steps 3 and 4 at B = 0).
    pub fn type_context_v1(
        &self,
        basics_count: usize,
        chosen: &[u16],
    ) -> Result<TypeContextV1<'f>, ModelErrorV1> {
        use TensorIdV1 as T;
        self.check_basics_count(basics_count)?;
        if basics_count == 0 {
            return Err(build_error(
                "B = 0 has no basic land types; steps 3 and 4 are skipped",
            ));
        }
        if chosen.len() != DECK_CARDS_V1 - basics_count {
            return Err(build_error(format!(
                "with B = {basics_count} the chosen list holds {} cards, not {}",
                chosen.len(),
                DECK_CARDS_V1 - basics_count
            )));
        }
        let chosen = self.resolve_pool_subset(chosen, "the chosen list")?;
        let mut context_input = Vec::with_capacity(TYPESET_CONTEXT_INPUT_WIDTH);
        context_input.extend_from_slice(&self.context);
        context_input.extend_from_slice(&set_mean(self.hidden, &chosen.rows));
        context_input.push(ratio(basics_count, 40.0));
        let mut context = [0.0f32; H];
        self.model
            .layer(T::TypesetContextWeight, Some(T::TypesetContextBias))
            .forward(&context_input, &mut context);
        MATH.tanh_in_place_v1(&mut context);
        Ok(TypeContextV1 {
            model: self.model,
            rows: self.rows,
            hidden: self.hidden,
            basics_count,
            context,
        })
    }
}

/// Logits of the split step, one per type of T, in ascending type bit.
#[derive(Clone, Debug, PartialEq)]
pub struct SplitLogitsV1 {
    /// Type bits of T, ascending (W=1, U=2, B=4, R=8, G=16).
    pub type_bits: Vec<u8>,
    /// `logits[k]` is the logit of `type_bits[k]`.
    pub logits: Vec<f32>,
}

/// Steps 3 and 4 of one build, with their shared context r.
pub struct TypeContextV1<'f> {
    model: &'f ModelV1,
    rows: &'f ModelCardRowsV1,
    hidden: &'f CardHiddenTableV1,
    basics_count: usize,
    context: [f32; H],
}

impl TypeContextV1<'_> {
    /// r.
    pub fn type_context(&self) -> &[f32] {
        &self.context
    }

    /// Step 3: one logit per non-empty type set T, by 5-bit mask 1 to 31.
    /// Sets with more types than B are included; D3 masks them.
    pub fn typeset_logits_v1(&self) -> Vec<f32> {
        use TensorIdV1 as T;
        let scorer = self
            .model
            .layer(T::TypesetScorer0Weight, Some(T::TypesetScorer0Bias));
        let last = self.model.layer(T::TypesetScorer2Weight, None);
        let mut shared = scorer.start();
        scorer.accumulate(0, &self.context, &mut shared);
        let basic_rows = self.rows.basic_rows();
        (1..=TYPE_SET_COUNT_V1)
            .map(|mask| {
                let mut members: Vec<usize> = (0..BASIC_TYPE_COUNT_V1)
                    .filter(|bit| mask & (1 << bit) != 0)
                    .map(|bit| basic_rows[bit])
                    .collect();
                members.sort_unstable();
                let mut pre = shared.clone();
                scorer.accumulate(H, &set_mean(self.hidden, &members), &mut pre);
                scorer.accumulate(2 * H, &[ratio(members.len(), 5.0)], &mut pre);
                MATH.tanh_in_place_v1(&mut pre);
                score(last, &pre)
            })
            .collect()
    }

    /// Step 4: one logit per type of T (mask 1 to 31, at most B types), in
    /// ascending type bit. The same logits serve every draw of the split.
    pub fn split_logits_v1(&self, type_mask: u8) -> Result<SplitLogitsV1, ModelErrorV1> {
        use TensorIdV1 as T;
        if type_mask == 0 || usize::from(type_mask) > TYPE_SET_COUNT_V1 {
            return Err(build_error(format!(
                "type mask {type_mask} is not 1 to {TYPE_SET_COUNT_V1}"
            )));
        }
        if type_mask.count_ones() as usize > self.basics_count {
            return Err(build_error(format!(
                "type mask {type_mask} has more types than B = {}",
                self.basics_count
            )));
        }
        let scorer = self
            .model
            .layer(T::SplitScorer0Weight, Some(T::SplitScorer0Bias));
        let last = self.model.layer(T::SplitScorer2Weight, None);
        let mut shared = scorer.start();
        scorer.accumulate(0, &self.context, &mut shared);
        let basic_rows = self.rows.basic_rows();
        let mut type_bits = Vec::new();
        let mut logits = Vec::new();
        for (bit, &row) in basic_rows.iter().enumerate() {
            let type_bit = 1u8 << bit;
            if type_mask & type_bit == 0 {
                continue;
            }
            let mut pre = shared.clone();
            scorer.accumulate(H, self.hidden.row(row), &mut pre);
            MATH.tanh_in_place_v1(&mut pre);
            type_bits.push(type_bit);
            logits.push(score(last, &pre));
        }
        Ok(SplitLogitsV1 { type_bits, logits })
    }
}

#[cfg(test)]
#[path = "model_tests.rs"]
mod tests;

#[path = "training.rs"]
pub mod training;
