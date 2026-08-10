use mtgo_blackbox_v1::MtgoOfflineLondonBottomingTraceV1;
use std::collections::HashSet;

const TRACE: &str = include_str!("../fixtures/offline_london_bottom_six_trace_20260810_v1.json");

#[test]
fn live_bottom_six_trace_preserves_objects_through_every_reflow() {
    let trace: MtgoOfflineLondonBottomingTraceV1 = serde_json::from_str(TRACE).unwrap();
    assert_eq!(trace.schema_version, 1);
    assert_eq!(trace.required_bottom_count, 6);
    assert_eq!(trace.original_hand.len(), 7);
    assert_eq!(trace.selected_for_bottom_click_order.len(), 6);
    assert_eq!(trace.states.len(), 7);

    let original_ids: Vec<_> = trace
        .original_hand
        .iter()
        .map(|card| card.adapter_object_id.as_str())
        .collect();
    assert_eq!(
        original_ids.iter().copied().collect::<HashSet<_>>().len(),
        7
    );
    assert_eq!(
        trace
            .selected_for_bottom_click_order
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        original_ids[..6]
    );

    for (index, state) in trace.states.iter().enumerate() {
        assert_eq!(usize::from(state.selected_count), index);
        assert_eq!(
            state
                .visible_remaining_object_ids
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            original_ids[index..]
        );
        assert_eq!(state.done_visible, index == 6);
        assert!(state.cancel_visible);
        assert!(state.prompt_reconciled);
        assert!(state.legal_action_set_complete);
    }
}

#[test]
fn live_trace_sources_are_unique_and_completion_is_solitaire_specific() {
    let trace: MtgoOfflineLondonBottomingTraceV1 = serde_json::from_str(TRACE).unwrap();
    let manifests: HashSet<_> = trace
        .states
        .iter()
        .map(|state| state.source_manifest_sha256.as_str())
        .chain(std::iter::once(
            trace.completion.source_manifest_sha256.as_str(),
        ))
        .collect();
    let frames: HashSet<_> = trace
        .states
        .iter()
        .map(|state| state.source_frame_sha256.as_str())
        .chain(std::iter::once(
            trace.completion.source_frame_sha256.as_str(),
        ))
        .collect();
    assert_eq!(manifests.len(), 8);
    assert_eq!(frames.len(), 8);
    assert_eq!(trace.completion.bottomed_card_count, 6);
    assert_eq!(trace.completion.began_game_hand_size, 1);
    assert!(trace.completion.solitaire_draw_observed);
    assert_eq!(trace.completion.resulting_visible_hand_size, 2);
    assert!(trace.completion.first_main_visible);
    assert!(trace.completion.visible_game_log_reconciled);
}
