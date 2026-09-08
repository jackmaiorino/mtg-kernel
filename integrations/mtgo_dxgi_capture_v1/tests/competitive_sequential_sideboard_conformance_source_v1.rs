#![cfg(target_os = "windows")]

use mtgo_dxgi_capture_v1::{
    advance_competitive_native_sideboard_deliberation_v1,
    begin_competitive_native_sideboard_deliberation_v1,
    competitive_native_sideboard_model_input_commitment_v1,
    MtgoCompetitiveNativeSideboardDeliberationActionV1,
    MtgoCompetitiveNativeSideboardDeliberationAdvanceV1,
    MtgoCompetitiveNativeSideboardDeliberationDecisionV1,
    MtgoCompetitiveNativeSideboardDeliberationScoreResponseV1,
    MtgoCompetitiveNativeSideboardDeliberationScorerV1, MtgoCompetitiveNativeSideboardModelInputV1,
    MTGO_COMPETITIVE_NATIVE_SIDEBOARD_DELIBERATION_SCHEMA_V1,
};
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};

const AUXILIARY_FIXTURE_JSON: &str = include_str!(
    "../fixtures/player_visible_competitive_auxiliary_heads_conformance_source_v1.json"
);
const SEQUENTIAL_FIXTURE_JSON: &str = include_str!(
    "../fixtures/player_visible_competitive_sequential_sideboard_conformance_source_v1.json"
);

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SequentialFixtureV1 {
    schema_version: u32,
    fixture_id: String,
    source_auxiliary_fixture_sha256: String,
    source_model_input_commitment_sha256: String,
    source_completed_match_history_commitment_sha256: String,
    deployment_commitment_sha256: String,
    unchanged: SequentialCaseV1,
    changed: SequentialCaseV1,
    whole_target_response_satisfies_native_model_provenance: bool,
    safe_for_live_input: bool,
    permits_sideboard_submission: bool,
    permits_event_entry: bool,
    permits_spending: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SequentialCaseV1 {
    steps: Vec<SequentialStepV1>,
    expected_no_changes_selected: bool,
    expected_decisions_consumed: u8,
    expected_model_selection_commitment_sha256: String,
    expected_final_trace_commitment_sha256: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SequentialStepV1 {
    expected_decision_number: u8,
    expected_ordered_action_count: usize,
    expected_selected_index: usize,
    expected_selected_action: MtgoCompetitiveNativeSideboardDeliberationActionV1,
    unselected_logit_f32_bits: u32,
    selected_logit_f32_bits: u32,
    value_f32_bits: u32,
    expected_decision_commitment_sha256: String,
    expected_step_receipt_commitment_sha256: String,
    expected_trace_after_step_sha256: String,
}

#[derive(Debug)]
struct GeneratedStepV1 {
    decision_commitment_sha256: String,
    step_receipt_commitment_sha256: String,
    trace_after_step_sha256: String,
}

#[derive(Debug)]
struct GeneratedCaseV1 {
    steps: Vec<GeneratedStepV1>,
    final_trace_commitment_sha256: String,
}

struct ScriptedSequentialScorerV1<'a> {
    steps: &'a [SequentialStepV1],
    next_step: usize,
}

impl MtgoCompetitiveNativeSideboardDeliberationScorerV1 for ScriptedSequentialScorerV1<'_> {
    fn score_sideboard_deliberation_v1(
        &mut self,
        decision: &MtgoCompetitiveNativeSideboardDeliberationDecisionV1,
    ) -> Result<MtgoCompetitiveNativeSideboardDeliberationScoreResponseV1, String> {
        let expected = self
            .steps
            .get(self.next_step)
            .ok_or("sequential fixture scorer received an extra decision")?;
        if decision.decision_number != expected.expected_decision_number
            || decision.ordered_actions.len() != expected.expected_ordered_action_count
            || decision
                .ordered_actions
                .get(expected.expected_selected_index)
                != Some(&expected.expected_selected_action)
        {
            return Err("sequential fixture decision shape or selected action changed".to_owned());
        }
        let mut ordered_logits_f32_bits =
            vec![expected.unselected_logit_f32_bits; decision.ordered_actions.len()];
        ordered_logits_f32_bits[expected.expected_selected_index] =
            expected.selected_logit_f32_bits;
        self.next_step += 1;
        Ok(MtgoCompetitiveNativeSideboardDeliberationScoreResponseV1 {
            schema_version: MTGO_COMPETITIVE_NATIVE_SIDEBOARD_DELIBERATION_SCHEMA_V1,
            decision_commitment_sha256: decision.decision_commitment_sha256.clone(),
            deployment_commitment_sha256: decision.deployment_commitment_sha256.clone(),
            ordered_logits_f32_bits,
            value_f32_bits: expected.value_f32_bits,
        })
    }
}

fn sha256_v1(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn source_model_input_v1() -> MtgoCompetitiveNativeSideboardModelInputV1 {
    let value: Value = serde_json::from_str(AUXILIARY_FIXTURE_JSON).unwrap();
    serde_json::from_value(value["sideboard"]["model_input"].clone()).unwrap()
}

fn run_case_v1(
    fixture: &SequentialFixtureV1,
    case: &SequentialCaseV1,
    model_input: &MtgoCompetitiveNativeSideboardModelInputV1,
) -> GeneratedCaseV1 {
    let mut scorer = ScriptedSequentialScorerV1 {
        steps: &case.steps,
        next_step: 0,
    };
    let mut deliberation = begin_competitive_native_sideboard_deliberation_v1(
        model_input.clone(),
        &fixture.deployment_commitment_sha256,
    )
    .unwrap();
    let mut generated_steps = Vec::new();
    loop {
        let advance =
            advance_competitive_native_sideboard_deliberation_v1(deliberation, &mut scorer)
                .unwrap();
        match advance {
            MtgoCompetitiveNativeSideboardDeliberationAdvanceV1::Continue {
                deliberation: next,
                receipt,
            } => {
                generated_steps.push(GeneratedStepV1 {
                    decision_commitment_sha256: receipt.decision_commitment_sha256,
                    step_receipt_commitment_sha256: receipt.step_receipt_commitment_sha256,
                    trace_after_step_sha256: next.trace_commitment_sha256_v1().to_owned(),
                });
                deliberation = next;
            }
            MtgoCompetitiveNativeSideboardDeliberationAdvanceV1::Submitted {
                submission,
                receipt,
            } => {
                generated_steps.push(GeneratedStepV1 {
                    decision_commitment_sha256: receipt.decision_commitment_sha256,
                    step_receipt_commitment_sha256: receipt.step_receipt_commitment_sha256,
                    trace_after_step_sha256: submission.trace_commitment_sha256_v1().to_owned(),
                });
                assert_eq!(scorer.next_step, case.steps.len());
                assert_eq!(
                    submission.no_changes_selected_v1(),
                    case.expected_no_changes_selected
                );
                assert_eq!(
                    submission.decisions_consumed_v1(),
                    case.expected_decisions_consumed
                );
                assert_eq!(
                    submission.model_selection_commitment_sha256_v1(),
                    case.expected_model_selection_commitment_sha256
                );
                assert!(!submission.safe_for_live_input_v1());
                assert!(!submission.permits_sideboard_submission_v1());
                assert!(!submission.permits_event_entry_v1());
                assert!(!submission.permits_spending_v1());
                return GeneratedCaseV1 {
                    steps: generated_steps,
                    final_trace_commitment_sha256: submission
                        .trace_commitment_sha256_v1()
                        .to_owned(),
                };
            }
        }
    }
}

fn assert_generated_case_v1(case: &SequentialCaseV1, generated: &GeneratedCaseV1) {
    assert_eq!(case.steps.len(), generated.steps.len());
    for (expected, actual) in case.steps.iter().zip(&generated.steps) {
        assert_eq!(
            expected.expected_decision_commitment_sha256,
            actual.decision_commitment_sha256
        );
        assert_eq!(
            expected.expected_step_receipt_commitment_sha256,
            actual.step_receipt_commitment_sha256
        );
        assert_eq!(
            expected.expected_trace_after_step_sha256,
            actual.trace_after_step_sha256
        );
    }
    assert_eq!(
        case.expected_final_trace_commitment_sha256,
        generated.final_trace_commitment_sha256
    );
}

#[test]
fn source_fixture_pins_sequential_changed_and_unchanged_sideboard_traces() {
    let fixture: SequentialFixtureV1 = serde_json::from_str(SEQUENTIAL_FIXTURE_JSON).unwrap();
    assert_eq!(fixture.schema_version, 1);
    assert_eq!(
        fixture.fixture_id,
        "player-visible-competitive-sequential-sideboard-game-two-v1"
    );
    assert_eq!(
        fixture.source_auxiliary_fixture_sha256,
        sha256_v1(AUXILIARY_FIXTURE_JSON.as_bytes())
    );
    assert!(!fixture.whole_target_response_satisfies_native_model_provenance);
    assert!(!fixture.safe_for_live_input);
    assert!(!fixture.permits_sideboard_submission);
    assert!(!fixture.permits_event_entry);
    assert!(!fixture.permits_spending);

    let source_value: Value = serde_json::from_str(AUXILIARY_FIXTURE_JSON).unwrap();
    assert_eq!(
        fixture.source_completed_match_history_commitment_sha256,
        source_value["expected_completed_match_history_source_commitment_sha256"]
            .as_str()
            .unwrap()
    );
    let model_input = source_model_input_v1();
    assert_eq!(
        fixture.source_model_input_commitment_sha256,
        competitive_native_sideboard_model_input_commitment_v1(&model_input).unwrap()
    );
    assert_eq!(
        fixture.deployment_commitment_sha256,
        source_value["fixture_deployment_commitment_sha256"]
            .as_str()
            .unwrap()
    );

    let unchanged = run_case_v1(&fixture, &fixture.unchanged, &model_input);
    let changed = run_case_v1(&fixture, &fixture.changed, &model_input);
    assert_generated_case_v1(&fixture.unchanged, &unchanged);
    assert_generated_case_v1(&fixture.changed, &changed);
}

#[test]
fn sequential_fixture_rejects_unknown_fields_and_hidden_state_keys() {
    let mut value: Value = serde_json::from_str(SEQUENTIAL_FIXTURE_JSON).unwrap();
    value.as_object_mut().unwrap().insert(
        "opponent_deck_id".to_owned(),
        Value::String("forbidden".to_owned()),
    );
    assert!(serde_json::from_value::<SequentialFixtureV1>(value).is_err());
    for forbidden in [
        "opponent_deck_id",
        "event_identity",
        "account",
        "capture",
        "frame_id",
        "rect",
        "card_db_id",
        "arena_id",
        "adapter_object_id",
        "client_object_id",
        "raw_game_log_text",
    ] {
        assert!(!SEQUENTIAL_FIXTURE_JSON.contains(forbidden));
    }
}
