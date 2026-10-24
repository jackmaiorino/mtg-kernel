//! Private NextFive V59 binding, separate from predecessor session modes.

use super::*;
use crate::next_five_observation_v59::NextFiveObservationV59;
#[cfg(all(
    test,
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
use crate::state::GameState;

// No Debug/Serialize: executable candidates and the transport observation
// must never become a user-facing receipt or request.
#[derive(Clone, PartialEq)]
pub(crate) struct NextFiveDecisionBindingV59 {
    expected: FastActorDecisionV1,
    candidates: Vec<CorePolicyActionCandidateV1>,
    observation: NextFiveObservationV59,
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
    crate::next_five_observation_v59::validate_state(state).map_err(RlContractError)?;
    #[cfg(feature = "limited-fdn-fixtures")]
    if let Some((player, pending, prefix)) = surface
        .hunter_order_parts_v45(state)
        .map_err(RlContractError)?
    {
        return crate::rl::hunter_trigger_order_candidates_v45(player, pending, prefix, state);
    }
    if let Some(candidates) =
        crate::divine_actions_v44::candidates_next_five_v59(origin, state, surface)?
    {
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
        return crate::rl::core_policy_action_candidates_next_five_v59(origin, state);
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
        crate::rl::core_policy_action_candidates_next_five_v59(origin, state)
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
        .map_err(|_| FlatActionDecisionSliceErrorV1::InvalidDecisionRelation)?
    {
        return Ok(true);
    }
    if crate::linked_aura_operation_v1::has_pending(&session.state) {
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
    crate::next_five_observation_v59::validate_state(&session.state)
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
            return Err(FlatActionDecisionSliceErrorV1::InvalidDecisionRelation);
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
            return Err(FlatActionDecisionSliceErrorV1::InvalidDecisionRelation);
        }
    }
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
            crate::rl::CommonProjectionScopeV42::NextFiveV59,
        )?;
    }
    let common =
        crate::rl::policy_observation_extensions_next_five_v59(&session.state, current.actor)
            .map_err(|_| FlatActionDecisionSliceErrorV1::InvalidDecisionRelation)?;
    if !crate::linked_aura_operation_v1::has_pending(&session.state) {
        super::flat_action_v3::normalize_candidates(
            &mut original.candidates,
            &common,
            session,
            &original.origin_decision,
        )?;
    }
    let projection = crate::next_five_observation_v59::observe_v59(
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
            &projection.common.common.projection,
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
    crate::next_five_observation_v59::observe_v59(
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
    pub(crate) fn reset_next_five_v59(
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
                "NextFive V59 requires the Foundations catalog",
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
                "invalid NextFive V59 reset",
            ));
        }
        preflight_declared_mainboards_for_scope(
            crate::rl::CommonProjectionScopeV42::NextFiveV59,
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
        crate::next_five_observation_v59::validate_state(&state)
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
            flat_action_contract_mode: FlatActionContractModeV1::NextFiveV59,
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

    pub(crate) fn next_five_restore_v59(
        &mut self,
        snapshot: &FastActorSessionSnapshotV1,
    ) -> Result<(), RlContractError> {
        let mut restored = snapshot.0.clone();
        if !restored.is_common_v40()
            || restored.flat_action_contract_mode != self.flat_action_contract_mode
            || restored.episode_id != self.episode_id
        {
            return Err(RlContractError("wrong NextFive snapshot owner".into()));
        }
        crate::next_five_observation_v59::validate_state(&restored.state)
            .map_err(RlContractError)?;
        let next_revision = self
            .environment_revision
            .max(restored.environment_revision)
            .checked_add(1)
            .ok_or_else(|| RlContractError("NextFive mixed restore revision exhausted".into()))?;
        if restored.current.is_some() == restored.terminal.is_some() {
            return Err(RlContractError(
                "invalid NextFive snapshot terminal/menu relation".into(),
            ));
        }
        if let Some(mut current) = restored.current.take() {
            // Rebuilding alone must not silently repair a forged original menu.
            let actual = normalized_origin(&restored, &current).map_err(|_| {
                RlContractError("invalid original NextFive restored binding".into())
            })?;
            if actual != current.candidates {
                return Err(RlContractError(
                    "changed original NextFive restored candidates".into(),
                ));
            }
            // Authenticate the original snapshot relation before rebinding its owner.
            prepare_current(&restored, &mut current)
                .map_err(|_| RlContractError("invalid original NextFive restored menu".into()))?;
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
                .map_err(|_| RlContractError("invalid rebound NextFive restored menu".into()))?;
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
    /// Explicit backend opt-in for the separate NextFive projector.
    /// It consumes the session; predecessor adapters keep their old mode.
    pub(crate) fn into_next_five_v59(mut self) -> Result<Self, RlContractError> {
        // Validate before any advancement can expire a malformed active marker.
        crate::next_five_observation_v59::validate_state(&self.state).map_err(RlContractError)?;
        self.flat_action_contract_mode = FlatActionContractModeV1::NextFiveV59;
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
                .map_err(|error| RlContractError(format!("next_five_v59:{error:?}")))?;
            self.current = Some(current);
        }
        Ok(self)
    }

    #[cfg(all(
        test,
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    ))]
    pub(crate) fn from_next_five_v59_fixture_state(state: GameState) -> Self {
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
        session.flat_action_contract_mode = FlatActionContractModeV1::NextFiveV59;
        session.flat_action_cache_spare = None;
        session.flat_action_cache_spare_v2 = None;
        if let Err(error) = crate::next_five_observation_v59::validate_state(&session.state) {
            session.terminal = Some(halted_terminal(
                &session.deck_ids,
                session.deck_hashes,
                session.episode_id,
                format!("fail_closed:next_five_v59:{error}"),
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
    pub(crate) fn next_five_fixture_state(&self) -> &GameState {
        &self.state
    }

    /// Actor-visible NextFive plus a complete ordered executable binding.
    /// No predecessor cache or observation constructor is consulted.
    pub(crate) fn next_five_current_decision_input_v59(
        &self,
        expected: FastActorDecisionV1,
        human_seat: PlayerSeatV1,
    ) -> Result<
        (
            NextFiveObservationV59,
            Vec<ActionSemanticV1>,
            NextFiveDecisionBindingV59,
        ),
        FlatActionDecisionSliceErrorV1,
    > {
        if self.flat_action_contract_mode != FlatActionContractModeV1::NextFiveV59 {
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
        let candidates = normalized_origin(self, current)?;
        if candidates != current.candidates {
            return Err(FlatActionDecisionSliceErrorV1::CorruptCurrentBinding);
        }
        let (observation, mut families) = crate::next_five_observation_v59::observe_parts_v59(
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
                    &observation.common.common.projection,
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
        let binding = NextFiveDecisionBindingV59 {
            expected,
            candidates,
            observation: observation.clone(),
            families,
        };
        Ok((observation, actions, binding))
    }
}

/// Sole owner of a V59 decision stream. Private bindings never cross serde.
pub struct NextFiveSessionV59 {
    inner: FastActorSessionV1,
    owner: std::sync::Arc<()>,
    generation: u64,
}
/// Public facts and ordered choices together with their original private binding.
/// Cloning preserves ownership; public DTOs cannot reconstruct this authority.
#[derive(Clone)]
pub struct NextFiveDecisionV59 {
    expected: FastActorDecisionV1,
    observation: NextFiveObservationV59,
    actions: Vec<ActionSemanticV1>,
    binding: NextFiveDecisionBindingV59,
    owner: std::sync::Arc<()>,
    generation: u64,
}
#[derive(Clone)]
pub struct NextFiveSnapshotV59 {
    inner: FastActorSessionSnapshotV1,
    owner: std::sync::Arc<()>,
}
impl NextFiveDecisionV59 {
    pub fn observation(&self) -> &NextFiveObservationV59 {
        &self.observation
    }
    pub fn actions(&self) -> &[ActionSemanticV1] {
        &self.actions
    }
    pub fn expected(&self) -> FastActorDecisionV1 {
        self.expected
    }
    /// Actor-authorized inherited family DTOs, without executable bindings.
    pub(crate) fn family_projections_v59(
        &self,
    ) -> &[crate::akroma_family_public_v40::FamilyProjectionV40] {
        &self.binding.families
    }
}
impl NextFiveSessionV59 {
    /// Explicit opt-in consumes the original stream and preserves any terminal.
    pub fn from_session(session: FastActorSessionV1) -> Result<Self, RlContractError> {
        Ok(Self {
            inner: session.into_next_five_v59()?,
            owner: std::sync::Arc::new(()),
            generation: 0,
        })
    }
    /// Retains genuine Full-only catalog preflight; no Partial printing is admitted.
    pub fn reset(
        episode_id: u64,
        env_seed: u64,
        limits: (u64, u64),
        deck_ids: SessionDeckIdsV1,
        mainboards: [Vec<u16>; 2],
        starting_player: PlayerId,
    ) -> Result<Self, RlSessionError> {
        let inner = FastActorSessionV1::reset_next_five_v59(
            episode_id,
            env_seed,
            limits,
            deck_ids,
            mainboards,
            starting_player,
        )?;
        Ok(Self {
            inner,
            owner: std::sync::Arc::new(()),
            generation: 0,
        })
    }
    pub fn current_response(&self) -> FastActorResponseV1 {
        self.inner.current_response()
    }
    #[cfg(all(
        test,
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    ))]
    pub(crate) fn fixture_state_v59(&self) -> &GameState {
        &self.inner.state
    }
    #[cfg(all(
        test,
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    ))]
    pub(crate) fn fixture_counts_v59(&self) -> (u64, u64) {
        (
            self.inner.policy_step_count(),
            self.inner.physical_decision_count(),
        )
    }
    #[cfg(all(
        test,
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    ))]
    pub(crate) fn fixture_native_v59(&self) -> FastActorSessionV1 {
        self.inner.clone()
    }
    pub fn snapshot_v59(&self) -> NextFiveSnapshotV59 {
        NextFiveSnapshotV59 {
            inner: self.inner.snapshot_v1(),
            owner: self.owner.clone(),
        }
    }
    pub fn restore_v59(&mut self, snapshot: &NextFiveSnapshotV59) -> Result<(), RlContractError> {
        if !std::sync::Arc::ptr_eq(&self.owner, &snapshot.owner) {
            return Err(RlContractError("foreign NextFive V59 snapshot".into()));
        }
        let generation = self
            .generation
            .checked_add(1)
            .ok_or_else(|| RlContractError("NextFive owner generation exhausted".into()))?;
        self.inner.next_five_restore_v59(&snapshot.inner)?;
        self.generation = generation;
        Ok(())
    }
    pub fn decision_v59(&self) -> Result<NextFiveDecisionV59, RlContractError> {
        let FastActorResponseV1::Decision(expected) = self.inner.current_response() else {
            return Err(RlContractError("NextFive V59 stream is terminal".into()));
        };
        let (observation, actions, binding) = self
            .inner
            .next_five_current_decision_input_v59(expected, expected.acting_player)
            .map_err(|error| RlContractError(format!("NextFive V59 binding:{error:?}")))?;
        Ok(NextFiveDecisionV59 {
            expected,
            observation,
            actions,
            binding,
            owner: self.owner.clone(),
            generation: self.generation,
        })
    }
    fn validated_current_input(
        &self,
        decision: &NextFiveDecisionV59,
    ) -> Result<(NextFiveObservationV59, Vec<ActionSemanticV1>), RlContractError> {
        if !std::sync::Arc::ptr_eq(&self.owner, &decision.owner)
            || decision.generation != self.generation
        {
            return Err(RlContractError(
                "foreign or stale NextFive V59 decision".into(),
            ));
        }
        let (observation, actions, binding) = self
            .inner
            .next_five_current_decision_input_v59(
                decision.expected,
                decision.expected.acting_player,
            )
            .map_err(|error| RlContractError(format!("NextFive V59 binding:{error:?}")))?;
        if binding != decision.binding {
            return Err(RlContractError(format!(
                "NextFive V59 binding:{:?}",
                FlatActionDecisionSliceErrorV1::CorruptCurrentBinding
            )));
        }
        if observation != decision.observation || actions != decision.actions {
            return Err(RlContractError("NextFive V59 public menu changed".into()));
        }
        Ok((observation, actions))
    }
    pub fn validate_decision_v59(
        &self,
        decision: &NextFiveDecisionV59,
    ) -> Result<(), RlContractError> {
        self.validated_current_input(decision).map(|_| ())
    }
    pub fn step_v59(
        &mut self,
        decision: &NextFiveDecisionV59,
        selected_index: u32,
    ) -> Result<FastActorResponseV1, RlContractError> {
        self.step_with_current_check(
            decision,
            selected_index,
            |_, _| Ok(()),
            |error| error,
            |error| error,
        )
    }

    /// A fresh complete input is confined to this exclusive mutable call.
    /// The callback sees only its public values and cannot retain step authority
    /// or mutate this session. The core session step still runs all its guards.
    pub(crate) fn step_with_current_check<E>(
        &mut self,
        decision: &NextFiveDecisionV59,
        selected_index: u32,
        check: impl FnOnce(&NextFiveObservationV59, &[ActionSemanticV1]) -> Result<(), E>,
        binding_error: impl FnOnce(RlContractError) -> E,
        native_error: impl FnOnce(RlContractError) -> E,
    ) -> Result<FastActorResponseV1, E> {
        let (observation, actions) = self
            .validated_current_input(decision)
            .map_err(binding_error)?;
        check(&observation, &actions)?;
        if usize::try_from(selected_index)
            .ok()
            .is_none_or(|index| index >= decision.actions.len())
        {
            return Err(native_error(RlContractError(
                "NextFive V59 action index out of range".into(),
            )));
        }
        self.inner
            .step(
                decision.expected.episode_id,
                decision.expected.step,
                selected_index,
            )
            .map_err(|error| {
                native_error(RlContractError(format!(
                    "NextFive V59 native step:{error:?}"
                )))
            })
    }
}
