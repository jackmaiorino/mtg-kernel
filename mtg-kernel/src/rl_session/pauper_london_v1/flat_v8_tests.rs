//! FlatV8 (`native_flat_tensorizer_v8.rs`) on real sessions: every FlatV8
//! tensor is the FlatV7 tensor with the include column appended, the include
//! bit is set exactly on the `include: true` rows of inclusion decisions, the
//! pairing relation has one row per pending block of the observation, and
//! the converted net (every new coordinate +0) scores every decision, London
//! pregame and blocker declarations with pending pairs included, exactly as
//! FlatV7. Also the Rust and Python blocker parity fixture.

use super::*;
use crate::native_flat_tensorizer_v2::NativeFlatDecisionTensorV2;
use crate::rl::ActionSemanticV1;
use crate::runtime_decks::runtime_deck_by_id;
use crate::sideboard_play_policy_v1::FrozenPlayPolicyV1;
use PauperMulliganModeV1::Learned;

const SEED: u64 = 0x5eed_0008;
const V7_WIDTH: usize = 195;
const V8_WIDTH: usize = 196;

fn session_v1(london: bool, starting: PlayerId, decks: [&str; 2]) -> FastActorSessionV1 {
    let card_ids = decks.map(|id| runtime_deck_by_id(id).unwrap().card_ids.to_vec());
    let labels = decks.map(str::to_owned);
    if london {
        FastActorSessionV1::reset_pauper_london_v1(
            7,
            SEED,
            4096,
            4096 * 128,
            labels,
            card_ids,
            starting,
            [Learned, Learned],
        )
        .unwrap()
    } else {
        FastActorSessionV1::reset_with_explicit_decks_and_limits_flat_action_v3_environment_v2_with_starting_player_v1(
            7, SEED, 4096, 4096 * 128, labels, card_ids, starting,
        )
        .unwrap()
    }
}

fn v8_policy() -> FrozenPlayPolicyV1 {
    FrozenPlayPolicyV1::training_fixture_v4()
        .into_flat_v8_v1()
        .unwrap()
}

fn v7_policy() -> FrozenPlayPolicyV1 {
    FrozenPlayPolicyV1::training_fixture_v4()
        .into_flat_v7_transfer_v1()
        .unwrap()
}

/// The pending blocks the observation shows: accumulated pairs, then the
/// current attacker's selected blockers at a blocker-inclusion decision.
fn observed_pending_blocks_v1(observation: &serde_json::Value) -> usize {
    let projection = &observation["projection"];
    let accumulated = projection["surface_context"]["private_blockers"]["accumulated"]
        .as_array()
        .map_or(0, Vec::len);
    let selection = &projection["policy_surface_context"]["private_combat_selection"];
    let selected = if selection["attacker"].is_null() {
        0
    } else {
        selection["selected"].as_array().map_or(0, Vec::len)
    };
    accumulated + selected
}

struct ScoredV1 {
    tensor: NativeFlatDecisionTensorV2,
    pairs: Vec<i64>,
    logits: Vec<u32>,
    value: u32,
}

fn score_v8(policy: &mut FrozenPlayPolicyV1, session: &FastActorSessionV1) -> ScoredV1 {
    let scores = policy.score_fast_session_v1(session).unwrap();
    ScoredV1 {
        tensor: policy
            .last_scored_training_tensor_v4()
            .unwrap()
            .common
            .clone(),
        pairs: policy.last_scored_pairing_v8().unwrap().to_vec(),
        logits: scores.logits.iter().map(|x| x.to_bits()).collect(),
        value: scores.value.to_bits(),
    }
}

/// Plays a session to its end, checking each decision: every attacker is
/// included, and every other decision, blocks among them, is the sampled
/// choice of a third (FlatV8) policy, so blocks spread over attackers and
/// accumulate. Returns (decisions, include rows, decisions with a pairing
/// relation).
fn check_game_v1(
    mut session: FastActorSessionV1,
    cases: &mut Vec<serde_json::Value>,
) -> (usize, usize, usize) {
    let mut v7 = v7_policy();
    let mut v8 = v8_policy();
    let mut chooser = v8_policy();
    chooser.reset_sampling_v1([SEED, SEED + 1]);
    let (mut decisions, mut include_rows, mut pairing_decisions) = (0, 0, 0);
    for _ in 0..4000 {
        let FastActorResponseV1::Decision(d) = session.current_response() else {
            break;
        };
        let old = v7.score_fast_session_v1(&session).unwrap();
        let old_tensor = v7.last_scored_training_tensor_v4().unwrap().common.clone();
        let new = score_v8(&mut v8, &session);
        // Function preservation: the converted net scores FlatV8 as FlatV7.
        assert_eq!(
            old.logits.iter().map(|x| x.to_bits()).collect::<Vec<_>>(),
            new.logits
        );
        assert_eq!(old.value.to_bits(), new.value);
        // Every tensor but the action rows is V7's; each row is V7's plus
        // the include bit.
        let semantics = session.diagnostic_current_action_semantics().unwrap();
        let mut stripped = new.tensor.clone();
        stripped.action_features = new
            .tensor
            .action_features
            .chunks_exact(V8_WIDTH)
            .flat_map(|row| row[..V7_WIDTH].iter().copied())
            .collect();
        assert_eq!(stripped, old_tensor);
        for (row, semantic) in new
            .tensor
            .action_features
            .chunks_exact(V8_WIDTH)
            .zip(&semantics)
        {
            let include = matches!(
                semantic,
                ActionSemanticV1::ChooseAttackerInclusion { include: true, .. }
                    | ActionSemanticV1::ChooseBlockerInclusion { include: true, .. }
            );
            assert_eq!(row[V7_WIDTH], f32::from(u8::from(include)));
            include_rows += usize::from(include);
        }
        // One pairing row per pending block, of two distinct object nodes.
        let (observation, _) = session.diagnostic_current_decision_input_v4(d).unwrap();
        let observation_json = serde_json::to_value(&observation).unwrap();
        assert_eq!(
            new.pairs.len() / 2,
            observed_pending_blocks_v1(&observation_json)
        );
        let objects = new.tensor.object_card_ids.len() as i64;
        for pair in new.pairs.chunks_exact(2) {
            assert!(pair[0] != pair[1] && pair.iter().all(|&node| (0..objects).contains(&node)));
        }
        if !new.pairs.is_empty() {
            pairing_decisions += 1;
            // Up to two fixture cases with one pending block and three with
            // more (gang blocks or blocks for earlier attackers).
            let singles = cases
                .iter()
                .filter(|case| {
                    case["tensor"]["pairing_node_indices"]
                        .as_array()
                        .unwrap()
                        .len()
                        == 2
                })
                .count();
            let multis = cases.len() - singles;
            // Known hand cards are left out: the FlatV4 Python reference
            // encodes them in observation order and the Rust tensorizer in
            // canonical order (`canonical_known_hand_cards`), a FlatV4
            // difference outside FlatV8.
            let no_known_hands =
                observation_json["known_hand_cards"]
                    .as_array()
                    .is_some_and(|seats| {
                        seats
                            .iter()
                            .all(|cards| cards.as_array().is_some_and(Vec::is_empty))
                    });
            if no_known_hands
                && ((new.pairs.len() > 2 && multis < 3) || (new.pairs.len() == 2 && singles < 2))
            {
                cases.push(parity_case_v1(&session, d, &new));
            }
        }
        decisions += 1;
        let include = semantics.iter().position(|semantic| {
            matches!(
                semantic,
                ActionSemanticV1::ChooseAttackerInclusion { include: true, .. }
            )
        });
        let selected = match include {
            Some(index) => index as u32,
            None => chooser.select_with_training_tensor_v4(&session).unwrap().0,
        };
        session.step(d.episode_id, d.step, selected).unwrap();
    }
    (decisions, include_rows, pairing_decisions)
}

/// The actor-visible observation, the menu and the Rust FlatV8 tensors of
/// the current decision, for the Python parity test.
fn parity_case_v1(
    session: &FastActorSessionV1,
    d: FastActorDecisionV1,
    scored: &ScoredV1,
) -> serde_json::Value {
    let (mut observation, semantics) = session.diagnostic_current_decision_input_v4(d).unwrap();
    observation.visible_projection_hash = 0;
    let actions: Vec<_> = semantics
        .iter()
        .enumerate()
        .map(|(index, semantic)| {
            serde_json::json!({
                "schema_version": 5,
                "selected_index": index,
                "stable_id": format!("legal-action-v5:v8-fixture-{index}"),
                "display_text": null,
                "semantic": semantic,
            })
        })
        .collect();
    let t = &scored.tensor;
    serde_json::json!({
        "observation": observation,
        "legal_actions": actions,
        "tensor": {
            "state": t.state,
            "object_features": t.object_features,
            "object_card_ids": t.object_card_ids,
            "object_groups": t.object_groups,
            "object_node_ids": t.object_node_ids,
            "edge_features": t.edge_features,
            "edge_source_indices": t.edge_source_indices,
            "edge_target_indices": t.edge_target_indices,
            "action_features": t.action_features,
            "action_ref_features": t.action_ref_features,
            "action_ref_card_ids": t.action_ref_card_ids,
            "action_ref_action_indices": t.action_ref_action_indices,
            "action_ref_node_indices": t.action_ref_node_indices,
            "pairing_node_indices": scored.pairs,
        },
    })
}

/// FlatV8 on real games, with and without London: tensors, include bits,
/// pairing rows and function preservation at every decision, with blocker
/// declarations that have pending pairs among them. The blocker parity
/// fixture shared with `python/tests/test_features_v11.py` holds the first
/// pairing decisions; `MTG_KERNEL_WRITE_FLAT_V8_FIXTURE=1` rewrites it,
/// otherwise it must match byte for byte.
#[test]
fn flat_v8_tensors_include_bits_and_pairs_match_the_sessions() {
    let mut cases = Vec::new();
    let mut totals = (0, 0, 0);
    for (london, starting, decks) in [
        (false, PlayerId::P0, ["Elves", "Affinity"]),
        (false, PlayerId::P1, ["Elves", "Elves"]),
        (false, PlayerId::P0, ["CawGates", "Rally"]),
        (false, PlayerId::P1, ["Rally", "Elves"]),
        (false, PlayerId::P0, ["Affinity", "Faeries"]),
        (false, PlayerId::P1, ["Elves", "CawGates"]),
        (false, PlayerId::P0, ["Rally", "Rally"]),
        (false, PlayerId::P1, ["Affinity", "Elves"]),
        (true, PlayerId::P0, ["Burn", "Elves"]),
    ] {
        let (decisions, include_rows, pairing) =
            check_game_v1(session_v1(london, starting, decks), &mut cases);
        totals = (
            totals.0 + decisions,
            totals.1 + include_rows,
            totals.2 + pairing,
        );
    }
    assert!(totals.0 > 100, "{totals:?}");
    assert!(totals.1 > 0, "{totals:?}");
    assert!(
        totals.2 > 0,
        "no blocker decision with a pending block: {totals:?}"
    );
    let fixture = serde_json::to_vec_pretty(&serde_json::json!({
        "schema": "flat-policy-v8-blocker-parity-fixture-1",
        "feature_contract_digest": crate::native_flat_tensorizer_v8::FEATURE_CONTRACT_DIGEST_V8,
        "cases": cases,
    }))
    .unwrap();
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../data/flat_policy_v8/blocker_parity_fixture_v1.json");
    if std::env::var("MTG_KERNEL_WRITE_FLAT_V8_FIXTURE").is_ok_and(|value| value.trim() == "1") {
        std::fs::write(&path, &fixture).unwrap();
    }
    assert_eq!(std::fs::read(&path).unwrap(), fixture);
}
