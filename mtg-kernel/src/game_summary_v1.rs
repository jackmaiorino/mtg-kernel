//! `GameSummaryV1` extractor (design section 3, W3): folds
//! `state.engine.event_history` plus the legal-cast hook into one
//! self-describing, checkpoint-identified row per game per side. Runs
//! in-loop with the same episode driver W2 and W5 use.

use crate::event::CommittedEvent;
use crate::ids::PlayerId;
use crate::rl_session::{RlEpisodeSessionV1, RlSessionDecisionV1, RlSessionResponseV1};
use crate::rl::ActionSemanticV1;
use crate::state::Zone;
use serde::Serialize;
use std::collections::BTreeMap;

pub const GAME_SUMMARY_SCHEMA_V1: u32 = 1;

#[derive(Debug, Clone, Serialize)]
pub struct OpponentEvidenceRowV1 {
    pub card_id: u16,
    pub first_seen_turn: u32,
    pub end_of_game_zone: Zone,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct OwnCardOutcomeV1 {
    pub times_drawn: u8,
    pub cast: bool,
    pub stuck_in_hand: bool,
    pub died_without_dealing_damage: bool,
    pub removal_no_target: bool,
    pub counterspell_held: bool,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct ResourceCurveV1 {
    pub lands_by_turn: Vec<u32>,
    pub hand_size_by_turn: [Vec<u32>; 2],
    /// `i32`, matching `PlayerState.life: i32` exactly (`state.rs:353`) so
    /// the driver loop can push `state.players[seat].life` with no cast and
    /// no truncation risk; design section 3 writes this as `[Vec<i16>; 2]`,
    /// but the live field it is sourced from is `i32`, so this plan widens
    /// the type to match the real source rather than truncating on push.
    pub life_by_turn: [Vec<i32>; 2],
    pub first_attack_turn: Option<u32>,
    pub damage_dealt_total: [i64; 2],
    pub damage_taken_total: [i64; 2],
}

#[derive(Debug, Clone, Serialize)]
pub struct GameSummaryV1 {
    pub schema_version: u32,
    pub checkpoint_weights_hash: String,
    pub checkpoint_git_head: String,
    pub winner: Option<PlayerId>,
    pub opponent_evidence: [Vec<OpponentEvidenceRowV1>; 2],
    pub own_card_outcomes: [BTreeMap<u16, OwnCardOutcomeV1>; 2],
    pub resource_curve: ResourceCurveV1,
}

/// The requires-target/is-counterspell ground truth (Task C, W4). A small
/// slice type so `game_summary_v1.rs` does not need to know how the tag
/// file is loaded from disk; the caller (Task E's driver) loads it once
/// per campaign, not once per game.
pub struct RemovalCounterspellTagsV1 {
    pub requires_target: std::collections::BTreeSet<u16>,
    pub is_counterspell: std::collections::BTreeSet<u16>,
}

fn was_card_offered_as_hand_cast_v1(semantic: &ActionSemanticV1, card_id: u16) -> bool {
    matches!(
        semantic,
        ActionSemanticV1::CastSpell { source, .. }
            if source.card_db_id == card_id && source.zone == Zone::Hand
    )
}

/// `ActionSemanticV1::CastSpell.actor` (`rl.rs:882`) and `CardStableRefV1`'s
/// own `owner`/`controller` (`rl.rs:207-208`) are `PlayerSeatV1`, the
/// RL-facing seat type; every `CommittedEvent` field that names a player
/// (`Draw.player`, `SpellCast.controller`, `ZoneChange.controller_before`)
/// is `PlayerId`, the engine-internal type (`ids.rs:35`, `event.rs`). The
/// two are related only by `impl From<PlayerId> for PlayerSeatV1`
/// (`rl.rs:184`) and have no reverse impl, so this task keeps two small,
/// separately named conversions rather than silently coercing one into the
/// other at a call site.
fn seat_index_v1(seat: crate::rl::PlayerSeatV1) -> usize {
    match seat {
        crate::rl::PlayerSeatV1::P0 => 0,
        crate::rl::PlayerSeatV1::P1 => 1,
    }
}

fn seat_index_from_player_id_v1(player: PlayerId) -> usize {
    player.0 as usize
}

fn count_lands_v1(state: &crate::state::GameState) -> u32 {
    state
        .objects
        .iter()
        .filter(|(_, object)| {
            object.zone == Zone::Battlefield
                && crate::card_def::CARD_DEFS[object.card_def as usize].is_land
        })
        .count() as u32
}

/// `Arena::push` (`ids.rs:72-76`) always assigns a new, strictly increasing
/// id and nothing ever removes an entry, so every `ObjectId` allocated this
/// game -- drawn, cast, milled, or created as a token mid-game -- is still
/// present in the terminal state's `objects` arena, only possibly in a
/// different zone, and `card_def` never changes for a given `ObjectId`
/// after creation. One pass over the terminal arena is therefore exactly
/// equivalent to incrementally rebuilding the map on every
/// `CreateToken`/`ZoneChange` event, and simpler.
fn build_object_card_def_map_v1(
    state: &crate::state::GameState,
) -> BTreeMap<crate::ids::ObjectId, u16> {
    state.objects.iter().map(|(id, object)| (id, object.card_def)).collect()
}

/// Every non-token object this game whose `owner` is `seat`: exactly the
/// distinct card ids of that seat's own registered 60 (a copy count > 1
/// collapses to one entry, which is what `own_card_outcomes`, keyed by
/// `card_id`, needs). Tokens are excluded because they are not registered
/// deck cards.
fn own_registered_card_ids_v1(
    state: &crate::state::GameState,
    seat: usize,
) -> std::collections::BTreeSet<u16> {
    let seat_player = PlayerId(seat as u8);
    state
        .objects
        .iter()
        .filter(|(_, object)| {
            object.owner == seat_player && !crate::card_def::CARD_DEFS[object.card_def as usize].is_token
        })
        .map(|(_, object)| object.card_def)
        .collect()
}

/// No `CommittedEvent` variant carries a turn stamp (design section 3), so
/// `run_episode_with_summary_v1` builds `turn_watermarks`, one
/// `(event_history.len(), turn)` pair recorded the first time each turn
/// reaches a decision. `event_index` is attributed to the smallest recorded
/// turn whose watermark index exceeds it (the earliest turn boundary
/// reached after the event committed); an event at or after the last
/// watermark is attributed to `final_turn`, the turn the game ended on.
fn turn_for_event_index_v1(turn_watermarks: &[(usize, u32)], final_turn: u32, event_index: usize) -> u32 {
    turn_watermarks
        .iter()
        .find(|&&(watermark_index, _)| watermark_index > event_index)
        .map(|&(_, turn)| turn)
        .unwrap_or(final_turn)
}

/// This task's operational proxy for "this specific cast resolved without
/// ever choosing a target": no `CommittedEvent` variant links a `Targeted`
/// event back to the spell that caused it (`Targeted::targeting_stack_item`
/// is a `StackItemId`, not this spell's `ObjectId`), so the proxy is
/// windowed on the casting object's own time on the stack (from its
/// `SpellCast` event at `cast_index` to its next `ZoneChange` away from
/// `Zone::Stack`) rather than a direct causal link. True iff no `Targeted`
/// event appears anywhere in that window.
fn spell_resolved_with_no_traced_target_v1(
    event_history: &[CommittedEvent],
    cast_index: usize,
    spell: crate::ids::ObjectId,
) -> bool {
    let leave_index = event_history
        .iter()
        .enumerate()
        .skip(cast_index + 1)
        .find(|(_, event)| {
            matches!(
                event,
                CommittedEvent::ZoneChange { object, from, .. }
                    if *object == spell && *from == Zone::Stack
            )
        })
        .map(|(index, _)| index)
        .unwrap_or(event_history.len());
    !event_history[cast_index..leave_index]
        .iter()
        .any(|event| matches!(event, CommittedEvent::Targeted { .. }))
}

/// Every card_def in `seat`'s hand at the state this is called against
/// (the terminal state, in `run_episode_with_summary_v1`'s real call site;
/// a hand-built set in the exact-value unit tests below).
fn end_of_game_hand_card_ids_v1(
    state: &crate::state::GameState,
    object_card_def: &BTreeMap<crate::ids::ObjectId, u16>,
    seat: usize,
) -> std::collections::BTreeSet<u16> {
    state.players[seat]
        .hand
        .iter()
        .filter_map(|object_id| object_card_def.get(object_id).copied())
        .collect()
}

/// Folds the full `event_history` plus the live legal-cast hook
/// (`offered_as_cast`, built by `run_episode_with_summary_v1`'s driver loop)
/// into `opponent_evidence` and `own_card_outcomes` (design section 3).
/// Runs once, at episode end. Deliberately takes plain data
/// (`event_history`, the `ObjectId -> card_def` map, each seat's own
/// registered ids, each seat's end-of-game hand) rather than `&GameState`
/// directly: `run_episode_with_summary_v1`'s real call site builds these
/// from the terminal state (via `build_object_card_def_map_v1`,
/// `own_registered_card_ids_v1`, `end_of_game_hand_card_ids_v1` above), and
/// this module's tests build them by hand from a synthetic event list, with
/// no need to construct a full `GameState`.
fn fold_event_history_v1(
    event_history: &[CommittedEvent],
    final_turn: u32,
    turn_watermarks: &[(usize, u32)],
    object_card_def: &BTreeMap<crate::ids::ObjectId, u16>,
    own_registered_ids: &[std::collections::BTreeSet<u16>; 2],
    end_of_game_hand_card_ids: &[std::collections::BTreeSet<u16>; 2],
    offered_as_cast: &[std::collections::BTreeSet<u16>; 2],
    tags: &RemovalCounterspellTagsV1,
) -> ([Vec<OpponentEvidenceRowV1>; 2], [BTreeMap<u16, OwnCardOutcomeV1>; 2]) {
    let mut opponent_first_seen: [BTreeMap<u16, u32>; 2] = Default::default();
    let mut opponent_end_of_game_zone: [BTreeMap<u16, Zone>; 2] = Default::default();
    let mut times_drawn: [BTreeMap<u16, u8>; 2] = Default::default();
    let mut cast_ids: [std::collections::BTreeSet<u16>; 2] = Default::default();
    let mut cast_no_target: [std::collections::BTreeSet<u16>; 2] = Default::default();
    let mut ever_dealt_damage: std::collections::BTreeSet<crate::ids::ObjectId> = Default::default();
    let mut died_without_damage: [std::collections::BTreeSet<u16>; 2] = Default::default();

    for (index, event) in event_history.iter().enumerate() {
        match event {
            CommittedEvent::Draw { player, object: Some(object_id) } => {
                let seat = seat_index_from_player_id_v1(*player);
                if let Some(&card_id) = object_card_def.get(object_id) {
                    if own_registered_ids[seat].contains(&card_id) {
                        *times_drawn[seat].entry(card_id).or_insert(0) += 1;
                    }
                }
            }
            CommittedEvent::SpellCast { spell, controller } => {
                let seat = seat_index_from_player_id_v1(*controller);
                if let Some(&card_id) = object_card_def.get(spell) {
                    if own_registered_ids[seat].contains(&card_id) {
                        cast_ids[seat].insert(card_id);
                        if spell_resolved_with_no_traced_target_v1(event_history, index, *spell) {
                            cast_no_target[seat].insert(card_id);
                        }
                    }
                }
            }
            CommittedEvent::Damage { source, .. } => {
                ever_dealt_damage.insert(*source);
            }
            CommittedEvent::CombatDamageToPlayer { source, .. } => {
                ever_dealt_damage.insert(*source);
            }
            CommittedEvent::ZoneChange { object, from, to, controller_before } => {
                let Some(&card_id) = object_card_def.get(object) else { continue };
                let is_public = matches!(to, Zone::Battlefield | Zone::Graveyard | Zone::Stack | Zone::Exile);
                let is_died = matches!(to, Zone::Graveyard | Zone::Exile)
                    && !matches!(from, Zone::Graveyard | Zone::Exile);

                // Opponent evidence, from each seat's own point of view:
                // `object`'s owner (not `controller_before`, so a stolen
                // permanent still reveals its true owner's card) belonging
                // to the *other* seat, made public.
                for seat in 0..2 {
                    let opponent_seat = 1 - seat;
                    if own_registered_ids[opponent_seat].contains(&card_id) && is_public {
                        opponent_first_seen[seat]
                            .entry(card_id)
                            .or_insert_with(|| turn_for_event_index_v1(turn_watermarks, final_turn, index));
                        opponent_end_of_game_zone[seat].insert(card_id, *to);
                    }
                }

                if is_died {
                    let owner_seat = seat_index_from_player_id_v1(*controller_before);
                    if own_registered_ids[owner_seat].contains(&card_id) && !ever_dealt_damage.contains(object) {
                        died_without_damage[owner_seat].insert(card_id);
                    }
                }
            }
            _ => {}
        }
    }

    let mut opponent_evidence: [Vec<OpponentEvidenceRowV1>; 2] = Default::default();
    for seat in 0..2 {
        for (&card_id, &first_seen_turn) in &opponent_first_seen[seat] {
            opponent_evidence[seat].push(OpponentEvidenceRowV1 {
                card_id,
                first_seen_turn,
                end_of_game_zone: opponent_end_of_game_zone[seat][&card_id],
            });
        }
    }

    let mut own_card_outcomes: [BTreeMap<u16, OwnCardOutcomeV1>; 2] = Default::default();
    for seat in 0..2 {
        for &card_id in &own_registered_ids[seat] {
            let stuck_in_hand = end_of_game_hand_card_ids[seat].contains(&card_id);
            let cast = cast_ids[seat].contains(&card_id);
            let removal_no_target = tags.requires_target.contains(&card_id)
                && cast
                && cast_no_target[seat].contains(&card_id);
            let counterspell_held = tags.is_counterspell.contains(&card_id)
                && !offered_as_cast[seat].contains(&card_id)
                && stuck_in_hand;
            own_card_outcomes[seat].insert(
                card_id,
                OwnCardOutcomeV1 {
                    times_drawn: *times_drawn[seat].get(&card_id).unwrap_or(&0),
                    cast,
                    stuck_in_hand,
                    died_without_dealing_damage: died_without_damage[seat].contains(&card_id),
                    removal_no_target,
                    counterspell_held,
                },
            );
        }
    }

    (opponent_evidence, own_card_outcomes)
}

pub fn run_episode_with_summary_v1(
    session: &mut RlEpisodeSessionV1,
    checkpoint_weights_hash: &str,
    tags: &RemovalCounterspellTagsV1,
    policy_fn: &mut dyn FnMut(&RlSessionDecisionV1) -> (u32, String),
) -> GameSummaryV1 {
    let mut offered_as_cast: [std::collections::BTreeSet<u16>; 2] = Default::default();
    let mut resource_curve = ResourceCurveV1::default();
    let mut turn_watermarks: Vec<(usize, u32)> = Vec::new();
    let mut last_turn = session.game_state().turn;

    loop {
        match session.current_response() {
            RlSessionResponseV1::Terminal(terminal) => {
                let final_state = session.game_state();
                let object_card_def = build_object_card_def_map_v1(final_state);
                let own_registered_ids = [
                    own_registered_card_ids_v1(final_state, 0),
                    own_registered_card_ids_v1(final_state, 1),
                ];
                let end_of_game_hand_card_ids = [
                    end_of_game_hand_card_ids_v1(final_state, &object_card_def, 0),
                    end_of_game_hand_card_ids_v1(final_state, &object_card_def, 1),
                ];
                let (opponent_evidence, own_card_outcomes) = fold_event_history_v1(
                    &final_state.engine.event_history,
                    final_state.turn,
                    &turn_watermarks,
                    &object_card_def,
                    &own_registered_ids,
                    &end_of_game_hand_card_ids,
                    &offered_as_cast,
                    tags,
                );
                let winner = match terminal.winner {
                    None => None,
                    Some(crate::rl::PlayerSeatV1::P0) => Some(PlayerId::P0),
                    Some(crate::rl::PlayerSeatV1::P1) => Some(PlayerId::P1),
                };
                return GameSummaryV1 {
                    schema_version: GAME_SUMMARY_SCHEMA_V1,
                    checkpoint_weights_hash: checkpoint_weights_hash.to_owned(),
                    checkpoint_git_head: env!("MTG_KERNEL_BUILD_GIT_HEAD").to_owned(),
                    winner,
                    opponent_evidence,
                    own_card_outcomes,
                    resource_curve,
                };
            }
            RlSessionResponseV1::Decision(decision) => {
                let turn = session.game_state().turn;
                if turn != last_turn || turn_watermarks.is_empty() {
                    let state = session.game_state();
                    turn_watermarks.push((state.engine.event_history.len(), turn));
                    resource_curve.lands_by_turn.push(count_lands_v1(state));
                    for seat in 0..2 {
                        resource_curve.hand_size_by_turn[seat]
                            .push(state.players[seat].hand.len() as u32);
                        // `PlayerState.life: i32` (`state.rs:353`) matches
                        // `ResourceCurveV1.life_by_turn: [Vec<i32>; 2]`
                        // exactly; no cast, no truncation.
                        resource_curve.life_by_turn[seat].push(state.players[seat].life);
                    }
                    last_turn = turn;
                }
                for legal_action in &decision.legal_actions {
                    if let ActionSemanticV1::CastSpell { actor, source } = &legal_action.semantic {
                        if was_card_offered_as_hand_cast_v1(&legal_action.semantic, source.card_db_id) {
                            let seat_index = seat_index_v1(*actor);
                            offered_as_cast[seat_index].insert(source.card_db_id);
                        }
                    }
                }
                let (selected_index, selected_action_id) = policy_fn(&decision);
                session
                    .step(decision.episode_id, decision.step, selected_index, &selected_action_id)
                    .expect("policy-selected action is legal by construction");
            }
        }
    }
}

pub fn append_game_summary_jsonl_v1(
    path: &std::path::Path,
    summary: &GameSummaryV1,
) -> std::io::Result<()> {
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new().create(true).append(true).open(path)?;
    let line = serde_json::to_string(summary).expect("GameSummaryV1 always serializes");
    writeln!(file, "{line}")
}

#[cfg(test)]
#[derive(serde::Deserialize)]
struct GameSummaryRoundTripCheckV1 {
    schema_version: u32,
    checkpoint_weights_hash: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime_decks::runtime_deck_by_id;
    use crate::state::SplitMix64;

    fn burn_and_rally() -> [Vec<u16>; 2] {
        [
            runtime_deck_by_id("Burn").unwrap().card_ids.to_vec(),
            runtime_deck_by_id("Rally").unwrap().card_ids.to_vec(),
        ]
    }

    #[test]
    fn run_episode_with_summary_produces_a_well_formed_row_for_both_sides() {
        let mainboards = burn_and_rally();
        let deck_ids = ["Burn".to_owned(), "Rally".to_owned()];
        let mut session = RlEpisodeSessionV1::reset_with_explicit_decks_and_limits(
            1, 0x7777_7777_7777_7777, 2000, 200_000, deck_ids, mainboards,
        )
        .unwrap();
        let tags = RemovalCounterspellTagsV1 {
            requires_target: Default::default(),
            is_counterspell: Default::default(),
        };
        let mut rng = SplitMix64::seed(0x8888_8888_8888_8888);
        let mut policy = |decision: &RlSessionDecisionV1| {
            let index = (rng.next_u64() as usize) % decision.legal_actions.len();
            (index as u32, decision.legal_actions[index].stable_id.clone())
        };
        let summary = run_episode_with_summary_v1(&mut session, "deadbeefdeadbeef", &tags, &mut policy);
        assert_eq!(summary.schema_version, GAME_SUMMARY_SCHEMA_V1);
        assert_eq!(summary.checkpoint_weights_hash, "deadbeefdeadbeef");
        assert!(!summary.checkpoint_git_head.is_empty());
        assert!(summary.resource_curve.lands_by_turn.iter().sum::<u32>() > 0, "some land entered play over a full game");
        // own_card_outcomes must only ever key cards from that seat's own
        // registered mainboard, never the opponent's.
        let burn_ids: std::collections::BTreeSet<u16> = runtime_deck_by_id("Burn").unwrap().card_ids.iter().copied().collect();
        for &card_id in summary.own_card_outcomes[0].keys() {
            assert!(burn_ids.contains(&card_id), "P0 own_card_outcomes leaked a non-Burn card_id {card_id}");
        }
    }

    #[test]
    fn legal_cast_hook_marks_a_card_offered_as_a_cast_even_if_never_cast() {
        // A minimal, direct check of the hook logic in isolation: build a
        // synthetic legal_actions list with one CastSpell semantic for a
        // known card_id in Zone::Hand and confirm the folding function
        // records it, independent of a full episode run.
        let semantic = ActionSemanticV1::CastSpell {
            actor: PlayerId::P0.into(),
            source: crate::rl::CardStableRefV1 {
                arena_id: 1,
                card_db_id: 42,
                owner: PlayerId::P0.into(),
                controller: PlayerId::P0.into(),
                zone: Zone::Hand,
                zone_change_count: 0,
            },
        };
        let offered = was_card_offered_as_hand_cast_v1(&semantic, 42);
        assert!(offered);
        let not_offered = was_card_offered_as_hand_cast_v1(&semantic, 43);
        assert!(!not_offered);
    }

    #[test]
    fn fold_event_history_records_opponent_evidence_both_directions_and_a_no_target_removal_cast() {
        use crate::ids::ObjectId;

        // A four-event synthetic history: P0 draws its own removal spell
        // (card_id 200, requires_target), P1's creature (card_id 100)
        // enters the battlefield turn 2 (revealing it to P0), P0 casts the
        // removal spell, and it resolves straight to the graveyard with no
        // `Targeted` event ever logged in its casting window (no legal
        // target was ever chosen).
        let event_history = vec![
            CommittedEvent::Draw { player: PlayerId::P0, object: Some(ObjectId(20)) }, // index 0
            CommittedEvent::ZoneChange {
                object: ObjectId(10),
                from: Zone::Library,
                to: Zone::Battlefield,
                controller_before: PlayerId::P1,
            }, // index 1: P1's card_id 100 becomes public
            CommittedEvent::SpellCast { spell: ObjectId(20), controller: PlayerId::P0 }, // index 2
            CommittedEvent::ZoneChange {
                object: ObjectId(20),
                from: Zone::Stack,
                to: Zone::Graveyard,
                controller_before: PlayerId::P0,
            }, // index 3: resolves with no Targeted event anywhere in [2, 3)
        ];
        // Turn 1 starts at history length 0; turn 2 starts at history
        // length 2 (after the Draw and the opponent's ZoneChange, before
        // the SpellCast); the game ends on turn 3.
        let turn_watermarks = vec![(0usize, 1u32), (2usize, 2u32)];
        let final_turn = 3u32;
        let object_card_def: BTreeMap<ObjectId, u16> =
            [(ObjectId(10), 100u16), (ObjectId(20), 200u16)].into_iter().collect();
        let own_registered_ids: [std::collections::BTreeSet<u16>; 2] = [
            [200u16].into_iter().collect(),
            [100u16].into_iter().collect(),
        ];
        let end_of_game_hand_card_ids: [std::collections::BTreeSet<u16>; 2] = Default::default();
        let offered_as_cast: [std::collections::BTreeSet<u16>; 2] =
            [[200u16].into_iter().collect(), Default::default()];
        let tags = RemovalCounterspellTagsV1 {
            requires_target: [200u16].into_iter().collect(),
            is_counterspell: Default::default(),
        };

        let (opponent_evidence, own_card_outcomes) = fold_event_history_v1(
            &event_history,
            final_turn,
            &turn_watermarks,
            &object_card_def,
            &own_registered_ids,
            &end_of_game_hand_card_ids,
            &offered_as_cast,
            &tags,
        );

        // P0's evidence about the opponent (P1's card_id 100): first seen
        // turn 2, the earliest turn boundary reached after index 1.
        assert_eq!(opponent_evidence[0].len(), 1);
        assert_eq!(opponent_evidence[0][0].card_id, 100);
        assert_eq!(opponent_evidence[0][0].first_seen_turn, 2);
        assert_eq!(opponent_evidence[0][0].end_of_game_zone, Zone::Battlefield);

        // P1's evidence about the opponent (P0's card_id 200): first seen
        // turn 3 (no watermark index exceeds 3, so it falls back to
        // final_turn), ending in the graveyard.
        assert_eq!(opponent_evidence[1].len(), 1);
        assert_eq!(opponent_evidence[1][0].card_id, 200);
        assert_eq!(opponent_evidence[1][0].first_seen_turn, 3);
        assert_eq!(opponent_evidence[1][0].end_of_game_zone, Zone::Graveyard);

        // P0's own_card_outcomes for its removal spell: drawn once, cast,
        // not stuck in hand, and removal_no_target true (it was cast, it
        // is tagged requires_target, and no Targeted event ever fired in
        // its casting window).
        let outcome = &own_card_outcomes[0][&200];
        assert_eq!(outcome.times_drawn, 1);
        assert!(outcome.cast);
        assert!(!outcome.stuck_in_hand);
        assert!(outcome.removal_no_target, "cast with no traced target must be recorded");
        assert!(!outcome.counterspell_held, "not tagged is_counterspell, so this field is always false here");
    }

    #[test]
    fn append_game_summary_jsonl_writes_one_line_per_call_and_round_trips() {
        let dir = std::env::temp_dir().join(format!("game_summary_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("summaries.jsonl");
        let _ = std::fs::remove_file(&path);

        let mainboards = burn_and_rally();
        let deck_ids = ["Burn".to_owned(), "Rally".to_owned()];
        let mut session = RlEpisodeSessionV1::reset_with_explicit_decks_and_limits(
            2, 0x2222_2222_2222_2222, 2000, 200_000, deck_ids, mainboards,
        )
        .unwrap();
        let tags = RemovalCounterspellTagsV1 { requires_target: Default::default(), is_counterspell: Default::default() };
        let mut rng = SplitMix64::seed(0x3333_3333_3333_3333);
        let mut policy = |decision: &RlSessionDecisionV1| {
            let index = (rng.next_u64() as usize) % decision.legal_actions.len();
            (index as u32, decision.legal_actions[index].stable_id.clone())
        };
        let summary = run_episode_with_summary_v1(&mut session, "cafef00dcafef00d", &tags, &mut policy);
        append_game_summary_jsonl_v1(&path, &summary).unwrap();
        append_game_summary_jsonl_v1(&path, &summary).unwrap();

        let contents = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = contents.lines().collect();
        assert_eq!(lines.len(), 2);
        let round_tripped: GameSummaryRoundTripCheckV1 = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(round_tripped.schema_version, GAME_SUMMARY_SCHEMA_V1);
        assert_eq!(round_tripped.checkpoint_weights_hash, "cafef00dcafef00d");
    }

    /// Privacy proof (design section 3: "only player-visible information for
    /// the opponent side"). `CommittedEvent::Draw` (`event.rs:738-741`) is
    /// the *only* event a private library-to-hand move ever produces --
    /// `ProposedEvent::Draw`'s handler in `event.rs` calls `state.draw_card`
    /// directly and never builds a `ZoneChange`/`commit_zone_change` for it
    /// (unlike every other zone move in the engine), so a card that is
    /// drawn and stays in hand leaves no public trace in `event_history` at
    /// all: the same discipline covers a private top-of-library look
    /// (Delver of Secrets), which updates only `state.library_knowledge`
    /// (`state.rs:1250`), a field `fold_event_history_v1` never reads (its
    /// signature does not even accept a `&GameState`). This test exercises
    /// that guarantee end to end: P1 draws two of its own cards privately;
    /// one (301) is later discarded to the graveyard (a public
    /// `ZoneChange`) and the other (300) stays in hand for the rest of the
    /// game. P0's opponent_evidence must contain 301 (first seen at the
    /// *public* discard's turn, not the earlier private draw's turn) and
    /// must never contain 300 at all.
    #[test]
    fn fold_event_history_never_leaks_a_private_hand_card_into_opponent_evidence() {
        use crate::ids::ObjectId;

        let event_history = vec![
            CommittedEvent::Draw { player: PlayerId::P1, object: Some(ObjectId(30)) }, // index 0: private, card_id 300, stays in hand
            CommittedEvent::Draw { player: PlayerId::P1, object: Some(ObjectId(31)) }, // index 1: private, card_id 301
            CommittedEvent::ZoneChange {
                object: ObjectId(31),
                from: Zone::Hand,
                to: Zone::Graveyard,
                controller_before: PlayerId::P1,
            }, // index 2: card_id 301 discarded turn 5, made public
        ];
        // Everything happens on turn 1 except the discard, which the game
        // attributes to turn 5 via the final_turn fallback (no watermark
        // index exceeds 2).
        let turn_watermarks = vec![(0usize, 1u32)];
        let final_turn = 5u32;
        let object_card_def: BTreeMap<ObjectId, u16> =
            [(ObjectId(30), 300u16), (ObjectId(31), 301u16)].into_iter().collect();
        let own_registered_ids: [std::collections::BTreeSet<u16>; 2] =
            [Default::default(), [300u16, 301u16].into_iter().collect()];
        let end_of_game_hand_card_ids: [std::collections::BTreeSet<u16>; 2] =
            [Default::default(), [300u16].into_iter().collect()];
        let offered_as_cast: [std::collections::BTreeSet<u16>; 2] = Default::default();
        let tags = RemovalCounterspellTagsV1 { requires_target: Default::default(), is_counterspell: Default::default() };

        let (opponent_evidence, own_card_outcomes) = fold_event_history_v1(
            &event_history,
            final_turn,
            &turn_watermarks,
            &object_card_def,
            &own_registered_ids,
            &end_of_game_hand_card_ids,
            &offered_as_cast,
            &tags,
        );

        // P0's evidence about P1: exactly the discarded card, never the
        // one that stayed private in hand.
        assert_eq!(opponent_evidence[0].len(), 1, "only the publicly-discarded card may appear");
        assert_eq!(opponent_evidence[0][0].card_id, 301);
        assert_eq!(opponent_evidence[0][0].first_seen_turn, 5, "first_seen_turn must be the public event's turn, not the private draw's");
        assert_eq!(opponent_evidence[0][0].end_of_game_zone, Zone::Graveyard);
        assert!(
            !opponent_evidence[0].iter().any(|row| row.card_id == 300),
            "a card that never left a private zone must never appear as opponent evidence"
        );

        // P1's own bookkeeping still records both draws correctly; this is
        // private-side data, not opponent-facing.
        assert_eq!(own_card_outcomes[1][&300].times_drawn, 1);
        assert!(own_card_outcomes[1][&300].stuck_in_hand);
        assert_eq!(own_card_outcomes[1][&301].times_drawn, 1);
        assert!(!own_card_outcomes[1][&301].stuck_in_hand);
    }
}
