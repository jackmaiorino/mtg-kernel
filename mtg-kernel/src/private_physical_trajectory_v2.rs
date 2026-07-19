//! Flat Policy V2 ownership adapter for the shared physical trajectory core.
//!
//! Every scorer row is copied without projection, so `blocked_order` and the
//! full `u32` action-reference card-token domain remain associated with the
//! exact V2 decision binding staged for training.

#![allow(dead_code)]

use crate::async_flat_scored_rollout_v2::{
    FlatScoredSelectedEventV2, FlatScoredTerminalEventV2, FlatScoredTrajectoryObserverV2,
};
use crate::async_rollout::AsyncRolloutTerminalV1;
use crate::flat_policy_v2::{
    FlatCompletedDungeonV2, FlatContextPathElementV2, FlatDecisionBindingV2,
    FlatEffectSubtypeChangeV2, FlatGlobalsV2, FlatObjectAbilityUseV2, FlatObjectCoreV2,
    FlatObjectGoadV2, FlatObjectSubtypeV2, FlatRelationV2, FlatScorerActionCoreV2,
    FlatScorerActionRefV2, FlatScoringDecisionViewV2,
};
use crate::private_physical_trajectory_core::{
    decision_kind_code, player_seat_code, FlatGroupedEpisodeCore, FlatGroupedTrajectoryBatchCore,
    FlatLearnerSubstepSampleCore, FlatPhysicalDecisionSampleCore, FlatPhysicalTrajectoryErrorCore,
    FlatPhysicalTrajectoryObserverCore, FlatPhysicalUpdateStagingCore, FlatSelectedSampleCore,
    FlatTerminalSampleCore,
};
use crate::rl::PlayerSeatV1;
use crate::rl_session::FastActorDecisionV1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FlatOwnedScoringInputsV2 {
    pub(crate) globals: FlatGlobalsV2,
    pub(crate) objects: Vec<FlatObjectCoreV2>,
    pub(crate) relations: Vec<FlatRelationV2>,
    pub(crate) object_subtypes: Vec<FlatObjectSubtypeV2>,
    pub(crate) ability_uses: Vec<FlatObjectAbilityUseV2>,
    pub(crate) goads: Vec<FlatObjectGoadV2>,
    pub(crate) completed_dungeons: Vec<FlatCompletedDungeonV2>,
    pub(crate) effect_subtype_changes: Vec<FlatEffectSubtypeChangeV2>,
    pub(crate) context_path_elements: Vec<FlatContextPathElementV2>,
    pub(crate) actions: Vec<FlatScorerActionCoreV2>,
    pub(crate) action_refs: Vec<FlatScorerActionRefV2>,
}

impl FlatOwnedScoringInputsV2 {
    fn copy_from_view(decision: FlatScoringDecisionViewV2<'_>) -> Self {
        Self {
            globals: *decision.globals(),
            objects: decision.objects().to_vec(),
            relations: decision.relations().to_vec(),
            object_subtypes: decision.object_subtypes().to_vec(),
            ability_uses: decision.ability_uses().to_vec(),
            goads: decision.goads().to_vec(),
            completed_dungeons: decision.completed_dungeons().to_vec(),
            effect_subtype_changes: decision.effect_subtype_changes().to_vec(),
            context_path_elements: decision.context_path_elements().to_vec(),
            actions: decision.actions().to_vec(),
            action_refs: decision.action_refs().to_vec(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FlatLearnerSubstepSampleV2 {
    pub(crate) expected: FastActorDecisionV1,
    pub(crate) binding: FlatDecisionBindingV2,
    pub(crate) learner_ordinal: u64,
    pub(crate) action_seed: u64,
    pub(crate) selected_index: u32,
    pub(crate) raw_action_logit_bits: Vec<u32>,
    pub(crate) selected_log_probability_bits: u32,
    pub(crate) predicted_value_bits: u32,
    pub(crate) scoring_inputs: FlatOwnedScoringInputsV2,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FlatPhysicalDecisionSampleV2 {
    pub(crate) episode_id: u64,
    pub(crate) physical_decision_id: u64,
    pub(crate) acting_player: PlayerSeatV1,
    pub(crate) first_learner_ordinal: u64,
    pub(crate) substep_count: u32,
    pub(crate) joint_selected_log_probability_bits: u32,
    pub(crate) value_bits: u32,
    pub(crate) substeps: Vec<FlatLearnerSubstepSampleV2>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FlatGroupedEpisodeV2 {
    pub(crate) episode_id: u64,
    pub(crate) learner_seat: PlayerSeatV1,
    pub(crate) learner_return: i32,
    pub(crate) learner_policy_step_count: u64,
    pub(crate) opponent_policy_step_count: u64,
    pub(crate) learner_physical_decision_count: u64,
    pub(crate) opponent_physical_decision_count: u64,
    pub(crate) learner_trace_hash: u64,
    pub(crate) terminal: AsyncRolloutTerminalV1,
    pub(crate) groups: Vec<FlatPhysicalDecisionSampleV2>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FlatPhysicalUpdateStagingV2 {
    NoUpdateZeroLearnerGroups,
    Ready { learner_group_count: u64 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FlatGroupedTrajectoryBatchV2 {
    pub(crate) learner_seat: PlayerSeatV1,
    pub(crate) first_episode_id: u64,
    pub(crate) episode_count: u64,
    pub(crate) learner_policy_step_count: u64,
    pub(crate) learner_physical_decision_count: u64,
    pub(crate) update_staging: FlatPhysicalUpdateStagingV2,
    pub(crate) episodes: Vec<FlatGroupedEpisodeV2>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FlatPhysicalTrajectoryErrorV2 {
    kind: FlatPhysicalTrajectoryErrorCore,
}

impl From<FlatPhysicalTrajectoryErrorCore> for FlatPhysicalTrajectoryErrorV2 {
    fn from(kind: FlatPhysicalTrajectoryErrorCore) -> Self {
        Self { kind }
    }
}

#[derive(Debug)]
pub(crate) struct FlatPhysicalTrajectoryObserverV2 {
    core: FlatPhysicalTrajectoryObserverCore<FlatDecisionBindingV2, FlatOwnedScoringInputsV2>,
}

impl FlatPhysicalTrajectoryObserverV2 {
    pub(crate) fn new(
        learner_seat: PlayerSeatV1,
        first_episode_id: u64,
        episode_count: u64,
    ) -> Result<Self, FlatPhysicalTrajectoryErrorV2> {
        Ok(Self {
            core: FlatPhysicalTrajectoryObserverCore::new(
                learner_seat,
                first_episode_id,
                episode_count,
            )
            .map_err(FlatPhysicalTrajectoryErrorV2::from)?,
        })
    }
}

impl FlatScoredTrajectoryObserverV2 for FlatPhysicalTrajectoryObserverV2 {
    type Error = FlatPhysicalTrajectoryErrorV2;
    type Output = FlatGroupedTrajectoryBatchV2;

    fn observe_selected_v2(
        &mut self,
        event: FlatScoredSelectedEventV2<'_>,
    ) -> Result<(), Self::Error> {
        let binding_matches = selected_binding_matches(&event);
        let decision = event.decision;
        self.core
            .observe_selected(
                FlatSelectedSampleCore {
                    expected: event.expected,
                    binding: event.binding,
                    binding_matches,
                    learner_ordinal: event.learner_ordinal,
                    action_seed: event.action_seed,
                    selected_index: event.selected_index,
                    raw_action_logits: event.raw_action_logits,
                    scorer_action_count: decision.actions().len(),
                    predicted_value_bits: event.predicted_value_bits,
                },
                || FlatOwnedScoringInputsV2::copy_from_view(decision),
            )
            .map_err(FlatPhysicalTrajectoryErrorV2::from)
    }

    fn observe_terminal_v2(&mut self, event: FlatScoredTerminalEventV2) -> Result<(), Self::Error> {
        self.core
            .observe_terminal(FlatTerminalSampleCore {
                terminal: event.terminal,
                learner_action_count: event.learner_action_count,
                learner_trace_hash: event.learner_trace_hash,
            })
            .map_err(FlatPhysicalTrajectoryErrorV2::from)
    }

    fn finish_v2(self) -> Result<Self::Output, Self::Error> {
        self.core
            .finish()
            .map(FlatGroupedTrajectoryBatchV2::from_core)
            .map_err(FlatPhysicalTrajectoryErrorV2::from)
    }
}

impl FlatGroupedTrajectoryBatchV2 {
    fn from_core(
        batch: FlatGroupedTrajectoryBatchCore<FlatDecisionBindingV2, FlatOwnedScoringInputsV2>,
    ) -> Self {
        Self {
            learner_seat: batch.learner_seat,
            first_episode_id: batch.first_episode_id,
            episode_count: batch.episode_count,
            learner_policy_step_count: batch.learner_policy_step_count,
            learner_physical_decision_count: batch.learner_physical_decision_count,
            update_staging: match batch.update_staging {
                FlatPhysicalUpdateStagingCore::NoUpdateZeroLearnerGroups => {
                    FlatPhysicalUpdateStagingV2::NoUpdateZeroLearnerGroups
                }
                FlatPhysicalUpdateStagingCore::Ready {
                    learner_group_count,
                } => FlatPhysicalUpdateStagingV2::Ready {
                    learner_group_count,
                },
            },
            episodes: batch
                .episodes
                .into_iter()
                .map(FlatGroupedEpisodeV2::from_core)
                .collect(),
        }
    }
}

impl FlatGroupedEpisodeV2 {
    fn from_core(
        episode: FlatGroupedEpisodeCore<FlatDecisionBindingV2, FlatOwnedScoringInputsV2>,
    ) -> Self {
        Self {
            episode_id: episode.episode_id,
            learner_seat: episode.learner_seat,
            learner_return: episode.learner_return,
            learner_policy_step_count: episode.learner_policy_step_count,
            opponent_policy_step_count: episode.opponent_policy_step_count,
            learner_physical_decision_count: episode.learner_physical_decision_count,
            opponent_physical_decision_count: episode.opponent_physical_decision_count,
            learner_trace_hash: episode.learner_trace_hash,
            terminal: episode.terminal,
            groups: episode
                .groups
                .into_iter()
                .map(FlatPhysicalDecisionSampleV2::from_core)
                .collect(),
        }
    }
}

impl FlatPhysicalDecisionSampleV2 {
    fn from_core(
        group: FlatPhysicalDecisionSampleCore<FlatDecisionBindingV2, FlatOwnedScoringInputsV2>,
    ) -> Self {
        Self {
            episode_id: group.episode_id,
            physical_decision_id: group.physical_decision_id,
            acting_player: group.acting_player,
            first_learner_ordinal: group.first_learner_ordinal,
            substep_count: group.substep_count,
            joint_selected_log_probability_bits: group.joint_selected_log_probability_bits,
            value_bits: group.value_bits,
            substeps: group
                .substeps
                .into_iter()
                .map(FlatLearnerSubstepSampleV2::from_core)
                .collect(),
        }
    }
}

impl FlatLearnerSubstepSampleV2 {
    fn from_core(
        sample: FlatLearnerSubstepSampleCore<FlatDecisionBindingV2, FlatOwnedScoringInputsV2>,
    ) -> Self {
        Self {
            expected: sample.expected,
            binding: sample.binding,
            learner_ordinal: sample.learner_ordinal,
            action_seed: sample.action_seed,
            selected_index: sample.selected_index,
            raw_action_logit_bits: sample.raw_action_logit_bits,
            selected_log_probability_bits: sample.selected_log_probability_bits,
            predicted_value_bits: sample.predicted_value_bits,
            scoring_inputs: sample.scoring_inputs,
        }
    }
}

fn selected_binding_matches(event: &FlatScoredSelectedEventV2<'_>) -> bool {
    let expected = event.expected;
    let binding = event.binding.action_binding;
    binding.episode_id == expected.episode_id
        && binding.environment_revision == expected.environment_revision
        && binding.bound_policy_step_count == expected.step
        && binding.physical_decision_id == expected.physical_decision_id
        && binding.bound_physical_decision_count == expected.physical_decision_id
        && binding.substep_index == expected.substep_index
        && binding.substep_count == expected.substep_count
        && binding.acting_player == player_seat_code(expected.acting_player)
        && binding.decision_kind == decision_kind_code(expected.decision_kind)
        && binding.legal_action_count == expected.legal_action_count
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flat_policy_v2::{FlatRelationPayloadV2, FlatScoringDecisionViewV2};
    use crate::rl_session::{FastActorDecisionKindV1, FlatActionDecisionBindingV2};

    #[test]
    fn owned_v2_inputs_retain_blocked_order_and_u32_card_token() {
        let globals = FlatGlobalsV2::default();
        let objects = [FlatObjectCoreV2 {
            card_token: 65_536,
            ..FlatObjectCoreV2::default()
        }];
        let relations = [FlatRelationV2 {
            payload: FlatRelationPayloadV2::CombatAttacker {
                blocked_order: Some(3),
            },
            ..FlatRelationV2::default()
        }];
        let actions = [FlatScorerActionCoreV2::default()];
        let refs = [FlatScorerActionRefV2 {
            card_token: 65_536,
            ..FlatScorerActionRefV2::default()
        }];
        let view = FlatScoringDecisionViewV2::new(
            &globals,
            &objects,
            &relations,
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &actions,
            &refs,
        );
        let owned = FlatOwnedScoringInputsV2::copy_from_view(view);
        assert_eq!(owned.action_refs[0].card_token, 65_536);
        assert_eq!(
            owned.relations[0].payload,
            FlatRelationPayloadV2::CombatAttacker {
                blocked_order: Some(3)
            }
        );
    }

    #[test]
    fn binding_failure_poisons_v2_observer_reuse() {
        let mut observer = FlatPhysicalTrajectoryObserverV2::new(PlayerSeatV1::P0, 7, 1).unwrap();
        let globals = FlatGlobalsV2::default();
        let actions = [FlatScorerActionCoreV2::default()];
        let view = FlatScoringDecisionViewV2::new(
            &globals,
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &actions,
            &[],
        );
        let expected = FastActorDecisionV1 {
            episode_id: 7,
            step: 0,
            environment_revision: 0,
            physical_decision_id: 0,
            substep_index: 0,
            substep_count: 1,
            acting_player: PlayerSeatV1::P0,
            decision_kind: FastActorDecisionKindV1::Surface,
            legal_action_count: 1,
        };
        let event = || FlatScoredSelectedEventV2 {
            expected,
            binding: FlatDecisionBindingV2 {
                action_binding: FlatActionDecisionBindingV2 {
                    episode_id: 999,
                    legal_action_count: 1,
                    ..FlatActionDecisionBindingV2::default()
                },
                ..FlatDecisionBindingV2::default()
            },
            learner_ordinal: 0,
            action_seed: 0,
            selected_index: 0,
            raw_action_logits: &[0.0],
            predicted_value_bits: 0.0f32.to_bits(),
            decision: view,
        };
        let first = observer.observe_selected_v2(event()).unwrap_err();
        assert_eq!(
            first.kind,
            FlatPhysicalTrajectoryErrorCore::SelectedBindingMismatch {
                episode_id: 7,
                learner_ordinal: 0
            }
        );
        assert_eq!(observer.observe_selected_v2(event()), Err(first));
        assert_eq!(observer.finish_v2(), Err(first));
    }
}
