//! Fresh-lineage (V4 contract) action-slice encoding, additive sibling of
//! `flat_action_v3.rs`. The V3/frozen path (`flat_visible_action_object_v2`,
//! `flat_visible_action_object_components_v1`,
//! `pending_trigger_frozen_source_components_v1`,
//! `flat_build_action_cache_v2`, `encode_current_flat_action_slice_v3`) is
//! never called from here and is left byte-identical.
//!
//! This module generalizes the single-position (`Decision::ChooseTargets`
//! on `pending_triggers[0]` only) frozen-source fallback the V3 path already
//! has to every position in `state.engine.pending_triggers`, so
//! `Decision::OrderTriggers` (and `ChooseTargets` for any position) can name
//! a hidden same-controller pending trigger's source by its frozen
//! `PendingTrigger::source_contract` instead of failing with
//! `HiddenActionReference`.
//!
//! This is a self-contained, cache-free re-derivation of
//! `flat_build_action_cache_v2` + `encode_current_flat_action_slice_v3`'s
//! combined logic (it does not read or write `current.flat_action_cache_v2`,
//! recomputing fresh on every call), so no shared session cache field needs
//! a V4 variant added to it.
//!
//! Position-based, not arena-id-based (adversarial-review fix): two
//! simultaneously hidden pending triggers can share one live source object
//! (one permanent with two abilities both triggering off the same event,
//! then shuffled into the library together), so resolving a hidden
//! reference by scanning `state.engine.pending_triggers` for the first
//! entry matching a live `arena_id` cannot disambiguate them -- both
//! positions would resolve to the same (first) match. This module instead
//! resolves each `PendingSources` reference by its own POSITION (the
//! `order_index` `flat_action_core_and_refs_v1` already threads through
//! `emit_ref`, equal to 0 for every non-`OrderTriggers` role and to the
//! pending-source index for `OrderTriggers`, matching
//! `Decision::OrderTriggers { pending }`'s own prefix-of-`state.engine.
//! pending_triggers` invariant), never by arena-id search, so two hidden
//! positions sharing one physical card never collapse into one. This
//! composes with `flat_policy_v2.rs`'s `register_extensions_v4`, which (per
//! its own doc comment) always inserts one registry row per hidden
//! position for the same reason.
//!
//! Item 14 (plan): [`frozen_pending_trigger_semantic_v4`] is the V4
//! producer that substitutes a hidden position's frozen `source_contract`
//! fields for the raw live `card_ref`, applied to a cloned candidate
//! semantic inside this module -- `rl.rs`'s `core_surface_action_candidates_v1`
//! (the `Decision::OrderTriggers` arm at ~2988-2993, and its `ChooseTarget`/
//! `FinishTargetSelection` arms) is never touched and keeps producing the
//! raw, unconditional `card_ref` this function reads as input.

use super::*;
use crate::ids::{ObjectId, PlayerId};
use crate::state::Zone;

/// Local (V4-only) analog of `FlatResolvedActionObjectV2`: adds `position`
/// to the dedup/consistency key so two hidden pending-trigger positions
/// sharing one physical source object (identical `arena_id`) are tracked
/// as distinct resolved entries instead of being collapsed into one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FlatResolvedActionObjectV4 {
    arena_id: u32,
    position: u32,
    object: FlatActionObjectV2,
}

/// V4 producer (plan item 14): rewrites a candidate's `ActionSemanticV1` so
/// a hidden same-controller pending trigger's source is named by its
/// frozen `PendingTrigger::source_contract` identity instead of the raw
/// live `card_ref` -- for every position in `Decision::OrderTriggers`'s
/// `pending_sources`, and for the single (always-0) position
/// `Decision::ChooseTargets` for a pending trigger can ever name
/// (`ChooseTarget`/`FinishTargetSelection`'s own `source` field, per
/// `trigger::pending_trigger_choose_targets_gate_v1`'s proven "only
/// `pending_triggers[0]`" invariant). A position is substituted only when
/// [`crate::trigger::pending_trigger_hidden_source_v1`] confirms it is
/// genuinely hidden and the live `arena_id` still matches that position's
/// own pending trigger; otherwise the reference is left as the ordinary
/// live `card_ref`, resolving through the normal (non-frozen) path exactly
/// as it would for V3. Every other `ActionSemanticV1` variant passes
/// through unchanged: none of them can ever name a pending trigger's own
/// source (they arise from engine stages -- casting, cost payment, effect
/// resolution -- mutually exclusive with `EngineDecisionStageV2::PendingTriggers`).
fn frozen_pending_trigger_semantic_v4(
    state: &crate::state::GameState,
    semantic: ActionSemanticV1,
) -> ActionSemanticV1 {
    let frozen_at = |position: usize, live: &CardStableRefV1, actor: PlayerSeatV1| -> CardStableRefV1 {
        let Some(pending) = state.engine.pending_triggers.get(position) else {
            return live.clone();
        };
        if PlayerSeatV1::from(pending.controller) != actor || pending.source.0 != live.arena_id {
            return live.clone();
        }
        let Some(contract) = pending.source_contract else {
            return live.clone();
        };
        if !crate::trigger::pending_trigger_hidden_source_v1(state, pending) {
            return live.clone();
        }
        CardStableRefV1 {
            arena_id: pending.source.0,
            card_db_id: contract.card_def,
            owner: contract.owner.into(),
            controller: contract.controller.into(),
            zone: contract.zone,
            zone_change_count: contract.zone_change_count,
        }
    };
    match semantic {
        ActionSemanticV1::OrderTriggers {
            actor,
            mut pending_sources,
            order,
        } => {
            for (position, source) in pending_sources.iter_mut().enumerate() {
                *source = frozen_at(position, source, actor);
            }
            ActionSemanticV1::OrderTriggers {
                actor,
                pending_sources,
                order,
            }
        }
        ActionSemanticV1::ChooseTarget {
            actor,
            source,
            remaining,
            target,
        } => {
            let source = frozen_at(0, &source, actor);
            ActionSemanticV1::ChooseTarget {
                actor,
                source,
                remaining,
                target,
            }
        }
        ActionSemanticV1::FinishTargetSelection {
            actor,
            source,
            selected_count,
        } => {
            let source = frozen_at(0, &source, actor);
            ActionSemanticV1::FinishTargetSelection {
                actor,
                source,
                selected_count,
            }
        }
        other => other,
    }
}

/// `Zone::Library` fallback for a V4 action reference at a known
/// `position` (0 for `ChooseTargets`, the pending-source index for
/// `OrderTriggers`). Position-based: looks up `state.engine.pending_triggers[position]`
/// directly (no arena-id scan), so two hidden positions sharing one
/// physical source object each resolve independently. Verifies `reference`
/// actually carries that position's exact frozen contract fields (which
/// [`frozen_pending_trigger_semantic_v4`] should already have substituted)
/// before treating it as the frozen fallback; a mismatch returns `None`
/// (falls through to the ordinary, non-frozen resolution below, which then
/// fails loudly with `HiddenActionReference` for a still-hidden position --
/// never a silent guess).
fn pending_trigger_frozen_source_components_v4(
    state: &crate::state::GameState,
    actor: PlayerId,
    position: u32,
    reference: &CardStableRefV1,
) -> Result<Option<FlatVisibleActionObjectComponentsV1>, FlatActionDecisionSliceErrorV1> {
    let index = usize::try_from(position)
        .map_err(|_| FlatActionDecisionSliceErrorV1::CheckedIntegerRange)?;
    let Some(pending) = state.engine.pending_triggers.get(index) else {
        return Ok(None);
    };
    if pending.controller != actor || pending.source.0 != reference.arena_id {
        return Ok(None);
    }
    let Some(contract) = pending.source_contract else {
        return Ok(None);
    };
    if !crate::trigger::pending_trigger_hidden_source_v1(state, pending) {
        return Ok(None);
    }
    if reference.card_db_id != contract.card_def
        || reference.owner != contract.owner.into()
        || reference.controller != contract.controller.into()
        || reference.zone != contract.zone
        || reference.zone_change_count != contract.zone_change_count
    {
        return Ok(None);
    }
    let ordinal = crate::trigger::historical_public_source_ordinal_ceiling_v1(state)
        .and_then(|ceiling| ceiling.checked_add(position))
        .ok_or(FlatActionDecisionSliceErrorV1::CheckedIntegerRange)?;
    Ok(Some(FlatVisibleActionObjectComponentsV1 {
        card_db_id: contract.card_def,
        group: FlatActionObjectGroupV1::HistoricalPublicSource,
        actor_visible_ordinal: usize::try_from(ordinal)
            .map_err(|_| FlatActionDecisionSliceErrorV1::CheckedIntegerRange)?,
        owner_relative: flat_relative_seat_v1(contract.owner.into(), actor.into())?,
        controller_relative: flat_relative_seat_v1(contract.controller.into(), actor.into())?,
        zone: flat_zone_v1(contract.zone),
        zone_change_count: contract.zone_change_count,
    }))
}

/// Fork of `flat_visible_action_object_components_v1`: every non-`Library`
/// arm is byte-identical; the frozen/hidden fallback
/// ([`pending_trigger_frozen_source_components_v4`]) is now checked FIRST
/// (a frozen reference's `zone`/`zone_change_count` intentionally do not
/// match the live object's current, post-shuffle state -- that mismatch is
/// exactly how a frozen reference is recognized, not an ordinary
/// consistency error), with `position` threaded through from the caller
/// instead of derived by scanning for a matching `arena_id`.
fn flat_visible_action_object_components_v4(
    state: &crate::state::GameState,
    actor: PlayerId,
    position: u32,
    reference: &CardStableRefV1,
) -> Result<FlatVisibleActionObjectComponentsV1, FlatActionDecisionSliceErrorV1> {
    if let Some(components) =
        pending_trigger_frozen_source_components_v4(state, actor, position, reference)?
    {
        return Ok(components);
    }
    let object_id = ObjectId(reference.arena_id);
    let object = state
        .objects
        .try_get(object_id)
        .ok_or(FlatActionDecisionSliceErrorV1::InvalidActionReference)?;
    let owner: PlayerSeatV1 = object.owner.into();
    let controller: PlayerSeatV1 = object.controller.into();
    if object.card_def != reference.card_db_id
        || owner != reference.owner
        || controller != reference.controller
        || object.zone != reference.zone
        || object.zone_change_count != reference.zone_change_count
    {
        return Err(FlatActionDecisionSliceErrorV1::InvalidActionReference);
    }
    let zone_position = |objects: &[ObjectId]| objects.iter().position(|&id| id == object_id);
    let (group, ordinal) = match object.zone {
        Zone::Hand if object.owner == actor => (
            FlatActionObjectGroupV1::SelfHand,
            zone_position(&state.players[actor.index()].hand)
                .ok_or(FlatActionDecisionSliceErrorV1::InvalidActionReference)?,
        ),
        Zone::Hand => {
            if zone_position(&state.players[object.owner.index()].hand).is_none() {
                return Err(FlatActionDecisionSliceErrorV1::InvalidActionReference);
            }
            let ordinal = known_opponent_hand_canonical_ordinal_v1(
                state,
                actor,
                object.owner,
                object_id,
                object.zone_change_count,
            )
            .ok_or(FlatActionDecisionSliceErrorV1::HiddenActionReference)?;
            (FlatActionObjectGroupV1::KnownOpponentHand, ordinal)
        }
        Zone::Battlefield => {
            let ordinal = zone_position(&state.players[object.controller.index()].battlefield)
                .ok_or(FlatActionDecisionSliceErrorV1::InvalidActionReference)?;
            (
                if object.controller == actor {
                    FlatActionObjectGroupV1::SelfBattlefield
                } else {
                    FlatActionObjectGroupV1::OpponentBattlefield
                },
                ordinal,
            )
        }
        Zone::Graveyard => {
            let ordinal = zone_position(&state.players[object.owner.index()].graveyard)
                .ok_or(FlatActionDecisionSliceErrorV1::InvalidActionReference)?;
            (
                if object.owner == actor {
                    FlatActionObjectGroupV1::SelfGraveyard
                } else {
                    FlatActionObjectGroupV1::OpponentGraveyard
                },
                ordinal,
            )
        }
        Zone::Exile => (
            FlatActionObjectGroupV1::Exile,
            zone_position(&state.exile)
                .ok_or(FlatActionDecisionSliceErrorV1::InvalidActionReference)?,
        ),
        Zone::Stack => (
            FlatActionObjectGroupV1::Stack,
            flat_stack_action_object_ordinal_v1(state, object_id, object.controller)?,
        ),
        Zone::Command => (
            FlatActionObjectGroupV1::Command,
            zone_position(&state.command)
                .ok_or(FlatActionDecisionSliceErrorV1::InvalidActionReference)?,
        ),
        Zone::Library => {
            let knowledge = state.library_knowledge[actor.index()][object.owner.index()]
                .iter()
                .find(|entry| {
                    entry.object == object_id && entry.zone_change_count == object.zone_change_count
                });
            let Some(knowledge) = knowledge else {
                return Err(FlatActionDecisionSliceErrorV1::HiddenActionReference);
            };
            let library_position = usize::try_from(knowledge.position)
                .map_err(|_| FlatActionDecisionSliceErrorV1::CheckedIntegerRange)?;
            if state.players[object.owner.index()]
                .library
                .get(library_position)
                != Some(&object_id)
            {
                return Err(FlatActionDecisionSliceErrorV1::InvalidActionReference);
            }
            (
                if object.owner == actor {
                    FlatActionObjectGroupV1::KnownSelfLibrary
                } else {
                    FlatActionObjectGroupV1::KnownOpponentLibrary
                },
                library_position,
            )
        }
    };
    Ok(FlatVisibleActionObjectComponentsV1 {
        card_db_id: object.card_def,
        group,
        actor_visible_ordinal: ordinal,
        owner_relative: flat_relative_seat_v1(owner, actor.into())?,
        controller_relative: flat_relative_seat_v1(controller, actor.into())?,
        zone: flat_zone_v1(object.zone),
        zone_change_count: object.zone_change_count,
    })
}

fn flat_visible_action_object_v4(
    state: &crate::state::GameState,
    actor: PlayerId,
    position: u32,
    reference: &CardStableRefV1,
) -> Result<FlatActionObjectV2, FlatActionDecisionSliceErrorV1> {
    let fields = flat_visible_action_object_components_v4(state, actor, position, reference)?;
    Ok(FlatActionObjectV2 {
        card_token: flat_card_token_v2(fields.card_db_id),
        group: fields.group,
        actor_visible_ordinal: u16::try_from(fields.actor_visible_ordinal)
            .map_err(|_| FlatActionDecisionSliceErrorV1::CheckedIntegerRange)?,
        owner_relative: fields.owner_relative,
        controller_relative: fields.controller_relative,
        zone: fields.zone,
        zone_change_count: fields.zone_change_count,
    })
}

impl FastActorSessionV1 {
    /// V4 sibling of `encode_current_flat_action_slice_v3`. Cache-free: it
    /// re-derives `(actions, refs, objects)` fresh from `current.candidates`
    /// every call, using [`flat_visible_action_object_v4`] in place of
    /// `flat_visible_action_object_v2`, instead of consulting or installing
    /// `current.flat_action_cache_v2` (a V3-mode-gated field this function
    /// never touches). Every other validation
    /// (`flat_validate_expected_decision_v1`,
    /// `flat_validate_current_binding_header_v1`,
    /// `flat_validate_origin_decision_v1`,
    /// `flat_validate_semantic_policy_pair_v1`) is reused unmodified.
    pub(crate) fn encode_current_flat_action_slice_v4(
        &self,
        expected: FastActorDecisionV1,
        buffers: &mut FlatActionDecisionSliceBuffersV2<'_>,
    ) -> Result<FlatActionDecisionSliceV3, FlatActionDecisionSliceErrorV1> {
        let current = self
            .current
            .as_ref()
            .ok_or(FlatActionDecisionSliceErrorV1::NoCurrentDecision)?;
        flat_validate_current_binding_header_v1(self, current)?;
        flat_validate_expected_decision_v1(self, current, expected)?;
        flat_validate_origin_decision_v1(current, &self.state)?;
        let actor: PlayerSeatV1 = current.actor.into();
        let mut actions_out: Vec<FlatActionCoreV1> = Vec::with_capacity(current.candidates.len());
        let mut unindexed_refs: Vec<FlatUnindexedActionRefV2> = Vec::new();
        // Keyed by (arena_id, position) rather than V3's arena_id alone:
        // two hidden pending-trigger positions can share one physical
        // source object, and must be tracked as distinct resolved entries
        // (see the module doc comment's collision-freedom note), while
        // still catching a genuine same-(arena_id, position) inconsistency
        // exactly as the arena_id-only check did for the ordinary case
        // (where `position` is always 0 for every non-`PendingSources`
        // role, so this is a strict generalization, not a relaxation).
        let mut resolved_objects: Vec<FlatResolvedActionObjectV4> = Vec::new();
        for (action_index, candidate) in current.candidates.iter().enumerate() {
            flat_validate_semantic_policy_pair_v1(candidate)?;
            let action_index_u32 = u32::try_from(action_index)
                .map_err(|_| FlatActionDecisionSliceErrorV1::CheckedIntegerRange)?;
            let ref_start = u32::try_from(unindexed_refs.len())
                .map_err(|_| FlatActionDecisionSliceErrorV1::CheckedIntegerRange)?;
            let semantic = frozen_pending_trigger_semantic_v4(&self.state, candidate.semantic.clone());
            let core = flat_action_core_and_refs_v1(
                &semantic,
                actor,
                ref_start,
                |role, order_index, associated_order, reference| {
                    let position = u32::from(order_index);
                    let object =
                        flat_visible_action_object_v4(&self.state, current.actor, position, reference)?;
                    if let Some(previous) = resolved_objects
                        .iter()
                        .find(|candidate| candidate.arena_id == reference.arena_id && candidate.position == position)
                    {
                        if previous.object != object {
                            return Err(FlatActionDecisionSliceErrorV1::InvalidActionReference);
                        }
                    } else {
                        if resolved_objects.iter().any(|candidate| {
                            candidate.object.canonical_key() == object.canonical_key()
                        }) {
                            return Err(FlatActionDecisionSliceErrorV1::DuplicateCanonicalObject);
                        }
                        resolved_objects.push(FlatResolvedActionObjectV4 {
                            arena_id: reference.arena_id,
                            position,
                            object,
                        });
                    }
                    unindexed_refs.push(FlatUnindexedActionRefV2 {
                        action_index: action_index_u32,
                        role,
                        order_index,
                        associated_order,
                        object,
                    });
                    Ok(())
                },
            )?;
            actions_out.push(core);
        }
        resolved_objects.sort_unstable_by_key(|candidate| candidate.object.canonical_key());
        if resolved_objects
            .windows(2)
            .any(|pair| pair[0].object.canonical_key() == pair[1].object.canonical_key())
        {
            return Err(FlatActionDecisionSliceErrorV1::DuplicateCanonicalObject);
        }
        let objects_out: Vec<FlatActionObjectV2> = resolved_objects
            .iter()
            .map(|candidate| candidate.object)
            .collect();
        let mut refs_out: Vec<FlatActionRefV2> = Vec::with_capacity(unindexed_refs.len());
        for reference in unindexed_refs {
            let key = reference.object.canonical_key();
            let object_index = objects_out
                .binary_search_by_key(&key, |candidate| candidate.canonical_key())
                .map_err(|_| FlatActionDecisionSliceErrorV1::InvalidActionReference)?;
            refs_out.push(FlatActionRefV2 {
                action_index: reference.action_index,
                role: reference.role,
                order_index: reference.order_index,
                associated_order: reference.associated_order,
                card_token: reference.object.card_token,
                object_index: u16::try_from(object_index)
                    .map_err(|_| FlatActionDecisionSliceErrorV1::CheckedIntegerRange)?,
            });
        }
        let action_count_u32 = u32::try_from(actions_out.len())
            .map_err(|_| FlatActionDecisionSliceErrorV1::CheckedIntegerRange)?;
        let commitment =
            flat_action_commitment_from_rows_v2(actor, &actions_out, &refs_out, &objects_out)?;
        let binding = flat_action_binding_v2(self, current, action_count_u32, commitment);

        let (a, r, o) = (actions_out.len(), refs_out.len(), objects_out.len());
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
        buffers.actions[..a].copy_from_slice(&actions_out);
        buffers.refs[..r].copy_from_slice(&refs_out);
        buffers.objects[..o].copy_from_slice(&objects_out);
        Ok(FlatActionDecisionSliceV3 {
            binding: FlatActionDecisionBindingV3(binding),
            active_action_count: a as u32,
            active_ref_count: r as u32,
            active_object_count: o as u16,
        })
    }

    /// V4 sibling of `flat_policy_observation_v3`. Deliberately does NOT
    /// call `validated_v3_cache`: that helper's job is to validate and
    /// return the *eagerly built* V3 action-slice cache
    /// (`current.flat_action_cache_v2`), which the session machinery
    /// builds for every V3-mode session regardless of which generation
    /// later consumes it -- and that eager build fails with
    /// `HiddenActionReference` for exactly the hidden-`OrderTriggers`
    /// states this generation exists to handle, since it is produced by
    /// the V3-only `flat_visible_action_object_components_v1`/
    /// `flat_build_action_cache_v2` path. Calling `flat_policy_observation_v3`
    /// from `build_scoring_owned_v4` would therefore fail before ever
    /// reaching this module's own resolver. This function instead performs
    /// only the two validations that do not depend on that cache
    /// (`flat_validate_current_binding_header_v1`,
    /// `flat_validate_expected_decision_v1`) and then builds the
    /// observation the same way `flat_policy_observation_v3` does
    /// (`rl::observe_policy_v6_unhashed_for_flat_policy`, unmodified) --
    /// the observation itself does not depend on the V3 action cache at
    /// all, only `flat_policy_observation_v3`'s own validation gate did.
    pub(crate) fn flat_policy_observation_v4(
        &self,
        expected: FastActorDecisionV1,
    ) -> Result<crate::policy_observation_v6::ObservationV6, FlatActionDecisionSliceErrorV1> {
        let current = self
            .current
            .as_ref()
            .ok_or(FlatActionDecisionSliceErrorV1::NoCurrentDecision)?;
        flat_validate_current_binding_header_v1(self, current)?;
        flat_validate_expected_decision_v1(self, current, expected)?;
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
}

/// Builds `count` distinct same-controller (`PlayerId::P0`), untargeted
/// (`TargetSpec::None`, needing zero targets, so `Decision::OrderTriggers`
/// is the very next decision -- `engine::drain_pending_triggers_or_decide`
/// surfaces `OrderTriggers` for any `group_len >= 2` regardless of target
/// shape), unordered pending triggers, each with a distinct physical source
/// object, then moves every one of those sources into P0's library exactly
/// like `shuffle_trigger_source_into_library_v1` does for a single source
/// (removed from the battlefield, zone bumped, no `library_knowledge`
/// entry) -- so all `count` positions are simultaneously hidden. `count`
/// must be at least 2 (`Decision::OrderTriggers` requires a same-controller
/// group of 2 or more).
///
/// Test-only. Re-exported as `crate::rl_session::hidden_order_triggers_state_v1`.
#[cfg(test)]
pub(crate) fn hidden_order_triggers_state_v1(count: usize) -> (crate::state::GameState, Vec<ObjectId>) {
    use crate::card_def::TargetSpec;
    use crate::effect::EffectOp;
    use crate::policy_observation_v6::tests::{put, ready_state};
    use crate::state::AbilitySourceContractV4;
    use crate::trigger::PendingTrigger;

    assert!(count >= 2, "Decision::OrderTriggers needs a group of 2 or more");
    let mut state = ready_state();
    let mut sources = Vec::with_capacity(count);
    for _ in 0..count {
        let object = put(&mut state, PlayerId::P0, "Myr Enforcer", Zone::Battlefield);
        let contract = AbilitySourceContractV4::capture(&state, object);
        state.engine.pending_triggers.push(PendingTrigger {
            controller: PlayerId::P0,
            source: object,
            granted_by: None,
            effect: EffectOp::Sequence(vec![]),
            is_madness_offer: false,
            kicked: false,
            target_spec: TargetSpec::None,
            targets: Vec::new(),
            target_contracts: Vec::new(),
            placement_ordered: false,
            source_contract: Some(contract),
            optional_additional_cost_paid: None,
            paid_cost_refs: Vec::new(),
        });
        sources.push(object);
    }
    for &object in &sources {
        state.players[PlayerId::P0.index()]
            .battlefield
            .retain(|&id| id != object);
        {
            let live = state.objects.get_mut(object);
            assert_eq!(live.zone, Zone::Battlefield);
            live.zone = Zone::Library;
            live.zone_change_count += 1;
        }
        state.players[PlayerId::P0.index()].library.push(object);
        assert!(state.library_knowledge[PlayerId::P0.index()][PlayerId::P0.index()]
            .iter()
            .all(|entry| entry.object != object));
    }
    (state, sources)
}

/// Adversarial-review fixture: one physical permanent with two abilities
/// that both trigger off the same event, giving two `PendingTrigger`
/// entries with the identical `source` `ObjectId` (and, once shuffled, the
/// identical live `zone_change_count` too) -- the exact scenario the
/// arena-id-based resolution bug applied to. Both positions are
/// simultaneously hidden. `Decision::OrderTriggers` is the active decision
/// (group_len 2).
///
/// Test-only. Re-exported as `crate::rl_session::hidden_order_triggers_shared_source_state_v1`.
#[cfg(test)]
pub(crate) fn hidden_order_triggers_shared_source_state_v1() -> (crate::state::GameState, ObjectId) {
    use crate::card_def::TargetSpec;
    use crate::effect::EffectOp;
    use crate::policy_observation_v6::tests::{put, ready_state};
    use crate::state::AbilitySourceContractV4;
    use crate::trigger::PendingTrigger;

    let mut state = ready_state();
    let object = put(&mut state, PlayerId::P0, "Myr Enforcer", Zone::Battlefield);
    let contract = AbilitySourceContractV4::capture(&state, object);
    for _ in 0..2 {
        state.engine.pending_triggers.push(PendingTrigger {
            controller: PlayerId::P0,
            source: object,
            granted_by: None,
            effect: EffectOp::Sequence(vec![]),
            is_madness_offer: false,
            kicked: false,
            target_spec: TargetSpec::None,
            targets: Vec::new(),
            target_contracts: Vec::new(),
            placement_ordered: false,
            source_contract: Some(contract),
            optional_additional_cost_paid: None,
            paid_cost_refs: Vec::new(),
        });
    }
    state.players[PlayerId::P0.index()]
        .battlefield
        .retain(|&id| id != object);
    {
        let live = state.objects.get_mut(object);
        assert_eq!(live.zone, Zone::Battlefield);
        live.zone = Zone::Library;
        live.zone_change_count += 1;
    }
    state.players[PlayerId::P0.index()].library.push(object);
    assert!(state.library_knowledge[PlayerId::P0.index()][PlayerId::P0.index()]
        .iter()
        .all(|entry| entry.object != object));
    (state, object)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::Decision;
    use crate::rl_session::{
        avenging_hunter_undercity_arena_choose_targets_state_v1, shuffle_trigger_source_into_library_v1,
    };

    fn expected(session: &FastActorSessionV1) -> FastActorDecisionV1 {
        match session.current_response() {
            FastActorResponseV1::Decision(decision) => decision,
            _ => panic!("fixture must have an active decision"),
        }
    }

    fn encoded_v4(
        session: &FastActorSessionV1,
    ) -> (FlatActionDecisionSliceV3, Vec<FlatActionObjectV2>) {
        // Sized from the decision's own legal_action_count rather than a
        // fixed constant: `Decision::OrderTriggers` for `count` pending
        // triggers has `count!` candidate permutations (5040 for the
        // `FLAT_ACTION_MAX_TRIGGER_ORDER_REFS_V1` ceiling of 7), each
        // needing its own action-core row and up to `count` refs/objects.
        let action_count = usize::try_from(expected(session).legal_action_count).unwrap();
        let max_refs = action_count
            .checked_mul(FLAT_ACTION_MAX_TRIGGER_ORDER_REFS_V1)
            .unwrap()
            .max(256);
        let mut actions = vec![FlatActionCoreV1::default(); action_count.max(1)];
        let mut refs = vec![FlatActionRefV2::default(); max_refs];
        let mut objects = vec![FlatActionObjectV2::default(); max_refs];
        let result = session
            .encode_current_flat_action_slice_v4(
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

    fn pending_source_ordinals(objects: &[FlatActionObjectV2]) -> Vec<u16> {
        let mut ordinals: Vec<u16> = objects
            .iter()
            .filter(|row| row.group == FlatActionObjectGroupV1::HistoricalPublicSource)
            .map(|row| row.actor_visible_ordinal)
            .collect();
        ordinals.sort_unstable();
        ordinals
    }

    #[test]
    fn v4_single_hidden_choose_targets_resolves_by_frozen_contract_same_as_v3() {
        // The V4 action-slice resolver must keep agreeing with V3 on the
        // single-position ChooseTargets case it already handles: this is
        // the "prove V3 bit-identity" fixture from item 14, encoded through
        // both paths, asserting they agree.
        let (mut state, hunter, _goaded, _ordinary) =
            avenging_hunter_undercity_arena_choose_targets_state_v1(false);
        shuffle_trigger_source_into_library_v1(&mut state, hunter, PlayerId::P0);
        let session = FastActorSessionV1::from_v3_fixture_state(state);

        let mut v3_actions = vec![FlatActionCoreV1::default(); 128];
        let mut v3_refs = vec![FlatActionRefV2::default(); 256];
        let mut v3_objects = vec![FlatActionObjectV2::default(); 128];
        let v3_result = session
            .encode_current_flat_action_slice_v3(
                expected(&session),
                &mut FlatActionDecisionSliceBuffersV2 {
                    actions: &mut v3_actions,
                    refs: &mut v3_refs,
                    objects: &mut v3_objects,
                },
            )
            .unwrap();
        v3_objects.truncate(usize::from(v3_result.active_object_count));

        let (_v4_result, v4_objects) = encoded_v4(&session);

        assert_eq!(pending_source_ordinals(&v3_objects), pending_source_ordinals(&v4_objects));
        assert_eq!(v3_objects, v4_objects, "V4 must reproduce the exact V3 row set for this decision");
    }

    #[test]
    fn v4_single_hidden_order_triggers_succeeds_where_v3_still_errors() {
        let (mut state, sources) = hidden_order_triggers_state_v1(2);
        // Only position 0 is hidden; move position 1 back to the battlefield
        // (ordinary, resolvable) so this fixture exercises exactly one
        // hidden position inside a real `Decision::OrderTriggers`.
        let visible = sources[1];
        state.players[PlayerId::P0.index()]
            .library
            .retain(|&id| id != visible);
        {
            let live = state.objects.get_mut(visible);
            assert_eq!(live.zone, Zone::Library);
            live.zone = Zone::Battlefield;
            live.zone_change_count += 1;
        }
        state.players[PlayerId::P0.index()].battlefield.push(visible);

        let session = FastActorSessionV1::from_v3_fixture_state(state);
        assert!(matches!(
            session.current.as_ref().unwrap().origin_decision,
            PolicyDecisionV5::Surface(SurfaceDecision::Decision(Decision::OrderTriggers { .. }))
        ));

        // V3's own action-slice encoder must still raise HiddenActionReference:
        // this residual is permanent for the frozen V3 path (never fixed
        // there), so this assertion documents, rather than merely assumes,
        // that this fixture actually reproduces the gap this work closes
        // only for V4.
        let mut v3_actions = vec![FlatActionCoreV1::default(); 128];
        let mut v3_refs = vec![FlatActionRefV2::default(); 256];
        let mut v3_objects = vec![FlatActionObjectV2::default(); 128];
        let v3_error = session
            .encode_current_flat_action_slice_v3(
                expected(&session),
                &mut FlatActionDecisionSliceBuffersV2 {
                    actions: &mut v3_actions,
                    refs: &mut v3_refs,
                    objects: &mut v3_objects,
                },
            )
            .unwrap_err();
        assert_eq!(v3_error, FlatActionDecisionSliceErrorV1::HiddenActionReference);

        let (result, objects) = encoded_v4(&session);
        assert!(result.active_action_count > 0);
        let pending_rows: Vec<_> = objects
            .iter()
            .filter(|row| row.group == FlatActionObjectGroupV1::HistoricalPublicSource)
            .collect();
        assert_eq!(pending_rows.len(), 1);
        assert_eq!(pending_rows[0].actor_visible_ordinal, 1);
        assert!(objects
            .iter()
            .any(|row| row.group == FlatActionObjectGroupV1::SelfBattlefield));
    }

    fn multi_hidden_order_triggers_case(count: usize) {
        let (state, _sources) = hidden_order_triggers_state_v1(count);
        let session = FastActorSessionV1::from_v3_fixture_state(state);
        assert!(matches!(
            session.current.as_ref().unwrap().origin_decision,
            PolicyDecisionV5::Surface(SurfaceDecision::Decision(Decision::OrderTriggers { .. }))
        ));

        // V3 remains permanently gapped for every position beyond 0 (and,
        // per the shared gate, gapped for OrderTriggers entirely).
        let mut v3_actions = vec![FlatActionCoreV1::default(); 128];
        let mut v3_refs = vec![FlatActionRefV2::default(); 256];
        let mut v3_objects = vec![FlatActionObjectV2::default(); 128];
        let v3_error = session
            .encode_current_flat_action_slice_v3(
                expected(&session),
                &mut FlatActionDecisionSliceBuffersV2 {
                    actions: &mut v3_actions,
                    refs: &mut v3_refs,
                    objects: &mut v3_objects,
                },
            )
            .unwrap_err();
        assert_eq!(v3_error, FlatActionDecisionSliceErrorV1::HiddenActionReference);

        let (result, objects) = encoded_v4(&session);
        assert!(result.active_action_count > 0);
        let ordinals = pending_source_ordinals(&objects);
        assert_eq!(ordinals.len(), count, "every hidden position must gain exactly one row");
        let mut expected_ordinals: Vec<u16> = (1..=count as u16).collect();
        expected_ordinals.sort_unstable();
        assert_eq!(
            ordinals, expected_ordinals,
            "ordinals must be collision-free and match the ceiling(=1, empty stack)+position formula"
        );
        let unique: std::collections::BTreeSet<_> = ordinals.iter().copied().collect();
        assert_eq!(unique.len(), count, "no two hidden positions may share an ordinal");
    }

    #[test]
    fn v4_multi_hidden_order_triggers_two_simultaneous() {
        multi_hidden_order_triggers_case(2);
    }

    #[test]
    fn v4_multi_hidden_order_triggers_seven_simultaneous_at_the_flat_action_ceiling() {
        assert_eq!(FLAT_ACTION_MAX_TRIGGER_ORDER_REFS_V1, 7);
        multi_hidden_order_triggers_case(FLAT_ACTION_MAX_TRIGGER_ORDER_REFS_V1);
    }

    /// Adversarial-review MAJOR-finding regression: two simultaneously
    /// hidden pending triggers sharing one physical source object (same
    /// `arena_id`) must each resolve to their OWN distinct ordinal, not
    /// collapse into one via an arena-id scan (which would either silently
    /// alias both positions to the first match, or -- with the old dedup
    /// key -- raise `InvalidActionReference` on the second). Position 0 and
    /// position 1 must both appear, with distinct ordinals, and the two
    /// `FlatActionObjectV2` rows must actually be distinct (`canonical_key`
    /// differs), proving they are two model objects, not one reused twice.
    #[test]
    fn v4_two_hidden_positions_sharing_one_physical_source_resolve_distinctly() {
        let (state, shared_object) = hidden_order_triggers_shared_source_state_v1();
        let session = FastActorSessionV1::from_v3_fixture_state(state);
        assert!(matches!(
            session.current.as_ref().unwrap().origin_decision,
            PolicyDecisionV5::Surface(SurfaceDecision::Decision(Decision::OrderTriggers { .. }))
        ));
        let (result, objects) = encoded_v4(&session);
        assert!(result.active_action_count > 0);
        let pending_rows: Vec<_> = objects
            .iter()
            .filter(|row| row.group == FlatActionObjectGroupV1::HistoricalPublicSource)
            .collect();
        assert_eq!(
            pending_rows.len(),
            2,
            "both positions must gain their own row despite sharing one physical source"
        );
        let mut ordinals: Vec<u16> = pending_rows.iter().map(|row| row.actor_visible_ordinal).collect();
        ordinals.sort_unstable();
        assert_eq!(ordinals, vec![1, 2], "ceiling(=1, empty stack) + position 0 and + position 1");
        assert_ne!(
            pending_rows[0].canonical_key(),
            pending_rows[1].canonical_key(),
            "the two positions must be two distinct model objects, not one row reused twice"
        );
        // Both rows still carry the one shared physical card's identity
        // (same card_token/zone/zone_change_count -- only the ordinal
        // differs, which is exactly what disambiguates the two positions).
        assert_eq!(pending_rows[0].card_token, pending_rows[1].card_token);
        assert_eq!(pending_rows[0].zone_change_count, pending_rows[1].zone_change_count);
        let _ = shared_object;
    }
}
