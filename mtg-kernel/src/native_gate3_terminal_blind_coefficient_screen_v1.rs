//! Test-only terminal-blind coefficient-screen evaluator.
//!
//! This module owns no training path, Store schema, or promotion decision. It
//! builds one parent-policy decision corpus, removes terminal-bearing fields,
//! scores that corpus against immutable checkpoint handles, and emits only
//! diagnostic coefficient-screen data.

use crate::async_flat_scored_rollout_v2::run_async_flat_scored_rollout_native_environment_randomization_v2;
use crate::async_rollout_v2::AsyncRolloutConfigV2;
use crate::native_checkpoint_inference_v1::load_native_checkpoint_inference_v1;
use crate::native_full_episode_trajectory_v2::preflight_native_environment_window_v2;
use crate::native_ladder_opponent_v1::LadderOpponentEngineV1;
use crate::native_ladder_pool_resolution_v1::resolve_ladder_pool_v1;
use crate::native_policy_value_net_v1::NativeNamedParameterV1;
use crate::native_train_state_payload_v1::decode_native_train_state_payload_v1;
use crate::native_training_store_digest_v1::lower_hex_raw32_v1;
use crate::native_training_store_resume_v2::load_native_training_boundary_v2;
use crate::native_training_store_root_v2::ValidatedNativeTrainingStoreRootV2;
use crate::native_training_store_run_v2::{
    decode_train_run_v2, OpponentLadderPoolContractV1, ValidatedTrainRunV2,
};
use crate::private_physical_trajectory_core::FlatGroupedTrajectoryBatchCore;
use crate::private_physical_trajectory_v2::{
    FlatOwnedScoringInputsV2, NativeFlatPhysicalTrajectoryObserverV2,
};
use crate::rl::PlayerSeatV1;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{self, OpenOptions};
use std::hash::{Hash, Hasher};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

const REQUEST_SCHEMA_V1: &str = "regularized-continuation-terminal-blind-request/v1";
const REPORT_SCHEMA_V1: &str = "regularized-continuation-terminal-blind-report/v1";
const PARENT_GENERATION_V1: u64 = 384;
const EVALUATION_SEED_V1: u64 = 1_941_001;
const PAIR_COUNT_V1: u64 = 512;
const EPISODE_COUNT_V1: u64 = PAIR_COUNT_V1 * 2;
const SCREEN_GENERATIONS_V1: [u64; 5] = [0, 8, 16, 24, 32];
const SCREEN_BETAS_V1: [f64; 5] = [0.0, 0.01, 0.03, 0.1, 0.3];

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ScreenRequestV1 {
    schema: String,
    parent: CheckpointRequestV1,
    pool_json_path: PathBuf,
    evaluation_base_seed: u64,
    pair_count: u64,
    arms: Vec<BetaArmRequestV1>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CheckpointRequestV1 {
    store_root: PathBuf,
    generation: u64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct BetaArmRequestV1 {
    beta: f64,
    store_root: PathBuf,
    generations: Vec<u64>,
}

#[derive(Debug, Serialize)]
struct ScreenOutputV1 {
    schema: &'static str,
    terminal_outcomes_read: bool,
    corpus: CorpusReportV1,
    arms: Vec<ArmReportV1>,
}

#[derive(Debug, Serialize)]
struct CorpusReportV1 {
    evaluation_base_seed: u64,
    pair_count: u64,
    episode_count: u64,
    all_natural: bool,
    sha256: String,
    inventory: InventoryReportV1,
    parent_identity: CheckpointIdentityReportV1,
}

#[derive(Debug, Serialize, Clone, Copy)]
struct InventoryReportV1 {
    episode_count: u64,
    physical_group_count: u64,
    substep_count: u64,
    row_count: u64,
    action_count: u64,
}

#[derive(Debug, Serialize)]
struct ArmReportV1 {
    beta: f64,
    store_root: String,
    complete: bool,
    finite: bool,
    checkpoints: Vec<GenerationReportV1>,
}

#[derive(Debug, Serialize)]
struct GenerationReportV1 {
    generation: u64,
    identity: CheckpointIdentityReportV1,
    parameter_l2_from_parent: f64,
    overall: MetricReportV1,
    by_learner_seat: [SeatMetricReportV1; 2],
}

#[derive(Debug, Serialize)]
struct SeatMetricReportV1 {
    learner_seat: &'static str,
    metrics: MetricReportV1,
}

#[derive(Debug, Serialize)]
struct CheckpointIdentityReportV1 {
    run_sha256: String,
    identity_bundle_sha256: String,
    checkpoint_manifest_sha256: String,
    checkpoint_payload_sha256: String,
    logical_state_sha256: String,
    model_parameter_sha256: String,
    train_state_sha256: String,
}

#[derive(Debug, Serialize, Clone)]
struct MetricReportV1 {
    finite: bool,
    episode_count: u64,
    physical_group_count: u64,
    row_count: u64,
    choice_row_count: u64,
    singleton_row_count: u64,
    action_count: u64,
    mean_forward_kl: f64,
    mean_row_tv: f64,
    p90_row_tv: f64,
    p99_row_tv: f64,
    mean_choice_entropy: f64,
    mean_choice_max_action_probability: f64,
    maximum_absolute_selected_group_log_ratio: f64,
}

#[derive(Debug)]
struct BlindCorpusV1 {
    episodes: Vec<BlindEpisodeV1>,
    report: CorpusReportV1,
}

#[derive(Debug)]
struct BlindEpisodeV1 {
    episode_id: u64,
    learner_seat: PlayerSeatV1,
    groups: Vec<BlindGroupV1>,
}

#[derive(Debug)]
struct BlindGroupV1 {
    physical_decision_id: u64,
    substeps: Vec<BlindSubstepV1>,
}

#[derive(Debug)]
struct BlindSubstepV1 {
    selected_index: usize,
    parent_logits: Vec<f32>,
    scoring_inputs: FlatOwnedScoringInputsV2,
}

#[derive(Debug, Default)]
struct MetricAccumulatorV1 {
    finite: bool,
    episodes: u64,
    kls: Vec<f64>,
    tvs: Vec<f64>,
    entropies: Vec<f64>,
    max_probabilities: Vec<f64>,
    max_group_abs_log_ratio: f64,
    rows: u64,
    actions: u64,
    singleton_rows: u64,
    groups: u64,
    substeps: u64,
}

impl MetricAccumulatorV1 {
    fn new() -> Self {
        Self {
            finite: true,
            ..Self::default()
        }
    }
}

#[derive(Debug)]
struct LoadedCheckpointV1 {
    run: ValidatedTrainRunV2,
    checkpoint: crate::native_training_store_checkpoint_v3::CheckpointManifestV3,
    payload: Vec<u8>,
}

#[derive(Debug)]
struct ScreenErrorV1(String);

impl ScreenErrorV1 {
    fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

fn read_validated_run_v1(
    root: &Path,
) -> Result<(ValidatedNativeTrainingStoreRootV2, ValidatedTrainRunV2), ScreenErrorV1> {
    let store = ValidatedNativeTrainingStoreRootV2::open_v2(root)
        .map_err(|error| ScreenErrorV1::new(format!("store root open failed: {error}")))?;
    let run_bytes = fs::read(root.join("run.json"))
        .map_err(|error| ScreenErrorV1::new(format!("run.json read failed: {error}")))?;
    let run = decode_train_run_v2(&run_bytes)
        .map_err(|error| ScreenErrorV1::new(format!("run.json validation failed: {error}")))?;
    Ok((store, run))
}

fn load_checkpoint_v1(root: &Path, generation: u64) -> Result<LoadedCheckpointV1, ScreenErrorV1> {
    let (store, run) = read_validated_run_v1(root)?;
    let boundary = load_native_training_boundary_v2(&store, &run, generation).map_err(|error| {
        ScreenErrorV1::new(format!("generation {generation} load failed: {error}"))
    })?;
    let (checkpoint, payload) = boundary.into_checkpoint_and_payload();
    if checkpoint.generation_index() != generation {
        return Err(ScreenErrorV1::new(
            "checkpoint generation identity mismatch",
        ));
    }
    if checkpoint.run_sha256() != run.run_sha256()
        || checkpoint.identity_bundle_sha256() != run.identity_bundle_sha256()
    {
        return Err(ScreenErrorV1::new("checkpoint run identity mismatch"));
    }
    Ok(LoadedCheckpointV1 {
        run,
        checkpoint,
        payload,
    })
}

fn seat_index_v1(seat: PlayerSeatV1) -> usize {
    match seat {
        PlayerSeatV1::P0 => 0,
        PlayerSeatV1::P1 => 1,
    }
}

struct Sha256FrameHasher<'a> {
    digest: &'a mut Sha256,
}

impl Hasher for Sha256FrameHasher<'_> {
    fn finish(&self) -> u64 {
        0
    }

    fn write(&mut self, bytes: &[u8]) {
        self.digest.update([0x42]);
        self.digest.update((bytes.len() as u64).to_le_bytes());
        self.digest.update(bytes);
    }

    fn write_u8(&mut self, value: u8) {
        self.digest.update([0x08, value]);
    }

    fn write_u16(&mut self, value: u16) {
        self.digest.update([0x10]);
        self.digest.update(value.to_le_bytes());
    }

    fn write_u32(&mut self, value: u32) {
        self.digest.update([0x20]);
        self.digest.update(value.to_le_bytes());
    }

    fn write_u64(&mut self, value: u64) {
        self.digest.update([0x40]);
        self.digest.update(value.to_le_bytes());
    }

    fn write_usize(&mut self, value: usize) {
        self.digest.update([0x41]);
        self.digest.update((value as u64).to_le_bytes());
    }

    fn write_i8(&mut self, value: i8) {
        self.digest.update([0x18, value as u8]);
    }

    fn write_i16(&mut self, value: i16) {
        self.digest.update([0x11]);
        self.digest.update(value.to_le_bytes());
    }

    fn write_i32(&mut self, value: i32) {
        self.digest.update([0x21]);
        self.digest.update(value.to_le_bytes());
    }

    fn write_i64(&mut self, value: i64) {
        self.digest.update([0x41]);
        self.digest.update(value.to_le_bytes());
    }

    fn write_isize(&mut self, value: isize) {
        self.digest.update([0x42]);
        self.digest.update((value as i64).to_le_bytes());
    }
}

fn hash_scoring_inputs_v1(digest: &mut Sha256, inputs: &FlatOwnedScoringInputsV2) {
    let mut frame = Sha256FrameHasher { digest };
    inputs.hash(&mut frame);
}

fn seat_inventory_v1() -> [InventoryReportV1; 2] {
    [InventoryReportV1 {
        episode_count: 0,
        physical_group_count: 0,
        substep_count: 0,
        row_count: 0,
        action_count: 0,
    }; 2]
}

fn strip_terminal_fields_v1(
    batch: FlatGroupedTrajectoryBatchCore<
        crate::flat_policy_v2::FlatDecisionBindingV2,
        FlatOwnedScoringInputsV2,
    >,
    rollout_all_natural: bool,
) -> Result<BlindCorpusV1, ScreenErrorV1> {
    if !rollout_all_natural || batch.episodes.len() != EPISODE_COUNT_V1 as usize {
        return Err(ScreenErrorV1::new(
            "corpus is incomplete or not all natural",
        ));
    }
    let mut inventory = InventoryReportV1 {
        episode_count: 0,
        physical_group_count: 0,
        substep_count: 0,
        row_count: 0,
        action_count: 0,
    };
    let mut per_seat = seat_inventory_v1();
    let mut episodes = Vec::with_capacity(batch.episodes.len());
    let mut hasher = Sha256::new();
    hasher.update(b"regularized-continuation/gate3-terminal-blind-corpus/v2\0");
    hasher.update(crate::flat_policy_v2::FLAT_POLICY_CONTRACT_DIGESTS_V2.mapping_sha256);
    hasher.update(crate::flat_policy_v2::FLAT_POLICY_CONTRACT_DIGESTS_V2.feature_inventory_sha256);
    hasher.update(crate::flat_policy_v2::FLAT_POLICY_CONTRACT_DIGESTS_V2.typed_layout_sha256);
    for episode in batch.episodes {
        let seat = seat_index_v1(episode.learner_seat);
        let episode_id = episode.episode_id;
        let mut blind_groups = Vec::with_capacity(episode.groups.len());
        inventory.episode_count += 1;
        inventory.physical_group_count += episode.groups.len() as u64;
        per_seat[seat].episode_count += 1;
        per_seat[seat].physical_group_count += episode.groups.len() as u64;
        hasher.update(episode_id.to_le_bytes());
        hasher.update([seat as u8]);
        for group in episode.groups {
            let mut blind_substeps = Vec::with_capacity(group.substeps.len());
            hasher.update(group.physical_decision_id.to_le_bytes());
            for substep in group.substeps {
                let selected_index = usize::try_from(substep.selected_index)
                    .map_err(|_| ScreenErrorV1::new("selected index overflow"))?;
                let parent_logits: Vec<f32> = substep
                    .raw_action_logit_bits
                    .iter()
                    .map(|bits| f32::from_bits(*bits))
                    .collect();
                if parent_logits.len() < 1
                    || selected_index >= parent_logits.len()
                    || parent_logits.iter().any(|value| !value.is_finite())
                {
                    return Err(ScreenErrorV1::new(
                        "nonfinite or invalid parent decision row",
                    ));
                }
                inventory.substep_count += 1;
                inventory.row_count += 1;
                inventory.action_count += parent_logits.len() as u64;
                per_seat[seat].substep_count += 1;
                per_seat[seat].row_count += 1;
                per_seat[seat].action_count += parent_logits.len() as u64;
                hasher.update((parent_logits.len() as u64).to_le_bytes());
                hasher.update((selected_index as u64).to_le_bytes());
                for value in &parent_logits {
                    hasher.update(value.to_bits().to_le_bytes());
                }
                hash_scoring_inputs_v1(&mut hasher, &substep.scoring_inputs);
                blind_substeps.push(BlindSubstepV1 {
                    selected_index,
                    parent_logits,
                    scoring_inputs: substep.scoring_inputs,
                });
            }
            blind_groups.push(BlindGroupV1 {
                physical_decision_id: group.physical_decision_id,
                substeps: blind_substeps,
            });
        }
        episodes.push(BlindEpisodeV1 {
            episode_id,
            learner_seat: episode.learner_seat,
            groups: blind_groups,
        });
    }
    if inventory.episode_count != EPISODE_COUNT_V1
        || per_seat[0].episode_count != PAIR_COUNT_V1
        || per_seat[1].episode_count != PAIR_COUNT_V1
    {
        return Err(ScreenErrorV1::new("seat-swapped corpus inventory mismatch"));
    }
    let report = CorpusReportV1 {
        evaluation_base_seed: EVALUATION_SEED_V1,
        pair_count: PAIR_COUNT_V1,
        episode_count: EPISODE_COUNT_V1,
        all_natural: true,
        sha256: lower_hex_raw32_v1(hasher.finalize().into()),
        inventory,
        parent_identity: CheckpointIdentityReportV1 {
            run_sha256: String::new(),
            identity_bundle_sha256: String::new(),
            checkpoint_manifest_sha256: String::new(),
            checkpoint_payload_sha256: String::new(),
            logical_state_sha256: String::new(),
            model_parameter_sha256: String::new(),
            train_state_sha256: String::new(),
        },
    };
    Ok(BlindCorpusV1 { episodes, report })
}

fn stable_log_softmax_v1(logits: &[f64]) -> Result<Vec<f64>, ScreenErrorV1> {
    if logits.is_empty() || logits.iter().any(|value| !value.is_finite()) {
        return Err(ScreenErrorV1::new("log-softmax input is not finite"));
    }
    let maximum = logits.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let normalizer = logits
        .iter()
        .map(|value| (*value - maximum).exp())
        .sum::<f64>();
    if !normalizer.is_finite() || normalizer <= 0.0 {
        return Err(ScreenErrorV1::new("log-softmax normalizer is invalid"));
    }
    let log_normalizer = normalizer.ln();
    Ok(logits
        .iter()
        .map(|value| *value - maximum - log_normalizer)
        .collect())
}

fn row_metrics_v1(
    parent: &[f32],
    candidate: &[f32],
) -> Result<(f64, f64, Option<(f64, f64)>), ScreenErrorV1> {
    if parent.len() != candidate.len() || candidate.is_empty() {
        return Err(ScreenErrorV1::new("candidate row shape mismatch"));
    }
    let parent: Vec<f64> = parent.iter().map(|value| f64::from(*value)).collect();
    let candidate: Vec<f64> = candidate.iter().map(|value| f64::from(*value)).collect();
    let parent_log = stable_log_softmax_v1(&parent)?;
    let candidate_log = stable_log_softmax_v1(&candidate)?;
    let mut kl = 0.0;
    let mut tv = 0.0;
    let mut entropy = 0.0;
    let mut max_probability: f64 = 0.0;
    for (parent_log, candidate_log) in parent_log.iter().zip(&candidate_log) {
        let parent_probability = parent_log.exp();
        let candidate_probability = candidate_log.exp();
        kl += parent_probability * (parent_log - candidate_log);
        tv += (parent_probability - candidate_probability).abs();
        entropy -= candidate_probability * candidate_log;
        max_probability = max_probability.max(candidate_probability);
    }
    let choice = if parent.len() >= 2 {
        Some((entropy, max_probability))
    } else {
        None
    };
    if !kl.is_finite()
        || !tv.is_finite()
        || choice.is_some_and(|(e, p)| !e.is_finite() || !p.is_finite())
    {
        return Err(ScreenErrorV1::new("nonfinite row metric"));
    }
    Ok((kl, 0.5 * tv, choice))
}

fn nearest_rank_v1(values: &[f64], quantile: f64) -> Result<f64, ScreenErrorV1> {
    if values.is_empty() || !quantile.is_finite() || !(0.0 < quantile && quantile <= 1.0) {
        return Err(ScreenErrorV1::new("percentile input is invalid"));
    }
    let mut sorted = values.to_vec();
    if sorted.iter().any(|value| !value.is_finite()) {
        return Err(ScreenErrorV1::new("percentile input is nonfinite"));
    }
    sorted.sort_by(f64::total_cmp);
    let rank = ((sorted.len() as f64) * quantile).ceil() as usize;
    Ok(sorted[rank.max(1).min(sorted.len()) - 1])
}

fn selected_group_abs_log_ratio_v1(rows: &[(&[f32], &[f32], usize)]) -> Result<f64, ScreenErrorV1> {
    let mut sum = 0.0;
    for (parent, candidate, selected_index) in rows {
        if *selected_index >= parent.len() || parent.len() != candidate.len() {
            return Err(ScreenErrorV1::new("selected group row shape mismatch"));
        }
        let parent_logits = parent
            .iter()
            .map(|value| f64::from(*value))
            .collect::<Vec<_>>();
        let candidate_logits = candidate
            .iter()
            .map(|value| f64::from(*value))
            .collect::<Vec<_>>();
        let parent_log = stable_log_softmax_v1(&parent_logits)?;
        let candidate_log = stable_log_softmax_v1(&candidate_logits)?;
        sum += candidate_log[*selected_index] - parent_log[*selected_index];
    }
    if !sum.is_finite() {
        return Err(ScreenErrorV1::new("selected group log ratio is nonfinite"));
    }
    Ok(sum.abs())
}

fn parameter_l2_v1(
    parent: &[NativeNamedParameterV1],
    candidate: &[NativeNamedParameterV1],
) -> Result<f64, ScreenErrorV1> {
    if parent.len() != candidate.len() {
        return Err(ScreenErrorV1::new("parameter tensor count mismatch"));
    }
    let mut squared = 0.0;
    for (parent, candidate) in parent.iter().zip(candidate) {
        if parent.name != candidate.name
            || parent.shape != candidate.shape
            || parent.values.len() != candidate.values.len()
        {
            return Err(ScreenErrorV1::new("parameter manifest mismatch"));
        }
        for (parent, candidate) in parent.values.iter().zip(&candidate.values) {
            if !parent.is_finite() || !candidate.is_finite() {
                return Err(ScreenErrorV1::new("nonfinite parameter"));
            }
            squared += f64::from(*parent - *candidate).powi(2);
        }
    }
    let l2 = squared.sqrt();
    if !l2.is_finite() {
        Err(ScreenErrorV1::new("parameter L2 is nonfinite"))
    } else {
        Ok(l2)
    }
}

fn accumulator_report_v1(
    accumulator: MetricAccumulatorV1,
) -> Result<MetricReportV1, ScreenErrorV1> {
    let mean = |values: &[f64]| values.iter().sum::<f64>() / values.len() as f64;
    if accumulator.kls.is_empty() || accumulator.tvs.is_empty() {
        return Err(ScreenErrorV1::new("empty metric accumulator"));
    }
    let mean_entropy = if accumulator.entropies.is_empty() {
        0.0
    } else {
        mean(&accumulator.entropies)
    };
    let mean_probability = if accumulator.max_probabilities.is_empty() {
        0.0
    } else {
        mean(&accumulator.max_probabilities)
    };
    Ok(MetricReportV1 {
        finite: accumulator.finite,
        episode_count: accumulator.episodes,
        physical_group_count: accumulator.groups,
        row_count: accumulator.rows,
        choice_row_count: accumulator.entropies.len() as u64,
        singleton_row_count: accumulator.singleton_rows,
        action_count: accumulator.actions,
        mean_forward_kl: mean(&accumulator.kls),
        mean_row_tv: mean(&accumulator.tvs),
        p90_row_tv: nearest_rank_v1(&accumulator.tvs, 0.90)?,
        p99_row_tv: nearest_rank_v1(&accumulator.tvs, 0.99)?,
        mean_choice_entropy: mean_entropy,
        mean_choice_max_action_probability: mean_probability,
        maximum_absolute_selected_group_log_ratio: accumulator.max_group_abs_log_ratio,
    })
}

fn score_generation_v1(
    corpus: &BlindCorpusV1,
    inference: &crate::native_checkpoint_inference_v1::NativeCheckpointInferenceV1,
    _checkpoint: &crate::native_training_store_checkpoint_v3::CheckpointManifestV3,
    parent_parameters: &[NativeNamedParameterV1],
    candidate_parameters: &[NativeNamedParameterV1],
) -> Result<(MetricReportV1, [MetricReportV1; 2], f64), ScreenErrorV1> {
    let parameter_l2 = parameter_l2_v1(parent_parameters, candidate_parameters)?;
    let mut accumulators = [MetricAccumulatorV1::new(), MetricAccumulatorV1::new()];
    let mut overall = MetricAccumulatorV1::new();
    for episode in &corpus.episodes {
        let seat = seat_index_v1(episode.learner_seat);
        let mut seat_group_max: f64 = 0.0;
        accumulators[seat].episodes += 1;
        overall.episodes += 1;
        for group in &episode.groups {
            let mut group_rows: Vec<(Vec<f32>, Vec<f32>, usize)> =
                Vec::with_capacity(group.substeps.len());
            for substep in &group.substeps {
                let output = inference
                    .score_decision_v1(substep.scoring_inputs.scoring_view_v2())
                    .map_err(|error| {
                        ScreenErrorV1::new(format!("candidate scoring failed: {error}"))
                    })?;
                let (kl, tv, choice) =
                    row_metrics_v1(&substep.parent_logits, output.action_logits())?;
                group_rows.push((
                    substep.parent_logits.clone(),
                    output.action_logits().to_vec(),
                    substep.selected_index,
                ));
                let count = substep.parent_logits.len() as u64;
                for accumulator in [&mut accumulators[seat], &mut overall] {
                    accumulator.kls.push(kl);
                    accumulator.tvs.push(tv);
                    accumulator.rows += 1;
                    accumulator.actions += count;
                    accumulator.substeps += 1;
                    if let Some((entropy, max_probability)) = choice {
                        accumulator.entropies.push(entropy);
                        accumulator.max_probabilities.push(max_probability);
                    } else {
                        accumulator.singleton_rows += 1;
                    }
                }
            }
            let group_refs = group_rows
                .iter()
                .map(|(parent, candidate, selected_index)| {
                    (parent.as_slice(), candidate.as_slice(), *selected_index)
                })
                .collect::<Vec<_>>();
            let group_value = selected_group_abs_log_ratio_v1(&group_refs)?;
            seat_group_max = seat_group_max.max(group_value);
            accumulators[seat].groups += 1;
            overall.groups += 1;
        }
        accumulators[seat].max_group_abs_log_ratio = accumulators[seat]
            .max_group_abs_log_ratio
            .max(seat_group_max);
        overall.max_group_abs_log_ratio = overall.max_group_abs_log_ratio.max(seat_group_max);
    }
    let overall_report = accumulator_report_v1(overall)?;
    let mut reports = [
        accumulator_report_v1(std::mem::take(&mut accumulators[0]))?,
        accumulator_report_v1(std::mem::take(&mut accumulators[1]))?,
    ];
    reports[0].finite = reports[0].finite && overall_report.finite;
    reports[1].finite = reports[1].finite && overall_report.finite;
    Ok((overall_report, reports, parameter_l2))
}

fn compatible_model_runtime_v1(
    parent: &ValidatedTrainRunV2,
    candidate: &ValidatedTrainRunV2,
) -> bool {
    let parent_record = parent.record();
    let candidate_record = candidate.record();
    parent_record.environment().deck_ids() == candidate_record.environment().deck_ids()
        && parent_record.environment().deck_hashes_u64_hex()
            == candidate_record.environment().deck_hashes_u64_hex()
        && parent_record.limits().max_physical_decisions()
            == candidate_record.limits().max_physical_decisions()
        && parent_record.limits().max_policy_steps() == candidate_record.limits().max_policy_steps()
        && parent_record.topology().worker_count() == candidate_record.topology().worker_count()
        && parent_record.topology().sessions_per_worker()
            == candidate_record.topology().sessions_per_worker()
        && parent_record.topology().broker_batch_target()
            == candidate_record.topology().broker_batch_target()
}

fn validate_continuation_run_v1(
    parent: &ValidatedTrainRunV2,
    candidate: &ValidatedTrainRunV2,
    pool: &OpponentLadderPoolContractV1,
) -> Result<(), ScreenErrorV1> {
    if candidate.record().contracts().opponent_ladder_pool.as_ref() != Some(pool) {
        return Err(ScreenErrorV1::new("candidate and Pool3 identity mismatch"));
    }
    if candidate.environment_trajectory_contract_v1()
        != crate::native_training_store_run_v2::NativeRunEnvironmentTrajectoryContractV1::EnvironmentRandomizationV2
    {
        return Err(ScreenErrorV1::new("candidate Store is not envrand-v2"));
    }
    if !compatible_model_runtime_v1(parent, candidate) {
        return Err(ScreenErrorV1::new(
            "candidate and parent model-runtime identity mismatch",
        ));
    }
    Ok(())
}

fn build_ladder_v1(pool_directory: &Path) -> Result<Arc<LadderOpponentEngineV1>, ScreenErrorV1> {
    let pool_bytes = fs::read(pool_directory.join("pool.json"))
        .map_err(|error| ScreenErrorV1::new(format!("Pool3 read failed: {error}")))?;
    let pool: OpponentLadderPoolContractV1 = serde_json::from_slice(&pool_bytes)
        .map_err(|error| ScreenErrorV1::new(format!("Pool3 decode failed: {error}")))?;
    let (primary, predecessor_a, predecessor_b) = resolve_ladder_pool_v1(
        &pool,
        &pool_directory.join("primary"),
        &pool_directory.join("pred-a"),
        &pool_directory.join("pred-b"),
    )
    .map_err(|error| ScreenErrorV1::new(format!("Pool3 checkpoint load failed: {error}")))?;
    Ok(Arc::new(
        LadderOpponentEngineV1::new_v1(pool, primary, predecessor_a, predecessor_b).map_err(
            |error| ScreenErrorV1::new(format!("Pool3 engine construction failed: {error}")),
        )?,
    ))
}

fn build_corpus_v1(
    parent: &LoadedCheckpointV1,
    environment_run: &ValidatedTrainRunV2,
    ladder: Arc<LadderOpponentEngineV1>,
    evaluation_seed: u64,
) -> Result<BlindCorpusV1, ScreenErrorV1> {
    let record = environment_run.record();
    let worker_count = usize::try_from(record.topology().worker_count())
        .map_err(|_| ScreenErrorV1::new("worker count overflow"))?;
    let sessions_per_worker = usize::try_from(record.topology().sessions_per_worker())
        .map_err(|_| ScreenErrorV1::new("session count overflow"))?;
    let broker_batch_target = usize::try_from(record.topology().broker_batch_target())
        .map_err(|_| ScreenErrorV1::new("broker target overflow"))?;
    let deck_ids = record.environment().deck_ids().clone();
    let deck_hashes = [
        u64::from_str_radix(&record.environment().deck_hashes_u64_hex()[0], 16)
            .map_err(|_| ScreenErrorV1::new("invalid first deck hash"))?,
        u64::from_str_radix(&record.environment().deck_hashes_u64_hex()[1], 16)
            .map_err(|_| ScreenErrorV1::new("invalid second deck hash"))?,
    ];
    let authority = preflight_native_environment_window_v2(
        evaluation_seed,
        0,
        EPISODE_COUNT_V1,
        &deck_ids,
        deck_hashes,
    )
    .map_err(|error| {
        ScreenErrorV1::new(format!("corpus environment preflight failed: {error:?}"))
    })?;
    let config = AsyncRolloutConfigV2 {
        deck_ids,
        learner_seat: PlayerSeatV1::P0,
        environment_seed: evaluation_seed,
        opponent_policy_seed: evaluation_seed,
        learner_policy_seed: evaluation_seed,
        max_physical_decisions: record.limits().max_physical_decisions(),
        max_policy_steps: record.limits().max_policy_steps(),
        worker_count,
        sessions_per_worker,
        broker_batch_target,
        first_episode_id: 0,
        episode_count: EPISODE_COUNT_V1,
        scheduler_timeout: Duration::from_secs(86_400),
        measure_broker_service_time: false,
    };
    let inference =
        load_native_checkpoint_inference_v1(&parent.run, &parent.checkpoint, &parent.payload)
            .map_err(|error| {
                ScreenErrorV1::new(format!("parent inference load failed: {error}"))
            })?;
    let mut scorer = inference.batch_scorer_v1();
    let observer =
        NativeFlatPhysicalTrajectoryObserverV2::new(0, EPISODE_COUNT_V1).map_err(|error| {
            ScreenErrorV1::new(format!("corpus observer construction failed: {error:?}"))
        })?;
    let (rollout, batch) = run_async_flat_scored_rollout_native_environment_randomization_v2(
        config,
        evaluation_seed,
        authority,
        Some(ladder),
        &mut scorer,
        observer,
    )
    .map_err(|error| ScreenErrorV1::new(format!("corpus rollout failed: {error:?}")))?;
    strip_terminal_fields_v1(batch, rollout.all_natural())
}

fn snapshot_parameters_v1(
    checkpoint: &crate::native_training_store_checkpoint_v3::CheckpointManifestV3,
    payload: &[u8],
) -> Result<Vec<NativeNamedParameterV1>, ScreenErrorV1> {
    let anchor = u32::try_from(checkpoint.train_state().scorer_bias_anchor_f32_bits())
        .map_err(|_| ScreenErrorV1::new("scorer anchor overflow"))?;
    let decoded =
        decode_native_train_state_payload_v1(payload, checkpoint.generation_index(), anchor)
            .map_err(|error| ScreenErrorV1::new(format!("train-state decode failed: {error}")))?;
    Ok(decoded.snapshot.parameters)
}

fn checkpoint_identity_report_v1(
    run: &ValidatedTrainRunV2,
    checkpoint: &crate::native_training_store_checkpoint_v3::CheckpointManifestV3,
) -> CheckpointIdentityReportV1 {
    CheckpointIdentityReportV1 {
        run_sha256: run.run_sha256().to_owned(),
        identity_bundle_sha256: run.identity_bundle_sha256().to_owned(),
        checkpoint_manifest_sha256: lower_hex_raw32_v1(checkpoint.checkpoint_manifest_sha256()),
        checkpoint_payload_sha256: lower_hex_raw32_v1(checkpoint.checkpoint_payload_sha256()),
        logical_state_sha256: lower_hex_raw32_v1(checkpoint.logical_state_sha256()),
        model_parameter_sha256: lower_hex_raw32_v1(checkpoint.model_parameter_sha256()),
        train_state_sha256: lower_hex_raw32_v1(checkpoint.train_state_sha256()),
    }
}

fn run_screen_v1(request: ScreenRequestV1) -> Result<ScreenOutputV1, ScreenErrorV1> {
    if request.schema != REQUEST_SCHEMA_V1
        || request.evaluation_base_seed != EVALUATION_SEED_V1
        || request.pair_count != PAIR_COUNT_V1
        || request.parent.generation != PARENT_GENERATION_V1
        || request.arms.len() != SCREEN_BETAS_V1.len()
        || request.parent.store_root.is_relative()
        || request.pool_json_path.is_relative()
    {
        return Err(ScreenErrorV1::new(
            "frozen Gate 3 request identity mismatch",
        ));
    }
    for (arm, expected_beta) in request.arms.iter().zip(SCREEN_BETAS_V1) {
        if arm.beta.to_bits() != expected_beta.to_bits()
            || arm.store_root.is_relative()
            || arm.generations != SCREEN_GENERATIONS_V1
        {
            return Err(ScreenErrorV1::new("beta-arm request identity mismatch"));
        }
    }
    let parent = load_checkpoint_v1(&request.parent.store_root, PARENT_GENERATION_V1)?;
    let pool_bytes = fs::read(&request.pool_json_path)
        .map_err(|error| ScreenErrorV1::new(format!("Pool3 read failed: {error}")))?;
    let pool: OpponentLadderPoolContractV1 = serde_json::from_slice(&pool_bytes)
        .map_err(|error| ScreenErrorV1::new(format!("Pool3 decode failed: {error}")))?;
    let first_arm = request
        .arms
        .first()
        .ok_or_else(|| ScreenErrorV1::new("Gate 3 request has no continuation arm"))?;
    let (_, environment_run) = read_validated_run_v1(&first_arm.store_root)?;
    validate_continuation_run_v1(&parent.run, &environment_run, &pool)?;
    let pool_directory = request
        .pool_json_path
        .parent()
        .ok_or_else(|| ScreenErrorV1::new("Pool3 JSON parent is missing"))?;
    let ladder = build_ladder_v1(pool_directory)?;
    let parent_parameters = snapshot_parameters_v1(&parent.checkpoint, &parent.payload)?;
    let mut corpus = build_corpus_v1(
        &parent,
        &environment_run,
        ladder,
        request.evaluation_base_seed,
    )?;
    corpus.report.evaluation_base_seed = request.evaluation_base_seed;
    corpus.report.parent_identity = checkpoint_identity_report_v1(&parent.run, &parent.checkpoint);
    let mut arms = Vec::with_capacity(request.arms.len());
    for arm_request in request.arms {
        let (_, arm_run) = read_validated_run_v1(&arm_request.store_root)?;
        validate_continuation_run_v1(&parent.run, &arm_run, &pool)?;
        let mut checkpoints = Vec::with_capacity(arm_request.generations.len());
        for generation in arm_request.generations {
            let candidate = load_checkpoint_v1(&arm_request.store_root, generation)?;
            let inference = load_native_checkpoint_inference_v1(
                &candidate.run,
                &candidate.checkpoint,
                &candidate.payload,
            )
            .map_err(|error| {
                ScreenErrorV1::new(format!("candidate inference load failed: {error}"))
            })?;
            let candidate_parameters =
                snapshot_parameters_v1(&candidate.checkpoint, &candidate.payload)?;
            let (overall, per_seat, parameter_l2) = score_generation_v1(
                &corpus,
                &inference,
                &candidate.checkpoint,
                &parent_parameters,
                &candidate_parameters,
            )?;
            checkpoints.push(GenerationReportV1 {
                generation,
                identity: checkpoint_identity_report_v1(&candidate.run, &candidate.checkpoint),
                parameter_l2_from_parent: parameter_l2,
                overall,
                by_learner_seat: [
                    SeatMetricReportV1 {
                        learner_seat: "P0",
                        metrics: per_seat[0].clone(),
                    },
                    SeatMetricReportV1 {
                        learner_seat: "P1",
                        metrics: per_seat[1].clone(),
                    },
                ],
            });
        }
        arms.push(ArmReportV1 {
            beta: arm_request.beta,
            store_root: arm_request.store_root.to_string_lossy().into_owned(),
            complete: checkpoints.len() == SCREEN_GENERATIONS_V1.len(),
            finite: true,
            checkpoints,
        });
    }
    Ok(ScreenOutputV1 {
        schema: REPORT_SCHEMA_V1,
        terminal_outcomes_read: false,
        corpus: corpus.report,
        arms,
    })
}

fn write_output_create_new_v1(path: &Path, bytes: &[u8]) -> Result<(), ScreenErrorV1> {
    if !path.is_absolute() {
        return Err(ScreenErrorV1::new("output path must be absolute"));
    }
    if path.exists() {
        return Err(ScreenErrorV1::new("output path already exists"));
    }
    let parent = path
        .parent()
        .ok_or_else(|| ScreenErrorV1::new("output parent is missing"))?;
    let temp = parent.join(format!(
        ".{}.gate3-{}.tmp",
        path.file_name().unwrap().to_string_lossy(),
        std::process::id()
    ));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)
            .map_err(|error| {
                ScreenErrorV1::new(format!("temporary output create failed: {error}"))
            })?;
        file.write_all(bytes).map_err(|error| {
            ScreenErrorV1::new(format!("temporary output write failed: {error}"))
        })?;
        file.sync_all().map_err(|error| {
            ScreenErrorV1::new(format!("temporary output sync failed: {error}"))
        })?;
        drop(file);
        fs::hard_link(&temp, path).map_err(|error| {
            ScreenErrorV1::new(format!("atomic create-new output publish failed: {error}"))
        })?;
        let _ = fs::remove_file(&temp);
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

#[test]
#[ignore = "Gate 3 GPU/Store integration; invoke explicitly with REGCONT_SCREEN_INPUT_JSON and REGCONT_SCREEN_OUTPUT_JSON"]
fn gate3_terminal_blind_coefficient_screen_v1() {
    let input_path = PathBuf::from(
        std::env::var_os("REGCONT_SCREEN_INPUT_JSON")
            .expect("REGCONT_SCREEN_INPUT_JSON is required"),
    );
    let output_path = PathBuf::from(
        std::env::var_os("REGCONT_SCREEN_OUTPUT_JSON")
            .expect("REGCONT_SCREEN_OUTPUT_JSON is required"),
    );
    let request_bytes = fs::read(&input_path).expect("Gate 3 request JSON must be readable");
    let request: ScreenRequestV1 =
        serde_json::from_slice(&request_bytes).expect("Gate 3 request JSON must validate");
    let output = run_screen_v1(request)
        .unwrap_or_else(|error| panic!("Gate 3 terminal-blind screen failed: {}", error.0));
    let output_bytes = serde_json::to_vec_pretty(&output).expect("Gate 3 output must serialize");
    write_output_create_new_v1(&output_path, &output_bytes)
        .unwrap_or_else(|error| panic!("Gate 3 output publish failed: {}", error.0));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_log_softmax_handles_large_logits() {
        let output = stable_log_softmax_v1(&[1000.0, 999.0]).unwrap();
        assert!((output[0].exp() + output[1].exp() - 1.0).abs() < 1e-12);
    }

    #[test]
    fn kl_and_tv_are_zero_for_identical_rows() {
        let (kl, tv, _) = row_metrics_v1(&[1.0, 2.0], &[1.0, 2.0]).unwrap();
        assert!(kl.abs() < 1e-15);
        assert!(tv.abs() < 1e-15);
    }

    #[test]
    fn known_forward_kl_and_tv_match_the_declared_direction() {
        let parent = [0.8_f32.ln(), 0.2_f32.ln()];
        let candidate = [0.6_f32.ln(), 0.4_f32.ln()];
        let (kl, tv, _) = row_metrics_v1(&parent, &candidate).unwrap();
        assert!((kl - 0.09151622184943575).abs() < 1e-7);
        assert!((tv - 0.2).abs() < 1e-7);
    }

    #[test]
    fn row_metrics_include_singletons_and_zero_choice_metrics() {
        let (kl, tv, choice) = row_metrics_v1(&[3.0], &[4.0]).unwrap();
        assert!(kl.abs() < 1e-15);
        assert!(tv.abs() < 1e-15);
        assert!(choice.is_none());
    }

    #[test]
    fn nearest_rank_is_deterministic() {
        assert_eq!(nearest_rank_v1(&[0.1, 0.4, 0.2, 0.3], 0.90).unwrap(), 0.4);
        assert_eq!(nearest_rank_v1(&[0.1, 0.4, 0.2, 0.3], 0.99).unwrap(), 0.4);
    }

    #[test]
    fn parameter_l2_is_euclidean_over_all_values() {
        let a = NativeNamedParameterV1 {
            name: "x",
            shape: vec![2],
            values: vec![0.0, 0.0],
        };
        let b = NativeNamedParameterV1 {
            name: "x",
            shape: vec![2],
            values: vec![3.0, 4.0],
        };
        assert_eq!(parameter_l2_v1(&[a], &[b]).unwrap(), 5.0);
    }

    #[test]
    fn selected_group_log_ratio_uses_candidate_minus_parent() {
        let parent_a = [0.0_f32, 0.0];
        let candidate_a = [3.0_f32.ln(), 0.0];
        let parent_b = [0.0_f32, 0.0];
        let candidate_b = [0.0, 3.0_f32.ln()];
        let group_sum = selected_group_abs_log_ratio_v1(&[
            (&parent_a, &candidate_a, 0),
            (&parent_b, &candidate_b, 1),
        ])
        .unwrap();
        let one_substep = selected_group_abs_log_ratio_v1(&[(&parent_a, &candidate_a, 0)]).unwrap();
        assert!((group_sum - 2.0 * 1.5_f64.ln()).abs() < 1e-6);
        assert!(group_sum > one_substep);
        assert!((group_sum.max(one_substep) - group_sum).abs() < 1e-15);
    }
}
