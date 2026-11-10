use super::super::{
    authenticate_serial_replay_row_v1, visit_serial_replay_rows_v1, ExpandedTrajectoryV1,
    GaeTargetsV1, S_UPDATE_REPLAY_LEARNER_ROW_V1, S_UPDATE_REPLAY_OTHER_ROW_V1,
};
use super::*;
use crate::s_transformer_v1::verified_row_v1::{ReplayErrorV1, SerialVerifiedGaeV1};
use crate::s_transformer_v1::SLossV1;
use crate::sideboard_play_policy_v1::FreshLineageGenerationV1;

pub(super) struct OptionsV1 {
    pub(super) gamma: f32,
    pub(super) lambda: f32,
    pub(super) coefficient: f32,
    pub(super) tape_cap: usize,
    pub(super) training_math: TrainingMathV1,
}

pub(super) struct GradientV1 {
    pub(super) loss: SLossV1,
    pub(super) gradient: Vec<f32>,
    pub(super) gae: GaeTargetsV1,
    pub(super) policy_substeps: usize,
    pub(super) peak_tape_bytes: usize,
}

/// The caller has validated the stored trajectories and their sampler draws.
/// The recorded first values are provisional until every actor row replays.
pub(super) fn gradient_v1(
    episodes: &[ExpandedTrajectoryV1],
    policy: &FrozenPlayPolicyV1,
    learner: &ExpandedSeatBehaviorV1,
    model: &SModelV1,
    options: OptionsV1,
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
                    return authenticate_serial_replay_row_v1(
                        generation,
                        acting,
                        row,
                        input,
                        S_UPDATE_REPLAY_OTHER_ROW_V1,
                    )
                    .map_err(ReplayErrorV1::from);
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
                    return Err(ReplayErrorV1::UnsupportedGeneration);
                }
                let sub = row.substep_index as usize;
                let last = row.substep_index + 1 == row.substep_count;
                work.authenticate_v1(
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
                )?;
                policy_substeps += 1;
                group += usize::from(last);
                Ok(())
            },
        )?;
    }
    // No provisional GAE or learner error outranks a later replay error.
    let gae = gae.ok_or_else(|| "no learner decisions".to_owned())??;
    let (loss, gradient, peak_tape_bytes) = work.finish_v1()?;
    Ok(GradientV1 {
        loss,
        gradient,
        gae,
        policy_substeps,
        peak_tape_bytes,
    })
}
