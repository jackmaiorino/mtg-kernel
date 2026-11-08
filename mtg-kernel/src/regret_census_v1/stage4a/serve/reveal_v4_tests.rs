//! Native V4 export, sampling, and the production worker request dispatcher.
use super::*;
use crate::ids::PlayerId;
use crate::infoset_v1::fixtures::{force_deal, new_game, restart};
use crate::infoset_v1::{export_reveal_v4, ExportInputV1};

fn repeated_reveal(
    viewer: PlayerId,
    distinct: bool,
    fewer_islands: bool,
) -> (FastActorSessionV1, PayloadV1) {
    let decks = ["Faeries", "Faeries"];
    let template = new_game(decks, 543, viewer);
    let mut state = template.game_state().clone();
    let owner = viewer.opponent();
    force_deal(
        &mut state,
        owner,
        &[
            "Island",
            "Island",
            "Island",
            "Spellstutter Sprite",
            if fewer_islands {
                "Spellstutter Sprite"
            } else {
                "Island"
            },
            "Force Spike",
            "Dispel",
        ],
        &[],
    );
    force_deal(&mut state, viewer, &["Island", "Faerie Seer"], &[]);
    state.reveal_meaning_v3.enable();
    let cards = state.players[owner.index()].hand.clone();
    assert_eq!(
        cards
            .iter()
            .filter(|&&id| state.objects.get(id).card_def
                == crate::card_def::card_id_by_name("Island").unwrap())
            .count(),
        if fewer_islands { 3 } else { 4 }
    );
    state.reveal_hand_card(viewer, owner, cards[0]).unwrap();
    state
        .reveal_hand_card(viewer, owner, cards[usize::from(distinct)])
        .unwrap();
    let live = restart(&template, state);
    let state = live.game_state();
    let deck = RUNTIME_DECKS.iter().find(|d| d.id == "Faeries").unwrap();
    let exported = export_reveal_v4(&ExportInputV1 {
        state,
        journal: state.observation_journal.notes(),
        viewer,
        decklists: [deck.card_ids, deck.card_ids],
        marks: None,
    })
    .unwrap();
    let payload = PayloadV1::new(&exported, &live, viewer, 0).unwrap();
    assert_eq!(payload.schema, INFOSET_SCHEMA_REVEAL_V4);
    let wire = serde_json::to_value(&payload).unwrap();
    assert_eq!(
        wire["reveal_hand"],
        json!([{"card_name":"Island", "minimum":1}])
    );
    (live, PayloadV1::from_bytes(&payload.to_bytes()).unwrap())
}

#[test]
fn reveal_v4_request_import_sample_and_worker_use_public_minima_both_seats() {
    let policy = FrozenPlayPolicyV1::training_fixture_v4();
    for viewer in [PlayerId::P0, PlayerId::P1] {
        let (_, payload) = repeated_reveal(viewer, false, false);
        let (_, other) = repeated_reveal(viewer, true, false);
        let (_, hidden_variant) = repeated_reveal(viewer, true, true);
        assert_eq!(payload.to_bytes(), other.to_bytes());
        assert_eq!(payload.to_bytes(), hidden_variant.to_bytes());
        require_profile(&payload, true).unwrap();
        assert!(require_profile(&payload, false).is_err());
        let imported = import_at_v1(&payload, &[]).unwrap();
        let ids = crate::infoset_v1::order::imported_ids_v1(&payload.handles);
        let keys = crate::infoset_v1::order::row_keys_v1(&imported, Some(&ids)).unwrap();
        let point: PointRequestV1 = serde_json::from_value(json!({
            "op":"point", "payload":payload, "path":[]
        }))
        .unwrap();
        let answer = point_v1(&point).unwrap();
        assert_eq!(answer["viewer_decision"], true);
        assert_eq!(answer["keys"], json!(keys));
        assert!(answer.get("flat").is_none());
        let prior = DeckPrior::new(&[opponent_deck_index(&payload).unwrap()]);
        for seed in [543, 544] {
            let sample =
                super::super::world::sample_recorded(&imported, seed, &prior, &mut None).unwrap();
            let names = &sample
                .world
                .game_state()
                .reveal_knowledge_v4
                .as_ref()
                .unwrap()[viewer.index()];
            assert_eq!(
                names.hand(viewer.opponent()).collect::<Vec<_>>(),
                vec![(crate::card_def::card_id_by_name("Island").unwrap(), 1)]
            );
            assert_eq!(
                menu_sha256(&sample.world).unwrap(),
                menu_sha256(&imported).unwrap()
            );
        }
        let request = json!({"root_id":format!("reveal-v4/{}", viewer.0),
            "tree":0, "units":8, "payload":payload, "path":[]})
        .to_string();
        let first = worker_answer_v4(&policy, true, &request);
        assert_eq!(first["ok"], true, "{first}");
        let result: TreeResultV1 = serde_json::from_value(first["result"].clone()).unwrap();
        assert!(
            result.transitions > 0,
            "worker did no actual transitions: {result:?}"
        );
        assert!(result.faults.is_empty(), "{result:?}");
        assert_eq!(result.menu_sha256, menu_sha256(&imported).unwrap());
        assert_eq!(result.edges.as_ref().unwrap().len(), keys.len());
        let replay = worker_answer_v4(&policy, true, &request);
        assert_eq!(first["result"]["digest"], replay["result"]["digest"]);
        let refused = worker_answer_v4(&policy, false, &request);
        assert_eq!(refused["ok"], false);
        assert_eq!(refused["root_id"], first["root_id"]);
        assert!(refused["error"].as_str().unwrap().contains("profile"));
        let invalid_path = json!({"root_id":"bad-path", "tree":0, "units":8,
            "payload":payload, "path":[u32::MAX]})
        .to_string();
        let refused = worker_answer_v4(&policy, true, &invalid_path);
        assert_eq!(refused["ok"], false);
        assert!(refused["error"]
            .as_str()
            .unwrap()
            .contains("outside the menu"));
    }
}

#[test]
fn reveal_v4_original_group_map_survives_actual_step_and_export_refusal() {
    use crate::agent_bridge_infoset_v1::{infoset_extension_v1, InfosetBridgeV1};
    use crate::rl_session::{RlEpisodeSessionV1, RlSessionResponseV1};

    for viewer in [PlayerId::P0, PlayerId::P1] {
        let (mut live, _) = repeated_reveal(viewer, false, false);
        let mut bridge = InfosetBridgeV1::reveal_v4();
        let RlSessionResponseV1::Decision(original) =
            RlEpisodeSessionV1::from_fast_session_for_test(&live).current_response()
        else {
            panic!("expected initial decision")
        };
        infoset_extension_v1(
            &live,
            &original,
            &mut bridge,
            &live.game_state().reveal_meaning_v3,
        )
        .unwrap();
        assert!(bridge.pending.as_ref().unwrap()["payload_json"].is_string());
        let (root_step, root_ids) = bridge.root_ids.clone().unwrap();
        // Isolate the bridge's group custody state machine with an explicit
        // group identity. Real group discovery/commit belongs to the process
        // harness; the state transition and exporter below are native.
        let support = json!({"choice":{"purpose":"look"}, "effect_instance":"qualification-group"});
        let proof = bridge
            .live_group_rows_v4(original.acting_player, &support, &live)
            .unwrap();
        assert_eq!(proof["root_step"], root_step);
        let selected = live
            .diagnostic_current_action_semantics()
            .unwrap()
            .iter()
            .position(|action| matches!(action, crate::rl::ActionSemanticV1::PlayLand { .. }))
            .unwrap() as u32;
        let current = decision(&live).unwrap();
        live.step(current.episode_id, current.step, selected)
            .unwrap();
        let RlSessionResponseV1::Decision(later) =
            RlEpisodeSessionV1::from_fast_session_for_test(&live).current_response()
        else {
            panic!("expected subsequent decision")
        };
        assert!(later.step > original.step);
        assert_eq!(later.acting_player, original.acting_player);
        // A serialized native snapshot has no private event-time journal.
        let restored: crate::state::GameState =
            serde_json::from_value(serde_json::to_value(live.game_state()).unwrap()).unwrap();
        infoset_extension_v1(&live, &later, &mut bridge, &restored.reveal_meaning_v3).unwrap();
        assert_eq!(bridge.pending.as_ref().unwrap()["refusal"], "R8");
        assert!(bridge.root_ids.is_none());
        let later_proof = bridge
            .live_group_rows_v4(later.acting_player, &support, &live)
            .unwrap();
        assert_eq!(later_proof["root_step"], root_step);
        assert_eq!(
            later_proof["row_keys"],
            json!(crate::infoset_v1::order::row_keys_v1(&live, Some(&root_ids)).unwrap())
        );
        let changed = json!({"choice":{"purpose":"look"}, "effect_instance":"different-group"});
        assert!(bridge
            .live_group_rows_v4(later.acting_player, &changed, &live)
            .is_none());
    }
}
