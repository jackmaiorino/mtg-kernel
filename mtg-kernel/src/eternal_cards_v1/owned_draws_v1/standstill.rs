//! Native Standstill cast trigger, successful sacrifice, and owned draw attempts.
use super::{has_started, record, JournalV1 as DrawJournalV1};
use crate::card_def::CARD_DEFS;
use crate::effect::{EffectContinuation, EffectOp};
use crate::eternal_cards_v1::EternalOpV1;
use crate::eternal_premodern_p1b_v1::{self as p1b, P1bOpV1};
use crate::event::{self, CommittedEvent, ProposedEvent};
use crate::ids::{ObjectId, PlayerId, StackItemId};
use crate::standard_cards_v1::StandardOpV1;
use crate::state::{AbilitySourceContractV4, GameState, StackItem, StackItemKind, StackStateV4, Zone};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum JournalV1 {
    Triggered { cast_at: usize, cast: crate::cast_identity_v1::ReceiptV1, source: AbilitySourceContractV4 },
    Published { triggered_at: usize, item: StackItemId, stack_sha256: [u8; 32] },
    SacrificeCompleted { triggered_at: usize, item: StackItemId, sacrifice_at: usize, departure_at: usize, destination: Zone, departed_generation: u32 },
    Skipped { triggered_at: usize, item: StackItemId },
}
fn journal(state: &GameState, at: usize) -> Option<JournalV1> {
    let CommittedEvent::EternalOwnedDrawV1 { journal } = state.engine.event_history.get(at)? else { return None; };
    let DrawJournalV1::StandstillV1(row) = **journal else { return None; };
    Some(row)
}
fn rows(state: &GameState) -> impl Iterator<Item = (usize, JournalV1)> + '_ {
    state.engine.event_history.iter().enumerate().filter_map(|(at, _)| journal(state, at).map(|row| (at, row)))
}
fn note(state: &mut GameState, row: JournalV1) -> usize {
    let at = state.engine.event_history.len();
    record(state, DrawJournalV1::StandstillV1(row));
    at
}
fn digest(item: &StackItem) -> Option<[u8; 32]> {
    Some(Sha256::digest(serde_json::to_vec(item).ok()?).into())
}
fn op(triggered_at: usize, caster: PlayerId) -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::P1bV1(P1bOpV1::StandstillBoundV1 { triggered_at, caster })))
}
pub(crate) fn bound(effect: &EffectOp) -> Option<(usize, PlayerId)> {
    match effect {
        EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::P1bV1(P1bOpV1::StandstillBoundV1 { triggered_at, caster }))) => Some((*triggered_at, *caster)),
        _ => None,
    }
}
fn physical(state: &GameState, source: AbilitySourceContractV4) -> bool {
    state.objects.try_get(source.source).is_some_and(|live| {
        source == AbilitySourceContractV4::capture(state, source.source)
            && source.zone == Zone::Battlefield && !live.v4.is_token && live.spell_copy_origin.is_none()
            && p1b::active_named(state, source.source, "Standstill")
            && !state.eternal_v1.iter().flat_map(|e| &e.vintage.copies).any(|copy| copy.host.object == source.source && copy.host.zone_change_count == source.zone_change_count)
    })
}
fn cast_identity(state: &GameState, cast_at: usize) -> Option<crate::cast_identity_v1::ReceiptV1> {
    let CommittedEvent::PublishedCastIdentityV1 { receipt } = state.engine.event_history.get(cast_at.checked_sub(1)?)? else { return None; };
    (usize::try_from(receipt.spell_cast_history_index).ok()? == cast_at
        && matches!(state.engine.event_history.get(cast_at), Some(CommittedEvent::SpellCast { spell, controller }) if *spell == receipt.spell && *controller == receipt.controller)
        && receipt.origin_zone_change_count.checked_add(1) == Some(receipt.stack_zone_change_count))
        .then_some(*receipt)
}
/// Called only while collecting the native SpellCast event. Ordinary finalize
/// captures before priority; resolution_cast_v1::finish_child captures before
/// resuming its parent. Trigger resolution later uses the saved receipt and
/// does not require this spell to remain live. A bare spell ID cannot select
/// a prior cast of that physical card or another stack row.
fn actual_cast(state: &GameState, spell: ObjectId, caster: PlayerId) -> Option<usize> {
    let mut live = state.stack.iter().filter(|row| row.kind == StackItemKind::Spell && row.source == spell);
    let item = live.next()?;
    if live.next().is_some() || item.is_copy || item.controller != caster { return None; }
    let source = item.v4.source_contract?;
    let origin = source.spell_cast_origin?;
    if source.source != spell || source.controller != caster || source.zone != Zone::Stack
        || source.spell_copy_origin.is_some() || origin.finalized_method != Some(source.cast_method)
        || crate::state::StackSourceContractV4::capture(state, spell, source.cast_method) != source
    { return None; }
    let mut matches = state.engine.event_history.iter().enumerate().filter_map(|(at, event)| {
        let CommittedEvent::PublishedCastIdentityV1 { receipt } = event else { return None; };
        let cast_at = at.checked_add(1)?;
        (receipt.stack_item_id == item.v4.stack_item_id && receipt.spell == spell
            && receipt.controller == caster && receipt.owner == source.owner
            && receipt.stack_zone_change_count == source.zone_change_count
            && receipt.origin_zone == origin.origin_zone && receipt.origin_zone_change_count == origin.origin_zone_change_count
            && cast_identity(state, cast_at) == Some(*receipt)).then_some(cast_at)
    });
    let at = matches.next()?;
    matches.next().is_none().then_some(at)
}
pub(crate) fn capture(state: &mut GameState, source: AbilitySourceContractV4, spell: ObjectId, caster: PlayerId) -> Result<Option<EffectOp>, String> {
    if !physical(state, source) { return Ok(None); }
    let cast_at = actual_cast(state, spell, caster).ok_or("Standstill trigger lacks its exact native paid cast occurrence")?;
    if rows(state).any(|(_, row)| matches!(row, JournalV1::Triggered { cast_at: previous, source: original, .. } if previous == cast_at && original.source == source.source && original.zone_change_count == source.zone_change_count)) {
        return Err("Standstill cannot recollect the same source and cast occurrence".into());
    }
    let cast = cast_identity(state, cast_at).ok_or("Standstill lost its paid cast identity")?;
    let triggered_at = note(state, JournalV1::Triggered { cast_at, cast, source });
    Ok(Some(op(triggered_at, caster)))
}
fn trigger(state: &GameState, triggered_at: usize) -> Option<(AbilitySourceContractV4, PlayerId)> {
    let JournalV1::Triggered { cast_at, cast, source } = journal(state, triggered_at)? else { return None; };
    (cast_identity(state, cast_at) == Some(cast) && cast_at < triggered_at && source.zone == Zone::Battlefield
        && CARD_DEFS.get(usize::from(source.card_def))?.object_name == "Standstill"
        && rows(state).filter(|(_, row)| matches!(row, JournalV1::Triggered { cast_at: previous, source: original, .. } if *previous == cast_at && original.source == source.source && original.zone_change_count == source.zone_change_count)).count() == 1)
        .then_some((source, cast.controller))
}
fn matches_trigger(state: &GameState, item: &StackItem, triggered_at: usize, caster: PlayerId) -> bool {
    trigger(state, triggered_at).is_some_and(|(source, original_caster)| {
        caster == original_caster && *item == StackItem {
            kind: StackItemKind::TriggeredAbility,
            source: source.source,
            controller: source.controller,
            targets: Vec::new(),
            is_copy: false,
            inline_effect: Some(op(triggered_at, caster)),
            discarded: Vec::new(),
            is_flashback: false,
            mode_chosen: 0,
            madness_offer: false,
            kicked: false,
            v4: StackStateV4 {
                stack_item_id: item.v4.stack_item_id,
                ability_source_contract: Some(source),
                ..StackStateV4::default()
            },
        }
    })
}

fn published(state: &GameState, item: &StackItem, triggered_at: usize) -> Option<usize> {
    let mut matches = rows(state).filter_map(|(at, row)| match row {
        JournalV1::Published { triggered_at: original, item: original_item, stack_sha256 } if original == triggered_at => Some((at, original_item, stack_sha256)),
        _ => None,
    });
    let (at, original_item, stack_sha256) = matches.next()?;
    (matches.next().is_none() && triggered_at < at && original_item == item.v4.stack_item_id && digest(item) == Some(stack_sha256)).then_some(at)
}
pub(crate) fn validate_trigger(state: &GameState, item: &StackItem, require_publication: bool) -> Result<(), String> {
    let (at, caster) = item.inline_effect.as_ref().and_then(bound).ok_or("Standstill lost its bound trigger recipe")?;
    if !matches_trigger(state, item, at, caster) || (require_publication && published(state, item, at).is_none()) {
        return Err("Standstill lost its original cast, source, or native publication".into());
    }
    Ok(())
}
pub(crate) fn publish(state: &mut GameState, id: StackItemId) -> Result<(), String> {
    let Some(item) = state.stack.iter().find(|row| row.v4.stack_item_id == id) else { return Err("Standstill lost its published row".into()); };
    let Some((triggered_at, caster)) = item.inline_effect.as_ref().and_then(bound) else { return Ok(()); };
    if !matches_trigger(state, item, triggered_at, caster)
        || rows(state).any(|(_, row)| matches!(row, JournalV1::Published { triggered_at: original, .. } if original == triggered_at))
    { return Err("Standstill changed or republished its original trigger".into()); }
    let stack_sha256 = digest(item).ok_or("Standstill could not bind its published row")?;
    note(state, JournalV1::Published { triggered_at, item: id, stack_sha256 });
    Ok(())
}
fn resolved(state: &GameState, triggered_at: usize) -> bool {
    rows(state).any(|(_, row)| matches!(row, JournalV1::SacrificeCompleted { triggered_at: original, .. } | JournalV1::Skipped { triggered_at: original, .. } if original == triggered_at))
}
fn sacrifice_proof(state: &GameState, item: &StackItem, triggered_at: usize) -> bool {
    let Some((source, _)) = trigger(state, triggered_at) else { return false; };
    let Some(published_at) = published(state, item, triggered_at) else { return false; };
    let mut matches = rows(state).filter_map(|(at, row)| match row {
        JournalV1::SacrificeCompleted { triggered_at: original, item, sacrifice_at, departure_at, destination, departed_generation } if original == triggered_at => Some((at, item, sacrifice_at, departure_at, destination, departed_generation)),
        _ => None,
    });
    let Some((at, original_item, sacrifice_at, departure_at, destination, departed_generation)) = matches.next() else { return false; };
    matches.next().is_none() && original_item == item.v4.stack_item_id
        && published_at < sacrifice_at && sacrifice_at < departure_at && departure_at < at
        && destination != Zone::Battlefield && source.zone_change_count.checked_add(1) == Some(departed_generation)
        && matches!(state.engine.event_history.get(sacrifice_at), Some(CommittedEvent::Sacrificed { object, controller_before, .. }) if *object == source.source && *controller_before == source.controller)
        && matches!(state.engine.event_history.get(departure_at), Some(CommittedEvent::ZoneChange { object, from: Zone::Battlefield, to, controller_before }) if *object == source.source && *to == destination && *controller_before == source.controller)
        && !rows(state).any(|(_, row)| matches!(row, JournalV1::Skipped { triggered_at: original, .. } if original == triggered_at))
}
pub(super) fn selected(state: &GameState, pending: &EffectContinuation, triggered_at: usize, drawer: PlayerId) -> bool {
    let item = &pending.resolving_item;
    let Some((source, caster)) = trigger(state, triggered_at) else { return false; };
    drawer == caster.opponent() && matches_trigger(state, item, triggered_at, caster)
        && published(state, item, triggered_at).is_some()
        && pending.ctx.source == item.source && pending.ctx.controller == item.controller
        && pending.ctx.stack_item_id == Some(item.v4.stack_item_id)
        && pending.ctx.targets == item.targets && pending.ctx.target_contracts == item.v4.target_contracts
        && pending.ctx.paid_cost_refs == item.v4.paid_cost_refs && pending.ctx.ability_source_contract == Some(source)
        && (!has_started(state, item.v4.stack_item_id) || sacrifice_proof(state, item, triggered_at))
}
/// The successful sacrifice is a native event plus departure, even when a
/// replacement sends the card to exile. Other stacked triggers then do nothing.
pub(super) fn sacrifice_once(state: &mut GameState, pending: &EffectContinuation, triggered_at: usize) -> Result<bool, String> {
    let item = &pending.resolving_item;
    let (source, caster) = trigger(state, triggered_at).ok_or("Standstill lost its trigger ancestry")?;
    if !selected(state, pending, triggered_at, caster.opponent()) || resolved(state, triggered_at)
        || state.stack.iter().any(|row| row.v4.stack_item_id == item.v4.stack_item_id) {
        return Err("Standstill cannot replay its sacrifice or skip source proof".into());
    }
    let can_sacrifice = state.objects.try_get(source.source).is_some_and(|live| live.zone == Zone::Battlefield && live.zone_change_count == source.zone_change_count && live.controller == source.controller);
    if !can_sacrifice {
        note(state, JournalV1::Skipped { triggered_at, item: item.v4.stack_item_id });
        return Ok(false);
    }
    let sacrifice_at = state.engine.event_history.len();
    event::log_sacrifice(state, source.source);
    event::propose_and_commit(state, ProposedEvent::zone_change(source.source, Zone::Graveyard));
    if state.engine.halted.is_some() { return Err("Standstill sacrifice was halted".into()); }
    let live = state.objects.get(source.source);
    if live.zone == Zone::Battlefield || live.zone_change_count != source.zone_change_count.checked_add(1).ok_or("Standstill source generation overflow")? {
        note(state, JournalV1::Skipped { triggered_at, item: item.v4.stack_item_id });
        return Ok(false);
    }
    let destination = live.zone;
    let departed_generation = live.zone_change_count;
    let departure_at = state.engine.event_history.iter().enumerate().skip(sacrifice_at + 1).find_map(|(at, event)| matches!(event, CommittedEvent::ZoneChange { object, from: Zone::Battlefield, to, controller_before } if *object == source.source && *to == destination && *controller_before == source.controller).then_some(at)).ok_or("Standstill sacrifice lacks its committed source departure")?;
    note(state, JournalV1::SacrificeCompleted { triggered_at, item: item.v4.stack_item_id, sacrifice_at, departure_at, destination, departed_generation });
    if !sacrifice_proof(state, item, triggered_at) { return Err("Standstill sacrifice lost its native event proof".into()); }
    Ok(true)
}
pub(crate) fn refuse_legacy(state: &GameState) -> Result<(), String> {
    if state.stack.iter().any(|item| item.inline_effect.as_ref().and_then(bound).is_some())
        || state.engine.pending_triggers.iter().any(|pending| bound(&pending.effect).is_some())
        || state.engine.pending_effect.as_ref().is_some_and(|pending| pending.resolving_item.inline_effect.as_ref().and_then(bound).is_some())
    { return Err("Standstill bound draws require an observation successor".into()); }
    Ok(())
}
