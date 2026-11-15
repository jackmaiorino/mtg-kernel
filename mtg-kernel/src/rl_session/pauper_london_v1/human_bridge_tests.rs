//! The human-play London bridge mode (`--pauper-london-human-v1`): the raw
//! session's London twin follows the fast session in lockstep through whole
//! games on every catalog deck (random answers, forced zero-card keeps), a
//! both-keep game equals the keep-seven twin, the `x_kernel_flat_v7`
//! extension tensor equals the harness tensor, the bridge speaks the two
//! London kinds, and no hidden card of the other seat reaches a decision.

use super::*;
use crate::agent_bridge_flat_v4::FlatV4LockstepV1;
use crate::agent_bridge_v1::{SpellbenchBridgeServerV1, SpellbenchEngineIdentityV1};
use crate::rl::{ActionSemanticV1, PlayerSeatV1};
use crate::sideboard_play_policy_v1::FrozenPlayPolicyV1;
use crate::state::SplitMix64;
use serde_json::{json, Value};

const DECKS: [&str; 8] = [
    "Wildfire", "Rally", "Affinity", "Elves", "Spy", "Burn", "CawGates", "Faeries",
];
const LEARNED: [PauperMulliganModeV1; 2] = [PauperMulliganModeV1::Learned; 2];

fn pair(
    deck: &str,
    seed: u64,
    episode: u64,
) -> (
    RlEpisodeSessionV1,
    crate::agent_bridge_flat_v4::FlatV4LockstepV1,
) {
    let decks: SessionDeckIdsV1 = [deck.to_string(), deck.to_string()];
    let raw = RlEpisodeSessionV1::reset_pauper_london_human_v1(
        episode,
        seed,
        2_000,
        200_000,
        decks.clone(),
        LEARNED,
    )
    .unwrap();
    let lockstep =
        FlatV4LockstepV1::reset_london_human(episode, seed, 2_000, 200_000, decks).unwrap();
    (raw, lockstep)
}

/// How a scripted seat answers a London announcement.
#[derive(Clone, Copy)]
enum Announce {
    Mulligan,
    Random,
}

struct Played {
    steps: u64,
    pregame_decisions: [u32; 2],
    terminal: bool,
}

/// Plays `raw` and `lockstep` together to the end (or `max_steps`), checking
/// the lockstep after every step. Non-London decisions pick uniformly.
fn play(
    raw: &mut RlEpisodeSessionV1,
    lockstep: &mut FlatV4LockstepV1,
    announce: Announce,
    seed: u64,
    max_steps: u64,
) -> Played {
    let mut rng = SplitMix64::seed(seed ^ 0xA11CE);
    let mut played = Played {
        steps: 0,
        pregame_decisions: [0; 2],
        terminal: false,
    };
    loop {
        let response = raw.current_response();
        lockstep
            .verify(raw, &response)
            .unwrap_or_else(|error| panic!("step {}: {error:?}", played.steps));
        let RlSessionResponseV1::Decision(decision) = response else {
            played.terminal = true;
            return played;
        };
        if played.steps >= max_steps {
            return played;
        }
        let rows = lockstep.row_candidate_ids(&decision).unwrap();
        let first = &decision.legal_actions[rows[0] as usize].semantic;
        let london = matches!(
            first,
            ActionSemanticV1::ChooseLondonMulligan { .. }
                | ActionSemanticV1::ChooseLondonBottom { .. }
        );
        let pick = if london {
            let seat = match decision.acting_player {
                PlayerSeatV1::P0 => 0,
                PlayerSeatV1::P1 => 1,
            };
            played.pregame_decisions[seat] += 1;
            match (first, announce) {
                (ActionSemanticV1::ChooseLondonMulligan { .. }, Announce::Mulligan) => *rows
                    .iter()
                    .find(|id| {
                        matches!(
                            decision.legal_actions[**id as usize].semantic,
                            ActionSemanticV1::ChooseLondonMulligan { mulligan: true, .. }
                        )
                    })
                    .unwrap(),
                _ => rows[(rng.next_u64() % rows.len() as u64) as usize],
            }
        } else {
            rows[(rng.next_u64() % rows.len() as u64) as usize]
        };
        let chosen = &decision.legal_actions[pick as usize];
        raw.step(decision.episode_id, decision.step, pick, &chosen.stable_id)
            .unwrap();
        lockstep.mirror_step(&rows, decision.step, pick).unwrap();
        played.steps += 1;
    }
}

#[test]
fn london_human_lockstep_follows_whole_games_on_every_catalog_deck() {
    let mut with_mulligans = 0;
    let mut finished = 0;
    for (index, deck) in DECKS.iter().enumerate() {
        for seed in [11_u64, 29, 47] {
            let episode = 0x10D0_0000 + index as u64 * 1_000 + seed;
            let (mut raw, mut lockstep) = pair(deck, seed, episode);
            let played = play(&mut raw, &mut lockstep, Announce::Random, seed, 3_000);
            assert!(
                played.steps > 20,
                "{deck} seed {seed}: {} steps",
                played.steps
            );
            finished += u32::from(played.terminal);
            let record = raw.pauper_opening_record_v1().expect("opening completed");
            assert_eq!(
                Some(record),
                lockstep.fast_session().pauper_opening_record_v1(),
                "{deck} seed {seed}: the two sessions record the same opening"
            );
            // The public facts after the opening name the same counts.
            let public = raw.pauper_london_public_v1().unwrap();
            assert_eq!(public.phase, "done");
            assert_eq!(
                public.mulligans_taken,
                [
                    record.seats[0].mulligans_taken,
                    record.seats[1].mulligans_taken
                ]
            );
            with_mulligans += u32::from(record.seats.iter().any(|seat| seat.mulligans_taken > 0));
        }
    }
    assert!(
        with_mulligans >= 6,
        "only {with_mulligans} of 24 games mulliganed"
    );
    assert!(finished >= 6, "only {finished} of 24 games finished");
}

#[test]
fn london_human_forced_zero_card_keep_runs_in_lockstep() {
    for (index, deck) in ["Burn", "Elves", "Spy"].iter().enumerate() {
        let episode = 0x10D1_0000 + index as u64;
        let (mut raw, mut lockstep) = pair(deck, 5 + index as u64, episode);
        let played = play(&mut raw, &mut lockstep, Announce::Mulligan, 3, 40_000);
        // Seven announcements and 1+2+..+7 = 28 bottoms are 35 pregame
        // decisions per seat (an empty hand keeps without a decision).
        assert_eq!(played.pregame_decisions, [35, 35], "{deck}");
        let record = raw.pauper_opening_record_v1().unwrap();
        assert!(record
            .seats
            .iter()
            .all(|seat| seat.forced_zero_card_keep && seat.kept_hand_size == 0));
        assert_eq!(
            Some(record),
            lockstep.fast_session().pauper_opening_record_v1()
        );
    }
}

#[test]
fn london_human_both_keep_game_equals_the_keep_seven_twin() {
    for deck in ["Burn", "Rally", "Faeries"] {
        let seed = 77;
        let (mut raw, mut lockstep) = pair(deck, seed, 9);
        // Keep both hands, then take the in-game decision.
        for _ in 0..2 {
            let RlSessionResponseV1::Decision(decision) = raw.current_response() else {
                panic!("pregame ended early");
            };
            let rows = lockstep.row_candidate_ids(&decision).unwrap();
            let pick = *rows
                .iter()
                .find(|id| {
                    matches!(
                        decision.legal_actions[**id as usize].semantic,
                        ActionSemanticV1::ChooseLondonMulligan {
                            mulligan: false,
                            ..
                        }
                    )
                })
                .unwrap();
            let chosen = &decision.legal_actions[pick as usize];
            raw.step(decision.episode_id, decision.step, pick, &chosen.stable_id)
                .unwrap();
            lockstep.mirror_step(&rows, decision.step, pick).unwrap();
        }
        let twin = RlEpisodeSessionV1::reset_with_decks_and_limits(
            9,
            seed,
            2_000,
            200_000,
            [deck.to_string(), deck.to_string()],
        )
        .unwrap();
        let (RlSessionResponseV1::Decision(london), RlSessionResponseV1::Decision(plain)) =
            (raw.current_response(), twin.current_response())
        else {
            panic!("no first in-game decision");
        };
        assert_eq!(london.acting_player, plain.acting_player);
        assert_eq!(
            london
                .legal_actions
                .iter()
                .map(|action| &action.semantic)
                .collect::<Vec<_>>(),
            plain
                .legal_actions
                .iter()
                .map(|action| &action.semantic)
                .collect::<Vec<_>>(),
            "{deck}: first in-game menu"
        );
        assert_eq!(
            serde_json::to_value(raw.game_state().players[0].hand.clone()).unwrap(),
            serde_json::to_value(twin.game_state().players[0].hand.clone()).unwrap(),
            "{deck}: the same opening hand"
        );
    }
}

fn line(value: Value) -> String {
    serde_json::to_string(&value).unwrap()
}

fn request(kind: &str, id: &str, extra: Value) -> String {
    let mut value = json!({"protocol": "spellbench/v1", "request_type": kind, "request_id": id});
    for (key, item) in extra.as_object().unwrap() {
        value[key] = item.clone();
    }
    line(value)
}

fn reset_request(deck: &str, game: &str, seed: u64) -> String {
    request(
        "reset",
        &format!("reset-{game}"),
        json!({"game_id": game, "format": "pauper-bo1", "game_seed": seed,
               "max_decisions": 100_000, "max_steps": 1_000_000,
               "seats": [{"seat": "p0", "deck": {"catalog_id": deck}},
                         {"seat": "p1", "deck": {"catalog_id": deck}}]}),
    )
}

fn london_server() -> SpellbenchBridgeServerV1 {
    SpellbenchBridgeServerV1::with_pauper_london_human_v1(
        SpellbenchEngineIdentityV1::with_source_revision(None),
    )
}

#[test]
fn london_human_bridge_speaks_the_london_kinds_and_flat_v7() {
    let mut server = london_server();
    let hello: Value =
        serde_json::from_str(&server.handle_line(&request("hello", "hello", json!({})))).unwrap();
    assert_eq!(
        hello["extensions"],
        json!(["x_kernel_v5", "x_kernel_flat_v7"])
    );
    let mut response: Value =
        serde_json::from_str(&server.handle_line(&reset_request("Burn", "gL", 13))).unwrap();
    let mut rng = SplitMix64::seed(0xBEEF);
    let mut kinds = std::collections::BTreeSet::new();
    let mut steps = 0_u64;
    let mut sent_flat = 0_u32;
    while response["response_type"] == "decision" && steps < 4_000 {
        let extensions = response["extensions"].as_object().unwrap();
        assert!(
            !extensions.contains_key("x_kernel_flat_v4"),
            "no V4 payload in London mode"
        );
        let flat = &extensions["x_kernel_flat_v7"];
        assert_eq!(
            flat["feature_contract_digest"],
            "1f894f0dcaaeb4e5ac0233e3f39d866c0dddca494f1afc8152107bded0551a4d"
        );
        assert_eq!(
            flat["feature_encoding_digest"],
            "4c592d136a74240f40b12c638203ab1758ed3cd63748ab7cd2788a60a78850a3"
        );
        assert_eq!(flat["acting_seat"], response["acting_seat"]);
        sent_flat += 1;
        let support = &extensions["x_kernel_v2_support"];
        let phase = support["london"]["phase"].as_str().unwrap().to_string();
        let candidates = response["candidates"].as_array().unwrap();
        let rows: Vec<usize> = flat["row_candidate_ids"]
            .as_array()
            .unwrap()
            .iter()
            .map(|id| id.as_u64().unwrap() as usize)
            .collect();
        let kind = candidates[rows[0]]["semantic"]["kind"]
            .as_str()
            .unwrap()
            .to_string();
        if phase != "done" {
            assert!(
                kind == "london_mulligan" || kind == "london_bottom",
                "pregame decision of kind {kind} in phase {phase}"
            );
            assert_eq!(
                kind == "london_mulligan",
                phase == "announce",
                "kind {kind} in phase {phase}"
            );
        } else {
            assert!(kind != "london_mulligan" && kind != "london_bottom");
        }
        kinds.insert(kind);
        let pick = rows[(rng.next_u64() % rows.len() as u64) as usize];
        let step = request(
            "step",
            &format!("s{steps}"),
            json!({"game_id": "gL", "expected_step": response["step"],
                   "selection": {"candidate_id": candidates[pick]["candidate_id"],
                                 "semantic_echo": candidates[pick]["semantic"]}}),
        );
        response = serde_json::from_str(&server.handle_line(&step)).unwrap();
        steps += 1;
    }
    assert!(kinds.contains("london_mulligan"), "{kinds:?}");
    assert!(sent_flat > 20);
    assert!(
        response["response_type"] == "terminal" && response["classification"] == "natural"
            || steps == 4_000,
        "{response}"
    );
}

#[test]
fn london_human_extension_tensor_equals_the_harness_tensor_on_pregame_and_in_game_decisions() {
    use crate::agent_bridge_flat_v4_encode::{
        encode_flat_v7_tensor_v1, flat_v7_extension_value_v1, tensor_bits_common_v1,
        FlatV4EncodeScratchV1,
    };
    let mut harness = FrozenPlayPolicyV1::training_fixture_v4()
        .into_flat_v7_transfer_v1()
        .unwrap();
    let mut scratch = FlatV4EncodeScratchV1::default();
    let (mut checked_pregame, mut checked_game) = (0, 0);
    for (index, deck) in ["Burn", "Elves", "Rally"].iter().enumerate() {
        let (mut raw, mut lockstep) = pair(deck, 21 + index as u64, 0x10D2_0000 + index as u64);
        let mut rng = SplitMix64::seed(index as u64 + 1);
        for _ in 0..160 {
            let response = raw.current_response();
            let RlSessionResponseV1::Decision(decision) = response else {
                break;
            };
            let rows = lockstep.row_candidate_ids(&decision).unwrap();
            let encoded = encode_flat_v7_tensor_v1(lockstep.fast_session(), &mut scratch).unwrap();
            harness
                .score_fast_session_v1(lockstep.fast_session())
                .unwrap();
            let theirs = harness.last_scored_training_tensor_v4().unwrap();
            let value = flat_v7_extension_value_v1(&encoded, &rows).unwrap();
            assert_eq!(
                value["tensor"],
                serde_json::to_value(tensor_bits_common_v1(&theirs.common)).unwrap(),
                "{deck} step {}",
                decision.step
            );
            if lockstep.fast_session().pauper_london_pending_v1() {
                checked_pregame += 1;
            } else {
                checked_game += 1;
            }
            let pick = rows[(rng.next_u64() % rows.len() as u64) as usize];
            let chosen = &decision.legal_actions[pick as usize];
            raw.step(decision.episode_id, decision.step, pick, &chosen.stable_id)
                .unwrap();
            lockstep.mirror_step(&rows, decision.step, pick).unwrap();
        }
    }
    assert!(
        checked_pregame >= 6,
        "pregame decisions checked: {checked_pregame}"
    );
    assert!(
        checked_game >= 100,
        "in-game decisions checked: {checked_game}"
    );
}

#[test]
fn london_human_decisions_never_name_the_other_seats_hand() {
    // Two sessions on one seed: one plays through the bridge, the other is
    // read for each seat's true hand at every pregame decision.
    let mut server = london_server();
    let mut response: Value =
        serde_json::from_str(&server.handle_line(&reset_request("Elves", "gH", 3))).unwrap();
    let (mut raw, mut lockstep) = pair("Elves", 3, 0x10D3_0000);
    let mut rng = SplitMix64::seed(9);
    let mut checked = 0;
    let mut steps = 0;
    while response["response_type"] == "decision" {
        let extensions = response["extensions"].as_object().unwrap();
        let phase = extensions["x_kernel_v2_support"]["london"]["phase"]
            .as_str()
            .unwrap()
            .to_string();
        if phase == "done" || steps > 80 {
            break;
        }
        // The other seat's hand objects, from the reference session.
        let actor = if response["acting_seat"] == "p0" {
            0
        } else {
            1
        };
        let other = 1 - actor;
        let text = response.to_string();
        for card in &raw.game_state().players[other].hand {
            let needle = format!("obj-{:06}-z", card.0);
            assert!(
                !text.contains(&needle),
                "the decision for p{actor} names a card of p{other}'s hand ({needle})"
            );
        }
        // The other seat's hand is a count in the observation.
        let observation: Value = serde_json::from_str(
            extensions["x_kernel_v5"]["observation_json"]
                .as_str()
                .unwrap(),
        )
        .unwrap();
        assert_eq!(
            observation["own_hand"].as_array().unwrap().len(),
            raw.game_state().players[actor].hand.len()
        );
        checked += 1;
        let rows: Vec<usize> = extensions["x_kernel_flat_v7"]["row_candidate_ids"]
            .as_array()
            .unwrap()
            .iter()
            .map(|id| id.as_u64().unwrap() as usize)
            .collect();
        let pick = rows[(rng.next_u64() % rows.len() as u64) as usize];
        let candidates = response["candidates"].as_array().unwrap().clone();
        let step = request(
            "step",
            &format!("s{steps}"),
            json!({"game_id": "gH", "expected_step": response["step"],
                   "selection": {"candidate_id": candidates[pick]["candidate_id"],
                                 "semantic_echo": candidates[pick]["semantic"]}}),
        );
        // The reference session takes the same answer.
        let RlSessionResponseV1::Decision(decision) = raw.current_response() else {
            panic!("reference ended");
        };
        let ref_rows = lockstep.row_candidate_ids(&decision).unwrap();
        let ref_pick = ref_rows[ref_rows.iter().position(|id| *id as usize == pick).unwrap()];
        let chosen = &decision.legal_actions[ref_pick as usize];
        raw.step(
            decision.episode_id,
            decision.step,
            ref_pick,
            &chosen.stable_id,
        )
        .unwrap();
        lockstep
            .mirror_step(&ref_rows, decision.step, ref_pick)
            .unwrap();
        response = serde_json::from_str(&server.handle_line(&step)).unwrap();
        steps += 1;
    }
    assert!(checked >= 2, "only {checked} pregame decisions checked");
}

#[test]
fn london_human_numeric_choice_rows_take_the_v4_fill_and_say_so() {
    use crate::agent_bridge_flat_v4_encode::{
        encode_flat_v7_tensor_v1, flat_v7_extension_value_v1, tensor_bits_common_v1,
        FlatV4EncodeScratchV1,
    };
    // Nyxborn Hydra's X (Elves) is a numeric choice in the bridge's neutral
    // mode; the FlatV7 contract refuses that row kind, so the bridge sends the
    // V4 fill (what the rated V4 bridge sends for it) and flags it.
    let mut v4 = FrozenPlayPolicyV1::training_fixture_v4();
    let mut scratch = FlatV4EncodeScratchV1::default();
    let mut found = 0;
    'seeds: for seed in 1..16_u64 {
        let (mut raw, mut lockstep) = pair("Elves", seed, 0x10D4_0000 + seed);
        let mut rng = SplitMix64::seed(seed);
        for _ in 0..3_000 {
            let RlSessionResponseV1::Decision(decision) = raw.current_response() else {
                continue 'seeds;
            };
            let rows = lockstep.row_candidate_ids(&decision).unwrap();
            let numeric = decision
                .legal_actions
                .iter()
                .any(|a| matches!(a.semantic, ActionSemanticV1::ChooseEffectNumber { .. }));
            if numeric {
                let encoded =
                    encode_flat_v7_tensor_v1(lockstep.fast_session(), &mut scratch).unwrap();
                let value = flat_v7_extension_value_v1(&encoded, &rows).unwrap();
                assert_eq!(value["numeric_choice_v4_fill"], true);
                v4.score_fast_session_v1(lockstep.fast_session()).unwrap();
                let theirs = v4.last_scored_training_tensor_v4().unwrap();
                assert_eq!(
                    value["tensor"],
                    serde_json::to_value(tensor_bits_common_v1(&theirs.common)).unwrap()
                );
                found += 1;
                if found >= 2 {
                    return;
                }
            }
            let pick = rows[(rng.next_u64() % rows.len() as u64) as usize];
            let chosen = &decision.legal_actions[pick as usize];
            raw.step(decision.episode_id, decision.step, pick, &chosen.stable_id)
                .unwrap();
            lockstep.mirror_step(&rows, decision.step, pick).unwrap();
        }
    }
    panic!("no Elves game reached a numeric choice");
}
