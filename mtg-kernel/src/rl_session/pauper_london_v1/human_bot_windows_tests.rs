//! The bot seat on its training window rule in human play
//! (`reset_pauper_london_human_seat_windows_v1`, the bridge reset field
//! `priority_windows`).
//!
//! Proven here:
//!
//! 1. The bot seat on `action-windows-v1`, the person's seat on engine
//!    windows (or on nothing): at every bot decision the bridge's sessions
//!    (the raw session and its lockstep copy) and the trainer's own London
//!    session (`FastActorSessionV1::reset_pauper_london_windows_v1`, the
//!    bot's seat on `action-windows-v1`, the other on `harness-v2`, the
//!    training runtime rule where the game allows it) have the same game
//!    state, the same menu, the same model input and the same trainer
//!    marker of a new priority window (`priority_pass_window_v1`). Both
//!    take the same deal: the trainer's explicit-deck deal, handed to the
//!    constructor the bridge reset calls after its own deal. The person
//!    passes at every engine window the trainer's other seat does not have,
//!    and answers everything else as the training game is answered.
//!    Neutral choices (on in every human-play game) are run both ways: on
//!    the trainer's session too (then everything is equal), and off it, the
//!    production trainer exactly; then the bridge poses some choices training
//!    resolves itself. For the person's revealed hand or settled graveyard partition, the script
//!    uses the trainer's actual move order. Other bridge-only choices must
//!    have exactly one answer, and at every shared
//!    bot decision the menu, the model input and the new-window marker are
//!    equal (the game states are not: neutral choices keep a pending
//!    activation in their own fields and number stack items differently).
//! 2. Both rules `harness-v2` is the engine-windows reset exactly.
//! 3. The bridge takes the rules on its reset request, marks the bot's new
//!    windows in the private support block, and refuses the field anywhere
//!    but the human-play London server and on a seat with engine windows.

use super::*;
use crate::agent_bridge_flat_v4::FlatV4LockstepV1;
use crate::agent_bridge_flat_v4_encode::{
    encode_flat_v7_tensor_v1, flat_v7_extension_value_v1, FlatV4EncodeScratchV1,
};
use crate::agent_bridge_v1::{SpellbenchBridgeServerV1, SpellbenchEngineIdentityV1};
use crate::rl::{ActionSemanticV1, PlayerSeatV1};
use crate::state::SplitMix64;
use serde_json::{json, Value};

const DECKS: [&str; 8] = [
    "Wildfire", "Rally", "Affinity", "Elves", "Spy", "Burn", "CawGates", "Faeries",
];
const LEARNED: [PauperMulliganModeV1; 2] = [PauperMulliganModeV1::Learned; 2];
const LIMITS: (u64, u64) = (200_000, 2_000_000);
const AW: SeatPriorityWindowsV1 = SeatPriorityWindowsV1::ActionWindowsV1;
const HV2: SeatPriorityWindowsV1 = SeatPriorityWindowsV1::HarnessV2;

fn seat_index(seat: PlayerSeatV1) -> usize {
    match seat {
        PlayerSeatV1::P0 => 0,
        PlayerSeatV1::P1 => 1,
    }
}

fn mainboards(decks: &SessionDeckIdsV1) -> [Vec<u16>; 2] {
    let resolved = resolve_runtime_decks(decks).unwrap();
    [resolved[0].card_ids.to_vec(), resolved[1].card_ids.to_vec()]
}

fn tensor(fast: &FastActorSessionV1, rows: &[u32], scratch: &mut FlatV4EncodeScratchV1) -> Value {
    let encoded = encode_flat_v7_tensor_v1(fast, scratch).unwrap();
    let mut value = flat_v7_extension_value_v1(&encoded, rows).unwrap();
    value["tensor"].take()
}

#[derive(Clone, Copy)]
enum NeutralComparison {
    BothNeutral,
    BridgeNeutral,
    TrainerPartitionForBot,
}

struct Sessions {
    training: FastActorSessionV1,
    raw: RlEpisodeSessionV1,
    copy: FlatV4LockstepV1,
}

/// The trainer's London session and the bridge's two sessions on one deal.
/// The training runtime rule is set on all three (refused alike for a game
/// with a Delve card). Neutral choices are the bridge's (every human-play
/// game has them): set on the trainer's session too with
/// `neutral_training`, else the trainer's session is production's.
fn sessions(
    decks: [&str; 2],
    seed: u64,
    episode: u64,
    bot: usize,
    person_windows: bool,
    comparison: NeutralComparison,
) -> Sessions {
    let deck_ids: SessionDeckIdsV1 = [decks[0].to_string(), decks[1].to_string()];
    let boards = mainboards(&deck_ids);
    let mut rules = [HV2; 2];
    rules[bot] = AW;
    let mut training = FastActorSessionV1::reset_pauper_london_windows_v1(
        episode,
        seed,
        LIMITS.0,
        LIMITS.1,
        deck_ids.clone(),
        boards.clone(),
        PlayerId::P0,
        LEARNED,
        rules,
    )
    .unwrap();
    let mut engine_windows = [false; 2];
    engine_windows[1 - bot] = person_windows;
    let start = LondonHumanStartV1 {
        episode_id: episode,
        max_physical_decisions: LIMITS.0,
        max_policy_steps: LIMITS.1,
        modes: LEARNED,
        engine_window_seats: engine_windows,
        action_windows: rules,
        trainer_choice_seats: std::array::from_fn(|seat| {
            seat == bot && matches!(comparison, NeutralComparison::TrainerPartitionForBot)
        }),
    };
    let deal = || {
        build_session_deck_pair_state_from_explicit_decks(&boards, seed, Some(PlayerId::P0))
            .unwrap()
    };
    let (hashes, state) = deal();
    let mut raw =
        RlEpisodeSessionV1::london_human_from_state_v1(start, deck_ids.clone(), hashes, state)
            .unwrap();
    let (hashes, state) = deal();
    let fast =
        FastActorSessionV1::london_human_from_state_v1(start, deck_ids, hashes, state).unwrap();
    let mut copy = FlatV4LockstepV1::from_fast_session_for_test(episode, fast);
    if matches!(comparison, NeutralComparison::BothNeutral) {
        training.enable_spellbench_neutral_choices();
    }
    raw.enable_spellbench_neutral_choices();
    copy.enable_spellbench_neutral_choices();
    let rule = crate::engine::RuntimeRulesV1::RESOLUTION_BOUNDARY_V1;
    let allowed = [
        training.set_runtime_rules_v1(rule).is_ok(),
        raw.set_runtime_rules_v1(rule).is_ok(),
        copy.set_runtime_rules_v1(rule).is_ok(),
    ];
    assert!(allowed == [true; 3] || allowed == [false; 3], "{allowed:?}");
    Sessions {
        training,
        raw,
        copy,
    }
}

#[derive(Default, Debug)]
struct Played {
    bot_decisions: u64,
    bot_new_windows: u64,
    person_engine_windows: u64,
    /// Decisions only the bridge poses, and how many of them belong to the bot.
    neutral_only: u64,
    bot_neutral_only: u64,
    person_revealed_order_picks: u64,
    bot_after_person_revealed_order: u64,
    shared: u64,
    finished: bool,
}

/// The first paths at which the two game states differ (diagnostics).
fn state_diff(training: &FastActorSessionV1, raw: &RlEpisodeSessionV1) -> Vec<String> {
    fn walk(a: &Value, b: &Value, path: String, out: &mut Vec<String>) {
        if out.len() >= 12 || a == b {
            return;
        }
        match (a, b) {
            (Value::Object(x), Value::Object(y)) => {
                for key in x.keys().chain(y.keys().filter(|k| !x.contains_key(*k))) {
                    walk(
                        x.get(key).unwrap_or(&Value::Null),
                        y.get(key).unwrap_or(&Value::Null),
                        format!("{path}.{key}"),
                        out,
                    );
                }
            }
            (Value::Array(x), Value::Array(y)) if x.len() == y.len() => {
                for (i, (p, q)) in x.iter().zip(y).enumerate() {
                    walk(p, q, format!("{path}[{i}]"), out);
                }
            }
            _ => out.push(format!(
                "{path}: {} vs {}",
                a.to_string().chars().take(200).collect::<String>(),
                b.to_string().chars().take(200).collect::<String>()
            )),
        }
    }
    let mut out = Vec::new();
    walk(
        &serde_json::to_value(training.game_state()).unwrap(),
        &serde_json::to_value(raw.game_state()).unwrap(),
        String::new(),
        &mut out,
    );
    out
}

/// Match the person's revealed-card order to the production trainer's committed
/// zone order. A graveyard-order decision that the trainer exposes stays shared.
fn person_revealed_order_pick(
    raw: &RlEpisodeSessionV1,
    training: &FastActorSessionV1,
    decision: &RlSessionDecisionV1,
    rows: &[u32],
) -> Option<u32> {
    use crate::effect::{
        EffectTargetSelectionPurpose::LookTopSelectByTypeToHandBottomRest,
        LibraryPartitionSelectionStage, PendingEffectChoice::SelectTargets,
    };
    use crate::rl::TargetRefV1;
    use crate::state::Target;

    let state = raw.game_state();
    let SelectTargets {
        player,
        selected,
        legal,
        min_targets,
        max_targets,
        purpose:
            LookTopSelectByTypeToHandBottomRest {
                stage,
                canonical_path,
                original_prefix,
                card_type,
                ..
            },
        ..
    } = state.engine.pending_effect.as_ref()?.choice.as_ref()?
    else {
        return None;
    };
    if state
        .engine
        .spellbench_neutral_v1
        .revealed_partition
        .as_ref()
        != Some(canonical_path)
    {
        return None;
    }
    let count = usize::from(*max_targets);
    assert_eq!(min_targets, max_targets);
    assert_eq!(selected.len() + legal.len(), count);
    let baseline = training.game_state();
    let matches_type = |binding: &crate::effect::EffectObjectBinding| {
        let card = state.objects.get(binding.object);
        crate::card_def::CARD_DEFS[card.card_def as usize].has_type(*card_type)
    };
    let player_state = &baseline.players[player.index()];
    let (destination, to_hand) = match stage {
        LibraryPartitionSelectionStage::ChooseMatchingSubset => (&player_state.hand, true),
        LibraryPartitionSelectionStage::OrderRest { .. }
            if !crate::engine::graveyard_order_choice_exposed(baseline, *player) =>
        {
            (&player_state.graveyard, false)
        }
        LibraryPartitionSelectionStage::OrderRest { .. } => return None,
    };
    let expected: Vec<_> = original_prefix
        .iter()
        .filter(|card| matches_type(card) == to_hand)
        .collect();
    assert_eq!(expected.len(), count, "the whole revealed group must move");
    // Other graveyard entries can follow the nonmatches, including the spell.
    let moved: Vec<_> = destination
        .iter()
        .filter_map(|id| {
            let binding = expected.iter().find(|card| card.object == *id)?;
            assert_eq!(
                baseline.objects.get(*id).zone_change_count,
                binding.expected_zone_change_count + 1
            );
            Some(*id)
        })
        .collect();
    assert_eq!(
        moved.len(),
        count,
        "trainer moved each revealed card to its destination"
    );
    for (picked, id) in selected.iter().zip(&moved) {
        assert_eq!(picked.target, Target::Object(*id));
    }
    for id in &moved[selected.len()..] {
        assert!(legal
            .iter()
            .any(|candidate| candidate.target == Target::Object(*id)));
    }
    let next = moved[selected.len()];
    Some(*rows.iter().find(|&&row| matches!(
        &decision.legal_actions[row as usize].semantic,
        ActionSemanticV1::ChooseEffectTarget { target: TargetRefV1::Object { object }, .. }
            if object.arena_id == next.0
    )).expect("the next card in the trainer's zone order has a bridge row"))
}

/// Plays the training session and the bridge sessions side by side with
/// seeded answers. The person's engine windows (absent from training) are
/// passed in the bridge sessions alone; so are the decisions only neutral
/// choices pose. For the person's revealed partition order, play the training order.
fn play(
    decks: [&str; 2],
    seed: u64,
    episode: u64,
    bot: usize,
    person_windows: bool,
    comparison: NeutralComparison,
    max_shared: u64,
) -> Played {
    let Sessions {
        mut training,
        mut raw,
        mut copy,
    } = sessions(decks, seed, episode, bot, person_windows, comparison);
    let neutral_training = matches!(comparison, NeutralComparison::BothNeutral);
    let mut rng = SplitMix64::seed(seed ^ 0xB07);
    let mut scratch = FlatV4EncodeScratchV1::default();
    let mut out = Played::default();
    let context = |what: &str, at: u64| {
        format!("{decks:?} seed {seed} bot p{bot} person windows {person_windows} neutral training {neutral_training}: {what} at shared decision {at}")
    };
    loop {
        let response = raw.current_response();
        copy.verify(&raw, &response).unwrap_or_else(|error| {
            panic!("{}", context(&format!("lockstep {error:?}"), out.shared))
        });
        if let (Some(_), RlSessionResponseV1::Decision(decision)) =
            (raw.engine_window_v1(), &response)
        {
            if seat_index(decision.acting_player) != bot {
                // The person's engine window: Pass, as the trained surface
                // would have; the training game never sees it.
                let rows = copy.row_candidate_ids(decision).unwrap();
                let pass = *rows
                    .iter()
                    .find(|id| {
                        matches!(
                            decision.legal_actions[**id as usize].semantic,
                            ActionSemanticV1::Pass { .. }
                        )
                    })
                    .expect("a priority window always offers Pass");
                let chosen = &decision.legal_actions[pass as usize];
                raw.step(decision.episode_id, decision.step, pass, &chosen.stable_id)
                    .unwrap();
                copy.mirror_step(&rows, decision.step, pass).unwrap();
                out.person_engine_windows += 1;
                continue;
            }
        }
        if let RlSessionResponseV1::Decision(decision) = &response {
            // Against the production trainer the states are not compared
            // (neutral choices hold a pending activation in their own
            // fields and number stack items differently); what the seat is
            // asked is: the acting seat, the substep and the menu.
            let shared = match training.current_response() {
                FastActorResponseV1::Decision(theirs) => {
                    theirs.acting_player == decision.acting_player
                        && (theirs.substep_index, theirs.substep_count)
                            == (decision.substep_index, decision.substep_count)
                        && training.diagnostic_current_action_semantics()
                            == copy.fast_session().diagnostic_current_action_semantics()
                }
                FastActorResponseV1::Terminal(_) => false,
            };
            if !shared && !neutral_training {
                if matches!(comparison, NeutralComparison::TrainerPartitionForBot)
                    && seat_index(decision.acting_player) == bot
                {
                    assert!(
                        raw.game_state()
                            .engine
                            .spellbench_neutral_v1
                            .revealed_partition
                            .is_none(),
                        "{}",
                        context("unexpected neutral partition on the bot", out.shared)
                    );
                }
                // Reproduce the person's training partition order if one is pending.
                // Every other unmatched menu, including the bot's, must be forced.
                let rows = copy.row_candidate_ids(decision).unwrap();
                let person_order = if seat_index(decision.acting_player) != bot && rows.len() > 1 {
                    person_revealed_order_pick(&raw, &training, decision, &rows)
                } else {
                    None
                };
                let pick = if let Some(pick) = person_order {
                    out.person_revealed_order_picks += 1;
                    pick
                } else {
                    assert_eq!(
                        rows.len(),
                        1,
                        "{} bridge {:?} training {:?} state differences {:?}",
                        context(
                            "a bridge-only decision with more than one answer",
                            out.shared
                        ),
                        copy.fast_session().diagnostic_current_action_semantics(),
                        training.diagnostic_current_action_semantics(),
                        state_diff(&training, &raw)
                    );
                    rows[0]
                };
                let chosen = &decision.legal_actions[pick as usize];
                raw.step(decision.episode_id, decision.step, pick, &chosen.stable_id)
                    .unwrap();
                copy.mirror_step(&rows, decision.step, pick).unwrap();
                out.neutral_only += 1;
                out.bot_neutral_only += u64::from(seat_index(decision.acting_player) == bot);
                continue;
            }
        }
        if neutral_training {
            assert_eq!(
                training.game_state(),
                raw.game_state(),
                "{}",
                context("game state", out.shared)
            );
        }
        let ahead = out.person_engine_windows + out.neutral_only;
        let (theirs, ours) = match (training.current_response(), response) {
            (FastActorResponseV1::Decision(theirs), RlSessionResponseV1::Decision(ours)) => {
                (theirs, ours)
            }
            (FastActorResponseV1::Terminal(theirs), RlSessionResponseV1::Terminal(ours)) => {
                assert_eq!(
                    (
                        theirs.terminal_outcome,
                        theirs.winner,
                        &theirs.terminal_reason
                    ),
                    (ours.terminal_outcome, ours.winner, &ours.terminal_reason),
                    "{}",
                    context("terminal", out.shared)
                );
                out.finished = true;
                return out;
            }
            _ => panic!(
                "{}",
                context("one game ended and the other did not", out.shared)
            ),
        };
        if out.shared >= max_shared {
            return out;
        }
        assert_eq!(
            theirs.acting_player,
            ours.acting_player,
            "{}",
            context("acting seat", out.shared)
        );
        assert_eq!(
            (theirs.substep_index, theirs.substep_count),
            (ours.substep_index, ours.substep_count),
            "{}",
            context("substep", out.shared)
        );
        assert_eq!(
            training.diagnostic_current_action_semantics(),
            copy.fast_session().diagnostic_current_action_semantics(),
            "{}",
            context("menu", out.shared)
        );
        // The session counters differ by exactly the bridge-only decisions.
        assert_eq!(
            (training.policy_step_count() + ahead, theirs.step + ahead),
            (raw.policy_step_count(), ours.step),
            "{}",
            context("counters", out.shared)
        );
        let rows = copy.row_candidate_ids(&ours).unwrap();
        if seat_index(ours.acting_player) == bot {
            out.bot_decisions += 1;
            out.bot_after_person_revealed_order += u64::from(out.person_revealed_order_picks > 0);
            let marker = training.priority_pass_window_v1().unwrap();
            assert_eq!(
                marker,
                copy.fast_session().priority_pass_window_v1().unwrap(),
                "{}",
                context("new-window marker", out.shared)
            );
            // The bridge marks exactly the trainer's new windows.
            assert_eq!(
                marker.is_some(),
                raw.engine_window_v1().is_some(),
                "{}",
                context("support mark", out.shared)
            );
            out.bot_new_windows += u64::from(marker.is_some());
            assert_eq!(
                tensor(&training, &rows, &mut scratch),
                tensor(copy.fast_session(), &rows, &mut scratch),
                "{}",
                context("model input", out.shared)
            );
        }
        let row = (rng.next_u64() % rows.len() as u64) as usize;
        let pick = rows[row];
        let chosen = &ours.legal_actions[pick as usize];
        raw.step(ours.episode_id, ours.step, pick, &chosen.stable_id)
            .unwrap();
        copy.mirror_step(&rows, ours.step, pick).unwrap();
        training
            .step(theirs.episode_id, theirs.step, row as u32)
            .unwrap();
        out.shared += 1;
    }
}

#[test]
fn shared_bot_input_after_persons_winding_way_hand_order() {
    let played = play(
        ["Wildfire", "Elves"],
        7,
        0xB0_0000 + 114,
        0,
        true,
        NeutralComparison::BridgeNeutral,
        76,
    );
    assert!(played.person_revealed_order_picks > 0, "{played:?}");
    assert!(played.bot_after_person_revealed_order > 0, "{played:?}");
    println!("Winding Way: {played:?}");
}

#[test]
fn shared_bot_input_after_persons_winding_way_graveyard_order() {
    let played = play(
        ["Spy", "Spy"],
        7,
        0xB0_0000 + 4014,
        0,
        true,
        NeutralComparison::BridgeNeutral,
        94,
    );
    assert!(played.person_revealed_order_picks >= 3, "{played:?}");
    assert!(played.bot_after_person_revealed_order > 0, "{played:?}");
    println!("Spy Winding Way: {played:?}");
}

#[test]
fn trainer_partition_preserves_shared_spy_bot_decisions_in_both_seats() {
    let mut extra_bot_decisions = [0; 2];
    for (bot, last_shared) in [(0, 277), (1, 65)] {
        let played = play(
            ["Spy", "Spy"],
            7,
            0xB0_0000 + 4014 + bot as u64,
            bot,
            true,
            NeutralComparison::TrainerPartitionForBot,
            last_shared,
        );
        assert_eq!(played.shared, last_shared, "{played:?}");
        if bot == 0 {
            assert!(played.person_revealed_order_picks >= 3, "{played:?}");
        }
        extra_bot_decisions[bot] = played.bot_neutral_only;
        println!("Spy seat {bot}: {played:?}");
    }
    assert_eq!(
        extra_bot_decisions,
        [0, 0],
        "the production trainer has no corresponding decisions"
    );
}

#[test]
fn the_bot_on_action_windows_plays_the_training_session() {
    let mut total = Played::default();
    let mut games = 0;
    for (index, deck) in DECKS.iter().enumerate() {
        let other = DECKS[(index + 3) % DECKS.len()];
        for (pairing, decks) in [[*deck, *deck], [*deck, other]].into_iter().enumerate() {
            for seed in [7_u64, 31] {
                for bot in [0, 1] {
                    for (person_windows, neutral_training) in
                        [(true, true), (false, true), (true, false), (false, false)]
                    {
                        let episode = 0xB0_0000
                            + index as u64 * 1_000
                            + pairing as u64 * 100
                            + seed * 2
                            + bot as u64;
                        let played = play(
                            decks,
                            seed,
                            episode,
                            bot,
                            person_windows,
                            if neutral_training {
                                NeutralComparison::BothNeutral
                            } else {
                                NeutralComparison::BridgeNeutral
                            },
                            8_000,
                        );
                        if neutral_training {
                            assert_eq!(played.neutral_only, 0);
                        }
                        assert!(
                            played.finished,
                            "{decks:?} seed {seed} bot p{bot}: not finished ({played:?})"
                        );
                        assert!(
                            played.bot_decisions > 10,
                            "{decks:?} seed {seed}: {played:?}"
                        );
                        if !person_windows {
                            assert_eq!(played.person_engine_windows, 0);
                        }
                        total.bot_decisions += played.bot_decisions;
                        total.bot_new_windows += played.bot_new_windows;
                        total.person_engine_windows += played.person_engine_windows;
                        total.neutral_only += played.neutral_only;
                        total.person_revealed_order_picks += played.person_revealed_order_picks;
                        total.bot_after_person_revealed_order +=
                            played.bot_after_person_revealed_order;
                        total.bot_neutral_only += played.bot_neutral_only;
                        total.shared += played.shared;
                        games += 1;
                    }
                }
            }
        }
    }
    println!("human bot windows proof: {games} games, {total:?}");
    // The proof is not vacuous: the bot meets new windows and the person
    // meets engine windows the training game does not have.
    assert!(total.bot_new_windows > 100, "{total:?}");
    assert!(total.person_engine_windows > 100, "{total:?}");
}

#[test]
fn both_rules_harness_v2_is_the_engine_windows_reset() {
    for seats in [[false; 2], [true, false], [false, true]] {
        let decks: SessionDeckIdsV1 = ["Spy".to_string(), "Faeries".to_string()];
        let old = RlEpisodeSessionV1::reset_pauper_london_human_windows_v1(
            7,
            19,
            LIMITS.0,
            LIMITS.1,
            decks.clone(),
            LEARNED,
            seats,
        )
        .unwrap();
        let new = RlEpisodeSessionV1::reset_pauper_london_human_seat_windows_v1(
            7,
            19,
            LIMITS.0,
            LIMITS.1,
            decks.clone(),
            LEARNED,
            seats,
            [HV2; 2],
        )
        .unwrap();
        assert_eq!(old.game_state(), new.game_state());
        assert_eq!(old.current_response(), new.current_response());
        // A seat cannot have both.
        for seat in 0..2 {
            let mut rules = [HV2; 2];
            rules[seat] = AW;
            let both = RlEpisodeSessionV1::reset_pauper_london_human_seat_windows_v1(
                7,
                19,
                LIMITS.0,
                LIMITS.1,
                decks.clone(),
                LEARNED,
                seats,
                rules,
            );
            assert_eq!(both.is_err(), seats[seat], "{seats:?} {rules:?}");
            let fast = FastActorSessionV1::reset_pauper_london_human_seat_windows_v1(
                7,
                19,
                LIMITS.0,
                LIMITS.1,
                decks.clone(),
                LEARNED,
                seats,
                rules,
            );
            assert_eq!(fast.is_err(), seats[seat], "{seats:?} {rules:?}");
        }
    }
}

fn request(kind: &str, id: &str, extra: Value) -> String {
    let mut value = json!({"protocol": "spellbench/v1", "request_type": kind, "request_id": id});
    for (key, item) in extra.as_object().unwrap() {
        value[key] = item.clone();
    }
    serde_json::to_string(&value).unwrap()
}

fn reset_request(game: &str, seed: u64, extra: Value) -> String {
    let mut fields = json!({"game_id": game, "format": "pauper-bo1", "game_seed": seed,
        "max_decisions": 100_000, "max_steps": 1_000_000,
        "seats": [{"seat": "p0", "deck": {"catalog_id": "Burn"}},
                  {"seat": "p1", "deck": {"catalog_id": "Faeries"}}]});
    for (key, item) in extra.as_object().unwrap() {
        fields[key] = item.clone();
    }
    request("reset", &format!("reset-{game}"), fields)
}

fn london_server() -> SpellbenchBridgeServerV1 {
    SpellbenchBridgeServerV1::with_pauper_london_human_v1(
        SpellbenchEngineIdentityV1::with_source_revision(None),
    )
}

/// One bridge game with seeded answers (a marked decision of `pass_seat` is
/// answered Pass). Returns the response lines and the marked decisions per
/// seat.
fn bridge_game(extra: Value, seed: u64, pass_seat: Option<&str>) -> (Vec<String>, [u64; 2]) {
    let mut server = london_server();
    let mut lines = vec![server.handle_line(&reset_request("gA", seed, extra))];
    let mut rng = SplitMix64::seed(seed ^ 0xA11);
    let mut marked = [0; 2];
    for step in 0..4_000 {
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
        let seat = response["acting_seat"].as_str().unwrap().to_string();
        let window = response["extensions"]["x_kernel_v2_support"]["engine_window"].is_string();
        if window {
            marked[usize::from(seat == "p1")] += 1;
        }
        let pick = if window && pass_seat == Some(seat.as_str()) {
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
            json!({"game_id": "gA", "expected_step": response["step"],
                   "selection": {"candidate_id": candidates[pick]["candidate_id"],
                                 "semantic_echo": candidates[pick]["semantic"]}}),
        )));
    }
    (lines, marked)
}

#[test]
fn the_bridge_takes_the_rules_on_reset_and_marks_the_bots_new_windows() {
    // Absent and both harness-v2: the bytes of the bridge without the field.
    let (plain, none) = bridge_game(
        json!({"engine_window_seats": ["p0"], "auto_graveyard_order": true}),
        23,
        Some("p0"),
    );
    let (spelled, _) = bridge_game(
        json!({"engine_window_seats": ["p0"], "auto_graveyard_order": true,
               "priority_windows": ["harness-v2", "harness-v2"]}),
        23,
        Some("p0"),
    );
    assert_eq!(plain, spelled);
    assert_eq!(none[1], 0);
    // The bot in p1 on action windows: its new windows are marked; the
    // person in p0 keeps engine windows; the game plays to its end.
    let (bot, marked) = bridge_game(
        json!({"engine_window_seats": ["p0"], "auto_graveyard_order": true,
               "priority_windows": ["harness-v2", "action-windows-v1"]}),
        23,
        Some("p0"),
    );
    assert!(marked[1] > 0, "{marked:?}");
    assert!(marked[0] > 0, "{marked:?}");
    let last: Value = serde_json::from_str(bot.last().unwrap()).unwrap();
    assert_eq!(last["response_type"], "terminal", "{last}");
}

#[test]
fn the_bridge_refuses_priority_windows_it_cannot_honour() {
    let mut server = london_server();
    let response: Value = serde_json::from_str(&server.handle_line(&reset_request(
        "gC",
        5,
        json!({"engine_window_seats": ["p1"], "priority_windows": ["harness-v2", "action-windows-v1"]}),
    )))
    .unwrap();
    assert_eq!(response["error"]["code"], "malformed_request", "{response}");
    let response: Value = serde_json::from_str(&server.handle_line(&reset_request(
        "gD",
        5,
        json!({"priority_windows": ["harness-v2", "every-window"]}),
    )))
    .unwrap();
    assert_eq!(response["response_type"], "error", "{response}");
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
        let response: Value = serde_json::from_str(&server.handle_line(&reset_request(
            "gR",
            5,
            json!({"priority_windows": ["action-windows-v1", "harness-v2"]}),
        )))
        .unwrap();
        assert_eq!(response["error"]["code"], "malformed_request", "{response}");
        // Both harness-v2 is today's surface, taken anywhere.
        let response: Value = serde_json::from_str(&server.handle_line(&reset_request(
            "gR2",
            5,
            json!({"priority_windows": ["harness-v2", "harness-v2"]}),
        )))
        .unwrap();
        assert_eq!(response["response_type"], "decision", "{response}");
    }
}
