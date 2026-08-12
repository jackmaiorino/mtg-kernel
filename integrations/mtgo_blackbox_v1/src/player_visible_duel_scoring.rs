use crate::{
    build_player_visible_duel_decision_input_v1, ActionSemanticV1,
    AdmittedMtgoDuelPerceptionProfileV1, CheckedUntrustedMtgoDxgiObservedDecisionCandidateV1,
    MtgoContractErrorV1, MtgoPlayerVisibleDuelActionV1, MtgoPlayerVisibleDuelDecisionInputV1,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const MTGO_PLAYER_VISIBLE_DUEL_SCORING_SCHEMA_V1: u32 = 1;

const PLAYER_VISIBLE_DUEL_SELECTION_DOMAIN_V1: &[u8] =
    b"mtgo-player-visible-profile-bound-duel-selection-v1";
const PLAYER_VISIBLE_DUEL_PRIVATE_SOURCE_BINDING_DOMAIN_V1: &[u8] =
    b"mtgo-private-player-visible-profile-bound-duel-source-binding-v1";

/// Coordinate-free model response for one exact player-visible duel input.
///
/// This transport contains no aggregate kernel observation, kernel object or
/// card identifiers, source pixels, coordinates, authorization, or input
/// primitive. The action order is the exact visible order in the request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoPlayerVisibleDuelScoreResponseV1 {
    pub schema_version: u32,
    pub ordered_action_logits_f32_bits: Vec<u32>,
    pub value_f32_bits: u32,
}

/// The only gameplay-scoring payload eligible to cross from the MTGO adapter
/// into a model implementation. A conforming scorer cannot request the source
/// `ObservationV5`, raw `ActionSemanticV1`, capture, or transport metadata.
pub trait MtgoPlayerVisibleDuelScorerV1 {
    fn score_player_visible_duel_v1(
        &mut self,
        model_input: &MtgoPlayerVisibleDuelDecisionInputV1,
    ) -> Result<MtgoPlayerVisibleDuelScoreResponseV1, String>;
}

/// A checked, source-bound selection from the player-visible-only scorer.
///
/// The source candidate and corresponding kernel semantic remain private so a
/// later resolver can ground the selected visible index against the exact
/// current-frame control. Public access is limited to player-visible values
/// and finite model-output bits. Integrity commitments remain private adapter
/// metadata. This type has no input authority.
///
/// ```compile_fail
/// use mtgo_blackbox_v1::CheckedUntrustedMtgoPlayerVisibleProfileBoundDuelModelSelectionV1;
/// fn cannot_read_hidden_state(value: &CheckedUntrustedMtgoPlayerVisibleProfileBoundDuelModelSelectionV1) {
///     let _ = value.observation();
///     let _ = value.selected_semantic();
///     let _ = value.input_command();
/// }
/// ```
pub struct CheckedUntrustedMtgoPlayerVisibleProfileBoundDuelModelSelectionV1 {
    candidate: CheckedUntrustedMtgoDxgiObservedDecisionCandidateV1,
    response: MtgoPlayerVisibleDuelScoreResponseV1,
    selected_index: usize,
    selected_action: MtgoPlayerVisibleDuelActionV1,
    selected_semantic: ActionSemanticV1,
    deployment_commitment_sha256: String,
    perception_profile_admission_commitment_sha256: String,
    selection_commitment_sha256: String,
    private_source_binding_commitment_sha256: String,
}

impl CheckedUntrustedMtgoPlayerVisibleProfileBoundDuelModelSelectionV1 {
    pub fn selected_index_v1(&self) -> usize {
        self.selected_index
    }

    pub fn selected_action_v1(&self) -> &MtgoPlayerVisibleDuelActionV1 {
        &self.selected_action
    }

    pub fn selected_logit_f32_bits_v1(&self) -> u32 {
        self.response.ordered_action_logits_f32_bits[self.selected_index]
    }

    pub fn value_f32_bits_v1(&self) -> u32 {
        self.response.value_f32_bits
    }

    pub(crate) fn selection_commitment_sha256_v1(&self) -> &str {
        &self.selection_commitment_sha256
    }

    pub fn safe_for_live_input_v1(&self) -> bool {
        false
    }

    pub fn permits_event_entry_v1(&self) -> bool {
        false
    }

    pub fn permits_spending_v1(&self) -> bool {
        false
    }

    pub(crate) fn source_candidate_commitment_sha256_v1(&self) -> &str {
        self.candidate.candidate_commitment_sha256()
    }

    pub(crate) fn perception_profile_admission_commitment_sha256_v1(&self) -> &str {
        &self.perception_profile_admission_commitment_sha256
    }

    pub(crate) fn deployment_commitment_sha256_v1(&self) -> &str {
        &self.deployment_commitment_sha256
    }

    pub(crate) fn selected_semantic_v1(&self) -> &ActionSemanticV1 {
        &self.selected_semantic
    }

    pub(crate) fn validated_decision_v1(&self) -> &crate::ValidatedMtgoObservedDecisionV1 {
        self.candidate.validated_decision_v1()
    }

    pub(crate) fn private_source_binding_commitment_sha256_v1(&self) -> &str {
        &self.private_source_binding_commitment_sha256
    }
}

/// Scores one exact source-bound decision through the player-visible-only
/// interface and deterministically selects the greatest finite logit. Equal
/// logits select the earliest action in the exact visible order.
pub fn score_and_select_player_visible_profile_bound_duel_candidate_v1<
    S: MtgoPlayerVisibleDuelScorerV1,
>(
    candidate: CheckedUntrustedMtgoDxgiObservedDecisionCandidateV1,
    profile: &AdmittedMtgoDuelPerceptionProfileV1,
    deployment_commitment_sha256: &str,
    scorer: &mut S,
) -> Result<CheckedUntrustedMtgoPlayerVisibleProfileBoundDuelModelSelectionV1, MtgoContractErrorV1>
{
    if candidate.perception_profile_commitment_sha256()
        != profile.perception_profile_commitment_sha256()
    {
        return Err(error_v1(
            "player_visible_duel_scoring_profile_mismatch",
            "source candidate and admitted perception profile differ",
        ));
    }
    require_sha256_v1(
        deployment_commitment_sha256,
        "player_visible_duel_scoring_deployment_hash_invalid",
    )?;
    let model_input =
        build_player_visible_duel_decision_input_v1(candidate.validated_decision_v1())?;
    let model_input_commitment_sha256 = model_input.commitment_sha256_v1()?;
    let response = scorer
        .score_player_visible_duel_v1(&model_input)
        .map_err(|error| error_v1("player_visible_duel_scorer_failed", error))?;
    if response.schema_version != MTGO_PLAYER_VISIBLE_DUEL_SCORING_SCHEMA_V1
        || response.ordered_action_logits_f32_bits.len() != model_input.ordered_legal_actions.len()
        || response.ordered_action_logits_f32_bits.is_empty()
    {
        return Err(error_v1(
            "player_visible_duel_score_response_mismatch",
            "response changed the exact visible action count",
        ));
    }
    let logits = response
        .ordered_action_logits_f32_bits
        .iter()
        .copied()
        .map(f32::from_bits)
        .collect::<Vec<_>>();
    if logits.iter().any(|value| !value.is_finite())
        || !f32::from_bits(response.value_f32_bits).is_finite()
    {
        return Err(error_v1(
            "player_visible_duel_score_nonfinite",
            "every policy logit and the value must be finite",
        ));
    }
    let mut selected_index = 0_usize;
    for index in 1..logits.len() {
        if logits[index].total_cmp(&logits[selected_index]).is_gt() {
            selected_index = index;
        }
    }
    let selected_action = model_input.ordered_legal_actions[selected_index].clone();
    let selected_semantic =
        candidate.validated_decision_v1().legal_actions()[selected_index].clone();
    let response_json = serde_json::to_vec(&response).map_err(|error| {
        error_v1(
            "player_visible_duel_response_serialization_failed",
            error.to_string(),
        )
    })?;
    let selected_action_json = serde_json::to_vec(&selected_action).map_err(|error| {
        error_v1(
            "player_visible_duel_action_serialization_failed",
            error.to_string(),
        )
    })?;
    let selection_commitment_sha256 = commitment_v1(
        PLAYER_VISIBLE_DUEL_SELECTION_DOMAIN_V1,
        &[
            model_input_commitment_sha256.as_bytes(),
            &response_json,
            &(selected_index as u64).to_be_bytes(),
            &selected_action_json,
            b"player_visible_only_checked_selection_no_input_or_event_entry",
        ],
    );
    let private_source_binding_commitment_sha256 = commitment_v1(
        PLAYER_VISIBLE_DUEL_PRIVATE_SOURCE_BINDING_DOMAIN_V1,
        &[
            candidate.candidate_commitment_sha256().as_bytes(),
            profile.admission_commitment_sha256().as_bytes(),
            deployment_commitment_sha256.as_bytes(),
            selection_commitment_sha256.as_bytes(),
            b"private_adapter_lineage_never_model_input_or_public_diagnostic",
        ],
    );
    Ok(
        CheckedUntrustedMtgoPlayerVisibleProfileBoundDuelModelSelectionV1 {
            candidate,
            response,
            selected_index,
            selected_action,
            selected_semantic,
            deployment_commitment_sha256: deployment_commitment_sha256.to_owned(),
            perception_profile_admission_commitment_sha256: profile
                .admission_commitment_sha256()
                .to_owned(),
            selection_commitment_sha256,
            private_source_binding_commitment_sha256,
        },
    )
}

fn commitment_v1(domain: &[u8], parts: &[&[u8]]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(domain);
    for part in parts {
        hasher.update((part.len() as u64).to_le_bytes());
        hasher.update(part);
    }
    format!("{:x}", hasher.finalize())
}

fn require_sha256_v1(value: &str, code: &'static str) -> Result<(), MtgoContractErrorV1> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(error_v1(code, value));
    }
    Ok(())
}

fn error_v1(code: &'static str, detail: impl Into<String>) -> MtgoContractErrorV1 {
    MtgoContractErrorV1::new(code, detail)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        check_untrusted_duel_perception_runtime_profile_v1,
        checked_untrusted_dxgi_decision_candidate_for_test_v1,
        duel_perception_profile_admitted_for_test_v1,
        duel_perception_runtime_profile_payload_for_test_v1,
        resolve_player_visible_profile_bound_selected_visible_control_v1,
        MtgoVisibleActionControlCandidateV1, MtgoVisibleActionControlSetV1,
        MtgoVisibleControlKindV1, MTGO_VISIBLE_ACTION_CONTROL_SET_SCHEMA_V1,
    };

    fn digest(character: char) -> String {
        std::iter::repeat_n(character, 64).collect()
    }

    struct RecordingScorerV1 {
        selected_index: usize,
        wrong_action_count: bool,
        nonfinite: bool,
        equal_logits: bool,
        observed_json: Option<String>,
    }

    impl MtgoPlayerVisibleDuelScorerV1 for RecordingScorerV1 {
        fn score_player_visible_duel_v1(
            &mut self,
            model_input: &MtgoPlayerVisibleDuelDecisionInputV1,
        ) -> Result<MtgoPlayerVisibleDuelScoreResponseV1, String> {
            self.observed_json = Some(serde_json::to_string(model_input).unwrap());
            let mut logits = vec![0.0_f32.to_bits(); model_input.ordered_legal_actions.len()];
            logits[self.selected_index] = if self.nonfinite {
                f32::NAN.to_bits()
            } else {
                1.0_f32.to_bits()
            };
            if self.equal_logits {
                logits.fill(1.0_f32.to_bits());
            }
            if self.wrong_action_count {
                logits.pop();
            }
            Ok(MtgoPlayerVisibleDuelScoreResponseV1 {
                schema_version: MTGO_PLAYER_VISIBLE_DUEL_SCORING_SCHEMA_V1,
                ordered_action_logits_f32_bits: logits,
                value_f32_bits: 0.25_f32.to_bits(),
            })
        }
    }

    #[test]
    fn scorer_receives_only_visible_payload_and_selects_exact_order() {
        let payload = duel_perception_runtime_profile_payload_for_test_v1();
        let checked = check_untrusted_duel_perception_runtime_profile_v1(payload.clone()).unwrap();
        let candidate = checked_untrusted_dxgi_decision_candidate_for_test_v1(&checked);
        let admitted = duel_perception_profile_admitted_for_test_v1(
            check_untrusted_duel_perception_runtime_profile_v1(payload).unwrap(),
        );
        let mut scorer = RecordingScorerV1 {
            selected_index: 1,
            wrong_action_count: false,
            nonfinite: false,
            equal_logits: false,
            observed_json: None,
        };
        let selection = score_and_select_player_visible_profile_bound_duel_candidate_v1(
            candidate,
            &admitted,
            &digest('a'),
            &mut scorer,
        )
        .unwrap();

        assert_eq!(selection.selected_index_v1(), 1);
        assert!(matches!(
            selection.selected_action_v1(),
            MtgoPlayerVisibleDuelActionV1::PlayLand { .. }
        ));
        assert_eq!(selection.selected_logit_f32_bits_v1(), 1.0_f32.to_bits());
        assert_eq!(selection.value_f32_bits_v1(), 0.25_f32.to_bits());
        assert!(!selection.safe_for_live_input_v1());
        assert!(!selection.permits_event_entry_v1());
        assert!(!selection.permits_spending_v1());

        let observed = scorer.observed_json.unwrap();
        for forbidden in [
            "arena_id",
            "card_db_id",
            "zone_change_count",
            "adapter_object_id",
            "engine_context",
            "surface_context",
            "policy_surface_context",
            "frame_id",
            "decision_commitment",
            "source_manifest",
        ] {
            assert!(
                !observed.contains(forbidden),
                "forbidden field leaked: {forbidden}"
            );
        }
    }

    #[test]
    fn response_shape_and_nonfinite_values_reject() {
        for mutation in 0..2 {
            let payload = duel_perception_runtime_profile_payload_for_test_v1();
            let checked =
                check_untrusted_duel_perception_runtime_profile_v1(payload.clone()).unwrap();
            let candidate = checked_untrusted_dxgi_decision_candidate_for_test_v1(&checked);
            let admitted = duel_perception_profile_admitted_for_test_v1(
                check_untrusted_duel_perception_runtime_profile_v1(payload).unwrap(),
            );
            let mut scorer = RecordingScorerV1 {
                selected_index: 0,
                wrong_action_count: mutation == 0,
                nonfinite: mutation == 1,
                equal_logits: false,
                observed_json: None,
            };
            assert!(
                score_and_select_player_visible_profile_bound_duel_candidate_v1(
                    candidate,
                    &admitted,
                    &digest('a'),
                    &mut scorer,
                )
                .is_err()
            );
        }
    }

    #[test]
    fn selection_resolves_the_same_current_frame_control() {
        let payload = duel_perception_runtime_profile_payload_for_test_v1();
        let checked = check_untrusted_duel_perception_runtime_profile_v1(payload.clone()).unwrap();
        let candidate = checked_untrusted_dxgi_decision_candidate_for_test_v1(&checked);
        let decision = candidate.validated_decision_v1();
        let control_set = MtgoVisibleActionControlSetV1 {
            schema_version: MTGO_VISIBLE_ACTION_CONTROL_SET_SCHEMA_V1,
            decision_commitment_sha256: decision.decision_commitment_sha256().to_owned(),
            frame_id: decision.frame_id(),
            frame_sequence: decision.frame_sequence(),
            prompt_frame_region_evidence_id: 30,
            prompt_reconciled: true,
            candidate_set_complete: true,
            controls: vec![
                MtgoVisibleActionControlCandidateV1 {
                    control_id: "control-0".to_owned(),
                    control_kind: MtgoVisibleControlKindV1::PhaseButton,
                    frame_region_evidence_id: 40,
                    semantic: decision.legal_actions()[0].clone(),
                    confidence_bps: 10_000,
                    visibly_enabled: true,
                },
                MtgoVisibleActionControlCandidateV1 {
                    control_id: "control-1".to_owned(),
                    control_kind: MtgoVisibleControlKindV1::Card,
                    frame_region_evidence_id: 50,
                    semantic: decision.legal_actions()[1].clone(),
                    confidence_bps: 10_000,
                    visibly_enabled: true,
                },
            ],
        };
        let admitted = duel_perception_profile_admitted_for_test_v1(
            check_untrusted_duel_perception_runtime_profile_v1(payload).unwrap(),
        );
        let mut scorer = RecordingScorerV1 {
            selected_index: 1,
            wrong_action_count: false,
            nonfinite: false,
            equal_logits: false,
            observed_json: None,
        };
        let selection = score_and_select_player_visible_profile_bound_duel_candidate_v1(
            candidate,
            &admitted,
            &digest('a'),
            &mut scorer,
        )
        .unwrap();
        let resolved = resolve_player_visible_profile_bound_selected_visible_control_v1(
            selection,
            control_set,
        )
        .unwrap();

        assert_eq!(resolved.control_id_v1(), "control-1");
        assert_eq!(resolved.frame_id_v1(), 1);
        assert_eq!(resolved.frame_sequence_v1(), 1);
        assert_eq!(
            resolved
                .private_profile_bound_resolution_commitment_sha256_v1()
                .len(),
            64
        );
        assert!(!resolved.safe_for_live_input_v1());
        assert!(!resolved.permits_match_entry_v1());
        assert!(!resolved.permits_spending_v1());
    }

    #[test]
    fn selection_commitment_is_visible_only_and_ties_choose_first_action() {
        let payload = duel_perception_runtime_profile_payload_for_test_v1();
        let first_profile =
            check_untrusted_duel_perception_runtime_profile_v1(payload.clone()).unwrap();
        let first_candidate = checked_untrusted_dxgi_decision_candidate_for_test_v1(&first_profile);
        let first_admitted = duel_perception_profile_admitted_for_test_v1(
            check_untrusted_duel_perception_runtime_profile_v1(payload.clone()).unwrap(),
        );
        let mut first_scorer = RecordingScorerV1 {
            selected_index: 0,
            wrong_action_count: false,
            nonfinite: false,
            equal_logits: true,
            observed_json: None,
        };
        let first = score_and_select_player_visible_profile_bound_duel_candidate_v1(
            first_candidate,
            &first_admitted,
            &digest('a'),
            &mut first_scorer,
        )
        .unwrap();

        let mut changed_payload = payload;
        changed_payload.classifier_assets_manifest_sha256 = digest('9');
        let second_profile =
            check_untrusted_duel_perception_runtime_profile_v1(changed_payload.clone()).unwrap();
        let second_candidate =
            checked_untrusted_dxgi_decision_candidate_for_test_v1(&second_profile);
        let second_admitted = duel_perception_profile_admitted_for_test_v1(
            check_untrusted_duel_perception_runtime_profile_v1(changed_payload).unwrap(),
        );
        let mut second_scorer = RecordingScorerV1 {
            selected_index: 0,
            wrong_action_count: false,
            nonfinite: false,
            equal_logits: true,
            observed_json: None,
        };
        let second = score_and_select_player_visible_profile_bound_duel_candidate_v1(
            second_candidate,
            &second_admitted,
            &digest('a'),
            &mut second_scorer,
        )
        .unwrap();

        assert_eq!(first.selected_index_v1(), 0);
        assert_eq!(second.selected_index_v1(), 0);
        assert_eq!(
            first.selection_commitment_sha256_v1(),
            second.selection_commitment_sha256_v1()
        );
        assert_ne!(
            first.private_source_binding_commitment_sha256_v1(),
            second.private_source_binding_commitment_sha256_v1()
        );
    }
}
