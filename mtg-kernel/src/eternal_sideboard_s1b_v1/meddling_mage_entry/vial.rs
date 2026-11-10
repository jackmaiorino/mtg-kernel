use super::{
    card_id_by_name, journal, record, AbilitySourceContractV4, GameState, JournalV1,
    MetamorphCopyV1, NoncastCopyChoiceV1, ObjectId, StackItemId, Zone,
};
use crate::effect::EffectContinuation;
use crate::eternal_legacy_e8a_v1::{self as e8a, AETHER_VIAL};
use crate::event::CommittedEvent;
use crate::state::{StackItem, StackItemKind, VialMetamorphPaidV1};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CopyOriginV1 {
    pub(super) source: AbilitySourceContractV4,
    paid_at: usize,
    pub(super) chosen_at: usize,
}

fn is_vial(source: AbilitySourceContractV4) -> bool {
    cfg!(feature = "eternal-fixtures")
        && source.zone == Zone::Battlefield
        && card_id_by_name(AETHER_VIAL) == Some(source.card_def)
}

fn printed_source(item: &StackItem) -> Option<AbilitySourceContractV4> {
    let source = item.v4.ability_source_contract?;
    (is_vial(source)
        && item.kind == StackItemKind::ActivatedAbility
        && item.source == source.source
        && item.controller == source.controller
        && item.v4.activated_ability_index == Some(0)
        && item.v4.granted_by.is_none()
        && item.v4.cauldron_grant.0.is_none()
        && !item.is_copy
        && item.targets.is_empty()
        && item.v4.target_contracts.is_empty()
        && item.inline_effect.as_ref() == Some(&e8a::aether_vial_put()))
    .then_some(source)
}

pub(crate) fn record_paid(
    state: &mut GameState,
    item: StackItemId,
    tap_at: usize,
) -> Result<(), String> {
    if !crate::eternal_opening_v1::enabled(state) {
        return Ok(());
    }
    let Some(source) = state
        .stack
        .last()
        .filter(|row| row.v4.stack_item_id == item)
        .and_then(printed_source)
    else {
        return Ok(());
    };
    if state.objects.try_get(source.source).is_none_or(|object| {
        object.v4.is_token || object.v4.face_down_v1.is_some() || object.v4.face_index != 0
    }) || crate::eternal_vintage_v1::active_copy(state, source.source)
        || crate::standard_cards_v1::had_card_def(
            state,
            source.source,
            source.zone_change_count,
            source.card_def,
        )
    {
        return Ok(());
    }
    let history = &state.engine.event_history;
    if !matches!(history.get(tap_at), Some(CommittedEvent::Tap { object }) if *object == source.source)
        || tap_at.checked_add(1) != Some(history.len())
    {
        return Err("Aether Vial has no original paid tap".into());
    }
    let paid_at = history.len();
    record(
        state,
        JournalV1::VialPaid {
            source,
            item,
            tap_at,
        },
    );
    state.stack.last_mut().unwrap().v4.vial_metamorph_paid_v1 = VialMetamorphPaidV1(Some(paid_at));
    Ok(())
}

pub(super) fn paid_matches(
    state: &GameState,
    paid_at: usize,
    source: AbilitySourceContractV4,
    item: StackItemId,
) -> bool {
    let Some(JournalV1::VialPaid {
        source: paid_source,
        item: paid_item,
        tap_at,
    }) = journal(state, paid_at)
    else {
        return false;
    };
    is_vial(source)
        && *paid_source == source
        && *paid_item == item
        && tap_at.checked_add(1) == Some(paid_at)
        && matches!(state.engine.event_history.get(*tap_at),
            Some(CommittedEvent::Tap { object }) if *object == source.source)
}

pub(super) fn validate_live_paid(state: &GameState) -> Result<(), String> {
    for row in &state.stack {
        let Some(at) = row.v4.vial_metamorph_paid_v1.0 else {
            continue;
        };
        if !crate::eternal_opening_v1::enabled(state)
            || printed_source(row)
                .is_none_or(|source| !paid_matches(state, at, source, row.v4.stack_item_id))
        {
            return Err("Aether Vial stack row lost its exact paid tap".into());
        }
    }
    Ok(())
}

pub(super) fn live_item_matches(
    state: &GameState,
    at: usize,
    item: StackItemId,
    source: AbilitySourceContractV4,
) -> bool {
    state
        .stack
        .iter()
        .find(|row| row.v4.stack_item_id == item)
        .is_none_or(|row| {
            printed_source(row) == Some(source) && row.v4.vial_metamorph_paid_v1.0 == Some(at)
        })
}

pub(crate) fn waiting(state: &GameState) -> bool {
    use crate::effect::{EffectFrame, EffectTargetSelectionPurpose, PendingEffectChoice};
    use crate::eternal_cards_v1::e3::E3ChoiceKindV1;

    let vial_choice = |kind: &E3ChoiceKindV1| {
        matches!(
            kind,
            E3ChoiceKindV1::E8aV1(e8a::E8aChoiceV1::VialHandToBattlefieldV1 { .. })
        )
    };
    state.engine.pending_effect.as_ref().is_some_and(|pending| {
        matches!(&pending.choice, Some(PendingEffectChoice::SelectTargets {
            purpose: EffectTargetSelectionPurpose::EternalV1 { choice },
            ..
        }) if vial_choice(&choice.kind))
            || matches!(pending.frames.last(), Some(EffectFrame::EternalSelectionV1 {
                choice, ..
            }) if vial_choice(&choice.kind))
    })
}

pub(crate) fn paid_at(
    state: &GameState,
    pending: &EffectContinuation,
) -> Result<Option<usize>, String> {
    let row = &pending.resolving_item;
    let Some(at) = row.v4.vial_metamorph_paid_v1.0 else {
        return Ok(None);
    };
    let source = printed_source(row).ok_or("Aether Vial changed its paid resolving source")?;
    let item = row.v4.stack_item_id;
    if !crate::eternal_opening_v1::enabled(state)
        || !paid_matches(state, at, source, item)
        || pending.ctx.ability_source_contract != Some(source)
        || pending.ctx.stack_item_id != Some(item)
        || pending.ctx.source != source.source
        || pending.ctx.controller != source.controller
    {
        return Err("Aether Vial changed its paid resolving source".into());
    }
    Ok(Some(at))
}

pub(crate) fn validate_hand_origin(
    state: &GameState,
    pending: &EffectContinuation,
    at: usize,
) -> Result<AbilitySourceContractV4, String> {
    let Some(source) = printed_source(&pending.resolving_item) else {
        return Err("Aether Vial hand choice lost its original activation".into());
    };
    if paid_at(state, pending)? != Some(at)
        || e8a::statics::vial_charge(state, source) != 4
        || !pending.frames.is_empty()
    {
        return Err("Aether Vial hand choice changed its original charge or continuation".into());
    }
    Ok(source)
}

pub(crate) fn capture(
    state: &GameState,
    pending: &EffectContinuation,
    at: usize,
    object: ObjectId,
) -> Result<Option<NoncastCopyChoiceV1>, String> {
    let source = validate_hand_origin(state, pending, at)?;
    let physical = AbilitySourceContractV4::capture(state, object);
    if physical.zone != Zone::Hand
        || physical.owner != source.controller
        || !is_physical_metamorph_in_hand(state, object)
    {
        return Err("Aether Vial copy choice changed its selected hand incarnation".into());
    }
    let candidates = NoncastCopyChoiceV1::candidates(state, object, source.controller, true)?;
    if candidates.is_empty() {
        return Ok(None);
    }
    Ok(Some(NoncastCopyChoiceV1 {
        physical,
        producer: source,
        candidates,
        cast_at: None,
        stack_item: pending.resolving_item.v4.stack_item_id,
        vial: Some(CopyOriginV1 {
            source,
            paid_at: at,
            chosen_at: state.engine.event_history.len(),
        }),
    }))
}

pub(crate) fn is_physical_metamorph_in_hand(state: &GameState, object: ObjectId) -> bool {
    state.objects.try_get(object).is_some_and(|live| {
        live.zone == Zone::Hand
            && !live.v4.is_token
            && live.v4.face_down_v1.is_none()
            && live.v4.face_index == 0
            && live.spell_copy_origin.is_none()
            && card_id_by_name("Phyrexian Metamorph") == Some(live.card_def)
    })
}

pub(super) fn entry_origin_matches(
    state: &GameState,
    copy: MetamorphCopyV1,
    origin: CopyOriginV1,
) -> bool {
    crate::eternal_opening_v1::enabled(state)
        && origin.paid_at < origin.chosen_at
        && paid_matches(state, origin.paid_at, origin.source, copy.stack_item)
        && copy.physical.zone == Zone::Hand
        && copy.physical.owner == origin.source.controller
        && card_id_by_name("Phyrexian Metamorph") == Some(copy.physical.card_def)
        && matches!(journal(state, origin.chosen_at),
            Some(JournalV1::VialMetamorphChosen { paid_at, physical })
            if *paid_at == origin.paid_at && *physical == copy.physical)
}

pub(crate) fn record_hand_answer(
    state: &mut GameState,
    copy: MetamorphCopyV1,
) -> Result<(), String> {
    let Some(origin) = copy.vial else {
        return Err("Aether Vial copy has no original hand answer".into());
    };
    if origin.chosen_at != state.engine.event_history.len() {
        return Err("Aether Vial copy changed its original hand answer position".into());
    }
    record(
        state,
        JournalV1::VialMetamorphChosen {
            paid_at: origin.paid_at,
            physical: copy.physical,
        },
    );
    Ok(())
}
