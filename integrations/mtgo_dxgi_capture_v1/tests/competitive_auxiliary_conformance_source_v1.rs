#![cfg(target_os = "windows")]

use mtgo_blackbox_v1::MtgoPlayerVisibleConfirmedDuelDecisionV1;
use mtgo_dxgi_capture_v1::{
    competitive_native_pregame_model_input_commitment_v1,
    competitive_native_sideboard_model_input_commitment_v1,
    competitive_native_sideboard_model_selection_commitment_v1,
    score_checked_untrusted_competitive_native_pregame_v1,
    score_checked_untrusted_competitive_native_sideboard_v1,
    validate_competitive_native_pregame_model_input_v1,
    validate_competitive_native_sideboard_model_input_v1,
    validate_competitive_native_sideboard_model_selection_v1,
    MtgoCompetitiveExternalPublicHistoryOrderingV1, MtgoCompetitiveNativePregameActionV1,
    MtgoCompetitiveNativePregameModelInputV1, MtgoCompetitiveNativePregameScoreResponseV1,
    MtgoCompetitiveNativePregameScorerV1, MtgoCompetitiveNativeSideboardModelInputV1,
    MtgoCompetitiveNativeSideboardModelSelectionV1, MtgoCompetitiveNativeSideboardScoreResponseV1,
    MtgoCompetitiveNativeSideboardScorerV1, MtgoCompetitivePlayerRelativeGameWinnerV1,
    MTGO_COMPETITIVE_AUXILIARY_MODEL_SCORING_SCHEMA_V1,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

const FIXTURE_JSON: &str = include_str!(
    "../fixtures/player_visible_competitive_auxiliary_heads_conformance_source_v1.json"
);
const LONDON_FIXTURE_JSON: &str = include_str!(
    "../fixtures/player_visible_competitive_london_bottoming_conformance_source_v1.json"
);
const COMPLETED_HISTORY_SOURCE_DOMAIN_V1: &[u8] =
    b"mtgo-player-visible-completed-match-history-conformance-source-v1";
const PREGAME_SOURCE_DOMAIN_V1: &[u8] = b"mtgo-player-visible-pregame-conformance-source-v1";
const SIDEBOARD_SOURCE_DOMAIN_V1: &[u8] = b"mtgo-player-visible-sideboard-conformance-source-v1";

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ConformanceFixtureV1 {
    schema_version: u32,
    fixture_id: String,
    fixture_deployment_commitment_sha256: String,
    completed_match_history: CompletedMatchHistorySourceV1,
    expected_completed_match_history_source_commitment_sha256: String,
    pregame: PregameFixtureV1,
    sideboard: SideboardFixtureV1,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LondonConformanceFixtureV1 {
    schema_version: u32,
    fixture_id: String,
    fixture_deployment_commitment_sha256: String,
    completed_match_history: CompletedMatchHistorySourceV1,
    expected_completed_match_history_source_commitment_sha256: String,
    pregame_cases: Vec<LondonPregameFixtureV1>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CompletedMatchHistorySourceV1 {
    ordering: MtgoCompetitiveExternalPublicHistoryOrderingV1,
    games: Vec<CompletedGameSourceV1>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CompletedGameSourceV1 {
    game_number: u8,
    winner: MtgoCompetitivePlayerRelativeGameWinnerV1,
    confirmed_decisions: Vec<ConfirmedDecisionSourceV1>,
    public_game_log_events: Vec<PublicGameLogEventSourceV1>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ConfirmedDecisionSourceV1 {
    within_source_position: u64,
    decision: MtgoPlayerVisibleConfirmedDuelDecisionV1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum PublicGameLogEventKindSourceV1 {
    PlayedCard,
    CastSpell,
    WonGame,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum PublicGameLogPlayerRoleSourceV1 {
    ActingPlayer,
    Opponent,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PublicGameLogEventSourceV1 {
    within_source_position: u32,
    kind: PublicGameLogEventKindSourceV1,
    actor_role: Option<PublicGameLogPlayerRoleSourceV1>,
    turn_number: Option<u32>,
    primary_count: Option<u8>,
    secondary_count: Option<u8>,
    visible_card_names: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PregameFixtureV1 {
    model_input: MtgoCompetitiveNativePregameModelInputV1,
    expected_model_input_commitment_sha256: String,
    expected_source_commitment_sha256: String,
    ordered_action_logits_f32_bits: Vec<u32>,
    value_f32_bits: u32,
    expected_selected_index: usize,
    expected_selected_action: MtgoCompetitiveNativePregameActionV1,
    expected_checked_selection_commitment_sha256: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LondonPregameFixtureV1 {
    case_id: String,
    model_input: MtgoCompetitiveNativePregameModelInputV1,
    expected_model_input_commitment_sha256: String,
    expected_source_commitment_sha256: String,
    ordered_action_logits_f32_bits: Vec<u32>,
    value_f32_bits: u32,
    expected_selected_index: usize,
    expected_selected_action: MtgoCompetitiveNativePregameActionV1,
    expected_checked_selection_commitment_sha256: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SideboardFixtureV1 {
    model_input: MtgoCompetitiveNativeSideboardModelInputV1,
    expected_model_input_commitment_sha256: String,
    expected_source_commitment_sha256: String,
    value_f32_bits: u32,
    unchanged_selection: MtgoCompetitiveNativeSideboardModelSelectionV1,
    expected_unchanged_model_selection_commitment_sha256: String,
    expected_unchanged_checked_selection_commitment_sha256: String,
    changed_selection: MtgoCompetitiveNativeSideboardModelSelectionV1,
    expected_changed_model_selection_commitment_sha256: String,
    expected_changed_checked_selection_commitment_sha256: String,
}

struct PregameFixtureScorerV1<'a> {
    ordered_action_logits_f32_bits: &'a [u32],
    value_f32_bits: u32,
}

impl MtgoCompetitiveNativePregameScorerV1 for PregameFixtureScorerV1<'_> {
    fn score_pregame_v1(
        &mut self,
        _model_input: &MtgoCompetitiveNativePregameModelInputV1,
        model_input_commitment_sha256: &str,
        deployment_commitment_sha256: &str,
    ) -> Result<MtgoCompetitiveNativePregameScoreResponseV1, String> {
        Ok(MtgoCompetitiveNativePregameScoreResponseV1 {
            schema_version: MTGO_COMPETITIVE_AUXILIARY_MODEL_SCORING_SCHEMA_V1,
            model_input_commitment_sha256: model_input_commitment_sha256.to_owned(),
            deployment_commitment_sha256: deployment_commitment_sha256.to_owned(),
            ordered_action_logits_f32_bits: self.ordered_action_logits_f32_bits.to_vec(),
            value_f32_bits: self.value_f32_bits,
        })
    }
}

struct SideboardFixtureScorerV1<'a> {
    selection: &'a MtgoCompetitiveNativeSideboardModelSelectionV1,
    value_f32_bits: u32,
}

impl MtgoCompetitiveNativeSideboardScorerV1 for SideboardFixtureScorerV1<'_> {
    fn score_sideboard_v1(
        &mut self,
        _model_input: &MtgoCompetitiveNativeSideboardModelInputV1,
        model_input_commitment_sha256: &str,
        deployment_commitment_sha256: &str,
    ) -> Result<MtgoCompetitiveNativeSideboardScoreResponseV1, String> {
        Ok(MtgoCompetitiveNativeSideboardScoreResponseV1 {
            schema_version: MTGO_COMPETITIVE_AUXILIARY_MODEL_SCORING_SCHEMA_V1,
            model_input_commitment_sha256: model_input_commitment_sha256.to_owned(),
            deployment_commitment_sha256: deployment_commitment_sha256.to_owned(),
            selection: self.selection.clone(),
            value_f32_bits: self.value_f32_bits,
        })
    }
}

fn commitment_v1(domain: &[u8], parts: &[&[u8]]) -> String {
    let mut hasher = Sha256::new();
    hasher.update((domain.len() as u64).to_be_bytes());
    hasher.update(domain);
    for part in parts {
        hasher.update((part.len() as u64).to_be_bytes());
        hasher.update(part);
    }
    format!("{:x}", hasher.finalize())
}

fn history_source_commitment_v1(history: &CompletedMatchHistorySourceV1) -> String {
    let canonical = serde_json::to_vec(history).unwrap();
    commitment_v1(
        COMPLETED_HISTORY_SOURCE_DOMAIN_V1,
        &[
            &canonical,
            b"two_independent_player_visible_streams_no_adapter_lineage_no_authority",
        ],
    )
}

fn assert_strictly_increasing<T: Copy + Ord + std::fmt::Debug>(values: &[T]) {
    assert!(
        values.windows(2).all(|pair| pair[0] < pair[1]),
        "{values:?}"
    );
}

fn assert_no_forbidden_model_keys(value: &Value) {
    match value {
        Value::Object(fields) => {
            for (key, child) in fields {
                assert!(
                    !matches!(
                        key.as_str(),
                        "event_identity"
                            | "match_identity"
                            | "account"
                            | "capture"
                            | "frame_id"
                            | "frame_sequence"
                            | "rect"
                            | "card_db_id"
                            | "arena_id"
                            | "zone_change_count"
                            | "adapter_object_id"
                            | "client_object_id"
                            | "source_visible_text_sha256"
                            | "raw_game_log_text"
                    ),
                    "forbidden model-source field leaked: {key}"
                );
                assert_no_forbidden_model_keys(child);
            }
        }
        Value::Array(values) => values.iter().for_each(assert_no_forbidden_model_keys),
        _ => {}
    }
}

#[test]
fn canonical_source_binds_complete_game_two_public_information() {
    let fixture: ConformanceFixtureV1 = serde_json::from_str(FIXTURE_JSON).unwrap();
    assert_eq!(fixture.schema_version, 1);
    assert_eq!(
        fixture.fixture_id,
        "player-visible-competitive-auxiliary-heads-game-two-v1"
    );
    assert_eq!(fixture.completed_match_history.games.len(), 1);
    assert_eq!(
        fixture.completed_match_history.ordering,
        MtgoCompetitiveExternalPublicHistoryOrderingV1::SeparateOrderedStreamsNoCrossSourceTotalOrder
    );
    let game = &fixture.completed_match_history.games[0];
    assert_eq!(game.game_number, 1);
    assert_eq!(
        game.winner,
        MtgoCompetitivePlayerRelativeGameWinnerV1::Opponent
    );
    assert_strictly_increasing(
        &game
            .confirmed_decisions
            .iter()
            .map(|decision| decision.within_source_position)
            .collect::<Vec<_>>(),
    );
    assert_strictly_increasing(
        &game
            .public_game_log_events
            .iter()
            .map(|event| event.within_source_position)
            .collect::<Vec<_>>(),
    );
    assert_eq!(fixture.pregame.model_input.game_number, 2);
    assert_eq!(fixture.sideboard.model_input.next_game_number, 2);
    assert_eq!(fixture.pregame.model_input.acting_player_games_won, 0);
    assert_eq!(fixture.pregame.model_input.opponent_games_won, 1);
    assert_eq!(fixture.sideboard.model_input.acting_player_games_won, 0);
    assert_eq!(fixture.sideboard.model_input.opponent_games_won, 1);

    let history_commitment = history_source_commitment_v1(&fixture.completed_match_history);
    assert_eq!(
        history_commitment,
        fixture.expected_completed_match_history_source_commitment_sha256
    );
    let pregame_source_commitment = commitment_v1(
        PREGAME_SOURCE_DOMAIN_V1,
        &[
            fixture
                .pregame
                .expected_model_input_commitment_sha256
                .as_bytes(),
            history_commitment.as_bytes(),
            b"complete_prior_games_plus_exact_current_pregame_decision_no_authority",
        ],
    );
    assert_eq!(
        pregame_source_commitment,
        fixture.pregame.expected_source_commitment_sha256
    );
    let sideboard_source_commitment = commitment_v1(
        SIDEBOARD_SOURCE_DOMAIN_V1,
        &[
            fixture
                .sideboard
                .expected_model_input_commitment_sha256
                .as_bytes(),
            history_commitment.as_bytes(),
            b"complete_prior_games_plus_exact_current_sideboard_decision_no_authority",
        ],
    );
    assert_eq!(
        sideboard_source_commitment,
        fixture.sideboard.expected_source_commitment_sha256
    );

    let sanitized = serde_json::json!({
        "history": fixture.completed_match_history,
        "pregame": fixture.pregame.model_input,
        "sideboard": fixture.sideboard.model_input,
    });
    assert_no_forbidden_model_keys(&sanitized);
}

#[test]
fn pregame_source_validates_and_selects_the_exact_ordered_action() {
    let fixture: ConformanceFixtureV1 = serde_json::from_str(FIXTURE_JSON).unwrap();
    validate_competitive_native_pregame_model_input_v1(&fixture.pregame.model_input).unwrap();
    let input_commitment =
        competitive_native_pregame_model_input_commitment_v1(&fixture.pregame.model_input).unwrap();
    assert_eq!(
        input_commitment,
        fixture.pregame.expected_model_input_commitment_sha256
    );
    let mut scorer = PregameFixtureScorerV1 {
        ordered_action_logits_f32_bits: &fixture.pregame.ordered_action_logits_f32_bits,
        value_f32_bits: fixture.pregame.value_f32_bits,
    };
    let checked = score_checked_untrusted_competitive_native_pregame_v1(
        &fixture.pregame.model_input,
        &fixture.fixture_deployment_commitment_sha256,
        &mut scorer,
    )
    .unwrap();
    assert_eq!(
        checked.selected_index_v1(),
        fixture.pregame.expected_selected_index
    );
    assert_eq!(
        checked.selected_action_v1(),
        &fixture.pregame.expected_selected_action
    );
    assert_eq!(
        checked.selection_commitment_sha256_v1(),
        fixture.pregame.expected_checked_selection_commitment_sha256
    );
    assert!(!checked.safe_for_live_input_v1());
    assert!(!checked.permits_event_session_recovery_v1());
}

#[test]
fn sideboard_source_validates_changed_and_unchanged_targets() {
    let fixture: ConformanceFixtureV1 = serde_json::from_str(FIXTURE_JSON).unwrap();
    validate_competitive_native_sideboard_model_input_v1(&fixture.sideboard.model_input).unwrap();
    let input_commitment =
        competitive_native_sideboard_model_input_commitment_v1(&fixture.sideboard.model_input)
            .unwrap();
    assert_eq!(
        input_commitment,
        fixture.sideboard.expected_model_input_commitment_sha256
    );

    for (selection, expected_model_commitment, expected_checked_commitment, no_changes) in [
        (
            &fixture.sideboard.unchanged_selection,
            &fixture
                .sideboard
                .expected_unchanged_model_selection_commitment_sha256,
            &fixture
                .sideboard
                .expected_unchanged_checked_selection_commitment_sha256,
            true,
        ),
        (
            &fixture.sideboard.changed_selection,
            &fixture
                .sideboard
                .expected_changed_model_selection_commitment_sha256,
            &fixture
                .sideboard
                .expected_changed_checked_selection_commitment_sha256,
            false,
        ),
    ] {
        validate_competitive_native_sideboard_model_selection_v1(
            &fixture.sideboard.model_input,
            selection,
        )
        .unwrap();
        assert_eq!(
            competitive_native_sideboard_model_selection_commitment_v1(
                &fixture.sideboard.model_input,
                selection,
            )
            .unwrap(),
            *expected_model_commitment
        );
        let mut scorer = SideboardFixtureScorerV1 {
            selection,
            value_f32_bits: fixture.sideboard.value_f32_bits,
        };
        let checked = score_checked_untrusted_competitive_native_sideboard_v1(
            &fixture.sideboard.model_input,
            &fixture.fixture_deployment_commitment_sha256,
            &mut scorer,
        )
        .unwrap();
        assert_eq!(
            checked.checked_selection_commitment_sha256_v1(),
            *expected_checked_commitment
        );
        assert_eq!(
            checked.no_changes_selected_v1(&fixture.sideboard.model_input),
            no_changes
        );
        assert!(!checked.safe_for_live_input_v1());
        assert!(!checked.permits_sideboard_submission_v1());
    }
}

#[test]
fn fixture_rejects_unknown_fields_and_history_is_not_optional_for_game_two() {
    let value: Value = serde_json::from_str(FIXTURE_JSON).unwrap();
    let mut extra = value.clone();
    extra.as_object_mut().unwrap().insert(
        "event_identity".to_owned(),
        Value::String("forbidden".to_owned()),
    );
    assert!(serde_json::from_value::<ConformanceFixtureV1>(extra).is_err());

    let mut missing_history = value;
    missing_history
        .as_object_mut()
        .unwrap()
        .remove("completed_match_history");
    assert!(serde_json::from_value::<ConformanceFixtureV1>(missing_history).is_err());
}

#[test]
fn london_bottoming_source_requires_empty_game_one_history_and_pins_select_then_submit() {
    let fixture: LondonConformanceFixtureV1 = serde_json::from_str(LONDON_FIXTURE_JSON).unwrap();
    assert_eq!(fixture.schema_version, 1);
    assert_eq!(
        fixture.fixture_id,
        "player-visible-competitive-london-bottoming-game-one-v1"
    );
    assert!(fixture.completed_match_history.games.is_empty());
    assert_eq!(
        fixture.completed_match_history.ordering,
        MtgoCompetitiveExternalPublicHistoryOrderingV1::SeparateOrderedStreamsNoCrossSourceTotalOrder
    );
    let history_commitment = history_source_commitment_v1(&fixture.completed_match_history);
    assert_eq!(
        history_commitment,
        fixture.expected_completed_match_history_source_commitment_sha256
    );
    assert_eq!(fixture.pregame_cases.len(), 2);

    for case in &fixture.pregame_cases {
        assert_eq!(case.model_input.game_number, 1);
        assert_eq!(case.model_input.acting_player_games_won, 0);
        assert_eq!(case.model_input.opponent_games_won, 0);
        validate_competitive_native_pregame_model_input_v1(&case.model_input).unwrap();
        let input_commitment =
            competitive_native_pregame_model_input_commitment_v1(&case.model_input).unwrap();
        assert_eq!(
            input_commitment,
            case.expected_model_input_commitment_sha256
        );
        let source_commitment = commitment_v1(
            PREGAME_SOURCE_DOMAIN_V1,
            &[
                input_commitment.as_bytes(),
                history_commitment.as_bytes(),
                b"complete_prior_games_plus_exact_current_pregame_decision_no_authority",
            ],
        );
        assert_eq!(source_commitment, case.expected_source_commitment_sha256);

        let mut scorer = PregameFixtureScorerV1 {
            ordered_action_logits_f32_bits: &case.ordered_action_logits_f32_bits,
            value_f32_bits: case.value_f32_bits,
        };
        let checked = score_checked_untrusted_competitive_native_pregame_v1(
            &case.model_input,
            &fixture.fixture_deployment_commitment_sha256,
            &mut scorer,
        )
        .unwrap();
        assert_eq!(checked.selected_index_v1(), case.expected_selected_index);
        assert_eq!(checked.selected_action_v1(), &case.expected_selected_action);
        assert_eq!(
            checked.selection_commitment_sha256_v1(),
            case.expected_checked_selection_commitment_sha256
        );
        assert!(!checked.safe_for_live_input_v1());
        assert!(!checked.permits_event_session_recovery_v1());
    }

    assert_eq!(
        fixture.pregame_cases[0].case_id,
        "bottom_one_select_card_v1"
    );
    assert!(matches!(
        fixture.pregame_cases[0].expected_selected_action,
        MtgoCompetitiveNativePregameActionV1::SelectForBottom { card_slot: 1 }
    ));
    assert_eq!(fixture.pregame_cases[1].case_id, "bottom_one_submit_v1");
    assert!(matches!(
        fixture.pregame_cases[1].expected_selected_action,
        MtgoCompetitiveNativePregameActionV1::SubmitBottoming
    ));

    let sanitized: Value = serde_json::from_str(LONDON_FIXTURE_JSON).unwrap();
    assert_no_forbidden_model_keys(&sanitized);
}
