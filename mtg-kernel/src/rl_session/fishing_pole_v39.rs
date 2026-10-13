//! Private FishingPole binding, separate from frozen observation/session modes.

use super::*;
use crate::fishing_pole_observation_v39::FishingPoleObservationV1;
use crate::state::GameState;

#[derive(Clone, PartialEq)]
pub(crate) struct GrantedLabelV39 {
    pub cost: Vec<crate::card_def::CostComponent>,
    pub effect: crate::effect::EffectOp,
    pub fishing: bool,
}
#[derive(Clone, PartialEq)]
pub(crate) struct ClassifiedActionV39 {
    pub semantic: ActionSemanticV1,
    pub granted_label: Option<GrantedLabelV39>,
}
fn classify(
    state: &GameState,
    semantic: ActionSemanticV1,
) -> Result<ClassifiedActionV39, FlatActionDecisionSliceErrorV1> {
    let mut granted_label = None;
    if let ActionSemanticV1::ActivateAbility {
        source,
        ability_index,
        granted_ability,
        ..
    } = &semantic
    {
        if granted_ability.is_none() {
            let id = ObjectId(source.arena_id);
            let live = state
                .objects
                .try_get(id)
                .ok_or(FlatActionDecisionSliceErrorV1::InvalidDecisionRelation)?;
            if usize::from(*ability_index)
                >= crate::card_def::CARD_DEFS[usize::from(live.card_def)]
                    .activated_abilities
                    .len()
            {
                let ability = crate::engine::resolved_activated_ability(
                    live.card_def,
                    *ability_index,
                    state,
                    id,
                )
                .ok_or(FlatActionDecisionSliceErrorV1::InvalidDecisionRelation)?;
                granted_label = Some(GrantedLabelV39 {
                    cost: ability.cost.to_vec(),
                    effect: (ability.effect)(),
                    fishing: crate::fishing_pole_v39::named_cost(ability.cost),
                });
            }
        }
    }
    Ok(ClassifiedActionV39 {
        semantic,
        granted_label,
    })
}

// No Debug/Serialize: executable candidates and the transport observation
// must never become a user-facing receipt or request.
#[derive(Clone, PartialEq)]
pub(crate) struct FishingPoleDecisionBindingV39 {
    expected: FastActorDecisionV1,
    candidates: Vec<CorePolicyActionCandidateV1>,
    observation: FishingPoleObservationV1,
    classified: Vec<ClassifiedActionV39>,
}

pub(super) fn normalized_origin(
    session: &FastActorSessionV1,
    current: &FastActorCurrentDecisionV1,
) -> Result<Vec<CorePolicyActionCandidateV1>, FlatActionDecisionSliceErrorV1> {
    crate::fishing_pole_observation_v39::validate_projection_authority(&session.state)
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
    crate::fishing_pole_observation_v39::observe_v1(
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
    pub(crate) fn fishing_pole_restore_v39(
        &mut self,
        snapshot: &FastActorSessionSnapshotV1,
    ) -> Result<(), RlContractError> {
        let mut restored = snapshot.0.clone();
        if restored.flat_action_contract_mode != FlatActionContractModeV1::FishingPoleV39
            || restored.episode_id != self.episode_id
        {
            return Err(RlContractError("wrong FishingPole snapshot".into()));
        }
        crate::fishing_pole_observation_v39::validate_projection_authority(&restored.state)?;
        restored.environment_revision = self
            .environment_revision
            .max(restored.environment_revision)
            .checked_add(1)
            .ok_or_else(|| RlContractError("Fishing restore revision exhausted".into()))?;
        if let Some(mut current) = restored.current.take() {
            prepare_current(&restored, &mut current)
                .map_err(|_| RlContractError("invalid restored FishingPole menu".into()))?;
            restored.current = Some(current);
        }
        *self = restored;
        Ok(())
    }
    /// Explicit backend opt-in for the separate FishingPole projector.
    /// It consumes the session; predecessor adapters keep their old mode.
    pub fn into_fishing_pole_v39(mut self) -> Result<Self, RlContractError> {
        // Validate before any advancement can expire a malformed active marker.
        crate::fishing_pole_observation_v39::validate_projection_authority(&self.state)?;
        self.flat_action_contract_mode = FlatActionContractModeV1::FishingPoleV39;
        self.flat_action_cache_spare = None;
        self.flat_action_cache_spare_v2 = None;
        if let Some(mut current) = self.current.take() {
            prepare_current(&self, &mut current)
                .map_err(|error| RlContractError(format!("fishing_pole_v39:{error:?}")))?;
            self.current = Some(current);
        }
        Ok(self)
    }

    #[cfg(test)]
    pub(crate) fn from_fishing_pole_v39_fixture_state(state: GameState) -> Self {
        let mut session = Self::reset_with_limits(23, 91, 10_000, 10_000);
        session.state = state;
        session.surface = PolicySurfaceV5::new_for_session();
        session.environment_revision = 0;
        session.policy_step_count = 0;
        session.physical_decision_count = 0;
        session.current = None;
        session.terminal = None;
        session.flat_action_contract_mode = FlatActionContractModeV1::FishingPoleV39;
        session.flat_action_cache_spare = None;
        session.flat_action_cache_spare_v2 = None;
        if let Err(error) =
            crate::fishing_pole_observation_v39::validate_projection_authority(&session.state)
        {
            session.terminal = Some(halted_terminal(
                &session.deck_ids,
                session.deck_hashes,
                session.episode_id,
                format!("fail_closed:fishing_pole_v39:{error}"),
                session.policy_step_count,
                session.physical_decision_count,
            ));
            return session;
        }
        session.advance_to_decision_or_terminal();
        session
    }

    #[cfg(test)]
    pub(crate) fn fishing_pole_fixture_state_mut(&mut self) -> &mut GameState {
        &mut self.state
    }

    /// Actor-visible authority plus a complete ordered executable binding.
    /// No predecessor cache or observation constructor is consulted.
    pub(crate) fn fishing_pole_current_decision_input_v39(
        &self,
        expected: FastActorDecisionV1,
        human_seat: PlayerSeatV1,
    ) -> Result<
        (
            FishingPoleObservationV1,
            Vec<ClassifiedActionV39>,
            FishingPoleDecisionBindingV39,
        ),
        FlatActionDecisionSliceErrorV1,
    > {
        if self.flat_action_contract_mode != FlatActionContractModeV1::FishingPoleV39 {
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
        let observation = crate::fishing_pole_observation_v39::observe_v1(
            &self.state,
            &self.surface,
            current.actor,
            self.policy_step_count,
            current.physical_decision_id,
            current.substep_index,
            current.substep_count,
        )
        .map_err(|_| FlatActionDecisionSliceErrorV1::CorruptCurrentBinding)?;
        let actions: Vec<ActionSemanticV1> = candidates
            .iter()
            .map(|candidate| {
                super::flat_action_v4::frozen_pending_trigger_semantic_v4(
                    &self.state,
                    candidate.semantic.clone(),
                )
            })
            .collect();
        let actions = actions
            .into_iter()
            .map(|semantic| classify(&self.state, semantic))
            .collect::<Result<Vec<_>, _>>()?;
        let binding = FishingPoleDecisionBindingV39 {
            expected,
            candidates,
            observation: observation.clone(),
            classified: actions.clone(),
        };
        Ok((observation, actions, binding))
    }

    /// Revalidation is read-only. The projector calls this immediately
    /// before applying its privately mapped index through session.step.
    pub(crate) fn fishing_pole_validate_bound_decision_v39(
        &self,
        expected: FastActorDecisionV1,
        human_seat: PlayerSeatV1,
        binding: &FishingPoleDecisionBindingV39,
    ) -> Result<(), FlatActionDecisionSliceErrorV1> {
        let (_, _, current) = self.fishing_pole_current_decision_input_v39(expected, human_seat)?;
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
            let mut session = FastActorSessionV1::from_fishing_pole_v39_fixture_state(state);
            let FastActorResponseV1::Decision(expected) = session.current_response() else {
                panic!("real priority decision");
            };
            let (_, _, binding) = session
                .fishing_pole_current_decision_input_v39(expected, actor.into())
                .unwrap();
            let before = session.privileged_core_environment_hash();
            let mut forged = binding.clone();
            forged.candidates.reverse();
            assert!(session
                .fishing_pole_validate_bound_decision_v39(expected, actor.into(), &forged)
                .is_err());
            let mut forged = binding.clone();
            forged.observation.producer_sha256.clear();
            assert!(session
                .fishing_pole_validate_bound_decision_v39(expected, actor.into(), &forged)
                .is_err());
            assert!(session
                .fishing_pole_current_decision_input_v39(expected, actor.opponent().into())
                .is_err());
            assert_eq!(session.privileged_core_environment_hash(), before);
            session.fishing_pole_fixture_state_mut().players[actor.index()].life += 1;
            let before = session.privileged_core_environment_hash();
            assert!(session
                .fishing_pole_validate_bound_decision_v39(expected, actor.into(), &binding)
                .is_err());
            assert_eq!(session.privileged_core_environment_hash(), before);
        }
    }
    #[test]
    fn repeated_actual_owning_restore_invalidates_old_binding_readonly_both_seats() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready_state_for(actor);
            put(&mut state, actor, "Forest", Zone::Hand);
            let mut session = FastActorSessionV1::from_fishing_pole_v39_fixture_state(state);
            let saved = session.snapshot_v1();
            session.fishing_pole_restore_v39(&saved).unwrap();
            let FastActorResponseV1::Decision(expected) = session.current_response() else {
                panic!("genuine restored decision");
            };
            let (_, _, binding) = session
                .fishing_pole_current_decision_input_v39(expected, actor.into())
                .unwrap();
            session.fishing_pole_restore_v39(&saved).unwrap();
            let before = session.privileged_core_environment_hash();
            assert!(session
                .fishing_pole_validate_bound_decision_v39(expected, actor.into(), &binding)
                .is_err());
            assert_eq!(session.privileged_core_environment_hash(), before);
        }
    }
}
