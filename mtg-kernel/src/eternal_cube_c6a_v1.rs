//! Vintage Cube wave C6a: discard, exile and impulse-draw trigger creatures.
//! Inti remains unsupported until all multi-card discard callers preserve
//! instruction boundaries: ordinary cleanup, Time Stop cleanup, Breakthrough,
//! and Cabal Therapy's legacy fallback still loop over single-card commits.
#![cfg_attr(not(feature = "eternal-fixtures"), allow(dead_code))]

use crate::card_def::{card_id_by_name, TargetSpec};
use crate::effect::{EffectContinuation, EffectOp, ExecCtx, ObjectRef, PlayerRef};
use crate::ids::{ObjectId, PlayerId};
use crate::state::{AbilitySourceContractV4, GameState, Zone};
use crate::trigger::{TriggerCondition, TriggeredAbilityDef};
use serde::{Deserialize, Serialize};

/// Reserved effect leaves for this sub-wave. Ivora composes existing ops.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum C6aOpV1 {}

impl C6aOpV1 {
    pub(crate) fn contains_player_choice(&self) -> bool {
        match *self {}
    }
}

const IVORA: &str = "Ivora, Insatiable Heir";

/// Put the resolving creature spell onto the battlefield.
pub fn permanent() -> EffectOp {
    EffectOp::MoveObject {
        object: ObjectRef::ThisSource,
        to_zone: Zone::Battlefield,
    }
}

/// Bloodtithe Harvester's entry effect, also used for combat player damage.
fn blood() -> EffectOp {
    EffectOp::CreateToken {
        token_def: card_id_by_name("Blood Token").expect("registered Blood Token"),
        controller: PlayerRef::Controller,
    }
}

fn ivora_counter() -> EffectOp {
    EffectOp::AddPlusOneCounterToAbilitySource
}

const fn trigger(condition: TriggerCondition, effect: fn() -> EffectOp) -> TriggeredAbilityDef {
    TriggeredAbilityDef {
        condition,
        home_zone: Zone::Battlefield,
        intervening_if_kicked: false,
        intervening_if_controls_another_source_card: false,
        face_index: 0,
        effect,
    }
}

const IVORA_TRIGGERS: [TriggeredAbilityDef; 3] = [
    trigger(TriggerCondition::Etb, blood),
    trigger(TriggerCondition::DealsCombatDamageToPlayer, blood),
    trigger(TriggerCondition::ControllerDiscardsCardV1, ivora_counter),
];

/// Capture observers before the discard moves any card, as Currency Converter
/// does. This counts cards regardless of their replacement destination and
/// retains each observer's original controller and battlefield incarnation.
/// These triggers disclose no identity from the discarded hand.
pub(crate) fn note_discards(state: &mut GameState, cards: &[ObjectId]) {
    for player in [PlayerId::P0, PlayerId::P1] {
        let sources = crate::eternal_e5a_v1::active_permanents(state, IVORA, Some(player))
            .map(|object| AbilitySourceContractV4::capture(state, object))
            .collect::<Vec<_>>();
        if sources.is_empty() {
            continue;
        }
        let count = cards
            .iter()
            .filter(|&&card| {
                state.players[player.index()].hand.contains(&card)
                    && state.objects.get(card).zone == Zone::Hand
            })
            .count();
        for source in sources {
            for _ in 0..count {
                state
                    .engine
                    .pending_triggers
                    .push(crate::eternal_cards_v1::delayed_trigger(
                        source,
                        player,
                        ivora_counter(),
                    ));
            }
        }
    }
}

pub(crate) fn triggers_for(name: &str) -> &'static [TriggeredAbilityDef] {
    match name {
        IVORA => &IVORA_TRIGGERS,
        _ => crate::eternal_cube_c6b_v1::triggers_for(name),
    }
}

pub(crate) fn trigger_target_spec(name: &str, effect: &EffectOp) -> Option<TargetSpec> {
    (name == IVORA && (*effect == blood() || *effect == ivora_counter()))
        .then_some(TargetSpec::None)
}

pub(crate) fn delayed_effect_matches(name: &str, effect: &EffectOp) -> bool {
    name == IVORA && *effect == ivora_counter()
}

pub(crate) fn execute(op: &C6aOpV1, _ctx: &ExecCtx, _state: &mut GameState) {
    match *op {}
}

pub(crate) fn drive(
    _state: &mut GameState,
    _continuation: &mut EffectContinuation,
    op: &C6aOpV1,
    _path: Vec<u16>,
) -> Result<bool, String> {
    match *op {}
}
