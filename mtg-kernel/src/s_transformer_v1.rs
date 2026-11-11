//! Native S: the small object transformer of W5's XS arm (collab
//! LANES/lead/W5-DESIGN.md sections 2, 5, 6.2 and E3), in Rust, with
//! forward, backward and Adam.
//!
//! **Architecture.** The bignet pilot's `ObjectTransformer` (collab
//! LANES/bignet-pilot-20261031/code/models.py) at d 96, 3 layers, 4 heads,
//! FFN 384, card embedding 32 and a 257-row card table: 460,346 parameters,
//! named and ordered as the PyTorch module's `named_parameters()`, so an S0
//! export loads by name. Tokens are one state token, one token per object
//! (features plus card embedding through a GELU MLP, plus a zone-group
//! embedding) and one token per legal action (features through a GELU MLP,
//! plus the sum of its references' encodings). Edges and action references
//! enter as learned per-head additive attention biases, shared by every
//! block. Pre-LayerNorm blocks with exact (erf) GELU, a final LayerNorm, a
//! policy MLP on `[state token, action token]` and a value MLP on the state
//! token. Dropout is 0 (inference and RL).
//!
//! **Inputs.** The FlatV4 tensors (`NativeFlatDecisionTensorV2`), read as
//! the pilot read them: the state's last 96 coordinates (the state digest)
//! are dropped; the action digest is kept.
//!
//! **Value.** The value head emits a logit `z` of the win probability (S0 is
//! distilled with an outcome log loss). The RL value is `tanh(z / 2) =
//! 2 sigmoid(z) - 1`, the expected terminal outcome in [-1, 1], which the
//! RT1 GAE objective regresses on its targets.
//!
//! **Arithmetic.** f32 throughout, every reduction in a fixed order
//! (`kernels`), transcendentals from IEEE operations only, and the MXCSR
//! pinned on entry, so the same inputs give the same bits on every x86-64
//! system (the portable class). There is one forward: inference and the
//! learner's tape share it, so collection logits equal the learner's
//! recomputation bit for bit.
//!
//! **Gauge.** Two parameter sets have an exactly zero gradient in exact
//! arithmetic, because a softmax ignores a shift shared by its inputs: the
//! policy's output bias (`policy.2.bias`) and the key third of every
//! `blocks.<i>.qkv.bias`. Their f32 gradient would be rounding noise, which
//! Adam would turn into steps, so S defines their gradient as exactly zero
//! and never moves them. The card table's row 0 is the padding row
//! (`padding_idx=0`) and receives no gradient, as in PyTorch.

#![allow(clippy::needless_range_loop)]

mod auxiliary;
mod checkpoint;
mod kernels;
mod parallel;
#[cfg(test)]
mod policy_tests;
pub mod profile;
#[cfg(test)]
mod tests;
pub mod tools;
pub(crate) mod update_profile_v1;
#[cfg(test)]
pub(crate) mod verified_row_v1;

use crate::native_flat_tensorizer_v2::NativeFlatDecisionTensorV2;
use crate::training_math_v1::TrainingMathV1;
use kernels::{gemm_v1, InitV1};
use parallel::SGradientSinkV1;
use profile::{SectionV1, StageV1};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub use auxiliary::{SAuxiliaryInputV1, SAuxiliaryResultV1, SAuxiliaryRowV1};
pub use checkpoint::{
    decode_checkpoint_v1, decode_weights_snapshot_v1, encode_checkpoint_v1,
    encode_weights_snapshot_v1, SWeightsSnapshotV1, S_WEIGHTS_SNAPSHOT_SCHEMA_V1,
};

/// The architecture identity records carry.
pub const S_ARCHITECTURE_V1: &str = "s-object-transformer-v1";
/// The train-state schema (hash and checkpoint document).
pub const S_TRAIN_STATE_SCHEMA_V1: &str = "s-transformer-train-state-v1";

pub(crate) const STATE_DIM_V1: usize = 219;
/// The state coordinates S reads: all but the trailing 96-wide state digest.
pub(crate) const STATE_KEEP_V1: usize = STATE_DIM_V1 - 96;
pub(crate) const OBJECT_FEATURE_DIM_V1: usize = 98;
pub(crate) const EDGE_FEATURE_DIM_V1: usize = 41;
pub(crate) const ACTION_FEATURE_DIM_V1: usize = 195;
pub(crate) const REF_FEATURE_DIM_V1: usize = 25;
pub(crate) const OBJECT_GROUP_COUNT_V1: usize = 20;

pub(crate) const ADAM_BETA1_V1: f32 = 0.9;
pub(crate) const ADAM_BETA2_V1: f32 = 0.999;
pub(crate) const ADAM_EPSILON_V1: f32 = 1.0e-8;

/// S's shape. [`SConfigV1::s_v1`] is W5's S; tests use smaller shapes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SConfigV1 {
    pub d_model: usize,
    pub layers: usize,
    pub heads: usize,
    pub ffn: usize,
    pub card_dim: usize,
    pub card_vocab: usize,
}

impl SConfigV1 {
    /// W5's S (design section 2): 460,346 parameters.
    pub const fn s_v1() -> Self {
        Self {
            d_model: 96,
            layers: 3,
            heads: 4,
            ffn: 384,
            card_dim: 32,
            card_vocab: 257,
        }
    }

    pub fn validate_v1(&self) -> Result<(), String> {
        let positive = self.d_model > 0
            && self.layers > 0
            && self.heads > 0
            && self.ffn > 0
            && self.card_dim > 0
            && self.card_vocab > 1;
        let bounded = self.d_model <= 4096
            && self.layers <= 64
            && self.heads <= 64
            && self.ffn <= 16384
            && self.card_dim <= 4096
            && self.card_vocab <= 1 << 20;
        if !positive
            || !bounded
            || !self.d_model.is_multiple_of(self.heads)
            || self.d_model / self.heads > 256
        {
            return Err(format!("invalid S config {self:?}"));
        }
        Ok(())
    }

    pub fn head_dim_v1(&self) -> usize {
        self.d_model / self.heads
    }
}

/// One named tensor of the flat parameter vector.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TensorSpecV1 {
    pub name: String,
    pub shape: Vec<usize>,
    pub offset: usize,
    pub len: usize,
    /// A `Linear` weight `[out, in]`, which the forward reads transposed.
    pub linear: bool,
}

/// Offsets of one `Linear` (weight `[out, in]`, bias `[out]`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct LinearIdxV1 {
    pub(crate) w: usize,
    pub(crate) b: usize,
    pub(crate) out_dim: usize,
    pub(crate) in_dim: usize,
}

impl LinearIdxV1 {
    /// The `Linear` of output rows `first..first + count`: the same inputs,
    /// those weight rows and bias entries.
    pub(crate) fn rows_v1(self, first: usize, count: usize) -> Self {
        assert!(first + count <= self.out_dim, "S Linear rows out of range");
        Self {
            w: self.w + first * self.in_dim,
            b: self.b + first,
            out_dim: count,
            in_dim: self.in_dim,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct BlockIdxV1 {
    pub(crate) ln1_g: usize,
    pub(crate) ln1_b: usize,
    pub(crate) qkv: LinearIdxV1,
    pub(crate) proj: LinearIdxV1,
    pub(crate) ln2_g: usize,
    pub(crate) ln2_b: usize,
    pub(crate) ff0: LinearIdxV1,
    pub(crate) ff2: LinearIdxV1,
}

/// The parameter layout of a config, in PyTorch `named_parameters()` order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LayoutV1 {
    pub config: SConfigV1,
    pub tensors: Vec<TensorSpecV1>,
    pub total: usize,
    pub(crate) card: usize,
    pub(crate) obj0: LinearIdxV1,
    pub(crate) obj2: LinearIdxV1,
    pub(crate) group: usize,
    pub(crate) state0: LinearIdxV1,
    pub(crate) state2: LinearIdxV1,
    pub(crate) act0: LinearIdxV1,
    pub(crate) act2: LinearIdxV1,
    pub(crate) ref0: LinearIdxV1,
    pub(crate) ref2: LinearIdxV1,
    pub(crate) edge: LinearIdxV1,
    pub(crate) ref_bias: usize,
    pub(crate) blocks: Vec<BlockIdxV1>,
    pub(crate) ln_g: usize,
    pub(crate) ln_b: usize,
    pub(crate) pol0: LinearIdxV1,
    pub(crate) pol2: LinearIdxV1,
    pub(crate) val0: LinearIdxV1,
    pub(crate) val2: LinearIdxV1,
}

struct LayoutBuilderV1 {
    tensors: Vec<TensorSpecV1>,
    total: usize,
}

impl LayoutBuilderV1 {
    fn push(&mut self, name: String, shape: Vec<usize>, linear: bool) -> usize {
        let len = shape.iter().product();
        let offset = self.total;
        self.tensors.push(TensorSpecV1 {
            name,
            shape,
            offset,
            len,
            linear,
        });
        self.total += len;
        offset
    }

    fn linear(&mut self, prefix: &str, in_dim: usize, out_dim: usize) -> LinearIdxV1 {
        let w = self.push(format!("{prefix}.weight"), vec![out_dim, in_dim], true);
        let b = self.push(format!("{prefix}.bias"), vec![out_dim], false);
        LinearIdxV1 {
            w,
            b,
            out_dim,
            in_dim,
        }
    }

    fn vector(&mut self, name: String, len: usize) -> usize {
        self.push(name, vec![len], false)
    }
}

impl LayoutV1 {
    pub fn of_v1(config: SConfigV1) -> Result<Self, String> {
        config.validate_v1()?;
        let d = config.d_model;
        let h = config.heads;
        let cd = config.card_dim;
        let mut b = LayoutBuilderV1 {
            tensors: Vec::new(),
            total: 0,
        };
        // `named_parameters()` yields the module's own parameter (`ref_bias`)
        // before its submodules', then the submodules in definition order.
        let ref_bias = b.push("ref_bias".into(), vec![2, h], false);
        let card = b.push("card.weight".into(), vec![config.card_vocab, cd], false);
        let obj0 = b.linear("obj_in.0", OBJECT_FEATURE_DIM_V1 + cd, d);
        let obj2 = b.linear("obj_in.2", d, d);
        let group = b.push("group.weight".into(), vec![OBJECT_GROUP_COUNT_V1, d], false);
        let state0 = b.linear("state_in.0", STATE_KEEP_V1, d);
        let state2 = b.linear("state_in.2", d, d);
        let act0 = b.linear("act_in.0", ACTION_FEATURE_DIM_V1, d);
        let act2 = b.linear("act_in.2", d, d);
        let ref0 = b.linear("ref_in.0", REF_FEATURE_DIM_V1 + cd, d);
        let ref2 = b.linear("ref_in.2", d, d);
        let edge = b.linear("edge_bias", EDGE_FEATURE_DIM_V1, 2 * h);
        let mut blocks = Vec::with_capacity(config.layers);
        for layer in 0..config.layers {
            let p = format!("blocks.{layer}");
            let ln1_g = b.vector(format!("{p}.ln1.weight"), d);
            let ln1_b = b.vector(format!("{p}.ln1.bias"), d);
            let qkv = b.linear(&format!("{p}.qkv"), d, 3 * d);
            let proj = b.linear(&format!("{p}.proj"), d, d);
            let ln2_g = b.vector(format!("{p}.ln2.weight"), d);
            let ln2_b = b.vector(format!("{p}.ln2.bias"), d);
            let ff0 = b.linear(&format!("{p}.ff.0"), d, config.ffn);
            let ff2 = b.linear(&format!("{p}.ff.2"), config.ffn, d);
            blocks.push(BlockIdxV1 {
                ln1_g,
                ln1_b,
                qkv,
                proj,
                ln2_g,
                ln2_b,
                ff0,
                ff2,
            });
        }
        let ln_g = b.vector("ln.weight".into(), d);
        let ln_b = b.vector("ln.bias".into(), d);
        let pol0 = b.linear("policy.0", 2 * d, d);
        let pol2 = b.linear("policy.2", d, 1);
        let val0 = b.linear("value.0", d, d);
        let val2 = b.linear("value.2", d, 1);
        Ok(Self {
            config,
            tensors: b.tensors,
            total: b.total,
            card,
            obj0,
            obj2,
            group,
            state0,
            state2,
            act0,
            act2,
            ref0,
            ref2,
            edge,
            ref_bias,
            blocks,
            ln_g,
            ln_b,
            pol0,
            pol2,
            val0,
            val2,
        })
    }

    /// Flat ranges whose gradient is defined as exactly zero (module doc).
    pub fn gauge_ranges_v1(&self) -> Vec<std::ops::Range<usize>> {
        let d = self.config.d_model;
        let mut ranges = Vec::with_capacity(1 + self.blocks.len());
        ranges.push(self.pol2.b..self.pol2.b + 1);
        for block in &self.blocks {
            ranges.push(block.qkv.b + d..block.qkv.b + 2 * d);
        }
        ranges
    }
}

/// Validated counts of one decision.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CountsV1 {
    pub(crate) objects: usize,
    pub(crate) edges: usize,
    pub(crate) actions: usize,
    pub(crate) refs: usize,
}

/// One decision's FlatV4 inputs, borrowed.
#[derive(Clone, Copy, Debug)]
pub struct SInputV1<'a> {
    pub state: &'a [f32],
    pub object_features: &'a [f32],
    pub object_card_ids: &'a [i64],
    pub object_groups: &'a [i64],
    pub edge_features: &'a [f32],
    pub edge_source: &'a [i64],
    pub edge_target: &'a [i64],
    pub action_features: &'a [f32],
    pub ref_features: &'a [f32],
    pub ref_card_ids: &'a [i64],
    pub ref_action: &'a [i64],
    pub ref_node: &'a [i64],
}

fn check_indices_v1(field: &str, values: &[i64], upper: usize) -> Result<(), String> {
    for (position, value) in values.iter().enumerate() {
        if *value < 0 || (*value as u64) >= upper as u64 {
            return Err(format!(
                "S input {field}[{position}] = {value} outside [0, {upper})"
            ));
        }
    }
    Ok(())
}

fn check_finite_v1(field: &str, values: &[f32]) -> Result<(), String> {
    match values.iter().position(|v| !v.is_finite()) {
        Some(position) => Err(format!("S input {field}[{position}] is not finite")),
        None => Ok(()),
    }
}

impl<'a> SInputV1<'a> {
    /// The FlatV4 tensors of one decision.
    pub(crate) fn of_tensor_v1(t: &'a NativeFlatDecisionTensorV2) -> Self {
        Self {
            state: &t.state,
            object_features: &t.object_features,
            object_card_ids: &t.object_card_ids,
            object_groups: &t.object_groups,
            edge_features: &t.edge_features,
            edge_source: &t.edge_source_indices,
            edge_target: &t.edge_target_indices,
            action_features: &t.action_features,
            ref_features: &t.action_ref_features,
            ref_card_ids: &t.action_ref_card_ids,
            ref_action: &t.action_ref_action_indices,
            ref_node: &t.action_ref_node_indices,
        }
    }

    pub(crate) fn validate_v1(&self, config: &SConfigV1) -> Result<CountsV1, String> {
        if self.state.len() != STATE_DIM_V1 {
            return Err(format!("S input state has {} values", self.state.len()));
        }
        let objects = self.object_card_ids.len();
        if objects == 0 {
            return Err("S input has no objects".into());
        }
        if self.object_features.len() != objects * OBJECT_FEATURE_DIM_V1
            || self.object_groups.len() != objects
        {
            return Err("S input object shapes differ".into());
        }
        let edges = self.edge_source.len();
        if self.edge_target.len() != edges
            || self.edge_features.len() != edges * EDGE_FEATURE_DIM_V1
        {
            return Err("S input edge shapes differ".into());
        }
        if self.action_features.is_empty()
            || !self
                .action_features
                .len()
                .is_multiple_of(ACTION_FEATURE_DIM_V1)
        {
            return Err("S input action shape differs".into());
        }
        let actions = self.action_features.len() / ACTION_FEATURE_DIM_V1;
        let refs = self.ref_card_ids.len();
        if self.ref_features.len() != refs * REF_FEATURE_DIM_V1
            || self.ref_action.len() != refs
            || self.ref_node.len() != refs
        {
            return Err("S input reference shapes differ".into());
        }
        check_finite_v1("state", self.state)?;
        check_finite_v1("object_features", self.object_features)?;
        check_finite_v1("edge_features", self.edge_features)?;
        check_finite_v1("action_features", self.action_features)?;
        check_finite_v1("action_ref_features", self.ref_features)?;
        check_indices_v1("object_card_ids", self.object_card_ids, config.card_vocab)?;
        check_indices_v1("object_groups", self.object_groups, OBJECT_GROUP_COUNT_V1)?;
        check_indices_v1("edge_source", self.edge_source, objects)?;
        check_indices_v1("edge_target", self.edge_target, objects)?;
        check_indices_v1("action_ref_card_ids", self.ref_card_ids, config.card_vocab)?;
        check_indices_v1("action_ref_action_indices", self.ref_action, actions)?;
        check_indices_v1("action_ref_node_indices", self.ref_node, objects)?;
        Ok(CountsV1 {
            objects,
            edges,
            actions,
            refs,
        })
    }
}

/// S's parameters plus the transposed (input-major) copy of every `Linear`
/// weight that the forward reads.
#[derive(Clone, Debug, PartialEq)]
pub struct SModelV1 {
    layout: LayoutV1,
    values: Vec<f32>,
    wt: Vec<f32>,
}

/// A small deterministic generator for initializers and synthetic data.
#[derive(Clone, Debug)]
pub(crate) struct SplitMixV1(u64);

impl SplitMixV1 {
    pub(crate) fn new(seed: u64) -> Self {
        Self(seed)
    }

    pub(crate) fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// Uniform in [0, 1) with 53 bits.
    pub(crate) fn unit_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 * (1.0 / (1u64 << 53) as f64)
    }

    pub(crate) fn uniform(&mut self, low: f32, high: f32) -> f32 {
        (f64::from(low) + (f64::from(high) - f64::from(low)) * self.unit_f64()) as f32
    }

    /// Standard normal by Box-Muller with the pinned pure-Rust libm.
    pub(crate) fn normal(&mut self) -> f32 {
        let u1 = 1.0 - self.unit_f64();
        let u2 = self.unit_f64();
        (libm::sqrt(-2.0 * libm::log(u1)) * libm::cos(2.0 * std::f64::consts::PI * u2)) as f32
    }

    pub(crate) fn below(&mut self, bound: u64) -> u64 {
        self.next_u64() % bound
    }
}

impl SModelV1 {
    pub fn from_values_v1(config: SConfigV1, values: Vec<f32>) -> Result<Self, String> {
        let layout = LayoutV1::of_v1(config)?;
        if values.len() != layout.total {
            return Err(format!(
                "S parameter count {} differs from the layout's {}",
                values.len(),
                layout.total
            ));
        }
        check_finite_v1("parameters", &values)?;
        let mut model = Self {
            layout,
            values,
            wt: Vec::new(),
        };
        model.rebuild_transposed_v1();
        Ok(model)
    }

    fn rebuild_transposed_v1(&mut self) {
        self.wt.clear();
        self.wt.extend_from_slice(&self.values);
        for spec in &self.layout.tensors {
            if spec.linear {
                let (out_dim, in_dim) = (spec.shape[0], spec.shape[1]);
                for o in 0..out_dim {
                    for i in 0..in_dim {
                        self.wt[spec.offset + i * out_dim + o] =
                            self.values[spec.offset + o * in_dim + i];
                    }
                }
            }
        }
    }

    /// The pilot's initializer scheme (models.py `_init`): every `Linear`
    /// weight and embedding table N(0, 0.02), biases zero, LayerNorms (1, 0),
    /// `ref_bias` zero, drawn from [`SplitMixV1`] (not PyTorch's generator).
    pub fn init_pilot_scheme_v1(config: SConfigV1, seed: u64) -> Result<Self, String> {
        ensure_mxcsr_v1()?;
        let layout = LayoutV1::of_v1(config)?;
        let mut rng = SplitMixV1::new(seed);
        let mut values = vec![0.0f32; layout.total];
        for spec in &layout.tensors {
            let slot = &mut values[spec.offset..spec.offset + spec.len];
            if spec.shape.len() == 2 && spec.name != "ref_bias" {
                for v in slot.iter_mut() {
                    *v = 0.02 * rng.normal();
                }
            } else if spec.name.ends_with("ln1.weight")
                || spec.name.ends_with("ln2.weight")
                || spec.name == "ln.weight"
            {
                slot.fill(1.0);
            }
        }
        Self::from_values_v1(config, values)
    }

    /// A well-conditioned random point for parity and determinism checks:
    /// fan-in-scaled weights, nonzero biases, LayerNorm gains near 1.
    pub fn init_parity_v1(config: SConfigV1, seed: u64) -> Result<Self, String> {
        ensure_mxcsr_v1()?;
        let layout = LayoutV1::of_v1(config)?;
        let mut rng = SplitMixV1::new(seed);
        let mut values = vec![0.0f32; layout.total];
        for spec in &layout.tensors {
            let slot = &mut values[spec.offset..spec.offset + spec.len];
            let layer_norm_gain = spec.name.ends_with("ln1.weight")
                || spec.name.ends_with("ln2.weight")
                || spec.name == "ln.weight";
            if spec.linear {
                let scale = 1.0 / (spec.shape[1] as f32).sqrt();
                for v in slot.iter_mut() {
                    *v = scale * rng.normal();
                }
            } else if spec.name == "card.weight" || spec.name == "group.weight" {
                for v in slot.iter_mut() {
                    *v = 0.3 * rng.normal();
                }
            } else if layer_norm_gain {
                for v in slot.iter_mut() {
                    *v = 1.0 + rng.uniform(-0.1, 0.1);
                }
            } else if spec.name == "ref_bias" {
                for v in slot.iter_mut() {
                    *v = rng.uniform(-0.5, 0.5);
                }
            } else {
                for v in slot.iter_mut() {
                    *v = rng.uniform(-0.1, 0.1);
                }
            }
        }
        Self::from_values_v1(config, values)
    }

    pub fn config_v1(&self) -> SConfigV1 {
        self.layout.config
    }

    pub fn layout_v1(&self) -> &LayoutV1 {
        &self.layout
    }

    pub fn values_v1(&self) -> &[f32] {
        &self.values
    }

    /// SHA-256 of the parameters (layout order, f32 bits little-endian).
    pub fn parameter_sha256_v1(&self) -> String {
        let mut hasher = Sha256::new();
        for value in &self.values {
            hasher.update(value.to_bits().to_le_bytes());
        }
        hex_v1(&hasher.finalize())
    }

    fn lin(&self, idx: LinearIdxV1) -> (&[f32], &[f32]) {
        (
            &self.wt[idx.w..idx.w + idx.out_dim * idx.in_dim],
            &self.values[idx.b..idx.b + idx.out_dim],
        )
    }

    /// `y[rows, out] = x[rows, in] W^T + b`.
    fn linear_fwd(&self, idx: LinearIdxV1, rows: usize, x: &[f32], y: &mut [f32]) {
        let (wt, bias) = self.lin(idx);
        gemm_v1(
            rows,
            idx.out_dim,
            idx.in_dim,
            x,
            idx.in_dim,
            1,
            wt,
            InitV1::Bias(bias),
            &mut y[..rows * idx.out_dim],
        );
    }

    /// Scores one decision: logits (one per legal action), RL value and the
    /// value logit. `tape` is reusable scratch; afterwards it holds every
    /// intermediate the backward needs.
    pub fn forward_v1(&self, input: SInputV1<'_>, tape: &mut STapeV1) -> Result<SOutputV1, String> {
        self.forward_into_v1(input, tape)?;
        Ok(SOutputV1 {
            logits: tape.logits.clone(),
            value: tape.value,
            value_logit: tape.value_logit,
        })
    }

    pub(crate) fn forward_into_v1(
        &self,
        input: SInputV1<'_>,
        tape: &mut STapeV1,
    ) -> Result<(), String> {
        ensure_mxcsr_v1()?;
        let counts = input.validate_v1(&self.layout.config)?;
        self.forward_validated_into_v1(input, tape, counts)
    }

    fn forward_validated_into_v1(
        &self,
        input: SInputV1<'_>,
        tape: &mut STapeV1,
        counts: CountsV1,
    ) -> Result<(), String> {
        #[cfg(test)]
        verified_row_v1::note_forward_v1();
        let config = self.layout.config;
        let (n, e, a, r) = (counts.objects, counts.edges, counts.actions, counts.refs);
        let d = config.d_model;
        let h = config.heads;
        let dh = config.head_dim_v1();
        let cd = config.card_dim;
        let ffn = config.ffn;
        let l = 1 + n + a;
        tape.reset_v1(&config, counts);
        let lay = &self.layout;

        // Object tokens.
        let obj_in = OBJECT_FEATURE_DIM_V1 + cd;
        for i in 0..n {
            let row = &mut tape.obj_inp[i * obj_in..(i + 1) * obj_in];
            row[..OBJECT_FEATURE_DIM_V1].copy_from_slice(
                &input.object_features[i * OBJECT_FEATURE_DIM_V1..(i + 1) * OBJECT_FEATURE_DIM_V1],
            );
            let card = input.object_card_ids[i] as usize;
            row[OBJECT_FEATURE_DIM_V1..]
                .copy_from_slice(&self.values[lay.card + card * cd..lay.card + (card + 1) * cd]);
        }
        self.linear_fwd(lay.obj0, n, &tape.obj_inp, &mut tape.obj_pre);
        kernels::gelu_slice_v1(&tape.obj_pre, &mut tape.obj_act);
        // Residual stream of layer 0 (`x0`): [state, objects, actions].
        let x0 = &mut tape.layers[0].x_in;
        self.linear_fwd(lay.obj2, n, &tape.obj_act, &mut x0[d..(1 + n) * d]);
        for i in 0..n {
            let g = input.object_groups[i] as usize;
            let group = &self.values[lay.group + g * d..lay.group + (g + 1) * d];
            for (slot, value) in x0[(1 + i) * d..(2 + i) * d].iter_mut().zip(group) {
                *slot += *value;
            }
        }
        // State token.
        self.linear_fwd(
            lay.state0,
            1,
            &input.state[..STATE_KEEP_V1],
            &mut tape.state_pre,
        );
        kernels::gelu_slice_v1(&tape.state_pre, &mut tape.state_act);
        self.linear_fwd(lay.state2, 1, &tape.state_act, &mut x0[..d]);
        // Action tokens, then their references' encodings in reference order.
        self.linear_fwd(lay.act0, a, input.action_features, &mut tape.act_pre);
        kernels::gelu_slice_v1(&tape.act_pre, &mut tape.act_act);
        self.linear_fwd(lay.act2, a, &tape.act_act, &mut x0[(1 + n) * d..l * d]);
        if r > 0 {
            let ref_in = REF_FEATURE_DIM_V1 + cd;
            for k in 0..r {
                let row = &mut tape.ref_inp[k * ref_in..(k + 1) * ref_in];
                row[..REF_FEATURE_DIM_V1].copy_from_slice(
                    &input.ref_features[k * REF_FEATURE_DIM_V1..(k + 1) * REF_FEATURE_DIM_V1],
                );
                let card = input.ref_card_ids[k] as usize;
                row[REF_FEATURE_DIM_V1..].copy_from_slice(
                    &self.values[lay.card + card * cd..lay.card + (card + 1) * cd],
                );
            }
            self.linear_fwd(lay.ref0, r, &tape.ref_inp, &mut tape.ref_pre);
            kernels::gelu_slice_v1(&tape.ref_pre, &mut tape.ref_act);
            self.linear_fwd(lay.ref2, r, &tape.ref_act, &mut tape.ref_out);
            for k in 0..r {
                let token = 1 + n + input.ref_action[k] as usize;
                for c in 0..d {
                    x0[token * d + c] += tape.ref_out[k * d + c];
                }
            }
        }

        // Attention bias [h, l, l]: every edge's source-to-target terms, then
        // every target-to-source term, then the references' two directions.
        tape.bias.fill(0.0);
        if e > 0 {
            self.linear_fwd(lay.edge, e, input.edge_features, &mut tape.edge_out);
            for k in 0..e {
                let s = 1 + input.edge_source[k] as usize;
                let t = 1 + input.edge_target[k] as usize;
                for head in 0..h {
                    tape.bias[(head * l + s) * l + t] += tape.edge_out[k * 2 * h + head];
                }
            }
            for k in 0..e {
                let s = 1 + input.edge_source[k] as usize;
                let t = 1 + input.edge_target[k] as usize;
                for head in 0..h {
                    tape.bias[(head * l + t) * l + s] += tape.edge_out[k * 2 * h + h + head];
                }
            }
        }
        if r > 0 {
            for k in 0..r {
                let ai = 1 + n + input.ref_action[k] as usize;
                let oi = 1 + input.ref_node[k] as usize;
                for head in 0..h {
                    tape.bias[(head * l + ai) * l + oi] += self.values[lay.ref_bias + head];
                }
            }
            for k in 0..r {
                let ai = 1 + n + input.ref_action[k] as usize;
                let oi = 1 + input.ref_node[k] as usize;
                for head in 0..h {
                    tape.bias[(head * l + oi) * l + ai] += self.values[lay.ref_bias + h + head];
                }
            }
        }

        // Blocks.
        let scale = 1.0 / (dh as f32).sqrt();
        for (layer, block) in lay.blocks.iter().enumerate() {
            let (before, after) = tape.layers.split_at_mut(layer + 1);
            let lt = &mut before[layer];
            for t in 0..l {
                let (mean, rstd) = kernels::layer_norm_row_v1(
                    &lt.x_in[t * d..(t + 1) * d],
                    &self.values[block.ln1_g..block.ln1_g + d],
                    &self.values[block.ln1_b..block.ln1_b + d],
                    &mut lt.ln1[t * d..(t + 1) * d],
                );
                lt.ln1_stats[2 * t] = mean;
                lt.ln1_stats[2 * t + 1] = rstd;
            }
            self.linear_fwd(block.qkv, l, &lt.ln1, &mut lt.qkv);
            // Query rows: every token, except in the last block, whose object
            // rows reach no output (the heads read the state and action
            // tokens only). Rows are independent, so the live rows' values
            // are the same either way; the skipped rows stay zero in the tape.
            let segments: Vec<(usize, usize)> = if layer + 1 == config.layers {
                vec![(0, 1), (1 + n, a)]
            } else {
                vec![(0, l)]
            };
            for head in 0..h {
                // Contiguous K_h^T [dh, l] and V_h [l, dh].
                for j in 0..l {
                    for c in 0..dh {
                        tape.key_t[c * l + j] = lt.qkv[j * 3 * d + d + head * dh + c];
                        tape.value_h[j * dh + c] = lt.qkv[j * 3 * d + 2 * d + head * dh + c];
                    }
                }
                for &(start, count) in &segments {
                    let probs =
                        &mut lt.probs[(head * l + start) * l..(head * l + start + count) * l];
                    // scores[i, j] = sum over c ascending of q[i, c] k[j, c].
                    gemm_v1(
                        count,
                        l,
                        dh,
                        &lt.qkv[start * 3 * d + head * dh..],
                        3 * d,
                        1,
                        &tape.key_t,
                        InitV1::Zero,
                        probs,
                    );
                    for r in 0..count {
                        let i = start + r;
                        let row = &mut probs[r * l..(r + 1) * l];
                        for j in 0..l {
                            row[j] = row[j] * scale + tape.bias[(head * l + i) * l + j];
                        }
                        kernels::softmax_row_v1(row);
                    }
                    // out[i, c] = sum over j ascending of p[i, j] v[j, c].
                    gemm_v1(
                        count,
                        dh,
                        l,
                        probs,
                        l,
                        1,
                        &tape.value_h,
                        InitV1::Zero,
                        &mut tape.head_out,
                    );
                    for r in 0..count {
                        let i = start + r;
                        lt.attn[i * d + head * dh..i * d + head * dh + dh]
                            .copy_from_slice(&tape.head_out[r * dh..(r + 1) * dh]);
                    }
                }
            }
            let next = match after.first_mut() {
                Some(next) => &mut next.x_in,
                None => &mut tape.x_out,
            };
            for &(start, count) in &segments {
                let rows = start * d..(start + count) * d;
                self.linear_fwd(
                    block.proj,
                    count,
                    &lt.attn[rows.clone()],
                    &mut lt.x_mid[rows.clone()],
                );
                for index in rows.clone() {
                    lt.x_mid[index] += lt.x_in[index];
                }
                for t in start..start + count {
                    let (mean, rstd) = kernels::layer_norm_row_v1(
                        &lt.x_mid[t * d..(t + 1) * d],
                        &self.values[block.ln2_g..block.ln2_g + d],
                        &self.values[block.ln2_b..block.ln2_b + d],
                        &mut lt.ln2[t * d..(t + 1) * d],
                    );
                    lt.ln2_stats[2 * t] = mean;
                    lt.ln2_stats[2 * t + 1] = rstd;
                }
                let wide = start * ffn..(start + count) * ffn;
                self.linear_fwd(
                    block.ff0,
                    count,
                    &lt.ln2[rows.clone()],
                    &mut lt.ff_pre[wide.clone()],
                );
                kernels::gelu_slice_v1(&lt.ff_pre[wide.clone()], &mut lt.ff_act[wide.clone()]);
                self.linear_fwd(block.ff2, count, &lt.ff_act[wide], &mut next[rows.clone()]);
                for index in rows {
                    next[index] += lt.x_mid[index];
                }
            }
        }

        // Final LayerNorm of the state token and the action tokens: rows
        // [state, action 0, ..., action a-1].
        for slot in 0..=a {
            let t = if slot == 0 { 0 } else { n + slot };
            let (mean, rstd) = kernels::layer_norm_row_v1(
                &tape.x_out[t * d..(t + 1) * d],
                &self.values[lay.ln_g..lay.ln_g + d],
                &self.values[lay.ln_b..lay.ln_b + d],
                &mut tape.fin[slot * d..(slot + 1) * d],
            );
            tape.fin_stats[2 * slot] = mean;
            tape.fin_stats[2 * slot + 1] = rstd;
        }
        // Policy: one row [state, action j] per action.
        for j in 0..a {
            tape.pol_inp[j * 2 * d..j * 2 * d + d].copy_from_slice(&tape.fin[..d]);
            tape.pol_inp[j * 2 * d + d..(j + 1) * 2 * d]
                .copy_from_slice(&tape.fin[(1 + j) * d..(2 + j) * d]);
        }
        self.linear_fwd(lay.pol0, a, &tape.pol_inp, &mut tape.pol_pre);
        kernels::gelu_slice_v1(&tape.pol_pre, &mut tape.pol_act);
        self.linear_fwd(lay.pol2, a, &tape.pol_act, &mut tape.logits);
        // Value.
        self.linear_fwd(lay.val0, 1, &tape.fin[..d], &mut tape.val_pre);
        kernels::gelu_slice_v1(&tape.val_pre, &mut tape.val_act);
        let mut z = [0.0f32];
        self.linear_fwd(lay.val2, 1, &tape.val_act, &mut z);
        tape.value_logit = z[0];
        tape.value = crate::deterministic_math_v1::tanh_f32_v1(0.5 * z[0]);
        if !tape.value.is_finite() || tape.logits.iter().any(|v| !v.is_finite()) {
            return Err("S forward produced a non-finite output".into());
        }
        Ok(())
    }
}

thread_local! {
    /// The calling thread's reusable forward tape for [`SModelV1::score_tensor_v1`].
    static THREAD_TAPE_V1: std::cell::RefCell<STapeV1> = std::cell::RefCell::new(STapeV1::default());
}

impl SModelV1 {
    /// Scores one FlatV4 decision with the calling thread's reusable tape:
    /// `(logits, RL value)`, the same bits as [`SModelV1::forward_v1`].
    pub(crate) fn score_tensor_v1(
        &self,
        t: &NativeFlatDecisionTensorV2,
    ) -> Result<(Vec<f32>, f32), String> {
        THREAD_TAPE_V1.with(|tape| {
            let mut tape = tape.borrow_mut();
            self.forward_into_v1(SInputV1::of_tensor_v1(t), &mut tape)?;
            Ok((tape.logits.clone(), tape.value))
        })
    }

    /// SHA-256 of the card table (`card.weight`, f32 bits little-endian).
    pub(crate) fn card_table_sha256_v1(&self) -> String {
        let config = self.layout.config;
        let card = self.layout.card;
        let mut hasher = Sha256::new();
        for value in &self.values[card..card + config.card_vocab * config.card_dim] {
            hasher.update(value.to_bits().to_le_bytes());
        }
        hex_v1(&hasher.finalize())
    }

    /// A prediction-cache namespace that no net-8 model shares: SHA-256 of a
    /// family tag, the manifest digest and the parameter digest.
    pub(crate) fn cache_namespace_v1(&self) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(b"mtg-kernel/s-transformer-v1/cache-namespace\0");
        hasher.update(self.manifest_sha256_v1().as_bytes());
        hasher.update(self.parameter_sha256_v1().as_bytes());
        hasher.finalize().into()
    }

    /// SHA-256 of the architecture and config: S's counterpart of net-8's
    /// parameter-manifest digest.
    pub(crate) fn manifest_sha256_v1(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(S_ARCHITECTURE_V1.as_bytes());
        hasher.update(b"\n");
        hasher.update(
            serde_json::to_string(&self.layout.config)
                .expect("config serializes")
                .as_bytes(),
        );
        hex_v1(&hasher.finalize())
    }
}

/// One decision's scores.
#[derive(Clone, Debug, PartialEq)]
pub struct SOutputV1 {
    pub logits: Vec<f32>,
    /// `tanh(value_logit / 2)`, in [-1, 1].
    pub value: f32,
    pub value_logit: f32,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct LayerTapeV1 {
    x_in: Vec<f32>,
    ln1: Vec<f32>,
    ln1_stats: Vec<f32>,
    qkv: Vec<f32>,
    probs: Vec<f32>,
    attn: Vec<f32>,
    x_mid: Vec<f32>,
    ln2: Vec<f32>,
    ln2_stats: Vec<f32>,
    ff_pre: Vec<f32>,
    ff_act: Vec<f32>,
}

impl LayerTapeV1 {
    fn size_v1(
        &mut self,
        config: &SConfigV1,
        counts: CountsV1,
        mut size: impl FnMut(&mut Vec<f32>, usize),
    ) {
        let d = config.d_model;
        let l = 1 + counts.objects + counts.actions;
        size(&mut self.x_in, l * d);
        size(&mut self.ln1, l * d);
        size(&mut self.ln1_stats, 2 * l);
        size(&mut self.qkv, l * 3 * d);
        size(&mut self.probs, config.heads * l * l);
        size(&mut self.attn, l * d);
        size(&mut self.x_mid, l * d);
        size(&mut self.ln2, l * d);
        size(&mut self.ln2_stats, 2 * l);
        size(&mut self.ff_pre, l * config.ffn);
        size(&mut self.ff_act, l * config.ffn);
    }
}

/// Forward workspace and tape. Buffers grow to the largest decision seen and
/// are reused.
#[derive(Clone, Debug, Default)]
pub struct STapeV1 {
    counts: Option<CountsV1>,
    obj_inp: Vec<f32>,
    obj_pre: Vec<f32>,
    obj_act: Vec<f32>,
    state_pre: Vec<f32>,
    state_act: Vec<f32>,
    act_pre: Vec<f32>,
    act_act: Vec<f32>,
    ref_inp: Vec<f32>,
    ref_pre: Vec<f32>,
    ref_act: Vec<f32>,
    ref_out: Vec<f32>,
    edge_out: Vec<f32>,
    bias: Vec<f32>,
    key_t: Vec<f32>,
    value_h: Vec<f32>,
    head_out: Vec<f32>,
    layers: Vec<LayerTapeV1>,
    x_out: Vec<f32>,
    fin: Vec<f32>,
    fin_stats: Vec<f32>,
    pol_inp: Vec<f32>,
    pol_pre: Vec<f32>,
    pol_act: Vec<f32>,
    logits: Vec<f32>,
    val_pre: Vec<f32>,
    val_act: Vec<f32>,
    value_logit: f32,
    value: f32,
}

fn sized_v1(buffer: &mut Vec<f32>, len: usize) {
    buffer.clear();
    buffer.resize(len, 0.0);
}

impl STapeV1 {
    fn reset_v1(&mut self, config: &SConfigV1, counts: CountsV1) {
        self.counts = Some(counts);
        self.size_prefix_v1(config, counts, sized_v1);
        self.layers.resize_with(config.layers, LayerTapeV1::default);
        for layer in &mut self.layers {
            layer.size_v1(config, counts, sized_v1);
        }
        self.size_tail_v1(config, counts, sized_v1);
        self.value_logit = 0.0;
        self.value = 0.0;
    }

    fn size_prefix_v1(
        &mut self,
        config: &SConfigV1,
        counts: CountsV1,
        mut size: impl FnMut(&mut Vec<f32>, usize),
    ) {
        let d = config.d_model;
        let cd = config.card_dim;
        let h = config.heads;
        let (n, e, a, r) = (counts.objects, counts.edges, counts.actions, counts.refs);
        let l = 1 + n + a;
        size(&mut self.obj_inp, n * (OBJECT_FEATURE_DIM_V1 + cd));
        size(&mut self.obj_pre, n * d);
        size(&mut self.obj_act, n * d);
        size(&mut self.state_pre, d);
        size(&mut self.state_act, d);
        size(&mut self.act_pre, a * d);
        size(&mut self.act_act, a * d);
        size(&mut self.ref_inp, r * (REF_FEATURE_DIM_V1 + cd));
        size(&mut self.ref_pre, r * d);
        size(&mut self.ref_act, r * d);
        size(&mut self.ref_out, r * d);
        size(&mut self.edge_out, e * 2 * h);
        size(&mut self.bias, h * l * l);
        let dh = config.head_dim_v1();
        size(&mut self.key_t, dh * l);
        size(&mut self.value_h, l * dh);
        size(&mut self.head_out, l * dh);
    }

    fn size_tail_v1(
        &mut self,
        config: &SConfigV1,
        counts: CountsV1,
        mut size: impl FnMut(&mut Vec<f32>, usize),
    ) {
        let d = config.d_model;
        let a = counts.actions;
        let l = 1 + counts.objects + a;
        size(&mut self.x_out, l * d);
        size(&mut self.fin, (1 + a) * d);
        size(&mut self.fin_stats, 2 * (1 + a));
        size(&mut self.pol_inp, a * 2 * d);
        size(&mut self.pol_pre, a * d);
        size(&mut self.pol_act, a * d);
        size(&mut self.logits, a);
        size(&mut self.val_pre, d);
        size(&mut self.val_act, d);
    }

    pub fn logits_v1(&self) -> &[f32] {
        &self.logits
    }

    pub fn value_v1(&self) -> f32 {
        self.value
    }

    /// The f32 bytes a forward of one decision with these counts leaves in
    /// its tape: the lengths `reset_v1` gives every buffer. A reused tape
    /// holds at least this much for the largest decision it has seen (its
    /// vectors keep their capacity). The saved-batch fixture
    /// (`s_update_fixture_v1`) reports it per row as the live-tape bound.
    pub(crate) fn bytes_for_counts_v1(config: &SConfigV1, counts: CountsV1) -> usize {
        let d = config.d_model;
        let cd = config.card_dim;
        let h = config.heads;
        let dh = config.head_dim_v1();
        let ffn = config.ffn;
        let (n, e, a, r) = (counts.objects, counts.edges, counts.actions, counts.refs);
        let l = 1 + n + a;
        // x_in, ln1, attn, x_mid, ln2 and the three qkv blocks; both norms'
        // stats; the attention probabilities; the feed-forward pair.
        let per_layer = 8 * l * d + 4 * l + h * l * l + 2 * l * ffn;
        let floats = n * (OBJECT_FEATURE_DIM_V1 + cd)
            + 2 * n * d
            + 2 * d
            + 2 * a * d
            + r * (REF_FEATURE_DIM_V1 + cd)
            + 3 * r * d
            + e * 2 * h
            + h * l * l
            + 3 * dh * l
            + config.layers * per_layer
            + l * d
            + (1 + a) * d
            + 2 * (1 + a)
            + 4 * a * d
            + a
            + 2 * d;
        floats * std::mem::size_of::<f32>()
    }

    /// The f32 bytes this tape's buffers hold at their current lengths.
    #[cfg(test)]
    pub(crate) fn live_bytes_v1(&self) -> usize {
        let mut floats = [
            &self.obj_inp,
            &self.obj_pre,
            &self.obj_act,
            &self.state_pre,
            &self.state_act,
            &self.act_pre,
            &self.act_act,
            &self.ref_inp,
            &self.ref_pre,
            &self.ref_act,
            &self.ref_out,
            &self.edge_out,
            &self.bias,
            &self.key_t,
            &self.value_h,
            &self.head_out,
            &self.x_out,
            &self.fin,
            &self.fin_stats,
            &self.pol_inp,
            &self.pol_pre,
            &self.pol_act,
            &self.logits,
            &self.val_pre,
            &self.val_act,
        ]
        .iter()
        .map(|buffer| buffer.len())
        .sum::<usize>();
        for lt in &self.layers {
            floats += [
                &lt.x_in,
                &lt.ln1,
                &lt.ln1_stats,
                &lt.qkv,
                &lt.probs,
                &lt.attn,
                &lt.x_mid,
                &lt.ln2,
                &lt.ln2_stats,
                &lt.ff_pre,
                &lt.ff_act,
            ]
            .iter()
            .map(|buffer| buffer.len())
            .sum::<usize>();
        }
        floats * std::mem::size_of::<f32>()
    }
}

/// Backward workspace.
#[derive(Clone, Debug, Default)]
pub(crate) struct SBackScratchV1 {
    dx: Vec<f32>,
    dx2: Vec<f32>,
    dwide: Vec<f32>,
    dqkv: Vec<f32>,
    dattn: Vec<f32>,
    dbias: Vec<f32>,
    dsmall: Vec<f32>,
    dsmall2: Vec<f32>,
    dfin: Vec<f32>,
    ln_scratch: Vec<f32>,
    head_q: Vec<f32>,
    head_k: Vec<f32>,
    head_vt: Vec<f32>,
    head_do: Vec<f32>,
    head_out: Vec<f32>,
    dscore: Vec<f32>,
    /// The compact final block's residual adjoint rows `[m, d]`.
    live_dx: Vec<f32>,
    /// Its gathered activation rows (`[m, ffn]` or `[m, d]`) and one head's
    /// probability rows (`[m, l]`).
    live_rows: Vec<f32>,
    live_probs: Vec<f32>,
    /// Its query and key-value parts of the QKV adjoint (`[m, d]`, `[l, 2d]`).
    live_dq: Vec<f32>,
    live_dkv: Vec<f32>,
}

/// The query rows one block's backward visits, in token order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum QueryRowsV1 {
    /// Every token `0..l`.
    All(usize),
    /// The final block's live rows: the state token, then the action tokens
    /// `1 + objects..1 + objects + actions`. Its forward computes no other
    /// query row (`forward_validated_into_v1`), because the heads read only
    /// these tokens.
    Live { objects: usize, actions: usize },
}

impl QueryRowsV1 {
    fn len(self) -> usize {
        match self {
            Self::All(tokens) => tokens,
            Self::Live { actions, .. } => 1 + actions,
        }
    }

    /// The token of row `r`: the final LayerNorm's slot order.
    fn token(self, r: usize) -> usize {
        match self {
            Self::All(_) => r,
            Self::Live { objects, .. } => {
                if r == 0 {
                    0
                } else {
                    objects + r
                }
            }
        }
    }
}

/// How the backward picks the final block's query rows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FinalRowsV1 {
    /// The live rows when `final_block_compacts_v1` allows them, else all.
    Auto,
    /// Every row: the reference the live rows must match.
    #[cfg(test)]
    All,
    /// The live rows even without the finiteness check (tests show the
    /// check is needed).
    #[cfg(test)]
    Live,
}

/// Grows `buffer` to at least `len` values. Callers write every value they
/// later read.
fn grown_v1(buffer: &mut Vec<f32>, len: usize) -> &mut [f32] {
    if buffer.len() < len {
        buffer.resize(len, 0.0);
    }
    &mut buffer[..len]
}

/// Copies `rows` (token order) of a `[tokens, width]` buffer to `out`.
fn gather_rows_v1(source: &[f32], width: usize, rows: QueryRowsV1, out: &mut Vec<f32>) {
    let out = grown_v1(out, rows.len() * width);
    for r in 0..rows.len() {
        let t = rows.token(r);
        out[r * width..(r + 1) * width].copy_from_slice(&source[t * width..(t + 1) * width]);
    }
}

/// Whether the final block's backward may visit only its live query rows
/// (`QueryRowsV1::Live`) with every bit unchanged: every Q, K and V value of
/// the block is finite.
///
/// The forward leaves the block's object rows of the probabilities,
/// attention output, residual, LayerNorm, feed-forward activations and
/// output at +0, and the final LayerNorm leaves their output adjoints at +0.
/// Each object-row term the full backward computes is then a product of
/// +0 with a finite value: a tape entry, a parameter, V (`dP = dO V^T`), K
/// (`dQ = dS K`) or the object query itself (`dK = dS^T Q`), which the
/// forward computes but never reads. Every such term is +0 or -0, and every
/// accumulator it joins starts at +0 and only adds, so it is never -0 and
/// adding a zero leaves it unchanged (an infinite or NaN accumulator stays
/// the same too). The object rows' adjoints themselves stay +0: those of the
/// residual and of the query part of the QKV adjoint, which the full path
/// computes as sums of zeros. With a non-finite Q, K or V, a skipped term
/// could be NaN, so the full backward runs and reports what it always did.
fn final_block_compacts_v1(lt: &LayerTapeV1) -> bool {
    lt.qkv.iter().all(|value| value.is_finite())
}

impl SModelV1 {
    /// Accumulates `W` and `b` gradients of `y = x W^T + b` over `rows`
    /// rows (row order), and writes `dx = dy W` when asked.
    #[allow(clippy::too_many_arguments)]
    fn linear_bwd(
        &self,
        idx: LinearIdxV1,
        rows: usize,
        x: &[f32],
        dy: &[f32],
        grad: &mut SGradientSinkV1<'_>,
        dx: Option<&mut [f32]>,
    ) {
        if rows == 0 {
            if let Some(dx) = dx {
                dx.fill(0.0);
            }
            return;
        }
        grad.linear_v1(idx, rows, x, dy);
        if let Some(dx) = dx {
            gemm_v1(
                rows,
                idx.in_dim,
                idx.out_dim,
                dy,
                idx.out_dim,
                1,
                &self.values[idx.w..idx.w + idx.out_dim * idx.in_dim],
                InitV1::Zero,
                &mut dx[..rows * idx.in_dim],
            );
        }
    }

    /// Adds one decision's gradient to `grad`, given the loss's derivative
    /// with respect to its logits and its RL value. `tape` holds this
    /// decision's forward. `grad` starts as +0 values (`vec![0.0; n]`) and
    /// holds only earlier decisions' terms.
    pub(crate) fn backward_v1(
        &self,
        input: SInputV1<'_>,
        tape: &STapeV1,
        dlogits: &[f32],
        dvalue: f32,
        grad: &mut [f32],
        ws: &mut SBackScratchV1,
    ) -> Result<(), String> {
        self.backward_into_v1(
            input,
            tape,
            dlogits,
            dvalue,
            &mut SGradientSinkV1::Direct(grad),
            ws,
        )
    }

    fn backward_into_v1(
        &self,
        input: SInputV1<'_>,
        tape: &STapeV1,
        dlogits: &[f32],
        dvalue: f32,
        grad: &mut SGradientSinkV1<'_>,
        ws: &mut SBackScratchV1,
    ) -> Result<(), String> {
        self.backward_rows_v1(input, tape, dlogits, dvalue, grad, ws, FinalRowsV1::Auto)
    }

    /// The backward with a choice of the final block's query rows. The
    /// gradient sink must start from +0 values (the callers' zeroed
    /// buffers), so that no accumulator is ever -0.
    #[allow(clippy::too_many_arguments)]
    fn backward_rows_v1(
        &self,
        input: SInputV1<'_>,
        tape: &STapeV1,
        dlogits: &[f32],
        dvalue: f32,
        grad: &mut SGradientSinkV1<'_>,
        ws: &mut SBackScratchV1,
        choice: FinalRowsV1,
    ) -> Result<(), String> {
        let config = self.layout.config;
        let counts = tape.counts.ok_or("S backward without a forward")?;
        if counts != input.validate_v1(&config)? || dlogits.len() != counts.actions {
            return Err("S backward input differs from its forward".into());
        }
        if matches!(grad, SGradientSinkV1::Direct(values) if values.len() != self.layout.total) {
            return Err("S gradient buffer has the wrong length".into());
        }
        let live = match choice {
            FinalRowsV1::Auto => final_block_compacts_v1(&tape.layers[config.layers - 1]),
            #[cfg(test)]
            FinalRowsV1::All => false,
            #[cfg(test)]
            FinalRowsV1::Live => true,
        };
        let final_rows = if live {
            QueryRowsV1::Live {
                objects: counts.objects,
                actions: counts.actions,
            }
        } else {
            QueryRowsV1::All(1 + counts.objects + counts.actions)
        };
        profile::backward_census_v1(&config, counts, final_rows.len());
        let lay = &self.layout;
        let (n, e, a, r) = (counts.objects, counts.edges, counts.actions, counts.refs);
        let d = config.d_model;
        let h = config.heads;
        let dh = config.head_dim_v1();
        let cd = config.card_dim;
        let ffn = config.ffn;
        let l = 1 + n + a;
        let width = d
            .max(ffn)
            .max(3 * d)
            .max(2 * d)
            .max(OBJECT_FEATURE_DIM_V1 + cd);
        sized_v1(&mut ws.dx, l * d);
        sized_v1(&mut ws.dx2, l * d);
        sized_v1(&mut ws.dwide, l * width);
        sized_v1(&mut ws.dqkv, l * 3 * d);
        sized_v1(&mut ws.dattn, l * d);
        sized_v1(&mut ws.dbias, h * l * l);
        sized_v1(&mut ws.dsmall, a.max(r).max(n).max(1) * width);
        sized_v1(&mut ws.dsmall2, a.max(r).max(n).max(1) * width);
        sized_v1(&mut ws.dfin, (1 + a) * d);
        sized_v1(&mut ws.ln_scratch, d);
        sized_v1(&mut ws.head_q, l * dh);
        sized_v1(&mut ws.head_k, l * dh);
        sized_v1(&mut ws.head_vt, dh * l);
        sized_v1(&mut ws.head_do, l * dh);
        sized_v1(&mut ws.head_out, l * dh);
        sized_v1(&mut ws.dscore, l * l);

        // Value head: v = tanh(z / 2).
        let heads = profile::span_v1(StageV1::BackHeads);
        let dz = dvalue * 0.5 * (1.0 - tape.value * tape.value);
        {
            let dz_row = [dz];
            let dact = &mut ws.dsmall[..d];
            self.linear_bwd(lay.val2, 1, &tape.val_act, &dz_row, grad, Some(&mut *dact));
            kernels::gelu_backward_slice_v1(&tape.val_pre, dact);
            let dpre = ws.dsmall[..d].to_vec();
            self.linear_bwd(
                lay.val0,
                1,
                &tape.fin[..d],
                &dpre,
                grad,
                Some(&mut ws.dsmall2[..d]),
            );
            ws.dfin[..d].copy_from_slice(&ws.dsmall2[..d]);
        }
        // Policy head.
        {
            let dact = &mut ws.dsmall[..a * d];
            self.linear_bwd(lay.pol2, a, &tape.pol_act, dlogits, grad, Some(&mut *dact));
            kernels::gelu_backward_slice_v1(&tape.pol_pre, dact);
            let dpre = ws.dsmall[..a * d].to_vec();
            let dinp = &mut ws.dsmall2[..a * 2 * d];
            self.linear_bwd(lay.pol0, a, &tape.pol_inp, &dpre, grad, Some(dinp));
            for j in 0..a {
                for c in 0..d {
                    ws.dfin[c] += ws.dsmall2[j * 2 * d + c];
                }
                ws.dfin[(1 + j) * d..(2 + j) * d]
                    .copy_from_slice(&ws.dsmall2[j * 2 * d + d..(j + 1) * 2 * d]);
            }
        }
        // Final LayerNorm.
        ws.dx.fill(0.0);
        {
            for slot in 0..=a {
                let t = if slot == 0 { 0 } else { n + slot };
                grad.layer_norm_v1(
                    lay.ln_g,
                    lay.ln_b,
                    &tape.x_out[t * d..(t + 1) * d],
                    tape.fin_stats[2 * slot],
                    tape.fin_stats[2 * slot + 1],
                    &self.values[lay.ln_g..lay.ln_g + d],
                    &ws.dfin[slot * d..(slot + 1) * d],
                    &mut ws.dx[t * d..(t + 1) * d],
                    &mut ws.ln_scratch,
                );
            }
        }
        drop(heads);
        // Blocks in reverse. `ws.dx` is the gradient of the block's output.
        ws.dbias.fill(0.0);
        let scale = 1.0 / (dh as f32).sqrt();
        for (layer, block) in lay.blocks.iter().enumerate().rev() {
            let lt = &tape.layers[layer];
            // The query rows: every token, except that the final block may
            // visit only its live rows (`final_block_compacts_v1`). Their
            // operands and residual adjoint rows are gathered into compact
            // `[m, width]` buffers; the full path reads the tape in place.
            let rows = if layer + 1 == config.layers {
                final_rows
            } else {
                QueryRowsV1::All(l)
            };
            let m = rows.len();
            let live = matches!(rows, QueryRowsV1::Live { .. });
            // FFN.
            let section = profile::section_v1(SectionV1::Ffn, layer);
            if live {
                gather_rows_v1(&ws.dx, d, rows, &mut ws.live_dx);
                gather_rows_v1(&lt.ff_act, ffn, rows, &mut ws.live_rows);
            }
            {
                let (x, dy) = if live {
                    (&ws.live_rows[..m * ffn], &ws.live_dx[..m * d])
                } else {
                    (&lt.ff_act[..], &ws.dx[..])
                };
                self.linear_bwd(block.ff2, m, x, dy, grad, Some(&mut ws.dwide[..m * ffn]));
            }
            if live {
                for r in 0..m {
                    let t = rows.token(r);
                    kernels::gelu_backward_slice_v1(
                        &lt.ff_pre[t * ffn..(t + 1) * ffn],
                        &mut ws.dwide[r * ffn..(r + 1) * ffn],
                    );
                }
                gather_rows_v1(&lt.ln2, d, rows, &mut ws.live_rows);
            } else {
                kernels::gelu_backward_slice_v1(&lt.ff_pre, &mut ws.dwide[..l * ffn]);
            }
            {
                let x = if live {
                    &ws.live_rows[..m * d]
                } else {
                    &lt.ln2[..]
                };
                self.linear_bwd(
                    block.ff0,
                    m,
                    x,
                    &ws.dwide[..m * ffn],
                    grad,
                    Some(&mut ws.dx2[..m * d]),
                );
            }
            drop(section);
            // LN2: dx_mid = dx_out + LN2'(dln2).
            let section = profile::section_v1(SectionV1::Ln2, layer);
            {
                let mut dmid = vec![0.0f32; d];
                for r in 0..m {
                    let t = rows.token(r);
                    grad.layer_norm_v1(
                        block.ln2_g,
                        block.ln2_b,
                        &lt.x_mid[t * d..(t + 1) * d],
                        lt.ln2_stats[2 * t],
                        lt.ln2_stats[2 * t + 1],
                        &self.values[block.ln2_g..block.ln2_g + d],
                        &ws.dx2[r * d..(r + 1) * d],
                        &mut dmid,
                        &mut ws.ln_scratch,
                    );
                    let dx = if live { &mut ws.live_dx } else { &mut ws.dx };
                    for c in 0..d {
                        dx[r * d + c] += dmid[c];
                    }
                }
            }
            drop(section);
            // Projection.
            let section = profile::section_v1(SectionV1::Proj, layer);
            if live {
                gather_rows_v1(&lt.attn, d, rows, &mut ws.live_rows);
            }
            {
                let (x, dy) = if live {
                    (&ws.live_rows[..m * d], &ws.live_dx[..m * d])
                } else {
                    (&lt.attn[..], &ws.dx[..])
                };
                self.linear_bwd(block.proj, m, x, dy, grad, Some(&mut ws.dattn[..m * d]));
            }
            if live {
                // The object rows of `ws.dx` stay +0, as the full path
                // leaves them; LN1 below adds to every row.
                for r in 0..m {
                    let t = rows.token(r);
                    ws.dx[t * d..(t + 1) * d].copy_from_slice(&ws.live_dx[r * d..(r + 1) * d]);
                }
            }
            drop(section);
            // Attention, per head, through the ordered kernels:
            // dP = dO V^T, dV = P^T dO, dS = P * (dP - rowsum(P * dP)),
            // dQ = scale dS K, dK = scale dS^T Q; dS also feeds the bias.
            // Query rows `r` (token `rows.token(r)`) against every key `j`;
            // the reductions over query rows (dV, dK) run in token order.
            let section = profile::section_v1(SectionV1::Attention, layer);
            ws.dqkv.fill(0.0);
            for head in 0..h {
                for j in 0..l {
                    for c in 0..dh {
                        ws.head_k[j * dh + c] = lt.qkv[j * 3 * d + d + head * dh + c];
                        ws.head_vt[c * l + j] = lt.qkv[j * 3 * d + 2 * d + head * dh + c];
                    }
                }
                for r in 0..m {
                    let i = rows.token(r);
                    for c in 0..dh {
                        ws.head_q[r * dh + c] = lt.qkv[i * 3 * d + head * dh + c];
                        ws.head_do[r * dh + c] = ws.dattn[r * d + head * dh + c];
                    }
                }
                let probs = if live {
                    let out = grown_v1(&mut ws.live_probs, m * l);
                    for r in 0..m {
                        let i = rows.token(r);
                        out[r * l..(r + 1) * l]
                            .copy_from_slice(&lt.probs[(head * l + i) * l..(head * l + i + 1) * l]);
                    }
                    &ws.live_probs[..m * l]
                } else {
                    &lt.probs[head * l * l..(head + 1) * l * l]
                };
                gemm_v1(
                    m,
                    l,
                    dh,
                    &ws.head_do,
                    dh,
                    1,
                    &ws.head_vt,
                    InitV1::Zero,
                    &mut ws.dscore,
                );
                gemm_v1(
                    l,
                    dh,
                    m,
                    probs,
                    1,
                    l,
                    &ws.head_do,
                    InitV1::Zero,
                    &mut ws.head_out,
                );
                for j in 0..l {
                    for c in 0..dh {
                        ws.dqkv[j * 3 * d + 2 * d + head * dh + c] = ws.head_out[j * dh + c];
                    }
                }
                for r in 0..m {
                    let i = rows.token(r);
                    let p_row = &probs[r * l..(r + 1) * l];
                    let row = &mut ws.dscore[r * l..(r + 1) * l];
                    let mut row_dot = 0.0f32;
                    for j in 0..l {
                        row_dot += p_row[j] * row[j];
                    }
                    for j in 0..l {
                        let ds = p_row[j] * (row[j] - row_dot);
                        row[j] = ds;
                        ws.dbias[(head * l + i) * l + j] += ds;
                    }
                }
                gemm_v1(
                    m,
                    dh,
                    l,
                    &ws.dscore,
                    l,
                    1,
                    &ws.head_k,
                    InitV1::Zero,
                    &mut ws.head_out,
                );
                for r in 0..m {
                    let i = rows.token(r);
                    for c in 0..dh {
                        ws.dqkv[i * 3 * d + head * dh + c] = ws.head_out[r * dh + c] * scale;
                    }
                }
                gemm_v1(
                    l,
                    dh,
                    m,
                    &ws.dscore,
                    1,
                    l,
                    &ws.head_q,
                    InitV1::Zero,
                    &mut ws.head_out,
                );
                for j in 0..l {
                    for c in 0..dh {
                        ws.dqkv[j * 3 * d + d + head * dh + c] = ws.head_out[j * dh + c] * scale;
                    }
                }
            }
            drop(section);
            // QKV. The live final block splits the weight into its query rows,
            // which only the live rows reach, and its key and value rows,
            // which every token reaches. `dx2` adds the query part's terms,
            // then the key and value terms, as the full product orders them;
            // an object row's query terms are +0 there.
            let section = profile::section_v1(SectionV1::Qkv, layer);
            if live {
                self.qkv_backward_live_v1(block.qkv, rows, lt, grad, ws);
            } else {
                self.linear_bwd(block.qkv, l, &lt.ln1, &ws.dqkv, grad, Some(&mut ws.dx2[..]));
            }
            drop(section);
            // LN1: dx_in = dx_mid + LN1'(dln1).
            let section = profile::section_v1(SectionV1::Ln1, layer);
            {
                let mut din = vec![0.0f32; d];
                for t in 0..l {
                    grad.layer_norm_v1(
                        block.ln1_g,
                        block.ln1_b,
                        &lt.x_in[t * d..(t + 1) * d],
                        lt.ln1_stats[2 * t],
                        lt.ln1_stats[2 * t + 1],
                        &self.values[block.ln1_g..block.ln1_g + d],
                        &ws.dx2[t * d..(t + 1) * d],
                        &mut din,
                        &mut ws.ln_scratch,
                    );
                    for c in 0..d {
                        ws.dx[t * d + c] += din[c];
                    }
                }
            }
            drop(section);
        }

        // Token inputs. State token.
        let _inputs = profile::span_v1(StageV1::BackInputs);
        {
            let dact = &mut ws.dsmall[..d];
            self.linear_bwd(
                lay.state2,
                1,
                &tape.state_act,
                &ws.dx[..d],
                grad,
                Some(&mut *dact),
            );
            kernels::gelu_backward_slice_v1(&tape.state_pre, dact);
            let dpre = ws.dsmall[..d].to_vec();
            self.linear_bwd(
                lay.state0,
                1,
                &input.state[..STATE_KEEP_V1],
                &dpre,
                grad,
                None,
            );
        }
        // Objects: group embedding, MLP, card rows (row 0 excluded).
        {
            let dobj = ws.dx[d..(1 + n) * d].to_vec();
            for i in 0..n {
                let g = input.object_groups[i] as usize;
                grad.add_v1(lay.group + g * d, &dobj[i * d..(i + 1) * d]);
            }
            let dact = &mut ws.dsmall[..n * d];
            self.linear_bwd(lay.obj2, n, &tape.obj_act, &dobj, grad, Some(&mut *dact));
            kernels::gelu_backward_slice_v1(&tape.obj_pre, dact);
            let dpre = ws.dsmall[..n * d].to_vec();
            let obj_in = OBJECT_FEATURE_DIM_V1 + cd;
            let dinp = &mut ws.dsmall2[..n * obj_in];
            self.linear_bwd(lay.obj0, n, &tape.obj_inp, &dpre, grad, Some(dinp));
            for i in 0..n {
                let card = input.object_card_ids[i] as usize;
                if card != 0 {
                    let first = i * obj_in + OBJECT_FEATURE_DIM_V1;
                    grad.add_v1(lay.card + card * cd, &ws.dsmall2[first..first + cd]);
                }
            }
        }
        // Actions and their references.
        {
            let dtok = ws.dx[(1 + n) * d..l * d].to_vec();
            let dact = &mut ws.dsmall[..a * d];
            self.linear_bwd(lay.act2, a, &tape.act_act, &dtok, grad, Some(&mut *dact));
            kernels::gelu_backward_slice_v1(&tape.act_pre, dact);
            let dpre = ws.dsmall[..a * d].to_vec();
            self.linear_bwd(lay.act0, a, input.action_features, &dpre, grad, None);
            if r > 0 {
                let mut dref = vec![0.0f32; r * d];
                for k in 0..r {
                    let j = input.ref_action[k] as usize;
                    dref[k * d..(k + 1) * d].copy_from_slice(&dtok[j * d..(j + 1) * d]);
                }
                let dact = &mut ws.dsmall[..r * d];
                self.linear_bwd(lay.ref2, r, &tape.ref_act, &dref, grad, Some(&mut *dact));
                kernels::gelu_backward_slice_v1(&tape.ref_pre, dact);
                let dpre = ws.dsmall[..r * d].to_vec();
                let ref_in = REF_FEATURE_DIM_V1 + cd;
                let dinp = &mut ws.dsmall2[..r * ref_in];
                self.linear_bwd(lay.ref0, r, &tape.ref_inp, &dpre, grad, Some(dinp));
                for k in 0..r {
                    let card = input.ref_card_ids[k] as usize;
                    if card != 0 {
                        let first = k * ref_in + REF_FEATURE_DIM_V1;
                        grad.add_v1(lay.card + card * cd, &ws.dsmall2[first..first + cd]);
                    }
                }
            }
        }
        // Attention-bias parameters (the bias is shared by every block).
        if e > 0 {
            let mut deb = vec![0.0f32; e * 2 * h];
            for k in 0..e {
                let s = 1 + input.edge_source[k] as usize;
                let t = 1 + input.edge_target[k] as usize;
                for head in 0..h {
                    deb[k * 2 * h + head] = ws.dbias[(head * l + s) * l + t];
                    deb[k * 2 * h + h + head] = ws.dbias[(head * l + t) * l + s];
                }
            }
            self.linear_bwd(lay.edge, e, input.edge_features, &deb, grad, None);
        }
        for k in 0..r {
            let ai = 1 + n + input.ref_action[k] as usize;
            let oi = 1 + input.ref_node[k] as usize;
            for head in 0..h {
                grad.add_v1(
                    lay.ref_bias + head,
                    &ws.dbias[(head * l + ai) * l + oi..][..1],
                );
            }
        }
        for k in 0..r {
            let ai = 1 + n + input.ref_action[k] as usize;
            let oi = 1 + input.ref_node[k] as usize;
            for head in 0..h {
                grad.add_v1(
                    lay.ref_bias + h + head,
                    &ws.dbias[(head * l + oi) * l + ai..][..1],
                );
            }
        }
        Ok(())
    }

    /// The live final block's QKV backward (`QueryRowsV1::Live`), from the
    /// adjoint `ws.dqkv` whose object query rows are +0. The weight's query
    /// rows take terms from the live rows only and its key and value rows
    /// from every token, each in token order. `dx2` adds each row's query
    /// terms before its key and value terms, as the full product orders
    /// them; an object row starts its key terms from +0, the sum its zero
    /// query terms give.
    fn qkv_backward_live_v1(
        &self,
        qkv: LinearIdxV1,
        rows: QueryRowsV1,
        lt: &LayerTapeV1,
        grad: &mut SGradientSinkV1<'_>,
        ws: &mut SBackScratchV1,
    ) {
        let QueryRowsV1::Live { objects, actions } = rows else {
            unreachable!("the full QKV backward is one Linear");
        };
        let d = self.layout.config.d_model;
        let m = rows.len();
        let l = 1 + objects + actions;
        let query = qkv.rows_v1(0, d);
        let key_value = qkv.rows_v1(d, 2 * d);
        gather_rows_v1(&lt.ln1, d, rows, &mut ws.live_rows);
        let dq = grown_v1(&mut ws.live_dq, m * d);
        for r in 0..m {
            let t = rows.token(r);
            dq[r * d..(r + 1) * d].copy_from_slice(&ws.dqkv[t * 3 * d..t * 3 * d + d]);
        }
        let dkv = grown_v1(&mut ws.live_dkv, l * 2 * d);
        for t in 0..l {
            dkv[t * 2 * d..(t + 1) * 2 * d]
                .copy_from_slice(&ws.dqkv[t * 3 * d + d..(t + 1) * 3 * d]);
        }
        let (x, dq, dkv) = (
            &ws.live_rows[..m * d],
            &ws.live_dq[..m * d],
            &ws.live_dkv[..l * 2 * d],
        );
        grad.linear_v1(query, m, x, dq);
        grad.linear_v1(key_value, l, &lt.ln1, dkv);
        let w_query = &self.values[query.w..query.w + d * d];
        let w_key_value = &self.values[key_value.w..key_value.w + 2 * d * d];
        let dx2 = &mut ws.dx2[..l * d];
        gemm_v1(1, d, d, dq, d, 1, w_query, InitV1::Zero, &mut dx2[..d]);
        dx2[d..(1 + objects) * d].fill(0.0);
        gemm_v1(
            actions,
            d,
            d,
            &dq[d..],
            d,
            1,
            w_query,
            InitV1::Zero,
            &mut dx2[(1 + objects) * d..],
        );
        gemm_v1(
            l,
            d,
            2 * d,
            dkv,
            2 * d,
            1,
            w_key_value,
            InitV1::Existing,
            dx2,
        );
    }
}

/// One substep of a physical decision: its inputs and the sampled action.
#[derive(Clone, Copy, Debug)]
pub struct SSubstepV1<'a> {
    pub input: SInputV1<'a>,
    pub selected: usize,
}

/// One RT1 group (a physical decision) with its GAE advantage and value
/// target, as `compute_gae_targets_v1` prepares them.
#[derive(Clone, Debug)]
pub struct SGroupV1<'a> {
    pub substeps: Vec<SSubstepV1<'a>>,
    pub advantage: f32,
    pub value_target: f32,
}

/// The RT1 GAE objective's terms for one batch (`gae_v1.rs`):
/// `loss = (policy_sum + c * value_sum) / G`, `policy_sum = sum_g -A_g *
/// (sum of the group's selected log-probabilities)`, `value_sum = sum_g
/// (v_g - T_g)^2` with `v_g` the first substep's value.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SLossV1 {
    pub group_count: usize,
    pub policy_sum: f32,
    pub value_sum: f32,
    /// `policy_sum / G` (over W5's unmasked group count with a mask).
    pub policy_term: f32,
    /// `value_sum / G`.
    pub value_term: f32,
    pub loss: f32,
}

/// Log-softmax under the portable math (libm 0.2.16), as the RT1 learner's
/// `selected_log_softmax_math_v1` computes it.
fn log_softmax_v1(logits: &[f32]) -> Vec<f32> {
    let math = TrainingMathV1::PortableV1;
    let maximum = logits.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let mut sum = 0.0f32;
    for logit in logits {
        sum += math.exp_f32_v1(*logit - maximum);
    }
    let log_sum = math.ln_f32_v1(sum);
    logits
        .iter()
        .map(|logit| (*logit - maximum) - log_sum)
        .collect()
}

fn gae_policy_row_v1(
    logits: &[f32],
    selected_action: usize,
    d_joint: f32,
    index: usize,
    sub: usize,
) -> Result<(f32, Vec<f32>), String> {
    if selected_action >= logits.len() {
        return Err(format!(
            "S group {index} substep {sub}: selected {selected_action} of {} actions",
            logits.len()
        ));
    }
    let log_probabilities = log_softmax_v1(logits);
    let selected = log_probabilities[selected_action];
    if !selected.is_finite() {
        return Err(format!("S group {index}: non-finite log-probability"));
    }
    let math = TrainingMathV1::PortableV1;
    let dlogits = log_probabilities
        .iter()
        .enumerate()
        .map(|(action, lp)| {
            let p = math.exp_f32_v1(*lp);
            let indicator = if action == selected_action { 1.0 } else { 0.0 };
            d_joint * (indicator - p)
        })
        .collect();
    Ok((selected, dlogits))
}

/// Reusable workspaces of one learner thread.
#[derive(Clone, Debug, Default)]
pub struct SWorkV1 {
    tape: STapeV1,
    back: SBackScratchV1,
}

impl SModelV1 {
    /// The RT1 GAE loss of `groups` and its gradient, accumulated into `grad`
    /// (which the caller fills with +0). Groups run in order; each substep's
    /// forward is followed at once by its backward, so one tape is live at a
    /// time. Gauge ranges are zeroed at the end.
    pub fn gae_loss_gradient_v1(
        &self,
        groups: &[SGroupV1<'_>],
        value_coefficient: f32,
        grad: &mut [f32],
        work: &mut SWorkV1,
    ) -> Result<SLossV1, String> {
        self.gae_loss_gradient_masked_v1(groups, value_coefficient, None, grad, work)
    }

    /// [`Self::gae_loss_gradient_v1`] with W5's policy-gradient mask
    /// (`auxiliary::SPolicyMaskV1`): a masked substep's selected
    /// log-probability leaves the policy term (its logit gradient is
    /// `0 * (indicator - p)`), and the policy term is normalized over the
    /// groups with an unmasked substep; the value term keeps the group count.
    /// Without a mask every bit is the unmasked function's.
    pub(crate) fn gae_loss_gradient_masked_v1(
        &self,
        groups: &[SGroupV1<'_>],
        value_coefficient: f32,
        mask: Option<&auxiliary::SPolicyMaskV1>,
        grad: &mut [f32],
        work: &mut SWorkV1,
    ) -> Result<SLossV1, String> {
        ensure_mxcsr_v1()?;
        if let Some(mask) = mask {
            if mask.masked.len() != groups.len()
                || mask
                    .masked
                    .iter()
                    .zip(groups)
                    .any(|(m, g)| m.len() != g.substeps.len())
            {
                return Err("S policy mask does not match the batch".into());
            }
        }
        if groups.is_empty() {
            return Err("S update has no groups".into());
        }
        if !value_coefficient.is_finite() || value_coefficient <= 0.0 {
            return Err("S value coefficient must be positive".into());
        }
        let group_count = groups.len() as f32;
        let mut policy_sum = 0.0f32;
        let mut value_sum = 0.0f32;
        for (index, group) in groups.iter().enumerate() {
            if group.substeps.is_empty() {
                return Err(format!("S group {index} has no substeps"));
            }
            if !group.advantage.is_finite() || !group.value_target.is_finite() {
                return Err(format!("S group {index} has a non-finite target"));
            }
            let masked = mask.map(|m| &m.masked[index]);
            let d_unmasked = match mask {
                None => -group.advantage / group_count,
                Some(m) => -group.advantage / m.policy_count,
            };
            let mut joint = 0.0f32;
            // The policy term's joint log-probability over unmasked substeps.
            let mut policy_joint: Option<f32> = None;
            let mut value_error = 0.0f32;
            for (sub, substep) in group.substeps.iter().enumerate() {
                let is_masked = masked.is_some_and(|m| m[sub]);
                let d_joint = if is_masked { 0.0 } else { d_unmasked };
                let update_forward =
                    update_profile_v1::span_v1(update_profile_v1::StageV1::LearnerForward);
                let forward = profile::span_v1(StageV1::Forward);
                self.forward_into_v1(substep.input, &mut work.tape)?;
                drop(forward);
                update_forward.finish_v1(&work.tape.logits, work.tape.value);
                let policy_row = profile::span_v1(StageV1::PolicyRow);
                let (selected, dlogits) =
                    gae_policy_row_v1(&work.tape.logits, substep.selected, d_joint, index, sub)?;
                drop(policy_row);
                joint = if sub == 0 { selected } else { joint + selected };
                if !is_masked {
                    policy_joint = Some(policy_joint.map_or(selected, |j| j + selected));
                }
                let dvalue = if sub == 0 {
                    value_error = work.tape.value - group.value_target;
                    (value_coefficient / group_count) * (2.0 * value_error)
                } else {
                    0.0
                };
                let update_backward =
                    update_profile_v1::span_v1(update_profile_v1::StageV1::LearnerBackward);
                let backward = profile::span_v1(StageV1::Backward);
                self.backward_v1(
                    substep.input,
                    &work.tape,
                    &dlogits,
                    dvalue,
                    grad,
                    &mut work.back,
                )?;
                drop(backward);
                update_backward.finish_plain_v1();
            }
            policy_sum += match masked {
                None => -joint * group.advantage,
                Some(_) => -policy_joint.unwrap_or(0.0) * group.advantage,
            };
            value_sum += value_error * value_error;
        }
        for range in self.layout.gauge_ranges_v1() {
            grad[range].fill(0.0);
        }
        let (loss, policy_term) = match mask {
            None => (
                (policy_sum + value_coefficient * value_sum) / group_count,
                policy_sum / group_count,
            ),
            Some(m) => (
                policy_sum / m.policy_count + value_coefficient * value_sum / group_count,
                policy_sum / m.policy_count,
            ),
        };
        let terms = SLossV1 {
            group_count: groups.len(),
            policy_sum,
            value_sum,
            policy_term,
            value_term: value_sum / group_count,
            loss,
        };
        if !loss.is_finite() || grad.iter().any(|g| !g.is_finite()) {
            return Err("S loss or gradient is not finite".into());
        }
        Ok(terms)
    }
}

/// S with its Adam state: the learner's whole train state.
#[derive(Clone, Debug, PartialEq)]
pub struct STrainStateV1 {
    model: SModelV1,
    first: Vec<f32>,
    second: Vec<f32>,
    adam_step: u64,
}

/// One Adam step, computed but not applied.
#[derive(Clone, Debug, PartialEq)]
pub struct SAdamUpdateV1 {
    pub first: Vec<f32>,
    pub second: Vec<f32>,
    pub update: Vec<f32>,
}

/// One update's result.
#[derive(Clone, Debug, PartialEq)]
pub struct SStepV1 {
    pub loss: SLossV1,
    pub adam_step: u64,
    pub after_state_sha256: String,
}

impl STrainStateV1 {
    /// A fresh optimizer state for `model`: zero first and second moments
    /// and step 0 (W5 ruling 6, XS's switch to native RL). The first update
    /// then applies the ordinary bias corrections from step 1.
    pub fn fresh_optimizer_v1(model: SModelV1) -> Self {
        let total = model.layout.total;
        Self {
            model,
            first: vec![0.0; total],
            second: vec![0.0; total],
            adam_step: 0,
        }
    }

    pub(crate) fn from_parts_v1(
        model: SModelV1,
        first: Vec<f32>,
        second: Vec<f32>,
        adam_step: u64,
    ) -> Result<Self, String> {
        let total = model.layout.total;
        if first.len() != total || second.len() != total {
            return Err("S optimizer state has the wrong length".into());
        }
        check_finite_v1("first_moments", &first)?;
        check_finite_v1("second_moments", &second)?;
        if second.iter().any(|v| *v < 0.0) {
            return Err("S second moment is negative".into());
        }
        for range in model.layout.gauge_ranges_v1() {
            if first[range.clone()]
                .iter()
                .chain(&second[range])
                .any(|v| v.to_bits() != 0)
            {
                return Err("S gauge moments must be +0".into());
            }
        }
        i32::try_from(adam_step).map_err(|_| "S Adam step overflows".to_owned())?;
        Ok(Self {
            model,
            first,
            second,
            adam_step,
        })
    }

    pub fn model_v1(&self) -> &SModelV1 {
        &self.model
    }

    pub fn adam_step_v1(&self) -> u64 {
        self.adam_step
    }

    pub fn first_moments_v1(&self) -> &[f32] {
        &self.first
    }

    pub fn second_moments_v1(&self) -> &[f32] {
        &self.second
    }

    /// One RT1 update: the GAE loss's gradient and one Adam step (betas 0.9
    /// and 0.999, epsilon 1e-8, bias corrections by the portable integer
    /// power), with net-8's equations and its exact-zero skip.
    pub fn train_step_gae_v1(
        &mut self,
        groups: &[SGroupV1<'_>],
        value_coefficient: f32,
        learning_rate: f32,
        work: &mut SWorkV1,
    ) -> Result<SStepV1, String> {
        self.train_step_gae_with_workers_v1(groups, value_coefficient, learning_rate, work, 1)
    }

    /// An ordinary GAE step with up to `workers` CPU workers. Contributions
    /// reach each parameter in the same order as in the serial step.
    pub fn train_step_gae_with_workers_v1(
        &mut self,
        groups: &[SGroupV1<'_>],
        value_coefficient: f32,
        learning_rate: f32,
        work: &mut SWorkV1,
        workers: usize,
    ) -> Result<SStepV1, String> {
        crate::expanded_deck_training_v1::validate_preparation_workers_v1(workers)?;
        let gradient = profile::span_v1(StageV1::Gradient);
        let mut grad = vec![0.0f32; self.model.layout.total];
        let loss = if workers == 1 {
            self.model
                .gae_loss_gradient_v1(groups, value_coefficient, &mut grad, work)?
        } else {
            self.model.gae_loss_gradient_parallel_v1(
                groups,
                value_coefficient,
                &mut grad,
                workers,
                work,
            )?
        };
        drop(gradient);
        let adam = profile::span_v1(StageV1::Adam);
        self.adam_step_with_gradient_v1(&grad, learning_rate)?;
        drop(adam);
        let _hash = profile::span_v1(StageV1::StateHash);
        Ok(SStepV1 {
            loss,
            adam_step: self.adam_step,
            after_state_sha256: self.state_sha256_v1(),
        })
    }

    /// One Adam step with a given gradient (gauge entries must be zero).
    pub fn adam_step_with_gradient_v1(
        &mut self,
        grad: &[f32],
        learning_rate: f32,
    ) -> Result<(), String> {
        let step = self.adam_update_v1(grad, learning_rate)?;
        let mut values = self.model.values.clone();
        for index in 0..values.len() {
            // Exactly as the step skips it: an all-+0 element keeps its bits.
            if grad[index].to_bits() == 0
                && self.first[index].to_bits() == 0
                && self.second[index].to_bits() == 0
            {
                continue;
            }
            let parameter = values[index] + step.update[index];
            if !parameter.is_finite() {
                return Err(format!("S Adam produced a non-finite value at {index}"));
            }
            values[index] = parameter;
        }
        self.model.values = values;
        self.model.rebuild_transposed_v1();
        self.first = step.first;
        self.second = step.second;
        self.adam_step += 1;
        Ok(())
    }

    /// The next Adam step without applying it: the new moments and the f32
    /// update each parameter adds (`(-step_size * m) / denominator`, +0 for
    /// an element whose gradient and moments are all +0). Receipts compare
    /// the update itself, before the parameter's own f32 rounding.
    pub fn adam_update_v1(
        &self,
        grad: &[f32],
        learning_rate: f32,
    ) -> Result<SAdamUpdateV1, String> {
        ensure_mxcsr_v1()?;
        if grad.len() != self.model.layout.total {
            return Err("S gradient has the wrong length".into());
        }
        if !learning_rate.is_finite() || learning_rate <= 0.0 {
            return Err("S learning rate must be positive".into());
        }
        for range in self.model.layout.gauge_ranges_v1() {
            if grad[range].iter().any(|g| g.to_bits() != 0) {
                return Err("S gauge gradient must be +0".into());
            }
        }
        let math = TrainingMathV1::PortableV1;
        let step = self.adam_step + 1;
        let exponent = i32::try_from(step).map_err(|_| "S Adam step overflows".to_owned())?;
        let bias_correction1 = 1.0f64 - math.powi_f64_v1(f64::from(ADAM_BETA1_V1), exponent);
        let bias_correction2 = 1.0f64 - math.powi_f64_v1(f64::from(ADAM_BETA2_V1), exponent);
        let step_size = (f64::from(learning_rate) / bias_correction1) as f32;
        let bias_correction2_sqrt = bias_correction2.sqrt() as f32;
        let mut first = self.first.clone();
        let mut second = self.second.clone();
        let mut update = vec![0.0f32; grad.len()];
        for index in 0..grad.len() {
            let gradient = grad[index];
            let previous_first = first[index];
            let previous_second = second[index];
            if gradient.to_bits() == 0
                && previous_first.to_bits() == 0
                && previous_second.to_bits() == 0
            {
                continue;
            }
            let m = previous_first + (gradient - previous_first) * (1.0 - ADAM_BETA1_V1);
            let v = previous_second * ADAM_BETA2_V1 + gradient * gradient * (1.0 - ADAM_BETA2_V1);
            let denominator = v.sqrt() / bias_correction2_sqrt + ADAM_EPSILON_V1;
            let delta = (-step_size) * m / denominator;
            if !m.is_finite() || !v.is_finite() || !delta.is_finite() || v < 0.0 {
                return Err(format!("S Adam produced a non-finite value at {index}"));
            }
            first[index] = m;
            second[index] = v;
            update[index] = delta;
        }
        Ok(SAdamUpdateV1 {
            first,
            second,
            update,
        })
    }

    /// SHA-256 of the whole train state: the schema, the config, the Adam
    /// step and every tensor of the parameters, first moments and second
    /// moments (name, shape, f32 bits) in layout order.
    pub fn state_sha256_v1(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(S_TRAIN_STATE_SCHEMA_V1.as_bytes());
        hasher.update(b"\n");
        hasher.update(
            serde_json::to_string(&self.model.layout.config)
                .expect("config serializes")
                .as_bytes(),
        );
        hasher.update(b"\n");
        hasher.update(self.adam_step.to_le_bytes());
        for (section, data) in [
            ("parameters", &self.model.values),
            ("first_moments", &self.first),
            ("second_moments", &self.second),
        ] {
            hasher.update(section.as_bytes());
            hasher.update([0u8]);
            for spec in &self.model.layout.tensors {
                hasher.update(spec.name.as_bytes());
                hasher.update([0u8]);
                for dim in &spec.shape {
                    hasher.update((*dim as u64).to_le_bytes());
                }
                for value in &data[spec.offset..spec.offset + spec.len] {
                    hasher.update(value.to_bits().to_le_bytes());
                }
            }
        }
        hex_v1(&hasher.finalize())
    }

    /// XS's frozen starting-state hash (design 6.2): this state's hash plus
    /// the trainer's named extras (RNG states, data cursors), in the order
    /// given.
    pub fn starting_state_sha256_v1(&self, extras: &[(&str, &[u8])]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(b"s-transformer-starting-state-v1\n");
        hasher.update(self.state_sha256_v1().as_bytes());
        for (name, bytes) in extras {
            hasher.update(b"\n");
            hasher.update(name.as_bytes());
            hasher.update([0u8]);
            hasher.update((bytes.len() as u64).to_le_bytes());
            hasher.update(bytes);
        }
        hex_v1(&hasher.finalize())
    }
}

/// Pins the calling thread's MXCSR (round to nearest, no FTZ or DAZ) on
/// first use and verifies it on every call: S's arithmetic entry points run
/// only in that state.
fn ensure_mxcsr_v1() -> Result<(), String> {
    crate::deterministic_math_v1::ensure_thread_mxcsr_normalized_v1()
        .map_err(|e| format!("S arithmetic: MXCSR not pinned: {e:?}"))
}

pub(crate) fn hex_v1(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
