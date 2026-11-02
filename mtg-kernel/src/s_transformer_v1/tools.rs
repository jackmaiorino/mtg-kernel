//! E3's receipt tools for native S (the `s_transformer_v1` binary):
//!
//! - `parity-dump`: frozen batches, S's initial parameters and, per batch,
//!   the native loss terms, the gradient, one Adam step's update from a fresh
//!   and from a warmed optimizer state, and the warmed step's stored
//!   parameters, for the float64
//!   PyTorch reference (`python/tools/s_transformer_reference_v1.py parity`).
//! - `logits-dump`: logits and values of a model (S, or a net-8 V4 endpoint)
//!   on fixed states, for the Linux-to-Windows parity receipt
//!   (`s_transformer_reference_v1.py compare-logits`).
//! - `bench`: S's forward cost per decision against net-8's on the same
//!   states.
//! - `resume-check`: run-to-run and checkpoint-resume identity of after-state
//!   hashes.
//!
//! Decisions are stored in the bignet pilot's `s4a-tensor-dump` record
//! format (`regret_census_v1/stage4a/tensor_dump.rs`), so real recorded
//! states can replace the synthetic ones (`states=td:<path>`).

#![allow(clippy::needless_range_loop)]

use super::{
    decode_checkpoint_v1, encode_checkpoint_v1, hex_v1, SConfigV1, SGroupV1, SInputV1, SModelV1,
    SSubstepV1, STapeV1, STrainStateV1, SWorkV1, SplitMixV1, ACTION_FEATURE_DIM_V1,
    EDGE_FEATURE_DIM_V1, OBJECT_FEATURE_DIM_V1, OBJECT_GROUP_COUNT_V1, REF_FEATURE_DIM_V1,
    STATE_DIM_V1, STATE_KEEP_V1,
};
use crate::native_flat_tensorizer_v2::NativeFlatDecisionTensorV2;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::time::Instant;

/// The RT1 recipe's constants (W5 design section 2).
const RT1_VALUE_COEFFICIENT_V1: f32 = 0.5;
const RT1_LEARNING_RATE_V1: f32 = 1.0e-4;
const DEFAULT_SEED_V1: u64 = 20_261_102;
/// Updates that warm the parity dump's second optimizer state.
const WARM_STEPS_V1: usize = 8;

/// Inclusive count ranges of synthetic decisions.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ShapeV1 {
    pub(crate) objects: (usize, usize),
    pub(crate) edges: (usize, usize),
    pub(crate) actions: (usize, usize),
    pub(crate) refs: (usize, usize),
}

/// Wide variety, for parity: single-action decisions up to 77 tokens.
pub(crate) const PARITY_SHAPE_V1: ShapeV1 = ShapeV1 {
    objects: (1, 60),
    edges: (0, 12),
    actions: (1, 16),
    refs: (0, 12),
};

/// Centred on the S4a store's mean shape (33 objects, 4 edges, 5 actions,
/// 5 references; pilot `macs.py`), for timing.
pub(crate) const MEAN_SHAPE_V1: ShapeV1 = ShapeV1 {
    objects: (20, 46),
    edges: (0, 8),
    actions: (2, 8),
    refs: (0, 10),
};

fn pick_v1(rng: &mut SplitMixV1, (low, high): (usize, usize)) -> usize {
    low + rng.below((high - low + 1) as u64) as usize
}

/// Mostly zeros, some flags, some counts and some fractions.
fn feature_v1(rng: &mut SplitMixV1) -> f32 {
    match rng.below(10) {
        0..=5 => 0.0,
        6 => 1.0,
        7 => (1 + rng.below(20)) as f32,
        8 => rng.uniform(-1.0, 1.0),
        _ => rng.uniform(0.0, 1.0),
    }
}

fn digest_v1(rng: &mut SplitMixV1) -> f32 {
    if rng.below(2) == 0 {
        -1.0
    } else {
        1.0
    }
}

fn card_v1(rng: &mut SplitMixV1) -> i64 {
    if rng.below(5) == 0 {
        0
    } else {
        1 + rng.below(192) as i64
    }
}

/// One synthetic FlatV4-shaped decision (192-card registry tokens).
pub(crate) fn synthetic_decision_v1(
    rng: &mut SplitMixV1,
    shape: ShapeV1,
) -> NativeFlatDecisionTensorV2 {
    let n = pick_v1(rng, shape.objects);
    let e = pick_v1(rng, shape.edges);
    let a = pick_v1(rng, shape.actions);
    let r = pick_v1(rng, shape.refs);
    let mut t = NativeFlatDecisionTensorV2::default();
    for index in 0..STATE_DIM_V1 {
        let value = if index < STATE_KEEP_V1 {
            feature_v1(rng)
        } else {
            digest_v1(rng)
        };
        t.state.push(value);
    }
    for i in 0..n {
        for _ in 0..OBJECT_FEATURE_DIM_V1 {
            let value = feature_v1(rng);
            t.object_features.push(value);
        }
        t.object_card_ids.push(card_v1(rng));
        t.object_groups
            .push(rng.below(OBJECT_GROUP_COUNT_V1 as u64) as i64);
        t.object_node_ids.push(i as i64);
    }
    for _ in 0..e {
        for _ in 0..EDGE_FEATURE_DIM_V1 {
            let value = if rng.below(4) == 0 { 1.0 } else { 0.0 };
            t.edge_features.push(value);
        }
        t.edge_source_indices.push(rng.below(n as u64) as i64);
        t.edge_target_indices.push(rng.below(n as u64) as i64);
    }
    for _ in 0..a {
        for index in 0..ACTION_FEATURE_DIM_V1 {
            let value = if index < ACTION_FEATURE_DIM_V1 - 96 {
                feature_v1(rng)
            } else {
                digest_v1(rng)
            };
            t.action_features.push(value);
        }
    }
    for _ in 0..r {
        for _ in 0..REF_FEATURE_DIM_V1 {
            let value = feature_v1(rng);
            t.action_ref_features.push(value);
        }
        t.action_ref_card_ids.push(card_v1(rng));
        t.action_ref_action_indices.push(rng.below(a as u64) as i64);
        t.action_ref_node_indices.push(rng.below(n as u64) as i64);
    }
    t
}

fn put_f32s_v1(out: &mut Vec<u8>, values: &[f32]) {
    for value in values {
        out.extend_from_slice(&value.to_bits().to_le_bytes());
    }
}

fn put_i32s_v1(out: &mut Vec<u8>, values: &[i64]) {
    for value in values {
        out.extend_from_slice(&(*value as i32).to_le_bytes());
    }
}

/// Appends one `s4a-tensor-dump` record.
pub(crate) fn put_record_v1(
    out: &mut Vec<u8>,
    t: &NativeFlatDecisionTensorV2,
    selected: usize,
    logits: &[f32],
    value: f32,
) {
    let n = t.object_card_ids.len();
    let e = t.edge_source_indices.len();
    let a = t.action_features.len() / ACTION_FEATURE_DIM_V1;
    let r = t.action_ref_card_ids.len();
    for field in [n, e, a, r, t.state.len(), selected, a, 0] {
        out.extend_from_slice(&(field as u32).to_le_bytes());
    }
    put_f32s_v1(out, &t.state);
    put_f32s_v1(out, &t.object_features);
    put_i32s_v1(out, &t.object_card_ids);
    put_i32s_v1(out, &t.object_groups);
    put_i32s_v1(out, &t.object_node_ids);
    put_f32s_v1(out, &t.edge_features);
    put_i32s_v1(out, &t.edge_source_indices);
    put_i32s_v1(out, &t.edge_target_indices);
    put_f32s_v1(out, &t.action_features);
    put_f32s_v1(out, &t.action_ref_features);
    put_i32s_v1(out, &t.action_ref_card_ids);
    put_i32s_v1(out, &t.action_ref_action_indices);
    put_i32s_v1(out, &t.action_ref_node_indices);
    let mut stored = logits.to_vec();
    stored.resize(a, 0.0);
    put_f32s_v1(out, &stored);
    put_f32s_v1(out, &[value]);
}

struct ReaderV1<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl ReaderV1<'_> {
    fn word(&mut self) -> Result<[u8; 4], String> {
        let chunk = self
            .bytes
            .get(self.at..self.at + 4)
            .ok_or("tensor-dump record is truncated")?;
        self.at += 4;
        Ok(chunk.try_into().expect("4 bytes"))
    }

    fn f32s(&mut self, count: usize) -> Result<Vec<f32>, String> {
        (0..count)
            .map(|_| self.word().map(|w| f32::from_bits(u32::from_le_bytes(w))))
            .collect()
    }

    fn i32s(&mut self, count: usize) -> Result<Vec<i64>, String> {
        (0..count)
            .map(|_| self.word().map(|w| i64::from(i32::from_le_bytes(w))))
            .collect()
    }
}

/// Reads every record of a tensor-dump file: tensors and selected actions.
pub(crate) fn read_records_v1(
    bytes: &[u8],
) -> Result<Vec<(NativeFlatDecisionTensorV2, usize)>, String> {
    let mut reader = ReaderV1 { bytes, at: 0 };
    let mut out = Vec::new();
    while reader.at < bytes.len() {
        let mut header = [0usize; 8];
        for slot in &mut header {
            *slot = u32::from_le_bytes(reader.word()?) as usize;
        }
        let [n, e, a, r, s, selected, legal, _actor] = header;
        if s != STATE_DIM_V1 || legal != a {
            return Err(format!(
                "bad tensor-dump record: state {s}, legal {legal}, actions {a}"
            ));
        }
        let t = NativeFlatDecisionTensorV2 {
            state: reader.f32s(s)?,
            object_features: reader.f32s(n * OBJECT_FEATURE_DIM_V1)?,
            object_card_ids: reader.i32s(n)?,
            object_groups: reader.i32s(n)?,
            object_node_ids: reader.i32s(n)?,
            edge_features: reader.f32s(e * EDGE_FEATURE_DIM_V1)?,
            edge_source_indices: reader.i32s(e)?,
            edge_target_indices: reader.i32s(e)?,
            action_features: reader.f32s(a * ACTION_FEATURE_DIM_V1)?,
            action_ref_features: reader.f32s(r * REF_FEATURE_DIM_V1)?,
            action_ref_card_ids: reader.i32s(r)?,
            action_ref_action_indices: reader.i32s(r)?,
            action_ref_node_indices: reader.i32s(r)?,
        };
        reader.f32s(a + 1)?;
        out.push((t, selected));
    }
    Ok(out)
}

fn sha256_hex_v1(bytes: &[u8]) -> String {
    hex_v1(&Sha256::digest(bytes))
}

fn write_v1(path: &Path, bytes: &[u8]) -> Result<String, String> {
    std::fs::write(path, bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(sha256_hex_v1(bytes))
}

/// One synthetic RT1 batch: groups of 1 to 3 substeps, advantages in
/// [-2, 2] and value targets in [-1, 1].
struct SyntheticGroupV1 {
    records: Vec<(NativeFlatDecisionTensorV2, usize)>,
    advantage: f32,
    value_target: f32,
}

fn synthetic_batch_v1(
    rng: &mut SplitMixV1,
    groups: usize,
    shape: ShapeV1,
) -> Vec<SyntheticGroupV1> {
    (0..groups)
        .map(|_| {
            let substeps = 1 + usize::from(rng.below(4) == 0) + usize::from(rng.below(8) == 0);
            let records = (0..substeps)
                .map(|_| {
                    let t = synthetic_decision_v1(rng, shape);
                    let a = t.action_features.len() / ACTION_FEATURE_DIM_V1;
                    let selected = rng.below(a as u64) as usize;
                    (t, selected)
                })
                .collect();
            SyntheticGroupV1 {
                records,
                advantage: rng.uniform(-2.0, 2.0),
                value_target: rng.uniform(-1.0, 1.0),
            }
        })
        .collect()
}

fn groups_of_v1(batch: &[SyntheticGroupV1]) -> Vec<SGroupV1<'_>> {
    batch
        .iter()
        .map(|group| SGroupV1 {
            substeps: group
                .records
                .iter()
                .map(|(t, selected)| SSubstepV1 {
                    input: SInputV1::of_tensor_v1(t),
                    selected: *selected,
                })
                .collect(),
            advantage: group.advantage,
            value_target: group.value_target,
        })
        .collect()
}

fn layout_json_v1(model: &SModelV1) -> Value {
    let layout = model.layout_v1();
    json!({
        "architecture": super::S_ARCHITECTURE_V1,
        "config": layout.config,
        "total": layout.total,
        "tensors": layout.tensors.iter().map(|t| json!({
            "name": t.name, "shape": t.shape, "offset": t.offset,
        })).collect::<Vec<_>>(),
        "gauge_ranges": layout.gauge_ranges_v1().iter().map(|r| [r.start, r.end]).collect::<Vec<_>>(),
    })
}

/// `parity-dump`: see the module doc.
pub fn parity_dump_v1(
    out: &Path,
    seed: u64,
    batches: usize,
    groups: usize,
) -> Result<Value, String> {
    std::fs::create_dir_all(out).map_err(|e| e.to_string())?;
    let config = SConfigV1::s_v1();
    let model = SModelV1::init_parity_v1(config, seed)?;
    let mut rng = SplitMixV1::new(seed ^ 0x5eed_ba7c_0000_0001);
    let mut records = Vec::new();
    let mut native = Vec::new();
    let mut batch_rows = Vec::new();
    let mut record_count = 0usize;
    let mut work = SWorkV1::default();
    let started = Instant::now();
    // A warmed optimizer state: S's moments after WARM_STEPS_V1 native updates
    // from the parity point on separate batches. Each parity batch also takes
    // one Adam step from it, with its own gradient at the parity point.
    let mut warm = STrainStateV1::fresh_optimizer_v1(model.clone());
    for _ in 0..WARM_STEPS_V1 {
        let batch = synthetic_batch_v1(&mut rng, groups, PARITY_SHAPE_V1);
        warm.train_step_gae_v1(
            &groups_of_v1(&batch),
            RT1_VALUE_COEFFICIENT_V1,
            RT1_LEARNING_RATE_V1,
            &mut work,
        )?;
    }
    let warm_first = warm.first_moments_v1().to_vec();
    let warm_second = warm.second_moments_v1().to_vec();
    for _ in 0..batches {
        let batch = synthetic_batch_v1(&mut rng, groups, PARITY_SHAPE_V1);
        let mut group_rows = Vec::new();
        for group in &batch {
            let mut indices = Vec::new();
            for (t, selected) in &group.records {
                put_record_v1(&mut records, t, *selected, &[], 0.0);
                indices.push(record_count);
                record_count += 1;
            }
            group_rows.push(json!({
                "records": indices,
                "advantage_bits": group.advantage.to_bits(),
                "value_target_bits": group.value_target.to_bits(),
            }));
        }
        batch_rows.push(json!({ "groups": group_rows }));
        let sgroups = groups_of_v1(&batch);
        let fresh = STrainStateV1::fresh_optimizer_v1(model.clone());
        let mut grad = vec![0.0f32; model.layout_v1().total];
        let loss =
            model.gae_loss_gradient_v1(&sgroups, RT1_VALUE_COEFFICIENT_V1, &mut grad, &mut work)?;
        let fresh_update = fresh.adam_update_v1(&grad, RT1_LEARNING_RATE_V1)?;
        let mut warmed = STrainStateV1::from_parts_v1(
            model.clone(),
            warm_first.clone(),
            warm_second.clone(),
            WARM_STEPS_V1 as u64,
        )?;
        let warm_update = warmed.adam_update_v1(&grad, RT1_LEARNING_RATE_V1)?;
        warmed.adam_step_with_gradient_v1(&grad, RT1_LEARNING_RATE_V1)?;
        put_f32s_v1(&mut native, &[loss.policy_term, loss.value_term, loss.loss]);
        put_f32s_v1(&mut native, &grad);
        put_f32s_v1(&mut native, &fresh_update.update);
        put_f32s_v1(&mut native, &warm_update.update);
        put_f32s_v1(&mut native, warmed.model_v1().values_v1());
    }
    let mut warm_first_bytes = Vec::new();
    put_f32s_v1(&mut warm_first_bytes, &warm_first);
    let mut warm_second_bytes = Vec::new();
    put_f32s_v1(&mut warm_second_bytes, &warm_second);
    let mut params = Vec::new();
    put_f32s_v1(&mut params, model.values_v1());
    let batches_json = json!({
        "schema": "s-parity-batches-v1",
        "seed": seed,
        "value_coefficient_bits": RT1_VALUE_COEFFICIENT_V1.to_bits(),
        "learning_rate_bits": RT1_LEARNING_RATE_V1.to_bits(),
        "warm_adam_step": WARM_STEPS_V1,
        "batches": batch_rows,
    });
    let files = json!({
        "decisions.td": write_v1(&out.join("decisions.td"), &records)?,
        "batches.json": write_v1(&out.join("batches.json"), &serde_json::to_vec(&batches_json).map_err(|e| e.to_string())?)?,
        "params.bin": write_v1(&out.join("params.bin"), &params)?,
        "layout.json": write_v1(&out.join("layout.json"), &serde_json::to_vec(&layout_json_v1(&model)).map_err(|e| e.to_string())?)?,
        "native.bin": write_v1(&out.join("native.bin"), &native)?,
        "warm_first.bin": write_v1(&out.join("warm_first.bin"), &warm_first_bytes)?,
        "warm_second.bin": write_v1(&out.join("warm_second.bin"), &warm_second_bytes)?,
    });
    Ok(json!({
        "schema": "s-parity-dump-v1",
        "seed": seed,
        "batches": batches,
        "groups_per_batch": groups,
        "decisions": record_count,
        "parameter_sha256": model.parameter_sha256_v1(),
        "files_sha256": files,
        "native_seconds": started.elapsed().as_secs_f64(),
    }))
}

/// The states a dump reads.
fn load_states_v1(spec: &str) -> Result<(Vec<NativeFlatDecisionTensorV2>, String), String> {
    if let Some(rest) = spec.strip_prefix("synthetic:") {
        let (seed, count) = rest
            .split_once(':')
            .ok_or("states=synthetic:<seed>:<count>")?;
        let seed: u64 = seed.parse().map_err(|_| "bad states seed")?;
        let count: usize = count.parse().map_err(|_| "bad states count")?;
        let mut rng = SplitMixV1::new(seed);
        let states = (0..count)
            .map(|_| synthetic_decision_v1(&mut rng, MEAN_SHAPE_V1))
            .collect();
        Ok((
            states,
            format!("synthetic-mean-shape seed {seed} count {count}"),
        ))
    } else if let Some(path) = spec.strip_prefix("td:") {
        let bytes = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
        let digest = sha256_hex_v1(&bytes);
        let states = read_records_v1(&bytes)?
            .into_iter()
            .map(|(t, _)| t)
            .collect();
        Ok((states, format!("td {path} sha256 {digest}")))
    } else {
        Err("states=synthetic:<seed>:<count> or states=td:<path>".into())
    }
}

/// Scores states with one model.
enum ScorerV1 {
    S(Box<SModelV1>, Box<STapeV1>),
    Net8(Box<crate::sideboard_play_policy_v1::FrozenPlayPolicyV1>),
    Net8Fixed(Box<crate::native_policy_value_net_v1::NativePolicyValueNetV1>),
}

impl ScorerV1 {
    fn load(spec: &str) -> Result<(Self, Value), String> {
        if let Some(seed) = spec.strip_prefix("s-parity:") {
            let model =
                SModelV1::init_parity_v1(SConfigV1::s_v1(), seed.parse().map_err(|_| "bad seed")?)?;
            let identity = json!({"family": "s", "init": spec, "parameter_sha256": model.parameter_sha256_v1()});
            Ok((Self::S(Box::new(model), Box::default()), identity))
        } else if let Some(seed) = spec.strip_prefix("s-pilot:") {
            let model = SModelV1::init_pilot_scheme_v1(
                SConfigV1::s_v1(),
                seed.parse().map_err(|_| "bad seed")?,
            )?;
            let identity = json!({"family": "s", "init": spec, "parameter_sha256": model.parameter_sha256_v1()});
            Ok((Self::S(Box::new(model), Box::default()), identity))
        } else if let Some(path) = spec.strip_prefix("s-checkpoint:") {
            let bytes = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
            let state = decode_checkpoint_v1(&bytes)?;
            let identity = json!({
                "family": "s", "checkpoint": path, "checkpoint_sha256": sha256_hex_v1(&bytes),
                "state_sha256": state.state_sha256_v1(),
                "parameter_sha256": state.model_v1().parameter_sha256_v1(),
            });
            Ok((
                Self::S(Box::new(state.model_v1().clone()), Box::default()),
                identity,
            ))
        } else if let Some(rest) = spec.strip_prefix("net8-source:") {
            let (path, math) = rest
                .rsplit_once(':')
                .ok_or("model=net8-source:<source.json>:<platform|portable>")?;
            let math = match math {
                "platform" => crate::training_math_v1::TrainingMathV1::PlatformLibm,
                "portable" => crate::training_math_v1::TrainingMathV1::PortableV1,
                _ => return Err("net8 math must be platform or portable".into()),
            };
            let bytes = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
            let source: crate::expanded_deck_training_v1::ExpandedModelSourceV1 =
                serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
            let (policy, identity) =
                crate::expanded_deck_training_v1::load_expanded_inference_bound_v1(&source, math)?;
            if policy.feature_identity_v1().feature_contract_digest
                != crate::native_flat_tensorizer_v4::FEATURE_CONTRACT_DIGEST_V4
            {
                return Err("the logits receipt reads FlatV4 net-8 endpoints only".into());
            }
            let identity = json!({
                "family": "net8", "source": path, "source_sha256": sha256_hex_v1(&bytes),
                "math": format!("{math:?}"), "inference": identity,
            });
            Ok((Self::Net8(Box::new(policy)), identity))
        } else if spec == "net8-fixed" {
            let model = crate::native_policy_value_net_v1::NativePolicyValueNetV1::runner_fixed_v1(
                crate::native_policy_value_net_v1::NativePolicyValueModelConfigV1::contract_v1(),
            )
            .map_err(|e| format!("{e:?}"))?;
            Ok((
                Self::Net8Fixed(Box::new(model)),
                json!({"family": "net8", "init": "runner-fixed-v1", "math": "portable"}),
            ))
        } else {
            Err("model=s-parity:<seed> | s-pilot:<seed> | s-checkpoint:<path> | net8-source:<path>:<math> | net8-fixed".into())
        }
    }

    fn score(&mut self, t: &NativeFlatDecisionTensorV2) -> Result<(Vec<f32>, f32), String> {
        match self {
            Self::S(model, tape) => {
                model.forward_into_v1(SInputV1::of_tensor_v1(t), tape)?;
                Ok((tape.logits_v1().to_vec(), tape.value_v1()))
            }
            Self::Net8(policy) => {
                let tensor = crate::native_flat_tensorizer_v4::NativeFlatDecisionTensorV4 {
                    common: t.clone(),
                };
                let scores = policy.diagnostic_forward_v4(&tensor)?;
                Ok((scores.logits, scores.value))
            }
            Self::Net8Fixed(model) => {
                let tensor = crate::native_flat_tensorizer_v4::NativeFlatDecisionTensorV4 {
                    common: t.clone(),
                };
                let output = model
                    .forward_feature_transfer_v4_mode(
                        crate::native_flat_tensorizer_v4::encoded_decision_view_v4(&tensor),
                        crate::native_policy_value_net_v1::ForwardActivationModeV1::KernelDeterministicTanh,
                    )
                    .map_err(|e| format!("{e:?}"))?;
                Ok((output.logits, output.value))
            }
        }
    }
}

/// `logits-dump`: `SLOGIT01`, u64 count, then per state u32 action count,
/// its logits and its value (f32 little-endian).
pub fn logits_dump_v1(out: &Path, model: &str, states: &str) -> Result<Value, String> {
    let (mut scorer, identity) = ScorerV1::load(model)?;
    let (states, states_identity) = load_states_v1(states)?;
    let mut bytes = b"SLOGIT01".to_vec();
    bytes.extend_from_slice(&(states.len() as u64).to_le_bytes());
    let started = Instant::now();
    for t in &states {
        let (logits, value) = scorer.score(t)?;
        bytes.extend_from_slice(&(logits.len() as u32).to_le_bytes());
        put_f32s_v1(&mut bytes, &logits);
        put_f32s_v1(&mut bytes, &[value]);
    }
    let seconds = started.elapsed().as_secs_f64();
    Ok(json!({
        "schema": "s-logits-dump-v1",
        "model": identity,
        "states": states_identity,
        "count": states.len(),
        "sha256": write_v1(out, &bytes)?,
        "os": std::env::consts::OS,
        "seconds": seconds,
    }))
}

fn time_per_decision_v1(
    reps: usize,
    states: &[NativeFlatDecisionTensorV2],
    mut score: impl FnMut(&NativeFlatDecisionTensorV2) -> Result<(), String>,
) -> Result<f64, String> {
    for t in states.iter().take(64) {
        score(t)?;
    }
    let mut best = f64::INFINITY;
    for _ in 0..reps {
        let started = Instant::now();
        for t in states {
            score(t)?;
        }
        best = best.min(started.elapsed().as_secs_f64() / states.len() as f64);
    }
    Ok(best * 1e6)
}

/// `bench`: microseconds per decision (best of `reps` passes, one thread).
pub fn bench_v1(seed: u64, count: usize, reps: usize) -> Result<Value, String> {
    let mut rng = SplitMixV1::new(seed);
    let states: Vec<_> = (0..count)
        .map(|_| synthetic_decision_v1(&mut rng, MEAN_SHAPE_V1))
        .collect();
    let tokens: usize = states
        .iter()
        .map(|t| 1 + t.object_card_ids.len() + t.action_features.len() / ACTION_FEATURE_DIM_V1)
        .sum();
    let s_model = SModelV1::init_pilot_scheme_v1(SConfigV1::s_v1(), seed)?;
    let mut tape = STapeV1::default();
    let s_us = time_per_decision_v1(reps, &states, |t| {
        s_model.forward_into_v1(SInputV1::of_tensor_v1(t), &mut tape)
    })?;
    let net8 = crate::native_policy_value_net_v1::NativePolicyValueNetV1::runner_fixed_v1(
        crate::native_policy_value_net_v1::NativePolicyValueModelConfigV1::contract_v1(),
    )
    .map_err(|e| format!("{e:?}"))?;
    let mut net8_us = Vec::new();
    for mode in [
        crate::native_policy_value_net_v1::ForwardActivationModeV1::KernelDeterministicTanh,
        crate::native_policy_value_net_v1::ForwardActivationModeV1::LibmTanh,
    ] {
        let tensors: Vec<_> = states
            .iter()
            .map(
                |t| crate::native_flat_tensorizer_v4::NativeFlatDecisionTensorV4 {
                    common: t.clone(),
                },
            )
            .collect();
        let mut index = 0usize;
        let us = time_per_decision_v1(reps, &states, |_| {
            let tensor = &tensors[index % tensors.len()];
            index += 1;
            net8.forward_feature_transfer_v4_mode(
                crate::native_flat_tensorizer_v4::encoded_decision_view_v4(tensor),
                mode,
            )
            .map(|_| ())
            .map_err(|e| format!("{e:?}"))
        })?;
        net8_us.push(us);
    }
    // Learner cost: forward plus backward per decision (one-substep groups).
    let groups: Vec<SGroupV1<'_>> = states
        .iter()
        .take(256)
        .map(|t| SGroupV1 {
            substeps: vec![SSubstepV1 {
                input: SInputV1::of_tensor_v1(t),
                selected: 0,
            }],
            advantage: 0.5,
            value_target: 0.0,
        })
        .collect();
    let mut work = SWorkV1::default();
    let mut grad = vec![0.0f32; s_model.layout_v1().total];
    let started = Instant::now();
    s_model.gae_loss_gradient_v1(&groups, RT1_VALUE_COEFFICIENT_V1, &mut grad, &mut work)?;
    let learner_us = started.elapsed().as_secs_f64() / groups.len() as f64 * 1e6;
    Ok(json!({
        "schema": "s-bench-v1",
        "os": std::env::consts::OS,
        "avx2": cfg!(target_arch = "x86_64") && avx2_v1(),
        "states": count,
        "mean_tokens": tokens as f64 / count as f64,
        "s_forward_us": s_us,
        "net8_forward_portable_us": net8_us[0],
        "net8_forward_platform_us": net8_us[1],
        "ratio_s_to_net8_portable": s_us / net8_us[0],
        "ratio_s_to_net8_platform": s_us / net8_us[1],
        "s_learner_forward_backward_us": learner_us,
    }))
}

fn avx2_v1() -> bool {
    #[cfg(target_arch = "x86_64")]
    {
        std::arch::is_x86_feature_detected!("avx2")
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        false
    }
}

fn run_steps_v1(
    state: &mut STrainStateV1,
    batches: &[Vec<SyntheticGroupV1>],
    range: std::ops::Range<usize>,
) -> Result<Vec<String>, String> {
    let mut work = SWorkV1::default();
    let mut hashes = Vec::new();
    for index in range {
        let groups = groups_of_v1(&batches[index]);
        let step = state.train_step_gae_v1(
            &groups,
            RT1_VALUE_COEFFICIENT_V1,
            RT1_LEARNING_RATE_V1,
            &mut work,
        )?;
        hashes.push(step.after_state_sha256);
    }
    Ok(hashes)
}

/// `resume-check`: `steps` updates three ways (straight, straight again, and
/// split at `split` through a checkpoint file); every after-state hash must
/// agree.
pub fn resume_check_v1(
    out: &Path,
    seed: u64,
    steps: usize,
    split: usize,
    groups: usize,
) -> Result<Value, String> {
    if split == 0 || split >= steps {
        return Err("resume-check needs 0 < split < steps".into());
    }
    std::fs::create_dir_all(out).map_err(|e| e.to_string())?;
    let mut rng = SplitMixV1::new(seed ^ 0x7e50_0e00_0000_0002);
    let batches: Vec<_> = (0..steps)
        .map(|_| synthetic_batch_v1(&mut rng, groups, PARITY_SHAPE_V1))
        .collect();
    let model = SModelV1::init_parity_v1(SConfigV1::s_v1(), seed)?;
    let fresh = STrainStateV1::fresh_optimizer_v1(model);
    let start_hash = fresh.state_sha256_v1();
    let starting = fresh.starting_state_sha256_v1(&[("seed", &seed.to_le_bytes()[..])]);
    let mut first = fresh.clone();
    let straight = run_steps_v1(&mut first, &batches, 0..steps)?;
    let mut second = fresh.clone();
    let again = run_steps_v1(&mut second, &batches, 0..steps)?;
    let mut third = fresh;
    let mut resumed = run_steps_v1(&mut third, &batches, 0..split)?;
    let path: PathBuf = out.join("resume.mtgckpt");
    let checkpoint = encode_checkpoint_v1(&third);
    let checkpoint_sha256 = write_v1(&path, &checkpoint)?;
    let mut restored = decode_checkpoint_v1(&std::fs::read(&path).map_err(|e| e.to_string())?)?;
    resumed.extend(run_steps_v1(&mut restored, &batches, split..steps)?);
    let identical = straight == again && straight == resumed;
    let receipt = json!({
        "schema": "s-resume-check-v1",
        "seed": seed,
        "steps": steps,
        "split": split,
        "groups_per_batch": groups,
        "fresh_state_sha256": start_hash,
        "starting_state_sha256": starting,
        "after_state_sha256": straight,
        "run_to_run_identical": straight == again,
        "resume_identical": straight == resumed,
        "checkpoint_sha256": checkpoint_sha256,
        "checkpoint_bytes": checkpoint.len(),
    });
    if !identical {
        return Err(format!("S determinism check failed: {receipt}"));
    }
    Ok(receipt)
}

/// `import-values`: an S state from PyTorch (`s_transformer_reference_v1.py
/// export-values`): `values` holds f32 little-endian parameters in `names`'s
/// order, which must equal S's layout (names and shapes). Writes a
/// checkpoint with a fresh optimizer (W5 ruling 6) and prints its hashes.
pub fn import_values_v1(values: &Path, names: &Path, out: &Path) -> Result<Value, String> {
    let config = SConfigV1::s_v1();
    let layout = super::LayoutV1::of_v1(config)?;
    let listed: Vec<(String, Vec<usize>)> = serde_json::from_slice(
        &std::fs::read(names).map_err(|e| format!("{}: {e}", names.display()))?,
    )
    .map_err(|e| e.to_string())?;
    let expected: Vec<(String, Vec<usize>)> = layout
        .tensors
        .iter()
        .map(|t| (t.name.clone(), t.shape.clone()))
        .collect();
    if listed != expected {
        return Err("exported parameter names or shapes differ from S's layout".into());
    }
    let bytes = std::fs::read(values).map_err(|e| format!("{}: {e}", values.display()))?;
    if bytes.len() != layout.total * 4 {
        return Err(format!(
            "{} holds {} bytes, S needs {}",
            values.display(),
            bytes.len(),
            layout.total * 4
        ));
    }
    let parameters = bytes
        .chunks_exact(4)
        .map(|c| f32::from_bits(u32::from_le_bytes(c.try_into().expect("4 bytes"))))
        .collect();
    let state = STrainStateV1::fresh_optimizer_v1(SModelV1::from_values_v1(config, parameters)?);
    let checkpoint = encode_checkpoint_v1(&state);
    Ok(json!({
        "schema": "s-import-values-v1",
        "values_sha256": sha256_hex_v1(&bytes),
        "parameter_sha256": state.model_v1().parameter_sha256_v1(),
        "state_sha256": state.state_sha256_v1(),
        "adam_step": state.adam_step_v1(),
        "checkpoint_sha256": write_v1(out, &checkpoint)?,
    }))
}

/// The binary's entry: `<command> key=value ...`, printing a JSON receipt.
pub fn main_v1(args: &[String]) -> Result<Value, String> {
    let (command, rest) = args.split_first().ok_or(USAGE_V1)?;
    let mut options = std::collections::BTreeMap::new();
    for arg in rest {
        let (key, value) = arg.split_once('=').ok_or(USAGE_V1)?;
        options.insert(key.to_owned(), value.to_owned());
    }
    let get = |key: &str| options.get(key).cloned();
    let number = |key: &str, default: u64| -> Result<u64, String> {
        get(key).map_or(Ok(default), |v| v.parse().map_err(|_| format!("bad {key}")))
    };
    let seed = number("seed", DEFAULT_SEED_V1)?;
    match command.as_str() {
        "parity-dump" => parity_dump_v1(
            Path::new(&get("out").ok_or("out=<dir> is required")?),
            seed,
            number("batches", 64)? as usize,
            number("groups", 32)? as usize,
        ),
        "logits-dump" => logits_dump_v1(
            Path::new(&get("out").ok_or("out=<file> is required")?),
            &get("model").ok_or("model= is required")?,
            &get("states").ok_or("states= is required")?,
        ),
        "bench" => bench_v1(
            seed,
            number("count", 2000)? as usize,
            number("reps", 3)? as usize,
        ),
        "resume-check" => resume_check_v1(
            Path::new(&get("out").ok_or("out=<dir> is required")?),
            seed,
            number("steps", 6)? as usize,
            number("split", 3)? as usize,
            number("groups", 16)? as usize,
        ),
        "import-values" => import_values_v1(
            Path::new(&get("values").ok_or("values=<file> is required")?),
            Path::new(&get("names").ok_or("names=<file> is required")?),
            Path::new(&get("out").ok_or("out=<file> is required")?),
        ),
        _ => Err(USAGE_V1.into()),
    }
}

const USAGE_V1: &str = "usage: s_transformer_v1 parity-dump out=<dir> [seed batches groups] | logits-dump out=<file> model=<spec> states=<spec> | bench [seed count reps] | resume-check out=<dir> [seed steps split groups] | import-values values=<file> names=<file> out=<file>";
