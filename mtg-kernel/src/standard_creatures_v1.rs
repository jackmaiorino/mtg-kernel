//! Persistent, incarnation-bound creature upgrades used by Standard cards.

use serde::{Deserialize, Serialize};

use crate::card_def::{Keywords, Subtype};
use crate::effect::ExecCtx;
use crate::ids::ObjectId;
use crate::state::{GameState, Zone};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CreatureUpgradeV1 {
    pub finality: u16,
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
    /// The activation that granted combat impulse, retained when a copy ends.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub combat_impulse_source: Option<crate::state::AbilitySourceContractV4>,
    /// The exiled card supplying that activation when it was borrowed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub combat_impulse_donor: Option<crate::state::AbilitySourceContractV4>,
}

impl std::hash::Hash for CreatureUpgradeV1 {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.finality.hash(state);
        self.temporary_creature.hash(state);
        self.suppressed_by.hash(state);
        self.graveyard_adventure.hash(state);
        self.haste_blockers_only.hash(state);
        self.creature_types.hash(state);
        self.base_stats.hash(state);
        self.color.hash(state);
        self.keyword_grants.hash(state);
        self.keyword_losses.hash(state);
        self.combat_impulse.hash(state);
        self.once_activated.hash(state);
        self.wurmlet_resolved_turn.hash(state);
        if let Some(source) = self.combat_impulse_source {
            "combat_impulse_source_v1".hash(state);
            source.hash(state);
        }
        if let Some(donor) = self.combat_impulse_donor {
            "combat_impulse_donor_v1".hash(state);
            donor.hash(state);
        }
    }
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
    FinalityReturnedTarget,
    LibrarianWerewolfAndCounters,
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
    #[serde(default)]
    pub loyalty: u32,
    pub lifelink: i16,
    pub time: u8,
    #[serde(default)]
    pub finality: u16,
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
        crate::doubling_season_v48::counter_amount(
            state,
            target,
            player,
            crate::doubling_season_v48::CounterOriginV48::Effect,
            amount,
        )
        .ok()
    };
    let placed_i16 = |amount: i16| i16::try_from(scale(i32::from(amount))?).ok();
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
    let charge = live.counters.charge.checked_add(scale(counters.charge)?)?;
    let net = live.counters.net.checked_add(scale(counters.net)?)?;
    let bait = live.counters.bait.checked_add(scale(counters.bait)?)?;
    let incubation_scaled =
        crate::standard_cards_v1::scale_counters_checked_v48(state, player, counters.incubation)
            .ok()?;
    let incubation_placed =
        crate::doubling_season_v48::ReplacementInputV48::capture(state, live.controller)
            .replaced_count(incubation_scaled)
            .ok()?;
    let incubation = live.counters.incubation.checked_add(incubation_placed)?;
    let soul_scaled =
        crate::standard_cards_v1::scale_counters_checked_v48(state, player, counters.soul).ok()?;
    let soul_placed =
        crate::doubling_season_v48::ReplacementInputV48::capture(state, live.controller)
            .replaced_count(soul_scaled)
            .ok()?;
    let soul = live.counters.soul.checked_add(soul_placed)?;
    let loyalty = snapshot.extras.loyalty;
    if loyalty > 0
        && (crate::doubling_season_v48::requires_successor(state) || loyalty > i32::MAX as u32)
    {
        crate::planeswalker_v1::quote_loyalty_counters_v48(state, target, loyalty).ok()?;
    }
    let lifelink = live
        .v4
        .lifelink_keyword_counters
        .checked_add(placed_i16(snapshot.extras.lifelink)?)?;
    let time = live
        .v4
        .time_counters_v1
        .checked_add(u8::try_from(scale(i32::from(snapshot.extras.time))?).ok()?)?;
    let finality = live
        .v4
        .creature_upgrade
        .as_ref()
        .map_or(0, |u| u.finality)
        .checked_add(u16::try_from(scale(i32::from(snapshot.extras.finality))?).ok()?)?;
    let bait_input = crate::doubling_season_v48::record_bait_transfer_input(
        state,
        target,
        player,
        counters.bait,
    )
    .ok()?;
    crate::event::add_plus_one_counters(state, target, player, counters.plus1_plus1).ok()?;
    let keyword_timestamp =
        (snapshot.extras.lifelink > 0).then(|| crate::engine::next_timestamp(state));
    let live = state.objects.get_mut(target);
    live.counters.minus1_minus1 = minus_one;
    live.counters.minus0_minus1 = minus_toughness;
    live.counters.stun = stun;
    live.counters.lore = lore;
    live.counters.oil = oil;
    live.counters.charge = charge;
    live.counters.net = net;
    live.counters.bait = bait;
    live.counters.soul = soul;
    live.counters.incubation = incubation;
    live.v4.lifelink_keyword_counters = lifelink;
    live.v4.time_counters_v1 = time;
    if finality > 0 {
        live.v4
            .creature_upgrade
            .get_or_insert_with(Default::default)
            .finality = finality;
    }
    if keyword_timestamp.is_some() {
        live.v4.lifelink_counter_timestamp = keyword_timestamp;
    }
    crate::planeswalker_v1::add_loyalty_counters_u32_v48(state, target, loyalty).ok()?;
    crate::doubling_season_v48::record_bait_transfer_applied(state, bait_input).ok()?;
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
        // The frozen materialized changeling pool is not a classification of
        // every native creature subtype. Preserve noncreature and unknown IDs.
        .filter(|value| {
            !Subtype::from_stable_id(*value).is_some_and(Subtype::is_native_creature_type)
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

pub(crate) fn grants_combat_impulse(card_def: u16) -> bool {
    static TABLE: std::sync::OnceLock<Box<[bool]>> = std::sync::OnceLock::new();
    TABLE
        .get_or_init(|| {
            crate::card_def::CARD_DEFS
                .iter()
                .map(|def| {
                    def.activated_abilities.iter().any(|ability| {
                        (ability.effect)()
                            == crate::effect::EffectOp::CreatureUpgrade(
                                CreatureEffectV1::KellanDetective,
                            )
                    })
                })
                .collect()
        })
        .get(usize::from(card_def))
        .copied()
        .unwrap_or(false)
}

pub(crate) fn combat_impulse_grant(
    state: &GameState,
    id: ObjectId,
) -> Option<crate::state::AbilitySourceContractV4> {
    let object = state.objects.try_get(id)?;
    let upgrade = upgrade(state, id)?;
    let source = upgrade.combat_impulse_source?;
    let grant = upgrade.combat_impulse_donor.unwrap_or(source);
    (object.zone == Zone::Battlefield
        && source.source == id
        && source.zone == Zone::Battlefield
        && source.zone_change_count == object.zone_change_count
        && upgrade
            .combat_impulse_donor
            .is_none_or(|donor| donor.zone == Zone::Exile)
        && grants_combat_impulse(grant.card_def)
        && combat_impulse_active(state, id))
    .then_some(grant)
}

pub(crate) fn activation_allowed(state: &GameState, id: ObjectId, index: usize) -> bool {
    if !crate::mild_mannered_librarian_v1::activation_allowed(state, id, index) {
        return false;
    }
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
    crate::mild_mannered_librarian_v1::activation_paid(state, id, index);
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
    if effect == CreatureEffectV1::LibrarianWerewolfAndCounters {
        crate::mild_mannered_librarian_v1::resolve_source_upgrade(state, ctx);
        return;
    }
    if effect == CreatureEffectV1::FinalityReturnedTarget {
        let Some(crate::state::StackTargetContractV4::Object {
            object,
            zone: Zone::Graveyard,
            zone_change_count,
            ..
        }) = ctx.target_contracts.first().copied()
        else {
            return;
        };
        let Some(live) = state.objects.try_get(object) else {
            return;
        };
        if live.zone != Zone::Battlefield
            || zone_change_count.checked_add(1) != Some(live.zone_change_count)
        {
            return;
        }
        let amount = crate::doubling_season_v48::counter_amount(
            state,
            object,
            live.controller,
            crate::doubling_season_v48::CounterOriginV48::Effect,
            1,
        );
        let before = live
            .v4
            .creature_upgrade
            .as_ref()
            .map_or(0, |upgrade| upgrade.finality);
        let total = amount
            .ok()
            .and_then(|amount| u16::try_from(amount).ok())
            .and_then(|amount| before.checked_add(amount));
        if let Some(total) = total {
            state
                .objects
                .get_mut(object)
                .v4
                .creature_upgrade
                .get_or_insert_with(Default::default)
                .finality = total;
        } else if crate::doubling_season_v48::requires_successor(state) {
            state.engine.halted = Some((
                crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
                ctx.source,
            ));
        }
        return;
    }
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
            let count = match crate::doubling_season_v48::counter_amount(
                state,
                target,
                state.objects.get(target).controller,
                crate::doubling_season_v48::CounterOriginV48::Effect,
                1,
            ) {
                Ok(count) => count,
                Err(_) => {
                    state.engine.halted = Some((
                        crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
                        ctx.source,
                    ));
                    return;
                }
            };
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
                crate::engine::attempt_untap(state, object.object);
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
        let turn = (state.turn_key_v1(), state.active_player);
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
        CreatureEffectV1::FinalityReturnedTarget | CreatureEffectV1::TidebinderCounter => {
            unreachable!("counter ability handled above")
        }
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
        CreatureEffectV1::LibrarianWerewolfAndCounters => {
            unreachable!("Librarian source upgrade handled above")
        }
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
            value.combat_impulse_source = Some(contract);
            value.combat_impulse_donor = ctx.cauldron_grant.0.map(|grant| grant.donor);
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

#[cfg(test)]
mod fdn_kellan_subtype_boundary_tests {
    use super::*;
    use crate::ids::PlayerId;
    use crate::policy_observation_v6::tests::{put, ready_state};

    /// Registered Elf stand-in for the physical subtype-replacement boundary.
    /// This is not printed Kellan gameplay or card admission evidence.
    #[test]
    fn replacement_removes_scout_but_preserves_noncreature_subtypes_after_restore() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready_state();
            let source = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
            let object = state.objects.get_mut(source);
            object.v4.effective_subtype_ids =
                vec![Subtype::Scout.stable_id(), Subtype::Equipment.stable_id()];
            object.v4.creature_upgrade = Some(CreatureUpgradeV1 {
                creature_types: Some((
                    vec![Subtype::Human.stable_id(), Subtype::Detective.stable_id()],
                    1,
                )),
                ..Default::default()
            });
            let expected = vec![
                Subtype::Equipment.stable_id(),
                Subtype::Human.stable_id(),
                Subtype::Detective.stable_id(),
            ];
            assert_eq!(creature_types(&state, source), Some((expected.clone(), 1)));
            let mut restored: GameState =
                serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
            assert_eq!(creature_types(&restored, source), Some((expected, 1)));
            crate::event::propose_and_commit(
                &mut restored,
                crate::event::ProposedEvent::zone_change(source, Zone::Hand),
            );
            crate::event::propose_and_commit(
                &mut restored,
                crate::event::ProposedEvent::zone_change(source, Zone::Battlefield),
            );
            assert!(creature_types(&restored, source).is_none());
        }
    }
}

#[cfg(all(
    test,
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
mod doubling_season_transfer_tests_v48 {
    use super::*;
    use crate::{
        event::{self, ProposedEvent},
        ids::PlayerId,
        policy_observation_v6::tests::{put, ready_state_for},
        state::Zone,
    };
    #[test]
    fn soul_transfer_preserves_value_restore_and_overflow_refusal_both_seats() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready_state_for(actor);
            let target = put(&mut state, actor, "Forest", Zone::Battlefield);
            state.objects.get_mut(target).counters.soul = 2;
            let snapshot = CounterTransferV1 {
                counters: crate::state::Counters {
                    soul: 3,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut restored: GameState =
                serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
            for branch in [&mut state, &mut restored] {
                apply_counter_transfer(branch, target, actor, snapshot).unwrap();
                assert_eq!(branch.objects.get(target).counters.soul, 5);
                branch.objects.get_mut(target).counters.soul = i32::MAX;
                let before = branch.clone();
                assert!(apply_counter_transfer(branch, target, actor, snapshot).is_none());
                assert_eq!(*branch, before);
            }
            assert_eq!(state, restored);
        }
    }
    #[test]
    #[cfg(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    ))]
    fn actual_bait_transfer_original_world_balance_and_restore_refuse_counter_forgery_both_seats() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready_state_for(actor);
            let season = put(&mut state, actor, "Doubling Season", Zone::Hand);
            event::propose_and_commit(
                &mut state,
                ProposedEvent::zone_change(season, Zone::Battlefield),
            );
            let recipient = put(&mut state, actor, "Fishing Pole", Zone::Battlefield);
            let original = CounterTransferV1 {
                counters: crate::state::Counters {
                    bait: 1,
                    soul: 1,
                    ..Default::default()
                },
                ..Default::default()
            };
            // Actual shared counter-transfer primitive, not an invented Pole ability.
            for _ in 0..2 {
                apply_counter_transfer(&mut state, recipient, actor, original).unwrap();
            }
            assert_eq!(state.objects.get(recipient).counters.bait, 4);
            let other = put(&mut state, actor, "Forest", Zone::Battlefield);
            apply_counter_transfer(&mut state, other, actor, original).unwrap();
            assert_eq!(state.objects.get(other).counters.bait, 2);
            let mut zeroed = state.clone();
            zeroed.objects.get_mut(other).counters.bait = 0;
            let before = zeroed.clone();
            assert!(crate::engine::step(&mut zeroed, crate::engine::Action::Pass).is_err());
            assert_eq!(zeroed, before);
            crate::doubling_season_v48::validate_token_journals(&state).unwrap();
            crate::fishing_pole_v39::lifecycle::validate_state(&state).unwrap();
            for _ in 0..3 {
                state = serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
                crate::doubling_season_v48::validate_token_journals(&state).unwrap();
                crate::fishing_pole_v39::lifecycle::validate_state(&state).unwrap();
            }
            let mut changed = state.clone();
            let at = changed
                .engine
                .event_history
                .iter()
                .position(|e| matches!(e, event::CommittedEvent::BaitTransferAppliedV48 { .. }))
                .unwrap();
            changed.engine.event_history[at] = event::CommittedEvent::Tap { object: recipient };
            let before = changed.clone();
            assert!(crate::engine::step(&mut changed, crate::engine::Action::Pass).is_err());
            assert_eq!(changed, before);
            let mut changed = state.clone();
            changed.objects.get_mut(recipient).counters.bait = 0;
            let before = changed.clone();
            assert!(crate::engine::step(&mut changed, crate::engine::Action::Pass).is_err());
            assert_eq!(changed, before);
            let mut changed = state.clone();
            for event in &mut changed.engine.event_history {
                if let event::CommittedEvent::BaitTransferAppliedV48 { placement } = event {
                    placement.after -= 1;
                    break;
                }
            }
            let before = changed.clone();
            assert!(crate::engine::step(&mut changed, crate::engine::Action::Pass).is_err());
            assert_eq!(changed, before);
        }
    }

    #[test]
    fn genuine_unsigned_loyalty_transfer_preserves_full_width_both_seats() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready_state_for(actor);
            let target = put(&mut state, actor, "Forest", Zone::Battlefield);
            let original = (i32::MAX as u32) + 1;
            let snapshot = CounterTransferV1 {
                extras: CounterExtrasV1 {
                    loyalty: original,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut restored: GameState =
                serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
            for branch in [&mut state, &mut restored] {
                apply_counter_transfer(branch, target, actor, snapshot).unwrap();
                assert_eq!(
                    crate::planeswalker_v1::loyalty(branch, target),
                    Some(original)
                );
            }
            assert_eq!(state, restored);
        }
    }

    #[test]
    #[cfg(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    ))]
    fn actual_shared_multikind_transfer_doubles_each_kind_and_restores() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready_state_for(actor);
            let season = put(&mut state, actor, "Doubling Season", Zone::Hand);
            event::propose_and_commit(
                &mut state,
                ProposedEvent::zone_change(season, Zone::Battlefield),
            );
            let target = put(&mut state, actor, "Forest", Zone::Hand);
            event::propose_and_commit(
                &mut state,
                ProposedEvent::zone_change(target, Zone::Battlefield),
            );
            // Typed counter-transfer primitive, not a fabricated printed ability.
            let snapshot = CounterTransferV1 {
                counters: crate::state::Counters {
                    plus1_plus1: 1,
                    minus1_minus1: 1,
                    minus0_minus1: 1,
                    stun: 1,
                    lore: 1,
                    oil: 1,
                    charge: 1,
                    net: 1,
                    bait: 1,
                    soul: 1,
                    incubation: 1,
                },
                extras: CounterExtrasV1 {
                    loyalty: 1,
                    lifelink: 1,
                    time: 1,
                    finality: 1,
                },
            };
            let mut restored: crate::state::GameState =
                serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
            for branch in [&mut state, &mut restored] {
                apply_counter_transfer(branch, target, actor, snapshot).unwrap();
                let o = branch.objects.get(target);
                assert_eq!(o.counters.plus1_plus1, 2);
                assert_eq!(o.counters.minus1_minus1, 2);
                assert_eq!(o.counters.minus0_minus1, 2);
                assert_eq!(o.counters.stun, 2);
                assert_eq!(o.counters.lore, 2);
                assert_eq!(o.counters.oil, 2);
                assert_eq!(o.counters.charge, 2);
                assert_eq!(o.counters.net, 2);
                assert_eq!(o.counters.bait, 2);
                assert_eq!(o.counters.soul, 2);
                assert_eq!(o.counters.incubation, 2);
                assert_eq!(o.v4.lifelink_keyword_counters, 2);
                assert_eq!(o.v4.time_counters_v1, 2);
                assert_eq!(o.v4.creature_upgrade.as_ref().unwrap().finality, 2);
                assert_eq!(crate::planeswalker_v1::loyalty(branch, target), Some(2));
                crate::doubling_season_v48::validate_token_journals(branch).unwrap();
                crate::fishing_pole_v39::lifecycle::validate_state(branch).unwrap();
            }
            assert_eq!(state, restored);
            let original = (i32::MAX as u32) / 2 + 1;
            let second = put(&mut state, actor, "Forest", Zone::Battlefield);
            let native = CounterTransferV1 {
                extras: CounterExtrasV1 {
                    loyalty: original,
                    ..Default::default()
                },
                ..Default::default()
            };
            apply_counter_transfer(&mut state, second, actor, native).unwrap();
            assert_eq!(
                crate::planeswalker_v1::loyalty(&state, second),
                Some(original * 2)
            );
            let before = state.clone();
            let overflow = CounterTransferV1 {
                extras: CounterExtrasV1 {
                    loyalty: u32::MAX,
                    ..Default::default()
                },
                ..Default::default()
            };
            assert!(apply_counter_transfer(&mut state, second, actor, overflow).is_none());
            assert_eq!(state, before);
        }
    }
}
