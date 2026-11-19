//! Per-seat engine priority windows for human play
//! (`HarnessSurfaceV2::set_engine_window_seats_v1`).
//!
//! The person's seat is asked at every engine priority window; the other
//! seat keeps the trained surface. Proven here:
//!
//! 1. A person who passes at every new window plays the game of the
//!    unflagged sessions: equal game state at every shared decision, equal
//!    menus, equal model input, and equal observations but for the two
//!    session counters (`step_index`, `physical_decision_id`), which are
//!    ahead by exactly the number of new windows passed so far, and the
//!    hash that covers them. A new window is an ordinary decision: it
//!    counts toward the session's caps, so a capped flagged game is
//!    truncated earlier (also shown here). The human bridge runs uncapped.
//! 2. With no seat flagged the windows reset is the plain reset.
//! 3. A scripted game: the person casts an instant in the other seat's end
//!    step and in their own upkeep, and holds priority with a spell on the
//!    stack; the other seat is never asked in a step its surface gates.
//! 4. The bridge takes the seats on its reset request, marks the new
//!    windows in the private support block and refuses the field anywhere
//!    but the human-play London server.

use super::*;
use crate::agent_bridge_flat_v4::FlatV4LockstepV1;
use crate::agent_bridge_flat_v4_encode::{
    encode_flat_v7_tensor_v1, flat_v7_extension_value_v1, FlatV4EncodeScratchV1,
};
use crate::agent_bridge_v1::{SpellbenchBridgeServerV1, SpellbenchEngineIdentityV1};
use crate::card_def::{CardType, CARD_DEFS};
use crate::ids::ObjectId;
use crate::rl::{ActionSemanticV1, ObservationV5, PlayerSeatV1};
use crate::state::{SplitMix64, Step};
use crate::surface::harness_never_offers_priority;
use serde_json::{json, Value};

const DECKS: [&str; 8] = [
    "Wildfire", "Rally", "Affinity", "Elves", "Spy", "Burn", "CawGates", "Faeries",
];
const LEARNED: [PauperMulliganModeV1; 2] = [PauperMulliganModeV1::Learned; 2];
const LIMITS: (u64, u64) = (200_000, 2_000_000);

fn pair(
    decks: [&str; 2],
    seed: u64,
    episode: u64,
    seats: [bool; 2],
) -> (RlEpisodeSessionV1, FlatV4LockstepV1) {
    let decks: SessionDeckIdsV1 = [decks[0].to_string(), decks[1].to_string()];
    let raw = RlEpisodeSessionV1::reset_pauper_london_human_windows_v1(
        episode,
        seed,
        LIMITS.0,
        LIMITS.1,
        decks.clone(),
        LEARNED,
        seats,
    )
    .unwrap();
    let lockstep =
        FlatV4LockstepV1::reset_london_human(episode, seed, LIMITS.0, LIMITS.1, decks, seats)
            .unwrap();
    (raw, lockstep)
}

fn seat_index(seat: PlayerSeatV1) -> usize {
    match seat {
        PlayerSeatV1::P0 => 0,
        PlayerSeatV1::P1 => 1,
    }
}

fn decision_of(raw: &RlEpisodeSessionV1) -> Option<RlSessionDecisionV1> {
    match raw.current_response() {
        RlSessionResponseV1::Decision(decision) => Some(decision),
        RlSessionResponseV1::Terminal(_) => None,
    }
}

fn answer(
    raw: &mut RlEpisodeSessionV1,
    lockstep: &mut FlatV4LockstepV1,
    decision: &RlSessionDecisionV1,
    rows: &[u32],
    pick: u32,
) {
    let chosen = &decision.legal_actions[pick as usize];
    raw.step(decision.episode_id, decision.step, pick, &chosen.stable_id)
        .unwrap();
    lockstep.mirror_step(rows, decision.step, pick).unwrap();
}

fn pass_row(decision: &RlSessionDecisionV1, rows: &[u32]) -> u32 {
    *rows
        .iter()
        .find(|id| {
            matches!(
                decision.legal_actions[**id as usize].semantic,
                ActionSemanticV1::Pass { .. }
            )
        })
        .expect("a priority window always offers Pass")
}

/// The observation with the two session counters moved back by `ahead` (the
/// new windows the flagged game has passed so far; 0 for the unflagged game)
/// and without the hash that covers them. After this nothing may differ: the
/// counters of the flagged game are ahead by exactly `ahead`, no more.
fn with_counters_moved_back(decision: &RlSessionDecisionV1, ahead: u64) -> ObservationV5 {
    let mut observation = (*decision.observation).clone();
    observation.step_index = observation
        .step_index
        .checked_sub(ahead)
        .expect("the step counter is at least the new windows passed");
    observation.physical_decision_id = observation
        .physical_decision_id
        .checked_sub(ahead)
        .expect("the decision counter is at least the new windows passed");
    observation.visible_projection_hash = 0;
    observation
}

/// Which forced pass a new window replaced, by where it happened: the reason,
/// whether a combat declare step, and for the caster's-own suppression
/// whether the seat's own object is on top of the stack (else its own mana
/// ability reopened the round).
fn window_kind(raw: &RlEpisodeSessionV1, reason: &'static str, person: usize) -> String {
    let state = raw.game_state();
    let combat = matches!(state.step, Step::DeclareAttackers | Step::DeclareBlockers);
    let place = if combat { "combat" } else { "outside_combat" };
    if reason == "stack_top_is_casters_own" {
        let own_top = state
            .stack
            .last()
            .is_some_and(|item| item.controller.index() == person);
        let what = if own_top {
            "own_stack_object"
        } else {
            "own_mana_ability"
        };
        return format!("{reason}:{place}:{what}");
    }
    format!("{reason}:{place}")
}

fn tensor(lockstep: &FlatV4LockstepV1, rows: &[u32], scratch: &mut FlatV4EncodeScratchV1) -> Value {
    let encoded = encode_flat_v7_tensor_v1(lockstep.fast_session(), scratch).unwrap();
    let mut value = flat_v7_extension_value_v1(&encoded, rows).unwrap();
    value["tensor"].take()
}

#[derive(Default)]
struct PassThrough {
    shared_decisions: u64,
    other_seat_decisions: u64,
    new_windows: u64,
    reasons: std::collections::BTreeMap<String, u64>,
    finished: bool,
}

/// Plays the unflagged pair and the pair flagged for `person` side by side.
/// The flagged game passes at every new window; every other decision takes
/// the same seeded answer in both games.
fn pass_through(
    deck: &str,
    seed: u64,
    episode: u64,
    person: usize,
    max_shared: u64,
) -> PassThrough {
    let mut seats = [false; 2];
    seats[person] = true;
    let (mut plain, mut plain_copy) = pair([deck, deck], seed, episode, [false; 2]);
    let (mut windows, mut windows_copy) = pair([deck, deck], seed, episode, seats);
    let mut rng = SplitMix64::seed(seed ^ 0x51DE);
    let mut scratch = FlatV4EncodeScratchV1::default();
    let mut out = PassThrough::default();
    let context = |what: &str, step: u64| {
        format!("{deck} seed {seed} person p{person}: {what} at shared decision {step}")
    };
    loop {
        let response = windows.current_response();
        windows_copy
            .verify(&windows, &response)
            .unwrap_or_else(|error| {
                panic!("{}", context(&format!("{error:?}"), out.shared_decisions))
            });
        if let (Some(reason), RlSessionResponseV1::Decision(decision)) =
            (windows.engine_window_v1(), &response)
        {
            // A window only the flagged seat has: answer Pass, as the
            // trained surface would have, and leave the plain game alone.
            assert_eq!(
                seat_index(decision.acting_player),
                person,
                "{}",
                context("a new window for the other seat", out.shared_decisions)
            );
            let rows = windows_copy.row_candidate_ids(decision).unwrap();
            // The model input of a new window still encodes (the advisor
            // reads it).
            tensor(&windows_copy, &rows, &mut scratch);
            let pass = pass_row(decision, &rows);
            answer(&mut windows, &mut windows_copy, decision, &rows, pass);
            out.new_windows += 1;
            *out.reasons
                .entry(window_kind(&windows, reason, person))
                .or_default() += 1;
            continue;
        }
        let plain_response = plain.current_response();
        plain_copy.verify(&plain, &plain_response).unwrap();
        assert_eq!(
            plain.game_state(),
            windows.game_state(),
            "{}",
            context("game state", out.shared_decisions)
        );
        let (theirs, ours) = match (plain_response, response) {
            (RlSessionResponseV1::Decision(theirs), RlSessionResponseV1::Decision(ours)) => {
                (theirs, ours)
            }
            (RlSessionResponseV1::Terminal(theirs), RlSessionResponseV1::Terminal(ours)) => {
                assert_eq!(
                    (
                        theirs.terminal_outcome,
                        theirs.winner,
                        &theirs.terminal_reason,
                        theirs.terminal_reward
                    ),
                    (
                        ours.terminal_outcome,
                        ours.winner,
                        &ours.terminal_reason,
                        ours.terminal_reward
                    ),
                    "{}",
                    context("terminal", out.shared_decisions)
                );
                out.finished = true;
                return out;
            }
            _ => panic!(
                "{}",
                context("one game ended and the other did not", out.shared_decisions)
            ),
        };
        if out.shared_decisions >= max_shared {
            return out;
        }
        assert_eq!(
            theirs.acting_player,
            ours.acting_player,
            "{}",
            context("acting seat", out.shared_decisions)
        );
        assert_eq!(
            (theirs.substep_index, theirs.substep_count),
            (ours.substep_index, ours.substep_count)
        );
        assert_eq!(
            theirs
                .legal_actions
                .iter()
                .map(|action| &action.semantic)
                .collect::<Vec<_>>(),
            ours.legal_actions
                .iter()
                .map(|action| &action.semantic)
                .collect::<Vec<_>>(),
            "{}",
            context("menu", out.shared_decisions)
        );
        assert_eq!(
            with_counters_moved_back(&theirs, 0),
            with_counters_moved_back(&ours, out.new_windows),
            "{}",
            context("observation", out.shared_decisions)
        );
        let rows = plain_copy.row_candidate_ids(&theirs).unwrap();
        assert_eq!(rows, windows_copy.row_candidate_ids(&ours).unwrap());
        assert_eq!(
            tensor(&plain_copy, &rows, &mut scratch),
            tensor(&windows_copy, &rows, &mut scratch),
            "{}",
            context("model input", out.shared_decisions)
        );
        // The session's own counters are ahead by the new windows, exactly.
        assert_eq!(
            (
                plain.policy_step_count() + out.new_windows,
                plain.physical_decision_count() + out.new_windows,
                theirs.step + out.new_windows,
                theirs.physical_decision_id + out.new_windows
            ),
            (
                windows.policy_step_count(),
                windows.physical_decision_count(),
                ours.step,
                ours.physical_decision_id
            ),
            "{}",
            context("session counters", out.shared_decisions)
        );
        let pick = rows[(rng.next_u64() % rows.len() as u64) as usize];
        answer(&mut plain, &mut plain_copy, &theirs, &rows, pick);
        answer(&mut windows, &mut windows_copy, &ours, &rows, pick);
        out.shared_decisions += 1;
        out.other_seat_decisions += u64::from(seat_index(ours.acting_player) != person);
    }
}

#[test]
fn a_person_who_passes_every_new_window_plays_the_unflagged_game() {
    let mut total = PassThrough::default();
    let mut finished = 0;
    for (index, deck) in DECKS.iter().enumerate() {
        for seed in [11_u64, 29] {
            for person in [0, 1] {
                let episode = 0xE07_0000 + index as u64 * 1_000 + seed * 2 + person as u64;
                let played = pass_through(deck, seed, episode, person, 6_000);
                assert!(played.shared_decisions > 20, "{deck} seed {seed}");
                assert!(
                    played.new_windows > 10,
                    "{deck} seed {seed}: {} new windows",
                    played.new_windows
                );
                finished += u32::from(played.finished);
                total.shared_decisions += played.shared_decisions;
                total.other_seat_decisions += played.other_seat_decisions;
                total.new_windows += played.new_windows;
                for (reason, count) in played.reasons {
                    *total.reasons.entry(reason).or_default() += count;
                }
            }
        }
    }
    eprintln!(
        "pass-through sweep: {finished} of 32 games finished, {} shared decisions ({} of the other seat), {} new windows passed: {:?}",
        total.shared_decisions, total.other_seat_decisions, total.new_windows, total.reasons
    );
    assert!(finished >= 24, "only {finished} of 32 games finished");
    assert!(
        total.other_seat_decisions > 2_000,
        "{}",
        total.other_seat_decisions
    );
    // Every forced pass of the trained surface became a window somewhere in
    // the sweep, each in the place it applies.
    for kind in [
        "step_gated:outside_combat",
        "no_real_option:outside_combat",
        "no_real_option:combat",
        "combat_priority_action_spent:combat",
        "stack_top_is_casters_own:combat:own_stack_object",
        "stack_top_is_casters_own:combat:own_mana_ability",
        "stack_top_is_casters_own:outside_combat:own_stack_object",
    ] {
        assert!(
            total.reasons.get(kind).copied().unwrap_or(0) > 0,
            "no new window of kind {kind}: {:?}",
            total.reasons
        );
    }
}

#[test]
fn a_new_window_is_a_decision_and_counts_toward_the_session_caps() {
    // Both games capped at the same number of decisions; the flagged game
    // spends some of them on new windows (all passed) and is truncated
    // while the unflagged game still has a decision to give.
    const CAP: u64 = 60;
    let decks: SessionDeckIdsV1 = ["Burn".to_string(), "Burn".to_string()];
    let reset = |seats| {
        RlEpisodeSessionV1::reset_pauper_london_human_windows_v1(
            0xE07_2000,
            19,
            CAP,
            LIMITS.1,
            decks.clone(),
            LEARNED,
            seats,
        )
        .unwrap()
    };
    let (mut plain, mut windows) = (reset([false; 2]), reset([true, false]));
    let mut rng = SplitMix64::seed(0xCA9);
    let (mut shared, mut passed) = (0_u64, 0_u64);
    let terminal = loop {
        let ours = match windows.current_response() {
            RlSessionResponseV1::Decision(ours) => ours,
            RlSessionResponseV1::Terminal(terminal) => break terminal,
        };
        if windows.engine_window_v1().is_some() {
            let pass = ours
                .legal_actions
                .iter()
                .position(|action| matches!(action.semantic, ActionSemanticV1::Pass { .. }))
                .unwrap();
            windows
                .step(
                    ours.episode_id,
                    ours.step,
                    pass as u32,
                    &ours.legal_actions[pass].stable_id,
                )
                .unwrap();
            passed += 1;
            continue;
        }
        let theirs = decision_of(&plain).expect("the unflagged game has not reached its cap");
        assert_eq!(theirs.legal_actions.len(), ours.legal_actions.len());
        let pick = (rng.next_u64() % ours.legal_actions.len() as u64) as usize;
        plain
            .step(
                theirs.episode_id,
                theirs.step,
                pick as u32,
                &theirs.legal_actions[pick].stable_id,
            )
            .unwrap();
        windows
            .step(
                ours.episode_id,
                ours.step,
                pick as u32,
                &ours.legal_actions[pick].stable_id,
            )
            .unwrap();
        shared += 1;
    };
    assert_eq!(
        terminal.terminal_reason,
        format!("physical_decision_cap_reached:{CAP}")
    );
    assert!(
        passed > 0 && shared + passed == CAP,
        "{shared} shared, {passed} passed"
    );
    assert_eq!(plain.physical_decision_count(), shared);
    assert!(
        decision_of(&plain).is_some(),
        "the unflagged game still has decisions under the same cap"
    );
}

#[test]
fn with_no_seat_flagged_the_windows_reset_is_the_plain_reset() {
    for (index, deck) in ["Burn", "Faeries", "Elves"].iter().enumerate() {
        let seed = 61 + index as u64;
        let decks: SessionDeckIdsV1 = [deck.to_string(), deck.to_string()];
        let mut plain = RlEpisodeSessionV1::reset_pauper_london_human_v1(
            7, seed, LIMITS.0, LIMITS.1, decks, LEARNED,
        )
        .unwrap();
        let (mut unflagged, mut copy) = pair([deck, deck], seed, 7, [false; 2]);
        let mut rng = SplitMix64::seed(seed);
        for _ in 0..600 {
            assert_eq!(plain.current_response(), unflagged.current_response());
            assert_eq!(unflagged.engine_window_v1(), None);
            let Some(decision) = decision_of(&unflagged) else {
                break;
            };
            let rows = copy.row_candidate_ids(&decision).unwrap();
            let pick = rows[(rng.next_u64() % rows.len() as u64) as usize];
            let chosen = &decision.legal_actions[pick as usize];
            plain
                .step(decision.episode_id, decision.step, pick, &chosen.stable_id)
                .unwrap();
            answer(&mut unflagged, &mut copy, &decision, &rows, pick);
        }
    }
}

fn card_of(raw: &RlEpisodeSessionV1, arena_id: u32) -> &'static crate::card_def::CardDef {
    &CARD_DEFS[raw.game_state().objects.get(ObjectId(arena_id)).card_def as usize]
}

/// The row that casts an instant, when the menu has one.
fn instant_row(
    raw: &RlEpisodeSessionV1,
    decision: &RlSessionDecisionV1,
    rows: &[u32],
) -> Option<u32> {
    rows.iter().copied().find(|id| {
        matches!(
            &decision.legal_actions[*id as usize].semantic,
            ActionSemanticV1::CastSpell { source, .. }
                if card_of(raw, source.arena_id).types.contains(&CardType::Instant)
        )
    })
}

#[derive(Default)]
struct Scripted {
    end_step_casts: u32,
    upkeep_casts: u32,
    held_priority: u32,
    other_seat_decisions: u64,
}

/// One game: the person (flagged seat) plays lands, keeps their spells for
/// the new windows and casts an instant in the other seat's end step and in
/// their own upkeep whenever one is offered; once, in a main phase, they
/// hold priority and cast a second spell on top of their own. The other seat
/// answers at random. Asserts what the other seat is and is not asked.
fn scripted_game(seed: u64, person: usize, tally: &mut Scripted) {
    let mut seats = [false; 2];
    seats[person] = true;
    let (mut raw, mut copy) = pair(["Burn", "Burn"], seed, 0xE07_1000 + seed, seats);
    let mut rng = SplitMix64::seed(seed ^ 0xB07);
    // While set: the step and turn in which the person cast in a gated step;
    // the other seat must not be asked before the stack is empty again.
    let mut resolving: Option<(Step, u32)> = None;
    let mut holding: Option<usize> = None;
    for _ in 0..6_000 {
        let response = raw.current_response();
        copy.verify(&raw, &response).unwrap();
        let RlSessionResponseV1::Decision(decision) = response else {
            return;
        };
        let state = raw.game_state();
        let (step, turn, stack) = (state.step, state.turn, state.stack.len());
        let mine_turn = state.active_player.index() == person;
        let actor = seat_index(decision.acting_player);
        let rows = copy.row_candidate_ids(&decision).unwrap();
        let window = raw.engine_window_v1();
        let priority = rows.iter().any(|id| {
            matches!(
                decision.legal_actions[*id as usize].semantic,
                ActionSemanticV1::Pass { .. }
            )
        });
        if let Some((cast_step, cast_turn)) = resolving {
            if stack == 0 || (step, turn) != (cast_step, cast_turn) {
                resolving = None;
            }
        }
        if actor != person {
            tally.other_seat_decisions += 1;
            assert_eq!(window, None, "the other seat is never given a new window");
            if priority {
                // The trained surface: priority only in the main phases and
                // the declare steps, never with its own fresh spell on top.
                assert!(
                    !harness_never_offers_priority(step),
                    "seed {seed}: the other seat was given priority in {step:?}"
                );
                assert!(
                    resolving.is_none(),
                    "seed {seed}: the other seat was asked while the person's {step:?} spell was on the stack"
                );
            }
            let pick = rows[(rng.next_u64() % rows.len() as u64) as usize];
            answer(&mut raw, &mut copy, &decision, &rows, pick);
            continue;
        }
        if !priority {
            // Targets, discards, blocks, mulligans: the first row.
            answer(&mut raw, &mut copy, &decision, &rows, rows[0]);
            continue;
        }
        if let Some(before) = holding.take() {
            // The decision right after the person's own main-phase cast: the
            // trained surface passes it; the person is asked, with their own
            // spell still on the stack, before the other seat has answered.
            assert_eq!(window, Some("stack_top_is_casters_own"), "seed {seed}");
            assert_eq!(stack, before + 1, "seed {seed}");
            assert_eq!(
                state.stack.last().map(|item| item.controller.index()),
                Some(person)
            );
            tally.held_priority += 1;
            if let Some(second) = instant_row(&raw, &decision, &rows) {
                answer(&mut raw, &mut copy, &decision, &rows, second);
                continue;
            }
            let pass = pass_row(&decision, &rows);
            answer(&mut raw, &mut copy, &decision, &rows, pass);
            continue;
        }
        let gated_cast = window == Some("step_gated")
            && stack == 0
            && ((step == Step::End && !mine_turn) || (step == Step::Upkeep && mine_turn));
        if gated_cast {
            if let Some(cast) = instant_row(&raw, &decision, &rows) {
                if step == Step::End {
                    tally.end_step_casts += 1;
                } else {
                    tally.upkeep_casts += 1;
                }
                resolving = Some((step, turn));
                answer(&mut raw, &mut copy, &decision, &rows, cast);
                continue;
            }
        }
        if window.is_none() && stack == 0 && mine_turn && matches!(step, Step::Main1 | Step::Main2)
        {
            let land = rows.iter().copied().find(|id| {
                matches!(
                    decision.legal_actions[*id as usize].semantic,
                    ActionSemanticV1::PlayLand { .. }
                )
            });
            if let Some(land) = land {
                answer(&mut raw, &mut copy, &decision, &rows, land);
                continue;
            }
            // Late in the game, once: cast in the main phase and hold.
            if tally.held_priority == 0 && turn >= 8 {
                if let Some(cast) = instant_row(&raw, &decision, &rows) {
                    holding = Some(stack);
                    answer(&mut raw, &mut copy, &decision, &rows, cast);
                    continue;
                }
            }
        }
        let pass = pass_row(&decision, &rows);
        answer(&mut raw, &mut copy, &decision, &rows, pass);
    }
}

#[test]
fn a_person_casts_an_instant_in_the_other_seats_end_step_and_in_their_own_upkeep() {
    let mut tally = Scripted::default();
    for seed in [3_u64, 5, 8, 13, 21, 34] {
        scripted_game(seed, (seed % 2) as usize, &mut tally);
    }
    assert!(
        tally.end_step_casts >= 3,
        "end step casts: {}",
        tally.end_step_casts
    );
    assert!(
        tally.upkeep_casts >= 3,
        "upkeep casts: {}",
        tally.upkeep_casts
    );
    assert!(
        tally.held_priority >= 1,
        "held priority: {}",
        tally.held_priority
    );
    assert!(
        tally.other_seat_decisions > 100,
        "{}",
        tally.other_seat_decisions
    );
}

#[test]
fn engine_windows_are_refused_outside_the_trained_harness_mode() {
    let mut surface = crate::surface_v2::HarnessSurfaceV2::new_with_priority_mode_v1(
        crate::surface_v2::PriorityModeV1::EngineWindowsV1,
        crate::surface_v2::SuppressionAuditMode::Off,
    );
    assert!(surface.set_engine_window_seats_v1([true, false]).is_err());
    assert!(surface.set_engine_window_seats_v1([false, false]).is_ok());
    let mut harness = crate::surface_v2::HarnessSurfaceV2::new();
    assert!(harness.set_engine_window_seats_v1([false, true]).is_ok());
    assert_eq!(harness.engine_window_seats_v1(), [false, true]);
    // The public context (observation and tensor input) does not carry it.
    assert_eq!(
        harness.public_context(),
        crate::surface_v2::HarnessSurfaceV2::new().public_context()
    );
}

fn request(kind: &str, id: &str, extra: Value) -> String {
    let mut value = json!({"protocol": "spellbench/v1", "request_type": kind, "request_id": id});
    for (key, item) in extra.as_object().unwrap() {
        value[key] = item.clone();
    }
    serde_json::to_string(&value).unwrap()
}

fn reset_request(game: &str, seed: u64, seats: Option<Value>) -> String {
    let mut fields = json!({"game_id": game, "format": "pauper-bo1", "game_seed": seed,
        "max_decisions": 100_000, "max_steps": 1_000_000,
        "seats": [{"seat": "p0", "deck": {"catalog_id": "Burn"}},
                  {"seat": "p1", "deck": {"catalog_id": "Burn"}}]});
    if let Some(seats) = seats {
        fields["engine_window_seats"] = seats;
    }
    request("reset", &format!("reset-{game}"), fields)
}

fn london_server() -> SpellbenchBridgeServerV1 {
    SpellbenchBridgeServerV1::with_pauper_london_human_v1(
        SpellbenchEngineIdentityV1::with_source_revision(None),
    )
}

/// Plays one bridge game with seeded answers; a decision marked
/// `engine_window` is answered Pass. Returns every response line and the
/// number of marked decisions.
fn bridge_game(seats: Option<Value>, seed: u64) -> (Vec<String>, u64) {
    let mut server = london_server();
    let mut lines = vec![server.handle_line(&reset_request("gW", seed, seats))];
    let mut rng = SplitMix64::seed(seed ^ 0xFEED);
    let mut marked = 0;
    for step in 0..2_500 {
        let response: Value = serde_json::from_str(lines.last().unwrap()).unwrap();
        if response["response_type"] != "decision" {
            break;
        }
        let candidates = response["candidates"].as_array().unwrap();
        let rows: Vec<usize> = response["extensions"]["x_kernel_flat_v7"]["row_candidate_ids"]
            .as_array()
            .unwrap()
            .iter()
            .map(|id| id.as_u64().unwrap() as usize)
            .collect();
        let window = &response["extensions"]["x_kernel_v2_support"]["engine_window"];
        let pick = if window.is_string() {
            marked += 1;
            *rows
                .iter()
                .find(|row| candidates[**row]["semantic"]["kind"] == "pass")
                .unwrap()
        } else {
            rows[(rng.next_u64() % rows.len() as u64) as usize]
        };
        lines.push(server.handle_line(&request(
            "step",
            &format!("s{step}"),
            json!({"game_id": "gW", "expected_step": response["step"],
                   "selection": {"candidate_id": candidates[pick]["candidate_id"],
                                 "semantic_echo": candidates[pick]["semantic"]}}),
        )));
    }
    (lines, marked)
}

#[test]
fn the_bridge_takes_the_seats_on_reset_and_marks_the_new_windows() {
    // Absent and empty: the bytes of the bridge without the field.
    let (plain, none) = bridge_game(None, 17);
    let (empty, _) = bridge_game(Some(json!([])), 17);
    assert_eq!(none, 0);
    assert_eq!(plain, empty);
    assert!(plain.iter().all(|line| !line.contains("engine_window")));
    // Flagged: new windows are marked, and passing them gives the other
    // seat the same decisions and the same model input as the plain game.
    let (flagged, marked) = bridge_game(Some(json!(["p1"])), 17);
    assert!(marked > 10, "{marked} marked windows");
    let other_seat = |lines: &[String]| -> Vec<(Value, Value)> {
        lines
            .iter()
            .map(|line| serde_json::from_str::<Value>(line).unwrap())
            .filter(|value| value["response_type"] == "decision" && value["acting_seat"] == "p0")
            .map(|value| {
                (
                    value["candidates"].clone(),
                    value["extensions"]["x_kernel_flat_v7"]["tensor"].clone(),
                )
            })
            .collect()
    };
    let (theirs, ours) = (other_seat(&plain), other_seat(&flagged));
    assert!(theirs.len() > 20);
    assert_eq!(theirs, ours);
    assert_eq!(
        serde_json::from_str::<Value>(plain.last().unwrap()).unwrap()["outcome"],
        serde_json::from_str::<Value>(flagged.last().unwrap()).unwrap()["outcome"]
    );
}

#[test]
fn the_bridge_refuses_engine_window_seats_it_cannot_honour() {
    for bad in [json!(["p2"]), json!(["p0", "p0"])] {
        let mut server = london_server();
        let response: Value =
            serde_json::from_str(&server.handle_line(&reset_request("gB", 5, Some(bad)))).unwrap();
        assert_eq!(response["error"]["code"], "malformed_request", "{response}");
    }
    // The rated bridge modes never take the field.
    for options in [
        crate::agent_bridge_v1::SpellbenchBridgeOptionsV1::default(),
        crate::agent_bridge_v1::SpellbenchBridgeOptionsV1 {
            x_kernel_flat_v4: true,
            x_kernel_infoset_v1: false,
            private_v2_support: true,
        },
    ] {
        let mut server = SpellbenchBridgeServerV1::with_options(
            SpellbenchEngineIdentityV1::with_source_revision(None),
            options,
        );
        let response: Value =
            serde_json::from_str(&server.handle_line(&reset_request("gR", 5, Some(json!(["p0"])))))
                .unwrap();
        assert_eq!(response["error"]["code"], "malformed_request", "{response}");
        // And still plays without it.
        let response: Value =
            serde_json::from_str(&server.handle_line(&reset_request("gR2", 5, None))).unwrap();
        assert_eq!(response["response_type"], "decision", "{response}");
    }
}
