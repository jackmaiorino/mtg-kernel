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
        "Terminal match win or loss is the",
        "only reward and the only promotion measure",
        "only production source of a target configuration",
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
