use super::super::{
    authenticate_serial_replay_row_v1, trajectory_contract_v1, visit_serial_replay_rows_v1,
    ExpandedTrajectoryV1, GaeTargetsV1, S_UPDATE_REPLAY_LEARNER_ROW_V1,
    S_UPDATE_REPLAY_OTHER_ROW_V1,
};
use super::*;
use crate::s_transformer_v1::verified_row_v1::{
    check_parallel_tape_cap_v1, parallel_gradient_v1, ParallelVerifiedUseV1, ReplayErrorV1,
    SerialVerifiedGaeV1, StoredRowV1,
};
use crate::s_transformer_v1::SLossV1;
use crate::sideboard_play_policy_v1::FreshLineageGenerationV1;
use std::sync::atomic::{AtomicU64, Ordering};

pub(super) struct OptionsV1 {
    pub(super) gamma: f32,
    pub(super) lambda: f32,
    pub(super) coefficient: f32,
    pub(super) tape_cap: usize,
    #[cfg(test)]
    pub(super) short_first_gradient: bool,
    pub(super) training_math: TrainingMathV1,
}

pub(super) struct GradientV1 {
    pub(super) loss: SLossV1,
    pub(super) gradient: Vec<f32>,
    pub(super) gae: GaeTargetsV1,
    pub(super) policy_substeps: usize,
}

/// The caller has validated the stored trajectories and their sampler draws.
/// The recorded first values are provisional until every actor row replays.
pub(super) fn gradient_v1(
    episodes: &[ExpandedTrajectoryV1],
    policy: &FrozenPlayPolicyV1,
    learner: &ExpandedSeatBehaviorV1,
    model: &SModelV1,
    options: OptionsV1,
    attempt: &mut VerifiedReplayAttemptV1,
) -> Result<GradientV1, ReplayErrorV1> {
    // A capacity fallback restarts legacy replay. If the first S forward
    // would change this thread's arithmetic controls, choose legacy before
    // replay so preceding opponent rows see their original environment.
    if crate::deterministic_math_v1::verify_pinned_mxcsr_state_v1().is_err() {
        return Err(ReplayErrorV1::CallerMathState);
    }
    let mut values = Vec::new();
    let mut returns = Vec::new();
    let mut ordinals = Vec::new();
    for (ordinal, episode) in episodes.iter().enumerate() {
        let seat = episode.episode.learner_seat;
        for row in &episode.decisions {
            if row.actor == seat && row.substep_index == 0 {
                values.push(f32::from_bits(row.value));
                returns.push(episode.terminal.terminal_reward[seat as usize] as i8);
                ordinals.push(ordinal);
            }
        }
    }

    // The policy owns an immutable clone of the loaded train-state model.
    // Bind the candidate's borrowed model to the actual replay generation.
    let expected = policy
        .s_model_v1()
        .ok_or("native S is not installed".to_owned())?;
    let mut work = SerialVerifiedGaeV1::new(
        model,
        expected.cache_namespace_v1(),
        options.tape_cap,
        values.len(),
        options.coefficient,
    )?;
    #[cfg(test)]
    if options.short_first_gradient {
        work.shorten_first_gradient_for_test_v1();
    }
    attempt.peak_retained_tape_bytes = work.peak_retained_tape_bytes_v1();
    let mut opponents = OpponentCacheV1::default().with_training_math_v1(options.training_math);
    let mut gae = None;
    let mut group = 0;
    let mut policy_substeps = 0;
    for episode in episodes {
        let opponent = episode
            .episode
            .opponent
            .as_ref()
            .map(|source| opponents.load(source))
            .transpose()?;
        validate_actual_behaviors_v1(
            episode,
            learner,
            opponent.as_ref().map(|other| &other.behavior),
        )?;
        visit_serial_replay_rows_v1(
            episode,
            policy,
            opponent.as_ref().map(|other| &other.policy),
            |generation, acting, row| {
                let input = row.tensor.tensor();
                if row.actor != episode.episode.learner_seat {
                    authenticate_serial_replay_row_v1(
                        generation,
                        acting,
                        row,
                        input,
                        S_UPDATE_REPLAY_OTHER_ROW_V1,
                    )?;
                    attempt.other_replay_forwards += 1;
                    return Ok(());
                }
                if generation != FreshLineageGenerationV1::V4
                    || !row.tensor.pairing_node_indices.is_empty()
                {
                    // Use the reference scorer to preserve its rejection and
                    // error text. Only its plain V4 S forward is reusable.
                    authenticate_serial_replay_row_v1(
                        generation,
                        acting,
                        row,
                        input,
                        S_UPDATE_REPLAY_LEARNER_ROW_V1,
                    )?;
                    attempt.learner_replay_forwards += 1;
                    return Err(ReplayErrorV1::UnsupportedGeneration);
                }
                let sub = row.substep_index as usize;
                let last = row.substep_index + 1 == row.substep_count;
                let backward = work.authenticate_v1(
                    SSubstepV1 {
                        input: SInputV1::of_tensor_v1(&input),
                        selected: row.selected as usize,
                    },
                    (group, sub),
                    last,
                    &row.logits,
                    row.value,
                    || {
                        // Serial replay pins the caller's MXCSR at the first
                        // S forward. Preserve that order for the target math.
                        gae.get_or_insert_with(|| {
                            compute_gae_targets_of_values_v1(
                                &values,
                                &returns,
                                &ordinals,
                                options.gamma,
                                options.lambda,
                            )
                        })
                        .as_ref()
                        .ok()
                        .map(|gae| (gae.advantages[group], gae.value_targets[group]))
                    },
                );
                attempt.peak_retained_tape_bytes = work.peak_retained_tape_bytes_v1();
                attempt.learner_backwards += u64::from(backward?);
                attempt.learner_replay_forwards += 1;
                policy_substeps += 1;
                group += usize::from(last);
                Ok(())
            },
        )?;
    }
    // No provisional GAE or learner error outranks a later replay error.
    let gae = gae.ok_or_else(|| "no learner decisions".to_owned())??;
    let (loss, gradient) = work.finish_v1()?;
    Ok(GradientV1 {
        loss,
        gradient,
        gae,
        policy_substeps,
    })
}

pub(super) struct PreparedGradientV1 {
    pub(super) loss: SLossV1,
    pub(super) gradient: Vec<f32>,
    pub(super) gae: GaeTargetsV1,
    pub(super) policy_substeps: usize,
    pub(super) telemetry: ordered_update_preparation::PreparationTelemetryV1,
}

/// The bounded candidate of a prepared update on `workers` workers: the
/// other rows replay as in `ordered_update_preparation::prepare_v1`, and
/// each learner row only in the parallel learner, whose forward of it is
/// checked against the stored row before its backward. The caller has
/// validated the stored trajectories and their sampler draws. The
/// preparation's errors and their precedence are not the reference's, so
/// every failure is for the caller to fall back to the reference.
#[allow(clippy::too_many_arguments)]
pub(super) fn prepared_gradient_v1(
    episodes: &[ExpandedTrajectoryV1],
    policy: &FrozenPlayPolicyV1,
    learner: &ExpandedSeatBehaviorV1,
    model: &SModelV1,
    workers: usize,
    max_prepared_tensor_mebibytes: usize,
    options: OptionsV1,
    attempt: &mut VerifiedReplayAttemptV1,
) -> Result<PreparedGradientV1, ReplayErrorV1> {
    // One tape per learner worker, made only if the learner runs.
    attempt.tape_slots = Some(workers);
    // Workers start in the caller's state. The reference pins a worker's
    // MXCSR at its first learner forward, before that worker's later other
    // rows; this candidate leaves the preparation workers' learner rows to
    // the learner, which is the same only from the pinned state.
    if crate::deterministic_math_v1::verify_pinned_mxcsr_state_v1().is_err() {
        return Err(ReplayErrorV1::CallerMathState);
    }
    // Only a plain V4 replay of a learner row is S's forward of the stored
    // tensor, which the learner's forward then repeats.
    let expected = policy
        .s_plain_v4_replay_v1()
        .ok_or(ReplayErrorV1::UnsupportedGeneration)?;
    // An empty slot over the cap refuses before any replay.
    check_parallel_tape_cap_v1(options.tape_cap)?;
    for episode in episodes {
        let seat = episode.episode.learner_seat;
        let generation = trajectory_contract_v1(episode)?.generation;
        if episode.decisions.iter().any(|row| {
            row.actor == seat
                && (generation != FreshLineageGenerationV1::V4
                    || !row.tensor.pairing_node_indices.is_empty())
        }) {
            return Err(ReplayErrorV1::UnsupportedGeneration);
        }
    }
    let other_forwards = AtomicU64::new(0);
    let prepared = ordered_update_preparation::prepare_deferring_learner_rows_v1(
        episodes,
        policy,
        learner,
        workers,
        max_prepared_tensor_mebibytes,
        options.training_math,
        &other_forwards,
    );
    attempt.other_replay_forwards = other_forwards.load(Ordering::Relaxed);
    let ordered_update_preparation::PreparedGroupsV1 { groups, telemetry } = prepared?;
    // The reference's targets, from the recorded first values.
    ensure(!groups.is_empty(), "no learner decisions")?;
    let mut values = Vec::with_capacity(groups.len());
    for (index, (_, _, rows)) in groups.iter().enumerate() {
        let (first, _) = rows
            .first()
            .ok_or_else(|| format!("gae group {index} has no substeps"))?;
        values.push(f32::from_bits(first.value));
    }
    let returns: Vec<i8> = groups.iter().map(|group| group.0).collect();
    let ordinals: Vec<usize> = groups.iter().map(|group| group.1).collect();
    let gae = compute_gae_targets_of_values_v1(
        &values,
        &returns,
        &ordinals,
        options.gamma,
        options.lambda,
    )?;
    let s_groups: Vec<SGroupV1<'_>> = groups
        .iter()
        .zip(gae.advantages.iter().zip(&gae.value_targets))
        .map(|((_, _, rows), (advantage, value_target))| SGroupV1 {
            substeps: rows
                .iter()
                .map(|(row, tensor)| SSubstepV1 {
                    input: SInputV1::of_tensor_v1(tensor),
                    selected: row.selected as usize,
                })
                .collect(),
            advantage: *advantage,
            value_target: *value_target,
        })
        .collect();
    let stored: Vec<StoredRowV1<'_>> = groups
        .iter()
        .flat_map(|(_, _, rows)| rows)
        .map(|(row, _)| StoredRowV1 {
            logits: &row.logits,
            value: row.value,
        })
        .collect();
    let mut usage = ParallelVerifiedUseV1::default();
    let result = parallel_gradient_v1(
        model,
        expected.cache_namespace_v1(),
        &s_groups,
        &stored,
        options.coefficient,
        workers,
        options.tape_cap,
        &mut usage,
    );
    attempt.peak_retained_tape_bytes = usage.peak_retained_tape_bytes;
    attempt.learner_replay_forwards = usage.learner_forwards;
    attempt.learner_backwards = usage.learner_backwards;
    let (loss, gradient) = result?;
    Ok(PreparedGradientV1 {
        loss,
        gradient,
        gae,
        policy_substeps: stored.len(),
        telemetry,
    })
}
