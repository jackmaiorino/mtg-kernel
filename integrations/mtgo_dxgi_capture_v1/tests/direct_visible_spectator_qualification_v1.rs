#![cfg(target_os = "windows")]

use serde::Deserialize;

const FIXTURE_JSON: &str =
    include_str!("../fixtures/direct_visible_spectator_qualification_20260814_v1.json");
const PRODUCER_SOURCE: &str =
    include_str!("../../mtgo_visible_duel_producer_v1/VisibleDuelProducerV1.cs");
const SANITIZER_SOURCE: &str =
    include_str!("../../mtgo_visible_duel_producer_v1/SanitizedVisibleDecisionV1.cs");
const RUNTIME_SOURCE: &str = include_str!("../src/probe/direct_visible_source_runtime.rs");
const CLI_SOURCE: &str =
    include_str!("../src/bin/qualify_mtgo_direct_visible_spectator_source_v1.rs");

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct QualificationFixtureV1 {
    schema_version: u32,
    qualification_id: String,
    surface: String,
    game_format: String,
    visible_title_shape: String,
    account_role: String,
    runtime_identity_commitment_sha256: String,
    broker_binary_sha256: String,
    producer_binary_sha256: String,
    before_capture_commitment_sha256: String,
    after_capture_commitment_sha256: String,
    sanitized_result_sha256: String,
    qualification_commitment_sha256: String,
    producer_result: ProducerResultV1,
    static_source_constraints: StaticSourceConstraintsV1,
    authority: AuthorityV1,
    nonclaims: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProducerResultV1 {
    status: String,
    reason: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct StaticSourceConstraintsV1 {
    visible_chrome_player_count_required: u8,
    visible_chrome_local_player_count_required: u8,
    semantic_builder_player_count_required: u8,
    first_failing_predicate_proven: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AuthorityV1 {
    acting_player_authority: bool,
    safe_for_live_semantic_evidence: bool,
    safe_for_model_scoring: bool,
    safe_for_input: bool,
    permits_event_entry: bool,
    permits_spending: bool,
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[test]
fn live_spectator_qualification_is_exact_non_authorizing_and_identity_free() {
    let fixture: QualificationFixtureV1 =
        serde_json::from_str(FIXTURE_JSON).expect("spectator qualification fixture");
    assert_eq!(fixture.schema_version, 1);
    assert_eq!(
        fixture.qualification_id,
        "direct_visible_spectator_qualification_20260814_v1"
    );
    assert_eq!(fixture.surface, "foreground_spectator_duel");
    assert_eq!(fixture.game_format, "Modern");
    assert_eq!(fixture.visible_title_shape, "two_visible_participants");
    assert_eq!(fixture.account_role, "spectator_only");
    for digest in [
        fixture.runtime_identity_commitment_sha256.as_str(),
        fixture.broker_binary_sha256.as_str(),
        fixture.producer_binary_sha256.as_str(),
        fixture.before_capture_commitment_sha256.as_str(),
        fixture.after_capture_commitment_sha256.as_str(),
        fixture.sanitized_result_sha256.as_str(),
        fixture.qualification_commitment_sha256.as_str(),
    ] {
        assert!(is_sha256(digest));
    }
    assert_ne!(
        fixture.before_capture_commitment_sha256,
        fixture.after_capture_commitment_sha256
    );
    assert_eq!(fixture.producer_result.status, "abstained");
    assert_eq!(fixture.producer_result.reason, "projection_incomplete");
    assert_eq!(
        fixture
            .static_source_constraints
            .visible_chrome_player_count_required,
        2
    );
    assert_eq!(
        fixture
            .static_source_constraints
            .visible_chrome_local_player_count_required,
        1
    );
    assert_eq!(
        fixture
            .static_source_constraints
            .semantic_builder_player_count_required,
        2
    );
    assert!(
        !fixture
            .static_source_constraints
            .first_failing_predicate_proven
    );
    assert!(!fixture.authority.acting_player_authority);
    assert!(!fixture.authority.safe_for_live_semantic_evidence);
    assert!(!fixture.authority.safe_for_model_scoring);
    assert!(!fixture.authority.safe_for_input);
    assert!(!fixture.authority.permits_event_entry);
    assert!(!fixture.authority.permits_spending);
    assert_eq!(fixture.nonclaims.len(), 3);

    for forbidden in [
        "UnbuckledPie",
        "Match #",
        "Game #",
        "process_id",
        "window_handle",
    ] {
        assert!(
            !FIXTURE_JSON.contains(forbidden),
            "fixture exposes forbidden identity or process field: {forbidden}"
        );
    }
}

#[test]
fn spectator_route_is_mode_separated_and_cannot_claim_acting_authority() {
    for required in [
        "CaptureWindowModeV2::SpectatorGame",
        "validate_same_unadmitted_spectator_observation_lineage_v1",
        "DIRECT_VISIBLE_SPECTATOR_SOURCE_QUALIFICATION_DOMAIN_V1",
        "spectator_visible_only_no_actor_knowledge_no_scoring_no_input",
    ] {
        assert!(
            RUNTIME_SOURCE.contains(required),
            "spectator runtime is missing: {required}"
        );
    }
    for required in [
        "qualification_role: \"spectator_only\"",
        "acting_player_authority: false",
        "safe_for_live_semantic_evidence: observation.safe_for_live_semantic_evidence_v1()",
        "safe_for_model_scoring: observation.safe_for_model_scoring_v1()",
        "safe_for_input: observation.safe_for_input_v1()",
        "permits_event_entry: observation.permits_event_entry_v1()",
        "permits_spending: observation.permits_spending_v1()",
    ] {
        assert!(
            CLI_SOURCE.contains(required),
            "spectator CLI is missing: {required}"
        );
    }
    for forbidden in [
        "score_ratified",
        "dispatch",
        "execute_",
        "SendInput",
        "SetCursorPos",
        "ReadProcessMemory",
    ] {
        assert!(
            !CLI_SOURCE.contains(forbidden),
            "spectator CLI contains forbidden authority or channel: {forbidden}"
        );
    }
}

#[test]
fn current_producer_source_explains_why_safe_surfaces_are_not_seated_duels() {
    for required in [
        "!TryBoundedCollectionV1(playersValue, 2, out List<object> players)",
        "players.Count != 2",
        "localPlayers != 1",
    ] {
        assert!(
            PRODUCER_SOURCE.contains(required),
            "producer source constraint is missing: {required}"
        );
    }
    for required in [
        "!TryBoundedCollectionV1(playersValue, 2, out List<object> players)",
        "players.Count != 2",
    ] {
        assert!(
            SANITIZER_SOURCE.contains(required),
            "sanitizer source constraint is missing: {required}"
        );
    }
}
