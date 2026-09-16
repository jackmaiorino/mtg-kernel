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

/// Local (V4-only) analog of `FlatResolvedActionObjectV2`. Two real defects
/// were found in real gameplay by conflating `arena_id` and `position` in
/// this type's dedup/consistency key; both are fixed by the fields below.
///
/// Defect 1 (`InvalidActionReference`): a `Source`-role reference for a
/// `PendingEffect`-context historical row is frozen at the object's
/// zone/`zone_change_count` from when the effect became pending (resolved
/// via the extension-row match in
/// [`flat_visible_action_object_extension_aware_v4`]), while a different
/// role's reference to the SAME physical `arena_id` (for example a
/// `TargetObject` naming a legal target) names the object's CURRENT
/// zone/`zone_change_count`, which can differ if the object has moved zones
/// since the effect became pending (for example battlefield-then-library).
/// Both roles default `position` to 0 (only `OrderTriggers` ever produces a
/// nonzero `order_index`), so `arena_id` and `position` alone collided even
/// though the two references, and their correctly-resolved objects, are
/// legitimately different. Fix: the full `reference` is now part of the
/// key too, so two references naming different points in one object's
/// history are tracked as distinct entries -- resolved the same way
/// `register_extensions_v4` resolves the analogous shared-physical-source
/// case, as two independently valid rows, never a forced mismatch.
///
/// Defect 2 (`DuplicateCanonicalObject`): the mirror-image mistake. Two
/// `OrderTriggers` positions (0 and 1) whose *physical* source is the same
/// VISIBLE (not hidden) permanent both resolve through the
/// position-INSENSITIVE ordinary zone lookup (see `position_sensitive`
/// below): the object is visible, so
/// `pending_trigger_frozen_source_components_v4`'s hidden check fails for
/// both positions, and the ordinary fallback's ordinal depends only on the
/// object's own live zone, never on `position`. Both positions therefore
/// legitimately resolve to the byte-identical row (same `reference`, same
/// object) -- exactly as V3's plain arena_id-only dedup would collapse them
/// -- yet keying strictly on `(arena_id, position, reference)` still forced
/// them apart (`position` differs), so the second one was rejected as a
/// canonical-key collision against the first. Fix: `position` is only part
/// of the "already resolved" key when the resolution that produced the
/// EXISTING entry was itself position-sensitive (i.e. actually came from
/// `pending_trigger_frozen_source_components_v4`, whose ordinal genuinely
/// depends on `position`); otherwise two positions naming the identical
/// `(arena_id, reference)` share one entry, matching V3's behavior for the
/// visible case (V3 has no position-sensitive path at all). A single
/// physical source cannot be position-sensitive at one position and not at
/// another within one decision: hiddenness is a property of the live
/// object alone (`pending_trigger_hidden_source_v1` reads only
/// `state.objects`/`state.library_knowledge`, never `position`), so this
/// split is unambiguous.
#[derive(Debug, Clone, PartialEq, Eq)]
struct FlatResolvedActionObjectV4 {
    arena_id: u32,
    position: u32,
    reference: CardStableRefV1,
    position_sensitive: bool,
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

/// V4-only stack-ordinal resolution. A card that is itself a spell on the
/// stack can simultaneously source a `TriggerCondition::CastSelf`/
/// `home_zone: Zone::Stack` triggered ability of its own (Writhing
/// Chrysalis's cast trigger, `trigger.rs`'s `WRITHING_CHRYSALIS_TRIGGERS` --
/// the only card using this shape at the time of this fix), which places a
/// SECOND `StackItem` on the stack whose `source` is the identical
/// `arena_id` as the spell (a `home_zone: Zone::Stack` trigger is sourced
/// from the stack object that spawned it, which is the spell itself). The
/// shared `flat_stack_action_object_ordinal_v1` (`rl_session.rs`, also
/// called by the frozen V3 path) scans for `item.source == object_id` and
/// deliberately refuses to guess when two or more stack items match,
/// returning `InvalidActionReference` -- correct when the ambiguity is
/// genuine, but a real V4 fresh-lineage game (campaign-001 block 1, seed
/// 3157112932801185221, Terror vs Wildfire) hit it for a `TargetObject`
/// reference that names the SPELL unambiguously (Terror's own `Counterspell`
/// choosing its only legal target: Writhing Chrysalis's spell, while
/// Writhing Chrysalis's own cast trigger also sits on the stack one
/// position above it) -- Counterspell can only ever target a spell, never
/// an ability, so the reference cannot mean the triggered ability.
///
/// A `StackItem` actually carries the distinction the shared scan ignores:
/// `kind`. The object's own spell presence is always the unique entry with
/// `source == object_id && kind == StackItemKind::Spell` -- an ability's
/// entry never claims to BE the object, only to be sourced FROM it. An
/// ordinary object reference names the object's own zone presence, exactly
/// as every other zone arm in `flat_visible_action_object_components_v4`
/// resolves to the object's own position and never some other thing's, so
/// it always means that spell entry when one exists, even while an ability
/// sourced from the same object also sits on the stack. Falls back to the
/// shared, unmodified `flat_stack_action_object_ordinal_v1` -- byte-identical
/// to what V3 still calls, so V3's own resolution (including its
/// `detached_matches`/resolving-spell-popped-before-an-optional-cost case)
/// is untouched -- whenever no unique spell entry exists, so no case that
/// path already resolves (correctly or by its own defensive error) changes.
fn flat_stack_action_object_ordinal_v4(
    state: &crate::state::GameState,
    object_id: ObjectId,
    controller: PlayerId,
) -> Result<usize, FlatActionDecisionSliceErrorV1> {
    let mut spell_positions = state.stack.iter().enumerate().filter(|(_, item)| {
        item.source == object_id && item.kind == crate::state::StackItemKind::Spell
    });
    match (spell_positions.next(), spell_positions.next()) {
        (Some((ordinal, _)), None) => Ok(ordinal),
        (None, _) => flat_stack_action_object_ordinal_v1(state, object_id, controller),
        (Some(_), Some(_)) => Err(FlatActionDecisionSliceErrorV1::InvalidActionReference),
    }
}

/// Fork of `flat_visible_action_object_components_v1`: every non-`Library`
/// arm is byte-identical; the frozen/hidden fallback
/// ([`pending_trigger_frozen_source_components_v4`]) is now checked FIRST
/// (a frozen reference's `zone`/`zone_change_count` intentionally do not
/// match the live object's current, post-shuffle state -- that mismatch is
/// exactly how a frozen reference is recognized, not an ordinary
/// consistency error), with `position` threaded through from the caller
/// instead of derived by scanning for a matching `arena_id`.
///
/// Resolves the components for a reference, and reports whether the
/// resolution was *position-sensitive*: `true` only when
/// [`pending_trigger_frozen_source_components_v4`] actually produced the
/// result (its ordinal is `ceiling + position`, so two different positions
/// for the same physical source genuinely need distinct rows there). Every
/// other arm (the ordinary zone-based lookup below) computes its ordinal
/// purely from the object's own live zone state, independent of `position`,
/// so two different positions resolving the SAME visible object through
/// this path always compute the identical result and must be allowed to
/// share one row -- exactly as V3's plain arena_id dedup already does for
/// the same case (V3 has no position-sensitive path at all). Callers use
/// this to decide whether `position` belongs in their own dedup key; see
/// `FlatResolvedActionObjectV4`'s doc comment.
fn flat_visible_action_object_components_v4(
    state: &crate::state::GameState,
    actor: PlayerId,
    position: u32,
    reference: &CardStableRefV1,
) -> Result<(FlatVisibleActionObjectComponentsV1, bool), FlatActionDecisionSliceErrorV1> {
    if let Some(components) =
        pending_trigger_frozen_source_components_v4(state, actor, position, reference)?
    {
        return Ok((components, true));
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
            flat_stack_action_object_ordinal_v4(state, object_id, object.controller)?,
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
    Ok((
        FlatVisibleActionObjectComponentsV1 {
            card_db_id: object.card_def,
            group,
            actor_visible_ordinal: ordinal,
            owner_relative: flat_relative_seat_v1(owner, actor.into())?,
            controller_relative: flat_relative_seat_v1(controller, actor.into())?,
            zone: flat_zone_v1(object.zone),
            zone_change_count: object.zone_change_count,
        },
        false,
    ))
}

fn flat_visible_action_object_v4(
    state: &crate::state::GameState,
    actor: PlayerId,
    position: u32,
    reference: &CardStableRefV1,
) -> Result<(FlatActionObjectV2, bool), FlatActionDecisionSliceErrorV1> {
    let (fields, position_sensitive) =
        flat_visible_action_object_components_v4(state, actor, position, reference)?;
    Ok((
        FlatActionObjectV2 {
            card_token: flat_card_token_v2(fields.card_db_id),
            group: fields.group,
            actor_visible_ordinal: u16::try_from(fields.actor_visible_ordinal)
                .map_err(|_| FlatActionDecisionSliceErrorV1::CheckedIntegerRange)?,
            owner_relative: fields.owner_relative,
            controller_relative: fields.controller_relative,
            zone: fields.zone,
            zone_change_count: fields.zone_change_count,
        },
        position_sensitive,
    ))
}

/// `current.candidates` is not always in `origin_decision`'s raw,
/// engine-surfaced shape: `flat_action_v3::prepare_and_build_v3` (run by
/// `advance_to_decision_or_terminal_profiled` for every physical decision,
/// unconditionally, since a fast-actor session's `flat_action_contract_mode`
/// is always `V3` regardless of which generation later scores it) calls
/// `flat_action_v3::normalize_candidates` in place on `current.candidates`,
/// which (a) reorders a `Decision::ChooseEffectTargets` menu into
/// decision-local-library canonical (class-ordinal-grouped) order whenever
/// the targets are cards being searched out of a library, and (b) *removes*
/// a `ChooseAttackerInclusion { include: false, .. }` candidate outright
/// when the attacker is goaded/forced, shrinking an `AttackerInclusion`
/// decision's fixed `[exclude, include]` pair down to a single `[include]`
/// entry (see `normalize_candidates`'s `decision_local_library` arm and its
/// goaded-attacker `retain` call). Calling `flat_validate_origin_decision_v1`
/// -- or, for the `AttackerInclusion`/`BlockerInclusion` shape check,
/// `flat_validate_current_binding_header_v1`/
/// `flat_validate_current_decision_relations_v1`'s
/// `let [first, second] = current.candidates.as_slice() else { .. }` -- directly
/// against `current` (as the V3 path never does -- see
/// `flat_action_v3::build_with_extensions`, which validates a freshly
/// re-derived *raw* candidate list instead) spuriously rejects both
/// reordered and filtered cases with `InvalidDecisionRelation`.
///
/// This mirrors `build_with_extensions`'s exact technique instead: re-derive
/// the raw candidate list from `origin_decision` with
/// `core_policy_action_candidates_v5` (this is provably what
/// `current.candidates` held immediately after the decision was surfaced,
/// before any in-place normalization -- `advance_to_decision_or_terminal_profiled`
/// builds it with that exact same call), validate the raw list against both
/// `flat_validate_current_binding_header_v1` and `flat_validate_origin_decision_v1`,
/// then apply the identical `normalize_candidates` transform (reused
/// unmodified from the V3 module, not reimplemented) and require the result
/// to equal `current.candidates` byte-for-byte, exactly as V3 already proves
/// for its own cache. Only the origin/shape proof is redone here: the actual
/// `(actions, refs, objects)` this function returns are still built from
/// `current.candidates` (the real, live decision) below, via
/// `flat_visible_action_object_v4`/`frozen_pending_trigger_semantic_v4`, not
/// from this function's `raw` reconstruction.
fn validate_origin_decision_against_reordered_candidates_v4(
    session: &FastActorSessionV1,
    current: &FastActorCurrentDecisionV1,
    extension: &crate::policy_observation_v6::PolicyObservationExtensionsV6,
) -> Result<(), FlatActionDecisionSliceErrorV1> {
    let raw = crate::rl::core_policy_action_candidates_v5(&current.origin_decision, &session.state)
        .map_err(|_| FlatActionDecisionSliceErrorV1::InvalidDecisionRelation)?;
    let mut original = current.clone();
    original.flat_action_cache = None;
    original.flat_action_cache_error = None;
    original.flat_action_cache_v2 = None;
    original.flat_action_cache_error_v2 = None;
    original.candidates = raw;
    flat_validate_current_binding_header_v1(session, &original)?;
    flat_validate_origin_decision_v1(&original, &session.state)?;
    super::flat_action_v3::normalize_candidates(
        &mut original.candidates,
        extension,
        session,
        &original.origin_decision,
    )?;
    if original.candidates != current.candidates {
        return Err(FlatActionDecisionSliceErrorV1::InvalidDecisionRelation);
    }
    Ok(())
}

/// V4 sibling of `flat_action_v3::extension_object` (byte-identical field
/// construction, including the `DecisionLocalLibrary` group's forced
/// `zone_change_count: 0`): builds an action-object row for a reference the
/// caller has already matched against an extension row, rather than an
/// ordinary live zone lookup.
fn extension_object_v4(
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

/// V4 sibling of `flat_action_v3::build_with_extensions`'s per-reference
/// resolution closure: a `decision_local_library` search reveals cards sitting
/// in `Zone::Library`, which `flat_visible_action_object_v4`'s ordinary
/// zone-based lookup can never resolve (a library card has no
/// `library_knowledge` entry precisely because it is privately, decision-
/// locally revealed, not publicly known) -- and a `PendingEffect` historical
/// source must resolve to the `HistoricalPublicSource` group/ordinal the
/// registry (`register_extensions_v4`) assigns it, not whatever ordinary
/// group its live zone happens to imply. Both extension rows are checked
/// first, exactly as the V3 closure does (`historical` restricted to the
/// `Source` role, `library` unconditional on role), falling through to the
/// ordinary [`flat_visible_action_object_v4`] resolution -- which already
/// covers the `PendingTrigger` case via [`pending_trigger_frozen_source_components_v4`]
/// -- for every other reference.
///
/// Also reports whether the resolution was position-sensitive (see
/// [`flat_visible_action_object_components_v4`]'s doc comment): always
/// `false` for both extension-row matches here, since neither the
/// `PendingEffect` row's enumerate-index ordinal nor the
/// `decision_local_library` search-position ordinal depends on `position`
/// at all -- only [`pending_trigger_frozen_source_components_v4`], reached
/// through the ordinary fallback, ever is.
fn flat_visible_action_object_extension_aware_v4(
    state: &crate::state::GameState,
    actor: PlayerId,
    position: u32,
    role: FlatActionRefRoleV1,
    reference: &CardStableRefV1,
    extension: &crate::policy_observation_v6::PolicyObservationExtensionsV6,
) -> Result<(FlatActionObjectV2, bool), FlatActionDecisionSliceErrorV1> {
    if role == FlatActionRefRoleV1::Source {
        if let Some((ordinal, _)) = extension
            .historical_public_sources
            .iter()
            .enumerate()
            .find(|(_, row)| {
                row.context == crate::policy_observation_v6::HistoricalSourceContextV6::PendingEffect
                    && row.source == *reference
            })
        {
            return extension_object_v4(
                reference,
                actor,
                FlatActionObjectGroupV1::HistoricalPublicSource,
                ordinal,
            )
            .map(|object| (object, false));
        }
    }
    if let Some(ordinal) = extension
        .decision_local_library
        .as_ref()
        .and_then(|search| search.cards.iter().position(|card| card.stable == *reference))
    {
        return extension_object_v4(reference, actor, FlatActionObjectGroupV1::DecisionLocalLibrary, ordinal)
            .map(|object| (object, false));
    }
    flat_visible_action_object_v4(state, actor, position, reference)
}

impl FastActorSessionV1 {
    /// V4 sibling of `encode_current_flat_action_slice_v3`. Cache-free: it
    /// re-derives `(actions, refs, objects)` fresh from `current.candidates`
    /// every call, using [`flat_visible_action_object_v4`] in place of
    /// `flat_visible_action_object_v2`, instead of consulting or installing
    /// `current.flat_action_cache_v2` (a V3-mode-gated field this function
    /// never touches). `flat_validate_expected_decision_v1` is reused
    /// unmodified, directly against `current` (it only compares counts and
    /// metadata, never candidate order or arity, so `current.candidates`
    /// already being reordered/filtered by `flat_action_v3::prepare_and_build_v3`
    /// does not affect it). `flat_validate_current_binding_header_v1` and
    /// `flat_validate_origin_decision_v1` are reused too, but never called
    /// directly against `current`: both assume `candidates` is still in
    /// `origin_decision`'s raw, unfiltered, engine-surfaced shape (a fixed
    /// `[exclude, include]` pair for `AttackerInclusion`/`BlockerInclusion`,
    /// raw order for a library-search `ChooseEffectTargets`), which
    /// `prepare_and_build_v3`'s `normalize_candidates` may no longer hold --
    /// it reorders decision-local-library targets, and its goaded-attacker
    /// retain-filter can shrink a forced attacker's menu from 2 candidates to
    /// 1. See [`validate_origin_decision_against_reordered_candidates_v4`],
    /// which re-derives the raw shape, validates both functions against
    /// that, then proves the live (possibly reordered/filtered)
    /// `current.candidates` really is `normalize_candidates`'s output.
    pub(crate) fn encode_current_flat_action_slice_v4(
        &self,
        expected: FastActorDecisionV1,
        buffers: &mut FlatActionDecisionSliceBuffersV2<'_>,
    ) -> Result<FlatActionDecisionSliceV3, FlatActionDecisionSliceErrorV1> {
        let current = self
            .current
            .as_ref()
            .ok_or(FlatActionDecisionSliceErrorV1::NoCurrentDecision)?;
        flat_validate_expected_decision_v1(self, current, expected)?;
        let extension = crate::rl::policy_observation_extensions_v6(&self.state, current.actor)
            .map_err(|_| FlatActionDecisionSliceErrorV1::InvalidDecisionRelation)?;
        validate_origin_decision_against_reordered_candidates_v4(self, current, &extension)?;
        let actor: PlayerSeatV1 = current.actor.into();
        let mut actions_out: Vec<FlatActionCoreV1> = Vec::with_capacity(current.candidates.len());
        let mut unindexed_refs: Vec<FlatUnindexedActionRefV2> = Vec::new();
        // See `FlatResolvedActionObjectV4`'s doc comment for the two real
        // defects this dedup/consistency key was widened to fix, and why
        // `position` only joins the "already resolved" match when the
        // EXISTING entry's own resolution was position-sensitive.
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
                    let (object, position_sensitive) = flat_visible_action_object_extension_aware_v4(
                        &self.state,
                        current.actor,
                        position,
                        role,
                        reference,
                        &extension,
                    )?;
                    if let Some(previous) = resolved_objects.iter().find(|candidate| {
                        candidate.arena_id == reference.arena_id
                            && candidate.reference == *reference
                            && candidate.position_sensitive == position_sensitive
                            && (!position_sensitive || candidate.position == position)
                    }) {
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
                            reference: reference.clone(),
                            position_sensitive,
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
    /// (staleness/non-emptiness via
    /// [`flat_validate_current_binding_staleness_v4`], not the full shared
    /// `flat_validate_current_binding_header_v1` -- see that helper's doc
    /// comment for why calling the shared one directly against `current` is
    /// unsafe here too, and
    /// `flat_validate_expected_decision_v1`) and then builds the
    /// observation the same way `flat_policy_observation_v3` does
    /// (`rl::observe_policy_v6_unhashed_for_flat_policy`, unmodified) --
    /// the observation itself does not depend on the V3 action cache at
    /// all, only `flat_policy_observation_v3`'s own validation gate did.
    /// The full origin-decision/candidate-shape proof is not redone here:
    /// this function's only caller, `build_scoring_owned_v4`, always calls
    /// `encode_current_flat_action_slice_v4` first, which already proved it
    /// (via `validate_origin_decision_against_reordered_candidates_v4`).
    pub(crate) fn flat_policy_observation_v4(
        &self,
        expected: FastActorDecisionV1,
    ) -> Result<crate::policy_observation_v6::ObservationV6, FlatActionDecisionSliceErrorV1> {
        let current = self
            .current
            .as_ref()
            .ok_or(FlatActionDecisionSliceErrorV1::NoCurrentDecision)?;
        flat_validate_current_binding_staleness_v4(self, current)?;
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

/// The staleness/non-emptiness half of the shared
/// `flat_validate_current_binding_header_v1` (`rl_session.rs`), without its
/// `flat_validate_current_decision_relations_v1` call: that call's
/// `AttackerInclusion`/`BlockerInclusion` arm requires `current.candidates`
/// to still be the origin's raw, unfiltered `[exclude, include]` pair, which
/// does not hold once `flat_action_v3::prepare_and_build_v3`'s
/// goaded-attacker retain-filter has shrunk it to a single forced `include`
/// candidate (the same class of problem
/// `validate_origin_decision_against_reordered_candidates_v4` fixes for the
/// action slice). [`flat_policy_observation_v4`] never needs that relations
/// proof redone (its only caller already ran it, successfully, through the
/// action slice first), only proof that `current` is still the session's
/// live, bound, non-empty decision.
fn flat_validate_current_binding_staleness_v4(
    session: &FastActorSessionV1,
    current: &FastActorCurrentDecisionV1,
) -> Result<(), FlatActionDecisionSliceErrorV1> {
    if current.environment_revision != session.environment_revision
        || current.bound_policy_step_count != session.policy_step_count
        || current.bound_physical_decision_count != session.physical_decision_count
    {
        return Err(FlatActionDecisionSliceErrorV1::CorruptCurrentBinding);
    }
    if current.candidates.is_empty() {
        return Err(FlatActionDecisionSliceErrorV1::InvalidDecisionRelation);
    }
    Ok(())
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

/// Campaign-001 block-1 regression fixture: a card that is itself a spell
/// on the stack simultaneously sources a `TriggerCondition::CastSelf`/
/// `home_zone: Zone::Stack` triggered ability of its own (Writhing
/// Chrysalis's cast trigger shape, `trigger.rs`'s
/// `WRITHING_CHRYSALIS_TRIGGERS`, the only card using it at the time of
/// this fix), so `state.stack` holds two entries with the identical
/// physical `source`: the spell itself (`StackItemKind::Spell`, pushed
/// first, so its ordinal is 0) and its own cast trigger
/// (`StackItemKind::TriggeredAbility`, pushed second). Mechanically
/// identical to real gameplay's shape (a real game reached it: campaign-001
/// block 1, seed 3157112932801185221, Terror vs Wildfire, step 155,
/// Terror's own `Counterspell` choosing its only legal target) without
/// depending on that real game or its policy weights, matching this
/// module's existing `*_shared_source_state_v1` fixtures' idiom.
///
/// Test-only. Re-exported as
/// `crate::rl_session::spell_and_own_cast_trigger_shared_source_state_v1`.
#[cfg(test)]
pub(crate) fn spell_and_own_cast_trigger_shared_source_state_v1(
) -> (crate::state::GameState, ObjectId) {
    use crate::policy_observation_v6::tests::{put, ready_state};
    use crate::state::{StackItem, StackItemKind, StackStateV4};

    let mut state = ready_state();
    let object = put(&mut state, PlayerId::P0, "Writhing Chrysalis", Zone::Battlefield);
    state.players[PlayerId::P0.index()]
        .battlefield
        .retain(|&id| id != object);
    {
        let live = state.objects.get_mut(object);
        assert_eq!(live.zone, Zone::Battlefield);
        live.zone = Zone::Stack;
        live.zone_change_count += 1;
    }
    let stack_item = |kind, inline_effect| StackItem {
        kind,
        source: object,
        controller: PlayerId::P0,
        targets: Vec::new(),
        is_copy: false,
        inline_effect,
        discarded: Vec::new(),
        is_flashback: false,
        mode_chosen: 0,
        madness_offer: false,
        kicked: false,
        v4: StackStateV4::default(),
    };
    state.stack.push(stack_item(StackItemKind::Spell, None));
    state.stack.push(stack_item(
        StackItemKind::TriggeredAbility,
        Some(crate::effect::EffectOp::Sequence(Vec::new())),
    ));
    (state, object)
}

/// Burst-2 regression fixture (defect 2, `DuplicateCanonicalObject`):
/// [`hidden_order_triggers_shared_source_state_v1`] minus the final
/// shuffle-into-library step, so the one shared physical source stays
/// VISIBLE on the battlefield instead of hidden. Both `pending_sources`
/// positions must therefore resolve through the position-INSENSITIVE
/// ordinary zone lookup (`pending_trigger_hidden_source_v1` is false for a
/// visible object, so `pending_trigger_frozen_source_components_v4` never
/// fires for either position), landing on the byte-identical
/// `SelfBattlefield` row for both -- real gameplay's actual shape (two
/// simultaneous triggers off one still-battlefield permanent is a common,
/// unremarkable board state, unlike the hidden case above).
///
/// Test-only. Re-exported as `crate::rl_session::visible_order_triggers_shared_source_state_v1`.
#[cfg(test)]
pub(crate) fn visible_order_triggers_shared_source_state_v1() -> (crate::state::GameState, ObjectId)
{
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
    (state, object)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::Decision;
    use crate::rl_session::{
        avenging_hunter_undercity_arena_choose_targets_state_v1, goaded_attacker_fixture_state_v3,
        shuffle_trigger_source_into_library_v1,
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

    /// V4 sibling of
    /// `flat_action_v3::tests::v3_active_goad_is_include_only_at_every_prefix_and_v2_pair_is_unchanged`:
    /// an always-run, outcome-asserting proof that
    /// `validate_origin_decision_against_reordered_candidates_v4` actually
    /// handles the goaded-attacker `AttackerInclusion` shrink (a fixed
    /// `[exclude, include]` pair reduced by `normalize_candidates`'s
    /// retain-filter to a single forced `include` candidate when the
    /// attacker is goaded), not merely "does not error" -- the only prior
    /// coverage was the `#[ignore]`d 20-game soak
    /// (`expanded_deck_training_v1.rs`'s
    /// `v4_gameplay_soak_over_standard_decks_completes_every_seed`), which
    /// asserts nothing about this specific decision. Reuses the same
    /// `goaded_attacker_fixture_state_v3` fixture the V3 test does (a real
    /// Arena-room goad, staged with an ordinary creature on either side of
    /// the goaded one), encodes every `AttackerInclusion` prefix through
    /// `encode_current_flat_action_slice_v4`, and drives the selected index
    /// through the same `session.step` real V4 gameplay uses (there is no
    /// V4-specific consume wrapper: `encode_current_flat_action_slice_v4` is
    /// cache-free, so nothing needs re-validating against a cached V4
    /// binding before stepping).
    #[test]
    fn v4_active_goad_is_include_only_at_every_prefix_and_apply_updates_declared_attackers() {
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
                if is_goaded {
                    assert!(matches!(
                        current.candidates[0].semantic,
                        ActionSemanticV1::ChooseAttackerInclusion { include: true, .. }
                    ));
                }
                let (slice, _) = encoded_v4(&session);
                assert_eq!(
                    slice.active_action_count,
                    if is_goaded { 1 } else { 2 },
                    "the V4 action slice must encode the single forced include candidate for \
                     the goaded attacker, and the ordinary optional pair otherwise"
                );
                // Index zero means forced include for the goaded creature,
                // and voluntary exclude for the ordinary creature (same
                // convention the V3 test uses).
                session.step(decision.episode_id, decision.step, 0).unwrap();
            }
            assert!(session.state.engine.combat.attackers.contains(&goaded));
            assert!(!session.state.engine.combat.attackers.contains(&ordinary));
        }
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

    /// Burst-2 regression: two simultaneous `OrderTriggers` positions (0 and
    /// 1) whose one shared physical source is VISIBLE (still on the
    /// battlefield, not hidden) must resolve to one shared
    /// `SelfBattlefield` row, not two, and must never raise
    /// `DuplicateCanonicalObject`. Before the fix, `position` was
    /// unconditionally part of the dedup key, so position 1's
    /// byte-identical resolution was treated as a NEW entry and rejected as
    /// a canonical-key collision against position 0's already-recorded
    /// entry. Reproduces the real burst-2 finding (seed
    /// 16977991839826055713, Faeries vs Affinity, step 303, a `Surface`
    /// decision) mechanically, without depending on that real game or its
    /// policy weights. V3 is proven unaffected on the identical state: V3's
    /// action-slice encoder has never had a position-sensitive path at all
    /// (its own single, always-position-0 hidden fallback aside), so its
    /// plain arena_id dedup already collapses this case correctly, and did
    /// before and after this fix.
    #[test]
    fn v4_two_visible_order_triggers_positions_sharing_one_physical_source_share_one_row() {
        let (state, shared_object) = visible_order_triggers_shared_source_state_v1();
        let session = FastActorSessionV1::from_v3_fixture_state(state);
        assert!(matches!(
            session.current.as_ref().unwrap().origin_decision,
            PolicyDecisionV5::Surface(SurfaceDecision::Decision(Decision::OrderTriggers { .. }))
        ));

        // V3 must accept this decision too (same live-source arena_id dedup
        // it has always used), never `DuplicateCanonicalObject` or any
        // other error -- proving V3 is unaffected by this class of defect.
        let mut v3_actions = vec![FlatActionCoreV1::default(); 128];
        let mut v3_refs = vec![FlatActionRefV2::default(); 256];
        let mut v3_objects = vec![FlatActionObjectV2::default(); 128];
        session
            .encode_current_flat_action_slice_v3(
                expected(&session),
                &mut FlatActionDecisionSliceBuffersV2 {
                    actions: &mut v3_actions,
                    refs: &mut v3_refs,
                    objects: &mut v3_objects,
                },
            )
            .unwrap();

        let (result, objects) = encoded_v4(&session);
        assert!(result.active_action_count > 0);
        let battlefield_rows: Vec<_> = objects
            .iter()
            .filter(|row| row.group == FlatActionObjectGroupV1::SelfBattlefield)
            .collect();
        assert_eq!(
            battlefield_rows.len(),
            1,
            "both positions share one physical, visible source and must collapse to one row, \
             matching V3's plain arena_id dedup for the same live-source case"
        );
        assert!(objects
            .iter()
            .all(|row| row.group != FlatActionObjectGroupV1::HistoricalPublicSource));
        let _ = shared_object;
    }

    /// Campaign-001 block-1 regression, fast self-contained unit fixture
    /// for the mechanism (see `flat_stack_action_object_ordinal_v4`'s doc
    /// comment and `spell_and_own_cast_trigger_shared_source_state_v1`):
    /// an ordinary object reference to a card that is itself a spell on the
    /// stack must resolve to that spell's own stack position, even while a
    /// `home_zone: Zone::Stack` triggered ability it sourced (Writhing
    /// Chrysalis's cast trigger) also sits on the stack claiming the same
    /// `source`. Before this fix, `flat_stack_action_object_ordinal_v1`'s
    /// same-`source` ambiguity guard could not tell the two stack entries
    /// apart and raised `InvalidActionReference` for both V3 and V4 alike
    /// (the real crash: campaign-001 block 1, seed 3157112932801185221,
    /// Terror vs Wildfire, step 155, `V4 actor-visible encoding:
    /// Action(InvalidActionReference)`).
    ///
    /// V3 confirmed unaffected on the identical synthetic state: it never
    /// gained the spell-kind preference, so
    /// `flat_visible_action_object_components_v1` still hits the same
    /// pre-existing ambiguity error it always has, exactly as before this
    /// fix -- this class of state was simply never resolvable through
    /// either generation until now.
    #[test]
    fn v4_spell_resolves_to_its_own_stack_position_despite_its_own_cast_trigger_sharing_source() {
        let (state, object) = spell_and_own_cast_trigger_shared_source_state_v1();
        let card_db_id = crate::card_def::card_id_by_name("Writhing Chrysalis").unwrap();
        let live = state.objects.try_get(object).unwrap();
        assert_eq!(live.zone, Zone::Stack);
        let reference = CardStableRefV1 {
            arena_id: object.0,
            card_db_id,
            owner: PlayerSeatV1::P0,
            controller: PlayerSeatV1::P0,
            zone: Zone::Stack,
            zone_change_count: live.zone_change_count,
        };

        let (object_row, position_sensitive) =
            flat_visible_action_object_v4(&state, PlayerId::P0, 0, &reference)
                .expect("V4 must resolve the spell's own stack position unambiguously");
        assert_eq!(object_row.group, FlatActionObjectGroupV1::Stack);
        assert_eq!(object_row.actor_visible_ordinal, 0, "the spell was pushed at stack position 0");
        assert!(!position_sensitive);

        assert_eq!(
            flat_visible_action_object_components_v1(&state, PlayerId::P0, &reference).unwrap_err(),
            FlatActionDecisionSliceErrorV1::InvalidActionReference,
            "V3 must stay byte-identical: its shared-source ambiguity guard is untouched"
        );
    }

    /// Defect-2 regression (`InvalidDecisionRelation`, campaign-001 block-1
    /// sweep offset 26, seed 3157112932801185247, step 457, actor P0,
    /// `legal_action_count` 4, involving Cryptic Serpent): a later,
    /// otherwise-unrelated decision's actor-visible encoding must not fail
    /// merely because a still-pending spell-sourced (`home_zone:
    /// Zone::Stack`) cast trigger's producing spell has already departed
    /// the stack -- the same
    /// `spell_sourced_trigger_state_after_producer_departs_v1` fixture as
    /// `policy_observation_v6::tests::
    /// v6_spell_sourced_trigger_survives_producer_countered_while_still_pending`,
    /// exercised through both actor-visible encoders instead of the raw V6
    /// observation call.
    ///
    /// Root cause was never V4-specific: `encode_current_flat_action_slice_v4`
    /// calls `crate::rl::policy_observation_extensions_v6` directly (for its
    /// own decision-local-library/historical-source extension data), and
    /// both `flat_policy_observation_v3` (`flat_action_v3.rs`) and
    /// `flat_policy_observation_v4` (this file) call the identical shared
    /// `crate::rl::observe_policy_v6_unhashed_for_flat_policy` ->
    /// `build_policy_observation_v6` -> `policy_observation_extensions_with_text_v6`
    /// chain. That chain calls `rl.rs`'s `stack_source_ref` for every stack
    /// item, which calls `engine::validated_stack_item_target_spec` ->
    /// `validate_spell_stack_source` -> `validate_spell_source_contract_fields`
    /// -> `engine::validate_spell_sourced_trigger` -- the exact function
    /// fixed for defect 1 (see its doc comment). Because this per-item
    /// revalidation runs for the WHOLE stack on every later observation,
    /// not only when the stale trigger is about to resolve, a departed
    /// producer anywhere on the stack broke every subsequent decision's
    /// encoding under both V3 and V4 alike, confirmed below: no V4-only or
    /// V3-only file needed a change, so `flat_action_v3.rs` and every V3
    /// pinned file (`docs/research/phase1_v3_contract_frozen_pins_2026-09.md`)
    /// stay byte-identical.
    #[test]
    fn v3_and_v4_encode_a_later_decision_despite_a_still_pending_departed_producer_trigger() {
        let (state, chrysalis) =
            crate::policy_observation_v6::tests::spell_sourced_trigger_state_after_producer_departs_v1(
            );
        assert_eq!(
            state.stack.len(),
            1,
            "only the still-pending cast trigger, its producer already departed"
        );
        let session = FastActorSessionV1::from_v3_fixture_state(state);

        let mut v3_actions = vec![FlatActionCoreV1::default(); 128];
        let mut v3_refs = vec![FlatActionRefV2::default(); 256];
        let mut v3_objects = vec![FlatActionObjectV2::default(); 128];
        session
            .encode_current_flat_action_slice_v3(
                expected(&session),
                &mut FlatActionDecisionSliceBuffersV2 {
                    actions: &mut v3_actions,
                    refs: &mut v3_refs,
                    objects: &mut v3_objects,
                },
            )
            .unwrap_or_else(|e| {
                panic!("V3 action-slice encoding must not fail on a departed-producer trigger: {e:?}")
            });
        session.flat_policy_observation_v3(expected(&session)).unwrap_or_else(|e| {
            panic!("V3 policy observation must not fail on a departed-producer trigger: {e:?}")
        });

        let (v4_result, _objects) = encoded_v4(&session);
        assert!(v4_result.active_action_count > 0);
        session.flat_policy_observation_v4(expected(&session)).unwrap_or_else(|e| {
            panic!("V4 policy observation must not fail on a departed-producer trigger: {e:?}")
        });

        let _ = chrysalis;
    }
}
