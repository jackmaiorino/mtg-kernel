//! Crate-private, inert physical-decision trajectory staging for the native
//! trainer. This module publishes no schema, performs no tensorization or
//! update, and deliberately owns every borrowed observer row before the
//! scored-rollout core can release it.
//!
//! This stages the terminal-REINFORCE/value-v3 *grouping shape* only. It does
//! not claim Python trajectory parity: this Rust rollout currently derives
//! action seeds from the per-substep learner ordinal and fixes one learner seat
//! for the batch, while the Python v3 trainer derives from physical-group
//! ordinal plus substep and alternates learner seats by episode.

#![allow(dead_code)]

use crate::async_flat_scored_rollout_v1::{
    FlatScoredSelectedEventV1, FlatScoredTerminalEventV1, FlatScoredTrajectoryObserverV1,
};
#[cfg(test)]
use crate::async_rollout::AsyncRolloutTerminalV1;
use crate::flat_policy_v1::{
    FlatCompletedDungeonV1, FlatContextPathElementV1, FlatEffectSubtypeChangeV1, FlatGlobalsV1,
    FlatObjectAbilityUseV1, FlatObjectCoreV1, FlatObjectGoadV1, FlatObjectSubtypeV1,
    FlatRelationV1, FlatScorerActionCoreV1, FlatScorerActionRefV1, FlatScoringDecisionViewV1,
};
use crate::private_physical_trajectory_core::{
    decision_kind_code as core_decision_kind_code, player_seat_code as core_player_seat_code,
    selected_log_probability as core_selected_log_probability, FlatGroupedEpisodeCore,
    FlatGroupedTrajectoryBatchCore, FlatLearnerSubstepSampleCore, FlatPhysicalDecisionSampleCore,
    FlatPhysicalTrajectoryErrorCore, FlatPhysicalTrajectoryObserverCore,
    FlatPhysicalUpdateStagingCore, FlatSelectedSampleCore, FlatTerminalSampleCore,
};
use crate::rl::PlayerSeatV1;
#[cfg(test)]
use crate::rl::{TerminalClassificationV1, TerminalOutcomeV1, TerminalSafeCodeV2};
use crate::rl_session::FastActorDecisionKindV1;
#[cfg(test)]
use crate::rl_session::FastActorDecisionV1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FlatOwnedScoringInputsV1 {
    pub(crate) globals: FlatGlobalsV1,
    pub(crate) objects: Vec<FlatObjectCoreV1>,
    pub(crate) relations: Vec<FlatRelationV1>,
    pub(crate) object_subtypes: Vec<FlatObjectSubtypeV1>,
    pub(crate) ability_uses: Vec<FlatObjectAbilityUseV1>,
    pub(crate) goads: Vec<FlatObjectGoadV1>,
    pub(crate) completed_dungeons: Vec<FlatCompletedDungeonV1>,
    pub(crate) effect_subtype_changes: Vec<FlatEffectSubtypeChangeV1>,
    pub(crate) context_path_elements: Vec<FlatContextPathElementV1>,
    pub(crate) actions: Vec<FlatScorerActionCoreV1>,
    pub(crate) action_refs: Vec<FlatScorerActionRefV1>,
}

impl FlatOwnedScoringInputsV1 {
    fn copy_from_view(decision: FlatScoringDecisionViewV1<'_>) -> Self {
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

pub(crate) type FlatLearnerSubstepSampleV1 = FlatLearnerSubstepSampleCore<
    crate::flat_policy_v1::FlatDecisionBindingV1,
    FlatOwnedScoringInputsV1,
>;
pub(crate) type FlatPhysicalDecisionSampleV1 = FlatPhysicalDecisionSampleCore<
    crate::flat_policy_v1::FlatDecisionBindingV1,
    FlatOwnedScoringInputsV1,
>;
pub(crate) type FlatGroupedEpisodeV1 =
    FlatGroupedEpisodeCore<crate::flat_policy_v1::FlatDecisionBindingV1, FlatOwnedScoringInputsV1>;
pub(crate) type FlatPhysicalUpdateStagingV1 = FlatPhysicalUpdateStagingCore;
pub(crate) type FlatGroupedTrajectoryBatchV1 = FlatGroupedTrajectoryBatchCore<
    crate::flat_policy_v1::FlatDecisionBindingV1,
    FlatOwnedScoringInputsV1,
>;
pub(crate) type FlatPhysicalTrajectoryErrorV1 = FlatPhysicalTrajectoryErrorCore;

#[derive(Debug)]
pub(crate) struct FlatPhysicalTrajectoryObserverV1 {
    core: FlatPhysicalTrajectoryObserverCore<
        crate::flat_policy_v1::FlatDecisionBindingV1,
        FlatOwnedScoringInputsV1,
    >,
}

impl FlatPhysicalTrajectoryObserverV1 {
    pub(crate) fn new(
        learner_seat: PlayerSeatV1,
        first_episode_id: u64,
        episode_count: u64,
    ) -> Result<Self, FlatPhysicalTrajectoryErrorV1> {
        Ok(Self {
            core: FlatPhysicalTrajectoryObserverCore::new(
                learner_seat,
                first_episode_id,
                episode_count,
            )?,
        })
    }

    #[cfg(test)]
    fn duplicate_first_group_for_test(&mut self, episode_id: u64) {
        self.core.duplicate_first_group_for_test(episode_id);
    }
}

impl FlatScoredTrajectoryObserverV1 for FlatPhysicalTrajectoryObserverV1 {
    type Error = FlatPhysicalTrajectoryErrorV1;
    type Output = FlatGroupedTrajectoryBatchV1;

    fn observe_selected_v1(
        &mut self,
        event: FlatScoredSelectedEventV1<'_>,
    ) -> Result<(), Self::Error> {
        let binding_matches = selected_binding_matches(&event);
        let decision = event.decision;
        self.core.observe_selected(
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
            || FlatOwnedScoringInputsV1::copy_from_view(decision),
        )
    }

    fn observe_terminal_v1(&mut self, event: FlatScoredTerminalEventV1) -> Result<(), Self::Error> {
        self.core.observe_terminal(FlatTerminalSampleCore {
            terminal: event.terminal,
            learner_action_count: event.learner_action_count,
            learner_trace_hash: event.learner_trace_hash,
        })
    }

    fn finish_v1(self) -> Result<Self::Output, Self::Error> {
        self.core.finish()
    }
}

fn selected_binding_matches(event: &FlatScoredSelectedEventV1<'_>) -> bool {
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

fn selected_log_probability(
    episode_id: u64,
    learner_ordinal: u64,
    selected_index: usize,
    logits: &[f32],
) -> Result<f32, FlatPhysicalTrajectoryErrorV1> {
    core_selected_log_probability(episode_id, learner_ordinal, selected_index, logits)
}

fn player_seat_code(seat: PlayerSeatV1) -> u8 {
    core_player_seat_code(seat)
}

fn decision_kind_code(kind: FastActorDecisionKindV1) -> u8 {
    core_decision_kind_code(kind)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::async_flat_scored_rollout_v1::{
        initial_learner_trace_hash_v1, record_learner_trace_v1,
        run_async_flat_scored_rollout_observed_v1, AsyncFlatScoredObservedRunErrorV1,
        FlatBatchScorerErrorV1, FlatBatchScorerV1, FlatScoredObserverPhaseV1,
        FlatScoringBatchViewV1, ASYNC_FLAT_SCORED_TEST_LOCK_V1,
    };
    use crate::async_rollout_v2::AsyncRolloutConfigV2;
    use crate::flat_policy_v1::{FlatDecisionBindingV1, FlatScoringDecisionViewV1};
    use std::time::Duration;

    type BindingMutatorV1 = Box<dyn Fn(&mut FastActorDecisionV1, &mut FlatDecisionBindingV1)>;
    type SelectedMutationCaseV1 = (
        SelectedSpecV1,
        BindingMutatorV1,
        FlatPhysicalTrajectoryErrorV1,
    );

    #[derive(Clone)]
    struct SelectedSpecV1 {
        episode_id: u64,
        step: u64,
        environment_revision: u64,
        physical_decision_id: u64,
        substep_index: u32,
        substep_count: u32,
        acting_player: PlayerSeatV1,
        decision_kind: FastActorDecisionKindV1,
        learner_ordinal: u64,
        action_seed: u64,
        selected_index: u32,
        logits: Vec<f32>,
        predicted_value_bits: u32,
    }

    impl SelectedSpecV1 {
        fn new(
            episode_id: u64,
            physical_decision_id: u64,
            substep_index: u32,
            substep_count: u32,
            learner_ordinal: u64,
        ) -> Self {
            Self {
                episode_id,
                step: learner_ordinal,
                environment_revision: learner_ordinal + 100,
                physical_decision_id,
                substep_index,
                substep_count,
                acting_player: PlayerSeatV1::P0,
                decision_kind: FastActorDecisionKindV1::Surface,
                learner_ordinal,
                action_seed: 0xa5a5_0000 + learner_ordinal,
                selected_index: 0,
                logits: vec![0.75, -0.25],
                predicted_value_bits: (0.125f32 + learner_ordinal as f32 / 16.0).to_bits(),
            }
        }

        fn expected(&self) -> FastActorDecisionV1 {
            FastActorDecisionV1 {
                episode_id: self.episode_id,
                step: self.step,
                environment_revision: self.environment_revision,
                physical_decision_id: self.physical_decision_id,
                substep_index: self.substep_index,
                substep_count: self.substep_count,
                acting_player: self.acting_player,
                decision_kind: self.decision_kind,
                legal_action_count: u32::try_from(self.logits.len()).unwrap(),
            }
        }

        fn binding(&self) -> FlatDecisionBindingV1 {
            let expected = self.expected();
            let mut binding = FlatDecisionBindingV1::default();
            binding.action_binding.episode_id = expected.episode_id;
            binding.action_binding.environment_revision = expected.environment_revision;
            binding.action_binding.bound_policy_step_count = expected.step;
            binding.action_binding.physical_decision_id = expected.physical_decision_id;
            binding.action_binding.bound_physical_decision_count = expected.physical_decision_id;
            binding.action_binding.substep_index = expected.substep_index;
            binding.action_binding.substep_count = expected.substep_count;
            binding.action_binding.acting_player = player_seat_code(expected.acting_player);
            binding.action_binding.decision_kind = decision_kind_code(expected.decision_kind);
            binding.action_binding.legal_action_count = expected.legal_action_count;
            binding
        }
    }

    fn observe_selected(
        observer: &mut FlatPhysicalTrajectoryObserverV1,
        spec: &SelectedSpecV1,
    ) -> Result<(), FlatPhysicalTrajectoryErrorV1> {
        observe_selected_with(observer, spec, |_, _| {})
    }

    fn observe_selected_with(
        observer: &mut FlatPhysicalTrajectoryObserverV1,
        spec: &SelectedSpecV1,
        mutate: impl FnOnce(&mut FastActorDecisionV1, &mut FlatDecisionBindingV1),
    ) -> Result<(), FlatPhysicalTrajectoryErrorV1> {
        let mut expected = spec.expected();
        let mut binding = spec.binding();
        mutate(&mut expected, &mut binding);
        let globals = FlatGlobalsV1::default();
        let actions = vec![FlatScorerActionCoreV1::default(); spec.logits.len()];
        let decision = FlatScoringDecisionViewV1::new(
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
        observer.observe_selected_v1(FlatScoredSelectedEventV1 {
            expected,
            binding,
            learner_ordinal: spec.learner_ordinal,
            action_seed: spec.action_seed,
            selected_index: spec.selected_index,
            raw_action_logits: &spec.logits,
            predicted_value_bits: spec.predicted_value_bits,
            decision,
        })
    }

    fn natural_terminal(
        episode_id: u64,
        learner_actions: &[SelectedSpecV1],
        policy_step_count: u64,
        physical_decision_count: u64,
        outcome: TerminalOutcomeV1,
    ) -> FlatScoredTerminalEventV1 {
        let (winner, terminal_reward) = match outcome {
            TerminalOutcomeV1::P0Win => (Some(PlayerSeatV1::P0), [1, -1]),
            TerminalOutcomeV1::P1Win => (Some(PlayerSeatV1::P1), [-1, 1]),
            TerminalOutcomeV1::Draw => (None, [0, 0]),
            TerminalOutcomeV1::Truncated | TerminalOutcomeV1::Halted => (None, [0, 0]),
        };
        let learner_trace_hash = learner_actions.iter().fold(
            initial_learner_trace_hash_v1(episode_id),
            |trace, action| {
                record_learner_trace_v1(trace, action.expected(), action.selected_index)
            },
        );
        FlatScoredTerminalEventV1 {
            terminal: AsyncRolloutTerminalV1 {
                episode_id,
                terminal_outcome: outcome,
                terminal_classification: TerminalClassificationV1::Natural,
                terminal_code: TerminalSafeCodeV2::NaturalGameOver,
                winner,
                terminal_reward,
                policy_step_count,
                physical_decision_count,
            },
            learner_action_count: u64::try_from(learner_actions.len()).unwrap(),
            learner_trace_hash,
            native_full_trajectory_receipt: None,
        }
    }

    fn assert_episode_arithmetic_invariants(episode: &FlatGroupedEpisodeV1) {
        assert_eq!(
            episode
                .learner_policy_step_count
                .checked_add(episode.opponent_policy_step_count),
            Some(episode.terminal.policy_step_count)
        );
        assert_eq!(
            episode
                .learner_physical_decision_count
                .checked_add(episode.opponent_physical_decision_count),
            Some(episode.terminal.physical_decision_count)
        );
        assert!(episode.opponent_policy_step_count >= episode.opponent_physical_decision_count);
        assert_eq!(
            episode
                .groups
                .iter()
                .map(|group| u64::from(group.substep_count))
                .sum::<u64>(),
            episode.learner_policy_step_count
        );
        assert_eq!(
            u64::try_from(episode.groups.len()).unwrap(),
            episode.learner_physical_decision_count
        );
        assert!(episode
            .groups
            .windows(2)
            .all(|groups| groups[0].physical_decision_id < groups[1].physical_decision_id));

        let substeps = episode
            .groups
            .iter()
            .flat_map(|group| {
                assert!(group.physical_decision_id < episode.terminal.physical_decision_count);
                assert_eq!(
                    usize::try_from(group.substep_count).unwrap(),
                    group.substeps.len()
                );
                group
                    .substeps
                    .iter()
                    .enumerate()
                    .map(move |(index, substep)| {
                        assert_eq!(substep.expected.episode_id, episode.episode_id);
                        assert_eq!(substep.expected.acting_player, episode.learner_seat);
                        assert_eq!(
                            substep.expected.physical_decision_id,
                            group.physical_decision_id
                        );
                        assert_eq!(substep.expected.substep_count, group.substep_count);
                        assert_eq!(
                            substep.expected.substep_index,
                            u32::try_from(index).unwrap()
                        );
                        assert!(substep.expected.step < episode.terminal.policy_step_count);
                        substep.expected
                    })
            })
            .collect::<Vec<_>>();
        assert!(substeps.windows(2).all(|steps| {
            if steps[0].physical_decision_id == steps[1].physical_decision_id {
                steps[0].step.checked_add(1) == Some(steps[1].step)
            } else {
                steps[0].step < steps[1].step
            }
        }));
    }

    #[test]
    fn groups_interleaved_episodes_with_gaps_and_first_value_only() {
        let mut observer = FlatPhysicalTrajectoryObserverV1::new(PlayerSeatV1::P0, 10, 2).unwrap();
        let mut first = SelectedSpecV1::new(10, 2, 0, 3, 0);
        first.logits = vec![1.0, 0.0, -1.0];
        first.selected_index = 1;
        first.predicted_value_bits = 0.25f32.to_bits();
        let mut second = SelectedSpecV1::new(10, 2, 1, 3, 1);
        second.logits = vec![-0.5, 0.5];
        second.selected_index = 0;
        second.predicted_value_bits = 0.75f32.to_bits();
        let mut third = SelectedSpecV1::new(10, 2, 2, 3, 2);
        third.logits = vec![0.25, 0.5];
        third.selected_index = 1;
        third.predicted_value_bits = (-0.5f32).to_bits();
        let mut fourth = SelectedSpecV1::new(10, 5, 0, 1, 3);
        fourth.step = 5;
        let actions = vec![first.clone(), second.clone(), third.clone(), fourth.clone()];

        observe_selected(&mut observer, &first).unwrap();
        observer
            .observe_terminal_v1(natural_terminal(11, &[], 3, 2, TerminalOutcomeV1::Draw))
            .unwrap();
        observe_selected(&mut observer, &second).unwrap();
        observe_selected(&mut observer, &third).unwrap();
        observe_selected(&mut observer, &fourth).unwrap();
        observer
            .observe_terminal_v1(natural_terminal(
                10,
                &actions,
                9,
                6,
                TerminalOutcomeV1::P0Win,
            ))
            .unwrap();

        let batch = observer.finish_v1().unwrap();
        assert_eq!(
            batch
                .episodes
                .iter()
                .map(|row| row.episode_id)
                .collect::<Vec<_>>(),
            vec![10, 11]
        );
        assert_eq!(batch.learner_policy_step_count, 4);
        assert_eq!(batch.learner_physical_decision_count, 2);
        assert_eq!(
            batch.update_staging,
            FlatPhysicalUpdateStagingV1::Ready {
                learner_group_count: 2
            }
        );
        let episode = &batch.episodes[0];
        assert_eq!(episode.learner_return, 1);
        assert_eq!(episode.opponent_policy_step_count, 5);
        assert_eq!(episode.opponent_physical_decision_count, 4);
        assert_eq!(episode.groups.len(), 2);
        assert_eq!(episode.groups[0].physical_decision_id, 2);
        assert_eq!(episode.groups[1].physical_decision_id, 5);
        assert_episode_arithmetic_invariants(episode);
        let expected_joint = selected_log_probability(10, 0, 1, &first.logits).unwrap()
            + selected_log_probability(10, 1, 0, &second.logits).unwrap()
            + selected_log_probability(10, 2, 1, &third.logits).unwrap();
        assert_eq!(
            episode.groups[0].joint_selected_log_probability_bits,
            expected_joint.to_bits()
        );
        assert_eq!(episode.groups[0].value_bits, first.predicted_value_bits);
        assert_eq!(
            episode.groups[0].substeps[1].predicted_value_bits,
            second.predicted_value_bits
        );
        assert_eq!(
            episode.groups[0].substeps[2].predicted_value_bits,
            third.predicted_value_bits
        );
        assert_eq!(episode.groups[0].substeps[0].raw_action_logit_bits.len(), 3);
        assert_eq!(batch.episodes[1].learner_physical_decision_count, 0);
    }

    #[test]
    fn owns_all_scorer_visible_tables_across_source_reuse_and_drop() {
        let mut observer = FlatPhysicalTrajectoryObserverV1::new(PlayerSeatV1::P0, 30, 1).unwrap();
        let first = SelectedSpecV1::new(30, 0, 0, 2, 0);
        let second = SelectedSpecV1::new(30, 0, 1, 2, 1);

        let (expected_first, expected_second) = {
            let mut globals = FlatGlobalsV1::default();
            globals.players[0].life = 101;
            globals.attackers_declared = true;

            let mut objects = vec![FlatObjectCoreV1 {
                card_token: 0x1010_0001,
                visible_ordinal: 0x1010,
                subtype_count: 1,
                ability_use_count: 1,
                goad_count: 1,
                ..FlatObjectCoreV1::default()
            }];

            let mut relations = vec![FlatRelationV1 {
                source_object: Some(0),
                primary_order: 0x2020,
                ..FlatRelationV1::default()
            }];

            let mut object_subtypes = vec![FlatObjectSubtypeV1 {
                object_index: 0,
                order: 0x3030,
                subtype_id: 0x3031,
            }];

            let mut ability_uses = vec![FlatObjectAbilityUseV1 {
                object_index: 0,
                ability_kind: 0x40,
                ability_index: 0x4040,
                uses: 0x4041,
                ..FlatObjectAbilityUseV1::default()
            }];

            let mut goads = vec![FlatObjectGoadV1 {
                object_index: 0,
                order: 0x5050,
                expires_after_turns: 0x5051,
                ..FlatObjectGoadV1::default()
            }];

            let mut completed_dungeons = vec![FlatCompletedDungeonV1 {
                order: 0x6060,
                dungeon_id: 0x6061,
                ..FlatCompletedDungeonV1::default()
            }];

            let mut effect_subtype_changes = vec![FlatEffectSubtypeChangeV1 {
                effect_order: 0x7070,
                order: 0x7071,
                subtype_id: 0x7072,
                ..FlatEffectSubtypeChangeV1::default()
            }];

            let mut context_path_elements = vec![FlatContextPathElementV1 {
                context_order: 0x8080,
                order: 0x8081,
                value: 0x8082,
                ..FlatContextPathElementV1::default()
            }];

            let first_action = FlatScorerActionCoreV1 {
                flags: 0x9090,
                option_index: 0x9091,
                ref_len: 1,
                ..FlatScorerActionCoreV1::default()
            };
            let second_action = FlatScorerActionCoreV1 {
                flags: 0x9092,
                option_index: 0x9093,
                ..FlatScorerActionCoreV1::default()
            };
            let mut actions = vec![first_action, second_action];

            let mut action_refs = vec![FlatScorerActionRefV1 {
                action_index: 0,
                projection_role_id: 0xa0,
                order_index: 0xa0a0,
                associated_order: 0xa0a1,
                card_token: 0xa0a2,
                model_object_index: 0,
            }];

            let expected_first = FlatOwnedScoringInputsV1 {
                globals,
                objects: objects.clone(),
                relations: relations.clone(),
                object_subtypes: object_subtypes.clone(),
                ability_uses: ability_uses.clone(),
                goads: goads.clone(),
                completed_dungeons: completed_dungeons.clone(),
                effect_subtype_changes: effect_subtype_changes.clone(),
                context_path_elements: context_path_elements.clone(),
                actions: actions.clone(),
                action_refs: action_refs.clone(),
            };
            observer
                .observe_selected_v1(FlatScoredSelectedEventV1 {
                    expected: first.expected(),
                    binding: first.binding(),
                    learner_ordinal: first.learner_ordinal,
                    action_seed: first.action_seed,
                    selected_index: first.selected_index,
                    raw_action_logits: &first.logits,
                    predicted_value_bits: first.predicted_value_bits,
                    decision: FlatScoringDecisionViewV1::new(
                        &globals,
                        &objects,
                        &relations,
                        &object_subtypes,
                        &ability_uses,
                        &goads,
                        &completed_dungeons,
                        &effect_subtype_changes,
                        &context_path_elements,
                        &actions,
                        &action_refs,
                    ),
                })
                .unwrap();

            globals.players[0].life = -111;
            globals.attackers_declared = false;
            globals.blockers_declared = true;
            objects[0].card_token = 0x1111_0001;
            objects[0].visible_ordinal = 0x1111;
            relations[0].primary_order = 0x2222;
            object_subtypes[0].subtype_id = 0x3333;
            ability_uses[0].uses = 0x4444;
            goads[0].expires_after_turns = 0x5555;
            completed_dungeons[0].dungeon_id = 0x6666;
            effect_subtype_changes[0].subtype_id = 0x7777;
            context_path_elements[0].value = 0x8888;
            actions[0].flags = 0x9999;
            actions[1].flags = 0x999a;
            action_refs[0].card_token = 0xaaaa;

            let expected_second = FlatOwnedScoringInputsV1 {
                globals,
                objects: objects.clone(),
                relations: relations.clone(),
                object_subtypes: object_subtypes.clone(),
                ability_uses: ability_uses.clone(),
                goads: goads.clone(),
                completed_dungeons: completed_dungeons.clone(),
                effect_subtype_changes: effect_subtype_changes.clone(),
                context_path_elements: context_path_elements.clone(),
                actions: actions.clone(),
                action_refs: action_refs.clone(),
            };
            observer
                .observe_selected_v1(FlatScoredSelectedEventV1 {
                    expected: second.expected(),
                    binding: second.binding(),
                    learner_ordinal: second.learner_ordinal,
                    action_seed: second.action_seed,
                    selected_index: second.selected_index,
                    raw_action_logits: &second.logits,
                    predicted_value_bits: second.predicted_value_bits,
                    decision: FlatScoringDecisionViewV1::new(
                        &globals,
                        &objects,
                        &relations,
                        &object_subtypes,
                        &ability_uses,
                        &goads,
                        &completed_dungeons,
                        &effect_subtype_changes,
                        &context_path_elements,
                        &actions,
                        &action_refs,
                    ),
                })
                .unwrap();

            drop((
                globals,
                objects,
                relations,
                object_subtypes,
                ability_uses,
                goads,
                completed_dungeons,
                effect_subtype_changes,
                context_path_elements,
                actions,
                action_refs,
            ));
            (expected_first, expected_second)
        };

        observer
            .observe_terminal_v1(natural_terminal(
                30,
                &[first, second],
                2,
                1,
                TerminalOutcomeV1::Draw,
            ))
            .unwrap();
        let batch = observer.finish_v1().unwrap();
        let substeps = &batch.episodes[0].groups[0].substeps;
        assert_eq!(substeps[0].scoring_inputs, expected_first);
        assert_eq!(substeps[1].scoring_inputs, expected_second);
        assert_ne!(substeps[0].scoring_inputs, substeps[1].scoring_inputs);
    }

    #[test]
    fn selected_log_probabilities_and_joint_sum_match_independent_binary32_golden() {
        const FIRST_LOG_PROBABILITY_BITS: u32 = 0xcf00_0000;
        const SECOND_LOG_PROBABILITY_BITS: u32 = 0xc300_0000;
        const THIRD_LOG_PROBABILITY_BITS: u32 = 0xc300_0000;
        const FOURTH_LOG_PROBABILITY_BITS: u32 = 0xb3ff_ffff;
        const FOURTH_F64_THEN_CAST_BITS: u32 = 0xb3f1_aadf;
        const ORDERED_JOINT_BITS: u32 = 0xcf00_0000;

        let mut first = SelectedSpecV1::new(31, 0, 0, 4, 0);
        first.logits = vec![-2_147_483_648.0_f32, 0.0];
        first.selected_index = 0;
        let mut second = SelectedSpecV1::new(31, 0, 1, 4, 1);
        second.logits = vec![-128.0, 0.0, -256.0];
        second.selected_index = 0;
        let mut third = SelectedSpecV1::new(31, 0, 2, 4, 2);
        third.logits = vec![-512.0, -384.0, -128.0, 0.0];
        third.selected_index = 2;
        let mut fourth = SelectedSpecV1::new(31, 0, 3, 4, 3);
        fourth.logits = vec![0.0, -16.0, -32.0, -48.0, -64.0];
        fourth.selected_index = 0;

        let fourth_f64_max = fourth
            .logits
            .iter()
            .copied()
            .map(f64::from)
            .fold(f64::NEG_INFINITY, f64::max);
        let fourth_f64_sum = fourth
            .logits
            .iter()
            .copied()
            .map(f64::from)
            .map(|value| (value - fourth_f64_max).exp())
            .sum::<f64>();
        let fourth_f64_then_cast =
            ((f64::from(fourth.logits[usize::try_from(fourth.selected_index).unwrap()])
                - fourth_f64_max)
                - fourth_f64_sum.ln()) as f32;
        assert_eq!(fourth_f64_then_cast.to_bits(), FOURTH_F64_THEN_CAST_BITS);
        assert_ne!(FOURTH_LOG_PROBABILITY_BITS, FOURTH_F64_THEN_CAST_BITS);

        let mut observer = FlatPhysicalTrajectoryObserverV1::new(PlayerSeatV1::P0, 31, 1).unwrap();
        observe_selected(&mut observer, &first).unwrap();
        observe_selected(&mut observer, &second).unwrap();
        observe_selected(&mut observer, &third).unwrap();
        observe_selected(&mut observer, &fourth).unwrap();
        observer
            .observe_terminal_v1(natural_terminal(
                31,
                &[first, second, third, fourth],
                4,
                1,
                TerminalOutcomeV1::Draw,
            ))
            .unwrap();

        let batch = observer.finish_v1().unwrap();
        let group = &batch.episodes[0].groups[0];
        assert_eq!(
            group
                .substeps
                .iter()
                .map(|substep| substep.selected_log_probability_bits)
                .collect::<Vec<_>>(),
            vec![
                FIRST_LOG_PROBABILITY_BITS,
                SECOND_LOG_PROBABILITY_BITS,
                THIRD_LOG_PROBABILITY_BITS,
                FOURTH_LOG_PROBABILITY_BITS,
            ]
        );
        assert_eq!(
            group.joint_selected_log_probability_bits,
            ORDERED_JOINT_BITS
        );
        assert_eq!(
            group
                .substeps
                .iter()
                .map(|substep| substep.raw_action_logit_bits.len())
                .collect::<Vec<_>>(),
            vec![2, 3, 4, 5]
        );
        assert_ne!(
            group.substeps[3].selected_log_probability_bits,
            FOURTH_F64_THEN_CAST_BITS
        );
        assert_ne!(
            ORDERED_JOINT_BITS,
            (f32::from_bits(SECOND_LOG_PROBABILITY_BITS)
                + f32::from_bits(THIRD_LOG_PROBABILITY_BITS)
                + f32::from_bits(FIRST_LOG_PROBABILITY_BITS)
                + f32::from_bits(FOURTH_LOG_PROBABILITY_BITS))
            .to_bits()
        );
    }

    #[test]
    fn all_zero_group_episodes_stage_explicit_no_update() {
        let mut observer = FlatPhysicalTrajectoryObserverV1::new(PlayerSeatV1::P1, 40, 2).unwrap();
        observer
            .observe_terminal_v1(natural_terminal(41, &[], 4, 3, TerminalOutcomeV1::P0Win))
            .unwrap();
        observer
            .observe_terminal_v1(natural_terminal(40, &[], 0, 0, TerminalOutcomeV1::Draw))
            .unwrap();
        let batch = observer.finish_v1().unwrap();
        assert_eq!(batch.learner_policy_step_count, 0);
        assert_eq!(batch.learner_physical_decision_count, 0);
        assert_eq!(
            batch.update_staging,
            FlatPhysicalUpdateStagingV1::NoUpdateZeroLearnerGroups
        );
        assert_eq!(batch.episodes[0].learner_return, 0);
        assert_eq!(batch.episodes[1].learner_return, -1);
        for episode in &batch.episodes {
            assert_episode_arithmetic_invariants(episode);
        }
    }

    #[test]
    fn selected_stream_mutations_fail_closed() {
        let cases: Vec<SelectedMutationCaseV1> = vec![
            (
                SelectedSpecV1::new(5, 0, 1, 2, 0),
                Box::new(|_, _| {}),
                FlatPhysicalTrajectoryErrorV1::FirstSubstepNotZero {
                    episode_id: 5,
                    physical_decision_id: 0,
                    actual: 1,
                },
            ),
            (
                SelectedSpecV1::new(5, 0, 0, 0, 0),
                Box::new(|_, _| {}),
                FlatPhysicalTrajectoryErrorV1::ZeroSubstepCount {
                    episode_id: 5,
                    physical_decision_id: 0,
                },
            ),
            (
                SelectedSpecV1::new(5, 0, 0, 1, 1),
                Box::new(|_, _| {}),
                FlatPhysicalTrajectoryErrorV1::LearnerOrdinalMismatch {
                    episode_id: 5,
                    expected: 0,
                    actual: 1,
                },
            ),
            (
                SelectedSpecV1::new(5, 0, 0, 1, 0),
                Box::new(|_, binding| binding.action_binding.physical_decision_id = 9),
                FlatPhysicalTrajectoryErrorV1::SelectedBindingMismatch {
                    episode_id: 5,
                    learner_ordinal: 0,
                },
            ),
        ];
        for (spec, mutate, expected_error) in cases {
            let mut observer =
                FlatPhysicalTrajectoryObserverV1::new(PlayerSeatV1::P0, 5, 1).unwrap();
            assert_eq!(
                observe_selected_with(&mut observer, &spec, mutate),
                Err(expected_error)
            );
            assert_eq!(observer.finish_v1(), Err(expected_error));
        }

        let mut actor_drift = SelectedSpecV1::new(5, 0, 0, 1, 0);
        actor_drift.acting_player = PlayerSeatV1::P1;
        let mut observer = FlatPhysicalTrajectoryObserverV1::new(PlayerSeatV1::P0, 5, 1).unwrap();
        assert_eq!(
            observe_selected(&mut observer, &actor_drift),
            Err(FlatPhysicalTrajectoryErrorV1::SelectedActorMismatch {
                episode_id: 5,
                expected: PlayerSeatV1::P0,
                actual: PlayerSeatV1::P1,
            })
        );

        let mut selected_index = SelectedSpecV1::new(5, 0, 0, 1, 0);
        selected_index.selected_index = 2;
        let mut observer = FlatPhysicalTrajectoryObserverV1::new(PlayerSeatV1::P0, 5, 1).unwrap();
        assert_eq!(
            observe_selected(&mut observer, &selected_index),
            Err(FlatPhysicalTrajectoryErrorV1::SelectedIndexOutOfRange {
                episode_id: 5,
                learner_ordinal: 0,
            })
        );

        let legal_width = SelectedSpecV1::new(5, 0, 0, 1, 0);
        let mut observer = FlatPhysicalTrajectoryObserverV1::new(PlayerSeatV1::P0, 5, 1).unwrap();
        assert_eq!(
            observe_selected_with(&mut observer, &legal_width, |expected, binding| {
                expected.legal_action_count = 3;
                binding.action_binding.legal_action_count = 3;
            }),
            Err(FlatPhysicalTrajectoryErrorV1::LegalActionCountMismatch {
                episode_id: 5,
                learner_ordinal: 0,
            })
        );
    }

    #[test]
    fn overlap_drop_duplicate_and_physical_id_reuse_fail_closed() {
        let first = SelectedSpecV1::new(8, 2, 0, 3, 0);

        let mut overlap = FlatPhysicalTrajectoryObserverV1::new(PlayerSeatV1::P0, 8, 1).unwrap();
        observe_selected(&mut overlap, &first).unwrap();
        let changed_key = SelectedSpecV1::new(8, 3, 1, 3, 1);
        assert_eq!(
            observe_selected(&mut overlap, &changed_key),
            Err(FlatPhysicalTrajectoryErrorV1::OpenGroupKeyMismatch {
                episode_id: 8,
                expected_physical_decision_id: 2,
                actual_physical_decision_id: 3,
            })
        );

        let mut dropped = FlatPhysicalTrajectoryObserverV1::new(PlayerSeatV1::P0, 8, 1).unwrap();
        observe_selected(&mut dropped, &first).unwrap();
        let skipped = SelectedSpecV1::new(8, 2, 2, 3, 1);
        assert_eq!(
            observe_selected(&mut dropped, &skipped),
            Err(FlatPhysicalTrajectoryErrorV1::SubstepIndexMismatch {
                episode_id: 8,
                physical_decision_id: 2,
                expected: 1,
                actual: 2,
            })
        );

        let mut count_drift =
            FlatPhysicalTrajectoryObserverV1::new(PlayerSeatV1::P0, 8, 1).unwrap();
        observe_selected(&mut count_drift, &first).unwrap();
        let changed_count = SelectedSpecV1::new(8, 2, 1, 2, 1);
        assert_eq!(
            observe_selected(&mut count_drift, &changed_count),
            Err(FlatPhysicalTrajectoryErrorV1::SubstepCountMismatch {
                episode_id: 8,
                physical_decision_id: 2,
                expected: 3,
                actual: 2,
            })
        );

        let mut duplicate = FlatPhysicalTrajectoryObserverV1::new(PlayerSeatV1::P0, 8, 1).unwrap();
        observe_selected(&mut duplicate, &first).unwrap();
        let repeated = SelectedSpecV1::new(8, 2, 0, 3, 1);
        assert_eq!(
            observe_selected(&mut duplicate, &repeated),
            Err(FlatPhysicalTrajectoryErrorV1::SubstepIndexMismatch {
                episode_id: 8,
                physical_decision_id: 2,
                expected: 1,
                actual: 0,
            })
        );

        let mut reuse = FlatPhysicalTrajectoryObserverV1::new(PlayerSeatV1::P0, 8, 1).unwrap();
        let complete = SelectedSpecV1::new(8, 2, 0, 1, 0);
        observe_selected(&mut reuse, &complete).unwrap();
        let reused = SelectedSpecV1::new(8, 2, 0, 1, 1);
        assert_eq!(
            observe_selected(&mut reuse, &reused),
            Err(
                FlatPhysicalTrajectoryErrorV1::PhysicalDecisionNotStrictlyIncreasing {
                    episode_id: 8,
                    previous: 2,
                    actual: 2,
                }
            )
        );

        let mut decreasing = FlatPhysicalTrajectoryObserverV1::new(PlayerSeatV1::P0, 8, 1).unwrap();
        observe_selected(&mut decreasing, &complete).unwrap();
        let decreased = SelectedSpecV1::new(8, 1, 0, 1, 1);
        assert_eq!(
            observe_selected(&mut decreasing, &decreased),
            Err(
                FlatPhysicalTrajectoryErrorV1::PhysicalDecisionNotStrictlyIncreasing {
                    episode_id: 8,
                    previous: 2,
                    actual: 1,
                }
            )
        );
    }

    #[test]
    fn learner_policy_steps_are_contiguous_within_groups_and_increase_between_groups() {
        let first = SelectedSpecV1::new(8, 2, 0, 2, 0);

        let mut within_group =
            FlatPhysicalTrajectoryObserverV1::new(PlayerSeatV1::P0, 8, 1).unwrap();
        observe_selected(&mut within_group, &first).unwrap();
        let mut skipped_step = SelectedSpecV1::new(8, 2, 1, 2, 1);
        skipped_step.step = 2;
        assert_eq!(
            observe_selected(&mut within_group, &skipped_step),
            Err(
                FlatPhysicalTrajectoryErrorV1::PolicyStepNotContiguousWithinGroup {
                    episode_id: 8,
                    physical_decision_id: 2,
                    expected: 1,
                    actual: 2,
                }
            )
        );

        let mut between_groups =
            FlatPhysicalTrajectoryObserverV1::new(PlayerSeatV1::P0, 8, 1).unwrap();
        let complete = SelectedSpecV1::new(8, 2, 0, 1, 0);
        observe_selected(&mut between_groups, &complete).unwrap();
        let mut nonincreasing = SelectedSpecV1::new(8, 5, 0, 1, 1);
        nonincreasing.step = 0;
        assert_eq!(
            observe_selected(&mut between_groups, &nonincreasing),
            Err(
                FlatPhysicalTrajectoryErrorV1::PolicyStepNotStrictlyIncreasing {
                    episode_id: 8,
                    previous: 0,
                    actual: 0,
                }
            )
        );

        let mut opponent_gap =
            FlatPhysicalTrajectoryObserverV1::new(PlayerSeatV1::P0, 8, 1).unwrap();
        observe_selected(&mut opponent_gap, &complete).unwrap();
        let mut after_gap = SelectedSpecV1::new(8, 5, 0, 1, 1);
        after_gap.step = 4;
        observe_selected(&mut opponent_gap, &after_gap).unwrap();
        opponent_gap
            .observe_terminal_v1(natural_terminal(
                8,
                &[complete, after_gap],
                6,
                6,
                TerminalOutcomeV1::Draw,
            ))
            .unwrap();
    }

    #[test]
    fn terminal_rejects_learner_step_out_of_range_and_impossible_opponent_counts() {
        let mut out_of_range =
            FlatPhysicalTrajectoryObserverV1::new(PlayerSeatV1::P0, 12, 1).unwrap();
        let mut selected = SelectedSpecV1::new(12, 0, 0, 1, 0);
        selected.step = 5;
        observe_selected(&mut out_of_range, &selected).unwrap();
        assert_eq!(
            out_of_range.observe_terminal_v1(natural_terminal(
                12,
                std::slice::from_ref(&selected),
                5,
                1,
                TerminalOutcomeV1::Draw,
            )),
            Err(FlatPhysicalTrajectoryErrorV1::LearnerPolicyStepOutOfRange {
                episode_id: 12,
                policy_step: 5,
                terminal_policy_step_count: 5,
            })
        );

        let mut greater_than =
            FlatPhysicalTrajectoryObserverV1::new(PlayerSeatV1::P0, 14, 1).unwrap();
        let mut selected = SelectedSpecV1::new(14, 0, 0, 1, 0);
        selected.step = 6;
        observe_selected(&mut greater_than, &selected).unwrap();
        assert_eq!(
            greater_than.observe_terminal_v1(natural_terminal(
                14,
                std::slice::from_ref(&selected),
                5,
                1,
                TerminalOutcomeV1::Draw,
            )),
            Err(FlatPhysicalTrajectoryErrorV1::LearnerPolicyStepOutOfRange {
                episode_id: 14,
                policy_step: 6,
                terminal_policy_step_count: 5,
            })
        );

        let mut impossible =
            FlatPhysicalTrajectoryObserverV1::new(PlayerSeatV1::P0, 13, 1).unwrap();
        assert_eq!(
            impossible.observe_terminal_v1(natural_terminal(
                13,
                &[],
                2,
                3,
                TerminalOutcomeV1::Draw,
            )),
            Err(
                FlatPhysicalTrajectoryErrorV1::OpponentPolicyStepsBelowPhysicalDecisions {
                    episode_id: 13,
                    opponent_policy_step_count: 2,
                    opponent_physical_decision_count: 3,
                }
            )
        );

        let mut mixed = FlatPhysicalTrajectoryObserverV1::new(PlayerSeatV1::P0, 15, 1).unwrap();
        let selected = SelectedSpecV1::new(15, 0, 0, 1, 0);
        observe_selected(&mut mixed, &selected).unwrap();
        assert_eq!(
            mixed.observe_terminal_v1(natural_terminal(
                15,
                std::slice::from_ref(&selected),
                2,
                3,
                TerminalOutcomeV1::Draw,
            )),
            Err(
                FlatPhysicalTrajectoryErrorV1::OpponentPolicyStepsBelowPhysicalDecisions {
                    episode_id: 15,
                    opponent_policy_step_count: 1,
                    opponent_physical_decision_count: 2,
                }
            )
        );
    }

    #[test]
    fn non_finite_float_mutations_fail_closed_before_staging() {
        let mut bad_logit = SelectedSpecV1::new(6, 0, 0, 1, 0);
        bad_logit.logits[1] = f32::INFINITY;
        let mut observer = FlatPhysicalTrajectoryObserverV1::new(PlayerSeatV1::P0, 6, 1).unwrap();
        assert_eq!(
            observe_selected(&mut observer, &bad_logit),
            Err(FlatPhysicalTrajectoryErrorV1::NonFiniteLogit {
                episode_id: 6,
                learner_ordinal: 0,
                action_index: 1,
                bits: f32::INFINITY.to_bits(),
            })
        );

        let mut bad_value = SelectedSpecV1::new(6, 0, 0, 1, 0);
        bad_value.predicted_value_bits = f32::NAN.to_bits();
        let mut observer = FlatPhysicalTrajectoryObserverV1::new(PlayerSeatV1::P0, 6, 1).unwrap();
        assert_eq!(
            observe_selected(&mut observer, &bad_value),
            Err(FlatPhysicalTrajectoryErrorV1::NonFinitePredictedValue {
                episode_id: 6,
                learner_ordinal: 0,
                bits: f32::NAN.to_bits(),
            })
        );
    }

    #[test]
    fn selected_error_poisoning_prevents_partial_output_even_if_caller_continues() {
        let mut observer = FlatPhysicalTrajectoryObserverV1::new(PlayerSeatV1::P0, 9, 1).unwrap();
        let first = SelectedSpecV1::new(9, 0, 0, 2, 0);
        observe_selected(&mut observer, &first).unwrap();
        let duplicate = SelectedSpecV1::new(9, 0, 0, 2, 1);
        let error = FlatPhysicalTrajectoryErrorV1::SubstepIndexMismatch {
            episode_id: 9,
            physical_decision_id: 0,
            expected: 1,
            actual: 0,
        };
        assert_eq!(observe_selected(&mut observer, &duplicate), Err(error));
        let valid_continuation = SelectedSpecV1::new(9, 0, 1, 2, 1);
        assert_eq!(
            observe_selected(&mut observer, &valid_continuation),
            Err(error)
        );
        assert_eq!(
            observer.observe_terminal_v1(natural_terminal(
                9,
                &[first, valid_continuation],
                2,
                1,
                TerminalOutcomeV1::Draw,
            )),
            Err(error)
        );
        assert_eq!(observer.finish_v1(), Err(error));
    }

    #[test]
    fn terminal_mutations_and_missing_terminal_fail_closed() {
        let action = SelectedSpecV1::new(12, 2, 0, 1, 0);

        let mut interrupted =
            FlatPhysicalTrajectoryObserverV1::new(PlayerSeatV1::P0, 12, 1).unwrap();
        let open = SelectedSpecV1::new(12, 2, 0, 2, 0);
        observe_selected(&mut interrupted, &open).unwrap();
        assert_eq!(
            interrupted.observe_terminal_v1(natural_terminal(
                12,
                &[open],
                1,
                3,
                TerminalOutcomeV1::Draw,
            )),
            Err(
                FlatPhysicalTrajectoryErrorV1::TerminalInterruptedOpenGroup {
                    episode_id: 12,
                    physical_decision_id: 2,
                }
            )
        );

        let mut count = FlatPhysicalTrajectoryObserverV1::new(PlayerSeatV1::P0, 12, 1).unwrap();
        observe_selected(&mut count, &action).unwrap();
        let mut bad_count = natural_terminal(
            12,
            std::slice::from_ref(&action),
            2,
            3,
            TerminalOutcomeV1::Draw,
        );
        bad_count.learner_action_count = 2;
        assert_eq!(
            count.observe_terminal_v1(bad_count),
            Err(
                FlatPhysicalTrajectoryErrorV1::TerminalLearnerActionCountMismatch {
                    episode_id: 12,
                    expected: 1,
                    actual: 2,
                }
            )
        );

        let mut trace = FlatPhysicalTrajectoryObserverV1::new(PlayerSeatV1::P0, 12, 1).unwrap();
        observe_selected(&mut trace, &action).unwrap();
        let mut bad_trace = natural_terminal(
            12,
            std::slice::from_ref(&action),
            2,
            3,
            TerminalOutcomeV1::Draw,
        );
        bad_trace.learner_trace_hash ^= 1;
        assert_eq!(
            trace.observe_terminal_v1(bad_trace),
            Err(FlatPhysicalTrajectoryErrorV1::TerminalLearnerTraceMismatch { episode_id: 12 })
        );

        let mut non_natural =
            FlatPhysicalTrajectoryObserverV1::new(PlayerSeatV1::P0, 12, 1).unwrap();
        observe_selected(&mut non_natural, &action).unwrap();
        let mut halted = natural_terminal(
            12,
            std::slice::from_ref(&action),
            2,
            3,
            TerminalOutcomeV1::Draw,
        );
        halted.terminal.terminal_classification = TerminalClassificationV1::Halted;
        halted.terminal.terminal_code = TerminalSafeCodeV2::FailClosed;
        assert_eq!(
            non_natural.observe_terminal_v1(halted),
            Err(FlatPhysicalTrajectoryErrorV1::NonNaturalTerminal { episode_id: 12 })
        );

        let mut tuple = FlatPhysicalTrajectoryObserverV1::new(PlayerSeatV1::P0, 12, 1).unwrap();
        observe_selected(&mut tuple, &action).unwrap();
        let mut bad_tuple = natural_terminal(
            12,
            std::slice::from_ref(&action),
            2,
            3,
            TerminalOutcomeV1::P0Win,
        );
        bad_tuple.terminal.terminal_reward = [-1, 1];
        assert_eq!(
            tuple.observe_terminal_v1(bad_tuple),
            Err(FlatPhysicalTrajectoryErrorV1::TerminalTupleMismatch { episode_id: 12 })
        );

        let mut range = FlatPhysicalTrajectoryObserverV1::new(PlayerSeatV1::P0, 12, 1).unwrap();
        observe_selected(&mut range, &action).unwrap();
        assert_eq!(
            range.observe_terminal_v1(natural_terminal(
                12,
                std::slice::from_ref(&action),
                2,
                2,
                TerminalOutcomeV1::Draw,
            )),
            Err(
                FlatPhysicalTrajectoryErrorV1::LearnerPhysicalDecisionOutOfRange {
                    episode_id: 12,
                    physical_decision_id: 2,
                    terminal_physical_decision_count: 2,
                }
            )
        );

        let mut policy_underflow =
            FlatPhysicalTrajectoryObserverV1::new(PlayerSeatV1::P0, 12, 1).unwrap();
        observe_selected(&mut policy_underflow, &action).unwrap();
        assert_eq!(
            policy_underflow.observe_terminal_v1(natural_terminal(
                12,
                std::slice::from_ref(&action),
                0,
                3,
                TerminalOutcomeV1::Draw,
            )),
            Err(FlatPhysicalTrajectoryErrorV1::TerminalPolicyStepCountUnderflow { episode_id: 12 })
        );

        let mut physical_underflow =
            FlatPhysicalTrajectoryObserverV1::new(PlayerSeatV1::P0, 12, 1).unwrap();
        let first_group = SelectedSpecV1::new(12, 0, 0, 1, 0);
        observe_selected(&mut physical_underflow, &first_group).unwrap();
        physical_underflow.duplicate_first_group_for_test(12);
        assert_eq!(
            physical_underflow.observe_terminal_v1(natural_terminal(
                12,
                std::slice::from_ref(&first_group),
                1,
                1,
                TerminalOutcomeV1::Draw,
            )),
            Err(
                FlatPhysicalTrajectoryErrorV1::TerminalPhysicalDecisionCountUnderflow {
                    episode_id: 12,
                }
            )
        );

        let missing = FlatPhysicalTrajectoryObserverV1::new(PlayerSeatV1::P0, 12, 1).unwrap();
        assert_eq!(
            missing.finish_v1(),
            Err(FlatPhysicalTrajectoryErrorV1::MissingTerminal { episode_id: 12 })
        );
    }

    #[test]
    fn terminal_winner_mismatch_poisons_all_later_observer_output() {
        let mut observer = FlatPhysicalTrajectoryObserverV1::new(PlayerSeatV1::P0, 21, 1).unwrap();
        let mut invalid = natural_terminal(21, &[], 0, 0, TerminalOutcomeV1::P0Win);
        invalid.terminal.winner = Some(PlayerSeatV1::P1);
        let error = FlatPhysicalTrajectoryErrorV1::TerminalTupleMismatch { episode_id: 21 };
        assert_eq!(observer.observe_terminal_v1(invalid), Err(error));
        assert_eq!(
            observer.observe_terminal_v1(natural_terminal(21, &[], 0, 0, TerminalOutcomeV1::Draw,)),
            Err(error)
        );
        assert_eq!(observer.finish_v1(), Err(error));
    }

    #[test]
    fn duplicate_terminal_and_selected_after_terminal_fail_closed() {
        let mut duplicate = FlatPhysicalTrajectoryObserverV1::new(PlayerSeatV1::P0, 20, 1).unwrap();
        let terminal = natural_terminal(20, &[], 2, 2, TerminalOutcomeV1::Draw);
        duplicate.observe_terminal_v1(terminal).unwrap();
        assert_eq!(
            duplicate.observe_terminal_v1(terminal),
            Err(FlatPhysicalTrajectoryErrorV1::DuplicateTerminal { episode_id: 20 })
        );

        let mut late = FlatPhysicalTrajectoryObserverV1::new(PlayerSeatV1::P0, 20, 1).unwrap();
        late.observe_terminal_v1(terminal).unwrap();
        let action = SelectedSpecV1::new(20, 0, 0, 1, 0);
        assert_eq!(
            observe_selected(&mut late, &action),
            Err(FlatPhysicalTrajectoryErrorV1::SelectedAfterTerminal { episode_id: 20 })
        );
    }

    struct ShapeInvariantScorerV1;

    impl FlatBatchScorerV1 for ShapeInvariantScorerV1 {
        fn score_batch_v1(
            &mut self,
            batch: &FlatScoringBatchViewV1<'_>,
            action_logits: &mut [f32],
            values: &mut [f32],
        ) -> Result<(), FlatBatchScorerErrorV1> {
            for (decision_index, value) in
                values.iter_mut().enumerate().take(batch.decision_count())
            {
                let start = batch.action_offsets()[decision_index];
                let end = batch.action_offsets()[decision_index + 1];
                for (local_index, output) in action_logits[start..end].iter_mut().enumerate() {
                    *output = local_index as f32 * 0.125 - 0.5;
                }
                let globals = batch.decision(decision_index).unwrap().globals();
                *value = (globals.players[0].life - globals.players[1].life) as f32 / 32.0;
            }
            Ok(())
        }
    }

    fn rollout_config(
        workers: usize,
        sessions: usize,
        batch_target: usize,
    ) -> AsyncRolloutConfigV2 {
        AsyncRolloutConfigV2 {
            deck_ids: ["Rally".to_string(), "Rally".to_string()],
            learner_seat: PlayerSeatV1::P0,
            environment_seed: 61_001,
            opponent_policy_seed: 62_001,
            learner_policy_seed: 63_001,
            max_physical_decisions: 5_000,
            max_policy_steps: 640_000,
            worker_count: workers,
            sessions_per_worker: sessions,
            broker_batch_target: batch_target,
            first_episode_id: 70,
            episode_count: 7,
            scheduler_timeout: Duration::from_secs(60),
            measure_broker_service_time: false,
        }
    }

    #[test]
    fn grouped_output_is_canonical_and_scheduler_shape_invariant() {
        let _lock = ASYNC_FLAT_SCORED_TEST_LOCK_V1.lock().unwrap();
        let shapes = [(1, 1, 1), (1, 4, 3), (4, 1, 3), (4, 2, 5)];
        let mut reference = None;
        for (workers, sessions, target) in shapes {
            let config = rollout_config(workers, sessions, target);
            let observer = FlatPhysicalTrajectoryObserverV1::new(
                config.learner_seat,
                config.first_episode_id,
                config.episode_count,
            )
            .unwrap();
            let mut scorer = ShapeInvariantScorerV1;
            let (result, batch) =
                run_async_flat_scored_rollout_observed_v1(config, &mut scorer, observer).unwrap();
            assert!(result.all_natural());
            assert_eq!(batch.episodes.len(), result.episodes.len());
            assert_eq!(
                batch.learner_policy_step_count,
                result.metrics.sampled_action_count
            );
            assert!(batch
                .episodes
                .windows(2)
                .all(|rows| rows[0].episode_id < rows[1].episode_id));
            for episode in &batch.episodes {
                assert_episode_arithmetic_invariants(episode);
            }
            match &reference {
                Some(expected) => assert_eq!(&batch, expected),
                None => reference = Some(batch),
            }
        }
    }

    struct CorruptingOrdinalObserverV1 {
        inner: FlatPhysicalTrajectoryObserverV1,
        selected_count: u64,
    }

    impl FlatScoredTrajectoryObserverV1 for CorruptingOrdinalObserverV1 {
        type Error = FlatPhysicalTrajectoryErrorV1;
        type Output = FlatGroupedTrajectoryBatchV1;

        fn observe_selected_v1(
            &mut self,
            mut event: FlatScoredSelectedEventV1<'_>,
        ) -> Result<(), Self::Error> {
            self.selected_count += 1;
            if self.selected_count == 2 {
                event.learner_ordinal = event.learner_ordinal.saturating_add(1);
            }
            self.inner.observe_selected_v1(event)
        }

        fn observe_terminal_v1(
            &mut self,
            event: FlatScoredTerminalEventV1,
        ) -> Result<(), Self::Error> {
            self.inner.observe_terminal_v1(event)
        }

        fn finish_v1(self) -> Result<Self::Output, Self::Error> {
            self.inner.finish_v1()
        }
    }

    #[test]
    fn observed_rollout_discards_grouped_output_on_mutated_association() {
        let _lock = ASYNC_FLAT_SCORED_TEST_LOCK_V1.lock().unwrap();
        let config = rollout_config(1, 2, 2);
        let inner = FlatPhysicalTrajectoryObserverV1::new(
            config.learner_seat,
            config.first_episode_id,
            config.episode_count,
        )
        .unwrap();
        let observer = CorruptingOrdinalObserverV1 {
            inner,
            selected_count: 0,
        };
        let mut scorer = ShapeInvariantScorerV1;
        let error =
            run_async_flat_scored_rollout_observed_v1(config, &mut scorer, observer).unwrap_err();
        assert!(matches!(
            error,
            AsyncFlatScoredObservedRunErrorV1::ObserverFailed {
                phase: FlatScoredObserverPhaseV1::Selected,
                error: FlatPhysicalTrajectoryErrorV1::LearnerOrdinalMismatch { .. }
            }
        ));
    }
}
