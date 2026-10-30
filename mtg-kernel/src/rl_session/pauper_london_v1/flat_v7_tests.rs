//! FlatV7 encoding of London decisions (design section 5): admission only
//! through a London session and the V7 identity, the pregame columns and
//! rows, function preservation in-game, and the Rust and Python pregame
//! parity fixture.

use super::*;
use crate::flat_policy_v2::{FlatScoringDecisionViewV2, FlatScoringOwnedBuffersV2};
use crate::flat_policy_v4::{FlatDecisionEncoderV4, FlatDecisionV4, FlatScoringDecisionViewV4};
use crate::native_flat_tensorizer_v2::NativeFlatDecisionTensorV2;
use crate::native_flat_tensorizer_v4::{NativeFlatDecisionTensorV4, NativeFlatTensorizerV4};
use crate::native_flat_tensorizer_v7::NativeFlatTensorizerV7;
use crate::rl::ActionSemanticV1;
use crate::runtime_decks::runtime_deck_by_id;
use crate::sideboard_play_policy_v1::FrozenPlayPolicyV1;
use PauperMulliganModeV1::Learned;

const SEED: u64 = 0x5eed_1a0d;
const KIND_COLUMN: usize = 15;
const SELF_HAS_LOST: usize = 30;
const OPPONENT_HAS_LOST: usize = 49;
const PREGAME_STAGE: usize = 89;
const EXPLICIT_STATE: usize = 219 - 96;

fn london(starting: PlayerId) -> FastActorSessionV1 {
    FastActorSessionV1::reset_pauper_london_v1(
        7,
        SEED,
        512,
        512 * 128,
        ["Burn".into(), "Elves".into()],
        [
            runtime_deck_by_id("Burn").unwrap().card_ids.to_vec(),
            runtime_deck_by_id("Elves").unwrap().card_ids.to_vec(),
        ],
        starting,
        [Learned, Learned],
    )
    .unwrap()
}

fn decision(session: &FastActorSessionV1) -> FastActorDecisionV1 {
    match session.current_response() {
        FastActorResponseV1::Decision(decision) => decision,
        FastActorResponseV1::Terminal(terminal) => panic!("unexpected terminal {terminal:?}"),
    }
}

fn announce(session: &mut FastActorSessionV1, mulligan: bool) {
    let index = session
        .diagnostic_current_action_semantics()
        .unwrap()
        .iter()
        .position(|semantic| {
            matches!(semantic, ActionSemanticV1::ChooseLondonMulligan { mulligan: m, .. } if *m == mulligan)
        })
        .unwrap() as u32;
    let d = decision(session);
    session.step(d.episode_id, d.step, index).unwrap();
}

fn v7_policy() -> FrozenPlayPolicyV1 {
    FrozenPlayPolicyV1::training_fixture_v4()
        .into_flat_v7_transfer_v1()
        .unwrap()
}

/// V7 tensors of the current decision.
fn v7_tensor(
    policy: &mut FrozenPlayPolicyV1,
    session: &FastActorSessionV1,
) -> NativeFlatDecisionTensorV2 {
    policy.score_fast_session_v1(session).unwrap();
    policy
        .last_scored_training_tensor_v4()
        .unwrap()
        .common
        .clone()
}

fn rows(tensor: &NativeFlatDecisionTensorV2) -> Vec<&[f32]> {
    tensor.action_features.chunks_exact(195).collect()
}

#[derive(Default)]
struct OwnedV4 {
    objects: Vec<crate::flat_policy_v2::FlatObjectCoreV2>,
    relations: Vec<crate::flat_policy_v2::FlatRelationV2>,
    object_subtypes: Vec<crate::flat_policy_v2::FlatObjectSubtypeV2>,
    ability_uses: Vec<crate::flat_policy_v2::FlatObjectAbilityUseV2>,
    goads: Vec<crate::flat_policy_v2::FlatObjectGoadV2>,
    completed_dungeons: Vec<crate::flat_policy_v2::FlatCompletedDungeonV2>,
    effect_subtype_changes: Vec<crate::flat_policy_v2::FlatEffectSubtypeChangeV2>,
    context_path_elements: Vec<crate::flat_policy_v2::FlatContextPathElementV2>,
    actions: Vec<crate::flat_policy_v2::FlatScorerActionCoreV2>,
    action_refs: Vec<crate::flat_policy_v2::FlatScorerActionRefV2>,
}

impl OwnedV4 {
    fn encode(&mut self, session: &FastActorSessionV1) -> FlatDecisionV4 {
        session
            .encode_current_flat_scoring_decision_owned_v4(
                decision(session),
                &mut FlatDecisionEncoderV4::default(),
                &mut FlatScoringOwnedBuffersV2 {
                    objects: &mut self.objects,
                    relations: &mut self.relations,
                    object_subtypes: &mut self.object_subtypes,
                    ability_uses: &mut self.ability_uses,
                    goads: &mut self.goads,
                    completed_dungeons: &mut self.completed_dungeons,
                    effect_subtype_changes: &mut self.effect_subtype_changes,
                    context_path_elements: &mut self.context_path_elements,
                    actions: &mut self.actions,
                    action_refs: &mut self.action_refs,
                },
            )
            .unwrap()
    }

    fn view<'a>(&'a self, decision: &'a FlatDecisionV4) -> FlatScoringDecisionViewV4<'a> {
        FlatScoringDecisionViewV4::new(
            FlatScoringDecisionViewV2::new(
                &decision.globals,
                &self.objects,
                &self.relations,
                &self.object_subtypes,
                &self.ability_uses,
                &self.goads,
                &self.completed_dungeons,
                &self.effect_subtype_changes,
                &self.context_path_elements,
                &self.actions,
                &self.action_refs,
            ),
            &decision.extensions,
        )
    }
}

#[test]
fn flat_v7_v4_and_v3_identities_refuse_london_decisions() {
    let session = london(PlayerId::P0);
    let mut v4 = FrozenPlayPolicyV1::training_fixture_v4();
    assert!(v4.score_fast_session_v1(&session).is_err());
    let mut v3 = FrozenPlayPolicyV1::training_fixture_v3();
    assert!(v3.score_fast_session_v1(&session).is_err());
    // The V4 tensorizer refuses the pregame decision the V7 one encodes.
    let mut owned = OwnedV4::default();
    let encoded = owned.encode(&session);
    assert!(encoded.extensions.london_pregame.is_some());
    let mut tensor = NativeFlatDecisionTensorV4::default();
    assert!(NativeFlatTensorizerV4::default()
        .fill(owned.view(&encoded), &mut tensor)
        .is_err());
    let mut tensor = NativeFlatDecisionTensorV2::default();
    NativeFlatTensorizerV7::default()
        .fill(owned.view(&encoded), &mut tensor)
        .unwrap();
    // The transfer is explicit and one-way.
    assert!(v7_policy().into_flat_v7_transfer_v1().is_err());
    assert!(FrozenPlayPolicyV1::training_fixture_v3()
        .into_flat_v7_transfer_v1()
        .is_err());
    assert_eq!(
        v7_policy().feature_identity_v1().feature_contract_digest,
        crate::native_flat_tensorizer_v7::FEATURE_CONTRACT_DIGEST_V7
    );
}

#[test]
fn flat_v7_announcement_and_bottom_rows_for_both_seats() {
    for starting in [PlayerId::P0, PlayerId::P1] {
        let mut session = london(starting);
        let mut policy = v7_policy();
        // First announcer: nobody has announced yet.
        let first = v7_tensor(&mut policy, &session);
        assert_eq!(first.state[PREGAME_STAGE], 1.0);
        assert_eq!(
            (first.state[SELF_HAS_LOST], first.state[OPPONENT_HAS_LOST]),
            (0.0, 0.0)
        );
        let first_rows = rows(&first);
        assert_eq!(first_rows.len(), 2);
        for (row, mulligan) in first_rows.iter().zip([0.0, 1.0]) {
            assert_eq!(row[KIND_COLUMN], 1.0);
            assert_eq!(row[..27].iter().sum::<f32>(), 1.0);
            assert_eq!(row[69], mulligan);
        }
        assert!(first.action_ref_features.is_empty());
        // Only the actor's own hand is an object row; the opponent's hand
        // is a count.
        assert_eq!(first.object_card_ids.len(), 7);
        assert!(first
            .object_groups
            .iter()
            .all(|group| *group == first.object_groups[0]));
        announce(&mut session, true);
        // Second announcer sees the first announcement.
        let second = v7_tensor(&mut policy, &session);
        assert_eq!(
            (second.state[SELF_HAS_LOST], second.state[OPPONENT_HAS_LOST]),
            (1.0, 0.0)
        );
        assert_eq!(rows(&second)[0][53], 1.0 / 16.0);
        assert_eq!(rows(&second)[0][52], 0.0);
        announce(&mut session, true);
        // Bottom: the starting player bottoms one of seven, both took one.
        assert_eq!(seat_of(decision(&session).acting_player), starting);
        let bottom = v7_tensor(&mut policy, &session);
        let bottom_rows = rows(&bottom);
        assert_eq!(bottom_rows.len(), 7);
        for row in &bottom_rows {
            assert_eq!(row[KIND_COLUMN], 1.0);
            assert_eq!(row[49], 1.0 / 8.0);
            assert_eq!(row[63], 1.0 / 16.0);
            assert_eq!(row[69], 0.0);
        }
        assert_eq!(bottom.action_ref_card_ids.len(), 7);
        let mut nodes = bottom.action_ref_node_indices.clone();
        nodes.sort_unstable();
        nodes.dedup();
        assert_eq!(nodes.len(), 7);
        assert_eq!(
            bottom.action_ref_action_indices,
            (0..7).collect::<Vec<i64>>()
        );
        // In-game decisions after the opening carry no pregame column.
        let d = decision(&session);
        session.step(d.episode_id, d.step, 0).unwrap();
        let d = decision(&session);
        session.step(d.episode_id, d.step, 0).unwrap();
        while session.pauper_london_pending_v1() {
            announce(&mut session, false);
        }
        let in_game = v7_tensor(&mut policy, &session);
        assert_eq!(in_game.state[PREGAME_STAGE], 0.0);
        assert!(rows(&in_game).iter().all(|row| row[KIND_COLUMN] == 0.0));
    }
}

fn seat_of(seat: crate::rl::PlayerSeatV1) -> PlayerId {
    flat_player_id_v1(seat)
}

#[test]
fn flat_v7_counterfactual_first_announcement_changes_explicit_columns() {
    for starting in [PlayerId::P0, PlayerId::P1] {
        let mut policy = v7_policy();
        let mut kept = london(starting);
        announce(&mut kept, false);
        let mut mulliganed = london(starting);
        announce(&mut mulliganed, true);
        let kept = v7_tensor(&mut policy, &kept);
        let mulliganed = v7_tensor(&mut policy, &mulliganed);
        assert_eq!(
            (kept.state[SELF_HAS_LOST], kept.state[OPPONENT_HAS_LOST]),
            (0.0, 1.0)
        );
        assert_eq!(
            (
                mulliganed.state[SELF_HAS_LOST],
                mulliganed.state[OPPONENT_HAS_LOST]
            ),
            (1.0, 0.0)
        );
        // Explicit columns only: the state digest is not consulted.
        assert_ne!(
            kept.state[..EXPLICIT_STATE],
            mulliganed.state[..EXPLICIT_STATE]
        );
        assert_ne!(rows(&kept)[0][..99], rows(&mulliganed)[0][..99]);
    }
}

/// Function preservation (design section 5): every in-game decision of a
/// London game with mulligans gives V7 tensors and logits bit-identical to
/// V4, and so does a loss-bearing row built directly at the flat level.
#[test]
fn flat_v7_in_game_tensors_and_logits_equal_v4() {
    for starting in [PlayerId::P0, PlayerId::P1] {
        let mut session = london(starting);
        announce(&mut session, true);
        announce(&mut session, false);
        while session.pauper_london_pending_v1() {
            let d = decision(&session);
            session.step(d.episode_id, d.step, 0).unwrap();
        }
        let mut v4 = FrozenPlayPolicyV1::training_fixture_v4();
        let mut v7 = v7_policy();
        let mut compared = 0;
        for _ in 0..80 {
            let FastActorResponseV1::Decision(d) = session.current_response() else {
                break;
            };
            let old = v4.score_fast_session_v1(&session).unwrap();
            let old_tensor = v4.last_scored_training_tensor_v4().unwrap().common.clone();
            let new = v7.score_fast_session_v1(&session).unwrap();
            let new_tensor = v7.last_scored_training_tensor_v4().unwrap().common.clone();
            assert_eq!(old_tensor, new_tensor);
            assert_eq!(
                old.logits.iter().map(|x| x.to_bits()).collect::<Vec<_>>(),
                new.logits.iter().map(|x| x.to_bits()).collect::<Vec<_>>()
            );
            assert_eq!(old.value.to_bits(), new.value.to_bits());
            compared += 1;
            session
                .step(d.episode_id, d.step, d.legal_action_count / 2)
                .unwrap();
        }
        assert!(compared > 20);
        // A loss-bearing row: has_lost set directly in the flat globals.
        let FastActorResponseV1::Decision(_) = session.current_response() else {
            continue;
        };
        let mut owned = OwnedV4::default();
        let mut encoded = owned.encode(&session);
        encoded.globals.players[1].has_lost = true;
        let mut old = NativeFlatDecisionTensorV4::default();
        NativeFlatTensorizerV4::default()
            .fill(owned.view(&encoded), &mut old)
            .unwrap();
        let mut new = NativeFlatDecisionTensorV2::default();
        NativeFlatTensorizerV7::default()
            .fill(owned.view(&encoded), &mut new)
            .unwrap();
        assert_eq!(old.common, new);
        assert_eq!(new.state[OPPONENT_HAS_LOST], 1.0);
        assert_eq!(new.state[PREGAME_STAGE], 0.0);
        // V7 refuses an in-game numeric choice instead of aliasing column 15.
        owned.actions[0].kind = crate::flat_policy_v2::FlatScorerActionKindV2::ChooseEffectNumber;
        assert!(NativeFlatTensorizerV7::default()
            .fill(owned.view(&encoded), &mut new)
            .is_err());
    }
}

/// The pregame parity fixture shared with `python/tests/test_features_v10.py`:
/// the actor-visible observation and menu of each pregame phase for both
/// seats, with the Rust V7 tensors. `MTG_KERNEL_WRITE_FLAT_V7_FIXTURE=1`
/// rewrites it; otherwise it must match byte for byte.
#[test]
fn flat_v7_pregame_parity_fixture_matches_rust_tensors() {
    let mut cases = Vec::new();
    for starting in [PlayerId::P0, PlayerId::P1] {
        let mut session = london(starting);
        let mut policy = v7_policy();
        // First announcement, second announcement, first bottom, second
        // seat's bottom after one mulligan each.
        for step in 0..4 {
            let d = decision(&session);
            let (mut observation, semantics) =
                session.diagnostic_current_decision_input_v4(d).unwrap();
            observation.visible_projection_hash = 0;
            let mut observation = serde_json::to_value(&observation).unwrap();
            let view = london_pregame_view_v1(&session.state).unwrap().unwrap();
            observation["extensions"]["london_pregame"] = serde_json::json!({
                "phase": if view.phase == LondonPregamePhaseV1::Bottom { "bottom" } else { "announce" },
                "mulligans_taken": view.counts,
                "kept": view.kept,
                "announced_mulligan": view.mulligan_this_round,
            });
            let actions: Vec<_> = semantics
                .iter()
                .enumerate()
                .map(|(index, semantic)| {
                    serde_json::json!({
                        "schema_version": 5,
                        "selected_index": index,
                        "stable_id": format!("legal-action-v5:london-fixture-{index}"),
                        "display_text": null,
                        "semantic": semantic,
                    })
                })
                .collect();
            let tensor = v7_tensor(&mut policy, &session);
            cases.push(serde_json::json!({
                "starting_player": starting.0,
                "step": step,
                "observation": observation,
                "legal_actions": actions,
                "tensor": {
                    "state": tensor.state,
                    "object_features": tensor.object_features,
                    "object_card_ids": tensor.object_card_ids,
                    "object_groups": tensor.object_groups,
                    "object_node_ids": tensor.object_node_ids,
                    "edge_features": tensor.edge_features,
                    "edge_source_indices": tensor.edge_source_indices,
                    "edge_target_indices": tensor.edge_target_indices,
                    "action_features": tensor.action_features,
                    "action_ref_features": tensor.action_ref_features,
                    "action_ref_card_ids": tensor.action_ref_card_ids,
                    "action_ref_action_indices": tensor.action_ref_action_indices,
                    "action_ref_node_indices": tensor.action_ref_node_indices,
                },
            }));
            if step < 2 {
                announce(&mut session, true);
            } else {
                let d = decision(&session);
                session.step(d.episode_id, d.step, 0).unwrap();
            }
        }
    }
    let fixture = serde_json::to_vec_pretty(&serde_json::json!({
        "schema": "flat-policy-v7-pregame-parity-fixture-1",
        "feature_contract_digest": crate::native_flat_tensorizer_v7::FEATURE_CONTRACT_DIGEST_V7,
        "cases": cases,
    }))
    .unwrap();
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../data/flat_policy_v7/pregame_parity_fixture_v1.json");
    if std::env::var("MTG_KERNEL_WRITE_FLAT_V7_FIXTURE").is_ok_and(|value| value.trim() == "1") {
        std::fs::write(&path, &fixture).unwrap();
    }
    assert_eq!(std::fs::read(&path).unwrap(), fixture);
}
