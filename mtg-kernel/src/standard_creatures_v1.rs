//! Persistent, incarnation-bound creature upgrades used by Standard cards.

use serde::{Deserialize, Serialize};

use crate::card_def::{Keywords, Subtype};
use crate::effect::ExecCtx;
use crate::ids::ObjectId;
use crate::state::{GameState, Zone};

#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CreatureUpgradeV1 {
    pub creature_types: Option<(Vec<u16>, u64)>,
    pub base_stats: Option<(i16, i16, u64)>,
    pub color: Option<(u8, u64)>,
    pub keyword_grants: Vec<(Keywords, u64)>,
    pub keyword_losses: Vec<(Keywords, u64)>,
    pub combat_impulse: Option<u64>,
    pub once_activated: Vec<u16>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CreatureEffectV1 {
    KellanDetective,
    KellanRogue,
    SurgeUnblockable,
    SurgeBlue,
}

fn upgrade(state: &GameState, id: ObjectId) -> Option<&CreatureUpgradeV1> {
    let object = state.objects.try_get(id)?;
    (object.zone == Zone::Battlefield)
        .then_some(object.v4.creature_upgrade.as_ref())
        .flatten()
}

/// Layer 7b: compare effect timestamps, independently of ability removal.
pub(crate) fn base_stats(state: &GameState, id: ObjectId) -> Option<(i32, i32)> {
    let (power, toughness, timestamp) = upgrade(state, id)?.base_stats?;
    if crate::continuous_characteristics_v1::creature_override(state, id)
        .is_some_and(|(_, other)| other > timestamp)
    {
        return None;
    }
    Some((i32::from(power), i32::from(toughness)))
}

pub(crate) fn color(state: &GameState, id: ObjectId) -> Option<u8> {
    let (color, timestamp) = upgrade(state, id)?.color?;
    if crate::continuous_characteristics_v1::creature_override(state, id)
        .is_some_and(|(_, other)| other > timestamp)
    {
        return None;
    }
    Some(color)
}

/// A newer creature-type replacement keeps this permanent's noncreature types.
pub(crate) fn creature_types(state: &GameState, id: ObjectId) -> Option<(Vec<u16>, u64)> {
    let (types, timestamp) = upgrade(state, id)?.creature_types.as_ref()?;
    if crate::continuous_characteristics_v1::creature_override(state, id)
        .is_some_and(|(_, other)| other > *timestamp)
    {
        return None;
    }
    let object = state.objects.get(id);
    let mut result: Vec<_> = object
        .v4
        .effective_subtype_ids
        .iter()
        .copied()
        .filter(|value| {
            !Subtype::CREATURE_TYPES
                .iter()
                .any(|subtype| subtype.stable_id() == *value)
        })
        .collect();
    result.extend_from_slice(types);
    Some((result, *timestamp))
}

pub(crate) fn keyword_granted(state: &GameState, id: ObjectId, keyword: Keywords) -> bool {
    upgrade(state, id).is_some_and(|value| {
        value.keyword_grants.iter().any(|(granted, timestamp)| {
            granted.has(keyword)
                && crate::continuous_characteristics_v1::grant_survives(state, id, *timestamp)
                && !value
                    .keyword_losses
                    .iter()
                    .any(|(lost, other)| lost.has(keyword) && other > timestamp)
        })
    })
}

pub(crate) fn printed_keyword_removed(state: &GameState, id: ObjectId, keyword: Keywords) -> bool {
    upgrade(state, id).is_some_and(|value| {
        value
            .keyword_losses
            .iter()
            .any(|(lost, _)| lost.has(keyword))
    })
}

pub(crate) fn combat_impulse_active(state: &GameState, id: ObjectId) -> bool {
    upgrade(state, id)
        .and_then(|value| value.combat_impulse)
        .is_some_and(|timestamp| {
            crate::continuous_characteristics_v1::grant_survives(state, id, timestamp)
        })
}

pub(crate) fn activation_allowed(state: &GameState, id: ObjectId, index: usize) -> bool {
    let object = state.objects.get(id);
    if crate::card_def::CARD_DEFS[object.card_def as usize].name != "Surge Engine" {
        return true;
    }
    match index {
        1 => !crate::engine::has_effective_keyword(state, id, Keywords::DEFENDER),
        2 => {
            crate::engine::object_color_mask(state, id) & 2 != 0
                && !upgrade(state, id).is_some_and(|value| value.once_activated.contains(&2))
        }
        _ => true,
    }
}

pub(crate) fn activation_paid(state: &mut GameState, id: ObjectId, index: usize) {
    if crate::card_def::CARD_DEFS[state.objects.get(id).card_def as usize].name == "Surge Engine"
        && index == 2
    {
        state
            .objects
            .get_mut(id)
            .v4
            .creature_upgrade
            .get_or_insert_with(Default::default)
            .once_activated
            .push(2);
    }
}

pub(crate) fn execute(effect: CreatureEffectV1, ctx: &ExecCtx, state: &mut GameState) {
    let Some(contract) = ctx.ability_source_contract else {
        return;
    };
    let Some(object) = state.objects.try_get(ctx.source) else {
        return;
    };
    if object.zone != Zone::Battlefield || object.zone_change_count != contract.zone_change_count {
        return;
    }
    let prerequisite = match effect {
        CreatureEffectV1::KellanDetective => Some(Subtype::Scout),
        CreatureEffectV1::KellanRogue => Some(Subtype::Detective),
        _ => None,
    };
    if prerequisite
        .is_some_and(|subtype| !crate::engine::has_effective_subtype(state, ctx.source, subtype))
    {
        return;
    }
    let timestamp = crate::engine::next_timestamp(state);
    let value = state
        .objects
        .get_mut(ctx.source)
        .v4
        .creature_upgrade
        .get_or_insert_with(Default::default);
    match effect {
        CreatureEffectV1::KellanDetective => {
            value.creature_types = Some((
                vec![
                    Subtype::Human.stable_id(),
                    Subtype::Faerie.stable_id(),
                    Subtype::Detective.stable_id(),
                ],
                timestamp,
            ));
            value.combat_impulse = Some(timestamp);
        }
        CreatureEffectV1::KellanRogue => {
            value.creature_types = Some((
                vec![
                    Subtype::Human.stable_id(),
                    Subtype::Faerie.stable_id(),
                    Subtype::Rogue.stable_id(),
                ],
                timestamp,
            ));
            value.base_stats = Some((3, 2, timestamp));
            value
                .keyword_grants
                .push((Keywords::DOUBLE_STRIKE, timestamp));
        }
        CreatureEffectV1::SurgeUnblockable => {
            value.keyword_losses.push((Keywords::DEFENDER, timestamp));
            value
                .keyword_grants
                .push((Keywords::CANT_BE_BLOCKED, timestamp));
        }
        CreatureEffectV1::SurgeBlue => {
            value.base_stats = Some((5, 4, timestamp));
            value.color = Some((2, timestamp));
        }
    }
}
