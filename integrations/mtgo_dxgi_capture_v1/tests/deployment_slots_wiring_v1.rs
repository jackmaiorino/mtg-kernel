#![cfg(target_os = "windows")]
//! The wiring test: one placeholder deployment drives every decision surface
//! through the adapter's existing checked-untrusted entry points, and every
//! result stays non-authorizing.

use mtgo_dxgi_capture_v1::{
    build_placeholder_deployment_slots_v1, score_checked_untrusted_competitive_native_pregame_v1,
    score_checked_untrusted_competitive_native_sideboard_v1, MtgoCompetitiveNativePregameActionV1,
    MtgoCompetitiveNativePregameCardV1, MtgoCompetitiveNativePregameModelInputV1,
    MtgoCompetitiveNativeSideboardCardCountV1, MtgoCompetitiveNativeSideboardConfigurationV1,
    MtgoCompetitiveNativeSideboardModelInputV1, MtgoCompetitivePregameStageV1,
    MtgoSearchRootDecisionV1, MtgoSearchRootProviderV1, MtgoUnknownCardPolicyV1,
    MTGO_SEARCH_ROOT_NOT_RATIFIED_REASON_V1, MTGO_VISIBLE_KERNEL_SCORER_NOT_QUALIFIED_REASON_V1,
};
use mtgo_blackbox_v1::{
    MtgoCompetitivePregamePlayDrawV1, MtgoPlayerVisibleDuelDecisionInputV1,
    MtgoPlayerVisibleDuelScorerV1,
};

fn deck_v1() -> MtgoCompetitiveNativeSideboardConfigurationV1 {
    MtgoCompetitiveNativeSideboardConfigurationV1 {
        mainboard: vec![
            MtgoCompetitiveNativeSideboardCardCountV1 { visible_card_name: "Lightning Bolt".to_owned(), count: 4 },
            MtgoCompetitiveNativeSideboardCardCountV1 { visible_card_name: "Mountain".to_owned(), count: 56 },
        ],
        sideboard: vec![MtgoCompetitiveNativeSideboardCardCountV1 { visible_card_name: "Lightning Bolt".to_owned(), count: 15 }],
    }
}

fn sample_duel_input() -> MtgoPlayerVisibleDuelDecisionInputV1 {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../mtgo_blackbox_v1/fixtures/player_visible_flat_v2_conformance_source_v1.json"
    ))
    .unwrap();
    serde_json::from_value(fixture["model_input"].clone()).unwrap()
}

#[test]
fn placeholder_deployment_drives_every_surface_without_authority() {
    let mut slots = build_placeholder_deployment_slots_v1(&deck_v1()).unwrap();
    let deployment = "f".repeat(64);

    let pregame = MtgoCompetitiveNativePregameModelInputV1 {
        game_number: 1,
        play_draw: MtgoCompetitivePregamePlayDrawV1::OnPlay,
        acting_player_games_won: 0,
        opponent_games_won: 0,
        player_known_deck_configuration: deck_v1(),
        stage: MtgoCompetitivePregameStageV1::MulliganChoice { prospective_keep_size: 7 },
        prospective_keep_size: Some(7),
        required_bottom_count: 0,
        selected_bottom_count: 0,
        ordered_visible_cards: ["Mountain", "Mountain", "Mountain", "Lightning Bolt", "Lightning Bolt", "Lightning Bolt", "Lightning Bolt"]
            .iter()
            .enumerate()
            .map(|(slot, name)| MtgoCompetitiveNativePregameCardV1 { card_slot: slot as u8, visible_card_name: (*name).to_owned(), selected_for_bottom: false })
            .collect(),
        ordered_confirmed_bottom_slots: Vec::new(),
        ordered_actions: vec![
            MtgoCompetitiveNativePregameActionV1::KeepOpeningHand,
            MtgoCompetitiveNativePregameActionV1::Mulligan { next_hand_size: 6 },
        ],
    };
    let pregame_selection = score_checked_untrusted_competitive_native_pregame_v1(&pregame, &deployment, &mut slots.pregame_controller).unwrap();
    assert_eq!(pregame_selection.selected_action_v1(), &MtgoCompetitiveNativePregameActionV1::KeepOpeningHand);
    assert!(!pregame_selection.safe_for_live_input_v1());

    let sideboard = MtgoCompetitiveNativeSideboardModelInputV1 { next_game_number: 2, acting_player_games_won: 1, opponent_games_won: 0, current_configuration: deck_v1() };
    let sideboard_selection = score_checked_untrusted_competitive_native_sideboard_v1(&sideboard, &deployment, &mut slots.sideboard_controller).unwrap();
    assert_eq!(sideboard_selection.selection_v1().target_configuration, deck_v1());
    assert!(!sideboard_selection.permits_sideboard_submission_v1());

    let duel = sample_duel_input();
    assert_eq!(slots.search_root_provider.search_root_v1(&duel), MtgoSearchRootDecisionV1::RawPolicyOnly { reason: MTGO_SEARCH_ROOT_NOT_RATIFIED_REASON_V1.to_owned() });
    assert_eq!(slots.duel_scorer.score_player_visible_duel_v1(&duel).unwrap_err(), MTGO_VISIBLE_KERNEL_SCORER_NOT_QUALIFIED_REASON_V1);
    assert_eq!(slots.unknown_card_policy, MtgoUnknownCardPolicyV1::FailClosedHumanTakeover);

    let report = slots.slot_report_v1();
    assert!(report.all_slots_wired);
    assert_eq!(report.placeholder_slot_count, 5);
    assert!(!report.grants_live_authority);
}
