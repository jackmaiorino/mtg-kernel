//! G115 line (b) teach step and teacher packet (design entry sections 3 to
//! 5; FABLE-REVIEW-20260927 exit-teacher change 1).
//!
//! After the batch is collected and before the update, with the frozen
//! start-of-update student: for every teacher game (the caller's per-game
//! seeds; canonical games carry none) the root is selected by replay, the
//! K x n coupled rollouts run for all roots on one worker pool, and each
//! root's census status and frozen target are formed. The packet records
//! what the update and the receipts need; the update publishes it and binds
//! its SHA-256. A defect in any rollout fails the step.

use serde::{Deserialize, Serialize};

use super::line_b_root_selection_v1::select_line_b_root_v1;
use super::line_b_teacher_operator_v1::{
    line_b_root_status_v1, line_b_teacher_rollouts_v1, LineBRolloutOutcomeV1,
    LineBRolloutPoliciesV1, LineBRolloutRecordV1, LineBRootStatusV1, LineBTeacherRootInputV1,
    LINE_B_TEACHER_OPERATOR_VERSION_V1,
};
use super::PinnedFileV1;
use super::{
    hex, load_expanded_inference_v1, ExpandedInferenceIdentityV1, ExpandedModelSourceV1,
    ExpandedTrajectoryV1,
};
use crate::line_b_teacher_target_v1::{
    line_b_divergence_v1, line_b_teacher_target_v1, LineBDivergenceV1,
    LINE_B_TEACHER_TARGET_VERSION_V1,
};
use crate::native_policy_train_step_v1::{
    LineBAuxiliaryInputV1, LineBAuxiliaryResultV1, LineBAuxiliaryRootV1,
};
use crate::sideboard_play_policy_v1::FrozenPlayPolicyV1;
use serde_json::{json, Value};

pub const LINE_B_TEACHER_PACKET_SCHEMA_V1: &str = "mtg-kernel-line-b-teacher-packet/v1";
/// Resource bound on K, not a recipe value (the declared K is 16).
pub const LINE_B_TEACHER_MAX_ROLLOUTS_V1: u32 = 1024;

/// A teacher game's production (or engineering) seeds.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LineBGameSeedsV1 {
    pub root_seed: u64,
    pub teacher_seed: u64,
}

/// The treatment slot's teacher: parameters (never tuned) and per-game seeds.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LineBTeacherOptionsV1 {
    pub direction: LineBDivergenceV1,
    /// `c`.
    pub coefficient: f64,
    /// `T`.
    pub temperature: f64,
    /// `K`.
    pub rollouts: u32,
    pub workers: usize,
    /// One entry per trajectory in batch order; `None` for a canonical game.
    pub games: Vec<Option<LineBGameSeedsV1>>,
    /// Receipt-only permuted-target control (CODEX 11:23 item 6): the same
    /// update on a CPU shadow with permuted targets, never published.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub permuted_control: bool,
}

impl LineBTeacherOptionsV1 {
    pub(super) fn validate_v1(&self, trajectories: usize) -> Result<(), String> {
        if self.games.len() != trajectories {
            return Err("line (b) teacher needs one seed entry per trajectory".into());
        }
        if !(self.coefficient.is_finite() && self.coefficient >= 0.0) {
            return Err("line (b) teacher coefficient must be finite and nonnegative".into());
        }
        if !(self.temperature.is_finite() && self.temperature > 0.0) {
            return Err("line (b) teacher temperature must be finite and positive".into());
        }
        if !(1..=LINE_B_TEACHER_MAX_ROLLOUTS_V1).contains(&self.rollouts) {
            return Err("line (b) teacher rollouts outside 1..=1024".into());
        }
        if !(1..=256).contains(&self.workers) {
            return Err("line (b) teacher workers outside 1..=256".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub(super) struct LineBPacketRootV1 {
    pub(super) trajectory_index: usize,
    pub(super) episode_id: String,
    pub(super) decision_index: usize,
    pub(super) physical_decision_id: u64,
    pub(super) actor: u8,
    pub(super) rank: String,
    pub(super) visible_key: String,
    pub(super) eligible_decisions: usize,
    pub(super) seeds: LineBGameSeedsV1,
    /// The recorded collection logits (binary32 bits): `p_u` and the target's `z`.
    pub(super) collection_logits: Vec<u32>,
    pub(super) rollouts: Vec<LineBRolloutRecordV1>,
    pub(super) status: LineBRootStatusV1,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub(super) enum LineBPacketGameV1 {
    /// A canonical game: ordinary RL only.
    Canonical {
        trajectory_index: usize,
    },
    /// A teacher game without an eligible decision.
    NoEligibleRoot {
        trajectory_index: usize,
        episode_id: String,
    },
    Root(LineBPacketRootV1),
}

/// Denominators and counts (proposal 12:51: selected and complete visible).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub(super) struct LineBCensusV1 {
    pub(super) teacher_games: usize,
    pub(super) no_eligible_root_games: usize,
    /// N, the auxiliary's denominator.
    pub(super) selected_roots: usize,
    pub(super) complete_roots: usize,
    pub(super) censored_roots: usize,
    pub(super) rollouts: usize,
    pub(super) censored_rollouts: usize,
    pub(super) physical_decisions: u64,
    pub(super) policy_steps: u64,
}

/// The packet's recipe and seeds, without the execution settings (`workers`,
/// `permuted_control`): the packet is a function of the student, the batch,
/// the seeds and the recipe, so its bytes do not depend on the worker count.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub(super) struct LineBPacketOptionsV1 {
    pub(super) direction: LineBDivergenceV1,
    pub(super) coefficient: f64,
    pub(super) temperature: f64,
    pub(super) rollouts: u32,
    pub(super) games: Vec<Option<LineBGameSeedsV1>>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub(super) struct LineBTeacherPacketV1 {
    pub(super) schema: &'static str,
    pub(super) operator: &'static str,
    pub(super) target: &'static str,
    pub(super) options: LineBPacketOptionsV1,
    /// The start-of-update student the rollouts continued with.
    pub(super) student_state_sha256: String,
    pub(super) games: Vec<LineBPacketGameV1>,
    pub(super) census: LineBCensusV1,
}

/// The teach step. `student` is the start-of-update policy of the batch.
pub(super) fn line_b_teach_v1(
    trajectories: &[ExpandedTrajectoryV1],
    student: &FrozenPlayPolicyV1,
    student_state_sha256: &str,
    options: &LineBTeacherOptionsV1,
) -> Result<LineBTeacherPacketV1, String> {
    options.validate_v1(trajectories.len())?;
    // Each distinct frozen opponent of a teacher game, loaded once; every
    // game checks it against the identity its collection recorded.
    let mut sources: Vec<ExpandedModelSourceV1> = Vec::new();
    let mut opponents: Vec<(FrozenPlayPolicyV1, ExpandedInferenceIdentityV1)> = Vec::new();
    for (trajectory, seeds) in trajectories.iter().zip(&options.games) {
        let (Some(_), Some(source)) = (seeds, &trajectory.episode.opponent) else {
            continue;
        };
        let position = match sources.iter().position(|known| known == source) {
            Some(position) => position,
            None => {
                opponents.push(load_expanded_inference_v1(source)?);
                sources.push(source.clone());
                sources.len() - 1
            }
        };
        let recorded = trajectory
            .seat_behaviors
            .as_ref()
            .map(|seats| &seats[1 - trajectory.episode.learner_seat as usize].identity);
        if recorded != Some(&opponents[position].1) {
            return Err("line (b) teacher opponent differs from the collection's".into());
        }
    }
    let mut roots = Vec::new();
    for (index, (trajectory, seeds)) in trajectories.iter().zip(&options.games).enumerate() {
        if let Some(seeds) = seeds {
            if let Some(root) = select_line_b_root_v1(trajectory, seeds.root_seed)? {
                roots.push((index, root, seeds.clone()));
            }
        }
    }
    let mut inputs: Vec<LineBTeacherRootInputV1<'_>> = Vec::with_capacity(roots.len());
    for (index, root, seeds) in &roots {
        let trajectory = &trajectories[*index];
        let opponent = match &trajectory.episode.opponent {
            None => None,
            Some(source) => {
                let position = sources
                    .iter()
                    .position(|known| known == source)
                    .ok_or("line (b) teacher opponent was not loaded")?;
                Some(&opponents[position].0)
            }
        };
        inputs.push(LineBTeacherRootInputV1 {
            root,
            teacher_seed: seeds.teacher_seed,
            learner_sampler: trajectory.learner_sampler.as_deref(),
            policies: LineBRolloutPoliciesV1 {
                student,
                opponent,
                learner_seat: trajectory.episode.learner_seat,
            },
        });
    }
    let records = line_b_teacher_rollouts_v1(&inputs, options.rollouts, options.workers)?;
    let mut census = LineBCensusV1 {
        selected_roots: roots.len(),
        ..LineBCensusV1::default()
    };
    let mut packets = roots.into_iter().zip(records).peekable();
    let mut games = Vec::with_capacity(trajectories.len());
    for (index, (trajectory, seeds)) in trajectories.iter().zip(&options.games).enumerate() {
        if seeds.is_none() {
            games.push(LineBPacketGameV1::Canonical {
                trajectory_index: index,
            });
            continue;
        }
        census.teacher_games += 1;
        if packets.peek().map(|((root_index, _, _), _)| *root_index) != Some(index) {
            census.no_eligible_root_games += 1;
            games.push(LineBPacketGameV1::NoEligibleRoot {
                trajectory_index: index,
                episode_id: trajectory.episode.id.clone(),
            });
            continue;
        }
        let ((_, root, seeds), rollouts) = packets.next().ok_or("line (b) teacher root missing")?;
        let collection_logits = trajectory.decisions[root.decision_index].logits.clone();
        let logits: Vec<f32> = collection_logits
            .iter()
            .map(|bits| f32::from_bits(*bits))
            .collect();
        let status =
            line_b_root_status_v1(&rollouts, options.rollouts, &logits, options.temperature)?;
        match &status {
            LineBRootStatusV1::Complete { .. } => census.complete_roots += 1,
            LineBRootStatusV1::Censored { .. } => census.censored_roots += 1,
        }
        for record in &rollouts {
            census.rollouts += 1;
            census.physical_decisions += record.physical_decisions;
            census.policy_steps += record.policy_steps;
            if matches!(record.outcome, LineBRolloutOutcomeV1::Censored(_)) {
                census.censored_rollouts += 1;
            }
        }
        games.push(LineBPacketGameV1::Root(LineBPacketRootV1 {
            trajectory_index: index,
            episode_id: trajectory.episode.id.clone(),
            decision_index: root.decision_index,
            physical_decision_id: root.physical_decision_id,
            actor: root.actor,
            rank: hex(&root.rank),
            visible_key: hex(&root.visible_key),
            eligible_decisions: root.eligible_decisions,
            seeds,
            collection_logits,
            rollouts,
            status,
        }));
    }
    Ok(LineBTeacherPacketV1 {
        schema: LINE_B_TEACHER_PACKET_SCHEMA_V1,
        operator: LINE_B_TEACHER_OPERATOR_VERSION_V1,
        target: LINE_B_TEACHER_TARGET_VERSION_V1,
        options: LineBPacketOptionsV1 {
            direction: options.direction,
            coefficient: options.coefficient,
            temperature: options.temperature,
            rollouts: options.rollouts,
            games: options.games.clone(),
        },
        student_state_sha256: student_state_sha256.into(),
        games,
        census,
    })
}

/// The update's auxiliary input: each root located among the prepared
/// learner groups (its trajectory's group whose first row is the root, at
/// substep 0), with its frozen target rebuilt from the packet and checked
/// bit for bit against the packet's `log_target`; censored roots stay in N.
/// `None` when the batch has no selected root (the update is then ordinary).
pub(super) fn line_b_auxiliary_input_v1(
    packet: &LineBTeacherPacketV1,
    groups: &[super::LearnerTensorGroupV1<'_>],
) -> Result<Option<LineBAuxiliaryInputV1>, String> {
    let mut roots = Vec::new();
    for game in &packet.games {
        let LineBPacketGameV1::Root(root) = game else {
            continue;
        };
        let mut matches = groups
            .iter()
            .enumerate()
            .filter(|(_, (_, ordinal, rows))| {
                *ordinal == root.trajectory_index
                    && rows
                        .first()
                        .is_some_and(|(row, _)| row.step == root.decision_index as u64)
            })
            .map(|(index, _)| index);
        let group_index = matches
            .next()
            .ok_or("line (b) root is not a learner decision of the batch")?;
        if matches.next().is_some() {
            return Err("line (b) root matches two learner groups".into());
        }
        let target = match &root.status {
            LineBRootStatusV1::Censored { .. } => None,
            LineBRootStatusV1::Complete {
                mean_returns,
                log_target,
            } => {
                let logits: Vec<f64> = root
                    .collection_logits
                    .iter()
                    .map(|bits| f64::from(f32::from_bits(*bits)))
                    .collect();
                let target =
                    line_b_teacher_target_v1(&logits, mean_returns, packet.options.temperature)
                        .map_err(|error| error.to_string())?;
                if target.log_probabilities.len() != log_target.len()
                    || target
                        .log_probabilities
                        .iter()
                        .zip(log_target)
                        .any(|(a, b)| a.to_bits() != b.to_bits())
                {
                    return Err("line (b) target differs from its packet".into());
                }
                Some(target)
            }
        };
        roots.push(LineBAuxiliaryRootV1 {
            group_index,
            substep_index: 0,
            target,
        });
    }
    if roots.is_empty() {
        return Ok(None);
    }
    Ok(Some(LineBAuxiliaryInputV1 {
        direction: packet.options.direction,
        coefficient: packet.options.coefficient,
        roots,
    }))
}

/// `result.line_b.teacher` of the update receipt (proposal 12:51 telemetry):
/// the published packet, the recipe values, the census with its
/// denominators, and per root the divergence before the update (null for a
/// censored root) plus the head-tensor gradient norms and their ratio (null
/// when the ordinary norm is zero). A batch without a selected root records
/// null telemetry.
pub(super) fn line_b_teacher_receipt_v1(
    packet: &LineBTeacherPacketV1,
    packet_pin: &PinnedFileV1,
    auxiliary: Option<&LineBAuxiliaryResultV1>,
    rescore: Option<&[Option<(f64, f64)>]>,
    teacher_seconds: f64,
) -> Value {
    let telemetry = auxiliary.map(|auxiliary| {
        let roots: Vec<Value> = packet
            .games
            .iter()
            .filter_map(|game| match game {
                LineBPacketGameV1::Root(root) => Some(root),
                _ => None,
            })
            .zip(&auxiliary.divergences_before)
            .enumerate()
            .map(|(index, (root, before))| {
                let pair = rescore.and_then(|pairs| pairs.get(index).copied().flatten());
                json!({"trajectory_index": root.trajectory_index,
                    "decision_index": root.decision_index,
                    "divergence_before": before,
                    "rescore_before": pair.map(|(before, _)| before),
                    "rescore_after": pair.map(|(_, after)| after),
                    "signed_difference": pair.map(|(before, after)| after - before)})
            })
            .collect();
        let ratio = if auxiliary.ordinary_head_l2 > 0.0 {
            json!(auxiliary.auxiliary_head_l2 / auxiliary.ordinary_head_l2)
        } else {
            Value::Null
        };
        json!({"roots": roots,
            "auxiliary_loss": auxiliary.auxiliary_loss,
            "auxiliary_head_l2": auxiliary.auxiliary_head_l2,
            "ordinary_head_l2": auxiliary.ordinary_head_l2,
            "head_l2_ratio": ratio,
            "auxiliary_bias_residual": auxiliary.auxiliary_bias_residual})
    });
    json!({"packet": packet_pin,
        "schema": packet.schema,
        "operator": packet.operator,
        "target": packet.target,
        "direction": packet.options.direction,
        "coefficient": packet.options.coefficient,
        "temperature": packet.options.temperature,
        "rollouts": packet.options.rollouts,
        "census": packet.census,
        "teacher_seconds": teacher_seconds,
        "telemetry": telemetry})
}

/// The proposal's 12:51 rescore on each selected root, same root, full menu
/// and frozen target for both readings: its divergence from the recorded
/// collection logits (the start-of-update forward, bit for bit) and from a
/// host forward of the published parameters, both in binary64 and the same
/// on either backend. Censored roots are unavailable (`None`); the rescore
/// never touches optimizer state.
pub(super) fn line_b_rescore_v1(
    input: &LineBAuxiliaryInputV1,
    groups: &[crate::native_policy_train_step_v1::NativePolicyPhysicalDecisionV1<'_>],
    model: &crate::native_policy_value_net_v1::NativePolicyValueNetV1,
    generation: crate::sideboard_play_policy_v1::FreshLineageGenerationV1,
) -> Result<Vec<Option<(f64, f64)>>, String> {
    use crate::native_policy_train_step_v1::NativePolicyForwardInputV1;
    use crate::sideboard_play_policy_v1::FreshLineageGenerationV1;
    input
        .roots
        .iter()
        .map(|root| {
            let Some(target) = &root.target else {
                return Ok(None);
            };
            let substep = groups
                .get(root.group_index)
                .and_then(|group| group.substeps.get(root.substep_index))
                .ok_or("line (b) rescore root is outside the batch")?;
            let before: Vec<f64> = substep
                .expected_raw_action_logit_bits
                .iter()
                .map(|bits| f64::from(f32::from_bits(*bits)))
                .collect();
            let view = match &substep.forward {
                NativePolicyForwardInputV1::Encoded(view) => **view,
                NativePolicyForwardInputV1::Packed { encoded, .. } => **encoded,
            };
            let output = match generation {
                FreshLineageGenerationV1::V3 => model.forward_feature_transfer_v3(view),
                FreshLineageGenerationV1::V4 => model.forward_feature_transfer_v4(view),
            }
            .map_err(|error| format!("line (b) rescore forward: {error:?}"))?;
            let after: Vec<f64> = output.logits.iter().map(|&z| f64::from(z)).collect();
            let divergence = |logits: &[f64]| {
                line_b_divergence_v1(input.direction, logits, target)
                    .map(|value| value.divergence)
                    .map_err(|error| error.to_string())
            };
            Ok(Some((divergence(&before)?, divergence(&after)?)))
        })
        .collect()
}

pub const LINE_B_HEAD_DISTANCE_SCHEMA_V1: &str = "mtg-kernel-line-b-head-distance/v1";

/// End-of-run head distance (proposal 12:51): the L2 distance over the seven
/// trainable head tensors between a treatment and its matched control, the
/// control's head L2 over the same tensors as the reference, and their ratio
/// (null when the reference norm is zero).
pub(super) fn line_b_head_distance_v1(
    treatment: &ExpandedModelSourceV1,
    control: &ExpandedModelSourceV1,
) -> Result<Value, String> {
    let (_, treatment_state) = super::initialize(treatment)?;
    let (_, control_state) = super::initialize(control)?;
    let treatment = treatment_state.snapshot_v1().map_err(super::err)?;
    let control = control_state.snapshot_v1().map_err(super::err)?;
    line_b_head_distance_from_parameters_v1(&treatment.parameters, &control.parameters)
}

pub(super) fn line_b_head_distance_from_parameters_v1(
    treatment: &[crate::native_policy_value_net_v1::NativeNamedParameterV1],
    control: &[crate::native_policy_value_net_v1::NativeNamedParameterV1],
) -> Result<Value, String> {
    use crate::native_policy_train_step_v1::HEAD_ONLY_TRAINABLE_TENSORS_V1;
    use crate::native_policy_value_net_v1::NativeNamedParameterV1;
    fn find<'a>(
        parameters: &'a [NativeNamedParameterV1],
        name: &str,
    ) -> Result<&'a NativeNamedParameterV1, String> {
        parameters
            .iter()
            .find(|parameter| parameter.name == name)
            .ok_or_else(|| format!("line (b) head distance: {name} is missing"))
    }
    let (mut squared_distance, mut squared_reference) = (0.0_f64, 0.0_f64);
    for name in HEAD_ONLY_TRAINABLE_TENSORS_V1 {
        let (a, b) = (find(treatment, name)?, find(control, name)?);
        if a.values.len() != b.values.len() {
            return Err(format!("line (b) head distance: {name} shapes differ"));
        }
        for (x, y) in a.values.iter().zip(&b.values) {
            let (x, y) = (f64::from(*x), f64::from(*y));
            squared_distance += (x - y) * (x - y);
            squared_reference += y * y;
        }
    }
    let (distance, reference) = (squared_distance.sqrt(), squared_reference.sqrt());
    Ok(json!({"schema": LINE_B_HEAD_DISTANCE_SCHEMA_V1,
        "tensors": HEAD_ONLY_TRAINABLE_TENSORS_V1,
        "l2": distance,
        "reference": "the control's L2 over the same tensors",
        "reference_l2": reference,
        "relative_l2": if reference > 0.0 { json!(distance / reference) } else { Value::Null }}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native_policy_train_step_v1::HEAD_ONLY_TRAINABLE_TENSORS_V1;
    use crate::native_policy_value_net_v1::NativeNamedParameterV1;

    fn heads(value: f32) -> Vec<NativeNamedParameterV1> {
        HEAD_ONLY_TRAINABLE_TENSORS_V1
            .into_iter()
            .map(|name| NativeNamedParameterV1 {
                name,
                shape: vec![2],
                values: vec![value; 2],
            })
            .collect()
    }

    #[test]
    fn head_distance_reports_l2_reference_and_a_null_ratio_at_a_zero_reference() {
        let (zero, one) = (heads(0.0), heads(1.0));
        let distance = line_b_head_distance_from_parameters_v1(&one, &zero).unwrap();
        assert_eq!(distance["schema"], LINE_B_HEAD_DISTANCE_SCHEMA_V1);
        assert_eq!(distance["l2"].as_f64().unwrap(), 14.0_f64.sqrt());
        assert_eq!(distance["reference_l2"].as_f64().unwrap(), 0.0);
        assert!(distance["relative_l2"].is_null());
        let back = line_b_head_distance_from_parameters_v1(&zero, &one).unwrap();
        assert_eq!(back["relative_l2"].as_f64().unwrap(), 1.0);
        let mut missing = one.clone();
        missing.pop();
        assert!(line_b_head_distance_from_parameters_v1(&missing, &one).is_err());
        let mut reshaped = one.clone();
        reshaped[0].values.push(1.0);
        assert!(line_b_head_distance_from_parameters_v1(&reshaped, &one).is_err());
    }
}
