//! Private Employment binding, separate from frozen observation/session modes.

use super::*;
use crate::employment_public_v1::EmploymentObservationV1;
#[cfg(test)]
use crate::state::GameState;

// No Debug/Serialize: executable candidates and the transport observation
// must never become a user-facing receipt or request.
#[derive(Clone, PartialEq)]
pub(crate) struct EmploymentDecisionBindingV18 {
    expected: FastActorDecisionV1,
    candidates: Vec<CorePolicyActionCandidateV1>,
    observation: EmploymentObservationV1,
}

pub(super) fn normalized_origin(
    session: &FastActorSessionV1,
    current: &FastActorCurrentDecisionV1,
) -> Result<Vec<CorePolicyActionCandidateV1>, FlatActionDecisionSliceErrorV1> {
    crate::effect::temporary_control_v1::authority::validate(&session.state)
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
    crate::employment_public_v1::observe_v1(
        &session.state,
        &session.surface,
        current.actor,
        session.policy_step_count,
        current.physical_decision_id,
        current.substep_index,
        current.substep_count,
    )
    .map_err(|_| FlatActionDecisionSliceErrorV1::InvalidDecisionRelation)?;
    Ok(())
}

impl FastActorSessionV1 {
    pub(crate) fn employment_restore_v18(
        &mut self,
        snapshot: &FastActorSessionSnapshotV1,
    ) -> Result<(), RlContractError> {
        let mut restored = snapshot.0.clone();
        if restored.flat_action_contract_mode != FlatActionContractModeV1::EmploymentV18
            || restored.episode_id != self.episode_id
        {
            return Err(RlContractError("wrong Employment snapshot owner".into()));
        }
        crate::effect::temporary_control_v1::authority::validate(&restored.state)
            .map_err(RlContractError)?;
        // The private surface scan owns this exact revision. The enclosing
        // Human owner advances its separate generation outside snapshots.
        if let Some(mut current) = restored.current.take() {
            prepare_current(&restored, &mut current)
                .map_err(|_| RlContractError("invalid Employment restored menu".into()))?;
            restored.current = Some(current);
        }
        *self = restored;
        Ok(())
    }

    /// Explicit backend opt-in for the separate Employment projector.
    /// It consumes the session; predecessor adapters keep their old mode.
    pub fn into_employment_v18(mut self) -> Result<Self, RlContractError> {
        crate::effect::temporary_control_v1::authority::validate(&self.state)
            .map_err(RlContractError)?;
        self.flat_action_contract_mode = FlatActionContractModeV1::EmploymentV18;
        self.flat_action_cache_spare = None;
        self.flat_action_cache_spare_v2 = None;
        if let Some(mut current) = self.current.take() {
            prepare_current(&self, &mut current)
                .map_err(|error| RlContractError(format!("employment_v18:{error:?}")))?;
            self.current = Some(current);
        }
        Ok(self)
    }

    #[cfg(test)]
    pub(crate) fn from_employment_v18_fixture_state(state: GameState) -> Self {
        let mut session = Self::reset_with_limits(23, 91, 10_000, 10_000);
        session.state = state;
        session.surface = PolicySurfaceV5::new_for_session();
        session.environment_revision = 0;
        session.policy_step_count = 0;
        session.physical_decision_count = 0;
        session.current = None;
        session.terminal = None;
        session.flat_action_contract_mode = FlatActionContractModeV1::EmploymentV18;
        session.flat_action_cache_spare = None;
        session.flat_action_cache_spare_v2 = None;
        session.advance_to_decision_or_terminal();
        session
    }

    #[cfg(test)]
    pub(crate) fn employment_fixture_state_mut(&mut self) -> &mut GameState {
        &mut self.state
    }

    /// Actor-visible authority plus a complete ordered executable binding.
    /// No predecessor cache or observation constructor is consulted.
    pub(crate) fn employment_current_decision_input_v18(
        &self,
        expected: FastActorDecisionV1,
        human_seat: PlayerSeatV1,
    ) -> Result<
        (
            EmploymentObservationV1,
            Vec<ActionSemanticV1>,
            EmploymentDecisionBindingV18,
        ),
        FlatActionDecisionSliceErrorV1,
    > {
        if self.flat_action_contract_mode != FlatActionContractModeV1::EmploymentV18 {
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
        let observation = crate::employment_public_v1::observe_v1(
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
        let binding = EmploymentDecisionBindingV18 {
            expected,
            candidates,
            observation: observation.clone(),
        };
        Ok((observation, actions, binding))
    }

    /// Revalidation is read-only. The projector calls this immediately
    /// before applying its privately mapped index through session.step.
    pub(crate) fn employment_validate_bound_decision_v18(
        &self,
        expected: FastActorDecisionV1,
        human_seat: PlayerSeatV1,
        binding: &EmploymentDecisionBindingV18,
    ) -> Result<(), FlatActionDecisionSliceErrorV1> {
        let (_, _, current) = self.employment_current_decision_input_v18(expected, human_seat)?;
        if &current != binding {
            return Err(FlatActionDecisionSliceErrorV1::CorruptCurrentBinding);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy_observation_v6::tests::{put, ready_state};

    #[test]
    fn complete_ordered_binding_observation_identity_seat_and_menu_changes_refuse_readonly() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready_state();
            state.active_player = actor;
            state.priority_player = actor;
            put(&mut state, actor, "Forest", Zone::Hand);
            put(&mut state, actor, "Mountain", Zone::Hand);
            let mut session = FastActorSessionV1::from_employment_v18_fixture_state(state);
            let FastActorResponseV1::Decision(expected) = session.current_response() else {
                panic!("real priority decision");
            };
            let (_, _, binding) = session
                .employment_current_decision_input_v18(expected, actor.into())
                .unwrap();
            let before = session.privileged_core_environment_hash();
            let mut forged = binding.clone();
            forged.candidates.reverse();
            assert!(session
                .employment_validate_bound_decision_v18(expected, actor.into(), &forged)
                .is_err());
            let mut forged = binding.clone();
            forged.observation.producer_sha256.clear();
            assert!(session
                .employment_validate_bound_decision_v18(expected, actor.into(), &forged)
                .is_err());
            assert!(session
                .employment_current_decision_input_v18(expected, actor.opponent().into())
                .is_err());
            assert_eq!(session.privileged_core_environment_hash(), before);
            session.employment_fixture_state_mut().players[actor.index()].life += 1;
            let before = session.privileged_core_environment_hash();
            assert!(session
                .employment_validate_bound_decision_v18(expected, actor.into(), &binding)
                .is_err());
            assert_eq!(session.privileged_core_environment_hash(), before);
        }
    }

    #[test]
    fn restored_authority_and_projection_failures_do_not_publish_the_candidate() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let (state, _, _) = crate::employment_public_v1::tests::primitive(actor);
            let mut session = FastActorSessionV1::from_employment_v18_fixture_state(state);
            let mut snapshot = session.snapshot_v1();
            snapshot.0.state.engine.event_history.retain(|event| {
                !matches!(
                    event,
                    crate::event::CommittedEvent::TemporaryControlInstalledV1 { .. }
                )
            });
            let before = session.privileged_core_environment_hash();
            assert!(session.employment_restore_v18(&snapshot).is_err());
            assert_eq!(session.privileged_core_environment_hash(), before);
            let mut snapshot = session.snapshot_v1();
            snapshot.0.current.as_mut().unwrap().bound_policy_step_count += 1;
            assert!(session.employment_restore_v18(&snapshot).is_err());
            assert_eq!(session.privileged_core_environment_hash(), before);
        }
    }
}
