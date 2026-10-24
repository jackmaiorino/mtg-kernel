//! Private Ravenous V51 binding, separate from predecessor session modes.

use super::*;
use crate::ravenous_observation_v51::RavenousObservationV51;

#[cfg(all(
    test,
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
use crate::state::GameState;

// Failure-only CI diagnostics. Stage names are fixed source constants: never
// print the error, decision, state, candidate, journal or hidden operands.
// The original error value is returned unchanged in every build.
pub(super) fn diagnose_origin_error<E>(stage: &'static str, error: E) -> E {
    #[cfg(test)]
    eprintln!("rl_session origin validation failed at {stage}");
    #[cfg(not(test))]
    let _ = stage;
    error
}

// No Debug/Serialize: executable candidates and the transport observation
// must never become a user-facing receipt or request.
#[derive(Clone, PartialEq)]
pub struct RavenousDecisionBindingV51 {
    expected: FastActorDecisionV1,
    candidates: Vec<CorePolicyActionCandidateV1>,
    observation: RavenousObservationV51,
    pub(crate) families: Vec<crate::akroma_family_public_v40::FamilyProjectionV40>,
}

#[cfg(all(
    test,
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
mod tests;

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
        #[cfg(feature = "limited-fdn-fixtures")]
        A::ChooseTriggerOrderNext {
            actor,
            source,
            trigger_index,
            ..
        } => {
            *source = pending_source(*trigger_index, source, *actor)?;
        }
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
        A::ChooseCostTarget { candidate, .. }
            if state
                .engine
                .pending_cast
                .as_ref()
                .is_some_and(|p| p.mandatory_additional_cost.is_some()) =>
        {
            let original = projection
                .surface
                .battlefield
                .iter()
                .flatten()
                .map(|row| &row.stable)
                .find(|row| {
                    row.arena_id == candidate.arena_id
                        && row.zone_change_count == candidate.zone_change_count
                })
                .ok_or(FlatActionDecisionSliceErrorV1::InvalidActionReference)?;
            *candidate = original.clone();
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
    crate::ravenous_observation_v51::validate_state(state).map_err(RlContractError)?;
    #[cfg(feature = "limited-fdn-fixtures")]
    if let Some((player, pending, prefix)) = surface
        .hunter_order_parts_v45(state)
        .map_err(RlContractError)?
    {
        return crate::rl::hunter_trigger_order_candidates_v45(player, pending, prefix, state);
    }
    if let Some(candidates) = crate::divine_actions_v44::candidates_fake_scoped_v50(
        origin, state, surface, true, true, true, true,
    )? {
        return Ok(candidates);
    }
    if crate::twinflame_tyrant_v1::combat::has_pending(state) {
        let decision = crate::twinflame_tyrant_v1::combat::decision(state)
            .map_err(RlContractError)?
            .ok_or_else(|| RlContractError("combat origin absent".into()))?;
        if origin != &PolicyDecisionV5::Surface(SurfaceDecision::Decision(decision)) {
            return Err(RlContractError(
                "combat origin changed from actual wave decision".into(),
            ));
        }
        return crate::rl::core_policy_action_candidates_ravenous_scoped_v51(origin, state);
    }
    if matches!(
        origin,
        PolicyDecisionV5::Surface(crate::surface::SurfaceDecision::Decision(
            crate::engine::Decision::ChooseEntryColorV36 { .. }
        ))
    ) {
        super::banner_v36::actual_candidates(origin, state)
    } else if super::inspiration_v37::is_mill(state) {
        super::inspiration_v37::actual_candidates(origin, state)
    } else {
        crate::rl::core_policy_action_candidates_ravenous_scoped_v51(origin, state)
    }
}
fn validate_additional_origin(
    session: &FastActorSessionV1,
    current: &FastActorCurrentDecisionV1,
) -> Result<bool, FlatActionDecisionSliceErrorV1> {
    if session
        .surface
        .validate_hunter_order_header_v45(
            &session.state,
            session.environment_revision,
            &current.origin_decision,
            (current.substep_index, current.substep_count),
        )
        .map_err(|_| {
            diagnose_origin_error(
                "ravenous_v51.additional.hunter_header",
                FlatActionDecisionSliceErrorV1::InvalidDecisionRelation,
            )
        })?
    {
        return Ok(true);
    }
    if crate::linked_aura_operation_v1::has_pending(&session.state) {
        let PolicyDecisionV5::Surface(SurfaceDecision::Decision(origin)) = &current.origin_decision
        else {
            return Err(diagnose_origin_error(
                "ravenous_v51.additional.linked_origin_shape",
                FlatActionDecisionSliceErrorV1::InvalidDecisionRelation,
            ));
        };
        let mut probe = session.state.clone();
        let actual = crate::engine::advance_until_decision(&mut probe);
        if &actual != origin
            || probe != session.state
            || current.decision_kind != FastActorDecisionKindV1::Surface
            || current.substep_index != 0
            || current.substep_count != 1
        {
            return Err(diagnose_origin_error(
                "ravenous_v51.additional.linked_readonly",
                FlatActionDecisionSliceErrorV1::InvalidDecisionRelation,
            ));
        }
        return Ok(true);
    }
    if crate::policy_observation_variable_cast_v1::variable_cast_context(
        &session.state,
        &current.origin_decision,
    )
    .map_err(|_| {
        diagnose_origin_error(
            "ravenous_v51.additional.variable_context",
            FlatActionDecisionSliceErrorV1::InvalidDecisionRelation,
        )
    })?
    .is_some()
        || matches!(&current.origin_decision,
            PolicyDecisionV5::Surface(SurfaceDecision::Decision(Decision::CastSpellOrPass { castable_spells, .. }))
            if castable_spells.iter().any(|source| session.state.objects.try_get(*source).is_some_and(|object|
                crate::card_def::CARD_DEFS[usize::from(object.card_def)].target_spec == crate::card_def::TargetSpec::KickerControlledCreatures)))
    {
        let PolicyDecisionV5::Surface(SurfaceDecision::Decision(origin)) = &current.origin_decision
        else {
            return Err(diagnose_origin_error(
                "ravenous_v51.additional.variable_origin_shape",
                FlatActionDecisionSliceErrorV1::InvalidDecisionRelation,
            ));
        };
        let mut probe = session.state.clone();
        let actual = crate::engine::advance_until_decision(&mut probe);
        if &actual != origin
            || probe != session.state
            || current.decision_kind != FastActorDecisionKindV1::Surface
            || current.substep_index != 0
            || current.substep_count != 1
        {
            return Err(diagnose_origin_error(
                "ravenous_v51.additional.variable_readonly",
                FlatActionDecisionSliceErrorV1::InvalidDecisionRelation,
            ));
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
    let invalid = || {
        diagnose_origin_error(
            "ravenous_v51.additional.special_origin",
            FlatActionDecisionSliceErrorV1::InvalidDecisionRelation,
        )
    };
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
    normalized_origin_with_parts(session, current).map(|(candidates, _)| candidates)
}

// Keep the genuine complete parts built for normalization within this one
// immutable request. No stored cache/proof or executable caller permission.
fn normalized_origin_with_parts(
    session: &FastActorSessionV1,
    current: &FastActorCurrentDecisionV1,
) -> Result<
    (
        Vec<CorePolicyActionCandidateV1>,
        crate::ravenous_observation_v51::RavenousPublicPartsV51,
    ),
    FlatActionDecisionSliceErrorV1,
> {
    crate::ravenous_observation_v51::validate_state(&session.state)
        .map_err(|_| FlatActionDecisionSliceErrorV1::CorruptCurrentBinding)?;
    // A mandatory cast menu is engine-owned, not an interpreter option menu.
    // Reconstruct the actual published decision without paying or moving anything.
    if session
        .state
        .engine
        .pending_cast
        .as_ref()
        .is_some_and(|p| p.mandatory_additional_cost.is_some())
    {
        let PolicyDecisionV5::Surface(SurfaceDecision::Decision(origin)) = &current.origin_decision
        else {
            return Err(diagnose_origin_error(
                "ravenous_v51.normalized.mandatory_origin_shape",
                FlatActionDecisionSliceErrorV1::InvalidDecisionRelation,
            ));
        };
        let mut probe = session.state.clone();
        let before = serde_json::to_vec(&probe)
            .map_err(|_| FlatActionDecisionSliceErrorV1::CorruptCurrentBinding)?;
        let actual = crate::engine::advance_until_decision(&mut probe);
        if &actual != origin
            || serde_json::to_vec(&probe)
                .map_err(|_| FlatActionDecisionSliceErrorV1::CorruptCurrentBinding)?
                != before
        {
            return Err(diagnose_origin_error(
                "ravenous_v51.normalized.mandatory_readonly",
                FlatActionDecisionSliceErrorV1::InvalidDecisionRelation,
            ));
        }
    }
    let mut original = current.clone();
    original.flat_action_cache = None;
    original.flat_action_cache_error = None;
    original.flat_action_cache_v2 = None;
    original.flat_action_cache_error_v2 = None;
    original.candidates =
        actual_candidates(&current.origin_decision, &session.state, &session.surface).map_err(
            |_| {
                diagnose_origin_error(
                    "ravenous_v51.normalized.actual_candidates",
                    FlatActionDecisionSliceErrorV1::InvalidDecisionRelation,
                )
            },
        )?;
    flat_validate_current_binding_header_akroma_inner(session, &original, true)
        .map_err(|error| diagnose_origin_error("ravenous_v51.normalized.binding_header", error))?;
    if !validate_additional_origin(session, &original).map_err(|error| {
        diagnose_origin_error("ravenous_v51.normalized.additional_origin", error)
    })? && !crate::twinflame_tyrant_v1::combat::has_pending(&session.state)
    {
        flat_validate_origin_decision_common_scoped_v42(
            &original,
            &session.state,
            true,
            crate::rl::CommonProjectionScopeV42::RavenousV51,
        )
        .map_err(|error| diagnose_origin_error("ravenous_v51.normalized.common_origin", error))?;
    }
    let common =
        crate::rl::policy_observation_extensions_ravenous_v51(&session.state, current.actor)
            .map_err(|_| {
                diagnose_origin_error(
                    "ravenous_v51.normalized.common_projection",
                    FlatActionDecisionSliceErrorV1::InvalidDecisionRelation,
                )
            })?;
    if !crate::linked_aura_operation_v1::has_pending(&session.state) {
        super::flat_action_v3::normalize_candidates(
            &mut original.candidates,
            &common,
            session,
            &original.origin_decision,
        )
        .map_err(|error| {
            diagnose_origin_error("ravenous_v51.normalized.candidate_operands", error)
        })?;
    }
    let (projection, families) = crate::ravenous_observation_v51::observe_parts_v51(
        &session.state,
        &session.surface,
        (current.actor, &current.origin_decision),
        session.policy_step_count,
        current.physical_decision_id,
        (current.substep_index, current.substep_count),
    )
    .map_err(|_| {
        diagnose_origin_error(
            "ravenous_v51.normalized.observation",
            FlatActionDecisionSliceErrorV1::InvalidDecisionRelation,
        )
    })?;
    for candidate in &mut original.candidates {
        candidate.semantic = visible_semantic(
            &session.state,
            &projection.projection,
            candidate.semantic.clone(),
        )
        .map_err(|error| {
            diagnose_origin_error("ravenous_v51.normalized.visible_semantic", error)
        })?;
    }
    Ok((original.candidates, (projection, families)))
}

pub(super) fn prepare_current(
    session: &FastActorSessionV1,
    current: &mut FastActorCurrentDecisionV1,
) -> Result<(), FlatActionDecisionSliceErrorV1> {
    if session.flat_action_contract_mode == FlatActionContractModeV1::KaitoV75 {
        return super::kaito_v75::prepare_current(session, current);
    } else if session.flat_action_contract_mode == FlatActionContractModeV1::ChandraV74 {
        return super::chandra_v74::prepare_current(session, current);
    } else if session.flat_action_contract_mode == FlatActionContractModeV1::LilianaV73 {
        return super::liliana_v73::prepare_current(session, current);
    }
    if session.flat_action_contract_mode == FlatActionContractModeV1::TinybonesV72 {
        return super::tinybones_v72::prepare_current(session, current);
    }
    if session.flat_action_contract_mode == FlatActionContractModeV1::MuldrothaV71 {
        return super::muldrotha_v71::prepare_current(session, current);
    }
    if session.flat_action_contract_mode == FlatActionContractModeV1::VivienV70 {
        return super::vivien_v70::prepare_current(session, current);
    }
    if session.flat_action_contract_mode == FlatActionContractModeV1::GenesisWaveV69 {
        return super::genesis_wave_v69::prepare_current(session, current);
    } else if session.flat_action_contract_mode == FlatActionContractModeV1::EndTurnV68 {
        return super::end_turn_v68::prepare_current(session, current);
    }
    if session.flat_action_contract_mode == FlatActionContractModeV1::CallV62 {
        return super::call_v62::prepare_current(session, current);
    }
    if session.flat_action_contract_mode == FlatActionContractModeV1::GraveyardReturnV61 {
        return super::graveyard_return_v61::prepare_current(session, current);
    }
    if session.flat_action_contract_mode == FlatActionContractModeV1::NextFiveV59 {
        return super::next_five_v59::prepare_current(session, current);
    }
    if session.flat_action_contract_mode == FlatActionContractModeV1::PreparedV57 {
        return super::prepared_v57::prepare_current(session, current);
    }
    if session.flat_action_contract_mode == FlatActionContractModeV1::ElendaV52 {
        return super::elenda_v52::prepare_current(session, current);
    }
    // Normalization already builds and validates the complete public parts.
    // Candidate/cache updates below change none of that observation's inputs.
    current.candidates = normalized_origin(session, current)?;
    current.flat_action_cache = None;
    current.flat_action_cache_error = None;
    current.flat_action_cache_v2 = None;
    current.flat_action_cache_error_v2 = None;
    Ok(())
}

impl FastActorSessionV1 {
    pub fn reset_ravenous_v51(
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
                "Ravenous V51 requires the Foundations catalog",
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
                "invalid Ravenous V51 reset",
            ));
        }
        preflight_declared_mainboards_for_scope(
            crate::rl::CommonProjectionScopeV42::ExistingV40,
            [&mainboards[0], &mainboards[1]],
        )?;
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
        crate::ravenous_observation_v51::validate_state(&state)
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
            flat_action_contract_mode: FlatActionContractModeV1::RavenousV51,
            v3_spell_target_reference_adapter: false,
            #[cfg(feature = "gameplay-decision-trace-v1")]
            trace_pending: crate::gameplay_trace_v1::TraceSlot::default(),
            flat_action_cache_spare: None,
            flat_action_cache_spare_v2: None,
            terminal: None,
        };
        session.surface.enable_hunter_order_v45();
        session.advance_to_decision_or_terminal();
        Ok(session)
    }

    pub fn ravenous_restore_v51(
        &mut self,
        snapshot: &FastActorSessionSnapshotV1,
    ) -> Result<(), RlContractError> {
        let mut restored = snapshot.0.clone();
        if !restored.is_common_v40()
            || restored.flat_action_contract_mode != self.flat_action_contract_mode
            || restored.episode_id != self.episode_id
        {
            return Err(RlContractError("wrong Ravenous snapshot owner".into()));
        }
        crate::ravenous_observation_v51::validate_state(&restored.state)
            .map_err(RlContractError)?;
        let next_revision = self
            .environment_revision
            .max(restored.environment_revision)
            .checked_add(1)
            .ok_or_else(|| RlContractError("Ravenous mixed restore revision exhausted".into()))?;
        if let Some(mut current) = restored.current.take() {
            // Authenticate the original snapshot relation before rebinding its owner.
            prepare_current(&restored, &mut current)
                .map_err(|_| RlContractError("invalid original Ravenous restored menu".into()))?;
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
                .map_err(|_| RlContractError("invalid rebound Ravenous restored menu".into()))?;
            restored.current = Some(current);
        } else {
            restored.environment_revision = next_revision;
        }
        crate::graveyard_return_public_v61::require_legacy_visible(
            &restored.state,
            restored.current.as_ref().map(|c| c.actor),
        )
        .map_err(RlContractError)?;
        *self = restored;
        Ok(())
    }
    /// Explicit backend opt-in for the separate Ravenous projector.
    /// It consumes the session; predecessor adapters keep their old mode.
    pub fn into_ravenous_v51(mut self) -> Result<Self, RlContractError> {
        // Validate before any advancement can expire a malformed active marker.
        crate::ravenous_observation_v51::validate_state(&self.state).map_err(RlContractError)?;
        self.flat_action_contract_mode = FlatActionContractModeV1::RavenousV51;
        self.flat_action_cache_spare = None;
        self.flat_action_cache_spare_v2 = None;
        self.surface.enable_hunter_order_v45();
        if let Some(mut current) = self.current.take() {
            self.surface
                .initialize_hunter_order_v45(
                    &self.state,
                    &current.origin_decision,
                    self.environment_revision,
                )
                .map_err(RlContractError)?;
            if let Some((index, count)) = self
                .surface
                .hunter_order_substeps_v45(&self.state)
                .map_err(RlContractError)?
            {
                current.substep_index = index;
                current.substep_count = count;
            }
            prepare_current(&self, &mut current)
                .map_err(|error| RlContractError(format!("ravenous_v51:{error:?}")))?;
            self.current = Some(current);
        }
        Ok(self)
    }

    #[cfg(all(
        test,
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    ))]
    pub(crate) fn from_ravenous_v51_fixture_state(state: GameState) -> Self {
        let mut session = Self::reset_with_limits(23, 91, 10_000, 10_000);
        session.state = state;
        session.surface = PolicySurfaceV5::new_with_priority_mode_v1(
            crate::surface_v2::PriorityModeV1::EngineWindowsV1,
            SuppressionAuditMode::Off,
        );
        session.surface.enable_hunter_order_v45();
        session.environment_revision = 0;
        session.policy_step_count = 0;
        session.physical_decision_count = 0;
        session.current = None;
        session.terminal = None;
        session.flat_action_contract_mode = FlatActionContractModeV1::RavenousV51;
        session.flat_action_cache_spare = None;
        session.flat_action_cache_spare_v2 = None;
        if let Err(error) = crate::ravenous_observation_v51::validate_state(&session.state) {
            session.terminal = Some(halted_terminal(
                &session.deck_ids,
                session.deck_hashes,
                session.episode_id,
                format!("fail_closed:ravenous_v51:{error}"),
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
    pub(crate) fn ravenous_fixture_state(&self) -> &GameState {
        &self.state
    }

    /// Actor-visible Ravenous plus a complete ordered executable binding.
    /// No predecessor cache or observation constructor is consulted.
    pub fn ravenous_current_decision_input_v51(
        &self,
        expected: FastActorDecisionV1,
        human_seat: PlayerSeatV1,
    ) -> Result<
        (
            RavenousObservationV51,
            Vec<ActionSemanticV1>,
            RavenousDecisionBindingV51,
        ),
        FlatActionDecisionSliceErrorV1,
    > {
        crate::continuous_source_frame_v1::with_frame(&self.state, || {
            self.ravenous_current_decision_input_v51_unframed(expected, human_seat)
        })
    }
    // The body of the function above, run inside its source-list frame.
    fn ravenous_current_decision_input_v51_unframed(
        &self,
        expected: FastActorDecisionV1,
        human_seat: PlayerSeatV1,
    ) -> Result<
        (
            RavenousObservationV51,
            Vec<ActionSemanticV1>,
            RavenousDecisionBindingV51,
        ),
        FlatActionDecisionSliceErrorV1,
    > {
        if self.flat_action_contract_mode != FlatActionContractModeV1::RavenousV51 {
            return Err(FlatActionDecisionSliceErrorV1::InvalidDecisionRelation);
        }
        let current = self
            .current
            .as_ref()
            .ok_or(FlatActionDecisionSliceErrorV1::NoCurrentDecision)?;
        flat_validate_expected_decision_ravenous_v51(self, current, expected)?;
        if PlayerSeatV1::from(current.actor) != human_seat {
            return Err(FlatActionDecisionSliceErrorV1::ActingPlayerMismatch);
        }
        let (candidates, (observation, mut families)) =
            normalized_origin_with_parts(self, current)?;
        if candidates != current.candidates {
            return Err(FlatActionDecisionSliceErrorV1::CorruptCurrentBinding);
        }
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
        let binding = RavenousDecisionBindingV51 {
            expected,
            candidates,
            observation: observation.clone(),
            families,
        };
        Ok((observation, actions, binding))
    }

    /// Revalidation is read-only. The projector calls this immediately
    /// before applying its privately mapped index through session.step.
    pub fn ravenous_validate_bound_decision_v51(
        &self,
        expected: FastActorDecisionV1,
        human_seat: PlayerSeatV1,
        binding: &RavenousDecisionBindingV51,
    ) -> Result<(), FlatActionDecisionSliceErrorV1> {
        self.ravenous_validated_current_input_v51(expected, human_seat, binding)
            .map(|_| ())
    }

    /// Return only the public parts of this fresh, fully compared input. The
    /// owning model can normalize them before its native step without capturing
    /// the identical immutable decision a second time. No authority escapes.
    pub(crate) fn ravenous_validated_current_input_v51(
        &self,
        expected: FastActorDecisionV1,
        human_seat: PlayerSeatV1,
        binding: &RavenousDecisionBindingV51,
    ) -> Result<(RavenousObservationV51, Vec<ActionSemanticV1>), FlatActionDecisionSliceErrorV1>
    {
        let (observation, actions, current) =
            self.ravenous_current_decision_input_v51(expected, human_seat)?;
        if current.expected != binding.expected
            || current.candidates != binding.candidates
            || current.observation != binding.observation
            || current.families != binding.families
        {
            return Err(FlatActionDecisionSliceErrorV1::CorruptCurrentBinding);
        }
        Ok((observation, actions))
    }
}
