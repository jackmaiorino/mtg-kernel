//! London mulligans before the first gameplay advance of a native human game.
//!
//! The human may repeatedly return seven cards, shuffle and draw seven, then
//! keep and bottom one card per mulligan in a chosen order. The other seat
//! explicitly keeps seven because no mulligan model has been trained here.
//! Human mulligans consume only that physical owner's checked environment-v2
//! library-shuffle ordinal. No seed, library order or other hand is projected.
//! Initial and replacement opening draws are pregame events. The Ready handoff
//! removes their turn counters and pending event log before gameplay begins.

use crate::event::{self, LibraryInsertVisibility, LibraryPlacement, ProposedEvent};
use crate::ids::PlayerId;
use crate::rl::PlayerSeatV1;
use crate::rl_session::{FastActorSessionV1, SessionDeckHashesV1, SessionDeckIdsV1};
use crate::state::{GameState, Step, Zone};
use serde::Serialize;
use std::collections::BTreeSet;

pub const HUMAN_OPENING_SCHEMA_V1: &str = "mtg-kernel-human-london-opening/v1";
pub const MODEL_MULLIGAN_POLICY_V1: &str = "keep-seven-untrained-mulligan-policy";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HumanOpeningPhaseV1 {
    Mulligan,
    Bottom,
    Ready,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct HumanOpeningCardV1 {
    pub index: u32,
    pub card_id: u16,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct HumanOpeningViewV1 {
    pub schema: &'static str,
    pub human_seat: PlayerSeatV1,
    pub starting_player: PlayerSeatV1,
    pub phase: HumanOpeningPhaseV1,
    pub mulligans_taken: u8,
    pub required_bottom: u8,
    pub hand: Vec<HumanOpeningCardV1>,
    pub can_mulligan: bool,
    pub can_keep: bool,
    pub opponent_hand_count: usize,
    pub opponent_has_kept: bool,
    pub model_mulligan_policy: &'static str,
}

/// The private state cannot be constructed from a user-provided game snapshot.
/// `view` is the only human projection; session construction consumes a Ready
/// opening and is the first operation permitted to advance gameplay.
pub struct HumanOpeningV1 {
    state: GameState,
    human_seat: PlayerId,
    phase: HumanOpeningPhaseV1,
    mulligans_taken: u8,
    episode_id: u64,
    max_physical_decisions: u64,
    max_policy_steps: u64,
    deck_ids: SessionDeckIdsV1,
    deck_hashes: SessionDeckHashesV1,
}

pub(crate) struct ReadyHumanOpeningPartsV1 {
    pub(crate) state: GameState,
    pub(crate) episode_id: u64,
    pub(crate) max_physical_decisions: u64,
    pub(crate) max_policy_steps: u64,
    pub(crate) deck_ids: SessionDeckIdsV1,
    pub(crate) deck_hashes: SessionDeckHashesV1,
}

impl HumanOpeningV1 {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        episode_id: u64,
        pair_environment_seed: u64,
        max_physical_decisions: u64,
        max_policy_steps: u64,
        deck_ids: SessionDeckIdsV1,
        mainboards: [Vec<u16>; 2],
        starting_player: PlayerId,
        human_seat: PlayerId,
    ) -> Result<Self, String> {
        if human_seat.0 > 1 || starting_player.0 > 1 {
            return Err("invalid opening seat".into());
        }
        if max_physical_decisions == 0 || max_policy_steps == 0 {
            return Err("positive gameplay limits required".into());
        }
        let (deck_hashes, state) = crate::rl_session::human_opening_v1::build_opening_state_v1(
            &mainboards,
            pair_environment_seed,
            starting_player,
        )?;
        validate_opening_state(&state)?;
        Ok(Self {
            state,
            human_seat,
            phase: HumanOpeningPhaseV1::Mulligan,
            mulligans_taken: 0,
            episode_id,
            max_physical_decisions,
            max_policy_steps,
            deck_ids,
            deck_hashes,
        })
    }

    pub fn view(&self) -> HumanOpeningViewV1 {
        HumanOpeningViewV1 {
            schema: HUMAN_OPENING_SCHEMA_V1,
            human_seat: self.human_seat.into(),
            starting_player: self.state.starting_player.into(),
            phase: self.phase,
            mulligans_taken: self.mulligans_taken,
            required_bottom: if self.phase == HumanOpeningPhaseV1::Bottom {
                self.mulligans_taken
            } else {
                0
            },
            hand: self.state.players[self.human_seat.index()]
                .hand
                .iter()
                .enumerate()
                .map(|(index, id)| {
                    let card = self.state.objects.get(*id);
                    HumanOpeningCardV1 {
                        index: index as u32,
                        card_id: card.card_def,
                        name: card.name.clone(),
                    }
                })
                .collect(),
            can_mulligan: self.phase == HumanOpeningPhaseV1::Mulligan && self.mulligans_taken < 7,
            can_keep: self.phase == HumanOpeningPhaseV1::Mulligan,
            opponent_hand_count: self.state.players[self.human_seat.opponent().index()]
                .hand
                .len(),
            opponent_has_kept: true,
            model_mulligan_policy: MODEL_MULLIGAN_POLICY_V1,
        }
    }

    /// Coordinator-only projection of the automatic other-seat keep. It is
    /// captured from this opening's actual hand, before the explicit seat
    /// keeps. It must never be passed to the explicit seat's policy.
    pub(crate) fn automatic_keep_seven_view_v1(&self) -> Result<HumanOpeningViewV1, String> {
        if self.phase != HumanOpeningPhaseV1::Mulligan || self.mulligans_taken != 0 {
            return Err("automatic keep capture requires the untouched keep-seven opening".into());
        }
        let actor = self.human_seat.opponent();
        let mut view = self.view();
        view.human_seat = actor.into();
        view.hand = self.state.players[actor.index()]
            .hand
            .iter()
            .enumerate()
            .map(|(index, id)| {
                let card = self.state.objects.get(*id);
                HumanOpeningCardV1 {
                    index: index as u32,
                    card_id: card.card_def,
                    name: card.name.clone(),
                }
            })
            .collect();
        view.opponent_hand_count = self.state.players[self.human_seat.index()].hand.len();
        view.opponent_has_kept = false;
        Ok(view)
    }

    pub fn mulligan(&mut self) -> Result<(), String> {
        if self.phase != HumanOpeningPhaseV1::Mulligan || self.mulligans_taken >= 7 {
            return Err("mulligan is not available".into());
        }
        let mut candidate = self.state.clone();
        let hand = candidate.players[self.human_seat.index()].hand.clone();
        if hand.len() != 7 {
            return Err("opening hand is not seven cards".into());
        }
        for object in hand {
            event::propose_and_commit(
                &mut candidate,
                ProposedEvent::zone_change(object, Zone::Library),
            );
        }
        candidate
            .shuffle_library(self.human_seat)
            .map_err(|e| format!("opening shuffle: {e:?}"))?;
        for _ in 0..7 {
            event::propose_and_commit(&mut candidate, ProposedEvent::draw(self.human_seat));
        }
        validate_opening_state(&candidate)?;
        if candidate.players[self.human_seat.index()].hand.len() != 7 {
            return Err("mulligan did not redraw seven cards".into());
        }
        self.state = candidate;
        self.mulligans_taken += 1;
        if self.mulligans_taken == 7 {
            self.phase = HumanOpeningPhaseV1::Bottom;
        }
        Ok(())
    }

    pub fn keep(&mut self) -> Result<(), String> {
        if self.phase != HumanOpeningPhaseV1::Mulligan {
            return Err("keep is not available".into());
        }
        self.phase = if self.mulligans_taken == 0 {
            HumanOpeningPhaseV1::Ready
        } else {
            HumanOpeningPhaseV1::Bottom
        };
        Ok(())
    }

    /// Indexes address the unchanged seven-card kept hand. The supplied order
    /// becomes the final library tail from its top toward its very bottom.
    /// All selections validate before any mutation; duplicate physical cards
    /// are distinguished by hand index even when names/card ids match.
    pub fn bottom(&mut self, ordered_hand_indices: &[u32]) -> Result<(), String> {
        if self.phase != HumanOpeningPhaseV1::Bottom {
            return Err("bottom selection is not available".into());
        }
        if ordered_hand_indices.len() != usize::from(self.mulligans_taken) {
            return Err("bottom exactly one card per mulligan".into());
        }
        let hand = &self.state.players[self.human_seat.index()].hand;
        let mut unique = BTreeSet::new();
        if ordered_hand_indices
            .iter()
            .any(|index| *index as usize >= hand.len() || !unique.insert(*index))
        {
            return Err("bottom indexes must be unique cards in the kept hand".into());
        }
        let selected: Vec<_> = ordered_hand_indices
            .iter()
            .map(|index| hand[*index as usize])
            .collect();
        let mut candidate = self.state.clone();
        for object in selected {
            let mut proposal = ProposedEvent::zone_change(object, Zone::Library);
            let ProposedEvent::ZoneChange(change) = &mut proposal else {
                unreachable!()
            };
            change.library_placement = LibraryPlacement::Bottom;
            change.library_insert_visibility = LibraryInsertVisibility::Owner;
            event::propose_and_commit(&mut candidate, proposal);
        }
        validate_opening_state(&candidate)?;
        if candidate.players[self.human_seat.index()].hand.len()
            != 7 - usize::from(self.mulligans_taken)
        {
            return Err("bottoming did not produce the kept hand size".into());
        }
        self.state = candidate;
        self.phase = HumanOpeningPhaseV1::Ready;
        Ok(())
    }

    pub fn into_session(self) -> Result<FastActorSessionV1, String> {
        FastActorSessionV1::from_human_opening_v1(self)
    }

    pub(crate) fn into_ready_parts(mut self) -> Result<ReadyHumanOpeningPartsV1, String> {
        if self.phase != HumanOpeningPhaseV1::Ready {
            return Err("complete London opening before gameplay".into());
        }
        validate_opening_state(&self.state)?;
        // The engine starts at Untap but its first advance enters Upkeep,
        // bypassing Untap's normal turn-counter reset. Opening hands and London
        // redraws are not draws during turn one. Clear the setup event batch at
        // the same boundary: trigger collection reconstructs draw counts by
        // subtracting pending Draw events from these counters.
        for player in &mut self.state.players {
            player.draws_this_turn = 0;
        }
        self.state.engine.event_log.clear();
        Ok(ReadyHumanOpeningPartsV1 {
            state: self.state,
            episode_id: self.episode_id,
            max_physical_decisions: self.max_physical_decisions,
            max_policy_steps: self.max_policy_steps,
            deck_ids: self.deck_ids,
            deck_hashes: self.deck_hashes,
        })
    }
}

fn validate_opening_state(state: &GameState) -> Result<(), String> {
    if state.turn != 1
        || state.step != Step::Untap
        || state.active_player != state.starting_player
        || state.priority_player != state.starting_player
        || !state.stack.is_empty()
        || !state.exile.is_empty()
        || !state.command.is_empty()
        || state.objects.len() != 120
    {
        return Err("opening state advanced or card conservation changed".into());
    }
    let mut all = BTreeSet::new();
    for seat in [PlayerId::P0, PlayerId::P1] {
        let player = &state.players[seat.index()];
        if player.hand.len() + player.library.len() != 60
            || !player.battlefield.is_empty()
            || !player.graveyard.is_empty()
        {
            return Err("opening zone conservation differs".into());
        }
        for (zone, cards) in [(Zone::Hand, &player.hand), (Zone::Library, &player.library)] {
            for id in cards {
                let card = state.objects.get(*id);
                if !all.insert(*id) || card.owner != seat || card.zone != zone {
                    return Err("opening card binding differs".into());
                }
            }
        }
    }
    if all.len() != 120 {
        return Err("opening physical card count differs".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::card_def::card_id_by_name;

    fn opening(human: PlayerId, starting: PlayerId, seed: u64) -> HumanOpeningV1 {
        let mountain = card_id_by_name("Mountain").unwrap();
        let island = card_id_by_name("Island").unwrap();
        HumanOpeningV1::new(
            1,
            seed,
            256,
            8192,
            ["Human".into(), "Model".into()],
            [vec![mountain; 60], vec![island; 60]],
            starting,
            human,
        )
        .unwrap()
    }

    #[test]
    fn human_opening_ready_handoff_removes_only_pregame_draw_bookkeeping() {
        for human in [PlayerId::P0, PlayerId::P1] {
            for count in [0u8, 2, 7] {
                let mut opening = opening(human, human.opponent(), 92);
                for _ in 0..count {
                    opening.mulligan().unwrap();
                }
                if count < 7 {
                    opening.keep().unwrap();
                }
                if count > 0 {
                    opening
                        .bottom(&(0..u32::from(count)).collect::<Vec<_>>())
                        .unwrap();
                }
                assert_eq!(
                    opening.state.players[human.index()].draws_this_turn,
                    7 * (u32::from(count) + 1)
                );
                assert_eq!(
                    opening.state.players[human.opponent().index()].draws_this_turn,
                    7
                );
                assert!(!opening.state.engine.event_log.is_empty());
                let mut expected = opening.state.clone();
                expected.players[0].draws_this_turn = 0;
                expected.players[1].draws_this_turn = 0;
                expected.engine.event_log.clear();
                let parts = opening.into_ready_parts().unwrap();
                assert_eq!(
                    serde_json::to_vec(&parts.state).unwrap(),
                    serde_json::to_vec(&expected).unwrap()
                );
                assert!(parts.state.engine.event_log.is_empty());
            }
        }
    }

    #[test]
    fn human_opening_london_counts_and_forced_zero_keep() {
        let mut opening = opening(PlayerId::P0, PlayerId::P0, 91);
        for count in 1..=7 {
            opening.mulligan().unwrap();
            assert_eq!(opening.view().hand.len(), 7);
            assert_eq!(opening.view().mulligans_taken, count);
            validate_opening_state(&opening.state).unwrap();
        }
        assert_eq!(opening.view().phase, HumanOpeningPhaseV1::Bottom);
        assert!(opening.mulligan().is_err());
        assert!(opening.keep().is_err());
        opening.bottom(&[6, 5, 4, 3, 2, 1, 0]).unwrap();
        assert!(opening.view().hand.is_empty());
        assert_eq!(opening.view().phase, HumanOpeningPhaseV1::Ready);
        assert_eq!(opening.state.players[0].library.len(), 60);
        assert!(opening.into_session().is_ok());
    }

    #[test]
    fn human_opening_bottom_order_is_private_and_errors_do_not_mutate() {
        let mut opening = opening(PlayerId::P0, PlayerId::P1, 92);
        opening.mulligan().unwrap();
        opening.mulligan().unwrap();
        opening.keep().unwrap();
        let hand = opening.state.players[0].hand.clone();
        let before = serde_json::to_vec(&opening.state).unwrap();
        for bad in [&[1, 1][..], &[7, 0][..], &[0][..]] {
            assert!(opening.bottom(bad).is_err());
            assert_eq!(serde_json::to_vec(&opening.state).unwrap(), before);
        }
        opening.bottom(&[5, 2]).unwrap();
        let library = &opening.state.players[0].library;
        assert_eq!(&library[library.len() - 2..], &[hand[5], hand[2]]);
        assert_eq!(opening.state.players[0].hand.len(), 5);
        assert_eq!(
            opening
                .state
                .known_library_cards(PlayerId::P0, PlayerId::P0)
                .len(),
            2
        );
        assert!(opening
            .state
            .known_library_cards(PlayerId::P1, PlayerId::P0)
            .is_empty());
        validate_opening_state(&opening.state).unwrap();
    }

    #[test]
    fn human_opening_seed_replay_keeps_other_seat_private_and_unchanged() {
        for human in [PlayerId::P0, PlayerId::P1] {
            let mut a = opening(human, PlayerId::P1, 93);
            let mut b = opening(human, PlayerId::P1, 93);
            let other = human.opponent().index();
            let other_hand = a.state.players[other].hand.clone();
            let other_library = a.state.players[other].library.clone();
            for _ in 0..3 {
                a.mulligan().unwrap();
                b.mulligan().unwrap();
            }
            a.keep().unwrap();
            b.keep().unwrap();
            a.bottom(&[4, 1, 6]).unwrap();
            b.bottom(&[4, 1, 6]).unwrap();
            assert_eq!(
                serde_json::to_vec(&a.state).unwrap(),
                serde_json::to_vec(&b.state).unwrap()
            );
            assert_eq!(a.state.players[other].hand, other_hand);
            assert_eq!(a.state.players[other].library, other_library);
            let owner = if human == PlayerId::P0 {
                crate::environment_randomization_v2::PhysicalOwnerV2::P0
            } else {
                crate::environment_randomization_v2::PhysicalOwnerV2::P1
            };
            let other_owner = if human == PlayerId::P0 {
                crate::environment_randomization_v2::PhysicalOwnerV2::P1
            } else {
                crate::environment_randomization_v2::PhysicalOwnerV2::P0
            };
            let randomness = a.state.environment_randomization_v2().unwrap();
            assert_eq!(randomness.next_live_shuffle_ordinal(owner), 3);
            assert_eq!(randomness.next_live_shuffle_ordinal(other_owner), 0);
            let visible = serde_json::to_value(a.view()).unwrap();
            assert_eq!(visible["opponent_hand_count"], 7);
            assert!(visible.get("seed").is_none() && visible.get("library").is_none());
            let own_name = if human == PlayerId::P0 {
                "Mountain"
            } else {
                "Island"
            };
            assert!(a.view().hand.iter().all(|card| card.name == own_name));
        }
    }
}
