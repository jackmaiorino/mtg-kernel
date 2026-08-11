const CONTRACT: &str = include_str!("../COMPETITIVE_PREGAME_MODEL_INTERFACE_2026-08-11.md");

#[test]
fn competitive_pregame_model_interface_requires_native_checkpoint_provenance() {
    for required in [
        "exact deck and current post-sideboard configuration commitments",
        "seven ordered private opening-hand card identities",
        "play or draw status, game number, and public games-won score",
        "Keep, Mulligan with the",
        "SelectForBottom with a stable hand object, or Submit",
        "finite logit per ordered action plus a finite value",
        "concrete checkpoint-bound pregame scorer",
        "terminal win or loss as the only reward and promotion measure",
        "heuristic path cannot satisfy the model-backed readiness flag",
        "legacy non-model executor is now crate-private",
        "non-forgeable opaque",
        "native selection that retains the exact checkpoint",
        "performs no live MTGO operation",
    ] {
        assert!(
            CONTRACT.contains(required),
            "competitive pregame model interface is missing: {required}"
        );
    }
}
