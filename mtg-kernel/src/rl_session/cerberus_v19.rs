//! Private Cerberus binding, separate from frozen observation/session modes.

use super::*;
use crate::cerberus_public_v1::CerberusObservationV1;
#[cfg(test)]
use crate::state::GameState;

// No Debug/Serialize: executable candidates and the transport observation
// must never become a user-facing receipt or request.
#[derive(Clone, PartialEq)]
pub(crate) struct CerberusDecisionBindingV19 {
    expected: FastActorDecisionV1,
    candidates: Vec<CorePolicyActionCandidateV1>,
    observation: CerberusObservationV1,
}

pub(super) fn normalized_origin(
    session: &FastActorSessionV1,
    current: &FastActorCurrentDecisionV1,
) -> Result<Vec<CorePolicyActionCandidateV1>, FlatActionDecisionSliceErrorV1> {
    crate::cerberus_public_v1::validate_state(&session.state)
        .map_err(|_| FlatActionDecisionSliceErrorV1::CorruptCurrentBinding)?;
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
        crate::rl::policy_observation_extensions_common_for_human_v8(&session.state, current.actor)
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
    /// Explicit backend opt-in for the separate Cerberus projector.
    /// It consumes the session; predecessor adapters keep their old mode.
    pub fn into_cerberus_v19(mut self) -> Result<Self, RlContractError> {
        // Validate before any advancement can expire a malformed active marker.
        crate::cerberus_public_v1::validate_state(&self.state).map_err(RlContractError)?;
        self.flat_action_contract_mode = FlatActionContractModeV1::CerberusV19;
        self.flat_action_cache_spare = None;
        self.flat_action_cache_spare_v2 = None;
        if let Some(mut current) = self.current.take() {
            prepare_current(&self, &mut current)
                .map_err(|error| RlContractError(format!("cerberus_v19:{error:?}")))?;
            self.current = Some(current);
        }
        Ok(self)
    }

    #[cfg(test)]
    pub(crate) fn from_cerberus_v19_fixture_state(state: GameState) -> Self {
        Self::from_cerberus_v19_fixture_state_and_surface(state, PolicySurfaceV5::new_for_session())
    }

    #[cfg(all(
        test,
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    ))]
    pub(crate) fn from_cerberus_v19_engine_priority_fixture_state(state: GameState) -> Self {
        Self::from_cerberus_v19_fixture_state_and_surface(
            state,
            PolicySurfaceV5::new_with_engine_priority_v1(),
        )
    }

    #[cfg(test)]
    fn from_cerberus_v19_fixture_state_and_surface(
        state: GameState,
        surface: PolicySurfaceV5,
    ) -> Self {
        let mut session = Self::reset_with_limits(23, 91, 10_000, 10_000);
        session.state = state;
        session.surface = surface;
        session.environment_revision = 0;
        session.policy_step_count = 0;
        session.physical_decision_count = 0;
        session.current = None;
        session.terminal = None;
        session.flat_action_contract_mode = FlatActionContractModeV1::CerberusV19;
        session.flat_action_cache_spare = None;
        session.flat_action_cache_spare_v2 = None;
        if let Err(error) = crate::cerberus_public_v1::validate_state(&session.state) {
            session.terminal = Some(halted_terminal(
                &session.deck_ids,
                session.deck_hashes,
                session.episode_id,
                format!("fail_closed:cerberus_v19:{error}"),
                session.policy_step_count,
                session.physical_decision_count,
            ));
            return session;
        }
        session.advance_to_decision_or_terminal();
        session
    }

    #[cfg(test)]
    pub(crate) fn cerberus_fixture_state_mut(&mut self) -> &mut GameState {
        &mut self.state
    }

    /// Actor-visible authority plus a complete ordered executable binding.
    /// No predecessor cache or observation constructor is consulted.
    pub(crate) fn cerberus_current_decision_input_v19(
        &self,
        expected: FastActorDecisionV1,
        human_seat: PlayerSeatV1,
    ) -> Result<
        (
            CerberusObservationV1,
            Vec<ActionSemanticV1>,
            CerberusDecisionBindingV19,
        ),
        FlatActionDecisionSliceErrorV1,
    > {
        if self.flat_action_contract_mode != FlatActionContractModeV1::CerberusV19 {
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
        let observation = crate::cerberus_public_v1::observe_v1(
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
        let binding = CerberusDecisionBindingV19 {
            expected,
            candidates,
            observation: observation.clone(),
        };
        Ok((observation, actions, binding))
    }

    /// Revalidation is read-only. The projector calls this immediately
    /// before applying its privately mapped index through session.step.
    pub(crate) fn cerberus_validate_bound_decision_v19(
        &self,
        expected: FastActorDecisionV1,
        human_seat: PlayerSeatV1,
        binding: &CerberusDecisionBindingV19,
    ) -> Result<(), FlatActionDecisionSliceErrorV1> {
        let (_, _, current) = self.cerberus_current_decision_input_v19(expected, human_seat)?;
        if &current != binding {
            return Err(FlatActionDecisionSliceErrorV1::CorruptCurrentBinding);
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy_observation_v6::tests::{put, ready_state_for};

    #[test]
    fn complete_ordered_binding_observation_identity_seat_and_menu_changes_refuse_readonly() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready_state_for(actor);
            put(&mut state, actor, "Forest", Zone::Hand);
            put(&mut state, actor, "Mountain", Zone::Hand);
            let mut session = FastActorSessionV1::from_cerberus_v19_fixture_state(state);
            let FastActorResponseV1::Decision(expected) = session.current_response() else {
                panic!("real priority decision");
            };
            let (_, _, binding) = session
                .cerberus_current_decision_input_v19(expected, actor.into())
                .unwrap();
            let before = session.privileged_core_environment_hash();
            let mut forged = binding.clone();
            forged.candidates.reverse();
            assert!(session
                .cerberus_validate_bound_decision_v19(expected, actor.into(), &forged)
                .is_err());
            let mut forged = binding.clone();
            forged.observation.producer_sha256.clear();
            assert!(session
                .cerberus_validate_bound_decision_v19(expected, actor.into(), &forged)
                .is_err());
            assert!(session
                .cerberus_current_decision_input_v19(expected, actor.opponent().into())
                .is_err());
            assert_eq!(session.privileged_core_environment_hash(), before);
            session.cerberus_fixture_state_mut().players[actor.index()].life += 1;
            let before = session.privileged_core_environment_hash();
            assert!(session
                .cerberus_validate_bound_decision_v19(expected, actor.into(), &binding)
                .is_err());
            assert_eq!(session.privileged_core_environment_hash(), before);
        }
    }
}
