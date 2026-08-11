use crate::{
    duel_action_family_v1, validate_duel_gesture_shape_v1, MtgoContractErrorV1,
    MtgoDuelActionFamilyV1, MtgoDuelGesturePlanV1,
};
use mtg_kernel::rl::ActionSemanticV1;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;

pub const MTGO_DUEL_GESTURE_EVALUATION_SCHEMA_V1: u32 = 1;

const DUEL_GESTURE_EVALUATION_COMMITMENT_DOMAIN_V1: &[u8] = b"mtgo-duel-gesture-evaluation-v1";
const DUEL_GESTURE_PROFILE_ADMISSION_DOMAIN_V1: &[u8] = b"mtgo-duel-gesture-profile-admission-v1";
pub(crate) const RATIFIED_DUEL_GESTURE_EVALUATION_COMMITMENT_V1: Option<&str> = None;
const MAX_GESTURE_EVALUATION_CASES_V1: usize = 2_048;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoDuelGestureEvaluationCaseV1 {
    pub case_id: String,
    pub source_manifest_sha256: String,
    pub source_frame_sha256: String,
    pub selected_semantic: ActionSemanticV1,
    pub expected_plan: MtgoDuelGesturePlanV1,
    pub predicted_plan: Option<MtgoDuelGesturePlanV1>,
    pub annotator_receipt_sha256: String,
    pub reviewed_at_utc: String,
    pub source_frame_visually_confirmed: bool,
    pub selected_semantic_visually_confirmed: bool,
    pub gesture_sequence_manually_demonstrated: bool,
    pub visible_postcondition_confirmed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoDuelGestureEvaluationV1 {
    pub schema_version: u32,
    pub evaluation_id: String,
    pub perception_profile_admission_commitment_sha256: String,
    pub gesture_target_runtime_binary_sha256: String,
    pub gesture_target_assets_manifest_sha256: String,
    pub corpus_manifest_sha256: String,
    pub annotation_protocol_sha256: String,
    pub minimum_prediction_coverage_bps: u16,
    pub required_action_families: Vec<MtgoDuelActionFamilyV1>,
    pub cases: Vec<MtgoDuelGestureEvaluationCaseV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoDuelGestureFamilyEvaluationOutcomeV1 {
    pub action_family: MtgoDuelActionFamilyV1,
    pub case_count: u32,
    pub prediction_count: u32,
    pub exact_plan_count: u32,
    pub prediction_coverage_bps: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoDuelGestureEvaluationOutcomeV1 {
    pub total_case_count: u32,
    pub total_prediction_count: u32,
    pub total_exact_plan_count: u32,
    pub aggregate_prediction_coverage_bps: u16,
    pub family_outcomes: Vec<MtgoDuelGestureFamilyEvaluationOutcomeV1>,
    pub gate_passed: bool,
}

/// Structurally checked reviewed-corpus evaluation. Human labels and manual
/// demonstrations remain external review inputs, so this is not production
/// gesture authority.
pub struct CheckedUntrustedMtgoDuelGestureEvaluationV1 {
    record: MtgoDuelGestureEvaluationV1,
    outcome: MtgoDuelGestureEvaluationOutcomeV1,
    evaluation_commitment_sha256: String,
}

impl CheckedUntrustedMtgoDuelGestureEvaluationV1 {
    pub fn outcome_v1(&self) -> &MtgoDuelGestureEvaluationOutcomeV1 {
        &self.outcome
    }

    pub fn evaluation_commitment_sha256(&self) -> &str {
        &self.evaluation_commitment_sha256
    }

    pub fn safe_for_live_input(&self) -> bool {
        false
    }

    pub fn permits_event_entry(&self) -> bool {
        false
    }
}

/// Exact compile-ratified gesture profile identity. It grants neither capture
/// nor input authority. The production ratification root is empty.
pub struct AdmittedMtgoDuelGestureProfileV1 {
    evaluation: CheckedUntrustedMtgoDuelGestureEvaluationV1,
    admission_commitment_sha256: String,
}

impl AdmittedMtgoDuelGestureProfileV1 {
    pub fn evaluation_commitment_sha256(&self) -> &str {
        self.evaluation.evaluation_commitment_sha256()
    }

    pub fn admission_commitment_sha256(&self) -> &str {
        &self.admission_commitment_sha256
    }

    pub fn perception_profile_admission_commitment_sha256(&self) -> &str {
        &self
            .evaluation
            .record
            .perception_profile_admission_commitment_sha256
    }

    pub fn gesture_target_runtime_binary_sha256(&self) -> &str {
        &self.evaluation.record.gesture_target_runtime_binary_sha256
    }

    pub fn gesture_target_assets_manifest_sha256(&self) -> &str {
        &self.evaluation.record.gesture_target_assets_manifest_sha256
    }

    pub fn supported_action_families(&self) -> &[MtgoDuelActionFamilyV1] {
        &self.evaluation.record.required_action_families
    }

    pub fn safe_for_live_input(&self) -> bool {
        false
    }
}

pub fn evaluate_untrusted_duel_gesture_profile_v1(
    record: MtgoDuelGestureEvaluationV1,
) -> Result<CheckedUntrustedMtgoDuelGestureEvaluationV1, MtgoContractErrorV1> {
    if record.schema_version != MTGO_DUEL_GESTURE_EVALUATION_SCHEMA_V1 {
        return Err(MtgoContractErrorV1::new(
            "duel_gesture_evaluation_schema_mismatch",
            record.schema_version.to_string(),
        ));
    }
    validate_identifier_v1(&record.evaluation_id, "duel_gesture_evaluation_id_invalid")?;
    for (value, code) in [
        (
            record
                .perception_profile_admission_commitment_sha256
                .as_str(),
            "duel_gesture_perception_profile_hash_invalid",
        ),
        (
            record.gesture_target_runtime_binary_sha256.as_str(),
            "duel_gesture_runtime_hash_invalid",
        ),
        (
            record.gesture_target_assets_manifest_sha256.as_str(),
            "duel_gesture_assets_hash_invalid",
        ),
        (
            record.corpus_manifest_sha256.as_str(),
            "duel_gesture_corpus_hash_invalid",
        ),
        (
            record.annotation_protocol_sha256.as_str(),
            "duel_gesture_annotation_hash_invalid",
        ),
    ] {
        require_sha256_v1(value, code)?;
    }
    if record.minimum_prediction_coverage_bps < 9_500
        || record.minimum_prediction_coverage_bps > 10_000
    {
        return Err(MtgoContractErrorV1::new(
            "duel_gesture_coverage_threshold_invalid",
            record.minimum_prediction_coverage_bps.to_string(),
        ));
    }
    let canonical_families = canonical_duel_gesture_action_families_v1();
    if record.required_action_families != canonical_families {
        return Err(MtgoContractErrorV1::new(
            "duel_gesture_required_families_invalid",
            "all eleven action families are required in canonical order",
        ));
    }
    if record.cases.len() < canonical_families.len()
        || record.cases.len() > MAX_GESTURE_EVALUATION_CASES_V1
    {
        return Err(MtgoContractErrorV1::new(
            "duel_gesture_case_count_invalid",
            record.cases.len().to_string(),
        ));
    }

    let mut case_ids = HashSet::new();
    let mut source_frames = HashSet::new();
    let mut counts = vec![(0_u32, 0_u32, 0_u32); canonical_families.len()];
    for case in &record.cases {
        validate_identifier_v1(&case.case_id, "duel_gesture_case_id_invalid")?;
        if !case_ids.insert(case.case_id.as_str()) {
            return Err(MtgoContractErrorV1::new(
                "duel_gesture_case_id_duplicate",
                &case.case_id,
            ));
        }
        require_sha256_v1(
            &case.source_manifest_sha256,
            "duel_gesture_case_manifest_hash_invalid",
        )?;
        require_sha256_v1(
            &case.source_frame_sha256,
            "duel_gesture_case_frame_hash_invalid",
        )?;
        require_sha256_v1(
            &case.annotator_receipt_sha256,
            "duel_gesture_annotator_receipt_invalid",
        )?;
        if !source_frames.insert((
            case.source_manifest_sha256.as_str(),
            case.source_frame_sha256.as_str(),
        )) {
            return Err(MtgoContractErrorV1::new(
                "duel_gesture_case_source_duplicate",
                &case.case_id,
            ));
        }
        validate_utc_timestamp_v1(&case.reviewed_at_utc)?;
        if !case.source_frame_visually_confirmed
            || !case.selected_semantic_visually_confirmed
            || !case.gesture_sequence_manually_demonstrated
            || !case.visible_postcondition_confirmed
        {
            return Err(MtgoContractErrorV1::new(
                "duel_gesture_case_review_incomplete",
                &case.case_id,
            ));
        }
        let family = duel_action_family_v1(&case.selected_semantic);
        if matches!(&case.selected_semantic, ActionSemanticV1::Ambiguous { .. }) {
            return Err(MtgoContractErrorV1::new(
                "duel_gesture_case_ambiguous",
                &case.case_id,
            ));
        }
        let family_index = canonical_families
            .iter()
            .position(|candidate| *candidate == family)
            .expect("canonical family inventory is complete");
        validate_evaluation_plan_v1(&case.expected_plan, &case.selected_semantic, &case.case_id)?;
        counts[family_index].0 += 1;
        if let Some(predicted) = &case.predicted_plan {
            validate_evaluation_plan_v1(predicted, &case.selected_semantic, &case.case_id)?;
            counts[family_index].1 += 1;
            if predicted == &case.expected_plan {
                counts[family_index].2 += 1;
            }
        }
    }

    let mut family_outcomes = Vec::with_capacity(canonical_families.len());
    let mut gate_passed = true;
    let mut total_prediction_count = 0_u32;
    let mut total_exact_plan_count = 0_u32;
    for (action_family, (case_count, prediction_count, exact_plan_count)) in
        canonical_families.into_iter().zip(counts)
    {
        let prediction_coverage_bps = coverage_bps_v1(prediction_count, case_count);
        if case_count == 0
            || prediction_coverage_bps < record.minimum_prediction_coverage_bps
            || exact_plan_count != prediction_count
        {
            gate_passed = false;
        }
        total_prediction_count += prediction_count;
        total_exact_plan_count += exact_plan_count;
        family_outcomes.push(MtgoDuelGestureFamilyEvaluationOutcomeV1 {
            action_family,
            case_count,
            prediction_count,
            exact_plan_count,
            prediction_coverage_bps,
        });
    }
    let total_case_count = record.cases.len() as u32;
    let outcome = MtgoDuelGestureEvaluationOutcomeV1 {
        total_case_count,
        total_prediction_count,
        total_exact_plan_count,
        aggregate_prediction_coverage_bps: coverage_bps_v1(
            total_prediction_count,
            total_case_count,
        ),
        family_outcomes,
        gate_passed,
    };
    let record_json = serde_json::to_vec(&record).map_err(|error| {
        MtgoContractErrorV1::new(
            "duel_gesture_evaluation_serialization_failed",
            error.to_string(),
        )
    })?;
    let outcome_json = serde_json::to_vec(&outcome).map_err(|error| {
        MtgoContractErrorV1::new(
            "duel_gesture_outcome_serialization_failed",
            error.to_string(),
        )
    })?;
    let mut hasher = Sha256::new();
    hasher.update(DUEL_GESTURE_EVALUATION_COMMITMENT_DOMAIN_V1);
    hasher.update((record_json.len() as u64).to_le_bytes());
    hasher.update(record_json);
    hasher.update((outcome_json.len() as u64).to_le_bytes());
    hasher.update(outcome_json);
    hasher.update(b"reviewed_corpus_checked_untrusted_no_runtime_or_input_authority");
    Ok(CheckedUntrustedMtgoDuelGestureEvaluationV1 {
        record,
        outcome,
        evaluation_commitment_sha256: format!("{:x}", hasher.finalize()),
    })
}

pub fn admit_ratified_duel_gesture_profile_v1(
    evaluation: CheckedUntrustedMtgoDuelGestureEvaluationV1,
) -> Result<AdmittedMtgoDuelGestureProfileV1, MtgoContractErrorV1> {
    if !evaluation.outcome.gate_passed {
        return Err(MtgoContractErrorV1::new(
            "duel_gesture_evaluation_gate_failed",
            "every family requires declared coverage and exact predicted plans",
        ));
    }
    let Some(ratified) = RATIFIED_DUEL_GESTURE_EVALUATION_COMMITMENT_V1 else {
        return Err(MtgoContractErrorV1::new(
            "duel_gesture_evaluation_not_ratified",
            evaluation.evaluation_commitment_sha256(),
        ));
    };
    if evaluation.evaluation_commitment_sha256() != ratified {
        return Err(MtgoContractErrorV1::new(
            "duel_gesture_evaluation_not_ratified",
            evaluation.evaluation_commitment_sha256(),
        ));
    }
    let mut hasher = Sha256::new();
    hasher.update(DUEL_GESTURE_PROFILE_ADMISSION_DOMAIN_V1);
    hasher.update(evaluation.evaluation_commitment_sha256().as_bytes());
    hasher.update(b"profile_identity_only_no_capture_or_input_authority");
    Ok(AdmittedMtgoDuelGestureProfileV1 {
        evaluation,
        admission_commitment_sha256: format!("{:x}", hasher.finalize()),
    })
}

pub fn canonical_duel_gesture_action_families_v1() -> Vec<MtgoDuelActionFamilyV1> {
    vec![
        MtgoDuelActionFamilyV1::PriorityPass,
        MtgoDuelActionFamilyV1::PlayLand,
        MtgoDuelActionFamilyV1::CastOrPlotSpell,
        MtgoDuelActionFamilyV1::ManaAbility,
        MtgoDuelActionFamilyV1::NonManaAbility,
        MtgoDuelActionFamilyV1::TargetChoice,
        MtgoDuelActionFamilyV1::CostOrModeChoice,
        MtgoDuelActionFamilyV1::EffectChoice,
        MtgoDuelActionFamilyV1::Discard,
        MtgoDuelActionFamilyV1::CombatChoice,
        MtgoDuelActionFamilyV1::TriggerOrdering,
    ]
}

fn validate_evaluation_plan_v1(
    plan: &MtgoDuelGesturePlanV1,
    semantic: &ActionSemanticV1,
    case_id: &str,
) -> Result<(), MtgoContractErrorV1> {
    if plan.schema_version != crate::MTGO_DUEL_GESTURE_PLAN_SCHEMA_V1
        || !plan.stage_set_complete
        || plan.frame_id == 0
        || plan.frame_sequence == 0
        || plan.selected_action_family != duel_action_family_v1(semantic)
    {
        return Err(MtgoContractErrorV1::new(
            "duel_gesture_evaluation_plan_binding_invalid",
            case_id,
        ));
    }
    for value in [
        plan.profile_bound_resolution_commitment_sha256.as_str(),
        plan.decision_commitment_sha256.as_str(),
        plan.selection_commitment_sha256.as_str(),
    ] {
        require_sha256_v1(value, "duel_gesture_evaluation_plan_hash_invalid")?;
    }
    validate_duel_gesture_shape_v1(semantic, &plan.stages)
}

fn coverage_bps_v1(numerator: u32, denominator: u32) -> u16 {
    if denominator == 0 {
        return 0;
    }
    ((u64::from(numerator) * 10_000) / u64::from(denominator)) as u16
}

fn require_sha256_v1(value: &str, code: &'static str) -> Result<(), MtgoContractErrorV1> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(MtgoContractErrorV1::new(code, value));
    }
    Ok(())
}

fn validate_identifier_v1(value: &str, code: &'static str) -> Result<(), MtgoContractErrorV1> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
    {
        return Err(MtgoContractErrorV1::new(code, value));
    }
    Ok(())
}

fn validate_utc_timestamp_v1(value: &str) -> Result<(), MtgoContractErrorV1> {
    if value.len() != 20
        || &value[4..5] != "-"
        || &value[7..8] != "-"
        || &value[10..11] != "T"
        || &value[13..14] != ":"
        || &value[16..17] != ":"
        || &value[19..20] != "Z"
        || !value.bytes().enumerate().all(|(index, byte)| {
            matches!(index, 4 | 7 | 10 | 13 | 16 | 19) || byte.is_ascii_digit()
        })
    {
        return Err(MtgoContractErrorV1::new(
            "duel_gesture_review_timestamp_invalid",
            value,
        ));
    }
    let parse = |range: std::ops::Range<usize>| value[range].parse::<u32>().ok();
    let year = parse(0..4);
    let month = parse(5..7);
    let day = parse(8..10);
    let max_day = match (year, month) {
        (Some(year), Some(2)) if year > 0 && is_leap_year_v1(year) => 29,
        (Some(year), Some(2)) if year > 0 => 28,
        (Some(year), Some(4 | 6 | 9 | 11)) if year > 0 => 30,
        (Some(year), Some(1 | 3 | 5 | 7 | 8 | 10 | 12)) if year > 0 => 31,
        _ => 0,
    };
    let valid = max_day > 0
        && matches!(day, Some(day) if day > 0 && day <= max_day)
        && matches!(parse(11..13), Some(0..=23))
        && matches!(parse(14..16), Some(0..=59))
        && matches!(parse(17..19), Some(0..=59));
    if !valid {
        return Err(MtgoContractErrorV1::new(
            "duel_gesture_review_timestamp_invalid",
            value,
        ));
    }
    Ok(())
}

fn is_leap_year_v1(year: u32) -> bool {
    year.is_multiple_of(4) && !year.is_multiple_of(100) || year.is_multiple_of(400)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        MtgoDuelGestureExpectedEffectV1, MtgoDuelGestureFrameBindingV1, MtgoDuelGesturePrimitiveV1,
        MtgoDuelGestureStageV1, MtgoDuelPrimaryActivationV1,
    };
    use mtg_kernel::rl::{CardStableRefV1, PlayerSeatV1};
    use mtg_kernel::state::Zone;

    fn hash_v1(value: usize) -> String {
        format!("{value:064x}")
    }

    fn card_v1(arena_id: u32) -> CardStableRefV1 {
        CardStableRefV1 {
            arena_id,
            card_db_id: arena_id as u16,
            owner: PlayerSeatV1::P0,
            controller: PlayerSeatV1::P0,
            zone: Zone::Battlefield,
            zone_change_count: 0,
        }
    }

    fn stages_v1(primitives: Vec<MtgoDuelGesturePrimitiveV1>) -> Vec<MtgoDuelGestureStageV1> {
        let count = primitives.len();
        primitives
            .into_iter()
            .enumerate()
            .map(|(index, primitive)| MtgoDuelGestureStageV1 {
                stage_index: index as u16,
                frame_binding: if index == 0 {
                    MtgoDuelGestureFrameBindingV1::SourceDecisionFrame
                } else {
                    MtgoDuelGestureFrameBindingV1::StrictlyNewerVisibleContinuation
                },
                primitive,
                expected_effect: if index + 1 == count {
                    MtgoDuelGestureExpectedEffectV1::CompleteSelectedSemantic
                } else {
                    MtgoDuelGestureExpectedEffectV1::ContinueSelectedSemantic
                },
            })
            .collect()
    }

    fn direct_v1(activation: MtgoDuelPrimaryActivationV1) -> Vec<MtgoDuelGestureStageV1> {
        stages_v1(vec![MtgoDuelGesturePrimitiveV1::ActivatePrimary {
            activation,
        }])
    }

    fn representatives_v1() -> Vec<(ActionSemanticV1, Vec<MtgoDuelGestureStageV1>)> {
        let first = card_v1(1);
        let second = card_v1(2);
        vec![
            (
                ActionSemanticV1::Pass {
                    actor: PlayerSeatV1::P0,
                },
                direct_v1(MtgoDuelPrimaryActivationV1::SingleLeftClick),
            ),
            (
                ActionSemanticV1::PlayLand {
                    actor: PlayerSeatV1::P0,
                    source: first.clone(),
                },
                direct_v1(MtgoDuelPrimaryActivationV1::DoubleLeftClick),
            ),
            (
                ActionSemanticV1::CastSpell {
                    actor: PlayerSeatV1::P0,
                    source: first.clone(),
                },
                stages_v1(vec![
                    MtgoDuelGesturePrimitiveV1::DragPrimaryToCalibratedPlayArea,
                ]),
            ),
            (
                ActionSemanticV1::ActivateManaAbility {
                    actor: PlayerSeatV1::P0,
                    source: first.clone(),
                    mana_choice: None,
                },
                direct_v1(MtgoDuelPrimaryActivationV1::SingleLeftClick),
            ),
            (
                ActionSemanticV1::ActivateAbility {
                    actor: PlayerSeatV1::P0,
                    source: first.clone(),
                    ability_index: 0,
                },
                stages_v1(vec![
                    MtgoDuelGesturePrimitiveV1::ActivatePrimary {
                        activation: MtgoDuelPrimaryActivationV1::RightClick,
                    },
                    MtgoDuelGesturePrimitiveV1::ActivateSemanticMenuChoice {
                        activation: MtgoDuelPrimaryActivationV1::SingleLeftClick,
                    },
                ]),
            ),
            (
                ActionSemanticV1::FinishTargetSelection {
                    actor: PlayerSeatV1::P0,
                    source: first.clone(),
                    selected_count: 1,
                },
                direct_v1(MtgoDuelPrimaryActivationV1::SingleLeftClick),
            ),
            (
                ActionSemanticV1::ChooseOptionalCostUse {
                    actor: PlayerSeatV1::P0,
                    use_cost: true,
                },
                direct_v1(MtgoDuelPrimaryActivationV1::SingleLeftClick),
            ),
            (
                ActionSemanticV1::FinishEffectSelection {
                    actor: PlayerSeatV1::P0,
                    source: first.clone(),
                    selected_count: 1,
                },
                direct_v1(MtgoDuelPrimaryActivationV1::SingleLeftClick),
            ),
            (
                ActionSemanticV1::Discard {
                    actor: PlayerSeatV1::P0,
                    cards: vec![first.clone(), second.clone()],
                },
                stages_v1(vec![
                    MtgoDuelGesturePrimitiveV1::SelectObject {
                        object: first.clone(),
                    },
                    MtgoDuelGesturePrimitiveV1::SelectObject {
                        object: second.clone(),
                    },
                    MtgoDuelGesturePrimitiveV1::Submit,
                ]),
            ),
            (
                ActionSemanticV1::DeclareAttackers {
                    actor: PlayerSeatV1::P0,
                    attackers: vec![first.clone()],
                },
                stages_v1(vec![
                    MtgoDuelGesturePrimitiveV1::SelectObject {
                        object: first.clone(),
                    },
                    MtgoDuelGesturePrimitiveV1::Submit,
                ]),
            ),
            (
                ActionSemanticV1::OrderTriggers {
                    actor: PlayerSeatV1::P0,
                    pending_sources: vec![first.clone(), second.clone()],
                    order: vec![1, 0],
                },
                stages_v1(vec![
                    MtgoDuelGesturePrimitiveV1::DragObjectToOrderSlot {
                        object: second,
                        slot_index: 0,
                    },
                    MtgoDuelGesturePrimitiveV1::DragObjectToOrderSlot {
                        object: first,
                        slot_index: 1,
                    },
                    MtgoDuelGesturePrimitiveV1::Submit,
                ]),
            ),
        ]
    }

    fn evaluation_v1() -> MtgoDuelGestureEvaluationV1 {
        let cases = representatives_v1()
            .into_iter()
            .enumerate()
            .map(|(index, (selected_semantic, stages))| {
                let expected_plan = MtgoDuelGesturePlanV1 {
                    schema_version: crate::MTGO_DUEL_GESTURE_PLAN_SCHEMA_V1,
                    profile_bound_resolution_commitment_sha256: hash_v1(100 + index),
                    decision_commitment_sha256: hash_v1(200 + index),
                    selection_commitment_sha256: hash_v1(300 + index),
                    frame_id: index as u64 + 1,
                    frame_sequence: index as u64 + 10,
                    selected_action_family: duel_action_family_v1(&selected_semantic),
                    stage_set_complete: true,
                    stages,
                };
                MtgoDuelGestureEvaluationCaseV1 {
                    case_id: format!("gesture-case-{index:02}"),
                    source_manifest_sha256: hash_v1(400 + index),
                    source_frame_sha256: hash_v1(500 + index),
                    selected_semantic,
                    predicted_plan: Some(expected_plan.clone()),
                    expected_plan,
                    annotator_receipt_sha256: hash_v1(600 + index),
                    reviewed_at_utc: "2026-08-11T12:00:00Z".to_owned(),
                    source_frame_visually_confirmed: true,
                    selected_semantic_visually_confirmed: true,
                    gesture_sequence_manually_demonstrated: true,
                    visible_postcondition_confirmed: true,
                }
            })
            .collect();
        MtgoDuelGestureEvaluationV1 {
            schema_version: MTGO_DUEL_GESTURE_EVALUATION_SCHEMA_V1,
            evaluation_id: "duel-gesture-evaluation-test-v1".to_owned(),
            perception_profile_admission_commitment_sha256: hash_v1(1),
            gesture_target_runtime_binary_sha256: hash_v1(2),
            gesture_target_assets_manifest_sha256: hash_v1(3),
            corpus_manifest_sha256: hash_v1(4),
            annotation_protocol_sha256: hash_v1(5),
            minimum_prediction_coverage_bps: 9_500,
            required_action_families: canonical_duel_gesture_action_families_v1(),
            cases,
        }
    }

    #[test]
    fn exact_all_family_evaluation_passes_without_production_authority() {
        let checked = evaluate_untrusted_duel_gesture_profile_v1(evaluation_v1()).unwrap();
        assert!(checked.outcome_v1().gate_passed);
        assert_eq!(checked.outcome_v1().family_outcomes.len(), 11);
        assert_eq!(checked.outcome_v1().total_exact_plan_count, 11);
        assert_eq!(checked.evaluation_commitment_sha256().len(), 64);
        assert!(!checked.safe_for_live_input());
        assert_eq!(
            admit_ratified_duel_gesture_profile_v1(checked)
                .err()
                .unwrap()
                .code(),
            "duel_gesture_evaluation_not_ratified"
        );
    }

    #[test]
    fn one_wrong_plan_or_one_family_abstention_fails_the_gate() {
        let mut wrong = evaluation_v1();
        wrong.cases[1].predicted_plan.as_mut().unwrap().stages[0].primitive =
            MtgoDuelGesturePrimitiveV1::ActivatePrimary {
                activation: MtgoDuelPrimaryActivationV1::SingleLeftClick,
            };
        let checked = evaluate_untrusted_duel_gesture_profile_v1(wrong).unwrap();
        assert!(!checked.outcome_v1().gate_passed);
        assert_eq!(checked.outcome_v1().total_exact_plan_count, 10);

        let mut abstained = evaluation_v1();
        abstained.cases[10].predicted_plan = None;
        let checked = evaluate_untrusted_duel_gesture_profile_v1(abstained).unwrap();
        assert!(!checked.outcome_v1().gate_passed);
        assert_eq!(
            checked.outcome_v1().family_outcomes[10].prediction_coverage_bps,
            0
        );
    }

    #[test]
    fn incomplete_review_and_invalid_semantic_shape_reject() {
        let mut incomplete = evaluation_v1();
        incomplete.cases[0].visible_postcondition_confirmed = false;
        assert_eq!(
            evaluate_untrusted_duel_gesture_profile_v1(incomplete)
                .err()
                .unwrap()
                .code(),
            "duel_gesture_case_review_incomplete"
        );

        let mut invalid = evaluation_v1();
        invalid.cases[8].expected_plan.stages.swap(0, 1);
        assert_eq!(
            evaluate_untrusted_duel_gesture_profile_v1(invalid)
                .err()
                .unwrap()
                .code(),
            "duel_gesture_stage_index_invalid"
        );
    }
}
