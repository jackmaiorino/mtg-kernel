//! Recover the count at entry to a staged draw from its generating effect and committed events.
use super::*;

/// Mark the beginning of the generating batch. Its zone changes, including
/// replacements to exile, determine the count independently of the draw frame.
pub(crate) fn begin_draw_input_batch(
    state: &mut GameState,
    pending: &EffectContinuation,
    path: &[u16],
    player: PlayerId,
    inputs: impl Iterator<Item = ObjectId> + Clone,
) {
    if !crate::eternal_cards_v1::owned_draws_v1::staged::tracks(state, pending, player) {
        return;
    }
    if crate::eternal_legacy_e5b_v1::next_draw_may_offer_miracle(state, player)
        || crate::eternal_vintage_e7b_v1::dredge::may_need_draw_choice(state, player)
        || inputs.clone().any(|id| {
            let card = state.objects.get(id);
            card.owner == player
                && crate::eternal_vintage_e7b_v1::dredge::dredge_count(
                    crate::card_def::CARD_DEFS[usize::from(card.card_def)].name,
                )
                .is_some()
        })
    {
        use crate::eternal_cards_v1::owned_draws_v1::{record, JournalV1};
        record(
            state,
            JournalV1::DrawInputBatchV1 {
                item: pending.resolving_item.v4.stack_item_id,
                path: path.to_vec(),
                objects: inputs.collect(),
            },
        );
    }
}

pub(super) fn staged_batch_count(
    state: &GameState,
    ctx: &ExecCtx,
    path: &[u16],
    end: usize,
    zone: Zone,
    owner: Option<PlayerId>,
    maximum: u32,
) -> Option<u32> {
    use crate::eternal_cards_v1::owned_draws_v1::JournalV1;
    use event::CommittedEvent;
    let history = state.engine.event_history.get(..end)?;
    let batch = history.iter().enumerate().rev().find_map(|(at, row)| {
        let CommittedEvent::EternalOwnedDrawV1 { journal } = row else {
            return None;
        };
        let JournalV1::DrawInputBatchV1 {
            item,
            path: recorded,
            objects,
        } = journal.as_ref()
        else {
            return None;
        };
        (Some(*item) == ctx.stack_item_id && recorded == path).then_some((at, objects))
    });
    let moved = |row: &CommittedEvent| match row {
        CommittedEvent::ZoneChange {
            object, from, to, ..
        } if *from == zone
            && from != to
            && owner.is_none_or(|player| {
                state
                    .objects
                    .try_get(*object)
                    .is_some_and(|card| card.owner == player)
            }) =>
        {
            Some(*object)
        }
        _ => None,
    };
    // Unmarked saves used the immediately preceding batch of departures.
    let count = match batch {
        Some((at, objects)) => history[at + 1..]
            .iter()
            .filter_map(moved)
            .filter(|object| objects.contains(object))
            .count(),
        None => history
            .iter()
            .rev()
            .filter(|row| {
                matches!(
                    row,
                    CommittedEvent::ZoneChange { .. }
                        | CommittedEvent::Draw { .. }
                        | CommittedEvent::SpellCast { .. }
                )
            })
            .take_while(|row| moved(row).is_some())
            .count(),
    };
    u32::try_from(count).ok().filter(|count| *count <= maximum)
}

pub(super) fn staged_dynamic_count(
    state: &GameState,
    ctx: &ExecCtx,
    value: crate::card_def::DynamicValueDef,
    start: usize,
) -> Option<u32> {
    use crate::card_def::DynamicValueDef::*;
    match value {
        GraveyardCardsNamedV1(id)
            if crate::card_def::CARD_DEFS.get(usize::from(id))?.name == "Accumulated Knowledge" =>
        {
            crate::eternal_cards_v1::owned_draws_v1::knowledge::graveyard_count_at(state, start)
        }
        Fixed(_)
        | ControlledPermanentsWithSubtype(_)
        | DistinctManaValuesAmongControlledNonlandPermanents => {
            Some(crate::engine::evaluate_dynamic_value(state, value, ctx.controller).max(0) as u32)
        }
        _ => None,
    }
}

#[cfg(feature = "eternal-fixtures")]
pub(super) fn draw_total_at(
    state: &GameState,
    player: PlayerId,
    base: u32,
    start: usize,
) -> Option<u32> {
    let hand = crate::eternal_cards_v1::owned_draws_v1::initial_hand_len(state, player, start)?;
    base.checked_add(event::draw_instruction_extra_at_hand_size(
        state, player, base, hand,
    ))
}

/// Validate both an open replacement choice and its saved answer before the
/// interpreter spends that attempt. Native instructions validate their own cursor.
#[cfg(feature = "eternal-fixtures")]
pub(super) fn validate_staged_draw_choice(
    state: &GameState,
    pending: &EffectContinuation,
) -> Result<(), String> {
    use crate::eternal_cards_v1::e3::E3ChoiceKindV1;
    use crate::eternal_vintage_e7b_v1::E7bChoiceV1;
    if state.eternal_owned_draw_progress_v1.is_some() {
        return Ok(());
    }
    let (choice, frames) = match (&pending.choice, pending.frames.split_last()) {
        (
            Some(PendingEffectChoice::SelectTargets {
                purpose: EffectTargetSelectionPurpose::EternalV1 { choice },
                ..
            }),
            _,
        ) => (choice, pending.frames.as_slice()),
        (None, Some((EffectFrame::EternalSelectionV1 { choice, .. }, rest))) => (choice, rest),
        _ => return Ok(()),
    };
    if !matches!(
        choice.kind,
        E3ChoiceKindV1::E7bV1(E7bChoiceV1::DredgeV1 | E7bChoiceV1::JaceOrHullbreacherV1)
    ) {
        return Ok(());
    }
    if !crate::eternal_cards_v1::owned_draws_v1::staged::tracks(state, pending, choice.player) {
        return Ok(());
    }
    let Ok(root) = validated_definition_owned_root_effect(state, pending) else {
        return Ok(());
    };
    let (first, spent) =
        crate::eternal_cards_v1::owned_draws_v1::staged::attempts(state, pending, &choice.path);
    crate::eternal_cards_v1::owned_draws_v1::staged::check_legacy_boundary(
        state, pending, spent, false,
    )?;
    let start = first.unwrap_or(state.engine.event_history.len());
    let Some((player, base, mut expected)) = draw_site(state, pending, &root, &choice.path, start)
    else {
        return if first.is_some()
            || crate::eternal_cards_v1::owned_draws_v1::staged::generated_path(state, pending)
                .is_some()
        {
            Err("replacement choice lost its draw site".into())
        } else {
            Ok(())
        };
    };
    let drawer = pending.ctx.resolve_player(player, state);
    let after = draw_total_at(state, drawer, base, start)
        .and_then(|total| total.checked_sub(spent)?.checked_sub(1))
        .ok_or("staged draw has no outstanding attempt")?;
    if after > 0 {
        expected.push(EffectFrame::Program {
            op: EffectOp::DrawCards {
                player,
                count: after,
            },
            path: choice.path.clone(),
        });
    }
    if drawer != choice.player || frames != expected || choice.remaining != expected {
        return Err("replacement choice changed its remaining draw count".into());
    }
    Ok(())
}
