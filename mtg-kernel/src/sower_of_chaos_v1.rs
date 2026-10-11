//! Sower's rule-changing restriction reuses the existing incarnation/cleanup vector.
use crate::card_def::TargetSpec;
use crate::effect::ExecCtx;
use crate::engine::{self, EffectDuration, UntilEndOfTurnEffect};
use crate::ids::ObjectId;
use crate::state::{GameState, Target, Zone};

pub(crate) fn cant_block(state: &GameState, object: ObjectId) -> bool {
    let Some(live) = state.objects.try_get(object) else {
        return false;
    };
    live.zone == Zone::Battlefield
        && state.engine.until_end_of_turn.iter().any(|effect| {
            matches!(effect, UntilEndOfTurnEffect::ResolvedObjectCantBlock {
            object_id, object_zone_change_count, duration: EffectDuration::EndOfTurn, ..
        } if *object_id == object && *object_zone_change_count == live.zone_change_count)
        })
}
pub(crate) fn install(state: &mut GameState, ctx: &ExecCtx, target_index: u8) {
    let index = usize::from(target_index);
    if !ctx.target_incarnation_matches(index, state) {
        return;
    }
    let legal = if let Some(source) = ctx.ability_source_contract {
        source.source == ctx.source
            && source.controller == ctx.controller
            && engine::effect_target_is_legal_from_ability_source(
                state,
                source,
                ctx.controller,
                TargetSpec::Creature,
                &ctx.targets,
                index,
            )
    } else {
        engine::effect_target_is_legal(
            state,
            ctx.source,
            ctx.controller,
            TargetSpec::Creature,
            &ctx.targets,
            index,
        )
    };
    if !legal {
        return;
    }
    let Some(Target::Object(object)) = ctx.targets.get(index).copied() else {
        return;
    };
    let count = state.objects.get(object).zone_change_count;
    let timestamp = engine::next_timestamp(state);
    state
        .engine
        .until_end_of_turn
        .push(UntilEndOfTurnEffect::ResolvedObjectCantBlock {
            object_id: object,
            object_zone_change_count: count,
            timestamp,
            duration: EffectDuration::EndOfTurn,
        });
}
