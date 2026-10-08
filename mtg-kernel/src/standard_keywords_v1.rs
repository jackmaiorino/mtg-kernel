//! Rules for the newer keyword mechanics of the MageZero Standard pool
//! (`standard-magezero-fixtures`) that are player- or game-scoped rather
//! than printed on one triggered or activated ability.
//!
//! Speed (Aetherdrift): "Start your engines!" is a static ability that gives
//! its controller speed 1 if they have none. Each player with speed then has
//! the inherent triggered ability "Whenever one or more opponents lose life
//! during your turn, if your speed is less than 4, increase your speed by 1.
//! This ability triggers only once each turn." This kernel applies that
//! increase directly when the life-loss event batch is processed instead of
//! putting the inherent trigger on the stack: nothing in the pool can respond
//! to it or observe the difference, and it keeps speed free of a source
//! object. "Max speed" abilities read `speed == 4`.
//!
//! Day and night (726): the designation starts when a daybound permanent
//! appears and changes as each turn begins. Daybound/nightbound permanents
//! are brought in line with it at the state-check point (`sync_day_night`)
//! rather than at the instant the designation changes; a daybound card
//! entering at night enters transformed (`enters_transformed_at_night`).
//!
//! Crimes (Outlaws of Thunder Junction, 700.13) are logged as
//! `CommittedEvent::CrimeCommitted` when a spell, activated ability or
//! triggered ability finishes targeting (`engine::log_final_targeting_events`).

use crate::card_def::{CardType, Keywords, CARD_DEFS};
use crate::event::CommittedEvent;
use crate::ids::{ObjectId, PlayerId};
use crate::mana::{Cost, PaymentPlan, Pip};
use crate::state::{
    CreatureDeathTurnV1, DayNightV1, DescendedTurnV1, GameState, SpeedV1, StackItem, Target, Zone,
};

/// Definitions with "Start your engines!".
fn has_start_your_engines(card_def: u16) -> bool {
    CARD_DEFS
        .get(usize::from(card_def))
        .is_some_and(|def| def.name == "Burnout Bashtronaut")
}

/// The player's current speed; zero means the player has no speed.
pub fn speed(state: &GameState, player: PlayerId) -> u8 {
    state
        .speed_v1
        .map_or(0, |speed| speed.speeds[player.index()])
}

/// State-check half of "Start your engines!": any player who controls a
/// permanent with it and has no speed gets speed 1. Idempotent.
pub(crate) fn start_your_engines(state: &mut GameState) {
    for player in [PlayerId::P0, PlayerId::P1] {
        if speed(state, player) != 0 {
            continue;
        }
        let starts = state.players[player.index()].battlefield.iter().any(|&id| {
            has_start_your_engines(state.objects.get(id).card_def)
                && crate::continuous_characteristics_v1::printed_abilities_active(state, id)
        });
        if starts {
            let speed = state.speed_v1.get_or_insert(SpeedV1 {
                speeds: [0; 2],
                last_increase: None,
            });
            speed.speeds[player.index()] = 1;
        }
    }
}

fn opponent_lost_life(events: &[CommittedEvent], active: PlayerId) -> bool {
    let opponent = active.opponent();
    events.iter().any(|event| match event {
        CommittedEvent::LifeLoss { player, amount } => *player == opponent && *amount > 0,
        CommittedEvent::Damage { target, amount, .. } => {
            *target == Target::Player(opponent) && *amount > 0
        }
        _ => false,
    })
}

/// Speed's once-per-turn increase for the active player, applied to one
/// committed event batch.
pub(crate) fn note_life_loss(state: &mut GameState, events: &[CommittedEvent]) {
    let Some(current) = state.speed_v1 else {
        return;
    };
    let active = state.active_player;
    let stamp = CreatureDeathTurnV1 {
        turn: state.turn,
        active_player: active,
    };
    let player_speed = current.speeds[active.index()];
    if player_speed == 0
        || player_speed >= SpeedV1::MAX
        || current.last_increase == Some(stamp)
        || !opponent_lost_life(events, active)
    {
        return;
    }
    let speed = state.speed_v1.as_mut().expect("checked above");
    speed.speeds[active.index()] += 1;
    speed.last_increase = Some(stamp);
}

/// Keywords a battlefield permanent has from its own "Max speed" ability.
pub(crate) fn max_speed_keywords(state: &GameState, id: ObjectId) -> Keywords {
    let object = state.objects.get(id);
    if object.zone == Zone::Battlefield
        && has_start_your_engines(object.card_def)
        && speed(state, object.controller) >= SpeedV1::MAX
    {
        Keywords::DOUBLE_STRIKE
    } else {
        Keywords::NONE
    }
}

/// 700.13: whether a stack item's final targets make its controller commit
/// a crime: an opponent, a permanent or spell an opponent controls, or a
/// card in an opponent's graveyard.
pub(crate) fn targets_commit_crime(state: &GameState, item: &StackItem) -> bool {
    let controller = item.controller;
    item.targets.iter().any(|target| match *target {
        Target::Player(player) => player != controller,
        Target::Object(id) => {
            let object = state.objects.get(id);
            match object.zone {
                Zone::Battlefield | Zone::Stack => object.controller != controller,
                Zone::Graveyard => object.owner != controller,
                _ => false,
            }
        }
    })
}

/// Printed "This creature can't block."
/// A creature card's permanent that currently isn't a creature: impending
/// with time counters, or an Enduring card returned as an enchantment.
pub(crate) fn not_a_creature(state: &GameState, id: ObjectId) -> bool {
    state
        .objects
        .try_get(id)
        .is_some_and(|object| object.v4.time_counters_v1 > 0 || object.v4.enduring_enchantment_v1)
}

fn is_enduring(card_def: u16) -> bool {
    CARD_DEFS
        .get(usize::from(card_def))
        .is_some_and(|def| matches!(def.name, "Enduring Curiosity" | "Enduring Innocence"))
}

/// Whether `object` was a creature as it last left the battlefield (logged
/// for Enduring cards' "if it was a creature").
pub(crate) fn was_creature_before_leaving(
    state: &GameState,
    object: ObjectId,
    zone_change_count: u32,
) -> bool {
    state.engine.event_history.iter().rev().any(|event| {
        matches!(
            event,
            CommittedEvent::WasCreatureBeforeLeavingBattlefield {
                object: logged,
                zone_change_count: logged_count,
            } if *logged == object && *logged_count == zone_change_count
        )
    })
}

pub(crate) fn cant_block(state: &GameState, id: ObjectId) -> bool {
    not_a_creature(state, id)
        || CARD_DEFS
            .get(usize::from(state.objects.get(id).card_def))
            .is_some_and(|def| def.name == "Forsaken Miner")
            && crate::continuous_characteristics_v1::printed_abilities_active(state, id)
}

/// The graveyard cards a player exiles to collect evidence `minimum`, or
/// `None` when their graveyard's total mana value falls short. Among the
/// subsets that reach the minimum it picks the smallest total, then the
/// fewest cards, then the earliest-found subset in graveyard order.
pub(crate) fn evidence_plan(
    state: &GameState,
    player: PlayerId,
    minimum: u16,
) -> Option<Vec<ObjectId>> {
    let cards: Vec<(ObjectId, usize)> = state.players[player.index()]
        .graveyard
        .iter()
        .map(|&id| {
            let mana_value = CARD_DEFS[usize::from(state.objects.get(id).card_def)].mana_value;
            (id, usize::from(mana_value))
        })
        .collect();
    let minimum = usize::from(minimum);
    let cap = minimum + cards.iter().map(|&(_, mv)| mv).max().unwrap_or(0);
    // best[sum] = fewest-card subset reaching exactly `sum` (sums above the
    // cap can never be the smallest total that reaches the minimum).
    let mut best: Vec<Option<Vec<usize>>> = vec![None; cap + 1];
    best[0] = Some(Vec::new());
    for (index, &(_, mana_value)) in cards.iter().enumerate() {
        for sum in (0..=cap.saturating_sub(mana_value)).rev() {
            let Some(base) = best[sum].as_ref() else {
                continue;
            };
            let next = sum + mana_value;
            if best[next]
                .as_ref()
                .is_none_or(|current| current.len() > base.len() + 1)
            {
                let mut chosen = base.clone();
                chosen.push(index);
                best[next] = Some(chosen);
            }
        }
    }
    let chosen = best.into_iter().skip(minimum).flatten().next()?;
    Some(chosen.into_iter().map(|index| cards[index].0).collect())
}

/// Daybound (front face) / nightbound (back face) transforming cards.
fn is_daybound(card_def: u16) -> bool {
    CARD_DEFS
        .get(usize::from(card_def))
        .is_some_and(|def| matches!(def.name, "Brutal Cathar" | "Graveyard Trespasser"))
}

/// 726.2: as a turn begins, day becomes night if the previous turn's active
/// player cast no spells that turn, and night becomes day if they cast two
/// or more. Called before the untap step resets spell counts.
pub(crate) fn advance_day_night(state: &mut GameState, previous_active: PlayerId) {
    let cast = state.players[previous_active.index()].spells_cast_this_turn;
    state.day_night_v1 = match state.day_night_v1 {
        Some(DayNightV1::Day) if cast == 0 => Some(DayNightV1::Night),
        Some(DayNightV1::Night) if cast >= 2 => Some(DayNightV1::Day),
        other => other,
    };
}

/// 726.2/702.145: it becomes day if a daybound permanent is on the
/// battlefield and it's neither; then daybound permanents show their front
/// face by day and their back face by night.
pub(crate) fn sync_day_night(state: &mut GameState) {
    let daybound: Vec<ObjectId> = state
        .objects
        .iter()
        .filter(|(_, object)| object.zone == Zone::Battlefield && is_daybound(object.card_def))
        .map(|(id, _)| id)
        .collect();
    if daybound.is_empty() {
        return;
    }
    let designation = *state.day_night_v1.get_or_insert(DayNightV1::Day);
    let face = u8::from(designation == DayNightV1::Night);
    for id in daybound {
        if state.objects.get(id).v4.face_index != face {
            crate::event::propose_and_commit(
                state,
                crate::event::ProposedEvent::transform_in_place(id, face),
            );
        }
    }
}

/// A daybound card entering the battlefield at night enters transformed.
pub(crate) fn enters_transformed_at_night(state: &GameState, card_def: u16) -> bool {
    state.day_night_v1 == Some(DayNightV1::Night) && is_daybound(card_def)
}

/// Convoke's deterministic payment: the untapped creatures `player`
/// controls that pay part of `cost`, and the mana plan for the rest. Uses as
/// many creatures as possible, summoning-sick ones first, then battlefield
/// order. A creature pays a colored pip only when creatures of the right
/// colors can cover every colored pip (the cost's pips stay whole or go
/// entirely); otherwise creatures pay generic mana only. Returns `None`
/// unless at least one creature convokes and the remainder is payable.
pub(crate) fn convoke_plan(
    cost: &Cost,
    player: PlayerId,
    state: &GameState,
) -> Option<(Vec<ObjectId>, PaymentPlan)> {
    let mut creatures: Vec<ObjectId> = state.players[player.index()]
        .battlefield
        .iter()
        .copied()
        .filter(|&id| {
            let object = state.objects.get(id);
            object.controller == player
                && !object.tapped
                && crate::engine::object_has_type(state, id, crate::card_def::CardType::Creature)
        })
        .collect();
    creatures.sort_by_key(|&id| !state.objects.get(id).summoning_sick);

    // Try covering every colored pip with a creature of that color.
    let mut pip_payers = Vec::new();
    let mut available = creatures.clone();
    let all_colored = cost.pips.iter().all(|pip| matches!(pip, Pip::Colored(_)));
    if all_colored {
        for pip in cost.pips {
            let Pip::Colored(color) = pip else {
                unreachable!("checked above")
            };
            let mask = crate::card_def::mana_color_mask(*color);
            let Some(position) = available
                .iter()
                .position(|&id| state.objects.get(id).v4.effective_color_mask & mask != 0)
            else {
                pip_payers.clear();
                available = creatures.clone();
                break;
            };
            pip_payers.push(available.remove(position));
        }
    }
    let pips_paid = !cost.pips.is_empty() && pip_payers.len() == cost.pips.len();
    let generic_payers = usize::from(cost.generic).min(available.len());
    for generic_paid in (0..=generic_payers).rev() {
        let mut tapped = if pips_paid {
            pip_payers.clone()
        } else {
            Vec::new()
        };
        tapped.extend_from_slice(&available[..generic_paid]);
        if tapped.is_empty() {
            return None;
        }
        let remaining = Cost {
            pips: if pips_paid { &[] } else { cost.pips },
            generic: cost.generic - generic_paid as u8,
            x_count: cost.x_count,
        };
        if let Some(plan) =
            crate::mana::can_pay_excluding_sources(&remaining, 0, player, state, &tapped)
        {
            return Some((tapped, plan));
        }
    }
    None
}

/// Cards whose leave-the-battlefield triggers read their last-known power.
fn needs_power_lki(card_def: u16) -> bool {
    CARD_DEFS
        .get(usize::from(card_def))
        .is_some_and(|def| def.name == "Heartfire Hero")
}

/// Bookkeeping just before `object` changes zones: last-known power for
/// leave triggers, and descend (700.14) for a permanent card going to its
/// owner's graveyard.
pub(crate) fn before_zone_change(state: &mut GameState, object: ObjectId, to: Zone) {
    let live = state.objects.get(object);
    if live.zone == Zone::Battlefield && needs_power_lki(live.card_def) {
        let marker = CommittedEvent::PowerBeforeLeavingBattlefield {
            object,
            zone_change_count: live.zone_change_count,
            power: crate::engine::effective_power(state, object),
        };
        state.engine.event_log.push(marker.clone());
        state.engine.event_history.push(marker);
    }
    let live = state.objects.get(object);
    if live.zone == Zone::Battlefield
        && is_enduring(live.card_def)
        && crate::engine::object_has_type(state, object, CardType::Creature)
    {
        let marker = CommittedEvent::WasCreatureBeforeLeavingBattlefield {
            object,
            zone_change_count: live.zone_change_count,
        };
        state.engine.event_log.push(marker.clone());
        state.engine.event_history.push(marker);
    }
    let live = state.objects.get(object);
    let def = &CARD_DEFS[usize::from(live.card_def)];
    let permanent_card = [
        CardType::Artifact,
        CardType::Creature,
        CardType::Enchantment,
        CardType::Land,
        CardType::Planeswalker,
    ]
    .into_iter()
    .any(|card_type| def.has_type(card_type));
    if to == Zone::Graveyard && !def.is_token && permanent_card {
        let owner = live.owner;
        let stamp = CreatureDeathTurnV1 {
            turn: state.turn,
            active_player: state.active_player,
        };
        let mut descended = state
            .descended_v1
            .filter(|descended| descended.turn == stamp)
            .unwrap_or(DescendedTurnV1 {
                turn: stamp,
                players: [false; 2],
            });
        descended.players[owner.index()] = true;
        state.descended_v1 = Some(descended);
    }
}

/// Whether `player` descended this turn.
pub(crate) fn descended_this_turn(state: &GameState, player: PlayerId) -> bool {
    state.descended_v1.is_some_and(|descended| {
        descended.turn
            == CreatureDeathTurnV1 {
                turn: state.turn,
                active_player: state.active_player,
            }
            && descended.players[player.index()]
    })
}

/// The last-known power logged for `object` as it left the battlefield.
pub(crate) fn power_before_leaving(state: &GameState, object: ObjectId) -> Option<i32> {
    state
        .engine
        .event_history
        .iter()
        .rev()
        .find_map(|event| match event {
            CommittedEvent::PowerBeforeLeavingBattlefield {
                object: logged,
                power,
                ..
            } if *logged == object => Some(*power),
            _ => None,
        })
}

/// Unearth: "If it would leave the battlefield, exile it instead of putting
/// it anywhere else." Applied as every zone change commits, so state-based
/// deaths and sacrifices are redirected too.
pub(crate) fn unearth_exile_instead(
    state: &GameState,
    change: &mut crate::event::ZoneChangeProposed,
) {
    if change.to_zone != Zone::Exile
        && state
            .objects
            .try_get(change.object)
            .is_some_and(|object| object.zone == Zone::Battlefield && object.v4.unearthed_v1)
    {
        change.to_zone = Zone::Exile;
    }
}
