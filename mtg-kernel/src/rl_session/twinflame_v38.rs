//! Private Twinflame binding, separate from frozen observation/session modes.

use super::*;
#[cfg(test)]
use crate::state::GameState;
use crate::twinflame_observation_v1::TwinflameObservationV1;

fn is_combat(state: &crate::state::GameState) -> bool {
    crate::twinflame_tyrant_v1::combat::has_pending(state)
}
pub(super) fn actual_candidates(
    origin: &PolicyDecisionV5,
    state: &crate::state::GameState,
) -> Result<Vec<CorePolicyActionCandidateV1>, RlContractError> {
    if is_combat(state) {
        let decision = crate::twinflame_tyrant_v1::combat::decision(state)
            .map_err(RlContractError)?
            .ok_or_else(|| RlContractError("combat origin absent".into()))?;
        if origin != &PolicyDecisionV5::Surface(crate::surface::SurfaceDecision::Decision(decision))
        {
            return Err(RlContractError(
                "combat origin changed from actual wave decision".into(),
            ));
        }
    }
    core_policy_action_candidates_v5(origin, state)
}

// No Debug/Serialize: executable candidates and the transport observation
// must never become a user-facing receipt or request.
#[derive(Clone, PartialEq)]
pub(crate) struct TwinflameDecisionBindingV38 {
    expected: FastActorDecisionV1,
    candidates: Vec<CorePolicyActionCandidateV1>,
    observation: TwinflameObservationV1,
}

pub(super) fn normalized_origin(
    session: &FastActorSessionV1,
    current: &FastActorCurrentDecisionV1,
) -> Result<Vec<CorePolicyActionCandidateV1>, FlatActionDecisionSliceErrorV1> {
    crate::twinflame_observation_v1::validate_projection_authority(&session.state)
        .map_err(|_| FlatActionDecisionSliceErrorV1::CorruptCurrentBinding)?;
    let mut original = current.clone();
    original.flat_action_cache = None;
    original.flat_action_cache_error = None;
    original.flat_action_cache_v2 = None;
    original.flat_action_cache_error_v2 = None;
    original.candidates = actual_candidates(&current.origin_decision, &session.state)
        .map_err(|_| FlatActionDecisionSliceErrorV1::InvalidDecisionRelation)?;
    flat_validate_current_binding_header_v1(session, &original)?;
    if !is_combat(&session.state) {
        flat_validate_origin_decision_v1(&original, &session.state)?;
    }
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
    crate::twinflame_observation_v1::observe_v1(
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
    pub(crate) fn twinflame_restore_v38(
        &mut self,
        snapshot: &FastActorSessionSnapshotV1,
    ) -> Result<(), RlContractError> {
        let mut restored = snapshot.0.clone();
        if restored.flat_action_contract_mode != FlatActionContractModeV1::TwinflameV38
            || restored.episode_id != self.episode_id
        {
            return Err(RlContractError("wrong Twinflame snapshot owner".into()));
        }
        crate::twinflame_observation_v1::validate_projection_authority(&restored.state)?;
        let next_revision = self
            .environment_revision
            .max(restored.environment_revision)
            .checked_add(1)
            .ok_or_else(|| RlContractError("Twinflame mixed restore revision exhausted".into()))?;
        if let Some(mut current) = restored.current.take() {
            // Authenticate the original snapshot relation before rebinding its owner.
            prepare_current(&restored, &mut current)
                .map_err(|_| RlContractError("invalid original Twinflame restored menu".into()))?;
            restored
                .surface
                .rebind_owned_revision_after_restore_v1(
                    &restored.state,
                    restored.environment_revision,
                    next_revision,
                )
                .map_err(|_| {
                    RlContractError("invalid original restored combat scan owner".into())
                })?;
            restored.environment_revision = next_revision;
            current.environment_revision = next_revision;
            prepare_current(&restored, &mut current)
                .map_err(|_| RlContractError("invalid rebound Twinflame restored menu".into()))?;
            restored.current = Some(current);
        } else {
            restored.environment_revision = next_revision;
        }
        *self = restored;
        Ok(())
    }
    /// Explicit backend opt-in for the separate Twinflame projector.
    /// It consumes the session; predecessor adapters keep their old mode.
    pub fn into_twinflame_v38(mut self) -> Result<Self, RlContractError> {
        // Validate before any advancement can expire a malformed active marker.
        crate::twinflame_observation_v1::validate_projection_authority(&self.state)?;
        self.flat_action_contract_mode = FlatActionContractModeV1::TwinflameV38;
        self.flat_action_cache_spare = None;
        self.flat_action_cache_spare_v2 = None;
        if let Some(mut current) = self.current.take() {
            prepare_current(&self, &mut current)
                .map_err(|error| RlContractError(format!("twinflame_v38:{error:?}")))?;
            self.current = Some(current);
        }
        Ok(self)
    }

    #[cfg(test)]
    pub(crate) fn from_twinflame_v38_fixture_state(state: GameState) -> Self {
        let mut session = Self::reset_with_limits(23, 91, 10_000, 10_000);
        session.state = state;
        session.surface = PolicySurfaceV5::new_for_session();
        session.environment_revision = 0;
        session.policy_step_count = 0;
        session.physical_decision_count = 0;
        session.current = None;
        session.terminal = None;
        session.flat_action_contract_mode = FlatActionContractModeV1::TwinflameV38;
        session.flat_action_cache_spare = None;
        session.flat_action_cache_spare_v2 = None;
        if let Err(error) =
            crate::twinflame_observation_v1::validate_projection_authority(&session.state)
        {
            session.terminal = Some(halted_terminal(
                &session.deck_ids,
                session.deck_hashes,
                session.episode_id,
                format!("fail_closed:twinflame_v38:{error}"),
                session.policy_step_count,
                session.physical_decision_count,
            ));
            return session;
        }
        session.advance_to_decision_or_terminal();
        session
    }

    #[cfg(test)]
    pub(crate) fn twinflame_fixture_state_mut(&mut self) -> &mut GameState {
        &mut self.state
    }

    #[cfg(test)]
    pub(crate) fn twinflame_fixture_state(&self) -> &GameState {
        &self.state
    }

    /// Actor-visible authority plus a complete ordered executable binding.
    /// No predecessor cache or observation constructor is consulted.
    pub(crate) fn twinflame_current_decision_input_v38(
        &self,
        expected: FastActorDecisionV1,
        human_seat: PlayerSeatV1,
    ) -> Result<
        (
            TwinflameObservationV1,
            Vec<ActionSemanticV1>,
            TwinflameDecisionBindingV38,
        ),
        FlatActionDecisionSliceErrorV1,
    > {
        if self.flat_action_contract_mode != FlatActionContractModeV1::TwinflameV38 {
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
        let observation = crate::twinflame_observation_v1::observe_v1(
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
        let binding = TwinflameDecisionBindingV38 {
            expected,
            candidates,
            observation: observation.clone(),
        };
        Ok((observation, actions, binding))
    }

    /// Revalidation is read-only. The projector calls this immediately
    /// before applying its privately mapped index through session.step.
    pub(crate) fn twinflame_validate_bound_decision_v38(
        &self,
        expected: FastActorDecisionV1,
        human_seat: PlayerSeatV1,
        binding: &TwinflameDecisionBindingV38,
    ) -> Result<(), FlatActionDecisionSliceErrorV1> {
        let (_, _, current) = self.twinflame_current_decision_input_v38(expected, human_seat)?;
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
    fn repeated_owning_restore_never_revives_previous_private_twinflame_binding_both_seats() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let state = crate::policy_observation_v6::tests::ready_state_for(actor);
            let mut session = FastActorSessionV1::from_twinflame_v38_fixture_state(state);
            let snapshot = session.snapshot_v1();
            for _ in 0..3 {
                let FastActorResponseV1::Decision(old) = session.current_response() else {
                    panic!("ordinary Twinflame menu");
                };
                let (_, _, binding) = session
                    .twinflame_current_decision_input_v38(old, actor.into())
                    .unwrap();
                session.twinflame_restore_v38(&snapshot).unwrap();
                let before = session.privileged_core_environment_hash();
                assert!(session
                    .twinflame_validate_bound_decision_v38(old, actor.into(), &binding)
                    .is_err());
                assert_eq!(session.privileged_core_environment_hash(), before);
                let FastActorResponseV1::Decision(current) = session.current_response() else {
                    panic!("restored Twinflame menu");
                };
                let (_, _, valid) = session
                    .twinflame_current_decision_input_v38(current, actor.into())
                    .unwrap();
                session
                    .twinflame_validate_bound_decision_v38(current, actor.into(), &valid)
                    .unwrap();
            }
        }
    }
    #[test]
    fn genuine_attacker_scan_restore_rebinds_derived_owner_and_accepts_next_action_both_seats() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = crate::policy_observation_v6::tests::ready_state_for(actor);
            let elf = crate::policy_observation_v6::tests::put(
                &mut state,
                actor,
                "Llanowar Elves",
                Zone::Battlefield,
            );
            state.objects.get_mut(elf).summoning_sick = false;
            for seat in [PlayerId::P0, PlayerId::P1] {
                put(&mut state, seat, "Forest", Zone::Battlefield);
                put(&mut state, seat, "Forest", Zone::Library);
            }
            state.step = crate::state::Step::DeclareAttackers;
            let mut session = FastActorSessionV1::from_twinflame_v38_fixture_state(state);
            let FastActorResponseV1::Decision(old) = session.current_response() else {
                panic!("actual attacker scan");
            };
            assert_eq!(
                old.decision_kind,
                FastActorDecisionKindV1::AttackerInclusion
            );
            let snapshot = session.snapshot_v1();
            let before = session.privileged_core_environment_hash();
            let mut corrupt = snapshot.clone();
            corrupt.0.current.as_mut().unwrap().environment_revision += 1;
            assert!(session.twinflame_restore_v38(&corrupt).is_err());
            assert_eq!(session.privileged_core_environment_hash(), before);
            assert_eq!(
                session.current_response(),
                FastActorResponseV1::Decision(old)
            );
            session.twinflame_restore_v38(&snapshot).unwrap();
            let FastActorResponseV1::Decision(current) = session.current_response() else {
                panic!("restored attacker scan");
            };
            assert_ne!(old.environment_revision, current.environment_revision);
            assert_eq!(
                current.decision_kind,
                FastActorDecisionKindV1::AttackerInclusion
            );
            let candidate = session
                .current
                .as_ref()
                .unwrap()
                .candidates
                .iter()
                .position(|c| {
                    matches!(
                        c.semantic,
                        ActionSemanticV1::ChooseAttackerInclusion { include: true, .. }
                    )
                })
                .unwrap();
            session
                .step(current.episode_id, current.step, candidate as u32)
                .unwrap();
            assert!(session.state.engine.combat.attackers.contains(&elf));
        }
    }
    #[test]
    fn actual_full_ordered_public_binding_and_fixed_seat_are_readonly_both_seats() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready_state_for(actor);
            put(&mut state, actor, "Forest", Zone::Hand);
            put(&mut state, actor, "Mountain", Zone::Hand);
            let mut session = FastActorSessionV1::from_twinflame_v38_fixture_state(state);
            let FastActorResponseV1::Decision(expected) = session.current_response() else {
                panic!("real priority menu");
            };
            assert_eq!(session.twinflame_fixture_state().active_player, actor);
            let (_, _, binding) = session
                .twinflame_current_decision_input_v38(expected, actor.into())
                .unwrap();
            let before = session.privileged_core_environment_hash();
            let mut forged = binding.clone();
            forged.candidates.reverse();
            assert!(session
                .twinflame_validate_bound_decision_v38(expected, actor.into(), &forged)
                .is_err());
            let mut forged = binding.clone();
            forged.observation.producer_sha256.clear();
            assert!(session
                .twinflame_validate_bound_decision_v38(expected, actor.into(), &forged)
                .is_err());
            let mut forged = binding.clone();
            forged.observation.attacked_this_turn[actor.index()] = true;
            assert!(session
                .twinflame_validate_bound_decision_v38(expected, actor.into(), &forged)
                .is_err());
            assert!(session
                .twinflame_current_decision_input_v38(expected, actor.opponent().into())
                .is_err());
            assert_eq!(session.privileged_core_environment_hash(), before);
            session.twinflame_fixture_state_mut().players[actor.index()].life += 1;
            let before = session.privileged_core_environment_hash();
            assert!(session
                .twinflame_validate_bound_decision_v38(expected, actor.into(), &binding)
                .is_err());
            assert_eq!(session.privileged_core_environment_hash(), before);
        }
    }
}
