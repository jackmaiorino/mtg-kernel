const CONTRACT: &str = include_str!("../COMPETITIVE_SIDEBOARD_MODEL_INTERFACE_2026-08-11.md");

#[test]
fn changed_sideboard_model_interface_requires_native_match_policy_provenance() {
    for required in [
        "best-of-three match session",
        "acting player's play-or-draw status",
        "`MoveOneToMainboard`",
        "`MoveOneToSideboard`",
        "`SubmitConfiguration`",
        "at most 64 model decisions",
        "kernel now has deterministic best-of-three match state and session",
        "simulator plumbing",
        "`opponent_deck_id`",
        "Terminal match win or loss is the",
        "only reward and the only promotion measure",
        "native checkpoint scorer must be the only production source of a target",
        "completed-game player-visible history",
        "loaded-checkpoint path remains required for production",
        "opponent's hidden deck list",
        "arbitrary externally",
        "coordinator's production",
        "performs no live MTGO operation",
    ] {
        assert!(
            CONTRACT.contains(required),
            "competitive sideboard model interface is missing: {required}"
        );
    }
}
