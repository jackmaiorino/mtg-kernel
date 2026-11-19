//! Ursine Monstrosity's resolved combat requirement and fixed temporary boost.
//! The engine has exactly two PlayerIds, so choosing an opponent has one outcome.

use crate::card_def::{CardType, CombatStaticRulesDef, Keywords, CARD_DEFS};
use crate::effect::{EffectObjectBinding, ExecCtx};
use crate::eternal_legacy_e9a_v1::TurnV1;
use crate::event::CommittedEvent;
use crate::ids::{ObjectId, PlayerId};
use crate::state::{GameState, Step, Target, Zone};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CombatV1 {
    object: ObjectId,
    generation: u32,
    opponent: PlayerId,
    turn: TurnV1,
    combat_at: usize,
}

fn combat_at(state: &GameState) -> Option<usize> {
    state
        .engine
        .event_history
        .iter()
        .rposition(|event| matches!(event, CommittedEvent::BeginningOfCombat { .. }))
}

pub(super) fn resolve(ctx: &ExecCtx, state: &mut GameState) {
    let Some(source) = ctx.ability_source_contract else {
        return;
    };
    let Some(live) = state.objects.try_get(source.source) else {
        return;
    };
    if live.zone != Zone::Battlefield || live.zone_change_count != source.zone_change_count {
        return;
    }
    // Mill is the preceding root instruction. Tokens and spell copies are not cards.
    let cards: Vec<_> = state.players[ctx.controller.index()]
        .graveyard
        .iter()
        .copied()
        .filter(|&object| {
            let live = state.objects.get(object);
            !live.v4.is_token && live.spell_copy_origin.is_none()
        })
        .collect();
    let count = i32::try_from(crate::eternal_cards_v1::card_type_count(state, &cards)).unwrap_or(0);
    crate::effect::install_temporary_boost(
        state,
        EffectObjectBinding {
            object: source.source,
            expected_zone: Zone::Battlefield,
            expected_zone_change_count: source.zone_change_count,
        },
        count,
        count,
        Keywords::INDESTRUCTIBLE,
    );
    let Some(combat_at) = combat_at(state) else {
        return;
    };
    let turn = TurnV1::now(state);
    let entries = &mut state
        .eternal_v1
        .get_or_insert_with(Default::default)
        .c6b
        .ursine_combats;
    entries.retain(|entry| entry.turn == turn && entry.combat_at == combat_at);
    entries.push(CombatV1 {
        object: source.source,
        generation: source.zone_change_count,
        opponent: ctx.controller.opponent(),
        turn,
        combat_at,
    });
}

fn required_opponent(state: &GameState, object: ObjectId) -> Option<PlayerId> {
    let entries = &state.eternal_v1.as_ref()?.c6b.ursine_combats;
    if entries.is_empty() || !matches!(state.step, Step::BeginCombat | Step::DeclareAttackers) {
        return None;
    }
    let live = state.objects.try_get(object)?;
    let turn = TurnV1::now(state);
    if live.zone != Zone::Battlefield
        || !entries.iter().any(|entry| {
            entry.object == object
                && entry.generation == live.zone_change_count
                && entry.turn == turn
        })
    {
        return None;
    }
    let combat_at = combat_at(state)?;
    entries
        .iter()
        .find(|entry| {
            entry.object == object
                && entry.generation == live.zone_change_count
                && live.zone == Zone::Battlefield
                && entry.turn == turn
                && entry.combat_at == combat_at
                && entry.opponent == live.controller.opponent()
        })
        .map(|entry| entry.opponent)
}

/// Rabblemaster's existing requirement handling also preserves attack costs.
/// This resolved effect persists if the source subsequently loses its abilities.
pub(crate) fn combat_rules(state: &GameState, object: ObjectId) -> Option<CombatStaticRulesDef> {
    required_opponent(state, object)?;
    if !crate::engine::object_has_type(state, object, CardType::Creature) {
        return None;
    }
    let live = state.objects.get(object);
    let printed = if crate::continuous_characteristics_v1::printed_abilities_active(state, object) {
        CARD_DEFS[usize::from(live.card_def)].combat_static_rules
    } else {
        CombatStaticRulesDef::NONE
    };
    Some(CombatStaticRulesDef {
        must_attack_each_combat: true,
        ..printed
    })
}

/// Both the offered choices and the answer validator use this same filter.
pub(crate) fn attack_target_allowed(state: &GameState, attacker: ObjectId, target: Target) -> bool {
    required_opponent(state, attacker).is_none_or(|opponent| target == Target::Player(opponent))
}
