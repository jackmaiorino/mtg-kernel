//! V3 authority is separate from frozen V2 despite sharing row storage widths.
//! Validation rebuilds the complete rows from the original private decision.

use super::*;
use crate::policy_observation_v6::{
    HistoricalSourceContextV6, ObservationV6, PolicyObservationExtensionsV6,
};
use crate::state::GameState;

const COMMITMENT_DOMAIN_V3: &[u8] = b"mtg-kernel-flat-action-candidate-v3\0";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FlatActionDecisionBindingV3(pub(crate) FlatActionDecisionBindingV2);

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FlatActionDecisionSliceV3 {
    pub binding: FlatActionDecisionBindingV3,
    pub active_action_count: u32,
    pub active_ref_count: u32,
    pub active_object_count: u16,
}

fn extensions(
    session: &FastActorSessionV1,
    current: &FastActorCurrentDecisionV1,
) -> Result<PolicyObservationExtensionsV6, FlatActionDecisionSliceErrorV1> {
    crate::rl::policy_observation_extensions_v6(&session.state, current.actor)
        .map_err(|_| FlatActionDecisionSliceErrorV1::InvalidDecisionRelation)
}

fn effect_source_mut(semantic: &mut ActionSemanticV1) -> Option<&mut CardStableRefV1> {
    match semantic {
        ActionSemanticV1::ChooseEffectOption { source, .. }
        | ActionSemanticV1::ChooseEffectTarget { source, .. }
        | ActionSemanticV1::FinishEffectSelection { source, .. }
        | ActionSemanticV1::ChooseEffectColor { source, .. }
        | ActionSemanticV1::ChooseEffectNumber { source, .. }
        | ActionSemanticV1::ChooseEffectBoolean { source, .. } => Some(source),
        _ => None,
    }
}

fn normalize_candidates(
    candidates: &mut Vec<CorePolicyActionCandidateV1>,
    extension: &PolicyObservationExtensionsV6,
    state: &GameState,
) -> Result<(), FlatActionDecisionSliceErrorV1> {
    let mut normalized = candidates.clone();
    if let Some(historical) = extension
        .historical_public_sources
        .iter()
        .find(|row| row.context == HistoricalSourceContextV6::PendingEffect)
    {
        for candidate in normalized.iter_mut() {
            if let Some(source) = effect_source_mut(&mut candidate.semantic) {
                if source.arena_id != historical.source.arena_id {
                    return Err(FlatActionDecisionSliceErrorV1::InvalidActionReference);
                }
                *source = historical.source.clone();
            }
        }
    }
    if let Some(search) = &extension.decision_local_library {
        let mut ordered = Vec::with_capacity(normalized.len());
        for candidate in normalized.drain(..) {
            let order = match &candidate.semantic {
                ActionSemanticV1::ChooseEffectTarget {
                    target: TargetRefV1::Object { object },
                    ..
                } => search
                    .cards
                    .iter()
                    .position(|card| card.stable == *object)
                    .ok_or(FlatActionDecisionSliceErrorV1::HiddenActionReference)?,
                ActionSemanticV1::FinishEffectSelection { .. } => usize::MAX,
                _ => return Err(FlatActionDecisionSliceErrorV1::InvalidDecisionRelation),
            };
            ordered.push((order, candidate));
        }
        ordered.sort_by_key(|(order, _)| *order);
        normalized = ordered
            .into_iter()
            .map(|(_, candidate)| candidate)
            .collect();
    }
    // V5's original pair remains the private origin contract. V3 must not
    // offer an exclusion that makes the eventual declaration illegal. Apply
    // the engine's requirement at this creature's own prefix, before scoring.
    normalized.retain(|candidate| match &candidate.semantic {
        ActionSemanticV1::ChooseAttackerInclusion {
            attacker,
            include: false,
            ..
        } => crate::engine::required_goaded_attackers(state, &[ObjectId(attacker.arena_id)])
            .is_empty(),
        _ => true,
    });
    if normalized.is_empty() {
        return Err(FlatActionDecisionSliceErrorV1::InvalidDecisionRelation);
    }
    *candidates = normalized;
    Ok(())
}

pub(super) fn prepare_and_build_v3(
    session: &FastActorSessionV1,
    current: &mut FastActorCurrentDecisionV1,
) -> Result<FlatActionDecisionCacheV2, FlatActionDecisionSliceErrorV1> {
    let extension = extensions(session, current)?;
    normalize_candidates(&mut current.candidates, &extension, &session.state)?;
    build_with_extensions(session, current, &extension)
}

fn extension_object(
    reference: &CardStableRefV1,
    actor: PlayerId,
    group: FlatActionObjectGroupV1,
    ordinal: usize,
) -> Result<FlatActionObjectV2, FlatActionDecisionSliceErrorV1> {
    Ok(FlatActionObjectV2 {
        card_token: flat_card_token_v2(reference.card_db_id),
        group,
        actor_visible_ordinal: u16::try_from(ordinal)
            .map_err(|_| FlatActionDecisionSliceErrorV1::CheckedIntegerRange)?,
        owner_relative: flat_relative_seat_v1(reference.owner, actor.into())?,
        controller_relative: flat_relative_seat_v1(reference.controller, actor.into())?,
        zone: flat_zone_v1(reference.zone),
        zone_change_count: if group == FlatActionObjectGroupV1::DecisionLocalLibrary {
            0
        } else {
            reference.zone_change_count
        },
    })
}

fn build_with_extensions(
    session: &FastActorSessionV1,
    current: &FastActorCurrentDecisionV1,
    extension: &PolicyObservationExtensionsV6,
) -> Result<FlatActionDecisionCacheV2, FlatActionDecisionSliceErrorV1> {
    // Restore the exact engine-origin order in a private validation view.
    // The consumed candidate vector remains V3-canonical and each entry keeps
    // its original executable policy_action, so sorting cannot change intent.
    let raw = core_policy_action_candidates_v5(&current.origin_decision, &session.state)
        .map_err(|_| FlatActionDecisionSliceErrorV1::InvalidDecisionRelation)?;
    let mut original = current.clone();
    original.flat_action_cache = None;
    original.flat_action_cache_v2 = None;
    original.candidates = raw;
    flat_validate_current_binding_header_v1(session, &original)?;
    flat_validate_origin_decision_v1(&original, &session.state)?;
    normalize_candidates(&mut original.candidates, extension, &session.state)?;
    if original.candidates != current.candidates {
        return Err(FlatActionDecisionSliceErrorV1::InvalidDecisionRelation);
    }

    let mut actions = Vec::with_capacity(current.candidates.len());
    let mut pending_refs = Vec::<FlatUnindexedActionRefV2>::new();
    let mut resolved = Vec::<(CardStableRefV1, FlatActionObjectV2)>::new();
    for (action_index, candidate) in current.candidates.iter().enumerate() {
        flat_validate_semantic_policy_pair_v1(candidate)?;
        let action_index = u32::try_from(action_index)
            .map_err(|_| FlatActionDecisionSliceErrorV1::CheckedIntegerRange)?;
        let ref_start = u32::try_from(pending_refs.len())
            .map_err(|_| FlatActionDecisionSliceErrorV1::CheckedIntegerRange)?;
        let core = flat_action_core_and_refs_v1(
            &candidate.semantic,
            current.actor.into(),
            ref_start,
            |role, order_index, associated_order, reference| {
                let historical = if role == FlatActionRefRoleV1::Source {
                    extension
                        .historical_public_sources
                        .iter()
                        .enumerate()
                        .find(|(_, row)| {
                            row.context == HistoricalSourceContextV6::PendingEffect
                                && row.source == *reference
                        })
                } else {
                    None
                };
                let library = extension
                    .decision_local_library
                    .as_ref()
                    .and_then(|search| {
                        search
                            .cards
                            .iter()
                            .position(|card| card.stable == *reference)
                    });
                let object = if let Some((ordinal, _)) = historical {
                    extension_object(
                        reference,
                        current.actor,
                        FlatActionObjectGroupV1::HistoricalPublicSource,
                        ordinal,
                    )?
                } else if let Some(ordinal) = library {
                    extension_object(
                        reference,
                        current.actor,
                        FlatActionObjectGroupV1::DecisionLocalLibrary,
                        ordinal,
                    )?
                } else {
                    flat_visible_action_object_v2(&session.state, current.actor, reference)?
                };
                // Full frozen identity and authority group/ordinal form the
                // key. A historical source and live later incarnation may
                // share an arena id without becoming the same object row.
                if !resolved
                    .iter()
                    .any(|(identity, row)| identity == reference && *row == object)
                {
                    if resolved
                        .iter()
                        .any(|(_, row)| row.canonical_key() == object.canonical_key())
                    {
                        return Err(FlatActionDecisionSliceErrorV1::DuplicateCanonicalObject);
                    }
                    resolved.push((reference.clone(), object));
                }
                pending_refs.push(FlatUnindexedActionRefV2 {
                    action_index,
                    role,
                    order_index,
                    associated_order,
                    object,
                });
                Ok(())
            },
        )?;
        actions.push(core);
    }
    resolved.sort_by_key(|(_, object)| object.canonical_key());
    let objects: Vec<_> = resolved.into_iter().map(|(_, object)| object).collect();
    let refs = pending_refs
        .into_iter()
        .map(|reference| {
            let index = objects
                .binary_search_by_key(&reference.object.canonical_key(), |row| row.canonical_key())
                .map_err(|_| FlatActionDecisionSliceErrorV1::InvalidActionReference)?;
            Ok(FlatActionRefV2 {
                action_index: reference.action_index,
                role: reference.role,
                order_index: reference.order_index,
                associated_order: reference.associated_order,
                card_token: reference.object.card_token,
                object_index: u16::try_from(index)
                    .map_err(|_| FlatActionDecisionSliceErrorV1::CheckedIntegerRange)?,
            })
        })
        .collect::<Result<Vec<_>, FlatActionDecisionSliceErrorV1>>()?;
    let action_count = u32::try_from(actions.len())
        .map_err(|_| FlatActionDecisionSliceErrorV1::CheckedIntegerRange)?;
    let ref_count = u32::try_from(refs.len())
        .map_err(|_| FlatActionDecisionSliceErrorV1::CheckedIntegerRange)?;
    let object_count = u16::try_from(objects.len())
        .map_err(|_| FlatActionDecisionSliceErrorV1::CheckedIntegerRange)?;
    let mut hash = Sha256::new();
    hash.update(COMMITMENT_DOMAIN_V3);
    for version in [
        3u32,
        FLAT_ACTION_REF_ROLE_MAPPING_VERSION_V2,
        FLAT_ACTION_CARD_TOKEN_MAPPING_VERSION_V2,
        3,
    ] {
        hash.update(version.to_le_bytes());
    }
    hash.update(KERNEL_CARDDB_HASH.to_le_bytes());
    hash.update([current.actor.0]);
    hash.update(action_count.to_le_bytes());
    hash.update(ref_count.to_le_bytes());
    hash.update(object_count.to_le_bytes());
    let mut commitment = FlatActionCommitmentHasherV2(hash);
    for (index, object) in objects.iter().copied().enumerate() {
        commitment.update_object(index as u16, object);
    }
    for (index, action) in actions.iter().copied().enumerate() {
        for reference in &refs
            [action.ref_start as usize..action.ref_start as usize + usize::from(action.ref_len)]
        {
            commitment.update_ref(*reference, objects[usize::from(reference.object_index)]);
        }
        commitment.update_action(index as u32, action);
    }
    let mut binding = flat_action_binding_v2(session, current, action_count, commitment.finish());
    binding.slice_version = 3;
    binding.candidate_commitment_version = 3;
    Ok(FlatActionDecisionCacheV2 {
        binding,
        actions,
        refs,
        objects,
        scratch_unindexed_refs: Vec::new(),
        scratch_resolved_objects: Vec::new(),
    })
}

fn validate_cache(
    session: &FastActorSessionV1,
    current: &FastActorCurrentDecisionV1,
    cache: &FlatActionDecisionCacheV2,
) -> Result<(), FlatActionDecisionSliceErrorV1> {
    if session.flat_action_contract_mode != FlatActionContractModeV1::V3 {
        return Err(FlatActionDecisionSliceErrorV1::CorruptCurrentBinding);
    }
    let extension = extensions(session, current)?;
    let rebuilt = build_with_extensions(session, current, &extension)?;
    if rebuilt.binding != cache.binding
        || rebuilt.actions != cache.actions
        || rebuilt.refs != cache.refs
        || rebuilt.objects != cache.objects
    {
        return Err(FlatActionDecisionSliceErrorV1::CorruptCurrentBinding);
    }
    Ok(())
}

impl FastActorSessionV1 {
    #[cfg(test)]
    pub(crate) fn from_v3_fixture_state(state: GameState) -> Self {
        let mut session = Self::reset_with_limits(23, 91, 10_000, 10_000);
        session.state = state;
        session.surface = PolicySurfaceV5::new_for_session();
        session.environment_revision = 0;
        session.policy_step_count = 0;
        session.physical_decision_count = 0;
        session.current = None;
        session.terminal = None;
        session.flat_action_contract_mode = FlatActionContractModeV1::V3;
        session.flat_action_cache_spare = None;
        session.flat_action_cache_spare_v2 = None;
        session.advance_to_decision_or_terminal();
        session
    }

    fn into_flat_action_v3(mut self) -> Self {
        self.flat_action_contract_mode = FlatActionContractModeV1::V3;
        self.flat_action_cache_spare = None;
        self.flat_action_cache_spare_v2 = None;
        if let Some(mut current) = self.current.take() {
            let result = prepare_and_build_v3(&self, &mut current);
            flat_install_action_cache_build_result_v2(&mut current, result);
            self.current = Some(current);
        }
        self
    }

    pub fn reset_with_explicit_decks_and_limits_flat_action_v3_environment_v2(
        episode_id: u64,
        seed: u64,
        max_physical_decisions: u64,
        max_policy_steps: u64,
        deck_ids: SessionDeckIdsV1,
        mainboards: [Vec<u16>; 2],
    ) -> Result<Self, RlSessionError> {
        Ok(
            Self::reset_with_explicit_decks_and_limits_flat_action_v2_environment_v2(
                episode_id,
                seed,
                max_physical_decisions,
                max_policy_steps,
                deck_ids,
                mainboards,
            )?
            .into_flat_action_v3(),
        )
    }

    pub fn reset_with_explicit_decks_and_limits_flat_action_v3_environment_v2_with_starting_player_v1(
        episode_id: u64,
        seed: u64,
        max_physical_decisions: u64,
        max_policy_steps: u64,
        deck_ids: SessionDeckIdsV1,
        mainboards: [Vec<u16>; 2],
        starting_player: PlayerId,
    ) -> Result<Self, RlSessionError> {
        Ok(Self::reset_with_explicit_decks_and_limits_flat_action_v2_environment_v2_with_starting_player_v1(
            episode_id, seed, max_physical_decisions, max_policy_steps, deck_ids, mainboards, starting_player,
        )?.into_flat_action_v3())
    }

    pub fn reset_with_decks_and_limits_flat_action_v3(
        episode_id: u64,
        seed: u64,
        max_physical_decisions: u64,
        max_policy_steps: u64,
        deck_ids: SessionDeckIdsV1,
    ) -> Result<Self, RlSessionError> {
        Ok(Self::reset_with_decks_and_limits_flat_action_v2(
            episode_id,
            seed,
            max_physical_decisions,
            max_policy_steps,
            deck_ids,
        )?
        .into_flat_action_v3())
    }

    fn validated_v3_cache(
        &self,
        expected: FastActorDecisionV1,
    ) -> Result<&FlatActionDecisionCacheV2, FlatActionDecisionSliceErrorV1> {
        let current = self
            .current
            .as_ref()
            .ok_or(FlatActionDecisionSliceErrorV1::NoCurrentDecision)?;
        flat_validate_expected_decision_v1(self, current, expected)?;
        if self.flat_action_contract_mode != FlatActionContractModeV1::V3 {
            return Err(FlatActionDecisionSliceErrorV1::CorruptCurrentBinding);
        }
        if let Some(error) = current.flat_action_cache_error_v2 {
            return Err(error);
        }
        let cache = current
            .flat_action_cache_v2
            .as_ref()
            .ok_or(FlatActionDecisionSliceErrorV1::CorruptCurrentBinding)?;
        validate_cache(self, current, cache)?;
        Ok(cache)
    }

    pub fn encode_current_flat_action_slice_v3(
        &self,
        expected: FastActorDecisionV1,
        buffers: &mut FlatActionDecisionSliceBuffersV2<'_>,
    ) -> Result<FlatActionDecisionSliceV3, FlatActionDecisionSliceErrorV1> {
        let cache = self.validated_v3_cache(expected)?;
        let (a, r, o) = (cache.actions.len(), cache.refs.len(), cache.objects.len());
        if buffers.actions.len() < a {
            return Err(FlatActionDecisionSliceErrorV1::InsufficientActionCapacity {
                required: a,
                available: buffers.actions.len(),
            });
        }
        if buffers.refs.len() < r {
            return Err(FlatActionDecisionSliceErrorV1::InsufficientRefCapacity {
                required: r,
                available: buffers.refs.len(),
            });
        }
        if buffers.objects.len() < o {
            return Err(FlatActionDecisionSliceErrorV1::InsufficientObjectCapacity {
                required: o,
                available: buffers.objects.len(),
            });
        }
        buffers.actions[..a].copy_from_slice(&cache.actions);
        buffers.refs[..r].copy_from_slice(&cache.refs);
        buffers.objects[..o].copy_from_slice(&cache.objects);
        Ok(FlatActionDecisionSliceV3 {
            binding: FlatActionDecisionBindingV3(cache.binding),
            active_action_count: a as u32,
            active_ref_count: r as u32,
            active_object_count: o as u16,
        })
    }

    pub(crate) fn flat_policy_observation_v3(
        &self,
        expected: FastActorDecisionV1,
    ) -> Result<ObservationV6, FlatActionDecisionSliceErrorV1> {
        self.validated_v3_cache(expected)?;
        let current = self.current.as_ref().unwrap();
        crate::rl::observe_policy_v6_unhashed_for_flat_policy(
            &self.state,
            &self.surface,
            current.actor,
            self.policy_step_count,
            current.physical_decision_id,
            current.substep_index,
            current.substep_count,
        )
        .map_err(|_| FlatActionDecisionSliceErrorV1::CorruptCurrentBinding)
    }

    pub(crate) fn flat_policy_validate_cached_binding_v3(
        &self,
        expected: FastActorDecisionV1,
        binding: FlatActionDecisionBindingV3,
    ) -> Result<(), FlatActionDecisionSliceErrorV1> {
        if self.validated_v3_cache(expected)?.binding != binding.0 {
            return Err(FlatActionDecisionSliceErrorV1::CorruptCurrentBinding);
        }
        Ok(())
    }

    pub fn consume_current_flat_action_slice_v3(
        &mut self,
        binding: FlatActionDecisionBindingV3,
        selected_index: u32,
    ) -> Result<FastActorResponseV1, RlSessionError> {
        let FastActorResponseV1::Decision(expected) = self.current_response() else {
            return Err(session_error(
                RlSessionErrorCode::StaleEnvironmentBinding,
                "V3 result has no active decision",
            ));
        };
        self.flat_policy_validate_cached_binding_v3(expected, binding)
            .map_err(|_| {
                session_error(
                    RlSessionErrorCode::StaleEnvironmentBinding,
                    "V3 result does not match the validated current decision",
                )
            })?;
        self.step(
            binding.0.episode_id,
            binding.0.bound_policy_step_count,
            selected_index,
        )
    }
}

/// Resolves a real Arena room trigger, then stages the affected player's next
/// attack declaration with an ordinary creature on either side of the goaded
/// one in engine candidate order.
#[cfg(test)]
pub(crate) fn goaded_attacker_fixture_state_v3(
    goad_first: bool,
) -> (GameState, ObjectId, ObjectId) {
    use crate::engine::{self, Action, Decision};
    use crate::policy_observation_v6::tests::{put, ready_state};
    use crate::state::{
        AbilitySourceContractV4, InitiativeTriggerKindV1, Step, Target, UndercityRoomV1,
    };

    let mut state = ready_state();
    let hunter = put(
        &mut state,
        PlayerId::P0,
        "Avenging Hunter",
        Zone::Battlefield,
    );
    let source = AbilitySourceContractV4::capture(&state, hunter);
    state.initiative = Some(PlayerId::P0);
    state.engine.initiative_source = Some(source);
    let (goaded, ordinary) = if goad_first {
        let goaded = put(
            &mut state,
            PlayerId::P1,
            "Voldaren Epicure",
            Zone::Battlefield,
        );
        let ordinary = put(&mut state, PlayerId::P1, "Myr Enforcer", Zone::Battlefield);
        (goaded, ordinary)
    } else {
        let ordinary = put(&mut state, PlayerId::P1, "Myr Enforcer", Zone::Battlefield);
        let goaded = put(
            &mut state,
            PlayerId::P1,
            "Voldaren Epicure",
            Zone::Battlefield,
        );
        (goaded, ordinary)
    };
    crate::event::log_initiative_trigger(
        &mut state,
        PlayerId::P0,
        source,
        InitiativeTriggerKindV1::UndercityRoom(UndercityRoomV1::Arena),
    )
    .unwrap();
    let triggers = crate::trigger::collect_and_process(&mut state);
    state.engine.pending_triggers.extend(triggers);
    assert!(matches!(engine::advance_until_decision(&mut state),
        Decision::ChooseTargets { ref legal_targets, .. } if legal_targets.contains(&Target::Object(goaded))));
    engine::step(&mut state, Action::ChooseTarget(Target::Object(goaded))).unwrap();
    for _ in 0..48 {
        let decision = engine::advance_until_decision(&mut state);
        if !state.objects.get(goaded).v4.goaded_by.is_empty() {
            state.active_player = PlayerId::P1;
            state.priority_player = PlayerId::P1;
            state.step = Step::DeclareAttackers;
            state.engine.combat = Default::default();
            return (state, goaded, ordinary);
        }
        match decision {
            Decision::CastSpellOrPass { .. } => engine::step(&mut state, Action::Pass).unwrap(),
            Decision::OrderTriggers { ref pending, .. } if pending.len() == 1 => {
                engine::step(&mut state, Action::OrderTriggers(vec![0])).unwrap()
            }
            other => panic!("unexpected Arena goad fixture decision: {other:?}"),
        }
    }
    panic!("Arena goad did not resolve");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy_observation_v6::tests::{
        escape_prefix_state, forest_search_state, forest_search_state_with_hidden_renumbering,
        map_choice_state,
    };

    fn expected(session: &FastActorSessionV1) -> FastActorDecisionV1 {
        match session.current_response() {
            FastActorResponseV1::Decision(decision) => decision,
            _ => panic!("fixture must have an active decision"),
        }
    }

    fn encoded(
        session: &FastActorSessionV1,
    ) -> (FlatActionDecisionSliceV3, Vec<FlatActionObjectV2>) {
        let mut actions = vec![FlatActionCoreV1::default(); 128];
        let mut refs = vec![FlatActionRefV2::default(); 256];
        let mut objects = vec![FlatActionObjectV2::default(); 128];
        let result = session
            .encode_current_flat_action_slice_v3(
                expected(session),
                &mut FlatActionDecisionSliceBuffersV2 {
                    actions: &mut actions,
                    refs: &mut refs,
                    objects: &mut objects,
                },
            )
            .unwrap();
        objects.truncate(usize::from(result.active_object_count));
        (result, objects)
    }

    #[test]
    fn v3_library_choices_are_canonical_private_and_keep_executable_identity() {
        let state = forest_search_state(false, "Lightning Bolt");
        let mut session = FastActorSessionV1::from_v3_fixture_state(state);
        let permuted =
            FastActorSessionV1::from_v3_fixture_state(forest_search_state(true, "Fireblast"));
        let (slice, objects) = encoded(&session);
        let (other, other_objects) = encoded(&permuted);
        let renumbered = FastActorSessionV1::from_v3_fixture_state(
            forest_search_state_with_hidden_renumbering(),
        );
        let (renumbered_slice, renumbered_objects) = encoded(&renumbered);
        assert_eq!(
            slice.binding.0.candidate_order_commitment,
            other.binding.0.candidate_order_commitment
        );
        assert_eq!(objects, other_objects);
        assert_eq!(objects, renumbered_objects);
        assert_eq!(
            slice.binding.0.candidate_order_commitment,
            renumbered_slice.binding.0.candidate_order_commitment
        );
        assert!(objects
            .iter()
            .filter(|row| row.group == FlatActionObjectGroupV1::DecisionLocalLibrary)
            .all(|row| row.zone_change_count == 0));
        let current = session.current.as_ref().unwrap();
        let mut raw = current.clone();
        raw.candidates =
            core_policy_action_candidates_v5(&current.origin_decision, &session.state).unwrap();
        assert_eq!(
            flat_build_action_cache_v2(&session, &raw, None).unwrap_err(),
            FlatActionDecisionSliceErrorV1::HiddenActionReference
        );
        let selected = current
            .candidates
            .iter()
            .position(|candidate| {
                matches!(
                    candidate.semantic,
                    ActionSemanticV1::ChooseEffectTarget { .. }
                )
            })
            .unwrap();
        let ActionSemanticV1::ChooseEffectTarget {
            target: TargetRefV1::Object { object },
            ..
        } = &current.candidates[selected].semantic
        else {
            unreachable!()
        };
        let chosen = crate::ids::ObjectId(object.arena_id);
        session
            .consume_current_flat_action_slice_v3(slice.binding, selected as u32)
            .unwrap();
        let selection_pending = session.state.engine.pending_effect.as_ref().is_some_and(|pending|
            matches!(&pending.choice, Some(crate::effect::PendingEffectChoice::SelectTargets { selected, .. })
                if selected.iter().any(|target| target.target == crate::state::Target::Object(chosen))));
        assert!(selection_pending || session.state.players[0].hand.contains(&chosen));
    }

    #[test]
    fn v3_historical_map_choice_uses_exact_frozen_source_and_v2_stays_rejected() {
        let (state, map) = map_choice_state();
        let session = FastActorSessionV1::from_v3_fixture_state(state);
        let (_, objects) = encoded(&session);
        assert!(objects
            .iter()
            .any(|row| row.group == FlatActionObjectGroupV1::HistoricalPublicSource));
        let observation = session
            .flat_policy_observation_v3(expected(&session))
            .unwrap();
        let source = observation
            .extensions
            .historical_public_sources
            .iter()
            .find(|row| row.context == HistoricalSourceContextV6::PendingEffect)
            .unwrap();
        assert_eq!(source.source.arena_id, map.0);
        assert!(session.diagnostic_current_action_semantics().unwrap().iter().all(|semantic|
            matches!(semantic, ActionSemanticV1::ChooseEffectOption { source: actual, .. } if actual == &source.source)));
        let current = session.current.as_ref().unwrap();
        let mut raw = current.clone();
        raw.candidates =
            core_policy_action_candidates_v5(&current.origin_decision, &session.state).unwrap();
        assert_eq!(
            flat_build_action_cache_v2(&session, &raw, None).unwrap_err(),
            FlatActionDecisionSliceErrorV1::InvalidActionReference
        );
    }

    #[test]
    fn v3_escape_all_intermediate_prefixes_encode_and_consume_exact_cost_targets() {
        let (state, _, picks) = escape_prefix_state();
        let mut session = FastActorSessionV1::from_v3_fixture_state(state);
        for (prefix, pick) in picks.iter().enumerate() {
            let observation = session
                .flat_policy_observation_v3(expected(&session))
                .unwrap();
            let cost = observation
                .extensions
                .pending_cast_object_cost
                .as_ref()
                .unwrap();
            assert_eq!(cost.selected.len(), prefix);
            assert_eq!(cost.remaining_count, 3 - prefix as u32);
            let (slice, _) = encoded(&session);
            let index = session.current.as_ref().unwrap().candidates.iter().position(|candidate|
                matches!(&candidate.semantic, ActionSemanticV1::ChooseCostTarget { candidate, .. } if candidate.arena_id == pick.0)).unwrap();
            session
                .consume_current_flat_action_slice_v3(slice.binding, index as u32)
                .unwrap();
        }
        // The final pick permits the engine to commit payment immediately.
        assert!(picks.iter().all(|id| session.state.exile.contains(id)));
    }

    #[test]
    fn v3_stale_binding_cache_tamper_and_candidate_reorder_are_rejected_atomically() {
        let session =
            FastActorSessionV1::from_v3_fixture_state(forest_search_state(false, "Lightning Bolt"));
        let (slice, _) = encoded(&session);
        let mut corrupt = session.clone();
        let before = corrupt.state.diagnostic_state_hash();
        let mut binding = slice.binding;
        binding.0.candidate_order_commitment[0] ^= 1;
        assert_eq!(
            corrupt
                .consume_current_flat_action_slice_v3(binding, 0)
                .unwrap_err()
                .code,
            RlSessionErrorCode::StaleEnvironmentBinding
        );
        assert_eq!(corrupt.state.diagnostic_state_hash(), before);
        assert_eq!(corrupt.policy_step_count, 0);
        corrupt.current.as_mut().unwrap().candidates.swap(0, 1);
        assert!(corrupt.validated_v3_cache(expected(&corrupt)).is_err());
        let mut corrupt = session.clone();
        corrupt
            .current
            .as_mut()
            .unwrap()
            .flat_action_cache_v2
            .as_mut()
            .unwrap()
            .objects[0]
            .card_token ^= 1;
        assert_eq!(
            corrupt.validated_v3_cache(expected(&corrupt)).unwrap_err(),
            FlatActionDecisionSliceErrorV1::CorruptCurrentBinding
        );
        let mut old = session.clone();
        old.flat_action_contract_mode = FlatActionContractModeV1::V2;
        assert_eq!(
            old.validated_v3_cache(expected(&old)).unwrap_err(),
            FlatActionDecisionSliceErrorV1::CorruptCurrentBinding
        );
        let mut stale = expected(&session);
        stale.environment_revision += 1;
        assert_eq!(
            session.validated_v3_cache(stale).unwrap_err(),
            FlatActionDecisionSliceErrorV1::StaleEnvironmentRevision
        );
    }

    #[test]
    fn v3_active_goad_is_include_only_at_every_prefix_and_v2_pair_is_unchanged() {
        for goad_first in [true, false] {
            let (state, goaded, ordinary) = goaded_attacker_fixture_state_v3(goad_first);
            let mut session = FastActorSessionV1::from_v3_fixture_state(state);
            for _ in 0..2 {
                let decision = expected(&session);
                assert_eq!(
                    decision.decision_kind,
                    FastActorDecisionKindV1::AttackerInclusion
                );
                let current = session.current.as_ref().unwrap();
                let ActionSemanticV1::ChooseAttackerInclusion { attacker, .. } =
                    &current.candidates[0].semantic
                else {
                    unreachable!()
                };
                let is_goaded = attacker.arena_id == goaded.0;
                assert_eq!(current.candidates.len(), if is_goaded { 1 } else { 2 });
                let raw =
                    core_policy_action_candidates_v5(&current.origin_decision, &session.state)
                        .unwrap();
                assert_eq!(raw.len(), 2, "frozen V5 origin remains the original pair");
                let mut legacy = current.clone();
                legacy.candidates = raw;
                assert_eq!(
                    flat_build_action_cache_v2(&session, &legacy, None)
                        .unwrap()
                        .actions
                        .len(),
                    2
                );
                if is_goaded {
                    assert!(matches!(
                        current.candidates[0].semantic,
                        ActionSemanticV1::ChooseAttackerInclusion { include: true, .. }
                    ));
                    let mut tampered = session.clone();
                    tampered.current.as_mut().unwrap().candidates = legacy.candidates;
                    assert!(tampered.validated_v3_cache(expected(&tampered)).is_err());
                }
                let (slice, _) = encoded(&session);
                // Index zero means forced include for the goaded creature,
                // and voluntary exclude for the ordinary creature.
                session
                    .consume_current_flat_action_slice_v3(slice.binding, 0)
                    .unwrap();
            }
            assert!(session.state.engine.combat.attackers.contains(&goaded));
            assert!(!session.state.engine.combat.attackers.contains(&ordinary));
        }
    }

    #[test]
    fn v3_expired_goad_keeps_optional_pair_and_ineligible_creatures_are_not_offered() {
        let (mut expired, goaded, _) = goaded_attacker_fixture_state_v3(true);
        expired.turn = expired.objects.get(goaded).v4.goaded_by[0].expires_at_turn + 1;
        let session = FastActorSessionV1::from_v3_fixture_state(expired);
        assert_eq!(expected(&session).legal_action_count, 2);
        assert!(matches!(
            session.current.as_ref().unwrap().candidates[0].semantic,
            ActionSemanticV1::ChooseAttackerInclusion { include: false, .. }
        ));
        encoded(&session);
        for tapped in [true, false] {
            let (mut state, goaded, ordinary) = goaded_attacker_fixture_state_v3(true);
            if tapped {
                state.objects.get_mut(goaded).tapped = true;
            } else {
                state.objects.get_mut(goaded).summoning_sick = true;
            }
            let session = FastActorSessionV1::from_v3_fixture_state(state);
            let decision = expected(&session);
            assert_eq!(decision.substep_count, 1);
            assert_eq!(decision.legal_action_count, 2);
            assert!(session.current.as_ref().unwrap().candidates.iter().all(|candidate|
                matches!(&candidate.semantic, ActionSemanticV1::ChooseAttackerInclusion { attacker, .. }
                    if attacker.arena_id == ordinary.0)));
            encoded(&session);
        }
    }
}
