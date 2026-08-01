//! Deterministic low-data policy correction over frozen Net8 latents.
//!
//! The parent network and value output remain immutable. The only learned
//! parameters are a 64 by 64 matrix applied as
//! `state_hidden^T * weights * action_hidden`. The fit is one analytic policy
//! gradient at the exact zero residual, followed by movement-only scale
//! calibration. No held-out outcome is consulted until the fixed candidate is
//! scored once.

use crate::flat_policy_v2::FlatScoringDecisionViewV2;
use crate::native_xmage_cp7_outcome_reinforce_v1::{
    load_xmage_cp7_outcome_bilinear_dataset_v1, load_xmage_cp7_outcome_inference_v1,
    NativeXmageCp7OutcomeBilinearDatasetV1, NativeXmageCp7OutcomeInferenceV1,
};
use crate::rl::{parse_strict_json_value, PlayerSeatV1};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::Path;
use std::time::Instant;

pub(crate) const BILINEAR_HIDDEN_DIM_V1: usize = 64;
pub(crate) const BILINEAR_WEIGHT_COUNT_V1: usize = BILINEAR_HIDDEN_DIM_V1 * BILINEAR_HIDDEN_DIM_V1;
pub(crate) const BILINEAR_TARGET_MEAN_TV_V1: f64 = 0.02;
pub(crate) const BILINEAR_MAX_MEAN_KL_V1: f64 = 0.01;
pub(crate) const BILINEAR_MAX_P90_TV_V1: f64 = 0.06;
const STANDARD_DEVIATION_FLOOR_V1: f64 = 1.0e-6;
const CALIBRATION_TOLERANCE_V1: f64 = 1.0e-6;
const CALIBRATION_BISECTIONS_V1: usize = 48;
const CALIBRATION_MAX_DOUBLINGS_V1: usize = 32;
const RETAINED_MANIFEST_SHA256_V1: &str =
    "706b3aa80ec7a3c067d458fef06bb2237320543f202fb2349c5cb885975fdbbb";
const RETAINED_PAYLOAD_SHA256_V1: &str =
    "eb83be33bcb7418b6f85ec9687da4b7ca5620a1df64721a1942d2793588bbd3c";
const RETAINED_NATIVE_STATE_SHA256_V1: &str =
    "2c55a13abb3157f3f4ba012af663ffa56599c5d6cb90743c1ba6e024ca47a9c8";
const RETAINED_MODEL_PARAMETER_SHA256_V1: &str =
    "883e4882d01d9cb55ecd7a4ae00e3c95793b6147baf3df08650ef1fa7f8e9546";
const RETAINED_ADAM_STEP_V1: u64 = 1;
const FIXED_CORPUS_SHA256_V1: &str =
    "b75677397c8461a702bdb5d0f7dfc47fe651e2cd1d4f048cc218001055a828cd";
const RETAINED_PARENT_TRAINING_CORPUS_SHA256_V1: &str =
    "ee42241ae8a508260746840b80eca2aa7e8abc8c89ae1caafd08bad755ff96b3";

pub(crate) const RANK1_TARGET_MEAN_TV_V1: f64 = 0.01;
pub(crate) const RANK1_MAX_MEAN_KL_V1: f64 = 0.005;
pub(crate) const RANK1_MAX_P90_TV_V1: f64 = 0.04;
pub(crate) const RANK1_POWER_ITERATIONS_V1: usize = 128;
const RANK1_EFFECTIVE_DEGREES_OF_FREEDOM_V1: usize = BILINEAR_HIDDEN_DIM_V1 * 2 - 1;
const RANK1_REPORT_FILENAME_V1: &str = "report.json";
const RANK1_WEIGHTS_FILENAME_V1: &str = "weights.f32le";
const RANK1_CANDIDATE_FILENAME_V1: &str = "candidate.json";
const RANK1_PARENT_DIRECTORY_V1: &str = "parent";
const RANK1_PARENT_MANIFEST_FILENAME_V1: &str = "checkpoint.json";
const RANK1_PARENT_STATE_FILENAME_V1: &str = "checkpoint.state.f32le";
const RANK1_CANDIDATE_SCHEMA_V1: &str = "mtg-kernel-native-rank1-policy-residual-candidate/v1";
const RANK1_REPORT_SCHEMA_V1: &str = "mtg-kernel-native-rank1-policy-residual-probe/v1";
const RANK1_PUBLICATION_ENCODING_V1: &str = "serde-json-pretty-utf8-trailing-lf/v1";
const RANK1_WEIGHTS_ENCODING_V1: &str =
    "4096-row-major-state-by-action-finite-f32-little-endian/v1";
const RANK1_RESIDUAL_FORMULA_V1: &str =
    "parent_logit_plus_state_hidden_transpose_weights_action_hidden/v1";
const RANK1_COMPOSITE_MODEL_DOMAIN_V1: &[u8] =
    b"mtg-kernel-native-rank1-policy-residual-composite-model/v1";
const RANK1_RELATIVE_RIGHT_RESIDUAL_MAX_V1: f64 = 1.0e-8;
const FIXED_OUTCOME_EXPORT_CONTRACT_V1: &str = "mtg-kernel-xmage-cp7-outcome-jsonl/v2";
const FIXED_OUTCOME_SCHEMA_VERSION_V1: u32 = 2;
const FIXED_OUTCOME_DECISION_ROW_COUNT_V1: usize = 2_995;
const FIXED_OUTCOME_TERMINAL_ROW_COUNT_V1: usize = 64;
const FIXED_PHYSICAL_GROUP_COUNT_V1: usize = 2_541;
const FIXED_TERMINAL_RETURN_COUNTS_V1: [u64; 3] = [37, 0, 27];
const RANK1_OBJECTIVE_V1: &str =
    "one-zero-point-analytic-policy-gradient-standardized-episode-balanced/v1";
const RANK1_ARCHITECTURE_V1: &str =
    "frozen-net8-logit-plus-deterministic-top-singular-rank1-bilinear/v1";
const RANK1_PAIR_USE_V1: &str = "all-32-pairs-no-heldout-reuse/v1";
const RANK1_ADVANTAGE_IDENTITY_V1: &str =
    "all-data-frozen-value-standardized-equal-episode-mass/v1";
const RANK1_PROJECTION_IDENTITY_V1: &str =
    "fixed-128-iteration-two-start-top-singular-projection/v1";
const RANK1_INITIALIZATION_V1: &str =
    "two-domain-separated-normalized-signed-affine-index-vectors/v1";
const RANK1_CALIBRATION_IDENTITY_V1: &str = "mean-tv-0p01-then-movement-only-safety-cap/v1";
const RANK1_DISPOSITION_V1: &str = "publish-one-fresh-cp7-candidate";
const RANK1_INTERPRETATION_V1: &str = "The exact 32-pair corpus estimates one deterministic rank-one projection. Outcome data chooses the two rank-one factors only; policy movement chooses magnitude and may only shrink it. Passing safety gates publishes a candidate for fresh CP7 evaluation, not a strength claim.";

#[derive(Clone, Debug)]
pub(crate) struct NativeBilinearSubstepV1 {
    pub(crate) parent_logits: Vec<f32>,
    pub(crate) selected_index: usize,
    pub(crate) state_hidden: [f32; BILINEAR_HIDDEN_DIM_V1],
    /// Row-major `[parent_logits.len(), BILINEAR_HIDDEN_DIM_V1]`.
    pub(crate) action_hidden: Vec<f32>,
}

#[derive(Clone, Debug)]
pub(crate) struct NativeBilinearPhysicalGroupV1 {
    pub(crate) pair_index: u64,
    pub(crate) episode_id: u64,
    pub(crate) candidate_seat: PlayerSeatV1,
    pub(crate) terminal_return: i8,
    pub(crate) first_substep_parent_value: f32,
    pub(crate) substeps: Vec<NativeBilinearSubstepV1>,
}

#[derive(Clone, Debug)]
pub(crate) struct NativeBilinearDatasetV1 {
    pub(crate) pair_count: usize,
    pub(crate) episode_count: usize,
    pub(crate) physical_group_count: usize,
    pub(crate) substep_count: usize,
    pub(crate) groups: Vec<NativeBilinearPhysicalGroupV1>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct NativeBilinearWeightsV1 {
    values: Vec<f32>,
}

impl NativeBilinearWeightsV1 {
    pub(crate) fn zero_v1() -> Self {
        Self {
            values: vec![0.0; BILINEAR_WEIGHT_COUNT_V1],
        }
    }

    pub(crate) fn from_values_v1(values: Vec<f32>) -> Result<Self, NativeBilinearErrorV1> {
        if values.len() != BILINEAR_WEIGHT_COUNT_V1 || values.iter().any(|value| !value.is_finite())
        {
            return Err(NativeBilinearErrorV1::InvalidWeights);
        }
        Ok(Self { values })
    }

    pub(crate) fn values_v1(&self) -> &[f32] {
        &self.values
    }

    pub(crate) fn is_exact_zero_v1(&self) -> bool {
        self.values.iter().all(|value| value.to_bits() == 0)
    }

    pub(crate) fn f32le_bytes_v1(&self) -> Vec<u8> {
        self.values
            .iter()
            .flat_map(|value| value.to_bits().to_le_bytes())
            .collect()
    }

    pub(crate) fn sha256_v1(&self) -> String {
        lower_hex_v1(Sha256::digest(self.f32le_bytes_v1()).into())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum NativeBilinearErrorV1 {
    InvalidDataset,
    InvalidWeights,
    NonFinite,
    DegenerateGradient,
    CalibrationFailed,
    RankOneProjectionFailed,
}

impl Display for NativeBilinearErrorV1 {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        write!(formatter, "native bilinear residual failed: {self:?}")
    }
}

impl Error for NativeBilinearErrorV1 {}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct NativeBilinearSplitV1 {
    pub(crate) identity: &'static str,
    pub(crate) fit_pair_count: usize,
    pub(crate) heldout_pair_count: usize,
    pub(crate) fit_pair_indices: Vec<u64>,
    pub(crate) heldout_pair_indices: Vec<u64>,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct NativeBilinearAdvantageStatsV1 {
    pub(crate) identity: &'static str,
    pub(crate) fit_group_count: usize,
    pub(crate) fit_episode_count: usize,
    pub(crate) weighted_mean: f64,
    pub(crate) weighted_population_standard_deviation: f64,
    pub(crate) normalization_denominator: f64,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct NativeBilinearMovementMetricsV1 {
    pub(crate) physical_group_count: usize,
    pub(crate) substep_count: usize,
    pub(crate) mean_total_variation: f64,
    pub(crate) p90_total_variation: f64,
    pub(crate) mean_parent_to_candidate_kl: f64,
    pub(crate) policy_surrogate_improvement: f64,
    pub(crate) p0_policy_surrogate_improvement: f64,
    pub(crate) p1_policy_surrogate_improvement: f64,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct NativeBilinearCalibrationV1 {
    pub(crate) target_mean_total_variation: f64,
    pub(crate) calibrated_scale: f64,
    pub(crate) achieved_mean_total_variation: f64,
    pub(crate) direction_l2_before_normalization: f64,
    pub(crate) weights_l2: f64,
    pub(crate) weights_sha256: String,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct NativeBilinearGatesV1 {
    pub(crate) exact_parent_at_zero: bool,
    pub(crate) finite_outputs: bool,
    pub(crate) fit_calibration_reached_target: bool,
    pub(crate) heldout_surrogate_positive: bool,
    pub(crate) heldout_p0_surrogate_positive: bool,
    pub(crate) heldout_p1_surrogate_positive: bool,
    pub(crate) heldout_mean_kl_at_most_0p01: bool,
    pub(crate) heldout_p90_tv_at_most_0p06: bool,
    pub(crate) heldout_screen_pass: bool,
    pub(crate) full_refit_calibration_reached_target: Option<bool>,
    pub(crate) full_refit_mean_kl_at_most_0p01: Option<bool>,
    pub(crate) full_refit_p90_tv_at_most_0p06: Option<bool>,
    pub(crate) advance_to_cp7: bool,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct NativeBilinearTrainingReportV1 {
    pub(crate) schema: &'static str,
    pub(crate) objective: &'static str,
    pub(crate) architecture: &'static str,
    pub(crate) parameter_count: usize,
    pub(crate) parent_parameters_frozen: bool,
    pub(crate) value_head_frozen: bool,
    pub(crate) outcome_independent_scale_selection: bool,
    pub(crate) pair_count: usize,
    pub(crate) episode_count: usize,
    pub(crate) physical_group_count: usize,
    pub(crate) substep_count: usize,
    pub(crate) split: NativeBilinearSplitV1,
    pub(crate) fit_advantages: NativeBilinearAdvantageStatsV1,
    pub(crate) fit_calibration: NativeBilinearCalibrationV1,
    pub(crate) fit_metrics: NativeBilinearMovementMetricsV1,
    pub(crate) heldout_metrics: NativeBilinearMovementMetricsV1,
    pub(crate) gates: NativeBilinearGatesV1,
    pub(crate) full_refit_calibration: Option<NativeBilinearCalibrationV1>,
    pub(crate) full_refit_metrics: Option<NativeBilinearMovementMetricsV1>,
    pub(crate) disposition: &'static str,
}

#[derive(Clone, Debug, Serialize)]
struct NativeBilinearProbeSourceV1 {
    outcome_manifest_sha256: String,
    outcome_payload_sha256: String,
    native_state_sha256: String,
    model_parameter_sha256: String,
    parent_training_corpus_sha256: String,
    parent_adam_step: u64,
    outcome_jsonl_sha256: String,
    outcome_export_contract: String,
    outcome_schema_version: u32,
    outcome_decision_row_count: usize,
    outcome_terminal_row_count: usize,
    outcome_terminal_return_counts_minus_one_zero_plus_one: [u64; 3],
}

#[derive(Clone, Debug, Serialize)]
struct NativeBilinearWeightsArtifactV1 {
    filename: &'static str,
    encoding: &'static str,
    parameter_count: usize,
    byte_count: usize,
    sha256: String,
}

#[derive(Clone, Debug, Serialize)]
struct NativeBilinearProbeReportV1 {
    schema: &'static str,
    publication_encoding: &'static str,
    source: NativeBilinearProbeSourceV1,
    training: NativeBilinearTrainingReportV1,
    final_weights: Option<NativeBilinearWeightsArtifactV1>,
    interpretation: &'static str,
}

/// Offline probe result. Runtime timing is deliberately outside the hashed
/// report, and weights are present only after every fixed holdout gate passes.
pub struct NativeBilinearProbeEnvelopeV1 {
    deterministic_report_bytes: Vec<u8>,
    deterministic_report_sha256: String,
    elapsed_milliseconds: u64,
    disposition: &'static str,
    advance_to_cp7: bool,
    final_weights_f32le: Option<Vec<u8>>,
}

impl NativeBilinearProbeEnvelopeV1 {
    pub fn deterministic_report_bytes_v1(&self) -> &[u8] {
        &self.deterministic_report_bytes
    }

    pub fn deterministic_report_sha256_v1(&self) -> &str {
        &self.deterministic_report_sha256
    }

    pub const fn elapsed_milliseconds_v1(&self) -> u64 {
        self.elapsed_milliseconds
    }

    pub const fn disposition_v1(&self) -> &'static str {
        self.disposition
    }

    pub const fn advance_to_cp7_v1(&self) -> bool {
        self.advance_to_cp7
    }

    pub fn final_weights_f32le_v1(&self) -> Option<&[u8]> {
        self.final_weights_f32le.as_deref()
    }
}

#[derive(Debug)]
pub(crate) struct NativeBilinearTrainingResultV1 {
    pub(crate) report: NativeBilinearTrainingReportV1,
    pub(crate) final_weights: Option<NativeBilinearWeightsV1>,
}

#[derive(Clone, Copy)]
struct AdvantageCoefficientV1 {
    group_index: usize,
    coefficient: f64,
}

fn lower_hex_v1(raw: [u8; 32]) -> String {
    raw.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn is_heldout_pair_v1(pair_index: u64) -> bool {
    pair_index % 4 == 0
}

fn validate_dataset_v1(dataset: &NativeBilinearDatasetV1) -> Result<(), NativeBilinearErrorV1> {
    if dataset.pair_count != 32
        || dataset.episode_count != 64
        || dataset.groups.is_empty()
        || dataset.physical_group_count != dataset.groups.len()
    {
        return Err(NativeBilinearErrorV1::InvalidDataset);
    }
    let mut pairs = BTreeMap::<u64, BTreeMap<u64, PlayerSeatV1>>::new();
    let mut episode_facts = BTreeMap::<u64, (u64, PlayerSeatV1, i8)>::new();
    let mut observed_substeps = 0usize;
    for group in &dataset.groups {
        if !matches!(group.terminal_return, -1..=1)
            || !group.first_substep_parent_value.is_finite()
            || group.substeps.is_empty()
        {
            return Err(NativeBilinearErrorV1::InvalidDataset);
        }
        let fact = (
            group.pair_index,
            group.candidate_seat,
            group.terminal_return,
        );
        if episode_facts
            .insert(group.episode_id, fact)
            .is_some_and(|previous| previous != fact)
        {
            return Err(NativeBilinearErrorV1::InvalidDataset);
        }
        let episodes = pairs.entry(group.pair_index).or_default();
        if episodes
            .insert(group.episode_id, group.candidate_seat)
            .is_some_and(|previous| previous != group.candidate_seat)
        {
            return Err(NativeBilinearErrorV1::InvalidDataset);
        }
        for substep in &group.substeps {
            let action_count = substep.parent_logits.len();
            if action_count == 0
                || substep.selected_index >= action_count
                || substep.action_hidden.len()
                    != action_count
                        .checked_mul(BILINEAR_HIDDEN_DIM_V1)
                        .ok_or(NativeBilinearErrorV1::InvalidDataset)?
                || substep
                    .parent_logits
                    .iter()
                    .chain(&substep.state_hidden)
                    .chain(&substep.action_hidden)
                    .any(|value| !value.is_finite())
            {
                return Err(NativeBilinearErrorV1::InvalidDataset);
            }
            observed_substeps = observed_substeps
                .checked_add(1)
                .ok_or(NativeBilinearErrorV1::InvalidDataset)?;
        }
    }
    if pairs.len() != dataset.pair_count
        || episode_facts.len() != dataset.episode_count
        || observed_substeps != dataset.substep_count
        || pairs.keys().copied().ne(0_u64..32_u64)
    {
        return Err(NativeBilinearErrorV1::InvalidDataset);
    }
    for episodes in pairs.values() {
        if episodes.len() != 2
            || episodes.values().copied().collect::<BTreeSet<_>>()
                != BTreeSet::from([PlayerSeatV1::P0, PlayerSeatV1::P1])
        {
            return Err(NativeBilinearErrorV1::InvalidDataset);
        }
    }
    Ok(())
}

fn selected_group_indices_v1(
    dataset: &NativeBilinearDatasetV1,
    select: impl Fn(u64) -> bool,
) -> Vec<usize> {
    dataset
        .groups
        .iter()
        .enumerate()
        .filter_map(|(index, group)| select(group.pair_index).then_some(index))
        .collect()
}

fn fit_advantage_stats_v1(
    dataset: &NativeBilinearDatasetV1,
    group_indices: &[usize],
) -> Result<NativeBilinearAdvantageStatsV1, NativeBilinearErrorV1> {
    if group_indices.is_empty() {
        return Err(NativeBilinearErrorV1::InvalidDataset);
    }
    let mut episode_counts = BTreeMap::<u64, usize>::new();
    for &index in group_indices {
        *episode_counts
            .entry(dataset.groups[index].episode_id)
            .or_default() += 1;
    }
    let group_count = group_indices.len() as f64;
    let episode_count = episode_counts.len() as f64;
    let weighted = |index: usize| {
        let group = &dataset.groups[index];
        let count = episode_counts[&group.episode_id] as f64;
        let weight = group_count / (episode_count * count);
        let raw = f64::from(group.terminal_return) - f64::from(group.first_substep_parent_value);
        (weight, raw)
    };
    let mean = group_indices
        .iter()
        .map(|&index| {
            let (weight, raw) = weighted(index);
            weight * raw
        })
        .sum::<f64>()
        / group_count;
    let variance = group_indices
        .iter()
        .map(|&index| {
            let (weight, raw) = weighted(index);
            weight * (raw - mean) * (raw - mean)
        })
        .sum::<f64>()
        / group_count;
    let standard_deviation = variance.sqrt();
    let denominator = standard_deviation.max(STANDARD_DEVIATION_FLOOR_V1);
    if !mean.is_finite() || !standard_deviation.is_finite() || !denominator.is_finite() {
        return Err(NativeBilinearErrorV1::NonFinite);
    }
    Ok(NativeBilinearAdvantageStatsV1 {
        identity: "fit-only-frozen-value-standardized-equal-episode-mass/v1",
        fit_group_count: group_indices.len(),
        fit_episode_count: episode_counts.len(),
        weighted_mean: mean,
        weighted_population_standard_deviation: standard_deviation,
        normalization_denominator: denominator,
    })
}

fn coefficients_v1(
    dataset: &NativeBilinearDatasetV1,
    group_indices: &[usize],
    stats: &NativeBilinearAdvantageStatsV1,
) -> Result<Vec<AdvantageCoefficientV1>, NativeBilinearErrorV1> {
    let mut episode_counts = BTreeMap::<u64, usize>::new();
    for &index in group_indices {
        *episode_counts
            .entry(dataset.groups[index].episode_id)
            .or_default() += 1;
    }
    let group_count = group_indices.len() as f64;
    let episode_count = episode_counts.len() as f64;
    group_indices
        .iter()
        .map(|&index| {
            let group = &dataset.groups[index];
            let episode_group_count = episode_counts[&group.episode_id] as f64;
            let effective_weight = group_count / (episode_count * episode_group_count);
            let raw =
                f64::from(group.terminal_return) - f64::from(group.first_substep_parent_value);
            let coefficient =
                effective_weight * (raw - stats.weighted_mean) / stats.normalization_denominator;
            if !coefficient.is_finite() {
                return Err(NativeBilinearErrorV1::NonFinite);
            }
            Ok(AdvantageCoefficientV1 {
                group_index: index,
                coefficient,
            })
        })
        .collect()
}

fn probabilities_v1(logits: &[f32]) -> Result<Vec<f64>, NativeBilinearErrorV1> {
    if logits.is_empty() || logits.iter().any(|value| !value.is_finite()) {
        return Err(NativeBilinearErrorV1::NonFinite);
    }
    let maximum = logits.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let exponentials = logits
        .iter()
        .map(|value| (f64::from(*value) - f64::from(maximum)).exp())
        .collect::<Vec<_>>();
    let sum = exponentials.iter().sum::<f64>();
    if !sum.is_finite() || sum <= 0.0 {
        return Err(NativeBilinearErrorV1::NonFinite);
    }
    Ok(exponentials.into_iter().map(|value| value / sum).collect())
}

fn log_probability_v1(
    probabilities: &[f64],
    selected: usize,
) -> Result<f64, NativeBilinearErrorV1> {
    let probability = probabilities
        .get(selected)
        .copied()
        .ok_or(NativeBilinearErrorV1::InvalidDataset)?;
    let value = probability.ln();
    if value.is_finite() {
        Ok(value)
    } else {
        Err(NativeBilinearErrorV1::NonFinite)
    }
}

fn analytic_gradient_matrix_v1(
    dataset: &NativeBilinearDatasetV1,
    coefficients: &[AdvantageCoefficientV1],
) -> Result<Vec<f64>, NativeBilinearErrorV1> {
    if coefficients.is_empty() {
        return Err(NativeBilinearErrorV1::InvalidDataset);
    }
    let mut gradient = vec![0.0_f64; BILINEAR_WEIGHT_COUNT_V1];
    let normalization = coefficients.len() as f64;
    for term in coefficients {
        let group = &dataset.groups[term.group_index];
        for substep in &group.substeps {
            let probabilities = probabilities_v1(&substep.parent_logits)?;
            let mut expected_action = [0.0_f64; BILINEAR_HIDDEN_DIM_V1];
            for (action_index, probability) in probabilities.iter().copied().enumerate() {
                let begin = action_index * BILINEAR_HIDDEN_DIM_V1;
                for coordinate in 0..BILINEAR_HIDDEN_DIM_V1 {
                    expected_action[coordinate] +=
                        probability * f64::from(substep.action_hidden[begin + coordinate]);
                }
            }
            let selected_begin = substep.selected_index * BILINEAR_HIDDEN_DIM_V1;
            for state_coordinate in 0..BILINEAR_HIDDEN_DIM_V1 {
                let state = f64::from(substep.state_hidden[state_coordinate]);
                for action_coordinate in 0..BILINEAR_HIDDEN_DIM_V1 {
                    let centered_action =
                        f64::from(substep.action_hidden[selected_begin + action_coordinate])
                            - expected_action[action_coordinate];
                    gradient[state_coordinate * BILINEAR_HIDDEN_DIM_V1 + action_coordinate] +=
                        term.coefficient * state * centered_action / normalization;
                }
            }
        }
    }
    if gradient.iter().any(|value| !value.is_finite()) {
        return Err(NativeBilinearErrorV1::NonFinite);
    }
    Ok(gradient)
}

fn gradient_direction_v1(
    dataset: &NativeBilinearDatasetV1,
    coefficients: &[AdvantageCoefficientV1],
) -> Result<(Vec<f32>, f64), NativeBilinearErrorV1> {
    let gradient = analytic_gradient_matrix_v1(dataset, coefficients)?;
    let norm = gradient
        .iter()
        .map(|value| value * value)
        .sum::<f64>()
        .sqrt();
    if !norm.is_finite() || norm <= 0.0 {
        return Err(NativeBilinearErrorV1::DegenerateGradient);
    }
    let direction = gradient
        .into_iter()
        .map(|value| (value / norm) as f32)
        .collect::<Vec<_>>();
    if direction.iter().any(|value| !value.is_finite()) {
        return Err(NativeBilinearErrorV1::NonFinite);
    }
    Ok((direction, norm))
}

pub(crate) fn apply_bilinear_residual_v1(
    parent_logits: &[f32],
    state_hidden: &[f32; BILINEAR_HIDDEN_DIM_V1],
    action_hidden: &[f32],
    weights: &NativeBilinearWeightsV1,
) -> Result<Vec<f32>, NativeBilinearErrorV1> {
    let action_count = parent_logits.len();
    if action_count == 0
        || action_hidden.len() != action_count * BILINEAR_HIDDEN_DIM_V1
        || parent_logits
            .iter()
            .chain(state_hidden)
            .chain(action_hidden)
            .any(|value| !value.is_finite())
    {
        return Err(NativeBilinearErrorV1::InvalidDataset);
    }
    if weights.is_exact_zero_v1() {
        return Ok(parent_logits.to_vec());
    }
    let mut output = Vec::with_capacity(action_count);
    for action_index in 0..action_count {
        let action_begin = action_index * BILINEAR_HIDDEN_DIM_V1;
        let mut residual = 0.0_f32;
        for state_coordinate in 0..BILINEAR_HIDDEN_DIM_V1 {
            let mut projected_action = 0.0_f32;
            let weight_begin = state_coordinate * BILINEAR_HIDDEN_DIM_V1;
            for action_coordinate in 0..BILINEAR_HIDDEN_DIM_V1 {
                projected_action += weights.values[weight_begin + action_coordinate]
                    * action_hidden[action_begin + action_coordinate];
            }
            residual += state_hidden[state_coordinate] * projected_action;
        }
        let value = parent_logits[action_index] + residual;
        if !value.is_finite() {
            return Err(NativeBilinearErrorV1::NonFinite);
        }
        output.push(value);
    }
    Ok(output)
}

fn scaled_weights_v1(
    direction: &[f32],
    scale: f64,
) -> Result<NativeBilinearWeightsV1, NativeBilinearErrorV1> {
    if direction.len() != BILINEAR_WEIGHT_COUNT_V1 || !scale.is_finite() || scale < 0.0 {
        return Err(NativeBilinearErrorV1::InvalidWeights);
    }
    NativeBilinearWeightsV1::from_values_v1(
        direction
            .iter()
            .map(|value| (f64::from(*value) * scale) as f32)
            .collect(),
    )
}

fn total_variation_v1(left: &[f64], right: &[f64]) -> f64 {
    0.5 * left
        .iter()
        .zip(right)
        .map(|(left, right)| (left - right).abs())
        .sum::<f64>()
}

fn mean_tv_v1(
    dataset: &NativeBilinearDatasetV1,
    group_indices: &[usize],
    weights: &NativeBilinearWeightsV1,
) -> Result<f64, NativeBilinearErrorV1> {
    let mut total = 0.0;
    let mut count = 0usize;
    for &group_index in group_indices {
        for substep in &dataset.groups[group_index].substeps {
            let candidate = apply_bilinear_residual_v1(
                &substep.parent_logits,
                &substep.state_hidden,
                &substep.action_hidden,
                weights,
            )?;
            total += total_variation_v1(
                &probabilities_v1(&substep.parent_logits)?,
                &probabilities_v1(&candidate)?,
            );
            count += 1;
        }
    }
    let mean = total / count as f64;
    if mean.is_finite() {
        Ok(mean)
    } else {
        Err(NativeBilinearErrorV1::NonFinite)
    }
}

fn calibrate_v1(
    dataset: &NativeBilinearDatasetV1,
    group_indices: &[usize],
    direction: &[f32],
    direction_norm: f64,
) -> Result<(NativeBilinearWeightsV1, NativeBilinearCalibrationV1), NativeBilinearErrorV1> {
    calibrate_to_target_v1(
        dataset,
        group_indices,
        direction,
        direction_norm,
        BILINEAR_TARGET_MEAN_TV_V1,
    )
}

fn calibrate_to_target_v1(
    dataset: &NativeBilinearDatasetV1,
    group_indices: &[usize],
    direction: &[f32],
    direction_norm: f64,
    target_mean_tv: f64,
) -> Result<(NativeBilinearWeightsV1, NativeBilinearCalibrationV1), NativeBilinearErrorV1> {
    if !target_mean_tv.is_finite() || !(0.0..1.0).contains(&target_mean_tv) {
        return Err(NativeBilinearErrorV1::CalibrationFailed);
    }
    let mut low = 0.0_f64;
    let mut high = 1.0_f64;
    let mut high_tv = mean_tv_v1(dataset, group_indices, &scaled_weights_v1(direction, high)?)?;
    for _ in 0..CALIBRATION_MAX_DOUBLINGS_V1 {
        if high_tv >= target_mean_tv {
            break;
        }
        low = high;
        high *= 2.0;
        high_tv = mean_tv_v1(dataset, group_indices, &scaled_weights_v1(direction, high)?)?;
    }
    if high_tv < target_mean_tv {
        return Err(NativeBilinearErrorV1::CalibrationFailed);
    }
    for _ in 0..CALIBRATION_BISECTIONS_V1 {
        let midpoint = (low + high) * 0.5;
        let midpoint_tv = mean_tv_v1(
            dataset,
            group_indices,
            &scaled_weights_v1(direction, midpoint)?,
        )?;
        if midpoint_tv < target_mean_tv {
            low = midpoint;
        } else {
            high = midpoint;
        }
    }
    let weights = scaled_weights_v1(direction, high)?;
    let achieved = mean_tv_v1(dataset, group_indices, &weights)?;
    let weights_l2 = weights
        .values
        .iter()
        .map(|value| f64::from(*value).powi(2))
        .sum::<f64>()
        .sqrt();
    let calibration = NativeBilinearCalibrationV1 {
        target_mean_total_variation: target_mean_tv,
        calibrated_scale: high,
        achieved_mean_total_variation: achieved,
        direction_l2_before_normalization: direction_norm,
        weights_l2,
        weights_sha256: weights.sha256_v1(),
    };
    Ok((weights, calibration))
}

fn movement_metrics_v1(
    dataset: &NativeBilinearDatasetV1,
    coefficients: &[AdvantageCoefficientV1],
    weights: &NativeBilinearWeightsV1,
) -> Result<NativeBilinearMovementMetricsV1, NativeBilinearErrorV1> {
    let mut televisions = Vec::new();
    let mut kl_sum = 0.0_f64;
    let mut overall_surrogate = 0.0_f64;
    let mut seat_surrogate = [0.0_f64; 2];
    let mut seat_group_count = [0usize; 2];
    for term in coefficients {
        let group = &dataset.groups[term.group_index];
        let seat = match group.candidate_seat {
            PlayerSeatV1::P0 => 0,
            PlayerSeatV1::P1 => 1,
        };
        let mut group_log_probability_delta = 0.0_f64;
        for substep in &group.substeps {
            let candidate_logits = apply_bilinear_residual_v1(
                &substep.parent_logits,
                &substep.state_hidden,
                &substep.action_hidden,
                weights,
            )?;
            let parent = probabilities_v1(&substep.parent_logits)?;
            let candidate = probabilities_v1(&candidate_logits)?;
            televisions.push(total_variation_v1(&parent, &candidate));
            kl_sum += parent
                .iter()
                .zip(&candidate)
                .map(|(parent, candidate)| {
                    if *parent == 0.0 {
                        0.0
                    } else {
                        parent * (parent.ln() - candidate.ln())
                    }
                })
                .sum::<f64>();
            group_log_probability_delta += log_probability_v1(&candidate, substep.selected_index)?
                - log_probability_v1(&parent, substep.selected_index)?;
        }
        let contribution = term.coefficient * group_log_probability_delta;
        overall_surrogate += contribution;
        seat_surrogate[seat] += contribution;
        seat_group_count[seat] += 1;
    }
    if televisions.is_empty() || seat_group_count.contains(&0) {
        return Err(NativeBilinearErrorV1::InvalidDataset);
    }
    televisions.sort_by(f64::total_cmp);
    let p90_index = (9 * televisions.len()).div_ceil(10).saturating_sub(1);
    let substep_count = televisions.len();
    let metrics = NativeBilinearMovementMetricsV1 {
        physical_group_count: coefficients.len(),
        substep_count,
        mean_total_variation: televisions.iter().sum::<f64>() / substep_count as f64,
        p90_total_variation: televisions[p90_index],
        mean_parent_to_candidate_kl: kl_sum / substep_count as f64,
        policy_surrogate_improvement: overall_surrogate / coefficients.len() as f64,
        p0_policy_surrogate_improvement: seat_surrogate[0] / seat_group_count[0] as f64,
        p1_policy_surrogate_improvement: seat_surrogate[1] / seat_group_count[1] as f64,
    };
    if [
        metrics.mean_total_variation,
        metrics.p90_total_variation,
        metrics.mean_parent_to_candidate_kl,
        metrics.policy_surrogate_improvement,
        metrics.p0_policy_surrogate_improvement,
        metrics.p1_policy_surrogate_improvement,
    ]
    .iter()
    .any(|value| !value.is_finite())
    {
        return Err(NativeBilinearErrorV1::NonFinite);
    }
    Ok(metrics)
}

fn exact_parent_at_zero_v1(dataset: &NativeBilinearDatasetV1) -> bool {
    let zero = NativeBilinearWeightsV1::zero_v1();
    dataset.groups.iter().all(|group| {
        group.substeps.iter().all(|substep| {
            apply_bilinear_residual_v1(
                &substep.parent_logits,
                &substep.state_hidden,
                &substep.action_hidden,
                &zero,
            )
            .is_ok_and(|candidate| {
                candidate
                    .iter()
                    .zip(&substep.parent_logits)
                    .all(|(left, right)| left.to_bits() == right.to_bits())
            })
        })
    })
}

pub(crate) fn train_native_bilinear_policy_residual_v1(
    dataset: &NativeBilinearDatasetV1,
) -> Result<NativeBilinearTrainingResultV1, NativeBilinearErrorV1> {
    validate_dataset_v1(dataset)?;
    let fit_indices = selected_group_indices_v1(dataset, |pair| !is_heldout_pair_v1(pair));
    let heldout_indices = selected_group_indices_v1(dataset, is_heldout_pair_v1);
    let fit_pair_indices = (0_u64..32)
        .filter(|pair| !is_heldout_pair_v1(*pair))
        .collect();
    let heldout_pair_indices = (0_u64..32)
        .filter(|pair| is_heldout_pair_v1(*pair))
        .collect();
    let split = NativeBilinearSplitV1 {
        identity: "pair-index-modulo-4-heldout-remainder-0/v1",
        fit_pair_count: 24,
        heldout_pair_count: 8,
        fit_pair_indices,
        heldout_pair_indices,
    };
    let fit_advantages = fit_advantage_stats_v1(dataset, &fit_indices)?;
    let fit_coefficients = coefficients_v1(dataset, &fit_indices, &fit_advantages)?;
    let (fit_direction, fit_direction_norm) = gradient_direction_v1(dataset, &fit_coefficients)?;
    let (fit_weights, fit_calibration) =
        calibrate_v1(dataset, &fit_indices, &fit_direction, fit_direction_norm)?;
    let fit_metrics = movement_metrics_v1(dataset, &fit_coefficients, &fit_weights)?;
    let heldout_coefficients = coefficients_v1(dataset, &heldout_indices, &fit_advantages)?;
    let heldout_metrics = movement_metrics_v1(dataset, &heldout_coefficients, &fit_weights)?;
    let exact_parent_at_zero = exact_parent_at_zero_v1(dataset);
    let finite_outputs = true;
    let fit_calibration_reached_target =
        (fit_calibration.achieved_mean_total_variation - BILINEAR_TARGET_MEAN_TV_V1).abs()
            <= CALIBRATION_TOLERANCE_V1;
    let heldout_surrogate_positive = heldout_metrics.policy_surrogate_improvement > 0.0;
    let heldout_p0_surrogate_positive = heldout_metrics.p0_policy_surrogate_improvement > 0.0;
    let heldout_p1_surrogate_positive = heldout_metrics.p1_policy_surrogate_improvement > 0.0;
    let heldout_mean_kl_at_most_0p01 =
        heldout_metrics.mean_parent_to_candidate_kl <= BILINEAR_MAX_MEAN_KL_V1;
    let heldout_p90_tv_at_most_0p06 = heldout_metrics.p90_total_variation <= BILINEAR_MAX_P90_TV_V1;
    let heldout_screen_pass = exact_parent_at_zero
        && finite_outputs
        && fit_calibration_reached_target
        && heldout_surrogate_positive
        && heldout_p0_surrogate_positive
        && heldout_p1_surrogate_positive
        && heldout_mean_kl_at_most_0p01
        && heldout_p90_tv_at_most_0p06;

    let (refit_weights, full_refit_calibration, full_refit_metrics) = if heldout_screen_pass {
        let all_indices = (0..dataset.groups.len()).collect::<Vec<_>>();
        let full_stats = fit_advantage_stats_v1(dataset, &all_indices)?;
        let full_coefficients = coefficients_v1(dataset, &all_indices, &full_stats)?;
        let (direction, norm) = gradient_direction_v1(dataset, &full_coefficients)?;
        let (weights, calibration) = calibrate_v1(dataset, &all_indices, &direction, norm)?;
        let metrics = movement_metrics_v1(dataset, &full_coefficients, &weights)?;
        (Some(weights), Some(calibration), Some(metrics))
    } else {
        (None, None, None)
    };
    let full_refit_calibration_reached_target =
        full_refit_calibration.as_ref().map(|calibration| {
            (calibration.achieved_mean_total_variation - BILINEAR_TARGET_MEAN_TV_V1).abs()
                <= CALIBRATION_TOLERANCE_V1
        });
    let full_refit_mean_kl_at_most_0p01 = full_refit_metrics
        .as_ref()
        .map(|metrics| metrics.mean_parent_to_candidate_kl <= BILINEAR_MAX_MEAN_KL_V1);
    let full_refit_p90_tv_at_most_0p06 = full_refit_metrics
        .as_ref()
        .map(|metrics| metrics.p90_total_variation <= BILINEAR_MAX_P90_TV_V1);
    let advance_to_cp7 = heldout_screen_pass
        && full_refit_calibration_reached_target == Some(true)
        && full_refit_mean_kl_at_most_0p01 == Some(true)
        && full_refit_p90_tv_at_most_0p06 == Some(true);
    let final_weights = if advance_to_cp7 {
        Some(refit_weights.expect("a heldout screen pass always computes full-refit weights"))
    } else {
        None
    };
    let gates = NativeBilinearGatesV1 {
        exact_parent_at_zero,
        finite_outputs,
        fit_calibration_reached_target,
        heldout_surrogate_positive,
        heldout_p0_surrogate_positive,
        heldout_p1_surrogate_positive,
        heldout_mean_kl_at_most_0p01,
        heldout_p90_tv_at_most_0p06,
        heldout_screen_pass,
        full_refit_calibration_reached_target,
        full_refit_mean_kl_at_most_0p01,
        full_refit_p90_tv_at_most_0p06,
        advance_to_cp7,
    };
    let report = NativeBilinearTrainingReportV1 {
        schema: "mtg-kernel-native-bilinear-policy-residual-training/v1",
        objective: "one-zero-point-analytic-policy-gradient-standardized-episode-balanced/v1",
        architecture: "frozen-net8-logit-plus-state-hidden-transpose-w-action-hidden/v1",
        parameter_count: BILINEAR_WEIGHT_COUNT_V1,
        parent_parameters_frozen: true,
        value_head_frozen: true,
        outcome_independent_scale_selection: true,
        pair_count: dataset.pair_count,
        episode_count: dataset.episode_count,
        physical_group_count: dataset.physical_group_count,
        substep_count: dataset.substep_count,
        split,
        fit_advantages,
        fit_calibration,
        fit_metrics,
        heldout_metrics,
        gates,
        full_refit_calibration,
        full_refit_metrics,
        disposition: if advance_to_cp7 {
            "advance-one-fresh-16-pair-cp7-gate"
        } else {
            "reject-before-cp7"
        },
    };
    Ok(NativeBilinearTrainingResultV1 {
        report,
        final_weights,
    })
}

fn require_fixed_source_v1(
    dataset: &NativeXmageCp7OutcomeBilinearDatasetV1,
) -> Result<(), io::Error> {
    let exact_pairs = dataset.pair_indices.iter().copied().eq(0_u64..32_u64);
    if lower_hex_v1(dataset.parent_manifest_sha256) != RETAINED_MANIFEST_SHA256_V1
        || lower_hex_v1(dataset.parent_payload_sha256) != RETAINED_PAYLOAD_SHA256_V1
        || lower_hex_v1(dataset.parent_native_state_sha256) != RETAINED_NATIVE_STATE_SHA256_V1
        || lower_hex_v1(dataset.parent_model_parameter_sha256) != RETAINED_MODEL_PARAMETER_SHA256_V1
        || dataset.parent_adam_step != RETAINED_ADAM_STEP_V1
        || lower_hex_v1(dataset.jsonl_sha256) != FIXED_CORPUS_SHA256_V1
        || dataset.pair_count != 32
        || dataset.episode_count != 64
        || !exact_pairs
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "bilinear probe source is not the exact retained parent and fixed 32-pair corpus",
        ));
    }
    Ok(())
}

fn convert_source_dataset_v1(
    source: NativeXmageCp7OutcomeBilinearDatasetV1,
) -> NativeBilinearDatasetV1 {
    NativeBilinearDatasetV1 {
        pair_count: source.pair_count,
        episode_count: source.episode_count,
        physical_group_count: source.physical_group_count,
        substep_count: source.decision_row_count,
        groups: source
            .groups
            .into_iter()
            .map(|group| NativeBilinearPhysicalGroupV1 {
                pair_index: group.pair_index,
                episode_id: group.episode_id,
                candidate_seat: group.candidate_seat,
                terminal_return: group.terminal_return,
                first_substep_parent_value: group.first_substep_old_value,
                substeps: group
                    .substeps
                    .into_iter()
                    .map(|substep| NativeBilinearSubstepV1 {
                        parent_logits: substep.parent_logits,
                        selected_index: substep.selected_index,
                        state_hidden: substep.state_hidden,
                        action_hidden: substep.action_hidden,
                    })
                    .collect(),
            })
            .collect(),
    }
}

/// Runs the one-shot residual screen against the exact retained parent and
/// exact predeclared 32-pair corpus. No candidate is emitted on rejection.
pub fn run_native_bilinear_policy_residual_probe_v1(
    source_outcome_root: impl AsRef<Path>,
    outcome_jsonl: impl AsRef<Path>,
) -> Result<NativeBilinearProbeEnvelopeV1, Box<dyn Error>> {
    let started = Instant::now();
    let parent = load_xmage_cp7_outcome_inference_v1(source_outcome_root.as_ref())?;
    let source_dataset =
        load_xmage_cp7_outcome_bilinear_dataset_v1(outcome_jsonl.as_ref(), &parent)?;
    require_fixed_source_v1(&source_dataset)?;
    let source = NativeBilinearProbeSourceV1 {
        outcome_manifest_sha256: lower_hex_v1(source_dataset.parent_manifest_sha256),
        outcome_payload_sha256: lower_hex_v1(source_dataset.parent_payload_sha256),
        native_state_sha256: lower_hex_v1(source_dataset.parent_native_state_sha256),
        model_parameter_sha256: lower_hex_v1(source_dataset.parent_model_parameter_sha256),
        parent_training_corpus_sha256: lower_hex_v1(source_dataset.parent_corpus_sha256),
        parent_adam_step: source_dataset.parent_adam_step,
        outcome_jsonl_sha256: lower_hex_v1(source_dataset.jsonl_sha256),
        outcome_export_contract: source_dataset.export_contract.clone(),
        outcome_schema_version: source_dataset.schema_version,
        outcome_decision_row_count: source_dataset.decision_row_count,
        outcome_terminal_row_count: source_dataset.terminal_row_count,
        outcome_terminal_return_counts_minus_one_zero_plus_one: source_dataset
            .terminal_return_counts,
    };
    let dataset = convert_source_dataset_v1(source_dataset);
    let result = train_native_bilinear_policy_residual_v1(&dataset)?;
    let advance_to_cp7 = result.report.gates.advance_to_cp7;
    let disposition = result.report.disposition;
    let final_weights_f32le = result
        .final_weights
        .as_ref()
        .map(NativeBilinearWeightsV1::f32le_bytes_v1);
    let final_weights = result.final_weights.as_ref().map(|weights| {
        let bytes = weights.f32le_bytes_v1();
        NativeBilinearWeightsArtifactV1 {
            filename: "weights.f32le",
            encoding: "4096-row-major-finite-f32-little-endian/v1",
            parameter_count: BILINEAR_WEIGHT_COUNT_V1,
            byte_count: bytes.len(),
            sha256: weights.sha256_v1(),
        }
    });
    let report = NativeBilinearProbeReportV1 {
        schema: "mtg-kernel-native-bilinear-policy-residual-probe/v1",
        publication_encoding: "serde-json-pretty-utf8-trailing-lf/v1",
        source,
        training: result.report,
        final_weights,
        interpretation: "One frozen-parent analytic bilinear policy-gradient direction fit on the predeclared 24-pair partition, scaled only to fixed fit-policy movement, then evaluated once on the predeclared 8-pair holdout. A rejection emits no candidate weights and authorizes no CP7 games.",
    };
    let mut deterministic_report_bytes = serde_json::to_vec_pretty(&report)?;
    deterministic_report_bytes.push(b'\n');
    let deterministic_report_sha256 =
        lower_hex_v1(Sha256::digest(&deterministic_report_bytes).into());
    let elapsed_milliseconds = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    Ok(NativeBilinearProbeEnvelopeV1 {
        deterministic_report_bytes,
        deterministic_report_sha256,
        elapsed_milliseconds,
        disposition,
        advance_to_cp7,
        final_weights_f32le,
    })
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct NativeRank1SourceV1 {
    parent_manifest_sha256: String,
    parent_payload_sha256: String,
    parent_native_state_sha256: String,
    parent_model_parameter_sha256: String,
    parent_training_corpus_sha256: String,
    parent_adam_step: u64,
    training_corpus_sha256: String,
    outcome_export_contract: String,
    outcome_schema_version: u32,
    outcome_decision_row_count: usize,
    outcome_terminal_row_count: usize,
    outcome_terminal_return_counts_minus_one_zero_plus_one: [u64; 3],
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct NativeRank1AdvantageStatsV1 {
    identity: String,
    physical_group_count: usize,
    episode_count: usize,
    weighted_mean: f64,
    weighted_population_standard_deviation: f64,
    normalization_denominator: f64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct NativeRank1ProjectionV1 {
    identity: String,
    rank: usize,
    effective_degrees_of_freedom: usize,
    power_iteration_count: usize,
    initialization: String,
    gradient_frobenius_norm: f64,
    top_singular_value: f64,
    captured_gradient_energy_fraction: f64,
    relative_right_residual: f64,
    expanded_direction_l2: f64,
    left_singular_vector_f32_bits: Vec<u32>,
    right_singular_vector_f32_bits: Vec<u32>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct NativeRank1CalibrationV1 {
    identity: String,
    target_mean_total_variation: f64,
    target_calibrated_scale: f64,
    final_scale: f64,
    safety_cap_applied: bool,
    achieved_mean_total_variation: f64,
    weights_l2: f64,
    weights_sha256: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct NativeRank1MovementMetricsV1 {
    physical_group_count: usize,
    substep_count: usize,
    mean_total_variation: f64,
    p90_total_variation: f64,
    mean_parent_to_candidate_kl: f64,
    training_policy_surrogate_improvement: f64,
    p0_training_policy_surrogate_improvement: f64,
    p1_training_policy_surrogate_improvement: f64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct NativeRank1SafetyGatesV1 {
    exact_parent_at_zero: bool,
    finite_outputs: bool,
    rank_one_projection_valid: bool,
    mean_total_variation_at_most_0p01: bool,
    mean_parent_to_candidate_kl_at_most_0p005: bool,
    p90_total_variation_at_most_0p04: bool,
    publish_candidate: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct NativeRank1TrainingV1 {
    objective: String,
    architecture: String,
    pair_use: String,
    parent_parameters_frozen: bool,
    value_head_frozen: bool,
    outcome_independent_scale_selection: bool,
    pair_count: usize,
    episode_count: usize,
    physical_group_count: usize,
    substep_count: usize,
    expanded_parameter_count: usize,
    advantage: NativeRank1AdvantageStatsV1,
    projection: NativeRank1ProjectionV1,
    calibration: NativeRank1CalibrationV1,
    movement: NativeRank1MovementMetricsV1,
    safety_gates: NativeRank1SafetyGatesV1,
    disposition: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct NativeRank1WeightsReportV1 {
    filename: String,
    encoding: String,
    count: usize,
    byte_count: usize,
    sha256: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct NativeRank1ReportV1 {
    schema: String,
    publication_encoding: String,
    source: NativeRank1SourceV1,
    training: NativeRank1TrainingV1,
    weights: NativeRank1WeightsReportV1,
    interpretation: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct NativeRank1CandidateParentV1 {
    relative_directory: String,
    checkpoint_manifest_filename: String,
    checkpoint_state_filename: String,
    manifest_sha256: String,
    payload_sha256: String,
    native_state_sha256: String,
    model_parameter_sha256: String,
    training_corpus_sha256: String,
    adam_step: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct NativeRank1CandidateWeightsV1 {
    filename: String,
    encoding: String,
    count: usize,
    byte_count: usize,
    sha256: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct NativeRank1CandidateReportBindingV1 {
    filename: String,
    encoding: String,
    sha256: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct NativeRank1CandidateV1 {
    schema: String,
    publication_encoding: String,
    parent: NativeRank1CandidateParentV1,
    weights: NativeRank1CandidateWeightsV1,
    rank: usize,
    hidden_dim: usize,
    residual_formula: String,
    training_corpus_sha256: String,
    report: NativeRank1CandidateReportBindingV1,
}

#[derive(Clone, Debug)]
struct NativeRank1TrainingResultV1 {
    report: NativeRank1ReportV1,
    weights: NativeBilinearWeightsV1,
}

pub struct NativeRank1PolicyResidualProbeEnvelopeV1 {
    report_sha256: String,
    candidate_json_sha256: String,
    weights_sha256: String,
    elapsed_milliseconds: u64,
    disposition: String,
}

impl NativeRank1PolicyResidualProbeEnvelopeV1 {
    pub fn report_sha256_v1(&self) -> &str {
        &self.report_sha256
    }

    pub fn candidate_json_sha256_v1(&self) -> &str {
        &self.candidate_json_sha256
    }

    pub fn weights_sha256_v1(&self) -> &str {
        &self.weights_sha256
    }

    pub const fn elapsed_milliseconds_v1(&self) -> u64 {
        self.elapsed_milliseconds
    }

    pub fn disposition_v1(&self) -> &str {
        &self.disposition
    }
}

fn normalize_f64_v1(values: &mut [f64]) -> Result<f64, NativeBilinearErrorV1> {
    let norm = values.iter().map(|value| value * value).sum::<f64>().sqrt();
    if !norm.is_finite() || norm <= 0.0 {
        return Err(NativeBilinearErrorV1::RankOneProjectionFailed);
    }
    for value in values {
        *value /= norm;
    }
    Ok(norm)
}

fn deterministic_singular_start_v1(domain: usize) -> Result<Vec<f64>, NativeBilinearErrorV1> {
    let mut values = match domain {
        0 => (0..BILINEAR_HIDDEN_DIM_V1)
            .map(|index| {
                let magnitude = ((index * 37 + 11) % 127 + 1) as f64;
                if index.is_multiple_of(2) {
                    magnitude
                } else {
                    -magnitude
                }
            })
            .collect::<Vec<_>>(),
        1 => (0..BILINEAR_HIDDEN_DIM_V1)
            .map(|index| {
                let magnitude = ((index * 53 + 29) % 131 + 1) as f64;
                if (index + index / 3).is_multiple_of(2) {
                    magnitude
                } else {
                    -magnitude
                }
            })
            .collect::<Vec<_>>(),
        _ => return Err(NativeBilinearErrorV1::RankOneProjectionFailed),
    };
    normalize_f64_v1(&mut values)?;
    Ok(values)
}

fn singular_candidate_v1(
    gradient: &[f64],
    mut right: Vec<f64>,
) -> Result<(Vec<f64>, Vec<f64>, f64, f64), NativeBilinearErrorV1> {
    let mut left = vec![0.0_f64; BILINEAR_HIDDEN_DIM_V1];
    for _ in 0..RANK1_POWER_ITERATIONS_V1 {
        left.fill(0.0);
        for row in 0..BILINEAR_HIDDEN_DIM_V1 {
            let begin = row * BILINEAR_HIDDEN_DIM_V1;
            for column in 0..BILINEAR_HIDDEN_DIM_V1 {
                left[row] += gradient[begin + column] * right[column];
            }
        }
        normalize_f64_v1(&mut left)?;
        right.fill(0.0);
        for column in 0..BILINEAR_HIDDEN_DIM_V1 {
            for row in 0..BILINEAR_HIDDEN_DIM_V1 {
                right[column] += gradient[row * BILINEAR_HIDDEN_DIM_V1 + column] * left[row];
            }
        }
        normalize_f64_v1(&mut right)?;
    }
    left.fill(0.0);
    for row in 0..BILINEAR_HIDDEN_DIM_V1 {
        let begin = row * BILINEAR_HIDDEN_DIM_V1;
        for column in 0..BILINEAR_HIDDEN_DIM_V1 {
            left[row] += gradient[begin + column] * right[column];
        }
    }
    let singular_value = normalize_f64_v1(&mut left)?;
    let mut residual_squared = 0.0_f64;
    for column in 0..BILINEAR_HIDDEN_DIM_V1 {
        let mut projected = 0.0_f64;
        for row in 0..BILINEAR_HIDDEN_DIM_V1 {
            projected += gradient[row * BILINEAR_HIDDEN_DIM_V1 + column] * left[row];
        }
        let residual = projected - singular_value * right[column];
        residual_squared += residual * residual;
    }
    let relative_right_residual = residual_squared.sqrt() / singular_value;
    if !relative_right_residual.is_finite() {
        return Err(NativeBilinearErrorV1::RankOneProjectionFailed);
    }
    Ok((left, right, singular_value, relative_right_residual))
}

fn top_singular_projection_v1(
    gradient: &[f64],
) -> Result<(Vec<f32>, Vec<f32>, f64, f64, f64, f64), NativeBilinearErrorV1> {
    if gradient.len() != BILINEAR_WEIGHT_COUNT_V1 || gradient.iter().any(|value| !value.is_finite())
    {
        return Err(NativeBilinearErrorV1::RankOneProjectionFailed);
    }
    let frobenius_norm = gradient
        .iter()
        .map(|value| value * value)
        .sum::<f64>()
        .sqrt();
    if !frobenius_norm.is_finite() || frobenius_norm <= 0.0 {
        return Err(NativeBilinearErrorV1::RankOneProjectionFailed);
    }
    let first = singular_candidate_v1(gradient, deterministic_singular_start_v1(0)?)?;
    let second = singular_candidate_v1(gradient, deterministic_singular_start_v1(1)?)?;
    let (mut left, mut right, singular_value, relative_right_residual) =
        if second.2 > first.2 { second } else { first };
    if relative_right_residual > RANK1_RELATIVE_RIGHT_RESIDUAL_MAX_V1 {
        return Err(NativeBilinearErrorV1::RankOneProjectionFailed);
    }
    let sign_anchor = left
        .iter()
        .enumerate()
        .max_by(|left, right| left.1.abs().total_cmp(&right.1.abs()))
        .map(|(_, value)| *value)
        .ok_or(NativeBilinearErrorV1::RankOneProjectionFailed)?;
    if sign_anchor < 0.0 {
        for value in &mut left {
            *value = -*value;
        }
        for value in &mut right {
            *value = -*value;
        }
    }
    let captured_fraction = (singular_value * singular_value) / (frobenius_norm * frobenius_norm);
    if !singular_value.is_finite()
        || singular_value <= 0.0
        || !captured_fraction.is_finite()
        || !(0.0..=1.0 + 1.0e-12).contains(&captured_fraction)
    {
        return Err(NativeBilinearErrorV1::RankOneProjectionFailed);
    }
    let left = left
        .into_iter()
        .map(|value| value as f32)
        .collect::<Vec<_>>();
    let right = right
        .into_iter()
        .map(|value| value as f32)
        .collect::<Vec<_>>();
    if left.iter().chain(&right).any(|value| !value.is_finite()) {
        return Err(NativeBilinearErrorV1::RankOneProjectionFailed);
    }
    Ok((
        left,
        right,
        frobenius_norm,
        singular_value,
        captured_fraction.min(1.0),
        relative_right_residual,
    ))
}

fn expanded_rank1_direction_v1(
    left: &[f32],
    right: &[f32],
) -> Result<Vec<f32>, NativeBilinearErrorV1> {
    if left.len() != BILINEAR_HIDDEN_DIM_V1 || right.len() != BILINEAR_HIDDEN_DIM_V1 {
        return Err(NativeBilinearErrorV1::RankOneProjectionFailed);
    }
    let mut direction = Vec::with_capacity(BILINEAR_WEIGHT_COUNT_V1);
    for left_value in left {
        for right_value in right {
            direction.push((f64::from(*left_value) * f64::from(*right_value)) as f32);
        }
    }
    if direction.iter().any(|value| !value.is_finite()) {
        return Err(NativeBilinearErrorV1::RankOneProjectionFailed);
    }
    Ok(direction)
}

fn rank1_movement_metrics_v1(
    metrics: &NativeBilinearMovementMetricsV1,
) -> NativeRank1MovementMetricsV1 {
    NativeRank1MovementMetricsV1 {
        physical_group_count: metrics.physical_group_count,
        substep_count: metrics.substep_count,
        mean_total_variation: metrics.mean_total_variation,
        p90_total_variation: metrics.p90_total_variation,
        mean_parent_to_candidate_kl: metrics.mean_parent_to_candidate_kl,
        training_policy_surrogate_improvement: metrics.policy_surrogate_improvement,
        p0_training_policy_surrogate_improvement: metrics.p0_policy_surrogate_improvement,
        p1_training_policy_surrogate_improvement: metrics.p1_policy_surrogate_improvement,
    }
}

fn movement_is_safe_v1(metrics: &NativeBilinearMovementMetricsV1) -> bool {
    metrics.mean_parent_to_candidate_kl <= RANK1_MAX_MEAN_KL_V1
        && metrics.p90_total_variation <= RANK1_MAX_P90_TV_V1
}

fn safety_cap_rank1_weights_v1(
    dataset: &NativeBilinearDatasetV1,
    indices: &[usize],
    coefficients: &[AdvantageCoefficientV1],
    direction: &[f32],
    target_scale: f64,
) -> Result<(NativeBilinearWeightsV1, f64, bool), NativeBilinearErrorV1> {
    let target_weights = scaled_weights_v1(direction, target_scale)?;
    let target_metrics = movement_metrics_v1(dataset, coefficients, &target_weights)?;
    if movement_is_safe_v1(&target_metrics) {
        return Ok((target_weights, target_scale, false));
    }
    let mut low = 0.0_f64;
    let mut high = target_scale;
    for _ in 0..CALIBRATION_BISECTIONS_V1 {
        let midpoint = (low + high) * 0.5;
        let weights = scaled_weights_v1(direction, midpoint)?;
        let metrics = movement_metrics_v1(dataset, coefficients, &weights)?;
        if movement_is_safe_v1(&metrics) {
            low = midpoint;
        } else {
            high = midpoint;
        }
    }
    let weights = scaled_weights_v1(direction, low)?;
    let metrics = movement_metrics_v1(dataset, coefficients, &weights)?;
    if !movement_is_safe_v1(&metrics)
        || mean_tv_v1(dataset, indices, &weights)?
            > RANK1_TARGET_MEAN_TV_V1 + CALIBRATION_TOLERANCE_V1
    {
        return Err(NativeBilinearErrorV1::CalibrationFailed);
    }
    Ok((weights, low, true))
}

fn train_native_rank1_policy_residual_v1(
    dataset: &NativeBilinearDatasetV1,
    source: NativeRank1SourceV1,
) -> Result<NativeRank1TrainingResultV1, NativeBilinearErrorV1> {
    validate_dataset_v1(dataset)?;
    let indices = (0..dataset.groups.len()).collect::<Vec<_>>();
    let advantage = fit_advantage_stats_v1(dataset, &indices)?;
    let coefficients = coefficients_v1(dataset, &indices, &advantage)?;
    let gradient = analytic_gradient_matrix_v1(dataset, &coefficients)?;
    let (left, right, gradient_norm, singular_value, captured_fraction, relative_right_residual) =
        top_singular_projection_v1(&gradient)?;
    let direction = expanded_rank1_direction_v1(&left, &right)?;
    let expanded_direction_l2 = direction
        .iter()
        .map(|value| f64::from(*value).powi(2))
        .sum::<f64>()
        .sqrt();
    if !expanded_direction_l2.is_finite() || expanded_direction_l2 <= 0.0 {
        return Err(NativeBilinearErrorV1::RankOneProjectionFailed);
    }
    let (target_weights, target_calibration) = calibrate_to_target_v1(
        dataset,
        &indices,
        &direction,
        gradient_norm,
        RANK1_TARGET_MEAN_TV_V1,
    )?;
    let (weights, final_scale, safety_cap_applied) = safety_cap_rank1_weights_v1(
        dataset,
        &indices,
        &coefficients,
        &direction,
        target_calibration.calibrated_scale,
    )?;
    let movement = movement_metrics_v1(dataset, &coefficients, &weights)?;
    let mean_tv = mean_tv_v1(dataset, &indices, &weights)?;
    let weights_l2 = weights
        .values_v1()
        .iter()
        .map(|value| f64::from(*value).powi(2))
        .sum::<f64>()
        .sqrt();
    let exact_parent_at_zero = exact_parent_at_zero_v1(dataset);
    let finite_outputs = weights.values_v1().iter().all(|value| value.is_finite())
        && [
            movement.mean_total_variation,
            movement.p90_total_variation,
            movement.mean_parent_to_candidate_kl,
            movement.policy_surrogate_improvement,
            movement.p0_policy_surrogate_improvement,
            movement.p1_policy_surrogate_improvement,
        ]
        .into_iter()
        .all(f64::is_finite);
    let rank_one_projection_valid = left.len() == BILINEAR_HIDDEN_DIM_V1
        && right.len() == BILINEAR_HIDDEN_DIM_V1
        && captured_fraction > 0.0
        && captured_fraction <= 1.0
        && relative_right_residual <= RANK1_RELATIVE_RIGHT_RESIDUAL_MAX_V1;
    let mean_total_variation_at_most_0p01 =
        mean_tv <= RANK1_TARGET_MEAN_TV_V1 + CALIBRATION_TOLERANCE_V1;
    let mean_parent_to_candidate_kl_at_most_0p005 =
        movement.mean_parent_to_candidate_kl <= RANK1_MAX_MEAN_KL_V1;
    let p90_total_variation_at_most_0p04 = movement.p90_total_variation <= RANK1_MAX_P90_TV_V1;
    let publish_candidate = exact_parent_at_zero
        && finite_outputs
        && rank_one_projection_valid
        && mean_total_variation_at_most_0p01
        && mean_parent_to_candidate_kl_at_most_0p005
        && p90_total_variation_at_most_0p04;
    if !publish_candidate {
        return Err(NativeBilinearErrorV1::CalibrationFailed);
    }
    let weights_sha256 = weights.sha256_v1();
    let report = NativeRank1ReportV1 {
        schema: RANK1_REPORT_SCHEMA_V1.to_owned(),
        publication_encoding: RANK1_PUBLICATION_ENCODING_V1.to_owned(),
        source,
        training: NativeRank1TrainingV1 {
            objective: RANK1_OBJECTIVE_V1.to_owned(),
            architecture: RANK1_ARCHITECTURE_V1.to_owned(),
            pair_use: RANK1_PAIR_USE_V1.to_owned(),
            parent_parameters_frozen: true,
            value_head_frozen: true,
            outcome_independent_scale_selection: true,
            pair_count: dataset.pair_count,
            episode_count: dataset.episode_count,
            physical_group_count: dataset.physical_group_count,
            substep_count: dataset.substep_count,
            expanded_parameter_count: BILINEAR_WEIGHT_COUNT_V1,
            advantage: NativeRank1AdvantageStatsV1 {
                identity: RANK1_ADVANTAGE_IDENTITY_V1.to_owned(),
                physical_group_count: advantage.fit_group_count,
                episode_count: advantage.fit_episode_count,
                weighted_mean: advantage.weighted_mean,
                weighted_population_standard_deviation: advantage
                    .weighted_population_standard_deviation,
                normalization_denominator: advantage.normalization_denominator,
            },
            projection: NativeRank1ProjectionV1 {
                identity: RANK1_PROJECTION_IDENTITY_V1.to_owned(),
                rank: 1,
                effective_degrees_of_freedom: RANK1_EFFECTIVE_DEGREES_OF_FREEDOM_V1,
                power_iteration_count: RANK1_POWER_ITERATIONS_V1,
                initialization: RANK1_INITIALIZATION_V1.to_owned(),
                gradient_frobenius_norm: gradient_norm,
                top_singular_value: singular_value,
                captured_gradient_energy_fraction: captured_fraction,
                relative_right_residual,
                expanded_direction_l2,
                left_singular_vector_f32_bits: left.iter().map(|value| value.to_bits()).collect(),
                right_singular_vector_f32_bits: right.iter().map(|value| value.to_bits()).collect(),
            },
            calibration: NativeRank1CalibrationV1 {
                identity: RANK1_CALIBRATION_IDENTITY_V1.to_owned(),
                target_mean_total_variation: RANK1_TARGET_MEAN_TV_V1,
                target_calibrated_scale: target_calibration.calibrated_scale,
                final_scale,
                safety_cap_applied,
                achieved_mean_total_variation: mean_tv,
                weights_l2,
                weights_sha256: weights_sha256.clone(),
            },
            movement: rank1_movement_metrics_v1(&movement),
            safety_gates: NativeRank1SafetyGatesV1 {
                exact_parent_at_zero,
                finite_outputs,
                rank_one_projection_valid,
                mean_total_variation_at_most_0p01,
                mean_parent_to_candidate_kl_at_most_0p005,
                p90_total_variation_at_most_0p04,
                publish_candidate,
            },
            disposition: RANK1_DISPOSITION_V1.to_owned(),
        },
        weights: NativeRank1WeightsReportV1 {
            filename: RANK1_WEIGHTS_FILENAME_V1.to_owned(),
            encoding: RANK1_WEIGHTS_ENCODING_V1.to_owned(),
            count: BILINEAR_WEIGHT_COUNT_V1,
            byte_count: BILINEAR_WEIGHT_COUNT_V1 * size_of::<f32>(),
            sha256: weights_sha256,
        },
        interpretation: RANK1_INTERPRETATION_V1.to_owned(),
    };
    drop(target_weights);
    Ok(NativeRank1TrainingResultV1 { report, weights })
}

fn canonical_json_bytes_v1<T: Serialize>(value: &T) -> Result<Vec<u8>, Box<dyn Error>> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn write_new_file_v1(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(bytes)?;
    file.flush()?;
    file.sync_all()
}

fn raw_sha256_v1(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

fn parse_lower_hex32_v1(value: &str) -> Result<[u8; 32], io::Error> {
    if value.len() != 64
        || !value
            .as_bytes()
            .iter()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(byte))
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid SHA-256",
        ));
    }
    let mut output = [0_u8; 32];
    for (index, byte) in output.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "invalid SHA-256"))?;
    }
    Ok(output)
}

fn composite_model_parameter_sha256_v1(
    parent_model_parameter_sha256: [u8; 32],
    weights_bytes: &[u8],
) -> [u8; 32] {
    let mut digest = Sha256::new();
    digest.update((RANK1_COMPOSITE_MODEL_DOMAIN_V1.len() as u64).to_be_bytes());
    digest.update(RANK1_COMPOSITE_MODEL_DOMAIN_V1);
    digest.update((parent_model_parameter_sha256.len() as u64).to_be_bytes());
    digest.update(parent_model_parameter_sha256);
    digest.update((weights_bytes.len() as u64).to_be_bytes());
    digest.update(weights_bytes);
    digest.finalize().into()
}

fn rank1_source_v1(source: &NativeXmageCp7OutcomeBilinearDatasetV1) -> NativeRank1SourceV1 {
    NativeRank1SourceV1 {
        parent_manifest_sha256: lower_hex_v1(source.parent_manifest_sha256),
        parent_payload_sha256: lower_hex_v1(source.parent_payload_sha256),
        parent_native_state_sha256: lower_hex_v1(source.parent_native_state_sha256),
        parent_model_parameter_sha256: lower_hex_v1(source.parent_model_parameter_sha256),
        parent_training_corpus_sha256: lower_hex_v1(source.parent_corpus_sha256),
        parent_adam_step: source.parent_adam_step,
        training_corpus_sha256: lower_hex_v1(source.jsonl_sha256),
        outcome_export_contract: source.export_contract.clone(),
        outcome_schema_version: source.schema_version,
        outcome_decision_row_count: source.decision_row_count,
        outcome_terminal_row_count: source.terminal_row_count,
        outcome_terminal_return_counts_minus_one_zero_plus_one: source.terminal_return_counts,
    }
}

pub fn run_native_rank1_policy_residual_v1(
    source_outcome_root: impl AsRef<Path>,
    outcome_jsonl: impl AsRef<Path>,
    output_directory: impl AsRef<Path>,
) -> Result<NativeRank1PolicyResidualProbeEnvelopeV1, Box<dyn Error>> {
    let started = Instant::now();
    let source_outcome_root = source_outcome_root.as_ref();
    let output_directory = output_directory.as_ref();
    let parent = load_xmage_cp7_outcome_inference_v1(source_outcome_root)?;
    let source_dataset =
        load_xmage_cp7_outcome_bilinear_dataset_v1(outcome_jsonl.as_ref(), &parent)?;
    require_fixed_source_v1(&source_dataset)?;
    if lower_hex_v1(source_dataset.parent_corpus_sha256)
        != RETAINED_PARENT_TRAINING_CORPUS_SHA256_V1
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "rank1 parent training corpus is not exact",
        )
        .into());
    }
    let source = rank1_source_v1(&source_dataset);
    let dataset = convert_source_dataset_v1(source_dataset);
    let trained = train_native_rank1_policy_residual_v1(&dataset, source)?;
    let weights_bytes = trained.weights.f32le_bytes_v1();
    let report_bytes = canonical_json_bytes_v1(&trained.report)?;
    let report_sha256 = lower_hex_v1(raw_sha256_v1(&report_bytes));
    let weights_sha256 = lower_hex_v1(raw_sha256_v1(&weights_bytes));
    let candidate = NativeRank1CandidateV1 {
        schema: RANK1_CANDIDATE_SCHEMA_V1.to_owned(),
        publication_encoding: RANK1_PUBLICATION_ENCODING_V1.to_owned(),
        parent: NativeRank1CandidateParentV1 {
            relative_directory: RANK1_PARENT_DIRECTORY_V1.to_owned(),
            checkpoint_manifest_filename: RANK1_PARENT_MANIFEST_FILENAME_V1.to_owned(),
            checkpoint_state_filename: RANK1_PARENT_STATE_FILENAME_V1.to_owned(),
            manifest_sha256: RETAINED_MANIFEST_SHA256_V1.to_owned(),
            payload_sha256: RETAINED_PAYLOAD_SHA256_V1.to_owned(),
            native_state_sha256: RETAINED_NATIVE_STATE_SHA256_V1.to_owned(),
            model_parameter_sha256: RETAINED_MODEL_PARAMETER_SHA256_V1.to_owned(),
            training_corpus_sha256: RETAINED_PARENT_TRAINING_CORPUS_SHA256_V1.to_owned(),
            adam_step: RETAINED_ADAM_STEP_V1,
        },
        weights: NativeRank1CandidateWeightsV1 {
            filename: RANK1_WEIGHTS_FILENAME_V1.to_owned(),
            encoding: RANK1_WEIGHTS_ENCODING_V1.to_owned(),
            count: BILINEAR_WEIGHT_COUNT_V1,
            byte_count: weights_bytes.len(),
            sha256: weights_sha256.clone(),
        },
        rank: 1,
        hidden_dim: BILINEAR_HIDDEN_DIM_V1,
        residual_formula: RANK1_RESIDUAL_FORMULA_V1.to_owned(),
        training_corpus_sha256: FIXED_CORPUS_SHA256_V1.to_owned(),
        report: NativeRank1CandidateReportBindingV1 {
            filename: RANK1_REPORT_FILENAME_V1.to_owned(),
            encoding: RANK1_PUBLICATION_ENCODING_V1.to_owned(),
            sha256: report_sha256.clone(),
        },
    };
    let candidate_bytes = canonical_json_bytes_v1(&candidate)?;
    let candidate_json_sha256 = lower_hex_v1(raw_sha256_v1(&candidate_bytes));

    fs::create_dir(output_directory)?;
    let parent_directory = output_directory.join(RANK1_PARENT_DIRECTORY_V1);
    fs::create_dir(&parent_directory)?;
    let parent_manifest_bytes =
        fs::read(source_outcome_root.join(RANK1_PARENT_MANIFEST_FILENAME_V1))?;
    let parent_state_bytes = fs::read(source_outcome_root.join(RANK1_PARENT_STATE_FILENAME_V1))?;
    if lower_hex_v1(raw_sha256_v1(&parent_manifest_bytes)) != RETAINED_MANIFEST_SHA256_V1
        || lower_hex_v1(raw_sha256_v1(&parent_state_bytes)) != RETAINED_PAYLOAD_SHA256_V1
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "rank1 parent copy source changed",
        )
        .into());
    }
    write_new_file_v1(
        &parent_directory.join(RANK1_PARENT_MANIFEST_FILENAME_V1),
        &parent_manifest_bytes,
    )?;
    write_new_file_v1(
        &parent_directory.join(RANK1_PARENT_STATE_FILENAME_V1),
        &parent_state_bytes,
    )?;
    write_new_file_v1(
        &output_directory.join(RANK1_WEIGHTS_FILENAME_V1),
        &weights_bytes,
    )?;
    write_new_file_v1(
        &output_directory.join(RANK1_REPORT_FILENAME_V1),
        &report_bytes,
    )?;
    write_new_file_v1(
        &output_directory.join(RANK1_CANDIDATE_FILENAME_V1),
        &candidate_bytes,
    )?;
    let elapsed_milliseconds = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    Ok(NativeRank1PolicyResidualProbeEnvelopeV1 {
        report_sha256,
        candidate_json_sha256,
        weights_sha256,
        elapsed_milliseconds,
        disposition: trained.report.training.disposition,
    })
}

pub(crate) struct NativeRank1PolicyResidualInferenceOutputV1 {
    logits: Vec<f32>,
    value: f32,
}

impl NativeRank1PolicyResidualInferenceOutputV1 {
    pub(crate) fn logits_v1(&self) -> &[f32] {
        &self.logits
    }

    pub(crate) const fn value_v1(&self) -> f32 {
        self.value
    }
}

pub(crate) struct NativeRank1PolicyResidualInferenceV1 {
    parent: NativeXmageCp7OutcomeInferenceV1,
    weights: NativeBilinearWeightsV1,
    candidate_json_sha256: [u8; 32],
    weights_sha256: [u8; 32],
    report_sha256: [u8; 32],
    composite_model_parameter_sha256: [u8; 32],
}

impl NativeRank1PolicyResidualInferenceV1 {
    pub(crate) const fn candidate_json_sha256_v1(&self) -> [u8; 32] {
        self.candidate_json_sha256
    }

    pub(crate) const fn weights_sha256_v1(&self) -> [u8; 32] {
        self.weights_sha256
    }

    pub(crate) const fn report_sha256_v1(&self) -> [u8; 32] {
        self.report_sha256
    }

    pub(crate) const fn composite_model_parameter_sha256_v1(&self) -> [u8; 32] {
        self.composite_model_parameter_sha256
    }

    pub(crate) const fn parent_adam_step_v1(&self) -> u64 {
        self.parent.adam_step_v1()
    }

    pub(crate) fn score_decision_v1(
        &self,
        decision: FlatScoringDecisionViewV2<'_>,
    ) -> Result<NativeRank1PolicyResidualInferenceOutputV1, ()> {
        let parent = self.parent.score_decision_with_latents_v1(decision)?;
        let logits = apply_bilinear_residual_v1(
            parent.logits_v1(),
            parent.state_hidden_v1(),
            parent.action_hidden_v1(),
            &self.weights,
        )
        .map_err(|_| ())?;
        Ok(NativeRank1PolicyResidualInferenceOutputV1 {
            logits,
            value: parent.value_v1(),
        })
    }
}

fn strict_canonical_json_v1<T>(bytes: &[u8]) -> Result<T, Box<dyn Error>>
where
    T: for<'de> Deserialize<'de> + Serialize,
{
    if !bytes.ends_with(b"\n") || bytes.contains(&b'\r') {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "invalid JSON framing").into());
    }
    let text = std::str::from_utf8(bytes)?;
    let value = parse_strict_json_value(text)?;
    let decoded = serde_json::from_value(value)?;
    if canonical_json_bytes_v1(&decoded)? != bytes {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "noncanonical JSON").into());
    }
    Ok(decoded)
}

fn decode_rank1_weights_v1(bytes: &[u8]) -> Result<NativeBilinearWeightsV1, Box<dyn Error>> {
    if bytes.len() != BILINEAR_WEIGHT_COUNT_V1 * size_of::<f32>() {
        return Err(
            io::Error::new(io::ErrorKind::InvalidData, "invalid rank1 weight length").into(),
        );
    }
    let values = bytes
        .chunks_exact(4)
        .map(|chunk| f32::from_bits(u32::from_le_bytes(chunk.try_into().expect("four bytes"))))
        .collect::<Vec<_>>();
    Ok(NativeBilinearWeightsV1::from_values_v1(values)?)
}

fn validate_exact_parent_inventory_v1(parent: &Path) -> Result<(), Box<dyn Error>> {
    let mut inventory = BTreeSet::new();
    for entry in fs::read_dir(parent)? {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "rank1 parent inventory contains a non-file entry",
            )
            .into());
        }
        inventory.insert(entry.file_name().into_string().map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "non-UTF8 rank1 parent inventory",
            )
        })?);
    }
    if inventory
        != BTreeSet::from([
            RANK1_PARENT_MANIFEST_FILENAME_V1.to_owned(),
            RANK1_PARENT_STATE_FILENAME_V1.to_owned(),
        ])
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "rank1 parent inventory is not exact",
        )
        .into());
    }
    Ok(())
}

fn validate_rank1_report_semantics_v1(
    report: &NativeRank1ReportV1,
    weights: &NativeBilinearWeightsV1,
    weights_bytes: &[u8],
    weights_sha256: [u8; 32],
) -> Result<(), Box<dyn Error>> {
    let training = &report.training;
    let projection = &training.projection;
    let calibration = &training.calibration;
    let movement = &training.movement;
    let gates = &training.safety_gates;
    let advantage = &training.advantage;
    let weights_sha256_hex = lower_hex_v1(weights_sha256);
    if report.schema != RANK1_REPORT_SCHEMA_V1
        || report.publication_encoding != RANK1_PUBLICATION_ENCODING_V1
        || report.source.parent_manifest_sha256 != RETAINED_MANIFEST_SHA256_V1
        || report.source.parent_payload_sha256 != RETAINED_PAYLOAD_SHA256_V1
        || report.source.parent_native_state_sha256 != RETAINED_NATIVE_STATE_SHA256_V1
        || report.source.parent_model_parameter_sha256 != RETAINED_MODEL_PARAMETER_SHA256_V1
        || report.source.parent_training_corpus_sha256 != RETAINED_PARENT_TRAINING_CORPUS_SHA256_V1
        || report.source.parent_adam_step != RETAINED_ADAM_STEP_V1
        || report.source.training_corpus_sha256 != FIXED_CORPUS_SHA256_V1
        || report.source.outcome_export_contract != FIXED_OUTCOME_EXPORT_CONTRACT_V1
        || report.source.outcome_schema_version != FIXED_OUTCOME_SCHEMA_VERSION_V1
        || report.source.outcome_decision_row_count != FIXED_OUTCOME_DECISION_ROW_COUNT_V1
        || report.source.outcome_terminal_row_count != FIXED_OUTCOME_TERMINAL_ROW_COUNT_V1
        || report
            .source
            .outcome_terminal_return_counts_minus_one_zero_plus_one
            != FIXED_TERMINAL_RETURN_COUNTS_V1
        || training.objective != RANK1_OBJECTIVE_V1
        || training.architecture != RANK1_ARCHITECTURE_V1
        || training.pair_use != RANK1_PAIR_USE_V1
        || !training.parent_parameters_frozen
        || !training.value_head_frozen
        || !training.outcome_independent_scale_selection
        || training.pair_count != 32
        || training.episode_count != 64
        || training.physical_group_count != FIXED_PHYSICAL_GROUP_COUNT_V1
        || training.substep_count != FIXED_OUTCOME_DECISION_ROW_COUNT_V1
        || training.expanded_parameter_count != BILINEAR_WEIGHT_COUNT_V1
        || advantage.identity != RANK1_ADVANTAGE_IDENTITY_V1
        || advantage.physical_group_count != FIXED_PHYSICAL_GROUP_COUNT_V1
        || advantage.episode_count != 64
        || projection.identity != RANK1_PROJECTION_IDENTITY_V1
        || projection.rank != 1
        || projection.effective_degrees_of_freedom != RANK1_EFFECTIVE_DEGREES_OF_FREEDOM_V1
        || projection.power_iteration_count != RANK1_POWER_ITERATIONS_V1
        || projection.initialization != RANK1_INITIALIZATION_V1
        || calibration.identity != RANK1_CALIBRATION_IDENTITY_V1
        || calibration.target_mean_total_variation.to_bits() != RANK1_TARGET_MEAN_TV_V1.to_bits()
        || training.disposition != RANK1_DISPOSITION_V1
        || report.interpretation != RANK1_INTERPRETATION_V1
        || report.weights.filename != RANK1_WEIGHTS_FILENAME_V1
        || report.weights.encoding != RANK1_WEIGHTS_ENCODING_V1
        || report.weights.count != BILINEAR_WEIGHT_COUNT_V1
        || report.weights.byte_count != BILINEAR_WEIGHT_COUNT_V1 * size_of::<f32>()
        || report.weights.byte_count != weights_bytes.len()
        || report.weights.sha256 != weights_sha256_hex
        || calibration.weights_sha256 != weights_sha256_hex
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "rank1 report fixed semantic mismatch",
        )
        .into());
    }
    let finite_scalars = [
        advantage.weighted_mean,
        advantage.weighted_population_standard_deviation,
        advantage.normalization_denominator,
        projection.gradient_frobenius_norm,
        projection.top_singular_value,
        projection.captured_gradient_energy_fraction,
        projection.relative_right_residual,
        projection.expanded_direction_l2,
        calibration.target_calibrated_scale,
        calibration.final_scale,
        calibration.achieved_mean_total_variation,
        calibration.weights_l2,
        movement.mean_total_variation,
        movement.p90_total_variation,
        movement.mean_parent_to_candidate_kl,
        movement.training_policy_surrogate_improvement,
        movement.p0_training_policy_surrogate_improvement,
        movement.p1_training_policy_surrogate_improvement,
    ];
    if finite_scalars.into_iter().any(|value| !value.is_finite())
        || advantage.weighted_population_standard_deviation < 0.0
        || advantage.normalization_denominator.to_bits()
            != advantage
                .weighted_population_standard_deviation
                .max(STANDARD_DEVIATION_FLOOR_V1)
                .to_bits()
        || projection.gradient_frobenius_norm <= 0.0
        || projection.top_singular_value <= 0.0
        || !(0.0..=1.0).contains(&projection.captured_gradient_energy_fraction)
        || projection.captured_gradient_energy_fraction <= 0.0
        || projection.relative_right_residual < 0.0
        || projection.relative_right_residual > RANK1_RELATIVE_RIGHT_RESIDUAL_MAX_V1
        || projection.expanded_direction_l2 <= 0.0
        || calibration.target_calibrated_scale <= 0.0
        || calibration.final_scale <= 0.0
        || calibration.final_scale > calibration.target_calibrated_scale
        || calibration.achieved_mean_total_variation < 0.0
        || calibration.achieved_mean_total_variation > 1.0
        || calibration.weights_l2 <= 0.0
        || movement.physical_group_count != FIXED_PHYSICAL_GROUP_COUNT_V1
        || movement.substep_count != FIXED_OUTCOME_DECISION_ROW_COUNT_V1
        || !(0.0..=1.0).contains(&movement.mean_total_variation)
        || !(0.0..=1.0).contains(&movement.p90_total_variation)
        || movement.mean_parent_to_candidate_kl < 0.0
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "rank1 report scalar semantic mismatch",
        )
        .into());
    }
    let captured =
        projection.top_singular_value.powi(2) / projection.gradient_frobenius_norm.powi(2);
    if (captured - projection.captured_gradient_energy_fraction).abs()
        > 1.0e-12 * captured.abs().max(1.0)
        || calibration.achieved_mean_total_variation.to_bits()
            != movement.mean_total_variation.to_bits()
        || calibration.safety_cap_applied
            != (calibration.final_scale.to_bits() != calibration.target_calibrated_scale.to_bits())
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "rank1 report derived scalar mismatch",
        )
        .into());
    }
    let left = projection
        .left_singular_vector_f32_bits
        .iter()
        .copied()
        .map(f32::from_bits)
        .collect::<Vec<_>>();
    let right = projection
        .right_singular_vector_f32_bits
        .iter()
        .copied()
        .map(f32::from_bits)
        .collect::<Vec<_>>();
    if left.len() != BILINEAR_HIDDEN_DIM_V1
        || right.len() != BILINEAR_HIDDEN_DIM_V1
        || left.iter().chain(&right).any(|value| !value.is_finite())
    {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "invalid rank1 factors").into());
    }
    let left_norm = left
        .iter()
        .map(|value| f64::from(*value).powi(2))
        .sum::<f64>()
        .sqrt();
    let right_norm = right
        .iter()
        .map(|value| f64::from(*value).powi(2))
        .sum::<f64>()
        .sqrt();
    if (left_norm - 1.0).abs() > 1.0e-6 || (right_norm - 1.0).abs() > 1.0e-6 {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "nonunit rank1 factors").into());
    }
    let direction = expanded_rank1_direction_v1(&left, &right)?;
    let direction_l2 = direction
        .iter()
        .map(|value| f64::from(*value).powi(2))
        .sum::<f64>()
        .sqrt();
    let expected_weights = scaled_weights_v1(&direction, calibration.final_scale)?;
    let weights_l2 = weights
        .values_v1()
        .iter()
        .map(|value| f64::from(*value).powi(2))
        .sum::<f64>()
        .sqrt();
    if direction_l2.to_bits() != projection.expanded_direction_l2.to_bits()
        || weights_l2.to_bits() != calibration.weights_l2.to_bits()
        || expected_weights.f32le_bytes_v1() != weights_bytes
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "rank1 factor or weight derivation mismatch",
        )
        .into());
    }
    let expected_finite = weights.values_v1().iter().all(|value| value.is_finite())
        && [
            movement.mean_total_variation,
            movement.p90_total_variation,
            movement.mean_parent_to_candidate_kl,
            movement.training_policy_surrogate_improvement,
            movement.p0_training_policy_surrogate_improvement,
            movement.p1_training_policy_surrogate_improvement,
        ]
        .into_iter()
        .all(f64::is_finite);
    let expected_rank_valid = projection.captured_gradient_energy_fraction > 0.0
        && projection.captured_gradient_energy_fraction <= 1.0
        && projection.relative_right_residual <= RANK1_RELATIVE_RIGHT_RESIDUAL_MAX_V1;
    let expected_mean_tv =
        movement.mean_total_variation <= RANK1_TARGET_MEAN_TV_V1 + CALIBRATION_TOLERANCE_V1;
    let expected_mean_kl = movement.mean_parent_to_candidate_kl <= RANK1_MAX_MEAN_KL_V1;
    let expected_p90_tv = movement.p90_total_variation <= RANK1_MAX_P90_TV_V1;
    let expected_publish = gates.exact_parent_at_zero
        && expected_finite
        && expected_rank_valid
        && expected_mean_tv
        && expected_mean_kl
        && expected_p90_tv;
    if !gates.exact_parent_at_zero
        || gates.finite_outputs != expected_finite
        || gates.rank_one_projection_valid != expected_rank_valid
        || gates.mean_total_variation_at_most_0p01 != expected_mean_tv
        || gates.mean_parent_to_candidate_kl_at_most_0p005 != expected_mean_kl
        || gates.p90_total_variation_at_most_0p04 != expected_p90_tv
        || gates.publish_candidate != expected_publish
        || !gates.publish_candidate
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "rank1 report safety gate mismatch",
        )
        .into());
    }
    Ok(())
}

pub(crate) fn load_native_rank1_policy_residual_inference_v1(
    root: &Path,
) -> Result<NativeRank1PolicyResidualInferenceV1, Box<dyn Error>> {
    let mut files = BTreeSet::new();
    let mut directories = BTreeSet::new();
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "non-UTF8 rank1 inventory"))?;
        let file_type = entry.file_type()?;
        if file_type.is_file() {
            files.insert(name);
        } else if file_type.is_dir() {
            directories.insert(name);
        } else {
            return Err(
                io::Error::new(io::ErrorKind::InvalidData, "invalid rank1 inventory type").into(),
            );
        }
    }
    if files
        != BTreeSet::from([
            RANK1_CANDIDATE_FILENAME_V1.to_owned(),
            RANK1_REPORT_FILENAME_V1.to_owned(),
            RANK1_WEIGHTS_FILENAME_V1.to_owned(),
        ])
        || directories != BTreeSet::from([RANK1_PARENT_DIRECTORY_V1.to_owned()])
    {
        return Err(
            io::Error::new(io::ErrorKind::InvalidData, "rank1 inventory is not exact").into(),
        );
    }
    let candidate_bytes = fs::read(root.join(RANK1_CANDIDATE_FILENAME_V1))?;
    let candidate: NativeRank1CandidateV1 = strict_canonical_json_v1(&candidate_bytes)?;
    let report_bytes = fs::read(root.join(RANK1_REPORT_FILENAME_V1))?;
    let report: NativeRank1ReportV1 = strict_canonical_json_v1(&report_bytes)?;
    let weights_bytes = fs::read(root.join(RANK1_WEIGHTS_FILENAME_V1))?;
    let weights_sha256 = raw_sha256_v1(&weights_bytes);
    let report_sha256 = raw_sha256_v1(&report_bytes);
    let candidate_json_sha256 = raw_sha256_v1(&candidate_bytes);
    if candidate.schema != RANK1_CANDIDATE_SCHEMA_V1
        || candidate.publication_encoding != RANK1_PUBLICATION_ENCODING_V1
        || candidate.rank != 1
        || candidate.hidden_dim != BILINEAR_HIDDEN_DIM_V1
        || candidate.residual_formula != RANK1_RESIDUAL_FORMULA_V1
        || candidate.training_corpus_sha256 != FIXED_CORPUS_SHA256_V1
        || candidate.parent.relative_directory != RANK1_PARENT_DIRECTORY_V1
        || candidate.parent.checkpoint_manifest_filename != RANK1_PARENT_MANIFEST_FILENAME_V1
        || candidate.parent.checkpoint_state_filename != RANK1_PARENT_STATE_FILENAME_V1
        || candidate.parent.manifest_sha256 != RETAINED_MANIFEST_SHA256_V1
        || candidate.parent.payload_sha256 != RETAINED_PAYLOAD_SHA256_V1
        || candidate.parent.native_state_sha256 != RETAINED_NATIVE_STATE_SHA256_V1
        || candidate.parent.model_parameter_sha256 != RETAINED_MODEL_PARAMETER_SHA256_V1
        || candidate.parent.training_corpus_sha256 != RETAINED_PARENT_TRAINING_CORPUS_SHA256_V1
        || candidate.parent.adam_step != RETAINED_ADAM_STEP_V1
        || candidate.weights.filename != RANK1_WEIGHTS_FILENAME_V1
        || candidate.weights.encoding != RANK1_WEIGHTS_ENCODING_V1
        || candidate.weights.count != BILINEAR_WEIGHT_COUNT_V1
        || candidate.weights.byte_count != BILINEAR_WEIGHT_COUNT_V1 * size_of::<f32>()
        || candidate.weights.byte_count != weights_bytes.len()
        || candidate.weights.sha256 != lower_hex_v1(weights_sha256)
        || candidate.report.filename != RANK1_REPORT_FILENAME_V1
        || candidate.report.encoding != RANK1_PUBLICATION_ENCODING_V1
        || candidate.report.sha256 != lower_hex_v1(report_sha256)
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "rank1 candidate binding mismatch",
        )
        .into());
    }
    let weights = decode_rank1_weights_v1(&weights_bytes)?;
    validate_rank1_report_semantics_v1(&report, &weights, &weights_bytes, weights_sha256)?;
    let parent_directory = root.join(RANK1_PARENT_DIRECTORY_V1);
    validate_exact_parent_inventory_v1(&parent_directory)?;
    let parent_manifest_bytes = fs::read(parent_directory.join(RANK1_PARENT_MANIFEST_FILENAME_V1))?;
    let parent_state_bytes = fs::read(parent_directory.join(RANK1_PARENT_STATE_FILENAME_V1))?;
    if lower_hex_v1(raw_sha256_v1(&parent_manifest_bytes)) != RETAINED_MANIFEST_SHA256_V1
        || lower_hex_v1(raw_sha256_v1(&parent_state_bytes)) != RETAINED_PAYLOAD_SHA256_V1
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "rank1 parent file digest mismatch",
        )
        .into());
    }
    let parent = load_xmage_cp7_outcome_inference_v1(&parent_directory)?;
    if parent.manifest_sha256_v1() != parse_lower_hex32_v1(RETAINED_MANIFEST_SHA256_V1)?
        || parent.payload_sha256_v1() != parse_lower_hex32_v1(RETAINED_PAYLOAD_SHA256_V1)?
        || parent.native_state_sha256_v1() != parse_lower_hex32_v1(RETAINED_NATIVE_STATE_SHA256_V1)?
        || parent.model_parameter_sha256_v1()
            != parse_lower_hex32_v1(RETAINED_MODEL_PARAMETER_SHA256_V1)?
        || parent.corpus_sha256_v1()
            != parse_lower_hex32_v1(RETAINED_PARENT_TRAINING_CORPUS_SHA256_V1)?
        || parent.adam_step_v1() != RETAINED_ADAM_STEP_V1
    {
        return Err(
            io::Error::new(io::ErrorKind::InvalidData, "rank1 loaded parent mismatch").into(),
        );
    }
    let composite_model_parameter_sha256 =
        composite_model_parameter_sha256_v1(parent.model_parameter_sha256_v1(), &weights_bytes);
    Ok(NativeRank1PolicyResidualInferenceV1 {
        parent,
        weights,
        candidate_json_sha256,
        weights_sha256,
        report_sha256,
        composite_model_parameter_sha256,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn synthetic_dataset_v1() -> NativeBilinearDatasetV1 {
        let mut groups = Vec::new();
        for pair in 0_u64..32 {
            for (seat_index, seat) in [PlayerSeatV1::P0, PlayerSeatV1::P1].into_iter().enumerate() {
                let terminal_return = if (pair + seat_index as u64).is_multiple_of(2) {
                    1
                } else {
                    -1
                };
                let mut state_hidden = [0.0; BILINEAR_HIDDEN_DIM_V1];
                state_hidden[0] = f32::from(terminal_return);
                let mut action_hidden = vec![0.0; 2 * BILINEAR_HIDDEN_DIM_V1];
                action_hidden[0] = 1.0;
                action_hidden[BILINEAR_HIDDEN_DIM_V1] = -1.0;
                groups.push(NativeBilinearPhysicalGroupV1 {
                    pair_index: pair,
                    episode_id: pair * 2 + seat_index as u64,
                    candidate_seat: seat,
                    terminal_return,
                    first_substep_parent_value: 0.0,
                    substeps: vec![NativeBilinearSubstepV1 {
                        parent_logits: vec![0.0, 0.0],
                        selected_index: 0,
                        state_hidden,
                        action_hidden,
                    }],
                });
            }
        }
        NativeBilinearDatasetV1 {
            pair_count: 32,
            episode_count: 64,
            physical_group_count: groups.len(),
            substep_count: groups.len(),
            groups,
        }
    }

    fn synthetic_rank1_source_v1() -> NativeRank1SourceV1 {
        NativeRank1SourceV1 {
            parent_manifest_sha256: RETAINED_MANIFEST_SHA256_V1.to_owned(),
            parent_payload_sha256: RETAINED_PAYLOAD_SHA256_V1.to_owned(),
            parent_native_state_sha256: RETAINED_NATIVE_STATE_SHA256_V1.to_owned(),
            parent_model_parameter_sha256: RETAINED_MODEL_PARAMETER_SHA256_V1.to_owned(),
            parent_training_corpus_sha256: RETAINED_PARENT_TRAINING_CORPUS_SHA256_V1.to_owned(),
            parent_adam_step: RETAINED_ADAM_STEP_V1,
            training_corpus_sha256: FIXED_CORPUS_SHA256_V1.to_owned(),
            outcome_export_contract: "synthetic/v1".to_owned(),
            outcome_schema_version: 1,
            outcome_decision_row_count: 64,
            outcome_terminal_row_count: 64,
            outcome_terminal_return_counts_minus_one_zero_plus_one: [32, 0, 32],
        }
    }

    #[test]
    fn zero_residual_is_bit_exact_and_signed_zero_safe_v1() {
        let mut parent = vec![-0.0, 1.0];
        let state = [0.5; BILINEAR_HIDDEN_DIM_V1];
        let actions = vec![0.25; 2 * BILINEAR_HIDDEN_DIM_V1];
        let output = apply_bilinear_residual_v1(
            &parent,
            &state,
            &actions,
            &NativeBilinearWeightsV1::zero_v1(),
        )
        .unwrap();
        assert_eq!(output[0].to_bits(), (-0.0_f32).to_bits());
        assert_eq!(output[1].to_bits(), 1.0_f32.to_bits());
        parent[0] = 0.0;
        assert_ne!(output[0].to_bits(), parent[0].to_bits());
    }

    #[test]
    fn synthetic_contextual_signal_passes_the_fixed_screen_v1() {
        let result = train_native_bilinear_policy_residual_v1(&synthetic_dataset_v1()).unwrap();
        assert!(result.report.gates.advance_to_cp7);
        assert!(result.report.heldout_metrics.policy_surrogate_improvement > 0.0);
        assert!(
            result
                .report
                .heldout_metrics
                .p0_policy_surrogate_improvement
                > 0.0
        );
        assert!(
            result
                .report
                .heldout_metrics
                .p1_policy_surrogate_improvement
                > 0.0
        );
        assert!(result.final_weights.is_some());
    }

    #[test]
    fn heldout_outcomes_cannot_change_the_fit_candidate_v1() {
        let original = synthetic_dataset_v1();
        let original_result = train_native_bilinear_policy_residual_v1(&original).unwrap();
        let mut mutated = original;
        for group in &mut mutated.groups {
            if is_heldout_pair_v1(group.pair_index) {
                group.terminal_return = -group.terminal_return;
            }
        }
        let mutated_result = train_native_bilinear_policy_residual_v1(&mutated).unwrap();
        assert_eq!(
            original_result.report.fit_calibration.weights_sha256,
            mutated_result.report.fit_calibration.weights_sha256
        );
        assert_eq!(
            original_result
                .report
                .fit_calibration
                .calibrated_scale
                .to_bits(),
            mutated_result
                .report
                .fit_calibration
                .calibrated_scale
                .to_bits()
        );
    }

    #[test]
    fn malformed_shape_fails_closed_v1() {
        let mut dataset = synthetic_dataset_v1();
        dataset.groups[0].substeps[0].action_hidden.pop();
        assert_eq!(
            train_native_bilinear_policy_residual_v1(&dataset).unwrap_err(),
            NativeBilinearErrorV1::InvalidDataset
        );
    }

    #[test]
    fn top_singular_projection_is_deterministic_and_captures_known_energy_v1() {
        let mut gradient = vec![0.0_f64; BILINEAR_WEIGHT_COUNT_V1];
        gradient[0] = 5.0;
        gradient[BILINEAR_HIDDEN_DIM_V1 + 1] = 2.0;
        let first = top_singular_projection_v1(&gradient).unwrap();
        let second = top_singular_projection_v1(&gradient).unwrap();
        assert_eq!(
            first
                .0
                .iter()
                .map(|value| value.to_bits())
                .collect::<Vec<_>>(),
            second
                .0
                .iter()
                .map(|value| value.to_bits())
                .collect::<Vec<_>>()
        );
        assert_eq!(
            first
                .1
                .iter()
                .map(|value| value.to_bits())
                .collect::<Vec<_>>(),
            second
                .1
                .iter()
                .map(|value| value.to_bits())
                .collect::<Vec<_>>()
        );
        assert!((first.2 - 29.0_f64.sqrt()).abs() <= 1.0e-12);
        assert!((first.3 - 5.0).abs() <= 1.0e-12);
        assert!((first.4 - 25.0 / 29.0).abs() <= 1.0e-12);
        assert!(first.5 <= RANK1_RELATIVE_RIGHT_RESIDUAL_MAX_V1);
    }

    #[test]
    fn second_start_recovers_top_mode_orthogonal_to_first_start_v1() {
        let first_start = deterministic_singular_start_v1(0).unwrap();
        let mut top_right = vec![0.0_f64; BILINEAR_HIDDEN_DIM_V1];
        top_right[0] = first_start[1];
        top_right[1] = -first_start[0];
        let top_singular_value = 8.0
            * top_right
                .iter()
                .map(|value| value * value)
                .sum::<f64>()
                .sqrt();
        assert!(top_singular_value > 0.1);
        let mut gradient = vec![0.0_f64; BILINEAR_WEIGHT_COUNT_V1];
        for column in 0..BILINEAR_HIDDEN_DIM_V1 {
            gradient[column] = 8.0 * top_right[column];
            gradient[BILINEAR_HIDDEN_DIM_V1 + column] = 0.1 * first_start[column];
        }
        let first_start_overlap = top_right
            .iter()
            .zip(&first_start)
            .map(|(left, right)| left * right)
            .sum::<f64>();
        assert!(first_start_overlap.abs() <= f64::EPSILON);
        let recovered = top_singular_projection_v1(&gradient).unwrap();
        assert!((recovered.3 - top_singular_value).abs() <= 1.0e-12);
        assert!(recovered.5 <= RANK1_RELATIVE_RIGHT_RESIDUAL_MAX_V1);
    }

    #[test]
    fn synthetic_rank1_training_emits_safe_exact_rank_one_expansion_v1() {
        let result = train_native_rank1_policy_residual_v1(
            &synthetic_dataset_v1(),
            synthetic_rank1_source_v1(),
        )
        .unwrap();
        assert!(result.report.training.safety_gates.publish_candidate);
        assert_eq!(result.report.training.projection.rank, 1);
        assert_eq!(
            result
                .report
                .training
                .projection
                .effective_degrees_of_freedom,
            127
        );
        assert!(
            result.report.training.movement.mean_total_variation
                <= RANK1_TARGET_MEAN_TV_V1 + CALIBRATION_TOLERANCE_V1
        );
        let left = result
            .report
            .training
            .projection
            .left_singular_vector_f32_bits
            .iter()
            .copied()
            .map(f32::from_bits)
            .collect::<Vec<_>>();
        let right = result
            .report
            .training
            .projection
            .right_singular_vector_f32_bits
            .iter()
            .copied()
            .map(f32::from_bits)
            .collect::<Vec<_>>();
        let expected = scaled_weights_v1(
            &expanded_rank1_direction_v1(&left, &right).unwrap(),
            result.report.training.calibration.final_scale,
        )
        .unwrap();
        assert_eq!(expected.f32le_bytes_v1(), result.weights.f32le_bytes_v1());
    }
}
