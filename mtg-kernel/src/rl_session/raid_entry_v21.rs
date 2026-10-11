//! Explicit Raid entry V21 ordered input. Frozen V8/V9 and existing modes remain.
use super::*;
use crate::raid_entry_public_v1::RaidEntryObservationV1;

#[derive(Clone, PartialEq)]
pub(crate) struct RaidEntryDecisionBindingV21 {
    expected: FastActorDecisionV1,
    candidates: Vec<CorePolicyActionCandidateV1>,
    observation: RaidEntryObservationV1,
}

fn validate_state(state: &crate::state::GameState) -> Result<(), RlContractError> {
    crate::effect::validate_pending_effect_choice(state).map_err(RlContractError)?;
    crate::planeswalker_v1::validate_heartfire_recipients_v12(state).map_err(RlContractError)
}

/// Same complete expected identity and old numeric/mechanic refusals, with
/// only the explicitly scoped planeswalker recipient successor admitted.
fn validate_expected(
    session: &FastActorSessionV1,
    current: &FastActorCurrentDecisionV1,
    expected: FastActorDecisionV1,
) -> Result<(), FlatActionDecisionSliceErrorV1> {
    if expected.episode_id != session.episode_id {
        return Err(FlatActionDecisionSliceErrorV1::StaleEpisodeBinding);
    }
    if expected.environment_revision != session.environment_revision {
        return Err(FlatActionDecisionSliceErrorV1::StaleEnvironmentRevision);
    }
    let count = u32::try_from(current.candidates.len())
        .map_err(|_| FlatActionDecisionSliceErrorV1::CheckedIntegerRange)?;
    if expected.step != session.policy_step_count
        || expected.physical_decision_id != current.physical_decision_id
        || expected.substep_index != current.substep_index
        || expected.substep_count != current.substep_count
        || expected.acting_player != PlayerSeatV1::from(current.actor)
        || expected.decision_kind != current.decision_kind
        || expected.legal_action_count != count
    {
        return Err(FlatActionDecisionSliceErrorV1::DecisionMetadataMismatch);
    }
    if session.state.objects.iter().any(|(_, object)| {
        i16::try_from(object.counters.plus1_plus1).is_err()
            || u16::try_from(object.damage).is_err()
    }) || session.state.trigger_uses_v1.is_some()
        || session.state.engine.active_replacements.iter().any(|replacement| {
            matches!(replacement.kind,
                crate::event::ReplacementEffectKind::PreventCombatDamageToObjectUntilEndOfTurn { .. })
        })
    {
        return Err(FlatActionDecisionSliceErrorV1::UnsupportedActionSemantic);
    }
    validate_state(&session.state)
        .map_err(|_| FlatActionDecisionSliceErrorV1::CorruptCurrentBinding)
}

pub(super) fn normalized_origin(
    session: &FastActorSessionV1,
    current: &FastActorCurrentDecisionV1,
) -> Result<Vec<CorePolicyActionCandidateV1>, FlatActionDecisionSliceErrorV1> {
    validate_state(&session.state)
        .map_err(|_| FlatActionDecisionSliceErrorV1::CorruptCurrentBinding)?;
    // Reuse the genuine ordered candidate producer and original decision
    // relations. This helper does not use the frozen expected-state guard.
    super::human_v8::normalized_origin(session, current)
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
    #[cfg(test)]
    pub(crate) fn from_raid_entry_v21_fixture_state(state: GameState) -> Self {
        let mut session = Self::reset_with_limits(23, 91, 10_000, 10_000);
        session.state = state;
        session.surface = PolicySurfaceV5::new_for_session();
        session.environment_revision = 0;
        session.policy_step_count = 0;
        session.physical_decision_count = 0;
        session.current = None;
        session.terminal = None;
        session.flat_action_contract_mode = FlatActionContractModeV1::RaidEntryV21;
        session.flat_action_cache_spare = None;
        session.flat_action_cache_spare_v2 = None;
        session.advance_to_decision_or_terminal();
        session
    }
    pub fn into_raid_entry_v21(mut self) -> Result<Self, RlContractError> {
        validate_state(&self.state)?;
        self.flat_action_contract_mode = FlatActionContractModeV1::RaidEntryV21;
        self.flat_action_cache_spare = None;
        self.flat_action_cache_spare_v2 = None;
        if let Some(mut current) = self.current.take() {
            prepare_current(&self, &mut current)
                .map_err(|error| RlContractError(format!("raid_entry_v21:{error:?}")))?;
            self.current = Some(current);
        }
        Ok(self)
    }

    pub(crate) fn raid_entry_current_decision_input_v21(
        &self,
        expected: FastActorDecisionV1,
        human: PlayerSeatV1,
    ) -> Result<
        (
            RaidEntryObservationV1,
            Vec<ActionSemanticV1>,
            RaidEntryDecisionBindingV21,
        ),
        FlatActionDecisionSliceErrorV1,
    > {
        if self.flat_action_contract_mode != FlatActionContractModeV1::RaidEntryV21 {
            return Err(FlatActionDecisionSliceErrorV1::InvalidDecisionRelation);
        }
        let current = self
            .current
            .as_ref()
            .ok_or(FlatActionDecisionSliceErrorV1::NoCurrentDecision)?;
        validate_expected(self, current, expected)?;
        if PlayerSeatV1::from(current.actor) != human {
            return Err(FlatActionDecisionSliceErrorV1::ActingPlayerMismatch);
        }
        let candidates = normalized_origin(self, current)?;
        if candidates != current.candidates {
            return Err(FlatActionDecisionSliceErrorV1::CorruptCurrentBinding);
        }
        let observation = crate::raid_entry_public_v1::observe_v1(
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
        let binding = RaidEntryDecisionBindingV21 {
            expected,
            candidates,
            observation: observation.clone(),
        };
        Ok((observation, actions, binding))
    }

    pub(crate) fn raid_entry_validate_bound_decision_v21(
        &self,
        expected: FastActorDecisionV1,
        human: PlayerSeatV1,
        binding: &RaidEntryDecisionBindingV21,
    ) -> Result<(), FlatActionDecisionSliceErrorV1> {
        let (_, _, actual) = self.raid_entry_current_decision_input_v21(expected, human)?;
        if &actual != binding {
            return Err(FlatActionDecisionSliceErrorV1::CorruptCurrentBinding);
        }
        Ok(())
    }
}
