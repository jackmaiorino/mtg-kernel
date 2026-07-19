//! Version-neutral inert assembly for scored physical-decision trajectories.
//!
//! Versioned wrappers prove their own scorer binding and own their packet
//! rows, then hand those already-associated values to this state machine. The
//! state machine owns ordering, grouping, terminal, poisoning, and finite-loss
//! invariants exactly once for both Flat Policy families.

#![allow(dead_code)]

use crate::async_flat_scored_rollout_v1::{initial_learner_trace_hash_v1, record_learner_trace_v1};
use crate::async_rollout::AsyncRolloutTerminalV1;
use crate::rl::{PlayerSeatV1, TerminalClassificationV1, TerminalOutcomeV1, TerminalSafeCodeV2};
use crate::rl_session::{FastActorDecisionKindV1, FastActorDecisionV1};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FlatLearnerSubstepSampleCore<B, I> {
    pub(crate) expected: FastActorDecisionV1,
    pub(crate) binding: B,
    pub(crate) learner_ordinal: u64,
    pub(crate) action_seed: u64,
    pub(crate) selected_index: u32,
    pub(crate) raw_action_logit_bits: Vec<u32>,
    pub(crate) selected_log_probability_bits: u32,
    pub(crate) predicted_value_bits: u32,
    pub(crate) scoring_inputs: I,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FlatPhysicalDecisionSampleCore<B, I> {
    pub(crate) episode_id: u64,
    pub(crate) physical_decision_id: u64,
    pub(crate) acting_player: PlayerSeatV1,
    pub(crate) first_learner_ordinal: u64,
    pub(crate) substep_count: u32,
    pub(crate) joint_selected_log_probability_bits: u32,
    pub(crate) value_bits: u32,
    pub(crate) substeps: Vec<FlatLearnerSubstepSampleCore<B, I>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FlatGroupedEpisodeCore<B, I> {
    pub(crate) episode_id: u64,
    pub(crate) learner_seat: PlayerSeatV1,
    pub(crate) learner_return: i32,
    pub(crate) learner_policy_step_count: u64,
    pub(crate) opponent_policy_step_count: u64,
    pub(crate) learner_physical_decision_count: u64,
    pub(crate) opponent_physical_decision_count: u64,
    pub(crate) learner_trace_hash: u64,
    pub(crate) terminal: AsyncRolloutTerminalV1,
    pub(crate) groups: Vec<FlatPhysicalDecisionSampleCore<B, I>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FlatPhysicalUpdateStagingCore {
    NoUpdateZeroLearnerGroups,
    Ready { learner_group_count: u64 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FlatGroupedTrajectoryBatchCore<B, I> {
    pub(crate) learner_seat: PlayerSeatV1,
    pub(crate) first_episode_id: u64,
    pub(crate) episode_count: u64,
    pub(crate) learner_policy_step_count: u64,
    pub(crate) learner_physical_decision_count: u64,
    pub(crate) update_staging: FlatPhysicalUpdateStagingCore,
    pub(crate) episodes: Vec<FlatGroupedEpisodeCore<B, I>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FlatPhysicalTrajectoryErrorCore {
    ExpectedEpisodeRangeOverflow,
    EpisodeCountExceedsAddressSpace,
    ResultAllocationFailed,
    EpisodeOutOfRange {
        episode_id: u64,
    },
    SelectedAfterTerminal {
        episode_id: u64,
    },
    SelectedActorMismatch {
        episode_id: u64,
        expected: PlayerSeatV1,
        actual: PlayerSeatV1,
    },
    SelectedBindingMismatch {
        episode_id: u64,
        learner_ordinal: u64,
    },
    LegalActionCountMismatch {
        episode_id: u64,
        learner_ordinal: u64,
    },
    SelectedIndexOutOfRange {
        episode_id: u64,
        learner_ordinal: u64,
    },
    NonFiniteLogit {
        episode_id: u64,
        learner_ordinal: u64,
        action_index: u32,
        bits: u32,
    },
    NonFiniteSelectedLogProbability {
        episode_id: u64,
        learner_ordinal: u64,
    },
    NonFinitePredictedValue {
        episode_id: u64,
        learner_ordinal: u64,
        bits: u32,
    },
    LearnerOrdinalMismatch {
        episode_id: u64,
        expected: u64,
        actual: u64,
    },
    ZeroSubstepCount {
        episode_id: u64,
        physical_decision_id: u64,
    },
    FirstSubstepNotZero {
        episode_id: u64,
        physical_decision_id: u64,
        actual: u32,
    },
    PhysicalDecisionNotStrictlyIncreasing {
        episode_id: u64,
        previous: u64,
        actual: u64,
    },
    PolicyStepNotStrictlyIncreasing {
        episode_id: u64,
        previous: u64,
        actual: u64,
    },
    OpenGroupKeyMismatch {
        episode_id: u64,
        expected_physical_decision_id: u64,
        actual_physical_decision_id: u64,
    },
    OpenGroupActorMismatch {
        episode_id: u64,
        physical_decision_id: u64,
    },
    SubstepCountMismatch {
        episode_id: u64,
        physical_decision_id: u64,
        expected: u32,
        actual: u32,
    },
    SubstepIndexMismatch {
        episode_id: u64,
        physical_decision_id: u64,
        expected: u32,
        actual: u32,
    },
    PolicyStepNotContiguousWithinGroup {
        episode_id: u64,
        physical_decision_id: u64,
        expected: u64,
        actual: u64,
    },
    JointLogProbabilityOverflow {
        episode_id: u64,
        physical_decision_id: u64,
    },
    CountOverflow,
    DuplicateTerminal {
        episode_id: u64,
    },
    TerminalInterruptedOpenGroup {
        episode_id: u64,
        physical_decision_id: u64,
    },
    TerminalLearnerActionCountMismatch {
        episode_id: u64,
        expected: u64,
        actual: u64,
    },
    TerminalLearnerTraceMismatch {
        episode_id: u64,
    },
    NonNaturalTerminal {
        episode_id: u64,
    },
    TerminalTupleMismatch {
        episode_id: u64,
    },
    LearnerRewardMismatch {
        episode_id: u64,
        learner_seat: PlayerSeatV1,
    },
    LearnerPhysicalDecisionOutOfRange {
        episode_id: u64,
        physical_decision_id: u64,
        terminal_physical_decision_count: u64,
    },
    LearnerPolicyStepOutOfRange {
        episode_id: u64,
        policy_step: u64,
        terminal_policy_step_count: u64,
    },
    TerminalPolicyStepCountUnderflow {
        episode_id: u64,
    },
    TerminalPhysicalDecisionCountUnderflow {
        episode_id: u64,
    },
    OpponentPolicyStepsBelowPhysicalDecisions {
        episode_id: u64,
        opponent_policy_step_count: u64,
        opponent_physical_decision_count: u64,
    },
    MissingTerminal {
        episode_id: u64,
    },
}

pub(crate) struct FlatSelectedSampleCore<'a, B> {
    pub(crate) expected: FastActorDecisionV1,
    pub(crate) binding: B,
    pub(crate) binding_matches: bool,
    pub(crate) learner_ordinal: u64,
    pub(crate) action_seed: u64,
    pub(crate) selected_index: u32,
    pub(crate) raw_action_logits: &'a [f32],
    pub(crate) scorer_action_count: usize,
    pub(crate) predicted_value_bits: u32,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct FlatTerminalSampleCore {
    pub(crate) terminal: AsyncRolloutTerminalV1,
    pub(crate) learner_action_count: u64,
    pub(crate) learner_trace_hash: u64,
}

#[derive(Debug)]
struct OpenPhysicalDecisionCore<B, I> {
    episode_id: u64,
    physical_decision_id: u64,
    acting_player: PlayerSeatV1,
    first_learner_ordinal: u64,
    substep_count: u32,
    next_substep_index: u32,
    last_policy_step: u64,
    joint_selected_log_probability: f32,
    value_bits: u32,
    substeps: Vec<FlatLearnerSubstepSampleCore<B, I>>,
}

impl<B, I> OpenPhysicalDecisionCore<B, I> {
    fn complete(self) -> FlatPhysicalDecisionSampleCore<B, I> {
        FlatPhysicalDecisionSampleCore {
            episode_id: self.episode_id,
            physical_decision_id: self.physical_decision_id,
            acting_player: self.acting_player,
            first_learner_ordinal: self.first_learner_ordinal,
            substep_count: self.substep_count,
            joint_selected_log_probability_bits: self.joint_selected_log_probability.to_bits(),
            value_bits: self.value_bits,
            substeps: self.substeps,
        }
    }
}

#[derive(Debug)]
struct EpisodeAssemblyCore<B, I> {
    next_learner_ordinal: u64,
    learner_trace_hash: u64,
    last_completed_physical_decision_id: Option<u64>,
    last_learner_policy_step: Option<u64>,
    open_group: Option<OpenPhysicalDecisionCore<B, I>>,
    groups: Vec<FlatPhysicalDecisionSampleCore<B, I>>,
    completed: Option<FlatGroupedEpisodeCore<B, I>>,
}

impl<B, I> EpisodeAssemblyCore<B, I> {
    fn new(episode_id: u64) -> Self {
        Self {
            next_learner_ordinal: 0,
            learner_trace_hash: initial_learner_trace_hash_v1(episode_id),
            last_completed_physical_decision_id: None,
            last_learner_policy_step: None,
            open_group: None,
            groups: Vec::new(),
            completed: None,
        }
    }
}

#[derive(Debug)]
pub(crate) struct FlatPhysicalTrajectoryObserverCore<B, I> {
    learner_seat: PlayerSeatV1,
    first_episode_id: u64,
    end_episode_id: u64,
    episode_count: u64,
    episodes: BTreeMap<u64, EpisodeAssemblyCore<B, I>>,
    poisoned_error: Option<FlatPhysicalTrajectoryErrorCore>,
}

impl<B, I> FlatPhysicalTrajectoryObserverCore<B, I> {
    pub(crate) fn new(
        learner_seat: PlayerSeatV1,
        first_episode_id: u64,
        episode_count: u64,
    ) -> Result<Self, FlatPhysicalTrajectoryErrorCore> {
        let end_episode_id = first_episode_id
            .checked_add(episode_count)
            .ok_or(FlatPhysicalTrajectoryErrorCore::ExpectedEpisodeRangeOverflow)?;
        usize::try_from(episode_count)
            .map_err(|_| FlatPhysicalTrajectoryErrorCore::EpisodeCountExceedsAddressSpace)?;
        Ok(Self {
            learner_seat,
            first_episode_id,
            end_episode_id,
            episode_count,
            episodes: BTreeMap::new(),
            poisoned_error: None,
        })
    }

    fn validate_episode_id(&self, episode_id: u64) -> Result<(), FlatPhysicalTrajectoryErrorCore> {
        if (self.first_episode_id..self.end_episode_id).contains(&episode_id) {
            Ok(())
        } else {
            Err(FlatPhysicalTrajectoryErrorCore::EpisodeOutOfRange { episode_id })
        }
    }

    pub(crate) fn observe_selected<F>(
        &mut self,
        event: FlatSelectedSampleCore<'_, B>,
        make_scoring_inputs: F,
    ) -> Result<(), FlatPhysicalTrajectoryErrorCore>
    where
        F: FnOnce() -> I,
    {
        if let Some(error) = self.poisoned_error {
            return Err(error);
        }
        let result = self.observe_selected_unpoisoned(event, make_scoring_inputs);
        if let Err(error) = result {
            self.poisoned_error = Some(error);
        }
        result
    }

    fn observe_selected_unpoisoned<F>(
        &mut self,
        event: FlatSelectedSampleCore<'_, B>,
        make_scoring_inputs: F,
    ) -> Result<(), FlatPhysicalTrajectoryErrorCore>
    where
        F: FnOnce() -> I,
    {
        let expected = event.expected;
        self.validate_episode_id(expected.episode_id)?;
        if expected.acting_player != self.learner_seat {
            return Err(FlatPhysicalTrajectoryErrorCore::SelectedActorMismatch {
                episode_id: expected.episode_id,
                expected: self.learner_seat,
                actual: expected.acting_player,
            });
        }
        if !event.binding_matches {
            return Err(FlatPhysicalTrajectoryErrorCore::SelectedBindingMismatch {
                episode_id: expected.episode_id,
                learner_ordinal: event.learner_ordinal,
            });
        }
        if event.raw_action_logits.len()
            != usize::try_from(expected.legal_action_count).unwrap_or(usize::MAX)
            || event.scorer_action_count != event.raw_action_logits.len()
        {
            return Err(FlatPhysicalTrajectoryErrorCore::LegalActionCountMismatch {
                episode_id: expected.episode_id,
                learner_ordinal: event.learner_ordinal,
            });
        }
        let selected_index = usize::try_from(event.selected_index).map_err(|_| {
            FlatPhysicalTrajectoryErrorCore::SelectedIndexOutOfRange {
                episode_id: expected.episode_id,
                learner_ordinal: event.learner_ordinal,
            }
        })?;
        if selected_index >= event.raw_action_logits.len() {
            return Err(FlatPhysicalTrajectoryErrorCore::SelectedIndexOutOfRange {
                episode_id: expected.episode_id,
                learner_ordinal: event.learner_ordinal,
            });
        }
        let selected_log_probability = selected_log_probability(
            expected.episode_id,
            event.learner_ordinal,
            selected_index,
            event.raw_action_logits,
        )?;
        if !f32::from_bits(event.predicted_value_bits).is_finite() {
            return Err(FlatPhysicalTrajectoryErrorCore::NonFinitePredictedValue {
                episode_id: expected.episode_id,
                learner_ordinal: event.learner_ordinal,
                bits: event.predicted_value_bits,
            });
        }
        let owned_substep = FlatLearnerSubstepSampleCore {
            expected,
            binding: event.binding,
            learner_ordinal: event.learner_ordinal,
            action_seed: event.action_seed,
            selected_index: event.selected_index,
            raw_action_logit_bits: event
                .raw_action_logits
                .iter()
                .map(|value| value.to_bits())
                .collect(),
            selected_log_probability_bits: selected_log_probability.to_bits(),
            predicted_value_bits: event.predicted_value_bits,
            scoring_inputs: make_scoring_inputs(),
        };

        let episode = self
            .episodes
            .entry(expected.episode_id)
            .or_insert_with(|| EpisodeAssemblyCore::new(expected.episode_id));
        if episode.completed.is_some() {
            return Err(FlatPhysicalTrajectoryErrorCore::SelectedAfterTerminal {
                episode_id: expected.episode_id,
            });
        }
        if event.learner_ordinal != episode.next_learner_ordinal {
            return Err(FlatPhysicalTrajectoryErrorCore::LearnerOrdinalMismatch {
                episode_id: expected.episode_id,
                expected: episode.next_learner_ordinal,
                actual: event.learner_ordinal,
            });
        }
        if expected.substep_count == 0 {
            return Err(FlatPhysicalTrajectoryErrorCore::ZeroSubstepCount {
                episode_id: expected.episode_id,
                physical_decision_id: expected.physical_decision_id,
            });
        }

        let next_learner_ordinal = episode
            .next_learner_ordinal
            .checked_add(1)
            .ok_or(FlatPhysicalTrajectoryErrorCore::CountOverflow)?;
        let next_trace_hash =
            record_learner_trace_v1(episode.learner_trace_hash, expected, event.selected_index);

        match episode.open_group.as_mut() {
            None => {
                if expected.substep_index != 0 {
                    return Err(FlatPhysicalTrajectoryErrorCore::FirstSubstepNotZero {
                        episode_id: expected.episode_id,
                        physical_decision_id: expected.physical_decision_id,
                        actual: expected.substep_index,
                    });
                }
                if let Some(previous) = episode.last_completed_physical_decision_id {
                    if expected.physical_decision_id <= previous {
                        return Err(
                            FlatPhysicalTrajectoryErrorCore::PhysicalDecisionNotStrictlyIncreasing {
                                episode_id: expected.episode_id,
                                previous,
                                actual: expected.physical_decision_id,
                            },
                        );
                    }
                }
                if let Some(previous) = episode.last_learner_policy_step {
                    if expected.step <= previous {
                        return Err(
                            FlatPhysicalTrajectoryErrorCore::PolicyStepNotStrictlyIncreasing {
                                episode_id: expected.episode_id,
                                previous,
                                actual: expected.step,
                            },
                        );
                    }
                }
                episode.open_group = Some(OpenPhysicalDecisionCore {
                    episode_id: expected.episode_id,
                    physical_decision_id: expected.physical_decision_id,
                    acting_player: expected.acting_player,
                    first_learner_ordinal: event.learner_ordinal,
                    substep_count: expected.substep_count,
                    next_substep_index: 1,
                    last_policy_step: expected.step,
                    joint_selected_log_probability: selected_log_probability,
                    value_bits: event.predicted_value_bits,
                    substeps: vec![owned_substep],
                });
            }
            Some(open) => {
                if expected.physical_decision_id != open.physical_decision_id {
                    return Err(FlatPhysicalTrajectoryErrorCore::OpenGroupKeyMismatch {
                        episode_id: expected.episode_id,
                        expected_physical_decision_id: open.physical_decision_id,
                        actual_physical_decision_id: expected.physical_decision_id,
                    });
                }
                if expected.acting_player != open.acting_player {
                    return Err(FlatPhysicalTrajectoryErrorCore::OpenGroupActorMismatch {
                        episode_id: expected.episode_id,
                        physical_decision_id: expected.physical_decision_id,
                    });
                }
                if expected.substep_count != open.substep_count {
                    return Err(FlatPhysicalTrajectoryErrorCore::SubstepCountMismatch {
                        episode_id: expected.episode_id,
                        physical_decision_id: expected.physical_decision_id,
                        expected: open.substep_count,
                        actual: expected.substep_count,
                    });
                }
                if expected.substep_index != open.next_substep_index {
                    return Err(FlatPhysicalTrajectoryErrorCore::SubstepIndexMismatch {
                        episode_id: expected.episode_id,
                        physical_decision_id: expected.physical_decision_id,
                        expected: open.next_substep_index,
                        actual: expected.substep_index,
                    });
                }
                let next_policy_step = open
                    .last_policy_step
                    .checked_add(1)
                    .ok_or(FlatPhysicalTrajectoryErrorCore::CountOverflow)?;
                if expected.step != next_policy_step {
                    return Err(
                        FlatPhysicalTrajectoryErrorCore::PolicyStepNotContiguousWithinGroup {
                            episode_id: expected.episode_id,
                            physical_decision_id: expected.physical_decision_id,
                            expected: next_policy_step,
                            actual: expected.step,
                        },
                    );
                }
                let joint = open.joint_selected_log_probability + selected_log_probability;
                if !joint.is_finite() {
                    return Err(
                        FlatPhysicalTrajectoryErrorCore::JointLogProbabilityOverflow {
                            episode_id: expected.episode_id,
                            physical_decision_id: expected.physical_decision_id,
                        },
                    );
                }
                open.joint_selected_log_probability = joint;
                open.next_substep_index = open
                    .next_substep_index
                    .checked_add(1)
                    .ok_or(FlatPhysicalTrajectoryErrorCore::CountOverflow)?;
                open.last_policy_step = expected.step;
                open.substeps.push(owned_substep);
            }
        }

        episode.next_learner_ordinal = next_learner_ordinal;
        episode.learner_trace_hash = next_trace_hash;
        episode.last_learner_policy_step = Some(expected.step);
        let group_complete = episode
            .open_group
            .as_ref()
            .is_some_and(|open| open.next_substep_index == open.substep_count);
        if group_complete {
            let complete = episode
                .open_group
                .take()
                .expect("group completion was checked above")
                .complete();
            episode.last_completed_physical_decision_id = Some(complete.physical_decision_id);
            episode.groups.push(complete);
        }
        Ok(())
    }

    pub(crate) fn observe_terminal(
        &mut self,
        event: FlatTerminalSampleCore,
    ) -> Result<(), FlatPhysicalTrajectoryErrorCore> {
        if let Some(error) = self.poisoned_error {
            return Err(error);
        }
        let result = self.observe_terminal_unpoisoned(event);
        if let Err(error) = result {
            self.poisoned_error = Some(error);
        }
        result
    }

    fn observe_terminal_unpoisoned(
        &mut self,
        event: FlatTerminalSampleCore,
    ) -> Result<(), FlatPhysicalTrajectoryErrorCore> {
        let terminal = event.terminal;
        self.validate_episode_id(terminal.episode_id)?;
        let episode = self
            .episodes
            .entry(terminal.episode_id)
            .or_insert_with(|| EpisodeAssemblyCore::new(terminal.episode_id));
        if episode.completed.is_some() {
            return Err(FlatPhysicalTrajectoryErrorCore::DuplicateTerminal {
                episode_id: terminal.episode_id,
            });
        }
        if let Some(open) = episode.open_group.as_ref() {
            return Err(
                FlatPhysicalTrajectoryErrorCore::TerminalInterruptedOpenGroup {
                    episode_id: terminal.episode_id,
                    physical_decision_id: open.physical_decision_id,
                },
            );
        }
        if event.learner_action_count != episode.next_learner_ordinal {
            return Err(
                FlatPhysicalTrajectoryErrorCore::TerminalLearnerActionCountMismatch {
                    episode_id: terminal.episode_id,
                    expected: episode.next_learner_ordinal,
                    actual: event.learner_action_count,
                },
            );
        }
        if event.learner_trace_hash != episode.learner_trace_hash {
            return Err(
                FlatPhysicalTrajectoryErrorCore::TerminalLearnerTraceMismatch {
                    episode_id: terminal.episode_id,
                },
            );
        }
        if terminal.terminal_classification != TerminalClassificationV1::Natural
            || terminal.terminal_code != TerminalSafeCodeV2::NaturalGameOver
        {
            return Err(FlatPhysicalTrajectoryErrorCore::NonNaturalTerminal {
                episode_id: terminal.episode_id,
            });
        }
        let (expected_winner, expected_reward) = match terminal.terminal_outcome {
            TerminalOutcomeV1::P0Win => (Some(PlayerSeatV1::P0), [1, -1]),
            TerminalOutcomeV1::P1Win => (Some(PlayerSeatV1::P1), [-1, 1]),
            TerminalOutcomeV1::Draw => (None, [0, 0]),
            TerminalOutcomeV1::Truncated | TerminalOutcomeV1::Halted => {
                return Err(FlatPhysicalTrajectoryErrorCore::TerminalTupleMismatch {
                    episode_id: terminal.episode_id,
                });
            }
        };
        if terminal.winner != expected_winner || terminal.terminal_reward != expected_reward {
            return Err(FlatPhysicalTrajectoryErrorCore::TerminalTupleMismatch {
                episode_id: terminal.episode_id,
            });
        }
        let learner_return = match (self.learner_seat, terminal.winner) {
            (_, None) => 0,
            (learner, Some(winner)) if learner == winner => 1,
            _ => -1,
        };
        let learner_reward_index = match self.learner_seat {
            PlayerSeatV1::P0 => 0,
            PlayerSeatV1::P1 => 1,
        };
        if terminal.terminal_reward[learner_reward_index] != learner_return {
            return Err(FlatPhysicalTrajectoryErrorCore::LearnerRewardMismatch {
                episode_id: terminal.episode_id,
                learner_seat: self.learner_seat,
            });
        }
        if let Some(invalid_group) = episode
            .groups
            .iter()
            .find(|group| group.physical_decision_id >= terminal.physical_decision_count)
        {
            return Err(
                FlatPhysicalTrajectoryErrorCore::LearnerPhysicalDecisionOutOfRange {
                    episode_id: terminal.episode_id,
                    physical_decision_id: invalid_group.physical_decision_id,
                    terminal_physical_decision_count: terminal.physical_decision_count,
                },
            );
        }
        let learner_group_count = u64::try_from(episode.groups.len())
            .map_err(|_| FlatPhysicalTrajectoryErrorCore::CountOverflow)?;
        let opponent_policy_step_count = terminal
            .policy_step_count
            .checked_sub(episode.next_learner_ordinal)
            .ok_or(
                FlatPhysicalTrajectoryErrorCore::TerminalPolicyStepCountUnderflow {
                    episode_id: terminal.episode_id,
                },
            )?;
        let opponent_physical_decision_count = terminal
            .physical_decision_count
            .checked_sub(learner_group_count)
            .ok_or(
                FlatPhysicalTrajectoryErrorCore::TerminalPhysicalDecisionCountUnderflow {
                    episode_id: terminal.episode_id,
                },
            )?;
        if let Some(invalid_substep) = episode
            .groups
            .iter()
            .flat_map(|group| group.substeps.iter())
            .find(|substep| substep.expected.step >= terminal.policy_step_count)
        {
            return Err(
                FlatPhysicalTrajectoryErrorCore::LearnerPolicyStepOutOfRange {
                    episode_id: terminal.episode_id,
                    policy_step: invalid_substep.expected.step,
                    terminal_policy_step_count: terminal.policy_step_count,
                },
            );
        }
        if opponent_policy_step_count < opponent_physical_decision_count {
            return Err(
                FlatPhysicalTrajectoryErrorCore::OpponentPolicyStepsBelowPhysicalDecisions {
                    episode_id: terminal.episode_id,
                    opponent_policy_step_count,
                    opponent_physical_decision_count,
                },
            );
        }
        let groups = std::mem::take(&mut episode.groups);
        episode.completed = Some(FlatGroupedEpisodeCore {
            episode_id: terminal.episode_id,
            learner_seat: self.learner_seat,
            learner_return,
            learner_policy_step_count: episode.next_learner_ordinal,
            opponent_policy_step_count,
            learner_physical_decision_count: learner_group_count,
            opponent_physical_decision_count,
            learner_trace_hash: episode.learner_trace_hash,
            terminal,
            groups,
        });
        Ok(())
    }

    pub(crate) fn finish(
        self,
    ) -> Result<FlatGroupedTrajectoryBatchCore<B, I>, FlatPhysicalTrajectoryErrorCore> {
        if let Some(error) = self.poisoned_error {
            return Err(error);
        }
        let capacity = usize::try_from(self.episode_count)
            .map_err(|_| FlatPhysicalTrajectoryErrorCore::EpisodeCountExceedsAddressSpace)?;
        let mut episodes = Vec::new();
        episodes
            .try_reserve_exact(capacity)
            .map_err(|_| FlatPhysicalTrajectoryErrorCore::ResultAllocationFailed)?;
        let mut assemblies = self.episodes;
        let mut learner_policy_step_count = 0u64;
        let mut learner_physical_decision_count = 0u64;
        for episode_id in self.first_episode_id..self.end_episode_id {
            let mut assembly = assemblies
                .remove(&episode_id)
                .ok_or(FlatPhysicalTrajectoryErrorCore::MissingTerminal { episode_id })?;
            let episode = assembly
                .completed
                .take()
                .ok_or(FlatPhysicalTrajectoryErrorCore::MissingTerminal { episode_id })?;
            learner_policy_step_count = learner_policy_step_count
                .checked_add(episode.learner_policy_step_count)
                .ok_or(FlatPhysicalTrajectoryErrorCore::CountOverflow)?;
            learner_physical_decision_count = learner_physical_decision_count
                .checked_add(episode.learner_physical_decision_count)
                .ok_or(FlatPhysicalTrajectoryErrorCore::CountOverflow)?;
            episodes.push(episode);
        }
        let update_staging = if learner_physical_decision_count == 0 {
            FlatPhysicalUpdateStagingCore::NoUpdateZeroLearnerGroups
        } else {
            FlatPhysicalUpdateStagingCore::Ready {
                learner_group_count: learner_physical_decision_count,
            }
        };
        Ok(FlatGroupedTrajectoryBatchCore {
            learner_seat: self.learner_seat,
            first_episode_id: self.first_episode_id,
            episode_count: self.episode_count,
            learner_policy_step_count,
            learner_physical_decision_count,
            update_staging,
            episodes,
        })
    }

    #[cfg(test)]
    pub(crate) fn duplicate_first_group_for_test(&mut self, episode_id: u64)
    where
        B: Clone,
        I: Clone,
    {
        let assembly = self
            .episodes
            .get_mut(&episode_id)
            .expect("test episode assembly must exist");
        let duplicate = assembly
            .groups
            .first()
            .expect("test episode must contain a completed group")
            .clone();
        assembly.groups.push(duplicate);
    }
}

pub(crate) fn selected_log_probability(
    episode_id: u64,
    learner_ordinal: u64,
    selected_index: usize,
    logits: &[f32],
) -> Result<f32, FlatPhysicalTrajectoryErrorCore> {
    let mut maximum = f32::NEG_INFINITY;
    for (action_index, &logit) in logits.iter().enumerate() {
        if !logit.is_finite() {
            return Err(FlatPhysicalTrajectoryErrorCore::NonFiniteLogit {
                episode_id,
                learner_ordinal,
                action_index: u32::try_from(action_index).unwrap_or(u32::MAX),
                bits: logit.to_bits(),
            });
        }
        maximum = maximum.max(logit);
    }
    let mut denominator = 0.0f32;
    for &logit in logits {
        denominator += (logit - maximum).exp();
    }
    let result = (logits[selected_index] - maximum) - denominator.ln();
    if result.is_finite() {
        Ok(result)
    } else {
        Err(
            FlatPhysicalTrajectoryErrorCore::NonFiniteSelectedLogProbability {
                episode_id,
                learner_ordinal,
            },
        )
    }
}

pub(crate) fn player_seat_code(seat: PlayerSeatV1) -> u8 {
    match seat {
        PlayerSeatV1::P0 => 0,
        PlayerSeatV1::P1 => 1,
    }
}

pub(crate) fn decision_kind_code(kind: FastActorDecisionKindV1) -> u8 {
    match kind {
        FastActorDecisionKindV1::Surface => 0,
        FastActorDecisionKindV1::AttackerInclusion => 1,
        FastActorDecisionKindV1::BlockerInclusion => 2,
    }
}
