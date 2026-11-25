//! D4 update from D6's ordered chairs, replayed states and completed game scores.
use super::TensorIdV1 as T;
use super::*;
use crate::m4_sampler_v1::{sample_v1, SampleV1, GRID_SIZE, SAMPLER_ID_V1};
use crate::native_policy_train_step_v1::{ADAM_BETA1_V1, ADAM_BETA2_V1, ADAM_EPSILON_V1};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[path = "training/tape.rs"]
mod tape;
use tape::Tape;
#[path = "training/fixture.rs"]
pub mod fixture;

/// D0.9 step size for all three parameter sets.
const M4_LEARNING_RATE: f32 = 1e-3;

fn m4_exp(x: f64) -> f64 {
    crate::deterministic_math_v1::exp_f64_v1(x)
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Head {
    Pick,
    Count,
    Pointer,
    TypeSet,
    Split,
}

/// The learned fields of an IF2 row. D6 can deserialize its pod rows directly.
#[derive(Clone, Debug, Deserialize)]
pub struct Row {
    pub kind: Head,
    pub sampler: String,
    pub options: Vec<u16>,
    pub chosen: u16,
    pub stream_values: Vec<u64>,
    pub logits_bits: Vec<u32>,
    pub p_chosen_bits: u64,
    pub value_bits: Option<u32>,
}
#[derive(Clone, Debug, Deserialize)]
pub struct Build {
    pub count: Row,
    pub pointer: Vec<Row>,
    pub type_set: Option<Row>,
    pub split: Vec<Row>,
}

pub enum Role<'a> {
    /// Seven reference and seven matched sibling outcomes in chair perspective.
    Learning { scores: [[i8; 7]; 2] },
    Older {
        /// None for the uniform picker in the builder-only phase.
        picker_pin: Option<&'a str>,
        builder_pin: &'a str,
    },
}
/// D6 supplies the pre-action pick states in draft order, the final pool
/// in pick order and the recorded builds (sibling a before sibling b).
pub struct Chair<'a> {
    pub role: Role<'a>,
    pub picks: &'a [(PickStateV1, Row)],
    pub pool: &'a [u16],
    pub builds: &'a [Build],
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Builder,
    Joint,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct HeadUpdates {
    pub pick: u64,
    pub value: u64,
    pub count: u64,
    pub pointer: u64,
    pub type_set: u64,
    pub split: u64,
}
impl HeadUpdates {
    fn commit(&mut self, present: &Self) -> Result<()> {
        for (n, increment) in [
            (&mut self.pick, present.pick),
            (&mut self.value, present.value),
            (&mut self.count, present.count),
            (&mut self.pointer, present.pointer),
            (&mut self.type_set, present.type_set),
            (&mut self.split, present.split),
        ] {
            *n = n.checked_add(increment).ok_or("head counter overflow")?;
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Optimizer {
    /// D2 tensor layout order and row-major element order.
    pub first: Vec<Vec<f32>>,
    pub second: Vec<Vec<f32>>,
    /// Build, pick, value.
    pub steps: [u64; 3],
    pub head_updates: HeadUpdates,
}
fn zeros(model: &ModelV1) -> Vec<Vec<f32>> {
    model
        .layout()
        .iter()
        .map(|s| vec![0.; s.element_count()])
        .collect()
}
impl Optimizer {
    pub fn new(model: &ModelV1) -> Self {
        Self {
            first: zeros(model),
            second: zeros(model),
            steps: [0; 3],
            head_updates: HeadUpdates::default(),
        }
    }
}
#[derive(Clone, Debug, Default)]
pub struct Losses {
    pub build: f64,
    pub pick: f64,
    pub value: f64,
}
type Result<T> = std::result::Result<T, String>;

fn replay(
    row: &Row,
    head: Head,
    options: &[u16],
    logits: &[f32],
    value: Option<f32>,
) -> Result<SampleV1> {
    let [word] = row.stream_values.as_slice() else {
        return Err("expected one learned stream word".into());
    };
    if row.kind != head
        || row.sampler != SAMPLER_ID_V1
        || options != row.options
        || logits.len() != options.len()
        || logits
            .iter()
            .map(|x| x.to_bits())
            .ne(row.logits_bits.iter().copied())
        || value.map(f32::to_bits) != row.value_bits
        || value.is_some_and(|v| !v.is_finite())
    {
        return Err(format!("{head:?} replay fields differ"));
    }
    let sampled = sample_v1(logits, &vec![true; logits.len()], *word).map_err(|e| e.to_string())?;
    let k = sampled.selected_index;
    if options[k] != row.chosen
        || (sampled.counts[k] as f64 / GRID_SIZE as f64).to_bits() != row.p_chosen_bits
    {
        return Err(format!(
            "{head:?} replay choice or count probability differs"
        ));
    }
    Ok(sampled)
}

/// Grouped split terms enter in ascending type-bit order. One final cast per logit.
fn policy_gradient(logp: &[f64], terms: &[(usize, f64)]) -> Vec<f32> {
    logp.iter()
        .enumerate()
        .map(|(j, &lp)| {
            if lp == f64::NEG_INFINITY {
                return 0.;
            }
            let p = m4_exp(lp.clamp(-120., 0.));
            let mut d = 0.;
            for &(k, c) in terms {
                d += c * (f64::from(j == k) - p);
            }
            d as f32
        })
        .collect()
}
fn policy_term(s: &SampleV1, c: f64) -> (f64, Vec<f32>) {
    (
        c * s.selected_log_probability,
        policy_gradient(&s.log_probabilities, &[(s.selected_index, c)]),
    )
}

fn build_coefficient(advantage: f64, seats: usize) -> f64 {
    -advantage / (2 * seats) as f64
}

fn sibling_advantages(a: f64, b: f64) -> [f64; 2] {
    [a - b, b - a]
}

fn pick_credit(r: f64, v: f32, seats: usize) -> (f64, f64, f32) {
    let error = f64::from(v) - r;
    (
        error / seats as f64,
        0.5 * error * error / seats as f64,
        (error / seats as f64) as f32,
    )
}

#[path = "training/paths.rs"]
mod paths;

fn score_means(scores: &[[i8; 7]; 2]) -> Result<[f64; 2]> {
    if scores.iter().flatten().any(|&s| !(-1..=1).contains(&s)) {
        return Err("invalid completed game score".into());
    }
    Ok(scores.map(|s| f64::from(s.iter().copied().map(i32::from).sum::<i32>()) / 7.))
}

struct Batch {
    losses: Losses,
    heads: HeadUpdates,
    grads: Vec<Vec<f32>>,
    seats: usize,
}
impl Batch {
    fn new(model: &ModelV1, seats: usize) -> Self {
        Self {
            losses: Losses::default(),
            heads: HeadUpdates::default(),
            grads: zeros(model),
            seats,
        }
    }
}

/// All chairs in pod and seat order. Snapshot keys are the pins from the record.
/// Replay every learned row before the first backward. Publish all three sets together.
pub fn update(
    model: &mut ModelV1,
    optimizer: &mut Optimizer,
    rows: &ModelCardRowsV1,
    phase: Phase,
    chairs: &[Chair<'_>],
    snapshots: &BTreeMap<String, &ModelV1>,
) -> Result<Losses> {
    model.check_rows(rows).map_err(|e| e.to_string())?;
    let seats = chairs
        .iter()
        .filter(|s| matches!(s.role, Role::Learning { .. }))
        .count();
    if seats == 0 {
        return Err("empty learning batch".into());
    }
    let mut batch = Batch::new(model, seats);
    for (i, chair) in chairs.iter().enumerate() {
        let (picker, builder, build_count) = match &chair.role {
            Role::Learning { scores } => {
                score_means(scores)?;
                (&*model, &*model, 2)
            }
            Role::Older {
                picker_pin,
                builder_pin,
            } => {
                let pinned = |pin: &str| -> Result<&ModelV1> {
                    let m = *snapshots
                        .get(pin)
                        .ok_or_else(|| format!("unavailable league pin {pin}"))?;
                    m.check_rows(rows).map_err(|e| e.to_string())?;
                    Ok(m)
                };
                (
                    if phase == Phase::Joint {
                        pinned(picker_pin.ok_or("missing older picker pin")?)?
                    } else {
                        &*model
                    },
                    pinned(builder_pin)?,
                    1,
                )
            }
        };
        let expected_steps = (0..STEP_COUNT_V1)
            .filter(|step| phase == Phase::Joint && step % PACK_SIZE != PACK_SIZE - 1);
        if chair.builds.len() != build_count
            || chair
                .picks
                .iter()
                .map(|(s, _)| usize::from(s.step))
                .ne(expected_steps)
        {
            return Err(format!("chair {i} row count or pick order differs"));
        }
        paths::picks(picker, rows, chair, 0., None)
            .and_then(|_| paths::builds(builder, rows, chair, [0.; 2], None))
            .map_err(|e| format!("chair {i}: {e}"))?;
    }
    for chair in chairs {
        if let Role::Learning { scores } = &chair.role {
            let [a, b] = score_means(scores)?;
            paths::picks(model, rows, chair, (a + b) / 2., Some(&mut batch))?;
            paths::builds(
                model,
                rows,
                chair,
                sibling_advantages(a, b),
                Some(&mut batch),
            )?;
        }
    }
    adam(model, optimizer, phase, &batch)?;
    Ok(batch.losses)
}

fn adam(model: &mut ModelV1, opt: &mut Optimizer, phase: Phase, batch: &Batch) -> Result<()> {
    let mut next = opt.clone();
    let mut weights = model.values.clone();
    let mut corrections = [(0., 0.); 3];
    for (i, (step, correction)) in next.steps.iter_mut().zip(&mut corrections).enumerate() {
        if phase == Phase::Builder && i != 0 {
            continue;
        }
        *step = step.checked_add(1).ok_or("Adam counter overflow")?;
        let power = i32::try_from(*step).map_err(|_| "Adam power overflow")?;
        let correction1 = 1. - MATH.powi_f64_v1(f64::from(ADAM_BETA1_V1), power);
        let correction2 = 1. - MATH.powi_f64_v1(f64::from(ADAM_BETA2_V1), power);
        *correction = (
            (f64::from(M4_LEARNING_RATE) / correction1) as f32,
            correction2.sqrt() as f32,
        );
    }
    if next.first.len() != weights.len() || next.second.len() != weights.len() {
        return Err("Adam tensor count differs".into());
    }
    for (i, params) in weights.iter_mut().enumerate() {
        let set = match model.specs[i].set {
            ParameterSetV1::Build => 0,
            ParameterSetV1::Pick => 1,
            ParameterSetV1::Value => 2,
        };
        if next.first[i].len() != params.len() || next.second[i].len() != params.len() {
            return Err("Adam tensor shape differs".into());
        }
        if phase == Phase::Builder && set != 0 {
            continue;
        }
        let (step_size, root) = corrections[set];
        for (j, w) in params.iter_mut().enumerate() {
            let g = batch.grads[i][j];
            let m = &mut next.first[i][j];
            let v = &mut next.second[i][j];
            *m += (g - *m) * (1. - ADAM_BETA1_V1);
            *v = *v * ADAM_BETA2_V1 + g * g * (1. - ADAM_BETA2_V1);
            *w += -step_size * *m / (v.sqrt() / root + ADAM_EPSILON_V1);
            if !w.is_finite() || !m.is_finite() || !v.is_finite() || *v < 0. {
                return Err("nonfinite Adam update".into());
            }
        }
    }
    next.head_updates.commit(&batch.heads)?;
    model.values = weights;
    *opt = next;
    Ok(())
}

#[cfg(test)]
#[path = "training/tests.rs"]
mod tests;
