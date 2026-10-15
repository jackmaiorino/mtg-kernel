//! Private Divine binding, separate from frozen observation/session modes.

use super::*;
use crate::divine_observation_v44::DivineObservationV44;
#[cfg(all(
    test,
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
use crate::state::GameState;

// No Debug/Serialize: executable candidates and the transport observation
// must never become a user-facing receipt or request.
#[derive(Clone, PartialEq)]
pub(crate) struct DivineDecisionBindingV44 {
    expected: FastActorDecisionV1,
    candidates: Vec<CorePolicyActionCandidateV1>,
    observation: DivineObservationV44,
    pub(crate) families: Vec<crate::akroma_family_public_v40::FamilyProjectionV40>,
}

fn visible_semantic(
    state: &crate::state::GameState,
    projection: &crate::rl::PublicObservationProjectionV5,
    semantic: ActionSemanticV1,
) -> Result<ActionSemanticV1, FlatActionDecisionSliceErrorV1> {
    use ActionSemanticV1 as A;
    let mut semantic = super::flat_action_v4::frozen_pending_trigger_semantic_v4(state, semantic);
    let pending_source = |position: usize,
                          source: &CardStableRefV1,
                          actor: PlayerSeatV1|
     -> Result<CardStableRefV1, FlatActionDecisionSliceErrorV1> {
        let Some(row) = projection
            .surface
            .engine_context
            .pending_triggers
            .get(position)
        else {
            return Ok(source.clone());
        };
        let Some(captured) = &row.source else {
            return Ok(source.clone());
        };
        if captured.arena_id != source.arena_id || row.controller != actor {
            return Ok(source.clone());
        }
        Ok(captured.clone())
    };
    match &mut semantic {
        A::OrderTriggers {
            actor,
            pending_sources,
            ..
        } => {
            for (position, source) in pending_sources.iter_mut().enumerate() {
                *source = pending_source(position, source, *actor)?;
            }
        }
        A::ChooseTarget { actor, source, .. } | A::FinishTargetSelection { actor, source, .. } => {
            *source = pending_source(0, source, *actor)?;
        }
        A::ChooseEffectOption { source, .. }
        | A::ChooseEffectTarget { source, .. }
        | A::FinishEffectSelection { source, .. }
        | A::ChooseEffectColor { source, .. }
        | A::ChooseEffectNumber { source, .. }
        | A::ChooseEffectBoolean { source, .. } => {
            if let Some(captured) = projection
                .surface
                .engine_context
                .pending_effect
                .as_ref()
                .and_then(|row| row.source.as_ref())
                .filter(|captured| captured.arena_id == source.arena_id)
            {
                *source = captured.clone();
            }
        }
        _ => {}
    }
    Ok(semantic)
}
pub(super) fn actual_candidates(
    origin: &PolicyDecisionV5,
    state: &crate::state::GameState,
    surface: &PolicySurfaceV5,
) -> Result<Vec<CorePolicyActionCandidateV1>, RlContractError> {
    if let Some(candidates) = crate::divine_actions_v44::candidates(origin, state, surface)? {
        return Ok(candidates);
    }
    if crate::twinflame_tyrant_v1::combat::has_pending(state) {
        return super::twinflame_v38::actual_candidates(origin, state);
    }
    if matches!(
        origin,
        PolicyDecisionV5::Surface(crate::surface::SurfaceDecision::Decision(
            crate::engine::Decision::ChooseEntryColorV36 { .. }
        ))
    ) {
        super::banner_v36::actual_candidates(origin, state)
    } else {
        super::inspiration_v37::actual_candidates(origin, state)
    }
}
fn validate_additional_origin(
    session: &FastActorSessionV1,
    current: &FastActorCurrentDecisionV1,
) -> Result<bool, FlatActionDecisionSliceErrorV1> {
    if crate::policy_observation_variable_cast_v1::variable_cast_context(
        &session.state,
        &current.origin_decision,
    )
    .map_err(|_| FlatActionDecisionSliceErrorV1::InvalidDecisionRelation)?
    .is_some()
        || matches!(&current.origin_decision,
            PolicyDecisionV5::Surface(SurfaceDecision::Decision(Decision::CastSpellOrPass { castable_spells, .. }))
            if castable_spells.iter().any(|source| session.state.objects.try_get(*source).is_some_and(|object|
                crate::card_def::CARD_DEFS[usize::from(object.card_def)].target_spec == crate::card_def::TargetSpec::KickerControlledCreatures)))
    {
        let PolicyDecisionV5::Surface(SurfaceDecision::Decision(origin)) = &current.origin_decision
        else {
            return Err(FlatActionDecisionSliceErrorV1::InvalidDecisionRelation);
        };
        let mut probe = session.state.clone();
        let actual = crate::engine::advance_until_decision(&mut probe);
        if &actual != origin
            || probe != session.state
            || current.decision_kind != FastActorDecisionKindV1::Surface
            || current.substep_index != 0
            || current.substep_count != 1
        {
            return Err(FlatActionDecisionSliceErrorV1::InvalidDecisionRelation);
        }
        return Ok(true);
    }
    let PolicyDecisionV5::Surface(SurfaceDecision::Decision(origin)) = &current.origin_decision
    else {
        return Ok(false);
    };
    if !matches!(
        origin,
        Decision::ChooseLondonMulligan { .. }
            | Decision::ChooseLondonBottom { .. }
            | Decision::ChooseLegendPermanent { .. }
            | Decision::ChooseCombatDamageRange { .. }
            | Decision::ChooseAttackTarget { .. }
    ) {
        return Ok(false);
    }
    let invalid = || FlatActionDecisionSliceErrorV1::InvalidDecisionRelation;
    if current.decision_kind != FastActorDecisionKindV1::Surface
        || current.substep_index != 0
        || current.substep_count != 1
    {
        return Err(invalid());
    }
    match origin {
        Decision::ChooseLondonMulligan { .. } | Decision::ChooseLondonBottom { .. }
            if session.state.london_mulligans_v1.is_none() =>
        {
            return Err(invalid())
        }
        Decision::ChooseCombatDamageRange { .. }
            if session.state.engine.combat.foundations_v1.is_none() =>
        {
            return Err(invalid())
        }
        Decision::ChooseLegendPermanent { .. }
            if session.state.pending_legend_rule_v1.is_none() =>
        {
            return Err(invalid())
        }
        _ => {}
    }
    let mut probe = session.state.clone();
    let before = serde_json::to_vec(&probe).map_err(|_| invalid())?;
    let actual = crate::engine::advance_until_decision(&mut probe);
    if &actual != origin || serde_json::to_vec(&probe).map_err(|_| invalid())? != before {
        return Err(invalid());
    }
    let actor = match origin {
        Decision::ChooseLondonMulligan { player, .. }
        | Decision::ChooseLondonBottom { player, .. }
        | Decision::ChooseLegendPermanent { player, .. }
        | Decision::ChooseCombatDamageRange { player, .. }
        | Decision::ChooseAttackTarget { player, .. } => *player,
        _ => return Err(invalid()),
    };
    if actor != current.actor {
        return Err(invalid());
    }
    Ok(true)
}
pub(super) fn normalized_origin(
    session: &FastActorSessionV1,
    current: &FastActorCurrentDecisionV1,
) -> Result<Vec<CorePolicyActionCandidateV1>, FlatActionDecisionSliceErrorV1> {
    crate::divine_observation_v44::validate_state(&session.state)
        .map_err(|_| FlatActionDecisionSliceErrorV1::CorruptCurrentBinding)?;
    let mut original = current.clone();
    original.flat_action_cache = None;
    original.flat_action_cache_error = None;
    original.flat_action_cache_v2 = None;
    original.flat_action_cache_error_v2 = None;
    original.candidates =
        actual_candidates(&current.origin_decision, &session.state, &session.surface)
            .map_err(|_| FlatActionDecisionSliceErrorV1::InvalidDecisionRelation)?;
    flat_validate_current_binding_header_akroma_inner(session, &original, true)?;
    if !validate_additional_origin(session, &original)?
        && !crate::twinflame_tyrant_v1::combat::has_pending(&session.state)
    {
        flat_validate_origin_decision_common_scoped_v42(
            &original,
            &session.state,
            true,
            crate::rl::CommonProjectionScopeV42::DivineV44,
        )?;
    }
    let common =
        crate::rl::policy_observation_extensions_faebloom_v43(&session.state, current.actor)
            .map_err(|_| FlatActionDecisionSliceErrorV1::InvalidDecisionRelation)?;
    super::flat_action_v3::normalize_candidates(
        &mut original.candidates,
        &common,
        session,
        &original.origin_decision,
    )?;
    let projection = crate::divine_observation_v44::observe_v44(
        &session.state,
        &session.surface,
        (current.actor, &current.origin_decision),
        session.policy_step_count,
        current.physical_decision_id,
        (current.substep_index, current.substep_count),
    )
    .map_err(|_| FlatActionDecisionSliceErrorV1::InvalidDecisionRelation)?;
    for candidate in &mut original.candidates {
        candidate.semantic = visible_semantic(
            &session.state,
            &projection.projection,
            candidate.semantic.clone(),
        )?;
    }
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
    crate::divine_observation_v44::observe_v44(
        &session.state,
        &session.surface,
        (current.actor, &current.origin_decision),
        session.policy_step_count,
        current.physical_decision_id,
        (current.substep_index, current.substep_count),
    )
    .map_err(|_| FlatActionDecisionSliceErrorV1::InvalidDecisionRelation)?;
    Ok(())
}

impl FastActorSessionV1 {
    pub fn reset_divine_v44(
        episode_id: u64,
        env_seed: u64,
        limits: (u64, u64),
        deck_ids: SessionDeckIdsV1,
        mainboards: [Vec<u16>; 2],
        starting_player: PlayerId,
    ) -> Result<Self, RlSessionError> {
        if !cfg!(all(
            feature = "limited-fdn-fixtures",
            not(feature = "standard-magezero-fixtures")
        )) {
            return Err(session_error(
                RlSessionErrorCode::UnsupportedDeck,
                "Divine V44 requires the Foundations catalog",
            ));
        }
        if !matches!(starting_player, PlayerId::P0 | PlayerId::P1)
            || mainboards.iter().any(|deck| {
                deck.len() < 40 || deck.len() > crate::limited_session_v1::MAX_CUSTOM_DECK_CARDS_V1
            })
            || limits.0 == 0
            || limits.1 == 0
        {
            return Err(session_error(
                RlSessionErrorCode::UnsupportedDeck,
                "invalid Divine V44 reset",
            ));
        }
        let mut state = crate::rl::build_deck_pair_state_environment_v2_with_starting_player_v1(
            env_seed,
            &mainboards[0],
            &mainboards[1],
            starting_player,
        )
        .map_err(map_deck_pair_build_error_v2)?;
        crate::combat_damage_v1::enable_foundations_combat_v1(&mut state)
            .map_err(|error| session_error(RlSessionErrorCode::UnsupportedDeck, &error))?;
        crate::london_mulligan_v1::enable_london_mulligans_v1(&mut state)
            .map_err(|error| session_error(RlSessionErrorCode::UnsupportedDeck, &error))?;
        crate::divine_observation_v44::validate_state(&state)
            .map_err(|error| session_error(RlSessionErrorCode::UnsupportedDeck, &error))?;
        let mut session = Self {
            deck_ids,
            deck_hashes: mainboards
                .each_ref()
                .map(|deck| explicit_deck_hash_v1(deck)),
            episode_id,
            max_physical_decisions: limits.0,
            max_policy_steps: limits.1,
            state,
            surface: PolicySurfaceV5::new_with_priority_mode_v1(
                crate::surface_v2::PriorityModeV1::EngineWindowsV1,
                SuppressionAuditMode::Off,
            ),
            environment_revision: 0,
            policy_step_count: 0,
            physical_decision_count: 0,
            current: None,
            flat_action_contract_mode: FlatActionContractModeV1::DivineV44,
            v3_spell_target_reference_adapter: false,
            #[cfg(feature = "gameplay-decision-trace-v1")]
            trace_pending: crate::gameplay_trace_v1::TraceSlot::default(),
            flat_action_cache_spare: None,
            flat_action_cache_spare_v2: None,
            terminal: None,
        };
        session.advance_to_decision_or_terminal();
        Ok(session)
    }

    pub(crate) fn divine_restore_v44(
        &mut self,
        snapshot: &FastActorSessionSnapshotV1,
    ) -> Result<(), RlContractError> {
        let mut restored = snapshot.0.clone();
        if !restored.is_common_v40()
            || restored.flat_action_contract_mode != self.flat_action_contract_mode
            || restored.episode_id != self.episode_id
        {
            return Err(RlContractError("wrong Divine snapshot owner".into()));
        }
        crate::divine_observation_v44::validate_state(&restored.state).map_err(RlContractError)?;
        let next_revision = self
            .environment_revision
            .max(restored.environment_revision)
            .checked_add(1)
            .ok_or_else(|| RlContractError("Divine mixed restore revision exhausted".into()))?;
        if let Some(mut current) = restored.current.take() {
            // Authenticate the original snapshot relation before rebinding its owner.
            prepare_current(&restored, &mut current)
                .map_err(|_| RlContractError("invalid original Divine restored menu".into()))?;
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
                .map_err(|_| RlContractError("invalid rebound Divine restored menu".into()))?;
            restored.current = Some(current);
        } else {
            restored.environment_revision = next_revision;
        }
        *self = restored;
        Ok(())
    }
    /// Explicit backend opt-in for the separate Divine projector.
    /// It consumes the session; predecessor adapters keep their old mode.
    pub fn into_divine_v44(mut self) -> Result<Self, RlContractError> {
        // Validate before any advancement can expire a malformed active marker.
        crate::divine_observation_v44::validate_state(&self.state).map_err(RlContractError)?;
        self.flat_action_contract_mode = FlatActionContractModeV1::DivineV44;
        self.flat_action_cache_spare = None;
        self.flat_action_cache_spare_v2 = None;
        if let Some(mut current) = self.current.take() {
            prepare_current(&self, &mut current)
                .map_err(|error| RlContractError(format!("divine_v44:{error:?}")))?;
            self.current = Some(current);
        }
        Ok(self)
    }

    #[cfg(all(
        test,
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    ))]
    pub(crate) fn from_divine_v44_fixture_state(state: GameState) -> Self {
        let mut session = Self::reset_with_limits(23, 91, 10_000, 10_000);
        session.state = state;
        session.surface = PolicySurfaceV5::new_with_priority_mode_v1(
            crate::surface_v2::PriorityModeV1::EngineWindowsV1,
            SuppressionAuditMode::Off,
        );
        session.environment_revision = 0;
        session.policy_step_count = 0;
        session.physical_decision_count = 0;
        session.current = None;
        session.terminal = None;
        session.flat_action_contract_mode = FlatActionContractModeV1::DivineV44;
        session.flat_action_cache_spare = None;
        session.flat_action_cache_spare_v2 = None;
        if let Err(error) = crate::divine_observation_v44::validate_state(&session.state) {
            session.terminal = Some(halted_terminal(
                &session.deck_ids,
                session.deck_hashes,
                session.episode_id,
                format!("fail_closed:divine_v44:{error}"),
                session.policy_step_count,
                session.physical_decision_count,
            ));
            return session;
        }
        session.advance_to_decision_or_terminal();
        session
    }

    #[cfg(all(
        test,
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    ))]
    pub(crate) fn divine_fixture_state(&self) -> &GameState {
        &self.state
    }
    #[cfg(all(
        test,
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    ))]
    pub(crate) fn divine_fixture_state_mut(&mut self) -> &mut GameState {
        &mut self.state
    }

    /// Actor-visible Divine plus a complete ordered executable binding.
    /// No predecessor cache or observation constructor is consulted.
    pub(crate) fn divine_current_decision_input_v44(
        &self,
        expected: FastActorDecisionV1,
        human_seat: PlayerSeatV1,
    ) -> Result<
        (
            DivineObservationV44,
            Vec<ActionSemanticV1>,
            DivineDecisionBindingV44,
        ),
        FlatActionDecisionSliceErrorV1,
    > {
        if self.flat_action_contract_mode != FlatActionContractModeV1::DivineV44 {
            return Err(FlatActionDecisionSliceErrorV1::InvalidDecisionRelation);
        }
        let current = self
            .current
            .as_ref()
            .ok_or(FlatActionDecisionSliceErrorV1::NoCurrentDecision)?;
        flat_validate_expected_decision_akroma_inner(self, current, expected, true)?;
        if PlayerSeatV1::from(current.actor) != human_seat {
            return Err(FlatActionDecisionSliceErrorV1::ActingPlayerMismatch);
        }
        let candidates = normalized_origin(self, current)?;
        if candidates != current.candidates {
            return Err(FlatActionDecisionSliceErrorV1::CorruptCurrentBinding);
        }
        let (observation, mut families) = crate::divine_observation_v44::observe_parts_v44(
            &self.state,
            &self.surface,
            (current.actor, &current.origin_decision),
            self.policy_step_count,
            current.physical_decision_id,
            (current.substep_index, current.substep_count),
        )
        .map_err(|_| FlatActionDecisionSliceErrorV1::CorruptCurrentBinding)?;
        let actions = candidates
            .iter()
            .map(|candidate| {
                visible_semantic(
                    &self.state,
                    &observation.projection,
                    candidate.semantic.clone(),
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
        let classified = actions
            .iter()
            .cloned()
            .map(|semantic| super::fishing_pole_v39::classify(&self.state, semantic))
            .collect::<Result<Vec<_>, _>>()?;
        for family in &mut families {
            if let crate::akroma_family_public_v40::FamilyProjectionV40::Fishing(fishing) = family {
                fishing.actions = classified.clone();
            }
        }
        let binding = DivineDecisionBindingV44 {
            expected,
            candidates,
            observation: observation.clone(),
            families,
        };
        Ok((observation, actions, binding))
    }

    /// Revalidation is read-only. The projector calls this immediately
    /// before applying its privately mapped index through session.step.
    pub(crate) fn divine_validate_bound_decision_v44(
        &self,
        expected: FastActorDecisionV1,
        human_seat: PlayerSeatV1,
        binding: &DivineDecisionBindingV44,
    ) -> Result<(), FlatActionDecisionSliceErrorV1> {
        let (_, _, current) = self.divine_current_decision_input_v44(expected, human_seat)?;
        if &current != binding {
            return Err(FlatActionDecisionSliceErrorV1::CorruptCurrentBinding);
        }
        Ok(())
    }
}

#[cfg(all(
    test,
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
mod tests;
