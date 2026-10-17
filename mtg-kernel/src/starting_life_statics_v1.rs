//! Definition-owned self bonuses based on the live controller's starting life.
//!
//! Selected by the genuine Partial Elenda printing. Source-type protection lives
//! in source_type_hexproof_v1; this module retains the supported fixed starting
//! life contract and does not introduce variable starting life.

use crate::card_def::{Keywords, StartingLifeSelfBonusDefV1, CARD_DEFS};
use crate::ids::{ObjectId, PlayerId};
use crate::state::{GameState, Zone, STARTING_LIFE};
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
pub struct StartingLifeSelfBonusV1 {
    pub power: i32,
    pub toughness: i32,
    pub keywords: Keywords,
}

/// Recomputed public facts. The rule retains its exact threshold and separate
/// P/T and keyword grants, including while the printed ability is suppressed.
/// These facts introduce no saved-state fields or historical capture authority.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct StartingLifeStaticFactsV1 {
    pub controller: PlayerId,
    pub starting_life: i32,
    pub controller_life: i32,
    pub printed_abilities_active: bool,
    pub rules: &'static [StartingLifeSelfBonusDefV1],
    pub bonus: StartingLifeSelfBonusV1,
}

fn bonus_for_life(
    rules: &[StartingLifeSelfBonusDefV1],
    controller_life: i32,
) -> StartingLifeSelfBonusV1 {
    let mut bonus = StartingLifeSelfBonusV1::default();
    for rule in rules {
        // Widen before subtracting. The supported life store is signed i32,
        // including negative totals; no overflow or threshold clamp is needed.
        if i64::from(controller_life) - i64::from(STARTING_LIFE)
            >= i64::from(rule.minimum_above_start)
        {
            bonus.power += i32::from(rule.power);
            bonus.toughness += i32::from(rule.toughness);
            bonus.keywords.0 |= rule.keywords.0;
        }
    }
    bonus
}

fn facts_for_rules(
    state: &GameState,
    id: ObjectId,
    rules: &'static [StartingLifeSelfBonusDefV1],
) -> Option<StartingLifeStaticFactsV1> {
    if rules.is_empty() {
        return None;
    }
    let object = state.objects.try_get(id)?;
    let definition = CARD_DEFS.get(usize::from(object.card_def))?;
    if object.zone != Zone::Battlefield || object.v4.face_index != 0 || !definition.is_executable()
    {
        return None;
    }
    let controller_life = state.players[object.controller.index()].life;
    let printed_abilities_active =
        crate::continuous_characteristics_v1::printed_abilities_active(state, id);
    Some(StartingLifeStaticFactsV1 {
        controller: object.controller,
        starting_life: STARTING_LIFE,
        controller_life,
        printed_abilities_active,
        rules,
        bonus: if printed_abilities_active {
            bonus_for_life(rules, controller_life)
        } else {
            StartingLifeSelfBonusV1::default()
        },
    })
}

pub fn characteristics(state: &GameState, id: ObjectId) -> Option<StartingLifeStaticFactsV1> {
    let object = state.objects.try_get(id)?;
    let definition = CARD_DEFS.get(usize::from(object.card_def))?;
    facts_for_rules(state, id, definition.starting_life_self_bonuses_v1)
}

pub(crate) fn self_bonus(state: &GameState, id: ObjectId) -> StartingLifeSelfBonusV1 {
    characteristics(state, id).map_or_else(StartingLifeSelfBonusV1::default, |facts| facts.bonus)
}

#[cfg(test)]
#[path = "starting_life_statics_v1_tests.rs"]
mod tests;
