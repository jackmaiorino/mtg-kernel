//! The optional sacrifice is not a target, and its consequence is cost-bound.
use crate::effect::{
    EffectContinuation, EffectObjectBinding, EffectOp, ObjectSelectionActionV1,
    ObjectSelectionFilterV1, ObjectSelectionRuleV1, PlayerRef,
};
use crate::engine::{EffectDuration, UntilEndOfTurnEffect};
use crate::event::CommittedEvent;
use crate::ids::ObjectId;
use crate::state::{AbilitySourceContractV4, GameState, Zone};

pub(crate) fn program() -> EffectOp {
    EffectOp::SelectObjectsV1 {
        rule: ObjectSelectionRuleV1 {
            player: PlayerRef::Controller,
            zone: Zone::Battlefield,
            any_player: false,
            filter: ObjectSelectionFilterV1::AnotherCreatureFromCapturedSourceV1,
            min: 0,
            max: 1,
            action: ObjectSelectionActionV1::SacrificeThenDrawAndMakeCapturedSourceUnblockableV1,
        },
    }
}
pub(crate) fn validate_source(
    state: &GameState,
    card_def: u16,
    source: Option<AbilitySourceContractV4>,
) -> Result<(), String> {
    if !crate::card_def::CARD_DEFS
        .get(usize::from(card_def))
        .is_some_and(|d| d.name == "Vampire Gourmand" && d.is_executable())
    {
        return Ok(());
    }
    let source = source.ok_or("Gourmand attack lost its captured source")?;
    let latest = state
        .engine
        .event_history
        .iter()
        .rev()
        .find_map(|e| match e {
            CommittedEvent::DeclaredAttacker {
                source: id,
                source_zone_change_count,
                controller,
            } if *id == source.source && *source_zone_change_count == source.zone_change_count => {
                Some(*controller)
            }
            _ => None,
        });
    if source.card_def != card_def
        || source.zone != Zone::Battlefield
        || latest != Some(source.controller)
    {
        return Err("Gourmand lost its latest committed exact attack declaration".into());
    }
    Ok(())
}
pub(crate) fn cant_be_blocked(state: &GameState, object: ObjectId) -> bool {
    let Some(live) = state.objects.try_get(object) else {
        return false;
    };
    live.zone == Zone::Battlefield
        && state.engine.until_end_of_turn.iter().any(|e| {
            matches!(e,
        UntilEndOfTurnEffect::ResolvedObjectCantBeBlockedV1 { object_id, object_zone_change_count,
            duration: EffectDuration::EndOfTurn, .. }
        if *object_id == object && *object_zone_change_count == live.zone_change_count)
        })
}
pub(crate) fn pay_and_apply(
    state: &mut GameState,
    pending: &EffectContinuation,
    selected: &[EffectObjectBinding],
) -> Result<(), String> {
    let source = pending
        .ctx
        .ability_source_contract
        .ok_or("Gourmand sacrifice has no captured source")?;
    if crate::card_def::CARD_DEFS[usize::from(source.card_def)].name != "Vampire Gourmand" {
        return Err("Gourmand consequence belongs to another definition".into());
    }
    validate_source(state, source.card_def, Some(source))?;
    if pending.resolving_item.inline_effect.as_ref() != Some(&program())
        || pending.resolving_item.kind != crate::state::StackItemKind::TriggeredAbility
        || pending.ctx.controller != source.controller
        || selected.len() > 1
    {
        return Err("Gourmand sacrifice consequence has the wrong originating ability".into());
    }
    let Some(chosen) = selected.first().copied() else {
        return Ok(());
    };
    let live = state.objects.get(chosen.object);
    if live.zone != Zone::Battlefield
        || chosen.expected_zone != Zone::Battlefield
        || live.zone_change_count != chosen.expected_zone_change_count
        || live.controller != source.controller
        || (chosen.object == source.source && live.zone_change_count == source.zone_change_count)
        || !crate::engine::object_has_type(
            state,
            chosen.object,
            crate::card_def::CardType::Creature,
        )
    {
        return Err("Gourmand sacrifice is not another exact controlled creature".into());
    }
    crate::event::commit_sacrifice_death_group_v1(state, &[chosen.object]);
    // A replaced move to exile still pays the sacrifice. A prevented move
    // which leaves the exact battlefield incarnation cannot pay it.
    let after = state.objects.get(chosen.object);
    if after.zone == Zone::Battlefield
        && after.zone_change_count == chosen.expected_zone_change_count
    {
        return Ok(());
    }
    crate::effect::execute(
        &EffectOp::DrawCards {
            player: PlayerRef::Controller,
            count: 1,
        },
        &pending.ctx,
        state,
    );
    install(state, source);
    Ok(())
}

pub(crate) fn install(state: &mut GameState, source: AbilitySourceContractV4) {
    let live = state.objects.get(source.source);
    if live.zone == Zone::Battlefield && live.zone_change_count == source.zone_change_count {
        let timestamp = crate::engine::next_timestamp(state);
        state
            .engine
            .until_end_of_turn
            .push(UntilEndOfTurnEffect::ResolvedObjectCantBeBlockedV1 {
                object_id: source.source,
                object_zone_change_count: source.zone_change_count,
                timestamp,
                duration: EffectDuration::EndOfTurn,
            });
    }
}
