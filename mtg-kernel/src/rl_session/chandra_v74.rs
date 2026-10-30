//! Sealed complete Chandra V74 owner. Source DTOs remain read-only.
use super::*;
use crate::{
    chandra_candidates_v74 as candidates, chandra_candidates_v74::ChandraCandidatesV74,
    chandra_observation_v74 as observation, chandra_observation_v74::ChandraObservationV74,
    state::GameState,
};

/// A fresh borrow of the genuine private issued origin. No constructor outside
/// this module, no Clone/serde, no mutable reference or persisted authority.
pub(crate) struct IssuedOriginV74<'a> {
    session: &'a FastActorSessionV1,
    current: &'a FastActorCurrentDecisionV1,
}
impl IssuedOriginV74<'_> {
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
            .ok_or_else(|| RlContractError("V74 issued origin absent".into()))?;
        if !std::ptr::eq(state, &self.session.state)
            || !std::ptr::eq(surface, &self.session.surface)
            || !std::ptr::eq(current, self.current)
            || !std::ptr::eq(origin.1, &current.origin_decision)
            || origin.0 != current.actor
            || step != self.session.policy_step_count
            || physical != current.physical_decision_id
            || substep != (current.substep_index, current.substep_count)
            || self.session.flat_action_contract_mode != FlatActionContractModeV1::ChandraV74
        {
            return Err(RlContractError("foreign V74 issued origin".into()));
        }
        let actual = normalized_origin(self.session, current).map_err(binding_error)?;
        if actual != current.candidates {
            return Err(RlContractError("changed V74 issued native pairs".into()));
        }
        Ok(())
    }
}
fn binding_error(error: FlatActionDecisionSliceErrorV1) -> RlContractError {
    RlContractError(format!("Chandra V74 binding:{error:?}"))
}
pub(super) fn normalized_origin(
    session: &FastActorSessionV1,
    current: &FastActorCurrentDecisionV1,
) -> Result<Vec<CorePolicyActionCandidateV1>, FlatActionDecisionSliceErrorV1> {
    if session.flat_action_contract_mode != FlatActionContractModeV1::ChandraV74 {
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
        super::chandra_candidates_v74(&current.origin_decision, &session.state, &session.surface)
            .map_err(|_| {
            super::ravenous_v51::diagnose_origin_error(
                "chandra_v74.normalized.actual_candidates",
                FlatActionDecisionSliceErrorV1::InvalidDecisionRelation,
            )
        })?;
    let mut original = current.clone();
    original.candidates = actual;
    flat_validate_current_binding_header_akroma_inner(session, &original, true).map_err(
        |error| {
            super::ravenous_v51::diagnose_origin_error(
                "chandra_v74.normalized.binding_header",
                error,
            )
        },
    )?;
    session
        .surface
        .scan_context_for_owned_revision_v1(
            &session.state,
            current.actor,
            session.environment_revision,
        )
        .map_err(|_| {
            super::ravenous_v51::diagnose_origin_error(
                "chandra_v74.normalized.scan_context",
                FlatActionDecisionSliceErrorV1::InvalidDecisionRelation,
            )
        })?;
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
    observation::observe_parts_v74(
        &session.state,
        &session.surface,
        (current.actor, &current.origin_decision),
        session.policy_step_count,
        current.physical_decision_id,
        (current.substep_index, current.substep_count),
    )
    .map_err(|_| {
        super::ravenous_v51::diagnose_origin_error(
            "chandra_v74.prepare.observation",
            FlatActionDecisionSliceErrorV1::InvalidDecisionRelation,
        )
    })?;
    Ok(())
}
impl FastActorSessionV1 {
    pub(crate) fn reset_chandra_v74(
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
                "Chandra V74 requires the Foundations catalog",
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
                "invalid Chandra V74 reset",
            ));
        }
        preflight_declared_mainboards_for_scope(
            crate::rl::CommonProjectionScopeV42::ChandraV74,
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
        crate::chandra_observation_v74::validate_state(&state)
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
            flat_action_contract_mode: FlatActionContractModeV1::ChandraV74,
            v3_spell_target_reference_adapter: false,
            #[cfg(feature = "gameplay-decision-trace-v1")]
            trace_pending: crate::gameplay_trace_v1::TraceSlot::default(),
            flat_action_cache_spare: None,
            flat_action_cache_spare_v2: None,
            pauper_london_v1: None,
            terminal: None,
        };
        session.surface.enable_hunter_order_v45();
        session.advance_to_decision_or_terminal();
        Ok(session)
    }

    pub(crate) fn chandra_restore_v74(
        &mut self,
        snapshot: &FastActorSessionSnapshotV1,
    ) -> Result<(), RlContractError> {
        let mut restored = snapshot.0.clone();
        if !restored.is_common_v40()
            || restored.flat_action_contract_mode != self.flat_action_contract_mode
            || restored.episode_id != self.episode_id
        {
            return Err(RlContractError("wrong Chandra snapshot owner".into()));
        }
        crate::chandra_observation_v74::validate_state(&restored.state).map_err(RlContractError)?;
        let next_revision = self
            .environment_revision
            .max(restored.environment_revision)
            .checked_add(1)
            .ok_or_else(|| RlContractError("Chandra restore revision exhausted".into()))?;
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
                .map_err(|_| RlContractError("invalid original Chandra restored menu".into()))?;
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
                .map_err(|_| RlContractError("invalid rebound Chandra restored menu".into()))?;
            restored.current = Some(current);
        } else {
            restored.environment_revision = next_revision;
        }
        *self = restored;
        Ok(())
    }
    /// Explicit backend opt-in for the separate Chandra projector.
    /// It consumes the session; predecessor adapters keep their old mode.
    pub(crate) fn into_chandra_v74(mut self) -> Result<Self, RlContractError> {
        // Validate before any advancement can expire a malformed active marker.
        crate::chandra_observation_v74::validate_state(&self.state).map_err(RlContractError)?;
        self.flat_action_contract_mode = FlatActionContractModeV1::ChandraV74;
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
                .map_err(|error| RlContractError(format!("chandra_v74:{error:?}")))?;
            self.current = Some(current);
        }
        Ok(self)
    }

    #[cfg(all(
        test,
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    ))]
    pub(crate) fn from_chandra_v74_fixture_state(state: GameState) -> Self {
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
        session.flat_action_contract_mode = FlatActionContractModeV1::ChandraV74;
        session.flat_action_cache_spare = None;
        session.flat_action_cache_spare_v2 = None;
        if let Err(error) = crate::chandra_observation_v74::validate_state(&session.state) {
            session.terminal = Some(halted_terminal(
                &session.deck_ids,
                session.deck_hashes,
                session.episode_id,
                format!("fail_closed:chandra_v74:{error}"),
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
    pub(crate) fn chandra_fixture_state(&self) -> &GameState {
        &self.state
    }
}

#[derive(Clone, PartialEq)]
struct BindingV74 {
    expected: FastActorDecisionV1,
    original: Vec<CorePolicyActionCandidateV1>,
    public_to_original: Vec<usize>,
    projection: ChandraCandidatesV74,
    families: Vec<crate::akroma_family_public_v40::FamilyProjectionV40>,
}
pub struct ChandraSessionV74 {
    inner: FastActorSessionV1,
    owner: std::sync::Arc<()>,
    generation: u64,
}
#[derive(Clone)]
pub struct ChandraDecisionV74 {
    binding: BindingV74,
    actions: Vec<ActionSemanticV1>,
    owner: std::sync::Arc<()>,
    generation: u64,
}
#[derive(Clone)]
pub struct ChandraSnapshotV74 {
    inner: FastActorSessionSnapshotV1,
    owner: std::sync::Arc<()>,
}
impl ChandraDecisionV74 {
    pub fn observation(&self) -> &ChandraObservationV74 {
        &self.binding.projection.observation
    }
    pub fn candidates_v74(&self) -> &ChandraCandidatesV74 {
        &self.binding.projection
    }
    pub fn actions(&self) -> &[ActionSemanticV1] {
        &self.actions
    }
    pub(crate) fn family_projections_v74(
        &self,
    ) -> &[crate::akroma_family_public_v40::FamilyProjectionV40] {
        &self.binding.families
    }
    /// Original native header/count. Public ordinals index actions(), not the
    /// native vector; the private map is applied only inside step_v74.
    pub fn expected(&self) -> FastActorDecisionV1 {
        self.binding.expected
    }
    #[cfg(all(
        test,
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    ))]
    pub(crate) fn fixture_original_index_v74(&self, ordinal: u32) -> Option<u32> {
        usize::try_from(ordinal)
            .ok()
            .and_then(|i| self.binding.public_to_original.get(i))
            .and_then(|i| u32::try_from(*i).ok())
    }
}
impl ChandraSessionV74 {
    pub fn from_session(session: FastActorSessionV1) -> Result<Self, RlContractError> {
        Ok(Self {
            inner: session.into_chandra_v74()?,
            owner: std::sync::Arc::new(()),
            generation: 0,
        })
    }
    /// Preserves the existing Full-deck preflight. Partial printings remain
    /// unavailable through reset until their separate native qualification.
    pub fn reset_explicit_mainboards_v74(
        episode_id: u64,
        env_seed: u64,
        limits: (u64, u64),
        deck_ids: SessionDeckIdsV1,
        mainboards: [Vec<u16>; 2],
        starting_player: PlayerId,
    ) -> Result<Self, RlSessionError> {
        Ok(Self {
            inner: FastActorSessionV1::reset_chandra_v74(
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
    pub fn snapshot_v74(&self) -> ChandraSnapshotV74 {
        ChandraSnapshotV74 {
            inner: self.inner.snapshot_v1(),
            owner: self.owner.clone(),
        }
    }
    pub fn restore_v74(&mut self, snapshot: &ChandraSnapshotV74) -> Result<(), RlContractError> {
        if !std::sync::Arc::ptr_eq(&self.owner, &snapshot.owner) {
            return Err(RlContractError("foreign V74 snapshot".into()));
        }
        let generation = self
            .generation
            .checked_add(1)
            .ok_or_else(|| RlContractError("V74 generation exhausted".into()))?;
        // Validate the entire restored projection before committing the restored backend.
        let mut restored = self.inner.clone();
        restored.chandra_restore_v74(&snapshot.inner)?;
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
    fn fresh_binding(&self) -> Result<BindingV74, RlContractError> {
        let FastActorResponseV1::Decision(expected) = self.response() else {
            return Err(RlContractError(format!(
                "Chandra V74 stream is terminal: {:?}",
                self.response()
            )));
        };
        let current = self
            .inner
            .current
            .as_ref()
            .ok_or_else(|| RlContractError("V74 current absent".into()))?;
        flat_validate_expected_decision_ravenous_v51(&self.inner, current, expected)
            .map_err(binding_error)?;
        let proof = IssuedOriginV74 {
            session: &self.inner,
            current,
        };
        let (projection, native) = candidates::project_issued_parts_v74(
            &proof,
            &self.inner.state,
            &self.inner.surface,
            (current.actor, &current.origin_decision),
            self.inner.policy_step_count,
            current.physical_decision_id,
            (current.substep_index, current.substep_count),
        )?;
        if native.original != current.candidates {
            return Err(RlContractError("V74 original pair binding changed".into()));
        }
        let (_, mut families) = observation::observe_parts_v74(
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
        Ok(BindingV74 {
            expected,
            original: native.original,
            public_to_original: native.public_to_original,
            projection,
            families,
        })
    }
    pub fn decision_v74(&self) -> Result<ChandraDecisionV74, RlContractError> {
        let binding = self.fresh_binding()?;
        let actions = binding
            .projection
            .candidates
            .iter()
            .map(|row| row.action.clone())
            .collect();
        Ok(ChandraDecisionV74 {
            binding,
            actions,
            owner: self.owner.clone(),
            generation: self.generation,
        })
    }
    fn validated_current(
        &self,
        decision: &ChandraDecisionV74,
    ) -> Result<BindingV74, RlContractError> {
        if !std::sync::Arc::ptr_eq(&self.owner, &decision.owner)
            || self.generation != decision.generation
        {
            return Err(RlContractError("foreign or stale V74 decision".into()));
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
                "changed V74 complete packet or native binding".into(),
            ));
        }
        Ok(fresh)
    }

    /// Samples only eligible current unknown slots. Factual native history,
    /// permission menus, costs and the complete original pair binding stay fixed.
    pub fn sampled_clone_v74(
        &self,
        decision: &ChandraDecisionV74,
        seed: u64,
    ) -> Result<Self, RlContractError> {
        let before = self.validated_current(decision)?;
        let actor = self
            .inner
            .current
            .as_ref()
            .ok_or_else(|| RlContractError("V74 sampling has no current decision".into()))?
            .actor;
        let mut inner = self.inner.clone();
        super::flat_action_v4::redeterminize_chandra_v74(&mut inner.state, actor, seed)
            .map_err(|e| RlContractError(format!("V74 current-slot sampling:{e:?}")))?;
        let mut current = inner
            .current
            .take()
            .ok_or_else(|| RlContractError("V74 sampled current decision absent".into()))?;
        prepare_current(&inner, &mut current).map_err(binding_error)?;
        inner.flat_action_cache_spare = None;
        inner.flat_action_cache_spare_v2 = None;
        inner.current = Some(current);
        let sampled = Self {
            inner,
            owner: std::sync::Arc::new(()),
            generation: self.generation,
        };
        if sampled.fresh_binding()? != before {
            return Err(RlContractError(
                "V74 sampled complete public/native binding changed".into(),
            ));
        }
        Ok(sampled)
    }
    pub fn validate_decision_v74(
        &self,
        decision: &ChandraDecisionV74,
    ) -> Result<(), RlContractError> {
        self.validated_current(decision).map(|_| ())
    }
    pub fn step_v74(
        &mut self,
        decision: &ChandraDecisionV74,
        index: u32,
    ) -> Result<FastActorResponseV1, RlContractError> {
        self.step_with_current_check(decision, index, |_, _| Ok(()), |e| e, |e| e)
    }
    pub(crate) fn step_with_current_check<E>(
        &mut self,
        decision: &ChandraDecisionV74,
        index: u32,
        check: impl FnOnce(&ChandraCandidatesV74, &[ActionSemanticV1]) -> Result<(), E>,
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
                "V74 public action index out of range".into(),
            )));
        };
        self.inner
            .step(fresh.expected.episode_id, fresh.expected.step, original)
            .map_err(|e| RlContractError(format!("V74 native step:{e:?}")))
            .map_err(native_error)
    }
    #[cfg(all(
        test,
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    ))]
    pub(crate) fn fixture_state_v74(&self) -> &GameState {
        &self.inner.state
    }
    #[cfg(all(
        test,
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    ))]
    pub(crate) fn fixture_native_clone_v74(&self) -> FastActorSessionV1 {
        self.inner.clone()
    }
    #[cfg(all(
        test,
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    ))]
    pub(crate) fn fixture_counts_v74(&self) -> (u64, u64) {
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
#[path = "chandra_v74/tests.rs"]
pub(crate) mod tests;

#[cfg(all(
    test,
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
mod sampler_tests;
