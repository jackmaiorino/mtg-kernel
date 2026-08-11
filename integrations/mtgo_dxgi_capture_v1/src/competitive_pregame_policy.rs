use crate::competitive_operator_bootstrap::OpaqueMtgoCompetitiveOperatorResourcesV1;
use crate::probe::{
    non_model_pregame_heuristic_algorithm_commitment_v1, MtgoNonModelPregameHeuristicV1,
};
use mtgo_blackbox_v1::{
    MtgoCompetitivePregameStageLabelV1, MtgoCompetitivePregameVisibleCardV1,
    MtgoCompetitivePregameVisibleControlSemanticV1, MtgoCompetitivePregameVisibleControlV1,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const MTGO_COMPETITIVE_PREGAME_HEURISTIC_REVIEW_SCHEMA_V1: u32 = 1;

const COMPETITIVE_PREGAME_HEURISTIC_REVIEW_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-pregame-heuristic-review-v1";
const COMPETITIVE_PREGAME_HEURISTIC_ADMISSION_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-pregame-heuristic-admission-v1";
const COMPETITIVE_OPERATOR_PREGAME_RESOURCE_BUNDLE_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-operator-pregame-resource-bundle-v1";

/// Intentionally empty until one exact competitive deck, feature catalog,
/// and heuristic behavior review are inspected and pinned in source.
const RATIFIED_COMPETITIVE_PREGAME_HEURISTIC_REVIEW_COMMITMENT_V1: Option<&str> = None;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoCompetitivePregameHeuristicReviewDeclarationsV1 {
    pub exact_deck_manifest_reviewed: bool,
    pub every_main_deck_card_feature_reviewed: bool,
    pub mulligan_behavior_reviewed: bool,
    pub london_bottoming_behavior_reviewed: bool,
    pub explicitly_non_model: bool,
    pub league_and_challenge_use_reviewed: bool,
    pub no_sideboard_policy_claimed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoReviewedCompetitivePregameHeuristicCandidateV1 {
    pub schema_version: u32,
    pub review_id: String,
    pub reviewer_alias_sha256: String,
    pub heuristic_profile_commitment_sha256: String,
    pub heuristic_algorithm_commitment_sha256: String,
    pub source_card_catalog_sha256: String,
    pub deck_list_sha256: String,
    pub deck_manifest_commitment_sha256: String,
    pub deck_format_sha256: String,
    pub gameplay_policy_deployment_commitment_sha256: String,
    pub declarations: MtgoCompetitivePregameHeuristicReviewDeclarationsV1,
    pub review_commitment_sha256: String,
}

/// Structurally checked review candidate. Its declarations are human claims,
/// so this type cannot score a competitive pregame or authorize input.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::CheckedUntrustedMtgoCompetitivePregameHeuristicV1;
/// let _forged = CheckedUntrustedMtgoCompetitivePregameHeuristicV1 {};
/// ```
pub struct CheckedUntrustedMtgoCompetitivePregameHeuristicV1 {
    heuristic: MtgoNonModelPregameHeuristicV1,
    review: MtgoReviewedCompetitivePregameHeuristicCandidateV1,
}

impl CheckedUntrustedMtgoCompetitivePregameHeuristicV1 {
    pub fn review_v1(&self) -> MtgoReviewedCompetitivePregameHeuristicCandidateV1 {
        self.review.clone()
    }

    pub fn safe_for_live_scoring_v1(&self) -> bool {
        false
    }

    pub fn safe_for_input_v1(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoAdmittedCompetitivePregameHeuristicCommitmentsV1 {
    pub heuristic_profile_commitment_sha256: String,
    pub heuristic_algorithm_commitment_sha256: String,
    pub source_card_catalog_sha256: String,
    pub deck_list_sha256: String,
    pub deck_manifest_commitment_sha256: String,
    pub deck_format_sha256: String,
    pub gameplay_policy_deployment_commitment_sha256: String,
    pub review_commitment_sha256: String,
    pub admission_commitment_sha256: String,
}

/// Move-only, compile-ratified deployment of the deterministic non-model
/// pregame stopgap for one exact competitive deck. It exposes commitments
/// only and has no input, entry, spending, or sideboard-selection conversion.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::AdmittedMtgoCompetitivePregameHeuristicV1;
/// let _forged = AdmittedMtgoCompetitivePregameHeuristicV1 {};
/// ```
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::AdmittedMtgoCompetitivePregameHeuristicV1;
/// fn require_clone<T: Clone>() {}
/// require_clone::<AdmittedMtgoCompetitivePregameHeuristicV1>();
/// ```
pub struct AdmittedMtgoCompetitivePregameHeuristicV1 {
    _heuristic: MtgoNonModelPregameHeuristicV1,
    commitments: MtgoAdmittedCompetitivePregameHeuristicCommitmentsV1,
}

impl AdmittedMtgoCompetitivePregameHeuristicV1 {
    pub fn commitments_v1(&self) -> MtgoAdmittedCompetitivePregameHeuristicCommitmentsV1 {
        self.commitments.clone()
    }

    pub fn is_model_backed_v1(&self) -> bool {
        false
    }

    pub fn safe_for_live_scoring_v1(&self) -> bool {
        false
    }

    pub fn safe_for_input_v1(&self) -> bool {
        false
    }

    pub fn permits_event_entry_v1(&self) -> bool {
        false
    }

    pub fn permits_spending_v1(&self) -> bool {
        false
    }

    pub fn permits_sideboard_selection_v1(&self) -> bool {
        false
    }

    pub(crate) fn select_visible_control_v1(
        &self,
        stage: MtgoCompetitivePregameStageLabelV1,
        visible_cards: &[MtgoCompetitivePregameVisibleCardV1],
        visible_controls: &[MtgoCompetitivePregameVisibleControlV1],
    ) -> Result<MtgoCompetitivePregameVisibleControlSemanticV1, String> {
        self._heuristic.select_competitive_visible_control_v1(
            stage,
            visible_cards,
            visible_controls,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoCompetitiveOperatorPregameResourceCommitmentsV1 {
    pub operator_resource_bundle_commitment_sha256: String,
    pub approved_account_alias_sha256: String,
    pub deck_list_sha256: String,
    pub deck_manifest_commitment_sha256: String,
    pub deck_format_sha256: String,
    pub gameplay_policy_deployment_commitment_sha256: String,
    pub heuristic_profile_commitment_sha256: String,
    pub heuristic_algorithm_commitment_sha256: String,
    pub heuristic_review_commitment_sha256: String,
    pub heuristic_admission_commitment_sha256: String,
    pub pregame_resource_bundle_commitment_sha256: String,
}

/// The exact operator resources plus the separately reviewed deterministic
/// pregame policy for the same deck and gameplay deployment.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoCompetitiveOperatorPregameResourcesV1;
/// let _forged = OpaqueMtgoCompetitiveOperatorPregameResourcesV1 {};
/// ```
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoCompetitiveOperatorPregameResourcesV1;
/// fn require_clone<T: Clone>() {}
/// require_clone::<OpaqueMtgoCompetitiveOperatorPregameResourcesV1>();
/// ```
pub struct OpaqueMtgoCompetitiveOperatorPregameResourcesV1 {
    operator_resources: OpaqueMtgoCompetitiveOperatorResourcesV1,
    pregame_heuristic: AdmittedMtgoCompetitivePregameHeuristicV1,
    commitments: MtgoCompetitiveOperatorPregameResourceCommitmentsV1,
}

impl OpaqueMtgoCompetitiveOperatorPregameResourcesV1 {
    pub fn commitments_v1(&self) -> MtgoCompetitiveOperatorPregameResourceCommitmentsV1 {
        self.commitments.clone()
    }

    pub fn safe_for_live_capture_v1(&self) -> bool {
        false
    }

    pub fn safe_for_input_v1(&self) -> bool {
        false
    }

    pub fn permits_event_entry_v1(&self) -> bool {
        false
    }

    pub fn permits_spending_v1(&self) -> bool {
        false
    }

    pub fn into_parts_v1(self) -> MtgoCompetitiveOperatorPregameResourcesPartsV1 {
        MtgoCompetitiveOperatorPregameResourcesPartsV1 {
            operator_resources: self.operator_resources,
            pregame_heuristic: self.pregame_heuristic,
        }
    }
}

pub struct MtgoCompetitiveOperatorPregameResourcesPartsV1 {
    pub operator_resources: OpaqueMtgoCompetitiveOperatorResourcesV1,
    pub pregame_heuristic: AdmittedMtgoCompetitivePregameHeuristicV1,
}

#[allow(clippy::too_many_arguments)]
pub fn check_untrusted_competitive_pregame_heuristic_v1(
    heuristic: MtgoNonModelPregameHeuristicV1,
    review_id: String,
    reviewer_alias_sha256: String,
    deck_list_sha256: String,
    deck_manifest_commitment_sha256: String,
    deck_format_sha256: String,
    gameplay_policy_deployment_commitment_sha256: String,
    declarations: MtgoCompetitivePregameHeuristicReviewDeclarationsV1,
) -> Result<CheckedUntrustedMtgoCompetitivePregameHeuristicV1, String> {
    let mut review = MtgoReviewedCompetitivePregameHeuristicCandidateV1 {
        schema_version: MTGO_COMPETITIVE_PREGAME_HEURISTIC_REVIEW_SCHEMA_V1,
        review_id,
        reviewer_alias_sha256,
        heuristic_profile_commitment_sha256: heuristic.profile_commitment_sha256_v1().to_owned(),
        heuristic_algorithm_commitment_sha256: non_model_pregame_heuristic_algorithm_commitment_v1(
        ),
        source_card_catalog_sha256: heuristic.profile_v1().source_card_catalog_sha256.clone(),
        deck_list_sha256,
        deck_manifest_commitment_sha256,
        deck_format_sha256,
        gameplay_policy_deployment_commitment_sha256,
        declarations,
        review_commitment_sha256: String::new(),
    };
    validate_review_candidate_v1(&review, &heuristic)?;
    review.review_commitment_sha256 = review_commitment_v1(&review)?;
    Ok(CheckedUntrustedMtgoCompetitivePregameHeuristicV1 { heuristic, review })
}

pub fn admit_ratified_competitive_pregame_heuristic_v1(
    checked: CheckedUntrustedMtgoCompetitivePregameHeuristicV1,
) -> Result<AdmittedMtgoCompetitivePregameHeuristicV1, String> {
    admit_competitive_pregame_heuristic_against_ratification_v1(
        checked,
        RATIFIED_COMPETITIVE_PREGAME_HEURISTIC_REVIEW_COMMITMENT_V1,
    )
}

pub fn bind_competitive_operator_pregame_resources_v1(
    operator_resources: OpaqueMtgoCompetitiveOperatorResourcesV1,
    pregame_heuristic: AdmittedMtgoCompetitivePregameHeuristicV1,
) -> Result<OpaqueMtgoCompetitiveOperatorPregameResourcesV1, String> {
    let commitments = operator_pregame_resource_commitments_v1(
        &operator_resources.commitments_v1(),
        &pregame_heuristic.commitments_v1(),
    )?;
    Ok(OpaqueMtgoCompetitiveOperatorPregameResourcesV1 {
        operator_resources,
        pregame_heuristic,
        commitments,
    })
}

pub(crate) fn competitive_pregame_heuristic_ratification_present_v1() -> bool {
    RATIFIED_COMPETITIVE_PREGAME_HEURISTIC_REVIEW_COMMITMENT_V1.is_some()
}

fn validate_review_candidate_v1(
    review: &MtgoReviewedCompetitivePregameHeuristicCandidateV1,
    heuristic: &MtgoNonModelPregameHeuristicV1,
) -> Result<(), String> {
    if review.schema_version != MTGO_COMPETITIVE_PREGAME_HEURISTIC_REVIEW_SCHEMA_V1
        || !valid_safe_identifier_v1(&review.review_id)
        || review.heuristic_profile_commitment_sha256 != heuristic.profile_commitment_sha256_v1()
        || review.heuristic_algorithm_commitment_sha256
            != non_model_pregame_heuristic_algorithm_commitment_v1()
        || review.source_card_catalog_sha256 != heuristic.profile_v1().source_card_catalog_sha256
        || !review.declarations.exact_deck_manifest_reviewed
        || !review.declarations.every_main_deck_card_feature_reviewed
        || !review.declarations.mulligan_behavior_reviewed
        || !review.declarations.london_bottoming_behavior_reviewed
        || !review.declarations.explicitly_non_model
        || !review.declarations.league_and_challenge_use_reviewed
        || !review.declarations.no_sideboard_policy_claimed
    {
        return Err("competitive pregame heuristic review is incomplete or crossed".to_owned());
    }
    for digest in [
        review.reviewer_alias_sha256.as_str(),
        review.heuristic_profile_commitment_sha256.as_str(),
        review.heuristic_algorithm_commitment_sha256.as_str(),
        review.source_card_catalog_sha256.as_str(),
        review.deck_list_sha256.as_str(),
        review.deck_manifest_commitment_sha256.as_str(),
        review.deck_format_sha256.as_str(),
        review.gameplay_policy_deployment_commitment_sha256.as_str(),
    ] {
        if !is_lower_sha256_v1(digest) {
            return Err(
                "competitive pregame heuristic review contains an invalid digest".to_owned(),
            );
        }
    }
    Ok(())
}

fn admit_competitive_pregame_heuristic_against_ratification_v1(
    checked: CheckedUntrustedMtgoCompetitivePregameHeuristicV1,
    ratified: Option<&str>,
) -> Result<AdmittedMtgoCompetitivePregameHeuristicV1, String> {
    validate_review_candidate_v1(&checked.review, &checked.heuristic)?;
    let expected_review_commitment_sha256 = review_commitment_v1(&checked.review)?;
    if checked.review.review_commitment_sha256 != expected_review_commitment_sha256 {
        return Err("competitive pregame heuristic review commitment changed".to_owned());
    }
    let ratified = ratified.ok_or("competitive pregame heuristic review is not ratified")?;
    if !is_lower_sha256_v1(ratified) || ratified != expected_review_commitment_sha256 {
        return Err(
            "competitive pregame heuristic review differs from the production root".to_owned(),
        );
    }
    let admission_commitment_sha256 = hash_parts_v1(
        COMPETITIVE_PREGAME_HEURISTIC_ADMISSION_DOMAIN_V1,
        &[
            expected_review_commitment_sha256.as_bytes(),
            checked
                .review
                .heuristic_profile_commitment_sha256
                .as_bytes(),
            checked
                .review
                .heuristic_algorithm_commitment_sha256
                .as_bytes(),
            checked.review.deck_manifest_commitment_sha256.as_bytes(),
            checked
                .review
                .gameplay_policy_deployment_commitment_sha256
                .as_bytes(),
            b"reviewed_non_model_mulligan_and_london_bottoming_no_sideboard_no_input",
        ],
    );
    let commitments = MtgoAdmittedCompetitivePregameHeuristicCommitmentsV1 {
        heuristic_profile_commitment_sha256: checked
            .review
            .heuristic_profile_commitment_sha256
            .clone(),
        heuristic_algorithm_commitment_sha256: checked
            .review
            .heuristic_algorithm_commitment_sha256
            .clone(),
        source_card_catalog_sha256: checked.review.source_card_catalog_sha256.clone(),
        deck_list_sha256: checked.review.deck_list_sha256.clone(),
        deck_manifest_commitment_sha256: checked.review.deck_manifest_commitment_sha256.clone(),
        deck_format_sha256: checked.review.deck_format_sha256.clone(),
        gameplay_policy_deployment_commitment_sha256: checked
            .review
            .gameplay_policy_deployment_commitment_sha256
            .clone(),
        review_commitment_sha256: expected_review_commitment_sha256,
        admission_commitment_sha256,
    };
    Ok(AdmittedMtgoCompetitivePregameHeuristicV1 {
        _heuristic: checked.heuristic,
        commitments,
    })
}

fn review_commitment_v1(
    review: &MtgoReviewedCompetitivePregameHeuristicCandidateV1,
) -> Result<String, String> {
    let mut unsigned = review.clone();
    unsigned.review_commitment_sha256.clear();
    let bytes = serde_json::to_vec(&unsigned)
        .map_err(|error| format!("serialize competitive pregame heuristic review: {error}"))?;
    Ok(hash_parts_v1(
        COMPETITIVE_PREGAME_HEURISTIC_REVIEW_DOMAIN_V1,
        &[&bytes],
    ))
}

fn operator_pregame_resource_commitments_v1(
    operator: &crate::competitive_operator_bootstrap::MtgoCompetitiveOperatorResourceCommitmentsV1,
    pregame: &MtgoAdmittedCompetitivePregameHeuristicCommitmentsV1,
) -> Result<MtgoCompetitiveOperatorPregameResourceCommitmentsV1, String> {
    if operator.deck_list_sha256 != pregame.deck_list_sha256
        || operator.deck_manifest_commitment_sha256 != pregame.deck_manifest_commitment_sha256
        || operator.deck_format_sha256 != pregame.deck_format_sha256
        || operator.policy_deployment_commitment_sha256
            != pregame.gameplay_policy_deployment_commitment_sha256
    {
        return Err(
            "competitive operator and pregame heuristic do not share one exact deck and gameplay deployment"
                .to_owned(),
        );
    }
    for digest in [
        operator.resource_bundle_commitment_sha256.as_str(),
        operator.approved_account_alias_sha256.as_str(),
        pregame.heuristic_profile_commitment_sha256.as_str(),
        pregame.heuristic_algorithm_commitment_sha256.as_str(),
        pregame.review_commitment_sha256.as_str(),
        pregame.admission_commitment_sha256.as_str(),
    ] {
        if !is_lower_sha256_v1(digest) {
            return Err(
                "competitive operator pregame bundle contains an invalid digest".to_owned(),
            );
        }
    }
    let mut commitments = MtgoCompetitiveOperatorPregameResourceCommitmentsV1 {
        operator_resource_bundle_commitment_sha256: operator
            .resource_bundle_commitment_sha256
            .clone(),
        approved_account_alias_sha256: operator.approved_account_alias_sha256.clone(),
        deck_list_sha256: operator.deck_list_sha256.clone(),
        deck_manifest_commitment_sha256: operator.deck_manifest_commitment_sha256.clone(),
        deck_format_sha256: operator.deck_format_sha256.clone(),
        gameplay_policy_deployment_commitment_sha256: operator
            .policy_deployment_commitment_sha256
            .clone(),
        heuristic_profile_commitment_sha256: pregame.heuristic_profile_commitment_sha256.clone(),
        heuristic_algorithm_commitment_sha256: pregame
            .heuristic_algorithm_commitment_sha256
            .clone(),
        heuristic_review_commitment_sha256: pregame.review_commitment_sha256.clone(),
        heuristic_admission_commitment_sha256: pregame.admission_commitment_sha256.clone(),
        pregame_resource_bundle_commitment_sha256: String::new(),
    };
    let bytes = serde_json::to_vec(&commitments)
        .map_err(|error| format!("serialize competitive operator pregame bundle: {error}"))?;
    commitments.pregame_resource_bundle_commitment_sha256 = hash_parts_v1(
        COMPETITIVE_OPERATOR_PREGAME_RESOURCE_BUNDLE_DOMAIN_V1,
        &[&bytes],
    );
    Ok(commitments)
}

fn hash_parts_v1(domain: &[u8], parts: &[&[u8]]) -> String {
    let mut hasher = Sha256::new();
    hasher.update((domain.len() as u64).to_be_bytes());
    hasher.update(domain);
    for part in parts {
        hasher.update((part.len() as u64).to_be_bytes());
        hasher.update(part);
    }
    format!("{:x}", hasher.finalize())
}

fn valid_safe_identifier_v1(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 96
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

fn is_lower_sha256_v1(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::competitive_operator_bootstrap::MtgoCompetitiveOperatorResourceCommitmentsV1;

    fn declarations_v1() -> MtgoCompetitivePregameHeuristicReviewDeclarationsV1 {
        MtgoCompetitivePregameHeuristicReviewDeclarationsV1 {
            exact_deck_manifest_reviewed: true,
            every_main_deck_card_feature_reviewed: true,
            mulligan_behavior_reviewed: true,
            london_bottoming_behavior_reviewed: true,
            explicitly_non_model: true,
            league_and_challenge_use_reviewed: true,
            no_sideboard_policy_claimed: true,
        }
    }

    fn checked_v1() -> CheckedUntrustedMtgoCompetitivePregameHeuristicV1 {
        check_untrusted_competitive_pregame_heuristic_v1(
            MtgoNonModelPregameHeuristicV1::kernel_basic_lands_wiring_only_v1().unwrap(),
            "competitive-pregame-review-v1".to_owned(),
            "1".repeat(64),
            "2".repeat(64),
            "3".repeat(64),
            "4".repeat(64),
            "5".repeat(64),
            declarations_v1(),
        )
        .unwrap()
    }

    fn admitted_v1() -> AdmittedMtgoCompetitivePregameHeuristicV1 {
        let checked = checked_v1();
        let ratification = checked.review.review_commitment_sha256.clone();
        admit_competitive_pregame_heuristic_against_ratification_v1(checked, Some(&ratification))
            .unwrap()
    }

    fn operator_v1() -> MtgoCompetitiveOperatorResourceCommitmentsV1 {
        MtgoCompetitiveOperatorResourceCommitmentsV1 {
            navigation_profile_commitment_sha256: "6".repeat(64),
            navigation_profile_admission_commitment_sha256: "7".repeat(64),
            approved_account_alias_sha256: "8".repeat(64),
            navigation_runtime_identity_commitment_sha256: "9".repeat(64),
            event_listing_evaluation_ratification_commitment_sha256: "a".repeat(64),
            event_listing_evaluation_admission_commitment_sha256: "b".repeat(64),
            event_record_evaluation_ratification_commitment_sha256: "c".repeat(64),
            event_record_evaluation_admission_commitment_sha256: "d".repeat(64),
            deck_list_sha256: "2".repeat(64),
            deck_manifest_commitment_sha256: "3".repeat(64),
            deck_format_sha256: "4".repeat(64),
            policy_deployment_commitment_sha256: "5".repeat(64),
            duel_perception_profile_commitment_sha256: "e".repeat(64),
            duel_perception_profile_admission_commitment_sha256: "f".repeat(64),
            duel_perception_runtime_identity_commitment_sha256: "0".repeat(64),
            duel_lifecycle_evaluation_commitment_sha256: "1".repeat(64),
            duel_lifecycle_admission_commitment_sha256: "2".repeat(64),
            duel_gesture_evaluation_commitment_sha256: "3".repeat(64),
            duel_gesture_profile_admission_commitment_sha256: "4".repeat(64),
            duel_gesture_runtime_identity_commitment_sha256: "5".repeat(64),
            changed_sideboard_evaluation_ratification_commitment_sha256: None,
            changed_sideboard_evaluation_admission_commitment_sha256: None,
            resource_bundle_commitment_sha256: "6".repeat(64),
        }
    }

    #[test]
    fn review_binds_profile_algorithm_and_exact_deck() {
        let checked = checked_v1();
        let review = checked.review_v1();
        assert_eq!(
            review.heuristic_algorithm_commitment_sha256,
            non_model_pregame_heuristic_algorithm_commitment_v1()
        );
        assert_eq!(review.deck_manifest_commitment_sha256, "3".repeat(64));
        assert!(is_lower_sha256_v1(&review.review_commitment_sha256));
        assert!(!checked.safe_for_live_scoring_v1());
        assert!(!checked.safe_for_input_v1());
    }

    #[test]
    fn incomplete_review_declarations_fail_closed() {
        let mut declarations = declarations_v1();
        declarations.every_main_deck_card_feature_reviewed = false;
        assert!(check_untrusted_competitive_pregame_heuristic_v1(
            MtgoNonModelPregameHeuristicV1::kernel_basic_lands_wiring_only_v1().unwrap(),
            "competitive-pregame-review-v1".to_owned(),
            "1".repeat(64),
            "2".repeat(64),
            "3".repeat(64),
            "4".repeat(64),
            "5".repeat(64),
            declarations,
        )
        .is_err());
    }

    #[test]
    fn production_admission_is_empty_but_exact_internal_ratification_binds() {
        assert!(admit_ratified_competitive_pregame_heuristic_v1(checked_v1()).is_err());
        let admitted = admitted_v1();
        let commitments = admitted.commitments_v1();
        assert!(is_lower_sha256_v1(&commitments.admission_commitment_sha256));
        assert!(!admitted.is_model_backed_v1());
        assert!(!admitted.safe_for_live_scoring_v1());
        assert!(!admitted.safe_for_input_v1());
        assert!(!admitted.permits_event_entry_v1());
        assert!(!admitted.permits_spending_v1());
        assert!(!admitted.permits_sideboard_selection_v1());
    }

    #[test]
    fn operator_binding_requires_exact_deck_format_and_gameplay_policy() {
        let operator = operator_v1();
        let admitted = admitted_v1().commitments_v1();
        let baseline = operator_pregame_resource_commitments_v1(&operator, &admitted).unwrap();
        assert!(is_lower_sha256_v1(
            &baseline.pregame_resource_bundle_commitment_sha256
        ));

        for mutate in [
            |value: &mut MtgoCompetitiveOperatorResourceCommitmentsV1| {
                value.deck_list_sha256 = "7".repeat(64)
            },
            |value: &mut MtgoCompetitiveOperatorResourceCommitmentsV1| {
                value.deck_manifest_commitment_sha256 = "7".repeat(64)
            },
            |value: &mut MtgoCompetitiveOperatorResourceCommitmentsV1| {
                value.deck_format_sha256 = "7".repeat(64)
            },
            |value: &mut MtgoCompetitiveOperatorResourceCommitmentsV1| {
                value.policy_deployment_commitment_sha256 = "7".repeat(64)
            },
        ] {
            let mut changed = operator.clone();
            mutate(&mut changed);
            assert!(operator_pregame_resource_commitments_v1(&changed, &admitted).is_err());
        }
    }
}
