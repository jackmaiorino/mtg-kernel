//! Show and Tell's paid resolution owns the private choices and simultaneous entry.
use super::{paid_cast_index, AbilitySourceContractV4, GameState, JournalV1, MetamorphCopyV1, NoncastCopyChoiceV1, StackItemId, Zone};
use crate::effect::EffectContinuation;
use crate::ids::PlayerId;
use crate::state::{StackItem, StackItemKind, StackSourceContractV4};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChoiceV1 {
    pub(crate) producer: AbilitySourceContractV4,
    pub(crate) player: PlayerId,
    pub(crate) physical: Option<AbilitySourceContractV4>,
    pub(crate) candidates: Vec<AbilitySourceContractV4>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CopyOriginV1 {
    pub(super) producer: AbilitySourceContractV4,
    pub(super) chosen_at: usize,
}

pub(crate) fn printed(item: &StackItem) -> bool {
    cfg!(feature = "eternal-fixtures")
        && item.kind == StackItemKind::Spell
        && !item.is_copy
        && item.v4.source_contract.is_some_and(|source| {
            source.zone == Zone::Stack
                && super::card_id_by_name("Show and Tell") == Some(source.card_def)
        })
}

pub(crate) fn capture_paid(
    state: &GameState,
    pending: &EffectContinuation,
    original: &StackItem,
) -> Result<(AbilitySourceContractV4, usize), String> {
    let item = &pending.resolving_item;
    let source = item.v4.source_contract.ok_or("Show and Tell lost its cast source")?;
    let method = item.v4.cast_method.ok_or("Show and Tell lost its finalized cast method")?;
    let ctx = &pending.ctx;
    if !printed(item)
        || item != original
        || item.inline_effect.is_some()
        || !item.targets.is_empty()
        || !item.v4.target_contracts.is_empty()
        || !pending.frames.is_empty()
        || ctx.stack_item_id != Some(item.v4.stack_item_id)
        || ctx.source != item.source
        || ctx.source != source.source
        || ctx.controller != item.controller
        || ctx.controller != source.controller
        || !ctx.targets.is_empty()
        || !ctx.target_contracts.is_empty()
        || state.stack.iter().any(|row| row.v4.stack_item_id == item.v4.stack_item_id)
        || state.objects.try_get(item.source).is_none_or(|live| {
            live.v4.is_token
                || live.v4.face_down_v1.is_some()
                || live.v4.face_index != 0
                || live.spell_copy_origin.is_some()
        })
        || StackSourceContractV4::capture(state, item.source, method) != source
        || source.spell_cast_origin.is_none_or(|origin| {
            origin.finalized_method != Some(method)
        })
    {
        return Err("Show and Tell lost its actual paid resolution".into());
    }
    let producer = AbilitySourceContractV4::capture(state, item.source);
    let cast_at = paid_cast_index(state, item.v4.stack_item_id)?;
    if !super::cast_receipt_matches(state, cast_at, item.v4.stack_item_id, producer) {
        return Err("Show and Tell lost its original cast publication".into());
    }
    Ok((producer, cast_at))
}

pub(crate) fn copy_choice(
    state: &GameState,
    producer: AbilitySourceContractV4,
    physical: AbilitySourceContractV4,
) -> Result<Option<ChoiceV1>, String> {
    let candidates = NoncastCopyChoiceV1::candidates(state, physical.source, physical.owner, super::NoncastProducer::ShowAndTell)?;
    Ok((!candidates.is_empty()).then_some(ChoiceV1 {
        producer,
        player: physical.owner,
        physical: Some(physical),
        candidates,
    }))
}

pub(crate) fn prospective_copy(
    state: &GameState,
    producer: AbilitySourceContractV4,
    cast_at: usize,
    stack_item: StackItemId,
    physical: AbilitySourceContractV4,
    copied: AbilitySourceContractV4,
) -> MetamorphCopyV1 {
    MetamorphCopyV1 {
        physical,
        copied,
        cast_at: Some(cast_at),
        stack_item,
        reanimated_by: None,
        vial: None,
        show_and_tell: Some(CopyOriginV1 {
            producer,
            chosen_at: state.engine.event_history.len(),
        }),
    }
}

pub(super) fn entry_origin_matches(
    state: &GameState,
    copy: MetamorphCopyV1,
    origin: CopyOriginV1,
    entry: usize,
) -> bool {
    let history = &state.engine.event_history;
    copy.cast_at.is_some_and(|cast| cast < origin.chosen_at)
        && origin.chosen_at < entry
        && copy.physical.zone == Zone::Hand
        && matches!(history.get(origin.chosen_at),
            Some(crate::event::CommittedEvent::MeddlingMageEntryV1 { journal })
            if matches!(journal.as_ref(), JournalV1::ShowAndTellMetamorphChosen { copy: actual }
                if *actual == copy))
        && !history[origin.chosen_at + 1..entry].iter().any(|event| {
            matches!(event, crate::event::CommittedEvent::ZoneChange { object, .. }
                if *object == copy.copied.source)
        })
}
