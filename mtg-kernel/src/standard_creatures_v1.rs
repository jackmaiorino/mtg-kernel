//! Persistent, incarnation-bound creature upgrades used by Standard cards.

use serde::{Deserialize, Serialize};

use crate::card_def::{Keywords, Subtype};
use crate::effect::ExecCtx;
use crate::ids::ObjectId;
use crate::state::{GameState, Zone};

#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(default)]
pub struct CreatureUpgradeV1 {
    pub temporary_creature: Option<(i16, i16, u64)>,
    pub suppressed_by: Vec<TidebinderSuppressionV1>,
    pub graveyard_adventure: Option<GraveyardAdventurePermissionV1>,
    pub haste_blockers_only: bool,
    pub creature_types: Option<(Vec<u16>, u64)>,
    pub base_stats: Option<(i16, i16, u64)>,
    pub color: Option<(u8, u64)>,
    pub keyword_grants: Vec<(Keywords, u64)>,
    pub keyword_losses: Vec<(Keywords, u64)>,
    pub combat_impulse: Option<u64>,
    pub once_activated: Vec<u16>,
    pub wurmlet_resolved_turn: Option<(u32, crate::ids::PlayerId)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CreatureEffectV1 {
    KellanDetective,
    KellanRogue,
    SurgeUnblockable,
    SurgeBlue,
    GingerEvasion,
    ToughCookieAnimate,
    WurmletCounterIfFirstResolution,
    HarvesterWeakening,
    SalvagerBoostTokens,
    VirtueCountersUntap,
    MosswoodGraveyardAdventure,
    FloodpitsTapStun,
    FloodpitsShuffle,
    EssenceTransferCounters,
    TidebinderCounter,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct GraveyardAdventurePermissionV1 {
    pub holder: crate::ids::PlayerId,
    pub holder_turn_started: bool,
}

pub(crate) fn graveyard_permission(
    state: &GameState,
    id: ObjectId,
) -> Option<GraveyardAdventurePermissionV1> {
    state
        .objects
        .try_get(id)?
        .v4
        .creature_upgrade
        .as_ref()?
        .graveyard_adventure
}

pub(crate) fn graveyard_adventure_allowed(
    state: &GameState,
    id: ObjectId,
    holder: crate::ids::PlayerId,
) -> bool {
    let object = state.objects.get(id);
    object.zone == Zone::Graveyard
        && crate::card_def::CARD_DEFS[object.card_def as usize].name == "Mosswood Dreadknight"
        && graveyard_permission(state, id).is_some_and(|permission| permission.holder == holder)
}

pub(crate) fn begin_turn(state: &mut GameState) {
    for (_, object) in state.objects.iter_mut() {
        if let Some(permission) = object
            .v4
            .creature_upgrade
            .as_mut()
            .and_then(|value| value.graveyard_adventure.as_mut())
        {
            if permission.holder == state.active_player {
                permission.holder_turn_started = true;
            }
        }
    }
}

/// Counter families stored outside the ordinary counter structure.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CounterExtrasV1 {
    pub lifelink: i16,
    pub time: u8,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CounterTransferV1 {
    pub counters: crate::state::Counters,
    pub extras: CounterExtrasV1,
}

pub(crate) fn counter_transfer_snapshot(
    state: &GameState,
    source: crate::state::AbilitySourceContractV4,
) -> CounterTransferV1 {
    state
        .counter_lki_v1
        .as_ref()
        .and_then(|entries| {
            entries.iter().find(|entry| {
                entry.source.object == source.source
                    && entry.source.zone_change_count == source.zone_change_count
            })
        })
        .map(|entry| CounterTransferV1 {
            counters: entry.counters,
            extras: entry.extras.unwrap_or_default(),
        })
        .unwrap_or_default()
}

fn apply_counter_transfer(
    state: &mut GameState,
    target: ObjectId,
    player: crate::ids::PlayerId,
    snapshot: CounterTransferV1,
) -> Option<()> {
    // Work on a caller-owned projection so every family commits atomically.
    let counters = snapshot.counters;
    let scale = |amount: i32| {
        #[cfg(feature = "standard-magezero-fixtures")]
        {
            crate::standard_cards_v1::scale_counters(
                state,
                state.objects.get(target).controller,
                amount,
            )
        }
        #[cfg(not(feature = "standard-magezero-fixtures"))]
        {
            amount
        }
    };
    let placed_i16 = |amount: i16| i16::try_from(scale(i32::from(amount))).ok();
    let live = state.objects.get(target);
    let minus_one = live
        .counters
        .minus1_minus1
        .checked_add(placed_i16(counters.minus1_minus1)?)?;
    let minus_toughness = live
        .counters
        .minus0_minus1
        .checked_add(placed_i16(counters.minus0_minus1)?)?;
    let stun = live.counters.stun.checked_add(placed_i16(counters.stun)?)?;
    let lore = live.counters.lore.checked_add(placed_i16(counters.lore)?)?;
    let oil = live.counters.oil.checked_add(placed_i16(counters.oil)?)?;
    let lifelink = live
        .v4
        .lifelink_keyword_counters
        .checked_add(placed_i16(snapshot.extras.lifelink)?)?;
    let time = live
        .v4
        .time_counters_v1
        .checked_add(u8::try_from(scale(i32::from(snapshot.extras.time))).ok()?)?;
    crate::event::add_plus_one_counters(state, target, player, counters.plus1_plus1).ok()?;
    let keyword_timestamp =
        (snapshot.extras.lifelink > 0).then(|| crate::engine::next_timestamp(state));
    let live = state.objects.get_mut(target);
    live.counters.minus1_minus1 = minus_one;
    live.counters.minus0_minus1 = minus_toughness;
    live.counters.stun = stun;
    live.counters.lore = lore;
    live.counters.oil = oil;
    live.v4.lifelink_keyword_counters = lifelink;
    live.v4.time_counters_v1 = time;
    if keyword_timestamp.is_some() {
        live.v4.lifelink_counter_timestamp = keyword_timestamp;
    }
    Some(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TidebinderSuppressionV1 {
    pub source: crate::state::ObjectLinkV4,
    pub timestamp: u64,
}

pub(crate) fn removal_timestamp(state: &GameState, target: ObjectId) -> Option<u64> {
    upgrade(state, target)?
        .suppressed_by
        .iter()
        .filter(|effect| {
            state
                .objects
                .try_get(effect.source.object)
                .is_some_and(|source| {
                    source.zone == Zone::Battlefield
                        && source.zone_change_count == effect.source.zone_change_count
                })
        })
        .map(|effect| effect.timestamp)
        .max()
}

fn upgrade(state: &GameState, id: ObjectId) -> Option<&CreatureUpgradeV1> {
    let object = state.objects.try_get(id)?;
    (object.zone == Zone::Battlefield)
        .then_some(object.v4.creature_upgrade.as_ref())
        .flatten()
}

/// Layer 7b: compare effect timestamps, independently of ability removal.
pub(crate) fn base_stats(state: &GameState, id: ObjectId) -> Option<(i32, i32)> {
    let value = upgrade(state, id)?;
    let (power, toughness, timestamp) = value
        .base_stats
        .into_iter()
        .chain(value.temporary_creature)
        .max_by_key(|entry| entry.2)?;
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

pub(crate) fn cleanup(state: &mut GameState) {
    for (_, object) in state.objects.iter_mut() {
        if let Some(upgrade) = &mut object.v4.creature_upgrade {
            upgrade.temporary_creature = None;
            upgrade.haste_blockers_only = false;
            if upgrade.graveyard_adventure.is_some_and(|permission| {
                permission.holder == state.active_player && permission.holder_turn_started
            }) {
                upgrade.graveyard_adventure = None;
            }
            if *upgrade == CreatureUpgradeV1::default() {
                object.v4.creature_upgrade = None;
            }
        }
    }
}

pub(crate) fn animated_creature(state: &GameState, id: ObjectId) -> bool {
    upgrade(state, id)
        .and_then(|value| value.temporary_creature)
        .is_some_and(|(_, _, timestamp)| {
            crate::continuous_characteristics_v1::creature_override(state, id)
                .is_none_or(|(_, other)| timestamp > other)
        })
}

pub(crate) fn attacks_player(state: &GameState, attacker: ObjectId) -> bool {
    crate::attack_target_v1::attacked_planeswalker(state, attacker).is_none()
}

pub(crate) fn blocker_allowed(state: &GameState, attacker: ObjectId, blocker: ObjectId) -> bool {
    !upgrade(state, attacker).is_some_and(|value| value.haste_blockers_only)
        || crate::engine::has_effective_keyword(state, blocker, Keywords::HASTE)
}

pub(crate) fn execute(effect: CreatureEffectV1, ctx: &ExecCtx, state: &mut GameState) {
    if effect == CreatureEffectV1::TidebinderCounter {
        let Some(crate::state::Target::StackItem(target)) = ctx.targets.first().copied() else {
            return;
        };
        let Some(source) = ctx.ability_source_contract else {
            return;
        };
        if !ctx.target_contracts.first().is_some_and(|&contract| {
            crate::engine::target_contract_matches_live(state, ctx.targets[0], contract)
        }) {
            return;
        }
        let Some(ability) = state
            .stack
            .iter()
            .find(|item| item.v4.stack_item_id == target)
        else {
            return;
        };
        if !matches!(
            ability.kind,
            crate::state::StackItemKind::ActivatedAbility
                | crate::state::StackItemKind::TriggeredAbility
        ) {
            return;
        }
        let permanent = ability.v4.ability_source_contract;
        if crate::engine::counter_stack_item_by_id(state, target).is_err() {
            state.engine.halted = Some((
                crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
                ctx.source,
            ));
            return;
        }
        if state
            .stack
            .iter()
            .any(|item| item.v4.stack_item_id == target)
        {
            return;
        }
        let Some(permanent) = permanent else {
            return;
        };
        if !state.objects.try_get(ctx.source).is_some_and(|live| {
            live.zone == Zone::Battlefield && live.zone_change_count == source.zone_change_count
        }) || !state.objects.try_get(permanent.source).is_some_and(|live| {
            live.zone == Zone::Battlefield && live.zone_change_count == permanent.zone_change_count
        }) || ![
            crate::card_def::CardType::Artifact,
            crate::card_def::CardType::Creature,
            crate::card_def::CardType::Planeswalker,
        ]
        .into_iter()
        .any(|kind| crate::engine::object_has_type(state, permanent.source, kind))
        {
            return;
        }
        let timestamp = crate::engine::next_timestamp(state);
        state
            .objects
            .get_mut(permanent.source)
            .v4
            .creature_upgrade
            .get_or_insert_with(Default::default)
            .suppressed_by
            .push(TidebinderSuppressionV1 {
                source: crate::state::ObjectLinkV4 {
                    object: ctx.source,
                    zone_change_count: source.zone_change_count,
                },
                timestamp,
            });
        return;
    }
    if effect == CreatureEffectV1::EssenceTransferCounters {
        let Some(crate::state::Target::Object(target)) = ctx.targets.first().copied() else {
            return;
        };
        let Some(source) = ctx.ability_source_contract else {
            return;
        };
        if !ctx.target_contracts.first().is_some_and(|&contract| {
            crate::engine::target_contract_matches_live(state, ctx.targets[0], contract)
        }) || !crate::engine::effect_target_is_legal_from_ability_source(
            state,
            source,
            ctx.controller,
            crate::card_def::TargetSpec::ControlledCreature,
            &ctx.targets,
            0,
        ) {
            return;
        }
        let snapshot = counter_transfer_snapshot(state, source);
        let mut staged = state.clone();
        if apply_counter_transfer(&mut staged, target, ctx.controller, snapshot).is_none() {
            state.engine.halted = Some((
                crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
                ctx.source,
            ));
            return;
        }
        *state = staged;
        return;
    }
    if matches!(
        effect,
        CreatureEffectV1::FloodpitsTapStun | CreatureEffectV1::FloodpitsShuffle
    ) {
        let Some(crate::state::Target::Object(target)) = ctx.targets.first().copied() else {
            return;
        };
        let Some(source) = ctx.ability_source_contract else {
            return;
        };
        let spec = if effect == CreatureEffectV1::FloodpitsTapStun {
            crate::card_def::TargetSpec::OpponentControlledCreature
        } else {
            crate::card_def::TargetSpec::CreatureWithStunCounter
        };
        if !ctx.target_contracts.first().is_some_and(|&contract| {
            crate::engine::target_contract_matches_live(state, ctx.targets[0], contract)
        }) || !crate::engine::effect_target_is_legal_from_ability_source(
            state,
            source,
            ctx.controller,
            spec,
            &ctx.targets,
            0,
        ) {
            return;
        }
        if effect == CreatureEffectV1::FloodpitsTapStun {
            crate::event::propose_and_commit(state, crate::event::ProposedEvent::tap(target));
            #[cfg(feature = "standard-magezero-fixtures")]
            let count = crate::standard_cards_v1::scale_counters(
                state,
                state.objects.get(target).controller,
                1,
            );
            #[cfg(not(feature = "standard-magezero-fixtures"))]
            let count = 1;
            let count = i16::try_from(count)
                .ok()
                .and_then(|count| state.objects.get(target).counters.stun.checked_add(count));
            let Some(count) = count else {
                state.engine.halted = Some((
                    crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
                    ctx.source,
                ));
                return;
            };
            state.objects.get_mut(target).counters.stun = count;
            return;
        }
        let mut objects = vec![target];
        if target != ctx.source
            && state.objects.try_get(ctx.source).is_some_and(|object| {
                object.zone == Zone::Battlefield
                    && object.zone_change_count == source.zone_change_count
            })
        {
            objects.push(ctx.source);
        }
        let mut staged = state.clone();
        let mut owners: Vec<_> = objects
            .iter()
            .map(|id| staged.objects.get(*id).owner)
            .collect();
        owners.sort_by_key(|owner| owner.index());
        owners.dedup();
        crate::event::propose_and_commit_batch(
            &mut staged,
            objects
                .iter()
                .map(|&id| {
                    crate::event::ProposedEvent::public_library_insert(
                        id,
                        crate::event::LibraryPlacement::Top,
                    )
                })
                .collect(),
        );
        for owner in owners {
            if staged.shuffle_library(owner).is_err() {
                state.engine.halted = Some((
                    crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
                    ctx.source,
                ));
                return;
            }
        }
        *state = staged;
        return;
    }
    if effect == CreatureEffectV1::MosswoodGraveyardAdventure {
        let Some(contract) = ctx.ability_source_contract else {
            return;
        };
        let Some(object) = state.objects.try_get(ctx.source) else {
            return;
        };
        if object.zone == Zone::Graveyard
            && contract.zone_change_count.checked_add(1) == Some(object.zone_change_count)
        {
            state
                .objects
                .get_mut(ctx.source)
                .v4
                .creature_upgrade
                .get_or_insert_with(Default::default)
                .graveyard_adventure = Some(GraveyardAdventurePermissionV1 {
                holder: ctx.controller,
                holder_turn_started: false,
            });
        }
        return;
    }
    if matches!(
        effect,
        CreatureEffectV1::SalvagerBoostTokens | CreatureEffectV1::VirtueCountersUntap
    ) {
        let objects: Vec<_> = state
            .objects
            .iter()
            .filter_map(|(object, live)| {
                (live.zone == Zone::Battlefield
                    && live.controller == ctx.controller
                    && (effect == CreatureEffectV1::VirtueCountersUntap || live.v4.is_token)
                    && crate::engine::object_has_type(
                        state,
                        object,
                        crate::card_def::CardType::Creature,
                    ))
                .then_some(crate::effect::EffectObjectBinding {
                    object,
                    expected_zone: Zone::Battlefield,
                    expected_zone_change_count: live.zone_change_count,
                })
            })
            .collect();
        for object in &objects {
            if crate::event::add_plus_one_counters(state, object.object, ctx.controller, 1).is_err()
            {
                state.engine.halted = Some((
                    crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
                    ctx.source,
                ));
                return;
            }
        }
        for object in objects {
            if effect == CreatureEffectV1::VirtueCountersUntap {
                state.objects.get_mut(object.object).tapped = false;
            } else {
                crate::effect::install_temporary_boost(state, object, 0, 0, Keywords::TRAMPLE);
            }
        }
        return;
    }
    if effect == CreatureEffectV1::HarvesterWeakening {
        let Some(crate::state::Target::Object(target)) = ctx.targets.first().copied() else {
            return;
        };
        if !ctx.target_contracts.first().is_some_and(|&contract| {
            crate::engine::target_contract_matches_live(state, ctx.targets[0], contract)
        }) || !ctx.ability_source_contract.is_some_and(|source| {
            crate::engine::effect_target_is_legal_from_ability_source(
                state,
                source,
                ctx.controller,
                crate::card_def::TargetSpec::Creature,
                &ctx.targets,
                0,
            )
        }) {
            return;
        }
        let count = state
            .objects
            .iter()
            .filter(|(id, object)| {
                object.zone == Zone::Battlefield
                    && object.controller == ctx.controller
                    && object.v4.is_token
                    && crate::engine::has_effective_subtype(state, *id, Subtype::Blood)
            })
            .count();
        let amount = i32::try_from(count)
            .unwrap_or(i32::MAX / 2)
            .saturating_mul(-2);
        let binding = crate::effect::EffectObjectBinding {
            object: target,
            expected_zone: Zone::Battlefield,
            expected_zone_change_count: state.objects.get(target).zone_change_count,
        };
        crate::effect::install_temporary_boost(state, binding, amount, amount, Keywords::NONE);
        return;
    }
    if effect == CreatureEffectV1::ToughCookieAnimate {
        let Some(crate::state::Target::Object(target)) = ctx.targets.first().copied() else {
            return;
        };
        if !ctx.target_contracts.first().is_some_and(|&contract| {
            crate::engine::target_contract_matches_live(state, ctx.targets[0], contract)
        }) || !ctx.ability_source_contract.is_some_and(|source| {
            crate::engine::effect_target_is_legal_from_ability_source(
                state,
                source,
                ctx.controller,
                crate::card_def::TargetSpec::ControlledNoncreatureArtifactPermanent,
                &ctx.targets,
                0,
            )
        }) {
            return;
        }
        let timestamp = crate::engine::next_timestamp(state);
        state
            .objects
            .get_mut(target)
            .v4
            .creature_upgrade
            .get_or_insert_with(Default::default)
            .temporary_creature = Some((4, 4, timestamp));
        return;
    }
    let Some(contract) = ctx.ability_source_contract else {
        return;
    };
    let Some(object) = state.objects.try_get(ctx.source) else {
        return;
    };
    if object.zone != Zone::Battlefield || object.zone_change_count != contract.zone_change_count {
        return;
    }
    if effect == CreatureEffectV1::WurmletCounterIfFirstResolution {
        let turn = (state.turn, state.active_player);
        let value = state
            .objects
            .get_mut(ctx.source)
            .v4
            .creature_upgrade
            .get_or_insert_with(Default::default);
        if value.wurmlet_resolved_turn == Some(turn) {
            return;
        }
        value.wurmlet_resolved_turn = Some(turn);
        if crate::event::add_plus_one_counters(state, ctx.source, ctx.controller, 1).is_err() {
            state.engine.halted = Some((
                crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
                ctx.source,
            ));
        }
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
        CreatureEffectV1::TidebinderCounter => unreachable!("counter ability handled above"),
        CreatureEffectV1::EssenceTransferCounters => unreachable!("counter transfer handled above"),
        CreatureEffectV1::FloodpitsTapStun | CreatureEffectV1::FloodpitsShuffle => {
            unreachable!("targeted effect handled above")
        }
        CreatureEffectV1::MosswoodGraveyardAdventure => {
            unreachable!("graveyard permission handled above")
        }
        CreatureEffectV1::WurmletCounterIfFirstResolution => {
            unreachable!("resolution marker handled above")
        }
        CreatureEffectV1::HarvesterWeakening
        | CreatureEffectV1::SalvagerBoostTokens
        | CreatureEffectV1::VirtueCountersUntap => {
            unreachable!("independent effect handled above")
        }
        CreatureEffectV1::ToughCookieAnimate => unreachable!("targeted animation handled above"),
        CreatureEffectV1::GingerEvasion => value.haste_blockers_only = true,
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
