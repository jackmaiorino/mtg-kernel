//! Bounded resolved control effects lasting through the current turn.
use super::*;
use crate::engine::UntilEndOfTurnEffect;

fn printed_program() -> Option<EffectOp> {
    let token_def = crate::card_def::card_id_by_name("Treasure Token")?;
    Some(EffectOp::Sequence(vec![
        EffectOp::GainControlOfTargetCreatureUntilEndOfTurn { target_index: 0 },
        EffectOp::UntapObject {
            object: ObjectRef::Target(0),
        },
        EffectOp::GrantKeywordTargetUntilEndOfTurn {
            object: ObjectRef::Target(0),
            keyword: Keywords::HASTE,
        },
        EffectOp::CreateToken {
            token_def,
            controller: PlayerRef::Controller,
        },
    ]))
}

/// Only this exact new printed spell accepts accepted copy-history target
/// definitions. All other spell roots retain their existing validation.
pub(crate) fn owns_spell(state: &GameState, item: &StackItem) -> bool {
    item.kind == crate::state::StackItemKind::Spell
        && item.inline_effect.is_none()
        && item.mode_chosen == 0
        && state.objects.try_get(item.source).is_some_and(|source| {
            crate::card_def::CARD_DEFS
                .get(usize::from(source.card_def))
                .is_some_and(|def| {
                    def.name == "Involuntary Employment"
                        && def.target_spec == crate::card_def::TargetSpec::Creature
                        && printed_program().is_some_and(|root| (def.spell_effect)() == Some(root))
                })
        })
}

/// Copying does not change the targeted battlefield incarnation. Historical
/// definition authority never substitutes a different owner, zone or generation.
pub(crate) fn normalize_historical_target(
    state: &GameState,
    mut contract: StackTargetContractV4,
) -> StackTargetContractV4 {
    if let StackTargetContractV4::Object {
        object,
        card_def,
        owner,
        zone: Zone::Battlefield,
        zone_change_count,
        spell_copy_origin,
        ..
    } = &mut contract
    {
        if let Some(live) = state.objects.try_get(*object) {
            if live.zone == Zone::Battlefield
                && live.owner == *owner
                && live.zone_change_count == *zone_change_count
                && live.spell_copy_origin == *spell_copy_origin
                && live.card_def != *card_def
                && crate::standard_cards_v1::had_card_def(
                    state,
                    *object,
                    *zone_change_count,
                    *card_def,
                )
            {
                *card_def = live.card_def;
            }
        }
    }
    contract
}

/// Registered recipients exercise the recipe as an explicitly typed primitive.
#[cfg(test)]
fn involuntary_employment_program() -> EffectOp {
    printed_program().expect("Treasure Token in CARD_DEFS")
}

fn transfer(state: &mut GameState, object: ObjectId, controller: PlayerId) {
    let previous = state.objects.get(object).controller;
    if previous == controller {
        return;
    }
    // 506.4: changing control removes the permanent from combat.
    state.engine.combat.remove_from_combat(object);
    state.players[previous.index()]
        .battlefield
        .retain(|id| *id != object);
    state.players[controller.index()].battlefield.push(object);
    let live = state.objects.get_mut(object);
    live.controller = controller;
    // 302.6: the new controller has not controlled it since their turn began.
    live.summoning_sick = true;
}

pub(super) fn gain_control(ctx: &ExecCtx, index: usize, state: &mut GameState) {
    if !ctx.target_incarnation_matches(index, state)
        || !crate::engine::effect_target_is_legal(
            state,
            ctx.source,
            ctx.controller,
            crate::card_def::TargetSpec::Creature,
            &ctx.targets,
            index,
        )
    {
        return;
    }
    let Some(Target::Object(object)) = ctx.targets.get(index).copied() else {
        return;
    };
    let live = state.objects.get(object);
    let previous_controller = live.controller;
    let object_zone_change_count = live.zone_change_count;
    let timestamp = crate::engine::next_timestamp(state);
    state
        .engine
        .until_end_of_turn
        .push(UntilEndOfTurnEffect::ResolvedObjectControlEffect {
            object_id: object,
            object_zone_change_count,
            previous_controller,
            controller: ctx.controller,
            timestamp,
        });
    transfer(state, object, ctx.controller);
}

/// Every supported control producer has the same expiry. Reversing application
/// order restores the underlying controller, including multiple steals and an
/// effect applied while the caster already controls that creature. Departed
/// and reentered objects never inherit the old record (400.7).
pub(crate) fn expire_at_cleanup(state: &mut GameState) {
    let effects: Vec<_> = state
        .engine
        .until_end_of_turn
        .iter()
        .rev()
        .filter_map(|effect| {
            if let UntilEndOfTurnEffect::ResolvedObjectControlEffect {
                object_id,
                object_zone_change_count,
                previous_controller,
                controller,
                ..
            } = effect
            {
                Some((
                    *object_id,
                    *object_zone_change_count,
                    *previous_controller,
                    *controller,
                ))
            } else {
                None
            }
        })
        .collect();
    for (object, incarnation, previous, controller) in effects {
        if state.objects.try_get(object).is_some_and(|live| {
            live.zone == Zone::Battlefield
                && live.zone_change_count == incarnation
                && live.controller == controller
        }) {
            transfer(state, object, previous);
        }
    }
}

fn contains_control(op: &EffectOp) -> bool {
    match op {
        EffectOp::GainControlOfTargetCreatureUntilEndOfTurn { .. } => true,
        EffectOp::Sequence(ops) | EffectOp::Choice { options: ops, .. } => {
            ops.iter().any(contains_control)
        }
        EffectOp::Conditional { then, else_, .. } => {
            contains_control(then) || contains_control(else_)
        }
        _ => false,
    }
}
fn printed_spell_contains_control(state: &GameState, source: ObjectId, mode: u8) -> bool {
    let Some(object) = state.objects.try_get(source) else {
        return false;
    };
    let Some(def) = crate::card_def::CARD_DEFS.get(usize::from(object.card_def)) else {
        return false;
    };
    let program = match mode {
        0 => (def.spell_effect)(),
        1 => def.mode2.as_ref().map(|mode| (mode.effect)()),
        2 => def.mode3.as_ref().map(|mode| (mode.effect)()),
        _ => None,
    };
    program.as_ref().is_some_and(contains_control)
}
pub(crate) fn refuse_legacy_observation(state: &GameState) -> Result<(), String> {
    let announcing = state.engine.pending_cast.as_ref().is_some_and(|pending| {
        printed_spell_contains_control(state, pending.spell, pending.mode_chosen.unwrap_or(0))
    });
    let resolving = state
        .engine
        .pending_effect
        .as_ref()
        .map(|pending| &pending.resolving_item);
    let stacked = state.stack.iter().chain(resolving).any(|item| {
        item.inline_effect.as_ref().is_some_and(contains_control)
            || (item.kind == crate::state::StackItemKind::Spell
                && printed_spell_contains_control(state, item.source, item.mode_chosen))
    });
    if announcing
        || stacked
        || state.engine.until_end_of_turn.iter().any(|effect| {
            matches!(
                effect,
                UntilEndOfTurnEffect::ResolvedObjectControlEffect { .. }
            )
        })
    {
        return Err("temporary creature control requires an explicit observation successor".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests;
