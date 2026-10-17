//! Sealed complete Muldrotha V71 owner. Source DTOs remain read-only.
use super::*;
use crate::{
    muldrotha_candidates_v71 as candidates, muldrotha_candidates_v71::MuldrothaCandidatesV71,
    muldrotha_observation_v71 as observation, muldrotha_observation_v71::MuldrothaObservationV71,
    state::GameState,
};

/// A fresh borrow of the genuine private issued origin. No constructor outside
/// this module, no Clone/serde, no mutable reference or persisted authority.
pub(crate) struct IssuedOriginV71<'a> {
    session: &'a FastActorSessionV1,
    current: &'a FastActorCurrentDecisionV1,
}
impl IssuedOriginV71<'_> {
    pub(crate) fn validate(
        &self,
        state: &GameState,
        surface: &PolicySurfaceV5,
        origin: (PlayerId, &PolicyDecisionV5),
        step: u64,
        physical: u64,
        substep: (u32, u32),
    ) -> Result<(), RlContractError> {
        let current = self
            .session
            .current
            .as_ref()
            .ok_or_else(|| RlContractError("V71 issued origin absent".into()))?;
        if !std::ptr::eq(state, &self.session.state)
            || !std::ptr::eq(surface, &self.session.surface)
            || !std::ptr::eq(current, self.current)
            || !std::ptr::eq(origin.1, &current.origin_decision)
            || origin.0 != current.actor
            || step != self.session.policy_step_count
            || physical != current.physical_decision_id
            || substep != (current.substep_index, current.substep_count)
            || self.session.flat_action_contract_mode != FlatActionContractModeV1::MuldrothaV71
        {
            return Err(RlContractError("foreign V71 issued origin".into()));
        }
        let actual = normalized_origin(self.session, current).map_err(binding_error)?;
        if actual != current.candidates {
            return Err(RlContractError("changed V71 issued native pairs".into()));
        }
        Ok(())
    }
}
fn binding_error(error: FlatActionDecisionSliceErrorV1) -> RlContractError {
    RlContractError(format!("Muldrotha V71 binding:{error:?}"))
}
pub(super) fn normalized_origin(
    session: &FastActorSessionV1,
    current: &FastActorCurrentDecisionV1,
) -> Result<Vec<CorePolicyActionCandidateV1>, FlatActionDecisionSliceErrorV1> {
    if session.flat_action_contract_mode != FlatActionContractModeV1::MuldrothaV71 {
        return Err(FlatActionDecisionSliceErrorV1::InvalidDecisionRelation);
    }
    if current.physical_decision_id != session.physical_decision_count
        || current.origin_decision.actor(&session.state) != Some(current.actor)
        || current.decision_kind
            != match &current.origin_decision {
                PolicyDecisionV5::Surface(_) => FastActorDecisionKindV1::Surface,
                PolicyDecisionV5::AttackerInclusion { .. } => {
                    FastActorDecisionKindV1::AttackerInclusion
                }
                PolicyDecisionV5::BlockerInclusion { .. } => {
                    FastActorDecisionKindV1::BlockerInclusion
                }
            }
    {
        return Err(FlatActionDecisionSliceErrorV1::InvalidDecisionRelation);
    }
    observation::validate_state(&session.state)
        .map_err(|_| FlatActionDecisionSliceErrorV1::CorruptCurrentBinding)?;
    let actual =
        super::muldrotha_candidates_v71(&current.origin_decision, &session.state, &session.surface)
            .map_err(|_| FlatActionDecisionSliceErrorV1::InvalidDecisionRelation)?;
    let mut original = current.clone();
    original.candidates = actual;
    flat_validate_current_binding_header_akroma_inner(session, &original, true)?;
    session
        .surface
        .scan_context_for_owned_revision_v1(
            &session.state,
            current.actor,
            session.environment_revision,
        )
        .map_err(|_| FlatActionDecisionSliceErrorV1::InvalidDecisionRelation)?;
    // Nonscan origins were issued by this private revision's native advance.
    // Re-advancing here would append a genuine Drake Enclosing receipt.
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
    observation::observe_parts_v71(
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
    pub(crate) fn reset_muldrotha_v71(
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
                "Muldrotha V71 requires the Foundations catalog",
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
                "invalid Muldrotha V71 reset",
            ));
        }
        preflight_declared_mainboards_for_scope(
            crate::rl::CommonProjectionScopeV42::MuldrothaV71,
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
        crate::muldrotha_observation_v71::validate_state(&state)
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
            flat_action_contract_mode: FlatActionContractModeV1::MuldrothaV71,
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

    pub(crate) fn muldrotha_restore_v71(
        &mut self,
        snapshot: &FastActorSessionSnapshotV1,
    ) -> Result<(), RlContractError> {
        let mut restored = snapshot.0.clone();
        if !restored.is_common_v40()
            || restored.flat_action_contract_mode != self.flat_action_contract_mode
            || restored.episode_id != self.episode_id
        {
            return Err(RlContractError("wrong Muldrotha snapshot owner".into()));
        }
        crate::muldrotha_observation_v71::validate_state(&restored.state)
            .map_err(RlContractError)?;
        let next_revision = self
            .environment_revision
            .max(restored.environment_revision)
            .checked_add(1)
            .ok_or_else(|| RlContractError("Muldrotha restore revision exhausted".into()))?;
        if restored.current.is_some() == restored.terminal.is_some() {
            return Err(RlContractError(
                "invalid Call snapshot terminal/menu relation".into(),
            ));
        }
        if let Some(mut current) = restored.current.take() {
            // Rebuilding alone must not silently repair a forged original menu.
            let actual = normalized_origin(&restored, &current)
                .map_err(|_| RlContractError("invalid original Call restored binding".into()))?;
            if actual != current.candidates {
                return Err(RlContractError(
                    "changed original Call restored candidates".into(),
                ));
            }
            // Authenticate the original snapshot relation before rebinding its owner.
            prepare_current(&restored, &mut current)
                .map_err(|_| RlContractError("invalid original Call restored menu".into()))?;
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
                .map_err(|_| RlContractError("invalid rebound Call restored menu".into()))?;
            restored.current = Some(current);
        } else {
            restored.environment_revision = next_revision;
        }
        *self = restored;
        Ok(())
    }
    /// Explicit backend opt-in for the separate Muldrotha projector.
    /// It consumes the session; predecessor adapters keep their old mode.
    pub(crate) fn into_muldrotha_v71(mut self) -> Result<Self, RlContractError> {
        // Validate before any advancement can expire a malformed active marker.
        crate::muldrotha_observation_v71::validate_state(&self.state).map_err(RlContractError)?;
        self.flat_action_contract_mode = FlatActionContractModeV1::MuldrothaV71;
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
                .map_err(|error| RlContractError(format!("muldrotha_v71:{error:?}")))?;
            self.current = Some(current);
        }
        Ok(self)
    }

    #[cfg(all(
        test,
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    ))]
    pub(crate) fn from_muldrotha_v71_fixture_state(state: GameState) -> Self {
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
        session.flat_action_contract_mode = FlatActionContractModeV1::MuldrothaV71;
        session.flat_action_cache_spare = None;
        session.flat_action_cache_spare_v2 = None;
        if let Err(error) = crate::muldrotha_observation_v71::validate_state(&session.state) {
            session.terminal = Some(halted_terminal(
                &session.deck_ids,
                session.deck_hashes,
                session.episode_id,
                format!("fail_closed:muldrotha_v71:{error}"),
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
    pub(crate) fn muldrotha_fixture_state(&self) -> &GameState {
        &self.state
    }
}

#[derive(Clone, PartialEq)]
struct BindingV71 {
    expected: FastActorDecisionV1,
    original: Vec<CorePolicyActionCandidateV1>,
    public_to_original: Vec<usize>,
    projection: MuldrothaCandidatesV71,
    families: Vec<crate::akroma_family_public_v40::FamilyProjectionV40>,
}
pub struct MuldrothaSessionV71 {
    inner: FastActorSessionV1,
    owner: std::sync::Arc<()>,
    generation: u64,
}
#[derive(Clone)]
pub struct MuldrothaDecisionV71 {
    binding: BindingV71,
    actions: Vec<ActionSemanticV1>,
    owner: std::sync::Arc<()>,
    generation: u64,
}
#[derive(Clone)]
pub struct MuldrothaSnapshotV71 {
    inner: FastActorSessionSnapshotV1,
    owner: std::sync::Arc<()>,
}
impl MuldrothaDecisionV71 {
    pub fn observation(&self) -> &MuldrothaObservationV71 {
        &self.binding.projection.observation
    }
    pub fn candidates_v71(&self) -> &MuldrothaCandidatesV71 {
        &self.binding.projection
    }
    pub fn actions(&self) -> &[ActionSemanticV1] {
        &self.actions
    }
    /// Original native header/count. Public ordinals index actions(), not the
    /// native vector; the private map is applied only inside step_v71.
    pub fn expected(&self) -> FastActorDecisionV1 {
        self.binding.expected
    }
    #[cfg(all(
        test,
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    ))]
    pub(crate) fn fixture_original_index_v71(&self, ordinal: u32) -> Option<u32> {
        usize::try_from(ordinal)
            .ok()
            .and_then(|i| self.binding.public_to_original.get(i))
            .and_then(|i| u32::try_from(*i).ok())
    }
    pub fn family_projections_v71(
        &self,
    ) -> &[crate::akroma_family_public_v40::FamilyProjectionV40] {
        &self.binding.families
    }
}
impl MuldrothaSessionV71 {
    pub fn from_session(session: FastActorSessionV1) -> Result<Self, RlContractError> {
        Ok(Self {
            inner: session.into_muldrotha_v71()?,
            owner: std::sync::Arc::new(()),
            generation: 0,
        })
    }
    /// Preserves the existing Full-deck preflight. Partial printings remain
    /// unavailable through reset until their separate native qualification.
    pub fn reset_explicit_mainboards_v71(
        episode_id: u64,
        env_seed: u64,
        limits: (u64, u64),
        deck_ids: SessionDeckIdsV1,
        mainboards: [Vec<u16>; 2],
        starting_player: PlayerId,
    ) -> Result<Self, RlSessionError> {
        Ok(Self {
            inner: FastActorSessionV1::reset_muldrotha_v71(
                episode_id,
                env_seed,
                limits,
                deck_ids,
                mainboards,
                starting_player,
            )?,
            owner: std::sync::Arc::new(()),
            generation: 0,
        })
    }
    pub fn response(&self) -> FastActorResponseV1 {
        self.inner.current_response()
    }
    pub fn snapshot_v71(&self) -> MuldrothaSnapshotV71 {
        MuldrothaSnapshotV71 {
            inner: self.inner.snapshot_v1(),
            owner: self.owner.clone(),
        }
    }
    pub fn restore_v71(&mut self, snapshot: &MuldrothaSnapshotV71) -> Result<(), RlContractError> {
        if !std::sync::Arc::ptr_eq(&self.owner, &snapshot.owner) {
            return Err(RlContractError("foreign V71 snapshot".into()));
        }
        let generation = self
            .generation
            .checked_add(1)
            .ok_or_else(|| RlContractError("V71 generation exhausted".into()))?;
        // Validate the entire restored projection before committing the restored backend.
        let mut restored = self.inner.clone();
        restored.muldrotha_restore_v71(&snapshot.inner)?;
        let prospective = Self {
            inner: restored,
            owner: self.owner.clone(),
            generation,
        };
        if prospective.inner.current.is_some() {
            prospective.fresh_binding()?;
        }
        *self = prospective;
        Ok(())
    }
    fn fresh_binding(&self) -> Result<BindingV71, RlContractError> {
        let FastActorResponseV1::Decision(expected) = self.response() else {
            return Err(RlContractError(format!(
                "Muldrotha V71 stream is terminal: {:?}",
                self.response()
            )));
        };
        let current = self
            .inner
            .current
            .as_ref()
            .ok_or_else(|| RlContractError("V71 current absent".into()))?;
        flat_validate_expected_decision_ravenous_v51(&self.inner, current, expected)
            .map_err(binding_error)?;
        let proof = IssuedOriginV71 {
            session: &self.inner,
            current,
        };
        let (projection, native) = candidates::project_issued_parts_v71(
            &proof,
            &self.inner.state,
            &self.inner.surface,
            (current.actor, &current.origin_decision),
            self.inner.policy_step_count,
            current.physical_decision_id,
            (current.substep_index, current.substep_count),
        )?;
        if native.original != current.candidates {
            return Err(RlContractError("V71 original pair binding changed".into()));
        }
        let (_, mut families) = observation::observe_parts_v71(
            &self.inner.state,
            &self.inner.surface,
            (current.actor, &current.origin_decision),
            self.inner.policy_step_count,
            current.physical_decision_id,
            (current.substep_index, current.substep_count),
        )?;
        let classified = projection
            .candidates
            .iter()
            .map(|row| super::fishing_pole_v39::classify(&self.inner.state, row.action.clone()))
            .collect::<Result<Vec<_>, _>>()
            .map_err(binding_error)?;
        for family in &mut families {
            if let crate::akroma_family_public_v40::FamilyProjectionV40::Fishing(value) = family {
                value.actions = classified.clone();
            }
        }
        Ok(BindingV71 {
            expected,
            original: native.original,
            public_to_original: native.public_to_original,
            projection,
            families,
        })
    }
    pub fn decision_v71(&self) -> Result<MuldrothaDecisionV71, RlContractError> {
        let binding = self.fresh_binding()?;
        let actions = binding
            .projection
            .candidates
            .iter()
            .map(|row| row.action.clone())
            .collect();
        Ok(MuldrothaDecisionV71 {
            binding,
            actions,
            owner: self.owner.clone(),
            generation: self.generation,
        })
    }
    fn validated_current(
        &self,
        decision: &MuldrothaDecisionV71,
    ) -> Result<BindingV71, RlContractError> {
        if !std::sync::Arc::ptr_eq(&self.owner, &decision.owner)
            || self.generation != decision.generation
        {
            return Err(RlContractError("foreign or stale V71 decision".into()));
        }
        let fresh = self.fresh_binding()?;
        if fresh != decision.binding
            || fresh
                .projection
                .candidates
                .iter()
                .map(|row| row.action.clone())
                .collect::<Vec<_>>()
                != decision.actions
        {
            return Err(RlContractError(
                "changed V71 complete packet or native binding".into(),
            ));
        }
        Ok(fresh)
    }
    pub fn validate_decision_v71(
        &self,
        decision: &MuldrothaDecisionV71,
    ) -> Result<(), RlContractError> {
        self.validated_current(decision).map(|_| ())
    }
    pub fn step_v71(
        &mut self,
        decision: &MuldrothaDecisionV71,
        index: u32,
    ) -> Result<FastActorResponseV1, RlContractError> {
        self.step_with_current_check(decision, index, |_, _| Ok(()), |e| e, |e| e)
    }
    pub(crate) fn step_with_current_check<E>(
        &mut self,
        decision: &MuldrothaDecisionV71,
        index: u32,
        check: impl FnOnce(&MuldrothaCandidatesV71, &[ActionSemanticV1]) -> Result<(), E>,
        binding_error: impl FnOnce(RlContractError) -> E,
        native_error: impl FnOnce(RlContractError) -> E,
    ) -> Result<FastActorResponseV1, E> {
        let fresh = self.validated_current(decision).map_err(binding_error)?;
        check(&fresh.projection, &decision.actions)?;
        let Some(original) = usize::try_from(index)
            .ok()
            .and_then(|i| fresh.public_to_original.get(i))
            .copied()
            .and_then(|i| u32::try_from(i).ok())
        else {
            return Err(native_error(RlContractError(
                "V71 public action index out of range".into(),
            )));
        };
        self.inner
            .step(fresh.expected.episode_id, fresh.expected.step, original)
            .map_err(|e| RlContractError(format!("V71 native step:{e:?}")))
            .map_err(native_error)
    }
    #[cfg(all(
        test,
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    ))]
    pub(crate) fn fixture_state_v71(&self) -> &GameState {
        &self.inner.state
    }
    #[cfg(all(
        test,
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    ))]
    pub(crate) fn fixture_counts_v71(&self) -> (u64, u64) {
        (
            self.inner.policy_step_count(),
            self.inner.physical_decision_count(),
        )
    }
}

#[cfg(all(
    test,
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
pub(crate) mod tests;
