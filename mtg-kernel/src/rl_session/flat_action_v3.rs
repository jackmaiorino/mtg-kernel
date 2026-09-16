//! V3 authority is separate from frozen V2 despite sharing row storage widths.
//! Validation rebuilds the complete rows from the original private decision.

use super::*;
use crate::policy_observation_v6::{
    HistoricalSourceContextV6, ObservationV6, PolicyObservationExtensionsV6,
};
#[cfg(test)]
use crate::state::GameState;

const COMMITMENT_DOMAIN_V3: &[u8] = b"mtg-kernel-flat-action-candidate-v3\0";

// Opt-in backend diagnostics only. Keep errors unchanged and never attach
// these records to a model observation, human response, or training record.
fn action_error_value_v3(
    session: &FastActorSessionV1,
    current: &FastActorCurrentDecisionV1,
    stage: &str,
    detail: &str,
    candidate_index: Option<usize>,
) -> serde_json::Value {
    // Raw origin order can contain chooser-only library ordering at other
    // prompts. Detailed action diagnostics are limited to ordinary priority;
    // all other prompts retain only the stage/count and stack-source audit.
    let ordinary_priority = matches!(
        &current.origin_decision,
        PolicyDecisionV5::Surface(SurfaceDecision::Decision(Decision::CastSpellOrPass { .. }))
    );
    let candidates: Vec<_> = current
        .candidates
        .iter()
        .filter(|_| ordinary_priority)
        .enumerate()
        .map(|(index, candidate)| {
            let mut visible = true;
            let inspected = flat_action_core_and_refs_v1(
                &candidate.semantic,
                current.actor.into(),
                0,
                |_, _, _, reference| {
                    visible &=
                        flat_visible_action_object_v2(&session.state, current.actor, reference)
                            .is_ok();
                    Ok(())
                },
            );
            let semantic = format!("{:?}", candidate.semantic);
            serde_json::json!({
                "index": index,
                "semantic_kind": semantic.split(" {").next().unwrap_or("unknown"),
                "semantic_debug": (visible && inspected.is_ok()).then_some(&semantic),
                "semantic_redacted": !visible || inspected.is_err(),
                "policy_action_debug": format!("{:?}", candidate.policy_action),
            })
        })
        .collect();
    let stack_sources: Vec<_> = session.state.stack.iter().enumerate().map(|(index, item)| {
        let live = session.state.objects.try_get(item.source);
        let public_live_source = live.filter(|source| {
            matches!(source.zone, Zone::Battlefield | Zone::Graveyard | Zone::Exile | Zone::Stack | Zone::Command)
                || (source.zone == Zone::Hand && source.owner == current.actor)
        }).map(|source| serde_json::json!({
            "card_def": source.card_def,
            "name": source.name,
            "owner": source.owner,
            "controller": source.controller,
            "zone": source.zone,
            "zone_change_count": source.zone_change_count,
            "attached_to": source.v4.attached_to,
        }));
        let structural_checks = item.v4.ability_source_contract.and_then(|contract| live.map(|source| {
            serde_json::json!({
                "source_id_matches": contract.source == item.source,
                "card_def_matches": contract.card_def == source.card_def,
                "owner_matches": contract.owner == source.owner,
                "controller_matches_item": contract.controller == item.controller,
                "contract_not_stack": contract.zone != Zone::Stack,
                "live_generation_not_older": source.zone_change_count >= contract.zone_change_count,
                "same_generation_zone_matches": source.zone_change_count != contract.zone_change_count || source.zone == contract.zone,
                "same_generation_attachment_matches": source.zone_change_count != contract.zone_change_count || source.v4.attached_to == contract.attached_to,
            })
        }));
        serde_json::json!({
            "stack_index": index,
            "source": item.source,
            "kind": item.kind,
            "controller": item.controller,
            "is_copy": item.is_copy,
            "is_flashback": item.is_flashback,
            "madness_offer": item.madness_offer,
            "kicked": item.kicked,
            "mode_chosen": item.mode_chosen,
            "has_inline_effect": item.inline_effect.is_some(),
            "stack_item_id": item.v4.stack_item_id,
            "cast_method": item.v4.cast_method,
            "face_index": item.v4.face_index,
            "x_value": item.v4.x_value,
            "activated_ability_index": item.v4.activated_ability_index,
            "has_hidden_ability_source": item.v4.hidden_ability_source.is_some(),
            "ability_source_contract": item.v4.ability_source_contract,
            "granted_by": item.v4.granted_by,
            "spell_source_identity": item.v4.source_contract.map(|contract| serde_json::json!({
                "source": contract.source, "card_def": contract.card_def,
                "owner": contract.owner, "controller": contract.controller,
                "zone": contract.zone, "zone_change_count": contract.zone_change_count,
            })),
            "madness_source_contract": item.v4.madness_source_contract,
            "live_source_exists": live.is_some(),
            "public_live_source": public_live_source,
            "ability_structural_checks": structural_checks,
            "validation_error": crate::engine::validated_stack_item_target_spec(item, &session.state).err(),
        })
    }).collect();
    serde_json::json!({
        "schema": "mtg-kernel-v3-action-validation-error/v1",
        "visibility": "backend-private diagnostic, never a human view",
        "stage": stage,
        "detail": detail,
        "candidate_index": candidate_index,
        "episode_id": session.episode_id,
        "policy_step": session.policy_step_count,
        "physical_decision_count": session.physical_decision_count,
        "physical_decision_id": current.physical_decision_id,
        "environment_revision": session.environment_revision,
        "current_revision": current.environment_revision,
        "actor": current.actor,
        "decision_kind": format!("{:?}", current.decision_kind),
        "substep_index": current.substep_index,
        "substep_count": current.substep_count,
        "turn": session.state.turn,
        "step": session.state.step,
        "origin_debug": ordinary_priority.then(|| format!("{:?}", current.origin_decision)),
        "action_details_redacted": !ordinary_priority,
        "candidate_count": current.candidates.len(),
        "candidates": candidates,
        "stack_sources": stack_sources,
    })
}

fn capture_action_error_v3(
    session: &FastActorSessionV1,
    current: &FastActorCurrentDecisionV1,
    stage: &str,
    detail: &dyn std::fmt::Debug,
    candidate_index: Option<usize>,
) {
    let Some(path) = std::env::var_os("MTG_KERNEL_V3_ACTION_ERROR_CAPTURE") else {
        return;
    };
    let path = std::path::PathBuf::from(path);
    let result = (|| -> Result<(), String> {
        if !path.is_absolute() {
            return Err("absolute V3 action error capture path required".into());
        }
        let value = action_error_value_v3(
            session,
            current,
            stage,
            &format!("{detail:?}"),
            candidate_index,
        );
        let bytes = serde_json::to_vec(&value).map_err(|error| error.to_string())?;
        if bytes.len() > 16 * 1024 * 1024 {
            return Err("V3 action error capture exceeds 16 MiB".into());
        }
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(|error| error.to_string())?;
        std::io::Write::write_all(&mut file, &bytes).map_err(|error| error.to_string())?;
        file.sync_all().map_err(|error| error.to_string())
    })();
    if let Err(error) = result {
        eprintln!("V3 action error capture failed: {error}");
    }
}

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
    crate::rl::policy_observation_extensions_v6(&session.state, current.actor).map_err(|error| {
        capture_action_error_v3(session, current, "observation_extensions", &error, None);
        FlatActionDecisionSliceErrorV1::InvalidDecisionRelation
    })
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
    session: &FastActorSessionV1,
    origin: &PolicyDecisionV5,
) -> Result<(), FlatActionDecisionSliceErrorV1> {
    let state = &session.state;
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
    if let PolicyDecisionV5::BlockerInclusion {
        player,
        attacker,
        blocker,
        candidate_index,
        candidate_count,
    } = origin
    {
        // H2 has already removed blockers assigned to earlier attackers. Use
        // that exact bound scan, never a fresh count of the battlefield or
        // the engine's initial per-attacker candidate list.
        let context = session
            .surface
            .scan_context_for_owned_revision_v1(state, *player, session.environment_revision)
            .map_err(|_| FlatActionDecisionSliceErrorV1::CorruptCurrentBinding)?;
        let scan = context
            .private_combat_selection
            .ok_or(FlatActionDecisionSliceErrorV1::InvalidDecisionRelation)?;
        if context.current_stage != crate::policy_surface_v5::PolicySurfaceStageV5::BlockerInclusion
            || scan.attacker != Some(*attacker)
            || scan.current_candidate != *blocker
            || scan.candidate_index != *candidate_index
            || scan.candidate_count != *candidate_count
        {
            return Err(FlatActionDecisionSliceErrorV1::InvalidDecisionRelation);
        }
        let object = state
            .objects
            .try_get(*attacker)
            .ok_or(FlatActionDecisionSliceErrorV1::InvalidActionReference)?;
        if crate::card_def::CARD_DEFS
            .get(object.card_def as usize)
            .is_none()
        {
            return Err(FlatActionDecisionSliceErrorV1::InvalidActionReference);
        }
        let minimum = crate::engine::minimum_blockers_required(state, *attacker);
        let mut feasible = Vec::with_capacity(normalized.len());
        for candidate in normalized {
            flat_validate_semantic_policy_pair_v1(&candidate)?;
            let PolicyActionV5::ChooseBlockerInclusion {
                actor,
                attacker: action_attacker,
                blocker: action_blocker,
                include,
            } = &candidate.policy_action
            else {
                return Err(FlatActionDecisionSliceErrorV1::InvalidDecisionRelation);
            };
            if actor != player || action_attacker != attacker || action_blocker != blocker {
                return Err(FlatActionDecisionSliceErrorV1::InvalidDecisionRelation);
            }
            let selected = scan.selected.len() + usize::from(*include);
            // Declaring zero blockers is legal. A nonempty block must still
            // be able to reach the minimum using the unanswered suffix. This forces
            // enough later includes after a partial commitment. The engine's
            // final aggregate validator remains the mutation-time authority.
            if selected == 0 || selected + scan.remaining_after_current.len() >= minimum {
                feasible.push(candidate);
            }
        }
        normalized = feasible;
    }
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
    normalize_candidates(
        &mut current.candidates,
        &extension,
        session,
        &current.origin_decision,
    )
    .map_err(|error| {
        capture_action_error_v3(session, current, "prepare_normalization", &error, None);
        error
    })?;
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
    let raw = core_policy_action_candidates_v5(&current.origin_decision, &session.state).map_err(
        |error| {
            capture_action_error_v3(session, current, "raw_origin_candidates", &error, None);
            FlatActionDecisionSliceErrorV1::InvalidDecisionRelation
        },
    )?;
    let mut original = current.clone();
    original.flat_action_cache = None;
    original.flat_action_cache_v2 = None;
    original.candidates = raw;
    flat_validate_current_binding_header_v1(session, &original).map_err(|error| {
        capture_action_error_v3(
            session,
            &original,
            "binding_header_and_relations",
            &error,
            None,
        );
        error
    })?;
    flat_validate_origin_decision_v1(&original, &session.state).map_err(|error| {
        capture_action_error_v3(
            session,
            &original,
            "origin_decision_relations",
            &error,
            None,
        );
        error
    })?;
    normalize_candidates(
        &mut original.candidates,
        extension,
        session,
        &original.origin_decision,
    )
    .map_err(|error| {
        capture_action_error_v3(session, &original, "rebuild_normalization", &error, None);
        error
    })?;
    if original.candidates != current.candidates {
        capture_action_error_v3(
            session,
            current,
            "normalized_candidate_equality",
            &"normalized original candidates differ from current candidates",
            None,
        );
        return Err(FlatActionDecisionSliceErrorV1::InvalidDecisionRelation);
    }

    let mut actions = Vec::with_capacity(current.candidates.len());
    let mut pending_refs = Vec::<FlatUnindexedActionRefV2>::new();
    let mut resolved = Vec::<(CardStableRefV1, FlatActionObjectV2)>::new();
    for (action_index, candidate) in current.candidates.iter().enumerate() {
        flat_validate_semantic_policy_pair_v1(candidate).map_err(|error| {
            capture_action_error_v3(
                session,
                current,
                "semantic_policy_pair",
                &error,
                Some(action_index),
            );
            error
        })?;
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
        )
        .map_err(|error| {
            capture_action_error_v3(
                session,
                current,
                "action_core_and_references",
                &error,
                Some(action_index as usize),
            );
            error
        })?;
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
        flat_validate_expected_decision_v1(self, current, expected).map_err(|error| {
            capture_action_error_v3(self, current, "expected_decision_binding", &error, None);
            error
        })?;
        if self.flat_action_contract_mode != FlatActionContractModeV1::V3 {
            return Err(FlatActionDecisionSliceErrorV1::CorruptCurrentBinding);
        }
        if let Some(error) = current.flat_action_cache_error_v2 {
            capture_action_error_v3(self, current, "stored_prepare_error", &error, None);
            return Err(error);
        }
        let cache = current
            .flat_action_cache_v2
            .as_ref()
            .ok_or(FlatActionDecisionSliceErrorV1::CorruptCurrentBinding)?;
        validate_cache(self, current, cache).map_err(|error| {
            capture_action_error_v3(self, current, "cache_validation", &error, None);
            error
        })?;
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

    /// Private human-adapter input. All three values come from the same
    /// current V3 binding; the caller must project references before transport.
    /// A fixed human seat cannot inspect an opponent's current actor view.
    pub(crate) fn human_current_decision_input_v1(
        &self,
        expected: FastActorDecisionV1,
        human_seat: PlayerSeatV1,
    ) -> Result<
        (
            ObservationV6,
            Vec<ActionSemanticV1>,
            FlatActionDecisionBindingV3,
        ),
        FlatActionDecisionSliceErrorV1,
    > {
        let cache = self.validated_v3_cache(expected)?;
        let current = self
            .current
            .as_ref()
            .ok_or(FlatActionDecisionSliceErrorV1::NoCurrentDecision)?;
        if PlayerSeatV1::from(current.actor) != human_seat {
            return Err(FlatActionDecisionSliceErrorV1::InvalidDecisionRelation);
        }
        let observation = self.flat_policy_observation_v3(expected)?;
        let actions = current
            .candidates
            .iter()
            .map(|candidate| candidate.semantic.clone())
            .collect();
        Ok((
            observation,
            actions,
            FlatActionDecisionBindingV3(cache.binding),
        ))
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

/// Builds Avenging Hunter, fires its Undercity Room Arena trigger, and
/// stops exactly at the resulting `Decision::ChooseTargets` (not yet
/// answered) with `hunter`'s `PendingTrigger::source_contract` freshly
/// captured while it is still on the battlefield. Shared by
/// [`goaded_attacker_fixture_state_v3`] (which answers the decision
/// immediately), the hidden-pending-trigger-source action-slice regression
/// tests (`rl_session/flat_action_v3.rs`'s `tests` module), and the
/// scoring-reconciliation regression test (`sideboard_play_policy_v1.rs`),
/// which all instead move `hunter` into its owner's library before
/// answering, to reproduce the exact repro condition
/// `EffectOp::ShuffleTriggerSourceIntoOwnersLibrary` (`effect.rs`) leaves
/// behind.
#[cfg(test)]
pub(crate) fn avenging_hunter_undercity_arena_choose_targets_state_v1(
    goad_first: bool,
) -> (GameState, ObjectId, ObjectId, ObjectId) {
    use crate::engine::{self, Decision};
    use crate::policy_observation_v6::tests::{put, ready_state};
    use crate::state::{AbilitySourceContractV4, InitiativeTriggerKindV1, Target, UndercityRoomV1};

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
    (state, hunter, goaded, ordinary)
}

/// Moves `hunter`'s live incarnation into `owner`'s library the way
/// `EffectOp::ShuffleTriggerSourceIntoOwnersLibrary` (`effect.rs`) does:
/// removed from the battlefield, its zone-change generation bumped, and
/// placed in the library without ever populating `library_knowledge` for
/// any observer. This is an equivalent state mutation rather than driving
/// that op directly: the real op only fires while a *different*
/// already-resolving trigger's own source sits in the graveyard (it reads
/// `ctx.ability_source_contract`, the currently-resolving stack item's own
/// contract, not an arbitrary pending trigger's), and reaching that from a
/// fixture where the Arena trigger's own `Decision::ChooseTargets` is
/// still unanswered is not reachable through ordinary engine flow:
/// `drain_pending_triggers_or_decide` (`engine.rs`) fully drains one
/// trigger's target selection in a tight loop before any stack item is
/// ever given a chance to resolve, so nothing can shuffle Hunter away in
/// between. The mutation below reproduces the exact repro condition
/// instead: a live `Zone::Library` object with no `library_knowledge`
/// entry, behind a pending trigger whose `source_contract` still names its
/// last public (`Battlefield`) incarnation.
///
/// Shared by the action-slice tests (`rl_session/flat_action_v3.rs`'s
/// `tests` module) and the scoring-reconciliation test
/// (`sideboard_play_policy_v1.rs`), which both need the identical repro
/// state.
#[cfg(test)]
pub(crate) fn shuffle_trigger_source_into_library_v1(
    state: &mut GameState,
    hunter: ObjectId,
    owner: PlayerId,
) {
    state.players[owner.index()]
        .battlefield
        .retain(|&id| id != hunter);
    {
        let object = state.objects.get_mut(hunter);
        assert_eq!(object.zone, Zone::Battlefield);
        object.zone = Zone::Library;
        object.zone_change_count += 1;
    }
    state.players[owner.index()].library.push(hunter);
    assert!(state.library_knowledge[owner.index()][owner.index()]
        .iter()
        .all(|entry| entry.object != hunter));
}

/// Moves `hunter` into `owner`'s library exactly like
/// [`shuffle_trigger_source_into_library_v1`], but also records the exact
/// live incarnation in `state.library_knowledge[owner][owner]`, as if it
/// had been revealed (scried, searched, and so on) to its own controller.
/// Returns the new `zone_change_count`. Used to prove
/// `trigger::pending_trigger_choose_targets_gate_v1` does not open just
/// because the live source sits in `Zone::Library`: a *known* library
/// source must resolve through the ordinary `KnownSelfLibrary` /
/// `KnownOpponentLibrary` path, gaining no `HistoricalPublicSource` row
/// and no registry growth.
#[cfg(test)]
pub(crate) fn move_trigger_source_to_known_library_v1(
    state: &mut GameState,
    hunter: ObjectId,
    owner: PlayerId,
) -> u32 {
    state.players[owner.index()]
        .battlefield
        .retain(|&id| id != hunter);
    let new_generation = {
        let object = state.objects.get_mut(hunter);
        assert_eq!(object.zone, Zone::Battlefield);
        object.zone = Zone::Library;
        object.zone_change_count += 1;
        object.zone_change_count
    };
    state.players[owner.index()].library.push(hunter);
    let position = u32::try_from(state.players[owner.index()].library.len() - 1).unwrap();
    state.library_knowledge[owner.index()][owner.index()].push(
        crate::state::LibraryKnowledgeEntry {
            position,
            object: hunter,
            zone_change_count: new_generation,
        },
    );
    new_generation
}

/// Moves `hunter` into `owner`'s graveyard: a public zone, easily
/// resolvable through the ordinary path, and -- like a real
/// `LeftBattlefieldToGraveyard` trigger's own frozen contract
/// (`trigger.rs`'s `uses_leave_lki` triggers) -- carrying a live
/// `zone_change_count` one more than whatever the frozen contract
/// captured. Returns the new `zone_change_count`. Used to prove the gate
/// does not open just because the live and frozen generations disagree:
/// it must also require `live.zone == Zone::Library`.
#[cfg(test)]
pub(crate) fn move_trigger_source_to_graveyard_v1(
    state: &mut GameState,
    hunter: ObjectId,
    owner: PlayerId,
) -> u32 {
    state.players[owner.index()]
        .battlefield
        .retain(|&id| id != hunter);
    let new_generation = {
        let object = state.objects.get_mut(hunter);
        assert_eq!(object.zone, Zone::Battlefield);
        object.zone = Zone::Graveyard;
        object.zone_change_count += 1;
        object.zone_change_count
    };
    state.players[owner.index()].graveyard.push(hunter);
    new_generation
}

/// Builds a real spell at stack index 0 and two real, untargeted triggered
/// abilities above it at stack indices 1 and 2 (Lembas's own ETB ability,
/// verbatim, on two different physical cards, placed via the same
/// mechanism [`avenging_hunter_undercity_arena_choose_targets_state_v1`]
/// documents: a same-controller pair would need `Decision::OrderTriggers`
/// first, so the two fillers are given different controllers to place
/// immediately), then fires Hunter's own Undercity Room Arena trigger and
/// stops at the resulting `Decision::ChooseTargets`, unanswered, with
/// `hunter` still on the battlefield.
///
/// This is the fixture `trigger::historical_public_source_ordinal_ceiling_v1`
/// exists to survive: real `HistoricalPublicSource` rows occupy raw
/// registry ordinals 1 and 2 (`Stack { stack_index: 1 }` and `Stack {
/// stack_index: 2 }`; the spell at index 0 is never a historical source),
/// exactly the range a naive "count of historical rows" ordinal (one prior
/// version of this fix used) could collide with.
#[cfg(test)]
pub(crate) fn avenging_hunter_hidden_source_with_stack_historical_rows_state_v1(
) -> (GameState, ObjectId) {
    use crate::engine::{self, Action, Decision};
    use crate::policy_observation_v6::tests::{put, ready_state};
    use crate::state::{AbilitySourceContractV4, InitiativeTriggerKindV1, StackItemKind, Target, UndercityRoomV1};
    use crate::trigger::PendingTrigger;

    let mut state = ready_state();
    let hunter = put(
        &mut state,
        PlayerId::P0,
        "Avenging Hunter",
        Zone::Battlefield,
    );
    let hunter_source = AbilitySourceContractV4::capture(&state, hunter);
    state.initiative = Some(PlayerId::P0);
    state.engine.initiative_source = Some(hunter_source);

    // Stack index 0: a real Spell. Never a historical source itself
    // (`register_extensions_v3` explicitly rejects a Spell-kind row), so it
    // exists here only to prove the two triggered abilities above it land
    // at raw stack indices 1 and 2, not 0 and 1.
    let bolt_target = put(&mut state, PlayerId::P1, "Myr Enforcer", Zone::Battlefield);
    let bolt = put(&mut state, PlayerId::P0, "Lightning Bolt", Zone::Hand);
    state.players[PlayerId::P0.index()].mana_pool[ManaColor::R.pool_index()] = 1;
    engine::step(&mut state, Action::CastSpell(bolt)).unwrap();
    engine::step(&mut state, Action::ChooseTarget(Target::Object(bolt_target))).unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::CastSpellOrPass { .. }
    ));
    assert_eq!(state.stack.len(), 1);
    assert_eq!(state.stack[0].kind, StackItemKind::Spell);

    let filler_card_def = crate::card_def::card_id_by_name("Lembas").unwrap();
    let filler_effect = (crate::trigger::triggers_for(filler_card_def)[0].effect)();
    let mut fillers = Vec::new();
    for controller in [PlayerId::P0, PlayerId::P1] {
        let filler = put(&mut state, controller, "Lembas", Zone::Battlefield);
        let filler_contract = AbilitySourceContractV4::capture(&state, filler);
        fillers.push(PendingTrigger {
            controller,
            source: filler,
            effect: filler_effect.clone(),
            is_madness_offer: false,
            kicked: false,
            target_spec: crate::card_def::TargetSpec::None,
            targets: Vec::new(),
            target_contracts: Vec::new(),
            placement_ordered: false,
            source_contract: Some(filler_contract),
            granted_by: None,
            optional_additional_cost_paid: None,
            paid_cost_refs: Vec::new(),
        });
    }
    state.engine.pending_triggers.extend(fillers);

    crate::event::log_initiative_trigger(
        &mut state,
        PlayerId::P0,
        hunter_source,
        InitiativeTriggerKindV1::UndercityRoom(UndercityRoomV1::Arena),
    )
    .unwrap();
    let triggers = crate::trigger::collect_and_process(&mut state);
    state.engine.pending_triggers.extend(triggers);

    let decision = engine::advance_until_decision(&mut state);
    assert!(
        matches!(&decision, Decision::ChooseTargets { spell, .. } if *spell == hunter),
        "got {decision:?}, halted={:?}, stack={:?}, pending_triggers={:?}",
        state.engine.halted,
        state.stack,
        state.engine.pending_triggers
    );
    assert_eq!(state.stack.len(), 3);
    assert_eq!(state.stack[0].kind, StackItemKind::Spell);
    assert_eq!(state.stack[1].kind, StackItemKind::TriggeredAbility);
    assert_eq!(state.stack[2].kind, StackItemKind::TriggeredAbility);
    (state, hunter)
}

/// Resolves a real Arena room trigger, then stages the affected player's next
/// attack declaration with an ordinary creature on either side of the goaded
/// one in engine candidate order.
#[cfg(test)]
pub(crate) fn goaded_attacker_fixture_state_v3(
    goad_first: bool,
) -> (GameState, ObjectId, ObjectId) {
    use crate::engine::{self, Action, Decision};
    use crate::state::{Step, Target};

    let (mut state, _hunter, goaded, ordinary) =
        avenging_hunter_undercity_arena_choose_targets_state_v1(goad_first);
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

    fn blocker_prefix_fixture(
        minimum: u8,
        defender: PlayerId,
        blocker_count: usize,
        earlier_attacker: bool,
    ) -> (GameState, ObjectId, Vec<ObjectId>, Option<ObjectId>) {
        use crate::policy_observation_v6::tests::{put, ready_state};
        let mut state = ready_state();
        let attacking_player = defender.opponent();
        let earlier = earlier_attacker.then(|| {
            put(
                &mut state,
                attacking_player,
                "Tolarian Terror",
                Zone::Battlefield,
            )
        });
        let attacker = put(
            &mut state,
            attacking_player,
            "Skeleton Token",
            Zone::Battlefield,
        );
        if minimum != 2 {
            state.objects.get_mut(attacker).v4.minimum_blockers_override = Some(minimum);
        }
        assert_eq!(
            crate::engine::minimum_blockers_required(&state, attacker),
            usize::from(minimum)
        );
        let blockers = (0..blocker_count)
            .map(|_| put(&mut state, defender, "Myr Enforcer", Zone::Battlefield))
            .collect();
        // Keep a real priority choice immediately after declaration so H2
        // cannot autopass an all-exclude branch through combat and next turn.
        put(&mut state, attacking_player, "Lightning Bolt", Zone::Hand);
        state.players[attacking_player.index()].mana_pool[crate::mana::ManaColor::R.pool_index()] =
            1;
        state.active_player = attacking_player;
        state.priority_player = defender;
        state.step = crate::state::Step::DeclareBlockers;
        state.engine.combat.attackers_declared = true;
        state.engine.combat.attackers = earlier.into_iter().chain([attacker]).collect();
        (state, attacker, blockers, earlier)
    }

    fn assert_original_blocker_pair_remains(session: &FastActorSessionV1) {
        let current = session.current.as_ref().unwrap();
        let mut legacy = current.clone();
        legacy.candidates =
            core_policy_action_candidates_v5(&current.origin_decision, &session.state).unwrap();
        assert_eq!(legacy.candidates.len(), 2);
        assert_eq!(
            flat_build_action_cache_v2(session, &legacy, None)
                .unwrap()
                .actions
                .len(),
            2,
            "legacy V2 retains the original pair"
        );
        if current.candidates.len() == 1 {
            let mut tampered = session.clone();
            tampered.current.as_mut().unwrap().candidates = legacy.candidates;
            assert!(tampered.validated_v3_cache(expected(&tampered)).is_err());
        }
    }

    #[test]
    fn v3_blocker_prefixes_preserve_every_engine_legal_completion_for_minima_two_and_three() {
        use std::collections::BTreeSet;
        for defender in [PlayerId::P0, PlayerId::P1] {
            for minimum in [2, 3] {
                let (state, attacker, blockers, _) =
                    blocker_prefix_fixture(minimum, defender, 4, false);
                let mut oracle = BTreeSet::new();
                for bits in 0..(1usize << blockers.len()) {
                    let selected: Vec<_> = blockers
                        .iter()
                        .copied()
                        .enumerate()
                        .filter_map(|(index, blocker)| {
                            ((bits >> index) & 1 == 1).then_some(blocker)
                        })
                        .collect();
                    let aggregate: Vec<_> = selected.iter().map(|id| (*id, attacker)).collect();
                    if crate::engine::validate_declare_blockers(&state, &aggregate).is_ok() {
                        oracle.insert(selected);
                    }
                }
                let mut pending = vec![FastActorSessionV1::from_v3_fixture_state(state)];
                let mut actual = BTreeSet::new();
                let mut forced_include = false;
                let mut forced_exclude = false;
                while let Some(session) = pending.pop() {
                    assert_eq!(
                        expected(&session).acting_player,
                        PlayerSeatV1::from(defender)
                    );
                    assert_eq!(
                        expected(&session).decision_kind,
                        FastActorDecisionKindV1::BlockerInclusion
                    );
                    assert_original_blocker_pair_remains(&session);
                    let (slice, _) = encoded(&session);
                    let current = session.current.as_ref().unwrap();
                    if current.candidates.len() == 1 {
                        match current.candidates[0].policy_action {
                            PolicyActionV5::ChooseBlockerInclusion { include: true, .. } => {
                                forced_include = true;
                            }
                            PolicyActionV5::ChooseBlockerInclusion { include: false, .. } => {
                                forced_exclude = true;
                            }
                            _ => unreachable!(),
                        }
                    }
                    for index in 0..current.candidates.len() {
                        let mut branch = session.clone();
                        branch
                            .consume_current_flat_action_slice_v3(slice.binding, index as u32)
                            .unwrap();
                        if branch.state.engine.combat.blockers_declared {
                            let selected = branch
                                .state
                                .engine
                                .combat
                                .blocked_by
                                .iter()
                                .find(|(id, _)| *id == attacker)
                                .map(|(_, selected)| selected.clone())
                                .unwrap_or_default();
                            assert!(
                                actual.insert(selected),
                                "each prefix has one execution path"
                            );
                        } else {
                            pending.push(branch);
                        }
                    }
                }
                assert!(forced_include && forced_exclude);
                assert_eq!(
                    actual, oracle,
                    "V3 must offer exactly the engine's legal assignments"
                );
            }
        }
    }

    #[test]
    fn v3_blocker_prefix_uses_suffix_after_assignments_to_an_earlier_attacker() {
        for defender in [PlayerId::P0, PlayerId::P1] {
            for minimum in [2, 3] {
                let (state, attacker, blockers, earlier) =
                    blocker_prefix_fixture(minimum, defender, usize::from(minimum), true);
                let earlier = earlier.unwrap();
                let mut session = FastActorSessionV1::from_v3_fixture_state(state);
                for (ordinal, blocker) in blockers.iter().enumerate() {
                    let current = session.current.as_ref().unwrap();
                    assert!(matches!(current.origin_decision,
                        PolicyDecisionV5::BlockerInclusion { attacker, blocker: actual, .. }
                        if attacker == earlier && actual == *blocker));
                    let index = usize::from(ordinal == 0);
                    let (slice, _) = encoded(&session);
                    session
                        .consume_current_flat_action_slice_v3(slice.binding, index as u32)
                        .unwrap();
                }
                // The engine initially saw enough blockers for the Skeleton.
                // H2 has now assigned one elsewhere, leaving fewer than its
                // minimum. All remaining choices must exclude from the start.
                for blocker in &blockers[1..] {
                    assert_original_blocker_pair_remains(&session);
                    let current = session.current.as_ref().unwrap();
                    assert_eq!(current.candidates.len(), 1);
                    assert!(matches!(current.candidates[0].policy_action,
                        PolicyActionV5::ChooseBlockerInclusion {
                            attacker: actual_attacker, blocker: actual_blocker, include: false, ..
                        } if actual_attacker == attacker && actual_blocker == *blocker));
                    let (slice, _) = encoded(&session);
                    session
                        .consume_current_flat_action_slice_v3(slice.binding, 0)
                        .unwrap();
                }
                assert!(session.state.engine.combat.blockers_declared);
                assert_eq!(
                    session.state.engine.combat.blocked_by,
                    vec![(earlier, vec![blockers[0]])]
                );
            }
        }
    }

    #[test]
    fn v3_blocker_prefix_rejects_a_scan_bound_to_another_revision_before_changing_candidates() {
        let (state, _, _, _) = blocker_prefix_fixture(2, PlayerId::P1, 3, false);
        let mut session = FastActorSessionV1::from_v3_fixture_state(state);
        let before_state = session.state.clone();
        session.environment_revision += 1;
        let mut current = session.current.take().unwrap();
        current.environment_revision = session.environment_revision;
        let before_candidates = current.candidates.clone();
        assert_eq!(
            prepare_and_build_v3(&session, &mut current).unwrap_err(),
            FlatActionDecisionSliceErrorV1::CorruptCurrentBinding
        );
        assert_eq!(current.candidates, before_candidates);
        assert_eq!(session.state, before_state);
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
    fn v3_pending_trigger_hidden_source_resolves_by_frozen_contract() {
        let (mut state, hunter, goaded, _ordinary) =
            avenging_hunter_undercity_arena_choose_targets_state_v1(false);
        let contract = state.engine.pending_triggers[0].source_contract.unwrap();
        assert_eq!(contract.source, hunter);
        assert_eq!(contract.zone, Zone::Battlefield);
        shuffle_trigger_source_into_library_v1(&mut state, hunter, PlayerId::P0);

        let session = FastActorSessionV1::from_v3_fixture_state(state);
        let current = session.current.as_ref().unwrap();
        assert!(matches!(
            current.origin_decision,
            PolicyDecisionV5::Surface(SurfaceDecision::Decision(Decision::ChooseTargets {
                spell,
                ..
            })) if spell == hunter
        ));
        let saw_choose_target_or_finish = current.candidates.iter().any(|candidate| {
            matches!(
                candidate.semantic,
                ActionSemanticV1::ChooseTarget { .. } | ActionSemanticV1::FinishTargetSelection { .. }
            )
        });
        assert!(saw_choose_target_or_finish);

        let (_, objects) = encoded(&session);
        let pending_rows: Vec<_> = objects
            .iter()
            .filter(|row| row.group == FlatActionObjectGroupV1::HistoricalPublicSource)
            .collect();
        assert!(
            !pending_rows.is_empty(),
            "the hidden Hunter reference must resolve through the frozen-contract fallback"
        );
        for row in &pending_rows {
            assert_eq!(row.zone, flat_zone_v1(Zone::Battlefield));
            assert_eq!(row.zone_change_count, contract.zone_change_count);
            assert_eq!(row.card_token, flat_card_token_v2(contract.card_def));
            // The stack is empty in this fixture, so the collision-safe
            // ceiling (`trigger::historical_public_source_ordinal_ceiling_v1`
            // = stack length + 1) plus this trigger's own position (0) is
            // exactly 1.
            assert_eq!(row.actor_visible_ordinal, 1);
        }
        // The chosen target (`goaded`) still resolves normally -- only
        // Hunter's own hidden reference needed the frozen-contract
        // fallback.
        assert!(objects
            .iter()
            .any(|row| row.group == FlatActionObjectGroupV1::OpponentBattlefield));
        let _ = goaded;

        // This only confirms the raw V6 observation itself still omits the
        // hidden source (`rl.rs`'s `policy_observation_extensions_v6` is
        // unmodified, so `historical_public_sources` never gains an entry
        // for it) -- it does not exercise `flat_policy_v2.rs`'s scoring
        // reconciliation at all. That proof is
        // `sideboard_play_policy_v1.rs`'s
        // `score_fast_session_v1_reconciles_a_pending_trigger_hidden_source`,
        // which actually drives `score_fast_session_v1` on this same kind
        // of fixture.
        let observation = session
            .flat_policy_observation_v3(expected(&session))
            .unwrap();
        assert!(observation
            .extensions
            .historical_public_sources
            .is_empty());
    }

    #[test]
    fn v3_pending_trigger_hidden_source_resolves_alongside_a_coexisting_stack_item() {
        use crate::policy_observation_v6::tests::put;
        use crate::trigger::PendingTrigger;

        let (mut state, hunter, _goaded, _ordinary) =
            avenging_hunter_undercity_arena_choose_targets_state_v1(false);
        // Insert a real, independently-valid non-spell stack item ahead of
        // Hunter's own still-pending trigger, exactly the way the engine
        // would if another untargeted triggered ability had already been
        // placed on the stack earlier in the same trigger-draining pass:
        // `drain_pending_triggers_or_decide` (`engine.rs`) places any
        // trigger needing no targets immediately, so queuing this filler
        // ahead of Hunter's own trigger and re-draining lands it on the
        // stack, unresolved, before Hunter's `Decision::ChooseTargets`
        // surfaces. Lembas's real, untargeted ETB ability
        // (`trigger::triggers_for`'s first entry) is used verbatim so
        // `engine::validate_pending_trigger`'s definition-owned check
        // (`triggered_stack_item_expected_target_spec`) accepts it as a
        // real card-defined trigger, not a synthetic placeholder.
        let filler = put(&mut state, PlayerId::P1, "Lembas", Zone::Battlefield);
        let filler_card_def = state.objects.get(filler).card_def;
        let filler_effect = (crate::trigger::triggers_for(filler_card_def)[0].effect)();
        let filler_contract = crate::state::AbilitySourceContractV4::capture(&state, filler);
        state.engine.pending_triggers.insert(
            0,
            PendingTrigger {
                controller: PlayerId::P1,
                source: filler,
                effect: filler_effect,
                is_madness_offer: false,
                kicked: false,
                target_spec: crate::card_def::TargetSpec::None,
                targets: Vec::new(),
                target_contracts: Vec::new(),
                placement_ordered: false,
                source_contract: Some(filler_contract),
                granted_by: None,
                optional_additional_cost_paid: None,
                paid_cost_refs: Vec::new(),
            },
        );
        let decision = crate::engine::advance_until_decision(&mut state);
        assert!(
            matches!(&decision, Decision::ChooseTargets { spell, .. } if *spell == hunter),
            "got {decision:?}, halted={:?}, stack={:?}",
            state.engine.halted,
            state.stack
        );
        assert_eq!(state.stack.len(), 1);
        assert_eq!(state.stack[0].source, filler);
        assert_eq!(state.stack[0].kind, crate::state::StackItemKind::TriggeredAbility);

        let contract = state.engine.pending_triggers[0].source_contract.unwrap();
        assert_eq!(contract.source, hunter);
        shuffle_trigger_source_into_library_v1(&mut state, hunter, PlayerId::P0);
        assert_eq!(state.stack.len(), 1, "the filler stack item is untouched by the shuffle");

        let session = FastActorSessionV1::from_v3_fixture_state(state);
        let (_, objects) = encoded(&session);
        let pending_rows: Vec<_> = objects
            .iter()
            .filter(|row| row.group == FlatActionObjectGroupV1::HistoricalPublicSource)
            .collect();
        assert!(
            !pending_rows.is_empty(),
            "the frozen-contract fallback must still resolve Hunter's hidden \
             reference with an unrelated non-spell item on the stack"
        );
        for row in &pending_rows {
            assert_eq!(row.zone, flat_zone_v1(Zone::Battlefield));
            assert_eq!(row.zone_change_count, contract.zone_change_count);
            // The stack holds exactly one item (the Lembas filler), so the
            // collision-safe ceiling (stack length 1 + 1) plus this
            // trigger's own position (0) is exactly 2 -- strictly past the
            // filler's own real historical ordinal (1), never colliding
            // with it regardless of where on the stack it sits.
            assert_eq!(row.actor_visible_ordinal, 2);
        }
    }

    /// Layer A's own half of the two gate-precision regressions: the
    /// action slice must keep using the ordinary group for a pending
    /// trigger's source that is not actually hidden. This alone cannot
    /// observe layer B's registry (`flat_policy_v2.rs`'s `self.objects`,
    /// grown only by `build_scoring_owned_v3`), so it is not this test's
    /// job to prove "no ghost row" -- `sideboard_play_policy_v1.rs`'s
    /// `score_fast_session_v1_registry_object_count_is_unchanged_for_a_known_or_public_source`
    /// does that directly against the real registry.
    #[test]
    fn v3_pending_trigger_known_library_source_takes_the_ordinary_path() {
        let (mut state, hunter, goaded, _ordinary) =
            avenging_hunter_undercity_arena_choose_targets_state_v1(false);
        let new_generation = move_trigger_source_to_known_library_v1(&mut state, hunter, PlayerId::P0);

        let session = FastActorSessionV1::from_v3_fixture_state(state);
        let (_, objects) = encoded(&session);
        assert!(objects
            .iter()
            .all(|row| row.group != FlatActionObjectGroupV1::HistoricalPublicSource));
        let hunter_card_token =
            flat_card_token_v2(crate::card_def::card_id_by_name("Avenging Hunter").unwrap());
        let hunter_rows: Vec<_> = objects
            .iter()
            .filter(|row| row.card_token == hunter_card_token)
            .collect();
        assert!(!hunter_rows.is_empty());
        for row in &hunter_rows {
            assert_eq!(row.group, FlatActionObjectGroupV1::KnownSelfLibrary);
            assert_eq!(row.zone, flat_zone_v1(Zone::Library));
            assert_eq!(row.zone_change_count, new_generation);
        }
        let _ = goaded;
    }

    /// See the doc comment on
    /// [`v3_pending_trigger_known_library_source_takes_the_ordinary_path`].
    #[test]
    fn v3_pending_trigger_graveyard_source_takes_the_ordinary_path() {
        let (mut state, hunter, goaded, _ordinary) =
            avenging_hunter_undercity_arena_choose_targets_state_v1(false);
        let new_generation = move_trigger_source_to_graveyard_v1(&mut state, hunter, PlayerId::P0);

        let session = FastActorSessionV1::from_v3_fixture_state(state);
        let (_, objects) = encoded(&session);
        assert!(objects
            .iter()
            .all(|row| row.group != FlatActionObjectGroupV1::HistoricalPublicSource));
        let hunter_card_token =
            flat_card_token_v2(crate::card_def::card_id_by_name("Avenging Hunter").unwrap());
        let hunter_rows: Vec<_> = objects
            .iter()
            .filter(|row| row.card_token == hunter_card_token)
            .collect();
        assert!(!hunter_rows.is_empty());
        for row in &hunter_rows {
            // Exactly the components the unmodified `Zone::Graveyard` arm
            // always produced -- this fix never touches it: the object's
            // own live identity, never the frozen contract's.
            assert_eq!(row.group, FlatActionObjectGroupV1::SelfGraveyard);
            assert_eq!(row.zone, flat_zone_v1(Zone::Graveyard));
            assert_eq!(row.zone_change_count, new_generation);
        }
        let _ = goaded;
    }

    #[test]
    fn v3_pending_trigger_hidden_source_ordinal_does_not_collide_with_real_stack_historical_rows() {
        let (mut state, hunter) =
            avenging_hunter_hidden_source_with_stack_historical_rows_state_v1();
        let contract = state.engine.pending_triggers[0].source_contract.unwrap();
        assert_eq!(contract.source, hunter);
        shuffle_trigger_source_into_library_v1(&mut state, hunter, PlayerId::P0);

        let session = FastActorSessionV1::from_v3_fixture_state(state);
        let (_, objects) = encoded(&session);
        let pending_rows: Vec<_> = objects
            .iter()
            .filter(|row| row.group == FlatActionObjectGroupV1::HistoricalPublicSource)
            .collect();
        assert!(
            !pending_rows.is_empty(),
            "the frozen-contract fallback must resolve Hunter's hidden \
             reference even with real Stack-context historical rows present"
        );
        for row in &pending_rows {
            assert_eq!(row.zone, flat_zone_v1(Zone::Battlefield));
            assert_eq!(row.zone_change_count, contract.zone_change_count);
            // Stack length 3 (spell + two triggered-ability fillers) + 1 +
            // trigger position 0 = 4, strictly past the two real
            // Stack-context historical rows' own raw ordinals (1 and 2).
            assert_eq!(row.actor_visible_ordinal, 4);
        }
    }

    #[test]
    fn v3_pending_trigger_unshuffled_source_encoding_is_unchanged() {
        let (state, hunter, goaded, _ordinary) =
            avenging_hunter_undercity_arena_choose_targets_state_v1(false);
        let session = FastActorSessionV1::from_v3_fixture_state(state);
        let (_, objects) = encoded(&session);
        assert!(objects
            .iter()
            .all(|row| row.group != FlatActionObjectGroupV1::HistoricalPublicSource));
        let hunter_rows: Vec<_> = objects
            .iter()
            .filter(|row| row.card_token == flat_card_token_v2(crate::card_def::card_id_by_name("Avenging Hunter").unwrap()))
            .collect();
        assert!(!hunter_rows.is_empty());
        for row in &hunter_rows {
            assert_eq!(row.group, FlatActionObjectGroupV1::SelfBattlefield);
            assert_eq!(row.zone, flat_zone_v1(Zone::Battlefield));
            assert_eq!(row.zone_change_count, 0);
        }
        let _ = hunter;
        assert!(objects
            .iter()
            .any(|row| row.group == FlatActionObjectGroupV1::OpponentBattlefield));
        let _ = goaded;
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
