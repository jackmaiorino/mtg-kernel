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
//! Crimes (Outlaws of Thunder Junction, 700.13) are logged as
//! `CommittedEvent::CrimeCommitted` when a spell, activated ability or
//! triggered ability finishes targeting (`engine::log_final_targeting_events`).

use crate::card_def::{Keywords, CARD_DEFS};
use crate::event::CommittedEvent;
use crate::ids::{ObjectId, PlayerId};
use crate::state::{CreatureDeathTurnV1, GameState, SpeedV1, StackItem, Target, Zone};

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
pub(crate) fn cant_block(state: &GameState, id: ObjectId) -> bool {
    CARD_DEFS
        .get(usize::from(state.objects.get(id).card_def))
        .is_some_and(|def| def.name == "Forsaken Miner")
        && crate::continuous_characteristics_v1::printed_abilities_active(state, id)
}
