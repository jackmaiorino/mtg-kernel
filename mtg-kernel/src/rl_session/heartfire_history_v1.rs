//! Independent captured-source Human session input. This is neither scorer admission
//! nor the safe human DTO: typed observations and executable candidates stay
//! backend-only. The separate projector resolves only visible handles.

use super::*;
use crate::heartfire_history_v1::HeartfireHistoryObservationV1;
#[cfg(test)]
use crate::state::GameState;

// No Debug/Serialize: executable candidates and the transport observation
// must never become a user-facing receipt or request.
#[derive(Clone, PartialEq)]
pub(crate) struct HeartfireHistoryDecisionBindingV1 {
    expected: FastActorDecisionV1,
    candidates: Vec<CorePolicyActionCandidateV1>,
    observation: HeartfireHistoryObservationV1,
}

pub(super) fn normalized_origin(
    session: &FastActorSessionV1,
    current: &FastActorCurrentDecisionV1,
) -> Result<Vec<CorePolicyActionCandidateV1>, FlatActionDecisionSliceErrorV1> {
    let mut original = current.clone();
    original.flat_action_cache = None;
    original.flat_action_cache_error = None;
    original.flat_action_cache_v2 = None;
    original.flat_action_cache_error_v2 = None;
    original.candidates =
        core_policy_action_candidates_v5(&current.origin_decision, &session.state)
            .map_err(|_| FlatActionDecisionSliceErrorV1::InvalidDecisionRelation)?;
    flat_validate_current_binding_header_v1(session, &original)?;
    flat_validate_origin_decision_v1(&original, &session.state)?;
    let common =
        crate::rl::heartfire_history_v1::normalization_extensions(&session.state, current.actor)
            .map_err(|_| FlatActionDecisionSliceErrorV1::InvalidDecisionRelation)?;
    super::flat_action_v3::normalize_candidates(
        &mut original.candidates,
        &common,
        session,
        &original.origin_decision,
    )?;
    Ok(original.candidates)
}

pub(super) fn prepare_current(
    session: &FastActorSessionV1,
    current: &mut FastActorCurrentDecisionV1,
) -> Result<(), FlatActionDecisionSliceErrorV1> {
    current.candidates = normalized_origin(session, current)?;
    current.flat_action_cache = None;
    current.flat_action_cache_error = None;
    current.flat_action_cache_v2 = None;
    current.flat_action_cache_error_v2 = None;
    Ok(())
}

impl FastActorSessionV1 {
    /// Explicit backend opt-in for the separate captured-source Human projector.
    /// It consumes the session; predecessor adapters keep their old mode.
    pub fn into_heartfire_history_v1(mut self) -> Result<Self, RlContractError> {
        self.flat_action_contract_mode = FlatActionContractModeV1::HeartfireHistoryV1;
        self.flat_action_cache_spare = None;
        self.flat_action_cache_spare_v2 = None;
        if let Some(mut current) = self.current.take() {
            prepare_current(&self, &mut current)
                .map_err(|error| RlContractError(format!("heartfire_history_v1:{error:?}")))?;
            self.current = Some(current);
        }
        Ok(self)
    }

    #[cfg(test)]
    pub(crate) fn from_heartfire_history_fixture_state(state: GameState) -> Self {
        let mut session = Self::reset_with_limits(23, 91, 10_000, 10_000);
        session.state = state;
        session.surface = PolicySurfaceV5::new_for_session();
        session.environment_revision = 0;
        session.policy_step_count = 0;
        session.physical_decision_count = 0;
        session.current = None;
        session.terminal = None;
        session.flat_action_contract_mode = FlatActionContractModeV1::HeartfireHistoryV1;
        session.flat_action_cache_spare = None;
        session.flat_action_cache_spare_v2 = None;
        session.advance_to_decision_or_terminal();
        session
    }

    /// Actor-visible authority plus a complete ordered executable binding.
    /// No predecessor cache or observation constructor is consulted.
    pub(crate) fn heartfire_history_current_input_v1(
        &self,
        expected: FastActorDecisionV1,
        human_seat: PlayerSeatV1,
    ) -> Result<
        (
            HeartfireHistoryObservationV1,
            Vec<ActionSemanticV1>,
            HeartfireHistoryDecisionBindingV1,
        ),
        FlatActionDecisionSliceErrorV1,
    > {
        if self.flat_action_contract_mode != FlatActionContractModeV1::HeartfireHistoryV1 {
            return Err(FlatActionDecisionSliceErrorV1::InvalidDecisionRelation);
        }
        let current = self
            .current
            .as_ref()
            .ok_or(FlatActionDecisionSliceErrorV1::NoCurrentDecision)?;
        flat_validate_expected_decision_v1(self, current, expected)?;
        if PlayerSeatV1::from(current.actor) != human_seat {
            return Err(FlatActionDecisionSliceErrorV1::ActingPlayerMismatch);
        }
        let candidates = normalized_origin(self, current)?;
        if candidates != current.candidates {
            return Err(FlatActionDecisionSliceErrorV1::CorruptCurrentBinding);
        }
        let observation = crate::heartfire_history_v1::observe(
            &self.state,
            &self.surface,
            current.actor,
            self.policy_step_count,
            current.physical_decision_id,
            current.substep_index,
            current.substep_count,
        )
        .map_err(|_| FlatActionDecisionSliceErrorV1::CorruptCurrentBinding)?;
        let actions = candidates
            .iter()
            .map(|candidate| {
                super::flat_action_v4::frozen_pending_trigger_semantic_v4(
                    &self.state,
                    candidate.semantic.clone(),
                )
            })
            .collect();
        let binding = HeartfireHistoryDecisionBindingV1 {
            expected,
            candidates,
            observation: observation.clone(),
        };
        Ok((observation, actions, binding))
    }

    /// Revalidation is read-only. The projector calls this immediately
    /// before applying its privately mapped index through session.step.
    pub(crate) fn heartfire_history_validate_bound_v1(
        &self,
        expected: FastActorDecisionV1,
        human_seat: PlayerSeatV1,
        binding: &HeartfireHistoryDecisionBindingV1,
    ) -> Result<(), FlatActionDecisionSliceErrorV1> {
        let (_, _, current) = self.heartfire_history_current_input_v1(expected, human_seat)?;
        if &current != binding {
            return Err(FlatActionDecisionSliceErrorV1::CorruptCurrentBinding);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy_observation_v6::tests::put;
    #[test]
    fn history_input_identity_complete_binding_and_foreign_mode_refusal_are_read_only() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = crate::policy_observation_v6::tests::ready_state_for(actor);
            put(&mut state, actor, "Forest", Zone::Hand);
            put(&mut state, actor.opponent(), "Lightning Bolt", Zone::Hand);
            let session = FastActorSessionV1::from_heartfire_history_fixture_state(state);
            let FastActorResponseV1::Decision(expected) = session.current_response() else {
                panic!("real priority fixture")
            };
            let before = session.privileged_core_environment_hash();
            let (observation, _, binding) = session
                .heartfire_history_current_input_v1(expected, actor.into())
                .unwrap();
            assert_eq!(observation.schema, crate::heartfire_history_v1::SCHEMA);
            crate::heartfire_history_v1::validate(&observation).unwrap();
            session
                .heartfire_history_validate_bound_v1(expected, actor.into(), &binding)
                .unwrap();
            assert!(session
                .human_current_decision_input_v8(expected, actor.into())
                .is_err());
            for field in 0..4 {
                let mut forged = binding.clone();
                match field {
                    0 => forged.observation.source_sha256.push('x'),
                    1 => forged.observation.card_db_hash ^= 1,
                    2 => forged.expected.step += 1,
                    _ => forged.candidates.reverse(),
                }
                assert!(session
                    .heartfire_history_validate_bound_v1(expected, actor.into(), &forged)
                    .is_err());
            }
            assert_eq!(session.privileged_core_environment_hash(), before);
            let legacy = session.clone().into_human_v8().unwrap();
            assert!(legacy
                .heartfire_history_current_input_v1(expected, actor.into())
                .is_err());
            assert_eq!(legacy.privileged_core_environment_hash(), before);
        }
    }
}
