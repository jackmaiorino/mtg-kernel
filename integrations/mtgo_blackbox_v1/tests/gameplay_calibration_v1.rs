use mtg_kernel::rl::{ActionSemanticV1, PlayerSeatV1};
use mtgo_blackbox_v1::{
    validate_gameplay_calibration_trace_v1, MtgoGameplayCalibrationActionV1,
    MtgoGameplayCalibrationTraceV1, MtgoGameplayVisibleChangeV1,
};

fn fixture() -> MtgoGameplayCalibrationTraceV1 {
    serde_json::from_str(include_str!(
        "../fixtures/solitaire_pass_to_combat_transition_v1.json"
    ))
    .expect("checked-in gameplay trace must parse")
}

#[test]
fn live_solitaire_pass_transition_is_structurally_checked_only() {
    let checked = validate_gameplay_calibration_trace_v1(fixture()).unwrap();
    assert_eq!(
        checked.action(),
        &ActionSemanticV1::Pass {
            actor: PlayerSeatV1::P0
        }
    );
    assert_eq!(checked.transition_commitment_sha256().len(), 64);
}

#[test]
fn unsupported_action_actor_or_missing_phase_change_fails_closed() {
    let mut actor = fixture();
    actor.action = MtgoGameplayCalibrationActionV1::Pass {
        actor: PlayerSeatV1::P1,
    };
    assert_eq!(
        validate_gameplay_calibration_trace_v1(actor)
            .err()
            .expect("non-local pass must fail")
            .code(),
        "gameplay_trace_action_unsupported"
    );

    let mut missing = fixture();
    missing
        .visible_postconditions
        .retain(|item| item.change != MtgoGameplayVisibleChangeV1::PhaseBarChanged);
    assert_eq!(
        validate_gameplay_calibration_trace_v1(missing)
            .err()
            .expect("missing phase change must fail")
            .code(),
        "gameplay_trace_postcondition_count_invalid"
    );
}

#[test]
fn gameplay_action_json_rejects_coordinates_and_unknown_fields() {
    let source = include_str!("../fixtures/solitaire_pass_to_combat_transition_v1.json");
    let altered = source.replacen(
        "\"actor\": \"p0\"",
        "\"actor\": \"p0\", \"x\": 29, \"y\": 155",
        1,
    );
    assert!(serde_json::from_str::<MtgoGameplayCalibrationTraceV1>(&altered).is_err());
}

#[test]
fn unchanged_prompt_and_input_authority_claim_fail_closed() {
    let mut unchanged = fixture();
    unchanged.visible_postconditions[0].after_bgra8_sha256 = unchanged.visible_postconditions[0]
        .before_bgra8_sha256
        .clone();
    assert_eq!(
        validate_gameplay_calibration_trace_v1(unchanged)
            .err()
            .expect("unchanged prompt must fail")
            .code(),
        "gameplay_trace_postcondition_unchanged"
    );

    let mut authority = fixture();
    authority.before_frame.safe_for_policy_scoring = true;
    assert_eq!(
        validate_gameplay_calibration_trace_v1(authority)
            .err()
            .expect("preview authority claim must fail")
            .code(),
        "gameplay_trace_preview_claims_authority"
    );
}
