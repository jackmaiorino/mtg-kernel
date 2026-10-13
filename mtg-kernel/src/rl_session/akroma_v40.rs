//! Private Akroma binding, separate from frozen observation/session modes.

use super::*;
use crate::akroma_public_v1::AkromaObservationV1;
#[cfg(test)]
use crate::state::GameState;

// No Debug/Serialize: executable candidates and the transport observation
// must never become a user-facing receipt or request.
#[derive(Clone, PartialEq)]
pub(crate) struct AkromaDecisionBindingV40 {
    expected: FastActorDecisionV1,
    candidates: Vec<CorePolicyActionCandidateV1>,
    observation: AkromaObservationV1,
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
) -> Result<Vec<CorePolicyActionCandidateV1>, RlContractError> {
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
pub(super) fn normalized_origin(
    session: &FastActorSessionV1,
    current: &FastActorCurrentDecisionV1,
) -> Result<Vec<CorePolicyActionCandidateV1>, FlatActionDecisionSliceErrorV1> {
    crate::akroma_public_v1::validate_state(&session.state)
        .map_err(|_| FlatActionDecisionSliceErrorV1::CorruptCurrentBinding)?;
    let mut original = current.clone();
    original.flat_action_cache = None;
    original.flat_action_cache_error = None;
    original.flat_action_cache_v2 = None;
    original.flat_action_cache_error_v2 = None;
    original.candidates = actual_candidates(&current.origin_decision, &session.state)
        .map_err(|_| FlatActionDecisionSliceErrorV1::InvalidDecisionRelation)?;
    flat_validate_current_binding_header_akroma_inner(session, &original, true)?;
    if !crate::twinflame_tyrant_v1::combat::has_pending(&session.state) {
        flat_validate_origin_decision_akroma_inner(&original, &session.state, true)?;
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
    crate::akroma_public_v1::observe_v1(
        &session.state,
        &session.surface,
        current.actor,
        session.policy_step_count,
        current.physical_decision_id,
        (current.substep_index, current.substep_count),
    )
    .map_err(|_| FlatActionDecisionSliceErrorV1::InvalidDecisionRelation)?;
    Ok(())
}

impl FastActorSessionV1 {
    pub(crate) fn akroma_restore_v40(
        &mut self,
        snapshot: &FastActorSessionSnapshotV1,
    ) -> Result<(), RlContractError> {
        let mut restored = snapshot.0.clone();
        if restored.flat_action_contract_mode != FlatActionContractModeV1::AkromaV40
            || restored.episode_id != self.episode_id
        {
            return Err(RlContractError("wrong Akroma snapshot owner".into()));
        }
        crate::akroma_public_v1::validate_state(&restored.state).map_err(RlContractError)?;
        let next_revision = self
            .environment_revision
            .max(restored.environment_revision)
            .checked_add(1)
            .ok_or_else(|| RlContractError("Akroma mixed restore revision exhausted".into()))?;
        if let Some(mut current) = restored.current.take() {
            // Authenticate the original snapshot relation before rebinding its owner.
            prepare_current(&restored, &mut current)
                .map_err(|_| RlContractError("invalid original Akroma restored menu".into()))?;
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
                .map_err(|_| RlContractError("invalid rebound Akroma restored menu".into()))?;
            restored.current = Some(current);
        } else {
            restored.environment_revision = next_revision;
        }
        *self = restored;
        Ok(())
    }
    /// Explicit backend opt-in for the separate Akroma projector.
    /// It consumes the session; predecessor adapters keep their old mode.
    pub fn into_akroma_v40(mut self) -> Result<Self, RlContractError> {
        // Validate before any advancement can expire a malformed active marker.
        crate::akroma_public_v1::validate_state(&self.state).map_err(RlContractError)?;
        self.flat_action_contract_mode = FlatActionContractModeV1::AkromaV40;
        self.flat_action_cache_spare = None;
        self.flat_action_cache_spare_v2 = None;
        if let Some(mut current) = self.current.take() {
            prepare_current(&self, &mut current)
                .map_err(|error| RlContractError(format!("akroma_v40:{error:?}")))?;
            self.current = Some(current);
        }
        Ok(self)
    }

    #[cfg(test)]
    pub(crate) fn from_akroma_v40_fixture_state(state: GameState) -> Self {
        let mut session = Self::reset_with_limits(23, 91, 10_000, 10_000);
        session.state = state;
        session.surface = PolicySurfaceV5::new_for_session();
        session.environment_revision = 0;
        session.policy_step_count = 0;
        session.physical_decision_count = 0;
        session.current = None;
        session.terminal = None;
        session.flat_action_contract_mode = FlatActionContractModeV1::AkromaV40;
        session.flat_action_cache_spare = None;
        session.flat_action_cache_spare_v2 = None;
        if let Err(error) = crate::akroma_public_v1::validate_state(&session.state) {
            session.terminal = Some(halted_terminal(
                &session.deck_ids,
                session.deck_hashes,
                session.episode_id,
                format!("fail_closed:akroma_v40:{error}"),
                session.policy_step_count,
                session.physical_decision_count,
            ));
            return session;
        }
        session.advance_to_decision_or_terminal();
        session
    }

    #[cfg(test)]
    pub(crate) fn akroma_fixture_state(&self) -> &GameState {
        &self.state
    }
    #[cfg(test)]
    pub(crate) fn akroma_fixture_state_mut(&mut self) -> &mut GameState {
        &mut self.state
    }

    /// Actor-visible akroma plus a complete ordered executable binding.
    /// No predecessor cache or observation constructor is consulted.
    pub(crate) fn akroma_current_decision_input_v40(
        &self,
        expected: FastActorDecisionV1,
        human_seat: PlayerSeatV1,
    ) -> Result<
        (
            AkromaObservationV1,
            Vec<ActionSemanticV1>,
            AkromaDecisionBindingV40,
        ),
        FlatActionDecisionSliceErrorV1,
    > {
        if self.flat_action_contract_mode != FlatActionContractModeV1::AkromaV40 {
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
        let (observation, families) = crate::akroma_public_v1::observe_parts_v40(
            &self.state,
            &self.surface,
            current.actor,
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
        let binding = AkromaDecisionBindingV40 {
            expected,
            candidates,
            observation: observation.clone(),
            families,
        };
        Ok((observation, actions, binding))
    }

    /// Revalidation is read-only. The projector calls this immediately
    /// before applying its privately mapped index through session.step.
    pub(crate) fn akroma_validate_bound_decision_v40(
        &self,
        expected: FastActorDecisionV1,
        human_seat: PlayerSeatV1,
        binding: &AkromaDecisionBindingV40,
    ) -> Result<(), FlatActionDecisionSliceErrorV1> {
        let (_, _, current) = self.akroma_current_decision_input_v40(expected, human_seat)?;
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
#[path = "akroma_v40_mixed_tests.rs"]
mod mixed_tests;
#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy_observation_v6::tests::put;
    #[test]
    fn repeated_owning_restore_never_revives_previous_private_union_binding_both_seats() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let state = crate::policy_observation_v6::tests::ready_state_for(actor);
            let mut session = FastActorSessionV1::from_akroma_v40_fixture_state(state);
            let snapshot = session.snapshot_v1();
            for _ in 0..3 {
                let FastActorResponseV1::Decision(old) = session.current_response() else {
                    panic!("ordinary union menu");
                };
                let (_, _, binding) = session
                    .akroma_current_decision_input_v40(old, actor.into())
                    .unwrap();
                session.akroma_restore_v40(&snapshot).unwrap();
                let before = session.privileged_core_environment_hash();
                assert!(session
                    .akroma_validate_bound_decision_v40(old, actor.into(), &binding)
                    .is_err());
                assert_eq!(session.privileged_core_environment_hash(), before);
                let FastActorResponseV1::Decision(current) = session.current_response() else {
                    panic!("restored union menu");
                };
                let (_, _, valid) = session
                    .akroma_current_decision_input_v40(current, actor.into())
                    .unwrap();
                session
                    .akroma_validate_bound_decision_v40(current, actor.into(), &valid)
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
            let mut session = FastActorSessionV1::from_akroma_v40_fixture_state(state);
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
            assert!(session.akroma_restore_v40(&corrupt).is_err());
            assert_eq!(session.privileged_core_environment_hash(), before);
            assert_eq!(
                session.current_response(),
                FastActorResponseV1::Decision(old)
            );
            session.akroma_restore_v40(&snapshot).unwrap();
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
    fn complete_ordered_binding_observation_identity_seat_and_menu_changes_refuse_readonly() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = crate::policy_observation_v6::tests::ready_state_for(actor);
            put(&mut state, actor, "Forest", Zone::Hand);
            put(&mut state, actor, "Mountain", Zone::Hand);
            let mut session = FastActorSessionV1::from_akroma_v40_fixture_state(state);
            let FastActorResponseV1::Decision(expected) = session.current_response() else {
                panic!("real priority decision");
            };
            let (_, _, binding) = session
                .akroma_current_decision_input_v40(expected, actor.into())
                .unwrap();
            let before = session.privileged_core_environment_hash();
            let mut forged = binding.clone();
            forged.candidates.reverse();
            assert!(session
                .akroma_validate_bound_decision_v40(expected, actor.into(), &forged)
                .is_err());
            let mut forged = binding.clone();
            forged.observation.producer_sha256.clear();
            assert!(session
                .akroma_validate_bound_decision_v40(expected, actor.into(), &forged)
                .is_err());
            assert!(session
                .akroma_current_decision_input_v40(expected, actor.opponent().into())
                .is_err());
            assert_eq!(session.privileged_core_environment_hash(), before);
            session.akroma_fixture_state_mut().players[actor.index()].life += 1;
            let before = session.privileged_core_environment_hash();
            assert!(session
                .akroma_validate_bound_decision_v40(expected, actor.into(), &binding)
                .is_err());
            assert_eq!(session.privileged_core_environment_hash(), before);
        }
    }

    #[test]
    #[ignore = "requires genuine Akroma registration; no synthetic registered meaning"]
    #[cfg(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    ))]
    fn actual_cast_grant_fact_forgeries_with_recomputed_content_digest_refuse_readonly() {
        use crate::engine::{self, Action, Decision};
        use crate::mana::ManaColor;
        use sha2::{Digest, Sha256};
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = crate::policy_observation_v6::tests::ready_state_for(actor);
            for player in [actor, actor.opponent()] {
                put(&mut state, player, "Lightning Bolt", Zone::Hand);
            }
            put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
            let memorial = put(&mut state, actor, "Akroma's Memorial", Zone::Hand);
            state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 7;
            engine::step(&mut state, Action::CastSpell(memorial)).unwrap();
            for _ in 0..40 {
                let decision = engine::advance_until_decision(&mut state);
                assert!(
                    matches!(decision, Decision::CastSpellOrPass { .. }),
                    "{decision:?}"
                );
                if state.stack.is_empty() && state.engine.pending_cast.is_none() {
                    break;
                }
                engine::step(&mut state, Action::Pass).unwrap();
            }
            assert_eq!(state.objects.get(memorial).zone, Zone::Battlefield);
            let session = FastActorSessionV1::from_akroma_v40_fixture_state(state);
            let FastActorResponseV1::Decision(expected) = session.current_response() else {
                panic!("actual priority");
            };
            let (_, _, binding) = session
                .akroma_current_decision_input_v40(expected, expected.acting_player)
                .unwrap();
            assert_eq!(binding.observation.grants.len(), 1);
            for mode in 0..4 {
                let mut forged = binding.clone();
                match mode {
                    0 => forged.observation.grants[0].protection_color_mask = 4,
                    1 => {
                        forged.observation.grants[0].keyword_mask ^=
                            crate::card_def::Keywords::HASTE.0
                    }
                    2 => forged.observation.grants[0].recipients.clear(),
                    3 => forged.observation.grants[0].source.zone_change_count += 1,
                    _ => unreachable!(),
                }
                let mut value = serde_json::to_value(&forged.observation).unwrap();
                value.as_object_mut().unwrap().remove("projection_sha256");
                forged.observation.projection_sha256 =
                    Sha256::digest(serde_json::to_vec(&value).unwrap())
                        .iter()
                        .map(|b| format!("{b:02x}"))
                        .collect();
                forged.observation.validate_artifact().unwrap();
                let before = session.privileged_core_environment_hash();
                assert!(session
                    .akroma_validate_bound_decision_v40(expected, expected.acting_player, &forged)
                    .is_err());
                assert_eq!(session.privileged_core_environment_hash(), before);
            }
        }
    }
}
