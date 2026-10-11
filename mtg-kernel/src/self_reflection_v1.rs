//! Unregistered Self-Reflection root using the accepted token-copy allocator.
use crate::card_def::{CardType, TargetSpec, CARD_DEFS};
use crate::effect::{EffectOp, ExecCtx};
use crate::state::{GameState, StackItem, StackItemKind, StackTargetContractV4, Target};

pub(crate) fn owns_spell(state: &GameState, item: &StackItem) -> bool {
    item.kind == StackItemKind::Spell
        && item.inline_effect.is_none()
        && state.objects.try_get(item.source).is_some_and(|source| {
            (CARD_DEFS[source.card_def as usize].spell_effect)()
                == Some(EffectOp::CreateControlledCreatureTargetCopyV1)
        })
}
/// Copy layers change definition, not physical incarnation. Only this new
/// root accepts the existing exact-generation historical-definition authority.
/// Owner, zone, generation and virtual-copy origin remain independently checked.
pub(crate) fn normalize_historical_target(
    state: &GameState,
    mut contract: StackTargetContractV4,
) -> StackTargetContractV4 {
    if let StackTargetContractV4::Object {
        object,
        card_def,
        zone_change_count,
        ..
    } = &mut contract
    {
        if state
            .objects
            .try_get(*object)
            .is_some_and(|live| live.card_def != *card_def)
            && crate::standard_cards_v1::had_card_def(state, *object, *zone_change_count, *card_def)
        {
            *card_def = state.objects.get(*object).card_def;
        }
    }
    contract
}
pub(crate) fn target_matches_live(
    state: &GameState,
    target: Target,
    contract: StackTargetContractV4,
) -> bool {
    crate::engine::target_contract_matches_live(
        state,
        target,
        normalize_historical_target(state, contract),
    )
}
pub(crate) fn execute(state: &mut GameState, ctx: &ExecCtx) {
    let ([target @ Target::Object(object)], [contract]) =
        (ctx.targets.as_slice(), ctx.target_contracts.as_slice())
    else {
        return;
    };
    if !target_matches_live(state, *target, *contract)
        || !crate::engine::effect_target_is_legal(
            state,
            ctx.source,
            ctx.controller,
            TargetSpec::ControlledCreature,
            &ctx.targets,
            0,
        )
        || !crate::engine::object_has_type(state, *object, CardType::Creature)
    {
        return;
    }
    crate::standard_cards_v1::create_token_copy(state, *object, ctx.controller);
}

#[cfg(test)]
mod tests;
