//! Private life-aware human session input. This is neither scorer admission
//! nor the safe human DTO: typed observations and executable candidates stay
//! backend-only. The separate projector resolves only visible handles.

use super::*;
use crate::policy_observation_v8::ObservationV8;

// No Debug/Serialize: executable candidates and the transport observation
// must never become a user-facing receipt or request.
#[derive(Clone, PartialEq)]
pub(crate) struct HumanDecisionBindingV8 {
    expected: FastActorDecisionV1,
    candidates: Vec<CorePolicyActionCandidateV1>,
    observation: ObservationV8,
}

fn normalized_origin(
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
    /// Explicit backend opt-in for the separate Human V8 projector.
    /// It consumes the session; predecessor adapters keep their old mode.
    pub fn into_human_v8(mut self) -> Result<Self, RlContractError> {
        self.flat_action_contract_mode = FlatActionContractModeV1::HumanV8;
        self.flat_action_cache_spare = None;
        self.flat_action_cache_spare_v2 = None;
        if let Some(mut current) = self.current.take() {
            prepare_current(&self, &mut current)
                .map_err(|error| RlContractError(format!("human_v8:{error:?}")))?;
            self.current = Some(current);
        }
        Ok(self)
    }

    #[cfg(test)]
    pub(crate) fn from_human_v8_fixture_state(state: GameState) -> Self {
        let mut session = Self::reset_with_limits(23, 91, 10_000, 10_000);
        session.state = state;
        session.surface = PolicySurfaceV5::new_for_session();
        session.environment_revision = 0;
        session.policy_step_count = 0;
        session.physical_decision_count = 0;
        session.current = None;
        session.terminal = None;
        session.flat_action_contract_mode = FlatActionContractModeV1::HumanV8;
        session.flat_action_cache_spare = None;
        session.flat_action_cache_spare_v2 = None;
        session.advance_to_decision_or_terminal();
        session
    }

    /// Actor-visible authority plus a complete ordered executable binding.
    /// No predecessor cache or observation constructor is consulted.
    pub(crate) fn human_current_decision_input_v8(
        &self,
        expected: FastActorDecisionV1,
        human_seat: PlayerSeatV1,
    ) -> Result<
        (ObservationV8, Vec<ActionSemanticV1>, HumanDecisionBindingV8),
        FlatActionDecisionSliceErrorV1,
    > {
        if self.flat_action_contract_mode != FlatActionContractModeV1::HumanV8 {
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
        let observation = crate::rl::observe_policy_v8(
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
        let binding = HumanDecisionBindingV8 {
            expected,
            candidates,
            observation: observation.clone(),
        };
        Ok((observation, actions, binding))
    }

    /// Revalidation is read-only. The projector calls this immediately
    /// before applying its privately mapped index through session.step.
    pub(crate) fn human_validate_bound_decision_v8(
        &self,
        expected: FastActorDecisionV1,
        human_seat: PlayerSeatV1,
        binding: &HumanDecisionBindingV8,
    ) -> Result<(), FlatActionDecisionSliceErrorV1> {
        let (_, _, current) = self.human_current_decision_input_v8(expected, human_seat)?;
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

    fn expected(session: &FastActorSessionV1) -> FastActorDecisionV1 {
        let FastActorResponseV1::Decision(expected) = session.current_response() else {
            panic!("fixture must reach a playable decision");
        };
        expected
    }

    fn fixture(actor: PlayerId) -> FastActorSessionV1 {
        let mut state = ready_state();
        state.active_player = actor;
        state.priority_player = actor;
        put(&mut state, actor, "Mountain", Zone::Hand);
        put(&mut state, actor, "Forest", Zone::Hand);
        FastActorSessionV1::from_v3_fixture_state(state)
    }

    #[test]
    fn explicit_human_v8_preserves_registered_menu_and_replays_without_caches() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let legacy = fixture(actor);
            let e = expected(&legacy);
            let (_, old_actions, _) = legacy
                .human_current_decision_input_v4(e, actor.into())
                .unwrap();
            // Compare common fields at the same FullArtifact text mode.
            // The V4 scorer input deliberately elides artifact-only text.
            let old = crate::rl::observe_policy_v6(
                &legacy.state,
                &legacy.surface,
                actor,
                e.step,
                e.physical_decision_id,
                e.substep_index,
                e.substep_count,
            )
            .unwrap();
            assert!(legacy
                .human_current_decision_input_v8(e, actor.into())
                .is_err());
            let mut session = legacy.clone().into_human_v8().unwrap();
            let before = session.privileged_core_environment_hash();
            let (observation, actions, binding) = session
                .human_current_decision_input_v8(e, actor.into())
                .unwrap();
            assert_eq!(observation.schema_version, 8);
            assert_eq!(observation.projection, old.projection);
            assert_eq!(actions, old_actions);
            assert_eq!(expected(&session), e);
            assert_eq!(session.privileged_core_environment_hash(), before);
            assert!(session
                .current
                .as_ref()
                .unwrap()
                .flat_action_cache
                .is_none());
            assert!(session
                .current
                .as_ref()
                .unwrap()
                .flat_action_cache_v2
                .is_none());
            let snapshot = session.snapshot_v1();
            let response = session.step(e.episode_id, e.step, 0).unwrap();
            let after = session.privileged_core_environment_hash();
            session.restore_v1(&snapshot);
            session
                .human_validate_bound_decision_v8(e, actor.into(), &binding)
                .unwrap();
            assert_eq!(session.step(e.episode_id, e.step, 0).unwrap(), response);
            assert_eq!(session.privileged_core_environment_hash(), after);
        }
    }

    #[test]
    fn human_v8_binding_rejects_changed_visible_state_menu_and_seat_without_mutation() {
        let mut session = fixture(PlayerId::P0).into_human_v8().unwrap();
        let e = expected(&session);
        let (_, _, binding) = session
            .human_current_decision_input_v8(e, e.acting_player)
            .unwrap();
        let mut forged = binding.clone();
        forged.observation.projection.surface.life_totals[0] += 1;
        let before = session.privileged_core_environment_hash();
        assert!(session
            .human_validate_bound_decision_v8(e, e.acting_player, &forged)
            .is_err());
        forged = binding.clone();
        forged.candidates.reverse();
        assert!(session
            .human_validate_bound_decision_v8(e, e.acting_player, &forged)
            .is_err());
        assert!(session
            .human_current_decision_input_v8(e, PlayerSeatV1::P1)
            .is_err());
        let mut stale = e;
        stale.environment_revision += 1;
        assert!(session
            .human_current_decision_input_v8(stale, e.acting_player)
            .is_err());
        assert_eq!(session.privileged_core_environment_hash(), before);
        session.state.players[0].life += 1;
        let changed = session.privileged_core_environment_hash();
        assert!(session
            .human_validate_bound_decision_v8(e, e.acting_player, &binding)
            .is_err());
        assert_eq!(session.privileged_core_environment_hash(), changed);
    }

    #[test]
    #[cfg(feature = "standard-magezero-fixtures")]
    fn human_v8_binding_includes_registered_printed_life_ward_traits() {
        let mut state = ready_state();
        let source = put(&mut state, PlayerId::P1, "Brutal Cathar", Zone::Battlefield);
        put(&mut state, PlayerId::P0, "Mountain", Zone::Hand);
        crate::event::propose_and_commit(
            &mut state,
            crate::event::ProposedEvent::transform_in_place(source, 1),
        );
        // Session advancement synchronizes daybound faces at state checks.
        state.day_night_v1 = Some(crate::state::DayNightV1::Night);
        let session = FastActorSessionV1::from_v3_fixture_state(state)
            .into_human_v8()
            .unwrap();
        let e = expected(&session);
        let (observation, _, binding) = session
            .human_current_decision_input_v8(e, e.acting_player)
            .unwrap();
        assert_eq!(observation.extensions.ward_life_card_traits.len(), 1);
        assert_eq!(observation.extensions.ward_life_card_traits[0].life, 3);
        assert!(observation.extensions.ward_life_card_traits[0].effective);
        let mut forged = binding.clone();
        forged.observation.extensions.ward_life_card_traits[0].life = 4;
        let before = session.privileged_core_environment_hash();
        assert!(session
            .human_validate_bound_decision_v8(e, e.acting_player, &forged)
            .is_err());
        assert_eq!(session.privileged_core_environment_hash(), before);
    }
}
