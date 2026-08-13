use crate::{
    player_visible_duel_action_family_v1, CheckedUntrustedMtgoActingPlayerDuelCalibrationCorpusV1,
    CheckedUntrustedMtgoPlayerVisibleDuelAnnotationSetV1, MtgoContractErrorV1,
    MtgoDuelActionFamilyV1, MtgoPlayerVisibleDuelDecisionInputV1,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;

pub const MTGO_PLAYER_VISIBLE_DIRECT_DUEL_PROJECTION_SCHEMA_V1: u32 = 1;

const PROJECTION_KIND_V1: &str = "mtgo_player_visible_direct_duel_projection_set_v1";
const INFORMATION_BOUNDARY_V1: &str = "seated_player_visible_ui_equivalent_projection_only_v1";
const PROJECTION_PROTOCOL_ID_V1: &str = "mtgo-player-visible-direct-duel-projection/v1";
const PROJECTION_PROTOCOL_SPEC_V1: &str = concat!(
    "raw_source_interface=producer_private;",
    "first_exported_value=mtgo_player_visible_duel_decision_input_v1;",
    "eligible_facts=seated_player_visible_ui_equivalent_only;",
    "hidden_zones_future_draws_rng_private_opponent_state=discarded;",
    "raw_protocol_paths_process_memory_and_transport_metadata=discarded;",
    "kernel_mtgo_card_database_and_internal_object_ids=discarded;",
    "object_identity=decision_local_dense_visible_ordinals_only;",
    "source_binding=exact_reviewed_ui_frame_corpus;",
    "qualification=offline_exact_agreement_only;",
    "authority=none"
);
const PROJECTION_PROTOCOL_DOMAIN_V1: &[u8] = b"mtgo-player-visible-direct-duel-protocol-v1";
const PROJECTION_SET_DOMAIN_V1: &[u8] = b"mtgo-player-visible-direct-duel-set-v1";
const PROJECTION_EVALUATION_DOMAIN_V1: &[u8] = b"mtgo-player-visible-direct-duel-evaluation-v1";
const MAX_PROJECTION_ENTRIES_V1: usize = 4_096;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoPlayerVisibleDirectDuelProjectionEntryV1 {
    pub case_id: String,
    pub source_manifest_sha256: String,
    pub source_canonical_bgra8_sha256: String,
    pub source_preview_png_sha256: String,
    pub prediction: Option<MtgoPlayerVisibleDuelDecisionInputV1>,
    pub player_visible_projection_complete: bool,
    pub producer_declares_nonvisible_fields_discarded_before_boundary: bool,
    pub producer_declares_internal_identifiers_discarded_before_boundary: bool,
    pub producer_declares_raw_source_retained_after_projection: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoPlayerVisibleDirectDuelProjectionSetV1 {
    pub schema_version: u32,
    pub projection_kind: String,
    pub projection_id: String,
    pub projection_protocol_sha256: String,
    pub information_boundary: String,
    pub corpus_manifest_sha256: String,
    pub corpus_commitment_sha256: String,
    pub declared_producer_binary_sha256: String,
    pub entries: Vec<MtgoPlayerVisibleDirectDuelProjectionEntryV1>,
    pub safe_for_live_semantic_evidence: bool,
    pub safe_for_model_scoring: bool,
    pub safe_for_input: bool,
}

/// Structurally checked output from an untrusted direct-source producer.
///
/// Raw client values cannot be represented here. The wrapper is move-only and
/// exposes only commitments, counts, and fixed false authority flags. Its
/// player-visible predictions are available only to this crate's offline
/// corpus evaluator.
///
/// ```compile_fail
/// use mtgo_blackbox_v1::CheckedUntrustedMtgoPlayerVisibleDirectDuelProjectionSetV1;
/// fn cannot_recover_source(value: &CheckedUntrustedMtgoPlayerVisibleDirectDuelProjectionSetV1) {
///     let _ = value.raw_source();
///     let _ = value.process_memory();
///     let _ = value.score_model();
///     let _ = value.input_command();
/// }
/// ```
pub struct CheckedUntrustedMtgoPlayerVisibleDirectDuelProjectionSetV1 {
    manifest: MtgoPlayerVisibleDirectDuelProjectionSetV1,
    canonical_manifest_sha256: String,
    projection_set_commitment_sha256: String,
}

impl CheckedUntrustedMtgoPlayerVisibleDirectDuelProjectionSetV1 {
    pub fn projection_protocol_sha256_v1(&self) -> &str {
        &self.manifest.projection_protocol_sha256
    }

    pub fn corpus_manifest_sha256_v1(&self) -> &str {
        &self.manifest.corpus_manifest_sha256
    }

    pub fn corpus_commitment_sha256_v1(&self) -> &str {
        &self.manifest.corpus_commitment_sha256
    }

    fn declared_producer_binary_sha256_v1(&self) -> &str {
        &self.manifest.declared_producer_binary_sha256
    }

    pub fn canonical_manifest_sha256_v1(&self) -> &str {
        &self.canonical_manifest_sha256
    }

    pub fn projection_set_commitment_sha256_v1(&self) -> &str {
        &self.projection_set_commitment_sha256
    }

    pub fn entry_count_v1(&self) -> usize {
        self.manifest.entries.len()
    }

    pub fn prediction_count_v1(&self) -> usize {
        self.manifest
            .entries
            .iter()
            .filter(|entry| entry.prediction.is_some())
            .count()
    }

    pub fn producer_execution_attested_v1(&self) -> bool {
        false
    }

    pub fn source_frame_association_attested_v1(&self) -> bool {
        false
    }

    pub fn safe_for_live_semantic_evidence_v1(&self) -> bool {
        false
    }

    pub fn safe_for_model_scoring_v1(&self) -> bool {
        false
    }

    pub fn safe_for_input_v1(&self) -> bool {
        false
    }

    fn entries_v1(&self) -> &[MtgoPlayerVisibleDirectDuelProjectionEntryV1] {
        &self.manifest.entries
    }
}

pub fn mtgo_player_visible_direct_duel_projection_protocol_sha256_v1() -> String {
    commitment_v1(
        PROJECTION_PROTOCOL_DOMAIN_V1,
        &[
            PROJECTION_PROTOCOL_ID_V1.as_bytes(),
            PROJECTION_PROTOCOL_SPEC_V1.as_bytes(),
        ],
    )
}

pub fn check_untrusted_player_visible_direct_duel_projection_set_v1(
    corpus: &CheckedUntrustedMtgoActingPlayerDuelCalibrationCorpusV1,
    manifest: MtgoPlayerVisibleDirectDuelProjectionSetV1,
) -> Result<CheckedUntrustedMtgoPlayerVisibleDirectDuelProjectionSetV1, MtgoContractErrorV1> {
    validate_identifier_v1(&manifest.projection_id)?;
    if manifest.schema_version != MTGO_PLAYER_VISIBLE_DIRECT_DUEL_PROJECTION_SCHEMA_V1
        || manifest.projection_kind != PROJECTION_KIND_V1
        || manifest.information_boundary != INFORMATION_BOUNDARY_V1
        || manifest.projection_protocol_sha256
            != mtgo_player_visible_direct_duel_projection_protocol_sha256_v1()
    {
        return Err(error_v1(
            "player_visible_direct_duel_projection_header",
            "schema, kind, information boundary, and projection protocol must be exact",
        ));
    }
    for (code, digest) in [
        (
            "player_visible_direct_duel_projection_corpus_manifest",
            manifest.corpus_manifest_sha256.as_str(),
        ),
        (
            "player_visible_direct_duel_projection_corpus_commitment",
            manifest.corpus_commitment_sha256.as_str(),
        ),
        (
            "player_visible_direct_duel_projection_producer",
            manifest.declared_producer_binary_sha256.as_str(),
        ),
    ] {
        validate_sha256_v1(code, digest)?;
    }
    if manifest.corpus_manifest_sha256 != corpus.canonical_manifest_sha256()
        || manifest.corpus_commitment_sha256 != corpus.corpus_commitment_sha256()
    {
        return Err(error_v1(
            "player_visible_direct_duel_projection_corpus_mismatch",
            "direct-source projections must bind the exact reviewed-frame corpus",
        ));
    }
    if manifest.safe_for_live_semantic_evidence
        || manifest.safe_for_model_scoring
        || manifest.safe_for_input
    {
        return Err(error_v1(
            "player_visible_direct_duel_projection_authority",
            "an untrusted direct-source projection grants no live evidence, scoring, or input authority",
        ));
    }
    if manifest.entries.is_empty()
        || manifest.entries.len() > MAX_PROJECTION_ENTRIES_V1
        || manifest.entries.len() != corpus.sample_count()
    {
        return Err(error_v1(
            "player_visible_direct_duel_projection_coverage",
            "the projection set must contain exactly one entry per reviewed corpus frame",
        ));
    }

    for (sample, entry) in corpus.manifest_v1().samples.iter().zip(&manifest.entries) {
        validate_identifier_v1(&entry.case_id)?;
        for (code, digest) in [
            (
                "player_visible_direct_duel_projection_source_manifest",
                entry.source_manifest_sha256.as_str(),
            ),
            (
                "player_visible_direct_duel_projection_source_pixels",
                entry.source_canonical_bgra8_sha256.as_str(),
            ),
            (
                "player_visible_direct_duel_projection_source_png",
                entry.source_preview_png_sha256.as_str(),
            ),
        ] {
            validate_sha256_v1(code, digest)?;
        }
        if entry.case_id != sample.sample_id
            || entry.source_manifest_sha256 != sample.source_manifest_sha256
            || entry.source_canonical_bgra8_sha256 != sample.source_canonical_bgra8_sha256
            || entry.source_preview_png_sha256 != sample.source_preview_png_sha256
        {
            return Err(error_v1(
                "player_visible_direct_duel_projection_source_mismatch",
                entry.case_id.clone(),
            ));
        }
        if !entry.producer_declares_nonvisible_fields_discarded_before_boundary
            || !entry.producer_declares_internal_identifiers_discarded_before_boundary
            || entry.producer_declares_raw_source_retained_after_projection
        {
            return Err(error_v1(
                "player_visible_direct_duel_projection_information_boundary",
                "non-visible fields, internal identifiers, and raw direct-source values must be absent before this boundary",
            ));
        }
        match &entry.prediction {
            Some(prediction) if entry.player_visible_projection_complete => {
                crate::validate_player_visible_expected_v2(prediction)?;
                crate::validate_player_visible_duel_decision_input_strict_v1(prediction)?;
            }
            None if !entry.player_visible_projection_complete => {}
            _ => {
                return Err(error_v1(
                    "player_visible_direct_duel_projection_completeness",
                    "a complete projection must contain one prediction and an abstention must declare incomplete",
                ))
            }
        }
    }

    let canonical = serde_json::to_vec(&manifest).map_err(|error| {
        error_v1(
            "player_visible_direct_duel_projection_serialization",
            error.to_string(),
        )
    })?;
    let canonical_manifest_sha256 = sha256_v1(&canonical);
    let projection_set_commitment_sha256 = commitment_v1(PROJECTION_SET_DOMAIN_V1, &[&canonical]);
    Ok(CheckedUntrustedMtgoPlayerVisibleDirectDuelProjectionSetV1 {
        manifest,
        canonical_manifest_sha256,
        projection_set_commitment_sha256,
    })
}

/// Exact offline comparison between a direct-source visible projection and
/// human labels from the same reviewed UI frames. The producer digest and
/// discard assertions remain caller declarations. A passing result therefore
/// measures only the submitted corpus output; it does not attest execution or
/// raw-input handling and grants no live authority.
pub struct CheckedUntrustedMtgoPlayerVisibleDirectDuelProjectionEvaluationV1 {
    evaluation_commitment_sha256: String,
    case_count: u32,
    prediction_count: u32,
    exact_visible_state_count: u32,
    exact_visible_legal_action_count: u32,
    exact_visible_decision_input_count: u32,
    prediction_coverage_bps: u16,
    annotated_action_families: Vec<MtgoDuelActionFamilyV1>,
    exact_complete_corpus_agreement: bool,
}

impl CheckedUntrustedMtgoPlayerVisibleDirectDuelProjectionEvaluationV1 {
    pub fn evaluation_commitment_sha256_v1(&self) -> &str {
        &self.evaluation_commitment_sha256
    }

    pub fn case_count_v1(&self) -> u32 {
        self.case_count
    }

    pub fn prediction_count_v1(&self) -> u32 {
        self.prediction_count
    }

    pub fn exact_visible_state_count_v1(&self) -> u32 {
        self.exact_visible_state_count
    }

    pub fn exact_visible_legal_action_count_v1(&self) -> u32 {
        self.exact_visible_legal_action_count
    }

    pub fn exact_visible_decision_input_count_v1(&self) -> u32 {
        self.exact_visible_decision_input_count
    }

    pub fn prediction_coverage_bps_v1(&self) -> u16 {
        self.prediction_coverage_bps
    }

    pub fn annotated_action_families_v1(&self) -> &[MtgoDuelActionFamilyV1] {
        &self.annotated_action_families
    }

    pub fn exact_complete_corpus_agreement_v1(&self) -> bool {
        self.exact_complete_corpus_agreement
    }

    pub fn safe_for_live_semantic_evidence_v1(&self) -> bool {
        false
    }

    pub fn safe_for_model_scoring_v1(&self) -> bool {
        false
    }

    pub fn safe_for_input_v1(&self) -> bool {
        false
    }
}

pub fn evaluate_untrusted_player_visible_direct_duel_projection_v1(
    corpus: &CheckedUntrustedMtgoActingPlayerDuelCalibrationCorpusV1,
    annotations: &CheckedUntrustedMtgoPlayerVisibleDuelAnnotationSetV1,
    projections: &CheckedUntrustedMtgoPlayerVisibleDirectDuelProjectionSetV1,
    evaluator_binary_sha256: &str,
) -> Result<CheckedUntrustedMtgoPlayerVisibleDirectDuelProjectionEvaluationV1, MtgoContractErrorV1>
{
    validate_sha256_v1(
        "player_visible_direct_duel_projection_evaluator",
        evaluator_binary_sha256,
    )?;
    if annotations.corpus_manifest_sha256() != corpus.canonical_manifest_sha256()
        || annotations.corpus_commitment_sha256() != corpus.corpus_commitment_sha256()
        || projections.corpus_manifest_sha256_v1() != corpus.canonical_manifest_sha256()
        || projections.corpus_commitment_sha256_v1() != corpus.corpus_commitment_sha256()
        || annotations.entry_count() != projections.entry_count_v1()
    {
        return Err(error_v1(
            "player_visible_direct_duel_projection_evaluation_source",
            "corpus, annotations, and direct projections must bind the same complete reviewed frame set",
        ));
    }

    let mut hasher = Sha256::new();
    hasher.update(PROJECTION_EVALUATION_DOMAIN_V1);
    for part in [
        corpus.canonical_manifest_sha256().as_bytes(),
        corpus.corpus_commitment_sha256().as_bytes(),
        annotations.annotation_set_commitment_sha256().as_bytes(),
        projections.projection_set_commitment_sha256_v1().as_bytes(),
        projections.declared_producer_binary_sha256_v1().as_bytes(),
        evaluator_binary_sha256.as_bytes(),
    ] {
        hash_part_v1(&mut hasher, part);
    }

    let mut prediction_count = 0_u32;
    let mut exact_visible_state_count = 0_u32;
    let mut exact_visible_legal_action_count = 0_u32;
    let mut exact_visible_decision_input_count = 0_u32;
    let mut action_families = HashSet::new();
    for (annotation, projection) in annotations
        .manifest_v1()
        .entries
        .iter()
        .zip(projections.entries_v1())
    {
        if annotation.case_id != projection.case_id
            || annotation.source_manifest_sha256 != projection.source_manifest_sha256
            || annotation.source_canonical_bgra8_sha256 != projection.source_canonical_bgra8_sha256
            || annotation.source_preview_png_sha256 != projection.source_preview_png_sha256
        {
            return Err(error_v1(
                "player_visible_direct_duel_projection_evaluation_case",
                "annotation and direct projection changed case order or exact UI source",
            ));
        }
        for action in &annotation.expected.ordered_legal_actions {
            action_families.insert(player_visible_duel_action_family_v1(action));
        }
        let (state_exact, actions_exact, input_exact, prediction_commitment) =
            if let Some(prediction) = &projection.prediction {
                prediction_count = prediction_count.checked_add(1).ok_or_else(|| {
                    error_v1(
                        "player_visible_direct_duel_projection_evaluation_count",
                        "prediction count overflow",
                    )
                })?;
                let state_exact = prediction.current_state == annotation.expected.current_state;
                let actions_exact =
                    prediction.ordered_legal_actions == annotation.expected.ordered_legal_actions;
                let input_exact = state_exact && actions_exact;
                exact_visible_state_count += u32::from(state_exact);
                exact_visible_legal_action_count += u32::from(actions_exact);
                exact_visible_decision_input_count += u32::from(input_exact);
                (
                    state_exact,
                    actions_exact,
                    input_exact,
                    prediction.commitment_sha256_v1()?,
                )
            } else {
                (false, false, false, "abstained".to_owned())
            };
        for part in [
            projection.case_id.as_bytes(),
            projection.source_manifest_sha256.as_bytes(),
            projection.source_canonical_bgra8_sha256.as_bytes(),
            annotation.expected.commitment_sha256_v1()?.as_bytes(),
            prediction_commitment.as_bytes(),
            &[
                u8::from(state_exact),
                u8::from(actions_exact),
                u8::from(input_exact),
            ],
        ] {
            hash_part_v1(&mut hasher, part);
        }
    }

    let case_count = u32::try_from(projections.entry_count_v1()).map_err(|_| {
        error_v1(
            "player_visible_direct_duel_projection_evaluation_count",
            "case count does not fit u32",
        )
    })?;
    let prediction_coverage_bps = ratio_bps_v1(prediction_count, case_count);
    let mut annotated_action_families = action_families.into_iter().collect::<Vec<_>>();
    annotated_action_families.sort_by_key(|family| action_family_sort_key_v1(*family));
    let exact_complete_corpus_agreement = prediction_count == case_count
        && exact_visible_state_count == case_count
        && exact_visible_legal_action_count == case_count
        && exact_visible_decision_input_count == case_count;
    Ok(
        CheckedUntrustedMtgoPlayerVisibleDirectDuelProjectionEvaluationV1 {
            evaluation_commitment_sha256: format!("{:x}", hasher.finalize()),
            case_count,
            prediction_count,
            exact_visible_state_count,
            exact_visible_legal_action_count,
            exact_visible_decision_input_count,
            prediction_coverage_bps,
            annotated_action_families,
            exact_complete_corpus_agreement,
        },
    )
}

fn action_family_sort_key_v1(family: MtgoDuelActionFamilyV1) -> u8 {
    use MtgoDuelActionFamilyV1 as F;
    match family {
        F::PriorityPass => 0,
        F::PlayLand => 1,
        F::CastOrPlotSpell => 2,
        F::ManaAbility => 3,
        F::NonManaAbility => 4,
        F::TargetChoice => 5,
        F::CostOrModeChoice => 6,
        F::EffectChoice => 7,
        F::Discard => 8,
        F::CombatChoice => 9,
        F::TriggerOrdering => 10,
    }
}

fn ratio_bps_v1(numerator: u32, denominator: u32) -> u16 {
    if denominator == 0 {
        return 0;
    }
    let ratio = (u64::from(numerator) * 10_000) / u64::from(denominator);
    u16::try_from(ratio).unwrap_or(10_000)
}

fn validate_identifier_v1(value: &str) -> Result<(), MtgoContractErrorV1> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err(error_v1(
            "player_visible_direct_duel_projection_identifier",
            "identifier must be bounded canonical ASCII",
        ));
    }
    Ok(())
}

fn validate_sha256_v1(code: &'static str, value: &str) -> Result<(), MtgoContractErrorV1> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(error_v1(code, "expected lowercase SHA-256"));
    }
    Ok(())
}

fn sha256_v1(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn commitment_v1(domain: &[u8], parts: &[&[u8]]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(domain);
    for part in parts {
        hash_part_v1(&mut hasher, part);
    }
    format!("{:x}", hasher.finalize())
}

fn hash_part_v1(hasher: &mut Sha256, part: &[u8]) {
    hasher.update((part.len() as u64).to_be_bytes());
    hasher.update(part);
}

fn error_v1(code: &'static str, detail: impl Into<String>) -> MtgoContractErrorV1 {
    MtgoContractErrorV1::new(code, detail)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        build_checked_untrusted_acting_player_duel_calibration_corpus_v1,
        check_untrusted_player_visible_duel_annotation_set_v1,
        checked_untrusted_dxgi_artifact_for_test_v1, MtgoDxgiCaptureRoleV2,
        MtgoPlayerRelativeRoleV1, MtgoPlayerVisibleCombatStateV1, MtgoPlayerVisibleDuelActionV1,
        MtgoPlayerVisibleDuelAnnotationEntryV1, MtgoPlayerVisibleDuelAnnotationSetV1,
        MtgoPlayerVisibleDuelStateV1, ZoneIndependentStepV1,
        MTGO_PLAYER_VISIBLE_DUEL_ANNOTATION_SET_SCHEMA_V1,
    };

    fn expected_v1() -> MtgoPlayerVisibleDuelDecisionInputV1 {
        MtgoPlayerVisibleDuelDecisionInputV1 {
            current_state: MtgoPlayerVisibleDuelStateV1 {
                acting_player: MtgoPlayerRelativeRoleV1::SeatedPlayer,
                turn: 1,
                phase: ZoneIndependentStepV1::Main1,
                active_player: MtgoPlayerRelativeRoleV1::SeatedPlayer,
                priority_player: MtgoPlayerRelativeRoleV1::SeatedPlayer,
                initiative: None,
                life_totals: [20, 20],
                mana_pools: [[0; 6]; 2],
                hand_counts: [0, 0],
                library_counts: [53, 53],
                battlefield: [Vec::new(), Vec::new()],
                graveyards: [Vec::new(), Vec::new()],
                exile: Vec::new(),
                stack: Vec::new(),
                combat: MtgoPlayerVisibleCombatStateV1 {
                    attackers_declared: false,
                    blockers_declared: false,
                    ordered_attackers: Vec::new(),
                    blocker_assignments: Vec::new(),
                },
                visible_object_relations: Vec::new(),
                own_hand: Vec::new(),
                known_library_cards: [Vec::new(), Vec::new()],
                known_hand_cards: [Vec::new(), Vec::new()],
            },
            ordered_legal_actions: vec![MtgoPlayerVisibleDuelActionV1::Pass {
                actor: MtgoPlayerRelativeRoleV1::SeatedPlayer,
            }],
        }
    }

    fn fixture_v1() -> (
        CheckedUntrustedMtgoActingPlayerDuelCalibrationCorpusV1,
        CheckedUntrustedMtgoPlayerVisibleDuelAnnotationSetV1,
        MtgoPlayerVisibleDirectDuelProjectionSetV1,
    ) {
        let source =
            checked_untrusted_dxgi_artifact_for_test_v1(MtgoDxgiCaptureRoleV2::ActingPlayerDuel);
        let corpus = build_checked_untrusted_acting_player_duel_calibration_corpus_v1(
            "direct-projection-test-corpus-v1",
            &[&source],
        )
        .unwrap();
        let sample = &corpus.manifest_v1().samples[0];
        let expected = expected_v1();
        let annotations = check_untrusted_player_visible_duel_annotation_set_v1(
            &corpus,
            MtgoPlayerVisibleDuelAnnotationSetV1 {
                schema_version: MTGO_PLAYER_VISIBLE_DUEL_ANNOTATION_SET_SCHEMA_V1,
                annotation_kind: "mtgo_player_visible_duel_annotation_set_v1".to_owned(),
                annotation_id: "direct-projection-test-labels-v1".to_owned(),
                annotation_protocol_sha256:
                    crate::mtgo_player_visible_duel_annotation_protocol_sha256_v1(),
                information_boundary: "seated_player_visible_source_frame_pixels_only_v1"
                    .to_owned(),
                corpus_manifest_sha256: corpus.canonical_manifest_sha256().to_owned(),
                corpus_commitment_sha256: corpus.corpus_commitment_sha256().to_owned(),
                reviewer_alias_sha256: "a".repeat(64),
                reviewed_at_unix_millis: 1_786_000_000_000,
                entries: vec![MtgoPlayerVisibleDuelAnnotationEntryV1 {
                    case_id: sample.sample_id.clone(),
                    source_manifest_sha256: sample.source_manifest_sha256.clone(),
                    source_canonical_bgra8_sha256: sample.source_canonical_bgra8_sha256.clone(),
                    source_preview_png_sha256: sample.source_preview_png_sha256.clone(),
                    expected: expected.clone(),
                    source_frame_visually_reviewed: true,
                    visible_state_reviewed: true,
                    visible_legal_actions_reviewed: true,
                    exact_source_frame_only_used: true,
                }],
                safe_for_live_semantic_evidence: false,
                safe_for_model_scoring: false,
                safe_for_input: false,
            },
        )
        .unwrap();
        let projections = MtgoPlayerVisibleDirectDuelProjectionSetV1 {
            schema_version: MTGO_PLAYER_VISIBLE_DIRECT_DUEL_PROJECTION_SCHEMA_V1,
            projection_kind: PROJECTION_KIND_V1.to_owned(),
            projection_id: "direct-projection-test-v1".to_owned(),
            projection_protocol_sha256:
                mtgo_player_visible_direct_duel_projection_protocol_sha256_v1(),
            information_boundary: INFORMATION_BOUNDARY_V1.to_owned(),
            corpus_manifest_sha256: corpus.canonical_manifest_sha256().to_owned(),
            corpus_commitment_sha256: corpus.corpus_commitment_sha256().to_owned(),
            declared_producer_binary_sha256: "b".repeat(64),
            entries: vec![MtgoPlayerVisibleDirectDuelProjectionEntryV1 {
                case_id: sample.sample_id.clone(),
                source_manifest_sha256: sample.source_manifest_sha256.clone(),
                source_canonical_bgra8_sha256: sample.source_canonical_bgra8_sha256.clone(),
                source_preview_png_sha256: sample.source_preview_png_sha256.clone(),
                prediction: Some(expected),
                player_visible_projection_complete: true,
                producer_declares_nonvisible_fields_discarded_before_boundary: true,
                producer_declares_internal_identifiers_discarded_before_boundary: true,
                producer_declares_raw_source_retained_after_projection: false,
            }],
            safe_for_live_semantic_evidence: false,
            safe_for_model_scoring: false,
            safe_for_input: false,
        };
        (corpus, annotations, projections)
    }

    #[test]
    fn exact_direct_projection_is_measurable_but_non_authorizing() {
        let (corpus, annotations, manifest) = fixture_v1();
        let checked =
            check_untrusted_player_visible_direct_duel_projection_set_v1(&corpus, manifest)
                .unwrap();
        let evaluation = evaluate_untrusted_player_visible_direct_duel_projection_v1(
            &corpus,
            &annotations,
            &checked,
            &"c".repeat(64),
        )
        .unwrap();
        assert_eq!(checked.prediction_count_v1(), 1);
        assert!(!checked.producer_execution_attested_v1());
        assert!(!checked.source_frame_association_attested_v1());
        assert!(evaluation.exact_complete_corpus_agreement_v1());
        assert_eq!(evaluation.prediction_coverage_bps_v1(), 10_000);
        assert!(!checked.safe_for_live_semantic_evidence_v1());
        assert!(!evaluation.safe_for_model_scoring_v1());
        assert!(!evaluation.safe_for_input_v1());
    }

    #[test]
    fn hidden_fields_and_authority_claims_reject() {
        let (_, _, manifest) = fixture_v1();
        let mut json = serde_json::to_value(manifest).unwrap();
        json.as_object_mut().unwrap().insert(
            "opponent_hidden_hand".to_owned(),
            serde_json::json!(["Island"]),
        );
        assert!(
            serde_json::from_value::<MtgoPlayerVisibleDirectDuelProjectionSetV1>(json).is_err()
        );

        let (corpus, _, mut authority) = fixture_v1();
        authority.safe_for_model_scoring = true;
        let error = match check_untrusted_player_visible_direct_duel_projection_set_v1(
            &corpus, authority,
        ) {
            Ok(_) => panic!("caller-declared live authority must reject"),
            Err(error) => error,
        };
        assert_eq!(
            error.code(),
            "player_visible_direct_duel_projection_authority"
        );
    }

    #[test]
    fn raw_retention_and_source_substitution_reject() {
        let (corpus, _, mut retained) = fixture_v1();
        retained.entries[0].producer_declares_raw_source_retained_after_projection = true;
        let error =
            match check_untrusted_player_visible_direct_duel_projection_set_v1(&corpus, retained) {
                Ok(_) => panic!("raw source retention must reject"),
                Err(error) => error,
            };
        assert_eq!(
            error.code(),
            "player_visible_direct_duel_projection_information_boundary"
        );

        let (corpus, _, mut substituted) = fixture_v1();
        substituted.entries[0].source_canonical_bgra8_sha256 = "f".repeat(64);
        let error = match check_untrusted_player_visible_direct_duel_projection_set_v1(
            &corpus,
            substituted,
        ) {
            Ok(_) => panic!("reviewed UI source substitution must reject"),
            Err(error) => error,
        };
        assert_eq!(
            error.code(),
            "player_visible_direct_duel_projection_source_mismatch"
        );
    }

    #[test]
    fn abstention_and_visible_mismatch_fail_exact_agreement() {
        let (corpus, annotations, mut abstained) = fixture_v1();
        abstained.entries[0].prediction = None;
        abstained.entries[0].player_visible_projection_complete = false;
        let checked =
            check_untrusted_player_visible_direct_duel_projection_set_v1(&corpus, abstained)
                .unwrap();
        let evaluation = evaluate_untrusted_player_visible_direct_duel_projection_v1(
            &corpus,
            &annotations,
            &checked,
            &"c".repeat(64),
        )
        .unwrap();
        assert!(!evaluation.exact_complete_corpus_agreement_v1());
        assert_eq!(evaluation.prediction_coverage_bps_v1(), 0);

        let (corpus, annotations, mut mismatch) = fixture_v1();
        mismatch.entries[0]
            .prediction
            .as_mut()
            .unwrap()
            .current_state
            .life_totals[0] += 1;
        let checked =
            check_untrusted_player_visible_direct_duel_projection_set_v1(&corpus, mismatch)
                .unwrap();
        let evaluation = evaluate_untrusted_player_visible_direct_duel_projection_v1(
            &corpus,
            &annotations,
            &checked,
            &"c".repeat(64),
        )
        .unwrap();
        assert_eq!(evaluation.prediction_count_v1(), 1);
        assert_eq!(evaluation.exact_visible_state_count_v1(), 0);
        assert!(!evaluation.exact_complete_corpus_agreement_v1());
    }
}
