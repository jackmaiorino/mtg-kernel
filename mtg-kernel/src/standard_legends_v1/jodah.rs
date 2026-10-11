//! Jodah binds the hand cast and its mana value before either spell can leave.
use super::LegendEffectV1;
use crate::card_def::{Supertype, CARD_DEFS};
use crate::effect::{EffectOp, ExileCastPredicateV1, PlayerRef};
use crate::event::CommittedEvent;
use crate::ids::{ObjectId, PlayerId, StackItemId};
use crate::state::{
    CastMethodV4, GameState, SpellCastRouteV4, StackItem, StackItemKind, StackSourceContractV4,
    Zone,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct JodahCastV1 {
    pub spell: StackItemId,
    pub source: StackSourceContractV4,
    pub cast_event_index: u64,
    pub mana_value: u16,
}

pub(crate) fn template() -> EffectOp {
    EffectOp::StandardLegendV1(LegendEffectV1::BindJodahCast)
}
fn program(binding: JodahCastV1) -> EffectOp {
    EffectOp::Sequence(vec![
        EffectOp::StandardLegendV1(LegendEffectV1::JodahCastSnapshot(binding)),
        EffectOp::ExileUntilThenCastV1 {
            players: vec![PlayerRef::Controller],
            predicate: ExileCastPredicateV1::LegendaryNonlandManaValueLessThan(binding.mana_value),
            return_rest_to_bottom: true,
        },
    ])
}
fn eligible(item: &StackItem, state: &GameState) -> bool {
    let Some(source) = item.v4.source_contract else {
        return false;
    };
    item.kind == StackItemKind::Spell
        && !item.is_copy
        && item.v4.cast_method != Some(CastMethodV4::Omen)
        && state.objects.get(item.source).v4.face_down_v1.is_none()
        && CARD_DEFS[source.card_def as usize]
            .supertypes
            .contains(&Supertype::Legendary)
        && source.spell_cast_origin.is_some_and(|origin| {
            origin.origin_zone == Zone::Hand && origin.route == SpellCastRouteV4::Hand
        })
}
pub(crate) fn trigger_matches(state: &GameState, source: ObjectId, event: &CommittedEvent) -> bool {
    let CommittedEvent::SpellCast { spell, controller } = event else {
        return false;
    };
    super::has_printed_ability(state, source, "Jodah, the Unifier")
        && state.objects.get(source).controller == *controller
        && state
            .stack
            .iter()
            .any(|item| item.source == *spell && eligible(item, state))
}
pub(crate) fn materialize(state: &GameState, event: &CommittedEvent) -> Option<EffectOp> {
    let CommittedEvent::SpellCast { spell, .. } = event else {
        return None;
    };
    let item = state
        .stack
        .iter()
        .find(|item| item.source == *spell && eligible(item, state))?;
    let cast_event_index = state
        .engine
        .event_history
        .iter()
        .rposition(|candidate| candidate == event)? as u64;
    Some(program(JodahCastV1 {
        spell: item.v4.stack_item_id,
        source: item.v4.source_contract?,
        cast_event_index,
        mana_value: crate::engine::stack_spell_mana_value(state, item),
    }))
}
fn binding(effect: &EffectOp) -> Option<JodahCastV1> {
    let EffectOp::Sequence(ops) = effect else {
        return None;
    };
    let EffectOp::StandardLegendV1(LegendEffectV1::JodahCastSnapshot(binding)) = ops.first()?
    else {
        return None;
    };
    Some(*binding)
}
pub(crate) fn template_matches(template_op: &EffectOp, effect: &EffectOp) -> bool {
    *template_op == template() && binding(effect).is_some_and(|cast| program(cast) == *effect)
}
pub(crate) fn validate_effect(
    state: &GameState,
    effect: &EffectOp,
    controller: PlayerId,
) -> Result<(), String> {
    let Some(cast) = binding(effect) else {
        return Ok(());
    };
    let source = cast.source;
    let live = state
        .objects
        .try_get(source.source)
        .ok_or("Jodah cast source missing")?;
    let def = CARD_DEFS
        .get(source.card_def as usize)
        .ok_or("Jodah cast definition missing")?;
    let origin = source
        .spell_cast_origin
        .ok_or("Jodah cast origin missing")?;
    let x = source
        .finalized_cast_binding
        .map_or(0, |binding| binding.x_value);
    let expected_value = def
        .mana_value
        .saturating_add(x.saturating_mul(u16::from(def.cost.x_count)));
    if program(cast) != *effect
        || source.controller != controller
        || source.zone != Zone::Stack
        || source.spell_copy_origin.is_some()
        || source.card_def != live.card_def
        || source.owner != live.owner
        || live.zone_change_count < source.zone_change_count
        || cast.spell == StackItemId::default()
        || source.cast_method == CastMethodV4::Omen
        || !def.supertypes.contains(&Supertype::Legendary)
        || origin.origin_zone != Zone::Hand
        || origin.route != SpellCastRouteV4::Hand
        || origin.finalized_method != Some(source.cast_method)
        || origin.origin_zone_change_count.checked_add(1) != Some(source.zone_change_count)
        || cast.mana_value != expected_value
        || state.engine.event_history.get(
            usize::try_from(cast.cast_event_index).map_err(|_| "Jodah cast event out of range")?,
        ) != Some(&CommittedEvent::SpellCast {
            spell: source.source,
            controller,
        })
    {
        return Err("Jodah changed its bound legendary hand cast".into());
    }
    if let Some(item) = state
        .stack
        .iter()
        .find(|item| item.v4.stack_item_id == cast.spell)
    {
        if item.v4.source_contract != Some(source)
            || !eligible(item, state)
            || crate::engine::stack_spell_mana_value(state, item) != cast.mana_value
        {
            return Err("Jodah cast snapshot differs from its spell".into());
        }
    } else if live.zone_change_count == source.zone_change_count {
        return Err("Jodah cast has no live spell or later incarnation".into());
    }
    Ok(())
}
