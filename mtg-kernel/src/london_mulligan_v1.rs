//! Two-player London mulligans, before any turn-based action or priority.
//!
//! Each round collects every announcement, redraws the mulliganing hands,
//! and bottoms each player's current mulligan count before asking again.

use crate::engine::{Action, Decision};
use crate::event::{self, ProposedEvent};
use crate::ids::PlayerId;
use crate::state::{GameState, ObjectLinkV4, Step, Zone};
use serde::{Deserialize, Serialize};

const HAND_SIZE: u8 = 7;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
enum StageV1 {
    Announce { next: u8 },
    Bottom { next: u8, remaining: u8 },
    Complete,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LondonMulligansV1 {
    starting_player: PlayerId,
    pub(crate) counts: [u8; 2],
    pub(crate) kept: [bool; 2],
    mulligan_this_round: [bool; 2],
    hands: [Vec<ObjectLinkV4>; 2],
    stage: StageV1,
}

impl LondonMulligansV1 {
    pub fn is_complete(&self) -> bool {
        self.stage == StageV1::Complete
    }

    pub fn counts(&self) -> [u8; 2] {
        self.counts
    }

    pub fn kept(&self) -> [bool; 2] {
        self.kept
    }

    pub(crate) fn phase(&self) -> &'static str {
        match self.stage {
            StageV1::Announce { .. } => "announce",
            StageV1::Bottom { .. } => "bottom",
            StageV1::Complete => "complete",
        }
    }

    fn player(&self, index: u8) -> PlayerId {
        if index == 0 {
            self.starting_player
        } else {
            self.starting_player.opponent()
        }
    }
}

fn hand_bindings(state: &GameState, player: PlayerId) -> Vec<ObjectLinkV4> {
    state.players[player.index()]
        .hand
        .iter()
        .map(|&object| ObjectLinkV4 {
            object,
            zone_change_count: state.objects.get(object).zone_change_count,
        })
        .collect()
}

/// Opt in after the normal initial seven event draws, before engine advance.
/// This preserves the initial-deal identity of all existing reset modes.
pub fn enable_london_mulligans_v1(state: &mut GameState) -> Result<(), String> {
    if state.london_mulligans_v1.is_some()
        || !matches!(state.starting_player, PlayerId::P0 | PlayerId::P1)
        || state.turn != 1
        || state.step != Step::Untap
        || state.active_player != state.starting_player
        || !state.stack.is_empty()
        || !state.exile.is_empty()
        || !state.command.is_empty()
        || state.players.iter().any(|player| {
            player.hand.len() != usize::from(HAND_SIZE)
                || !player.battlefield.is_empty()
                || !player.graveyard.is_empty()
                || player.has_lost
        })
    {
        return Err("London mulligans require a fresh seven-card opening deal".into());
    }
    let hands = [PlayerId::P0, PlayerId::P1].map(|player| hand_bindings(state, player));
    state.london_mulligans_v1 = Some(LondonMulligansV1 {
        starting_player: state.starting_player,
        counts: [0; 2],
        kept: [false; 2],
        mulligan_this_round: [false; 2],
        hands,
        stage: StageV1::Announce { next: 0 },
    });
    Ok(())
}

pub(crate) fn has_pending(state: &GameState) -> bool {
    state
        .london_mulligans_v1
        .as_ref()
        .is_some_and(|pregame| !pregame.is_complete())
}

fn validate(state: &GameState) -> Result<(), String> {
    let pregame = state
        .london_mulligans_v1
        .as_ref()
        .ok_or("no London mulligan is pending")?;
    if pregame.starting_player != state.starting_player
        || !matches!(pregame.starting_player, PlayerId::P0 | PlayerId::P1)
        || pregame.counts.iter().any(|&count| count > HAND_SIZE)
        || state.step != Step::Untap
        || state.turn != 1
    {
        return Err("invalid London mulligan continuation".into());
    }
    for player in [PlayerId::P0, PlayerId::P1] {
        let hand = &state.players[player.index()].hand;
        let bindings = &pregame.hands[player.index()];
        if hand.len() != bindings.len()
            || hand.iter().zip(bindings).any(|(&id, binding)| {
                id != binding.object
                    || !state.objects.try_get(id).is_some_and(|object| {
                        object.zone == Zone::Hand
                            && object.owner == player
                            && object.zone_change_count == binding.zone_change_count
                    })
            })
        {
            return Err("London mulligan hand no longer matches its exact binding".into());
        }
    }
    Ok(())
}

pub(crate) fn drain_or_decide(state: &mut GameState) -> Result<Option<Decision>, String> {
    while has_pending(state) {
        validate(state)?;
        let pregame = state.london_mulligans_v1.as_ref().unwrap();
        match pregame.stage {
            StageV1::Announce { next } if next < 2 => {
                let player = pregame.player(next);
                if pregame.kept[player.index()] || pregame.counts[player.index()] == HAND_SIZE {
                    let pregame = state.london_mulligans_v1.as_mut().unwrap();
                    pregame.kept[player.index()] = true;
                    pregame.stage = StageV1::Announce { next: next + 1 };
                    continue;
                }
                return Ok(Some(Decision::ChooseLondonMulligan {
                    player,
                    mulligan_count: pregame.counts[player.index()],
                }));
            }
            StageV1::Announce { next: 2 } => {
                if pregame.kept.iter().all(|&kept| kept) {
                    state.london_mulligans_v1.as_mut().unwrap().stage = StageV1::Complete;
                    return Ok(None);
                }
                // All announcements are complete before any hand changes.
                let players = [pregame.player(0), pregame.player(1)];
                for player in players {
                    if !state
                        .london_mulligans_v1
                        .as_ref()
                        .unwrap()
                        .mulligan_this_round[player.index()]
                    {
                        continue;
                    }
                    let hand = state.players[player.index()].hand.clone();
                    for card in hand {
                        event::propose_and_commit(
                            state,
                            ProposedEvent::zone_change(card, Zone::Library),
                        );
                    }
                    state
                        .shuffle_library(player)
                        .map_err(|error| format!("mulligan shuffle failed: {error:?}"))?;
                    for _ in 0..HAND_SIZE {
                        event::propose_and_commit(state, ProposedEvent::draw(player));
                    }
                    let bindings = hand_bindings(state, player);
                    let pregame = state.london_mulligans_v1.as_mut().unwrap();
                    pregame.counts[player.index()] += 1;
                    pregame.hands[player.index()] = bindings;
                }
                state.london_mulligans_v1.as_mut().unwrap().stage = StageV1::Bottom {
                    next: 0,
                    remaining: 0,
                };
            }
            StageV1::Bottom { next, remaining } if next < 2 => {
                let player = pregame.player(next);
                if !pregame.mulligan_this_round[player.index()] {
                    state.london_mulligans_v1.as_mut().unwrap().stage = StageV1::Bottom {
                        next: next + 1,
                        remaining: 0,
                    };
                    continue;
                }
                let remaining = if remaining == 0 {
                    pregame.counts[player.index()]
                } else {
                    remaining
                };
                state.london_mulligans_v1.as_mut().unwrap().stage =
                    StageV1::Bottom { next, remaining };
                return Ok(Some(Decision::ChooseLondonBottom {
                    player,
                    remaining,
                    candidates: state.players[player.index()].hand.clone(),
                }));
            }
            StageV1::Bottom { next: 2, .. } => {
                let pregame = state.london_mulligans_v1.as_mut().unwrap();
                pregame.mulligan_this_round = [false; 2];
                pregame.stage = StageV1::Announce { next: 0 };
            }
            _ => return Err("invalid London mulligan stage".into()),
        }
    }
    Ok(None)
}

pub(crate) fn answer(state: &mut GameState, action: Action) -> Result<(), String> {
    validate(state)?;
    let pregame = state.london_mulligans_v1.as_ref().unwrap();
    match (pregame.stage.clone(), action) {
        (StageV1::Announce { next }, Action::ChooseLondonMulligan { mulligan }) if next < 2 => {
            let player = pregame.player(next);
            if pregame.kept[player.index()] || pregame.counts[player.index()] >= HAND_SIZE {
                return Err("this player cannot take another mulligan".into());
            }
            let pregame = state.london_mulligans_v1.as_mut().unwrap();
            pregame.kept[player.index()] = !mulligan;
            pregame.mulligan_this_round[player.index()] = mulligan;
            pregame.stage = StageV1::Announce { next: next + 1 };
            Ok(())
        }
        (StageV1::Bottom { next, remaining }, Action::ChooseLondonBottom(card))
            if next < 2 && remaining > 0 =>
        {
            let player = pregame.player(next);
            if !state.players[player.index()].hand.contains(&card) {
                return Err("bottom choice is not a card in the acting player's hand".into());
            }
            event::propose_and_commit(state, ProposedEvent::private_bottom_library_insert(card));
            let bindings = hand_bindings(state, player);
            let pregame = state.london_mulligans_v1.as_mut().unwrap();
            pregame.hands[player.index()] = bindings;
            pregame.stage = if remaining == 1 {
                StageV1::Bottom {
                    next: next + 1,
                    remaining: 0,
                }
            } else {
                StageV1::Bottom {
                    next,
                    remaining: remaining - 1,
                }
            };
            Ok(())
        }
        _ => Err("action does not answer the current London mulligan decision".into()),
    }
}
