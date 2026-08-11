use crate::{
    MtgoVerifiedCompetitiveNavigationClassifierRuntimeCommitmentsV1,
    MtgoVerifiedDuelGestureTargetRuntimeCommitmentsV1,
    MtgoVerifiedDuelPerceptionRuntimeCommitmentsV1,
    OpaqueMtgoVerifiedCompetitiveNavigationClassifierRuntimeV1,
    OpaqueMtgoVerifiedDuelGestureTargetRuntimeV1, OpaqueMtgoVerifiedDuelPerceptionRuntimeV1,
};
use mtgo_blackbox_v1::{
    AdmittedMtgoCompetitiveDuelLifecycleProfileV1, AdmittedMtgoCompetitiveEventListingEvaluationV1,
    AdmittedMtgoCompetitiveEventRecordEvaluationV1, AdmittedMtgoCompetitiveNavigationProfileV1,
    AdmittedMtgoCompetitiveSideboardEvaluationV1, AdmittedMtgoDuelGestureProfileV1,
    AdmittedMtgoDuelPerceptionProfileV1, LoadedMtgoNativeCheckpointDeploymentV1,
    MtgoReviewedCompetitiveEventListingEvaluationRatificationCandidateV1,
    MtgoReviewedCompetitiveEventRecordEvaluationRatificationCandidateV1,
    MtgoReviewedCompetitiveSideboardEvaluationRatificationCandidateV1,
    ValidatedMtgoCompetitiveDeckManifestV1,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const COMPETITIVE_OPERATOR_RESOURCE_BUNDLE_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-operator-resource-bundle-v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoCompetitiveOperatorResourceCommitmentsV1 {
    pub navigation_profile_commitment_sha256: String,
    pub navigation_profile_admission_commitment_sha256: String,
    pub approved_account_alias_sha256: String,
    pub navigation_runtime_identity_commitment_sha256: String,
    pub event_listing_evaluation_ratification_commitment_sha256: String,
    pub event_listing_evaluation_admission_commitment_sha256: String,
    pub event_record_evaluation_ratification_commitment_sha256: String,
    pub event_record_evaluation_admission_commitment_sha256: String,
    pub deck_list_sha256: String,
    pub deck_manifest_commitment_sha256: String,
    pub deck_format_sha256: String,
    pub policy_deployment_commitment_sha256: String,
    pub duel_perception_profile_commitment_sha256: String,
    pub duel_perception_profile_admission_commitment_sha256: String,
    pub duel_perception_runtime_identity_commitment_sha256: String,
    pub duel_lifecycle_evaluation_commitment_sha256: String,
    pub duel_lifecycle_admission_commitment_sha256: String,
    pub duel_gesture_evaluation_commitment_sha256: String,
    pub duel_gesture_profile_admission_commitment_sha256: String,
    pub duel_gesture_runtime_identity_commitment_sha256: String,
    pub changed_sideboard_evaluation_ratification_commitment_sha256: Option<String>,
    pub changed_sideboard_evaluation_admission_commitment_sha256: Option<String>,
    pub resource_bundle_commitment_sha256: String,
}

/// One cross-checked set of the semantic profiles, exact runtime artifacts,
/// and native checkpoint needed by a future League or Challenge operator.
///
/// The bundle is move-only and contains no authorization, account alias text,
/// pixels, coordinates, event-entry method, or input method. Production cannot
/// construct it while the component evaluation roots remain empty.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoCompetitiveOperatorResourcesV1;
/// let _forged = OpaqueMtgoCompetitiveOperatorResourcesV1 {};
/// ```
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoCompetitiveOperatorResourcesV1;
/// fn require_clone<T: Clone>() {}
/// require_clone::<OpaqueMtgoCompetitiveOperatorResourcesV1>();
/// ```
pub struct OpaqueMtgoCompetitiveOperatorResourcesV1 {
    parts: MtgoCompetitiveOperatorResourcesPartsV1,
    commitments: MtgoCompetitiveOperatorResourceCommitmentsV1,
}

impl OpaqueMtgoCompetitiveOperatorResourcesV1 {
    pub fn commitments_v1(&self) -> MtgoCompetitiveOperatorResourceCommitmentsV1 {
        self.commitments.clone()
    }

    pub fn changed_sideboard_resources_present_v1(&self) -> bool {
        self.parts.changed_sideboard_evaluation.is_some()
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

    pub fn navigation_profile_v1(&self) -> &AdmittedMtgoCompetitiveNavigationProfileV1 {
        &self.parts.navigation_profile
    }

    pub fn navigation_runtime_v1(
        &self,
    ) -> &OpaqueMtgoVerifiedCompetitiveNavigationClassifierRuntimeV1 {
        &self.parts.navigation_runtime
    }

    /// Returns the same already-opaque resources for the eventual operator.
    /// Every downstream authority boundary still performs its own exact join.
    pub fn into_parts_v1(self) -> MtgoCompetitiveOperatorResourcesPartsV1 {
        self.parts
    }
}

pub struct MtgoCompetitiveOperatorResourcesPartsV1 {
    pub navigation_profile: AdmittedMtgoCompetitiveNavigationProfileV1,
    pub navigation_runtime: OpaqueMtgoVerifiedCompetitiveNavigationClassifierRuntimeV1,
    pub event_listing_evaluation: AdmittedMtgoCompetitiveEventListingEvaluationV1,
    pub event_record_evaluation: AdmittedMtgoCompetitiveEventRecordEvaluationV1,
    pub deck_manifest: ValidatedMtgoCompetitiveDeckManifestV1,
    pub duel_perception_profile: AdmittedMtgoDuelPerceptionProfileV1,
    pub duel_perception_runtime: OpaqueMtgoVerifiedDuelPerceptionRuntimeV1,
    pub duel_lifecycle_profile: AdmittedMtgoCompetitiveDuelLifecycleProfileV1,
    pub duel_gesture_profile: AdmittedMtgoDuelGestureProfileV1,
    pub duel_gesture_runtime: OpaqueMtgoVerifiedDuelGestureTargetRuntimeV1,
    pub checkpoint_deployment: LoadedMtgoNativeCheckpointDeploymentV1,
    pub changed_sideboard_evaluation: Option<AdmittedMtgoCompetitiveSideboardEvaluationV1>,
}

#[allow(clippy::too_many_arguments)]
pub fn bind_competitive_operator_resources_v1(
    navigation_profile: AdmittedMtgoCompetitiveNavigationProfileV1,
    navigation_runtime: OpaqueMtgoVerifiedCompetitiveNavigationClassifierRuntimeV1,
    event_listing_evaluation: AdmittedMtgoCompetitiveEventListingEvaluationV1,
    event_record_evaluation: AdmittedMtgoCompetitiveEventRecordEvaluationV1,
    deck_manifest: ValidatedMtgoCompetitiveDeckManifestV1,
    duel_perception_profile: AdmittedMtgoDuelPerceptionProfileV1,
    duel_perception_runtime: OpaqueMtgoVerifiedDuelPerceptionRuntimeV1,
    duel_lifecycle_profile: AdmittedMtgoCompetitiveDuelLifecycleProfileV1,
    duel_gesture_profile: AdmittedMtgoDuelGestureProfileV1,
    duel_gesture_runtime: OpaqueMtgoVerifiedDuelGestureTargetRuntimeV1,
    checkpoint_deployment: LoadedMtgoNativeCheckpointDeploymentV1,
    changed_sideboard_evaluation: Option<AdmittedMtgoCompetitiveSideboardEvaluationV1>,
) -> Result<OpaqueMtgoCompetitiveOperatorResourcesV1, String> {
    let navigation = navigation_runtime.commitments_v1();
    let listing = event_listing_evaluation.commitments_v1();
    let record = event_record_evaluation.commitments_v1();
    let perception = duel_perception_runtime.commitments_v1();
    let gesture = duel_gesture_runtime.commitments_v1();
    let sideboard = changed_sideboard_evaluation
        .as_ref()
        .map(AdmittedMtgoCompetitiveSideboardEvaluationV1::commitments_v1);
    let identity = MtgoCompetitiveOperatorResourceIdentityV1 {
        navigation_profile_commitment_sha256: navigation_profile
            .profile_commitment_sha256()
            .to_owned(),
        navigation_profile_admission_commitment_sha256: navigation_profile
            .admission_commitment_sha256()
            .to_owned(),
        approved_account_alias_sha256: navigation_profile
            .checked_runtime_profile()
            .approved_account_alias_sha256()
            .to_owned(),
        navigation,
        listing,
        listing_admission_commitment_sha256: event_listing_evaluation
            .admission_commitment_sha256_v1()
            .to_owned(),
        record,
        record_admission_commitment_sha256: event_record_evaluation
            .admission_commitment_sha256_v1()
            .to_owned(),
        deck_list_sha256: deck_manifest.deck_list_sha256().to_owned(),
        deck_manifest_commitment_sha256: deck_manifest.manifest_commitment_sha256().to_owned(),
        deck_format_sha256: deck_manifest.format_sha256().to_owned(),
        duel_perception_profile_commitment_sha256: duel_perception_profile
            .perception_profile_commitment_sha256()
            .to_owned(),
        duel_perception_profile_admission_commitment_sha256: duel_perception_profile
            .admission_commitment_sha256()
            .to_owned(),
        perception,
        duel_lifecycle_perception_profile_commitment_sha256: duel_lifecycle_profile
            .duel_perception_profile_commitment_sha256()
            .to_owned(),
        duel_lifecycle_perception_admission_commitment_sha256: duel_lifecycle_profile
            .duel_perception_profile_admission_commitment_sha256()
            .to_owned(),
        duel_lifecycle_evaluation_commitment_sha256: duel_lifecycle_profile
            .evaluation_commitment_sha256()
            .to_owned(),
        duel_lifecycle_admission_commitment_sha256: duel_lifecycle_profile
            .admission_commitment_sha256()
            .to_owned(),
        duel_gesture_evaluation_commitment_sha256: duel_gesture_profile
            .evaluation_commitment_sha256()
            .to_owned(),
        duel_gesture_profile_admission_commitment_sha256: duel_gesture_profile
            .admission_commitment_sha256()
            .to_owned(),
        duel_gesture_perception_admission_commitment_sha256: duel_gesture_profile
            .perception_profile_admission_commitment_sha256()
            .to_owned(),
        gesture,
        policy_deployment_commitment_sha256: checkpoint_deployment
            .deployment_commitment_sha256()
            .to_owned(),
        sideboard,
        sideboard_admission_commitment_sha256: changed_sideboard_evaluation
            .as_ref()
            .map(|value| value.admission_commitment_sha256_v1().to_owned()),
    };
    let commitments = validate_operator_resource_identity_v1(&identity)?;
    Ok(OpaqueMtgoCompetitiveOperatorResourcesV1 {
        parts: MtgoCompetitiveOperatorResourcesPartsV1 {
            navigation_profile,
            navigation_runtime,
            event_listing_evaluation,
            event_record_evaluation,
            deck_manifest,
            duel_perception_profile,
            duel_perception_runtime,
            duel_lifecycle_profile,
            duel_gesture_profile,
            duel_gesture_runtime,
            checkpoint_deployment,
            changed_sideboard_evaluation,
        },
        commitments,
    })
}

#[derive(Clone)]
struct MtgoCompetitiveOperatorResourceIdentityV1 {
    navigation_profile_commitment_sha256: String,
    navigation_profile_admission_commitment_sha256: String,
    approved_account_alias_sha256: String,
    navigation: MtgoVerifiedCompetitiveNavigationClassifierRuntimeCommitmentsV1,
    listing: MtgoReviewedCompetitiveEventListingEvaluationRatificationCandidateV1,
    listing_admission_commitment_sha256: String,
    record: MtgoReviewedCompetitiveEventRecordEvaluationRatificationCandidateV1,
    record_admission_commitment_sha256: String,
    deck_list_sha256: String,
    deck_manifest_commitment_sha256: String,
    deck_format_sha256: String,
    duel_perception_profile_commitment_sha256: String,
    duel_perception_profile_admission_commitment_sha256: String,
    perception: MtgoVerifiedDuelPerceptionRuntimeCommitmentsV1,
    duel_lifecycle_perception_profile_commitment_sha256: String,
    duel_lifecycle_perception_admission_commitment_sha256: String,
    duel_lifecycle_evaluation_commitment_sha256: String,
    duel_lifecycle_admission_commitment_sha256: String,
    duel_gesture_evaluation_commitment_sha256: String,
    duel_gesture_profile_admission_commitment_sha256: String,
    duel_gesture_perception_admission_commitment_sha256: String,
    gesture: MtgoVerifiedDuelGestureTargetRuntimeCommitmentsV1,
    policy_deployment_commitment_sha256: String,
    sideboard: Option<MtgoReviewedCompetitiveSideboardEvaluationRatificationCandidateV1>,
    sideboard_admission_commitment_sha256: Option<String>,
}

fn validate_operator_resource_identity_v1(
    value: &MtgoCompetitiveOperatorResourceIdentityV1,
) -> Result<MtgoCompetitiveOperatorResourceCommitmentsV1, String> {
    if value.navigation.navigation_profile_commitment_sha256
        != value.navigation_profile_commitment_sha256
        || value
            .navigation
            .navigation_profile_admission_commitment_sha256
            != value.navigation_profile_admission_commitment_sha256
        || value.navigation.approved_account_alias_sha256 != value.approved_account_alias_sha256
    {
        return Err("competitive operator navigation profile and runtime are crossed".to_owned());
    }
    if value.listing.profile_commitment_sha256 != value.navigation_profile_commitment_sha256
        || value.listing.approved_account_alias_sha256 != value.approved_account_alias_sha256
        || value.record.navigation_profile_commitment_sha256
            != value.navigation_profile_commitment_sha256
        || value.record.approved_account_alias_sha256 != value.approved_account_alias_sha256
    {
        return Err("competitive operator event evaluations are crossed".to_owned());
    }
    if value.listing.deck_list_sha256 != value.deck_list_sha256
        || value.listing.deck_manifest_commitment_sha256 != value.deck_manifest_commitment_sha256
        || value.listing.deck_format_sha256 != value.deck_format_sha256
    {
        return Err("competitive operator deck manifest and listing are crossed".to_owned());
    }
    if value.perception.perception_profile_commitment_sha256
        != value.duel_perception_profile_commitment_sha256
        || value
            .perception
            .perception_profile_admission_commitment_sha256
            != value.duel_perception_profile_admission_commitment_sha256
        || value.duel_lifecycle_perception_profile_commitment_sha256
            != value.duel_perception_profile_commitment_sha256
        || value.duel_lifecycle_perception_admission_commitment_sha256
            != value.duel_perception_profile_admission_commitment_sha256
    {
        return Err(
            "competitive operator duel perception and lifecycle profiles are crossed".to_owned(),
        );
    }
    if value.duel_gesture_perception_admission_commitment_sha256
        != value.duel_perception_profile_admission_commitment_sha256
        || value.gesture.gesture_evaluation_commitment_sha256
            != value.duel_gesture_evaluation_commitment_sha256
        || value.gesture.gesture_profile_admission_commitment_sha256
            != value.duel_gesture_profile_admission_commitment_sha256
        || value.gesture.perception_profile_admission_commitment_sha256
            != value.duel_perception_profile_admission_commitment_sha256
    {
        return Err("competitive operator duel gesture resources are crossed".to_owned());
    }
    if value.listing.policy_deployment_commitment_sha256
        != value.policy_deployment_commitment_sha256
    {
        return Err(
            "competitive operator listing and checkpoint deployment are crossed".to_owned(),
        );
    }
    match (
        &value.sideboard,
        &value.sideboard_admission_commitment_sha256,
    ) {
        (Some(sideboard), Some(_))
            if sideboard.profile_commitment_sha256
                == value.navigation_profile_commitment_sha256
                && sideboard.approved_account_alias_sha256
                    == value.approved_account_alias_sha256
                && sideboard.deck_list_sha256 == value.listing.deck_list_sha256
                && sideboard.deck_manifest_commitment_sha256
                    == value.listing.deck_manifest_commitment_sha256
                && sideboard.deck_format_sha256 == value.listing.deck_format_sha256
                && sideboard.policy_deployment_commitment_sha256
                    == value.policy_deployment_commitment_sha256 => {}
        (None, None) => {}
        _ => return Err("competitive operator sideboard resources are crossed".to_owned()),
    }

    let mut commitments = MtgoCompetitiveOperatorResourceCommitmentsV1 {
        navigation_profile_commitment_sha256: value.navigation_profile_commitment_sha256.clone(),
        navigation_profile_admission_commitment_sha256: value
            .navigation_profile_admission_commitment_sha256
            .clone(),
        approved_account_alias_sha256: value.approved_account_alias_sha256.clone(),
        navigation_runtime_identity_commitment_sha256: value
            .navigation
            .runtime_identity_commitment_sha256
            .clone(),
        event_listing_evaluation_ratification_commitment_sha256: value
            .listing
            .ratification_commitment_sha256
            .clone(),
        event_listing_evaluation_admission_commitment_sha256: value
            .listing_admission_commitment_sha256
            .clone(),
        event_record_evaluation_ratification_commitment_sha256: value
            .record
            .ratification_commitment_sha256
            .clone(),
        event_record_evaluation_admission_commitment_sha256: value
            .record_admission_commitment_sha256
            .clone(),
        deck_list_sha256: value.listing.deck_list_sha256.clone(),
        deck_manifest_commitment_sha256: value.listing.deck_manifest_commitment_sha256.clone(),
        deck_format_sha256: value.listing.deck_format_sha256.clone(),
        policy_deployment_commitment_sha256: value.policy_deployment_commitment_sha256.clone(),
        duel_perception_profile_commitment_sha256: value
            .duel_perception_profile_commitment_sha256
            .clone(),
        duel_perception_profile_admission_commitment_sha256: value
            .duel_perception_profile_admission_commitment_sha256
            .clone(),
        duel_perception_runtime_identity_commitment_sha256: value
            .perception
            .runtime_identity_commitment_sha256
            .clone(),
        duel_lifecycle_evaluation_commitment_sha256: value
            .duel_lifecycle_evaluation_commitment_sha256
            .clone(),
        duel_lifecycle_admission_commitment_sha256: value
            .duel_lifecycle_admission_commitment_sha256
            .clone(),
        duel_gesture_evaluation_commitment_sha256: value
            .duel_gesture_evaluation_commitment_sha256
            .clone(),
        duel_gesture_profile_admission_commitment_sha256: value
            .duel_gesture_profile_admission_commitment_sha256
            .clone(),
        duel_gesture_runtime_identity_commitment_sha256: value
            .gesture
            .runtime_identity_commitment_sha256
            .clone(),
        changed_sideboard_evaluation_ratification_commitment_sha256: value
            .sideboard
            .as_ref()
            .map(|item| item.ratification_commitment_sha256.clone()),
        changed_sideboard_evaluation_admission_commitment_sha256: value
            .sideboard_admission_commitment_sha256
            .clone(),
        resource_bundle_commitment_sha256: String::new(),
    };
    validate_all_commitment_digests_v1(&commitments)?;
    commitments.resource_bundle_commitment_sha256 = resource_bundle_commitment_v1(&commitments)?;
    Ok(commitments)
}

fn validate_all_commitment_digests_v1(
    value: &MtgoCompetitiveOperatorResourceCommitmentsV1,
) -> Result<(), String> {
    for digest in [
        &value.navigation_profile_commitment_sha256,
        &value.navigation_profile_admission_commitment_sha256,
        &value.approved_account_alias_sha256,
        &value.navigation_runtime_identity_commitment_sha256,
        &value.event_listing_evaluation_ratification_commitment_sha256,
        &value.event_listing_evaluation_admission_commitment_sha256,
        &value.event_record_evaluation_ratification_commitment_sha256,
        &value.event_record_evaluation_admission_commitment_sha256,
        &value.deck_list_sha256,
        &value.deck_manifest_commitment_sha256,
        &value.deck_format_sha256,
        &value.policy_deployment_commitment_sha256,
        &value.duel_perception_profile_commitment_sha256,
        &value.duel_perception_profile_admission_commitment_sha256,
        &value.duel_perception_runtime_identity_commitment_sha256,
        &value.duel_lifecycle_evaluation_commitment_sha256,
        &value.duel_lifecycle_admission_commitment_sha256,
        &value.duel_gesture_evaluation_commitment_sha256,
        &value.duel_gesture_profile_admission_commitment_sha256,
        &value.duel_gesture_runtime_identity_commitment_sha256,
    ]
    .into_iter()
    .chain(
        value
            .changed_sideboard_evaluation_ratification_commitment_sha256
            .iter(),
    )
    .chain(
        value
            .changed_sideboard_evaluation_admission_commitment_sha256
            .iter(),
    ) {
        if !is_lower_sha256_v1(digest) {
            return Err("competitive operator resource contains an invalid digest".to_owned());
        }
    }
    Ok(())
}

fn resource_bundle_commitment_v1(
    value: &MtgoCompetitiveOperatorResourceCommitmentsV1,
) -> Result<String, String> {
    let bytes = serde_json::to_vec(value)
        .map_err(|error| format!("serialize competitive operator resources: {error}"))?;
    let mut hasher = Sha256::new();
    hasher.update(COMPETITIVE_OPERATOR_RESOURCE_BUNDLE_DOMAIN_V1);
    hasher.update((bytes.len() as u64).to_le_bytes());
    hasher.update(bytes);
    Ok(format!("{:x}", hasher.finalize()))
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

    fn digest(value: char) -> String {
        value.to_string().repeat(64)
    }

    fn identity_v1(with_sideboard: bool) -> MtgoCompetitiveOperatorResourceIdentityV1 {
        let profile = digest('1');
        let profile_admission = digest('2');
        let account = digest('3');
        let deck_list = digest('4');
        let deck_manifest = digest('5');
        let format = digest('6');
        let policy = digest('7');
        let perception_profile = digest('8');
        let perception_admission = digest('9');
        let gesture_evaluation = digest('a');
        let gesture_admission = digest('b');
        let sideboard = with_sideboard.then(|| {
            MtgoReviewedCompetitiveSideboardEvaluationRatificationCandidateV1 {
                profile_commitment_sha256: profile.clone(),
                approved_account_alias_sha256: account.clone(),
                deck_list_sha256: deck_list.clone(),
                deck_manifest_commitment_sha256: deck_manifest.clone(),
                deck_format_sha256: format.clone(),
                policy_deployment_commitment_sha256: policy.clone(),
                corpus_manifest_sha256: digest('c'),
                evaluation_commitment_sha256: digest('d'),
                ratification_commitment_sha256: digest('e'),
            }
        });
        MtgoCompetitiveOperatorResourceIdentityV1 {
            navigation_profile_commitment_sha256: profile.clone(),
            navigation_profile_admission_commitment_sha256: profile_admission.clone(),
            approved_account_alias_sha256: account.clone(),
            navigation: MtgoVerifiedCompetitiveNavigationClassifierRuntimeCommitmentsV1 {
                navigation_profile_commitment_sha256: profile.clone(),
                navigation_profile_admission_commitment_sha256: profile_admission,
                approved_account_alias_sha256: account.clone(),
                classifier_binary_sha256: digest('f'),
                classifier_assets_manifest_sha256: digest('0'),
                runtime_identity_commitment_sha256: digest('a'),
            },
            listing: MtgoReviewedCompetitiveEventListingEvaluationRatificationCandidateV1 {
                profile_commitment_sha256: profile.clone(),
                approved_account_alias_sha256: account.clone(),
                deck_list_sha256: deck_list.clone(),
                deck_manifest_commitment_sha256: deck_manifest.clone(),
                deck_format_sha256: format.clone(),
                policy_deployment_commitment_sha256: policy.clone(),
                corpus_manifest_sha256: digest('b'),
                evaluation_commitment_sha256: digest('c'),
                ratification_commitment_sha256: digest('d'),
            },
            listing_admission_commitment_sha256: digest('e'),
            record: MtgoReviewedCompetitiveEventRecordEvaluationRatificationCandidateV1 {
                navigation_profile_commitment_sha256: profile.clone(),
                approved_account_alias_sha256: account.clone(),
                corpus_manifest_sha256: digest('f'),
                evaluation_commitment_sha256: digest('0'),
                ratification_commitment_sha256: digest('1'),
            },
            record_admission_commitment_sha256: digest('2'),
            deck_list_sha256: deck_list.clone(),
            deck_manifest_commitment_sha256: deck_manifest.clone(),
            deck_format_sha256: format.clone(),
            duel_perception_profile_commitment_sha256: perception_profile.clone(),
            duel_perception_profile_admission_commitment_sha256: perception_admission.clone(),
            perception: MtgoVerifiedDuelPerceptionRuntimeCommitmentsV1 {
                perception_profile_commitment_sha256: perception_profile.clone(),
                perception_profile_admission_commitment_sha256: perception_admission.clone(),
                perception_pipeline_binary_sha256: digest('3'),
                classifier_assets_manifest_sha256: digest('4'),
                card_database_profile_sha256: digest('5'),
                runtime_identity_commitment_sha256: digest('6'),
            },
            duel_lifecycle_perception_profile_commitment_sha256: perception_profile,
            duel_lifecycle_perception_admission_commitment_sha256: perception_admission.clone(),
            duel_lifecycle_evaluation_commitment_sha256: digest('7'),
            duel_lifecycle_admission_commitment_sha256: digest('8'),
            duel_gesture_evaluation_commitment_sha256: gesture_evaluation.clone(),
            duel_gesture_profile_admission_commitment_sha256: gesture_admission.clone(),
            duel_gesture_perception_admission_commitment_sha256: perception_admission.clone(),
            gesture: MtgoVerifiedDuelGestureTargetRuntimeCommitmentsV1 {
                gesture_evaluation_commitment_sha256: gesture_evaluation,
                gesture_profile_admission_commitment_sha256: gesture_admission,
                perception_profile_admission_commitment_sha256: perception_admission,
                gesture_target_runtime_binary_sha256: digest('9'),
                gesture_target_assets_manifest_sha256: digest('a'),
                runtime_identity_commitment_sha256: digest('b'),
            },
            policy_deployment_commitment_sha256: policy,
            sideboard,
            sideboard_admission_commitment_sha256: with_sideboard.then(|| digest('f')),
        }
    }

    #[test]
    fn exact_resources_bind_with_or_without_changed_sideboarding() {
        let unchanged = validate_operator_resource_identity_v1(&identity_v1(false)).unwrap();
        assert!(unchanged
            .changed_sideboard_evaluation_ratification_commitment_sha256
            .is_none());
        assert!(is_lower_sha256_v1(
            &unchanged.resource_bundle_commitment_sha256
        ));
        let changed = validate_operator_resource_identity_v1(&identity_v1(true)).unwrap();
        assert!(changed
            .changed_sideboard_evaluation_ratification_commitment_sha256
            .is_some());
        assert_ne!(
            unchanged.resource_bundle_commitment_sha256,
            changed.resource_bundle_commitment_sha256
        );
    }

    #[test]
    fn every_cross_resource_substitution_is_rejected() {
        let mut navigation = identity_v1(false);
        navigation.navigation.approved_account_alias_sha256 = digest('0');
        assert!(validate_operator_resource_identity_v1(&navigation).is_err());

        let mut listing = identity_v1(false);
        listing.listing.policy_deployment_commitment_sha256 = digest('0');
        assert!(validate_operator_resource_identity_v1(&listing).is_err());

        let mut lifecycle = identity_v1(false);
        lifecycle.duel_lifecycle_perception_admission_commitment_sha256 = digest('0');
        assert!(validate_operator_resource_identity_v1(&lifecycle).is_err());

        let mut deck = identity_v1(false);
        deck.deck_manifest_commitment_sha256 = digest('0');
        assert!(validate_operator_resource_identity_v1(&deck).is_err());

        let mut gesture = identity_v1(false);
        gesture.gesture.gesture_profile_admission_commitment_sha256 = digest('0');
        assert!(validate_operator_resource_identity_v1(&gesture).is_err());

        let mut sideboard = identity_v1(true);
        sideboard
            .sideboard
            .as_mut()
            .unwrap()
            .deck_manifest_commitment_sha256 = digest('0');
        assert!(validate_operator_resource_identity_v1(&sideboard).is_err());

        let mut malformed = identity_v1(false);
        malformed.record_admission_commitment_sha256 = "A".repeat(64);
        assert!(validate_operator_resource_identity_v1(&malformed).is_err());
    }
}
