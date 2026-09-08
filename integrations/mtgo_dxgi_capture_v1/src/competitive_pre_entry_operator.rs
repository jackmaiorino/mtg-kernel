use crate::competitive_operator_bootstrap::{
    MtgoCompetitiveOperatorResourceCommitmentsV1, OpaqueMtgoCompetitiveOperatorResourcesV1,
};
use crate::competitive_operator_loop::{
    begin_competitive_post_entry_operator_v1, OpaqueMtgoCompetitivePostEntryOperatorV1,
};
use crate::{
    begin_competitive_event_runtime_after_entry_v1,
    bind_confirmed_competitive_open_entry_review_to_entry_review_v1,
    confirm_pending_competitive_entry_v1, confirm_pending_competitive_open_entry_review_v1,
    execute_prepared_competitive_entry_v1, execute_prepared_competitive_open_entry_review_v1,
    prepare_ratified_competitive_entry_v1, prepare_ratified_competitive_open_entry_review_v1,
    ratify_competitive_entry_authorization_from_selected_listing_v2,
    CheckedUntrustedMtgoControlBoundCompetitiveEntryReviewV4,
    CheckedUntrustedMtgoSelectedListingBoundCompetitiveEntryReviewV1,
    MtgoCompetitiveEntryInputReceiptCommitmentsV1, MtgoCompetitiveNavigationFrameIdentityV1,
    MtgoCompetitiveOpenEntryReviewInputReceiptCommitmentsV1,
    MtgoConfirmedCompetitiveEntryCommitmentsV1,
    MtgoConfirmedCompetitiveOpenEntryReviewCommitmentsV1,
    MtgoEvaluatedCompetitiveEventListingCommitmentsV1, MtgoPreparedCompetitiveEntryCommitmentsV1,
    MtgoPreparedCompetitiveOpenEntryReviewCommitmentsV1,
    MtgoReviewedCompetitiveEntryRatificationCandidateV1,
    MtgoReviewedSelectedListingCompetitiveEntryRatificationCandidateV2,
    MtgoSelectedListingBoundCompetitiveEntryReviewCommitmentsV1,
    OpaqueMtgoClassifiedCompetitiveNavigationFrameV1, OpaqueMtgoConfirmedCompetitiveEntryV1,
    OpaqueMtgoConfirmedCompetitiveOpenEntryReviewV1, OpaqueMtgoEvaluatedCompetitiveEventListingV1,
    OpaqueMtgoPendingCompetitiveEntryV1, OpaqueMtgoPendingCompetitiveOpenEntryReviewV1,
    OpaqueMtgoPreparedCompetitiveEntryV1, OpaqueMtgoPreparedCompetitiveOpenEntryReviewV1,
    RatifiedMtgoCompetitiveEntryAuthorizationV1, RatifiedMtgoCompetitiveLifecycleAuthorizationV1,
    RatifiedMtgoCompetitiveOpenEntryReviewAuthorizationV1,
};

/// Move-only ownership of the exact operator resources while the selected
/// listing's Open Entry Review click is prepared. The underlying operation is
/// still separately correspondence-ratified and cannot confirm a paid entry.
pub struct OpaqueMtgoPreparedCompetitiveOperatorOpenEntryReviewV1 {
    resources: OpaqueMtgoCompetitiveOperatorResourcesV1,
    prepared: OpaqueMtgoPreparedCompetitiveOpenEntryReviewV1,
}

impl OpaqueMtgoPreparedCompetitiveOperatorOpenEntryReviewV1 {
    pub fn resource_commitments_v1(&self) -> MtgoCompetitiveOperatorResourceCommitmentsV1 {
        self.resources.commitments_v1()
    }

    pub fn open_review_commitments_v1(
        &self,
    ) -> MtgoPreparedCompetitiveOpenEntryReviewCommitmentsV1 {
        self.prepared.commitments_v1()
    }

    pub fn permits_event_entry_v1(&self) -> bool {
        false
    }

    pub fn permits_spending_v1(&self) -> bool {
        false
    }
}

/// Move-only ownership after exactly one Open Entry Review click. The process
/// gate remains pending until the exact newer Entry Review arrival is visible.
pub struct OpaqueMtgoPendingCompetitiveOperatorOpenEntryReviewV1 {
    resources: OpaqueMtgoCompetitiveOperatorResourcesV1,
    pending: OpaqueMtgoPendingCompetitiveOpenEntryReviewV1,
}

impl OpaqueMtgoPendingCompetitiveOperatorOpenEntryReviewV1 {
    pub fn resource_commitments_v1(&self) -> MtgoCompetitiveOperatorResourceCommitmentsV1 {
        self.resources.commitments_v1()
    }

    pub fn input_commitments_v1(&self) -> MtgoCompetitiveOpenEntryReviewInputReceiptCommitmentsV1 {
        self.pending.commitments_v1()
    }

    pub fn safe_for_next_input_v1(&self) -> bool {
        false
    }
}

/// Move-only ownership at the exact visibly confirmed Entry Review surface.
/// This grants no authority to accept its paid terms.
pub struct OpaqueMtgoConfirmedCompetitiveOperatorOpenEntryReviewV1 {
    resources: OpaqueMtgoCompetitiveOperatorResourcesV1,
    confirmed: OpaqueMtgoConfirmedCompetitiveOpenEntryReviewV1,
}

impl OpaqueMtgoConfirmedCompetitiveOperatorOpenEntryReviewV1 {
    pub fn resource_commitments_v1(&self) -> MtgoCompetitiveOperatorResourceCommitmentsV1 {
        self.resources.commitments_v1()
    }

    pub fn open_review_commitments_v1(
        &self,
    ) -> MtgoConfirmedCompetitiveOpenEntryReviewCommitmentsV1 {
        self.confirmed.commitments_v1()
    }

    pub fn permits_event_entry_v1(&self) -> bool {
        false
    }

    pub fn permits_spending_v1(&self) -> bool {
        false
    }
}

/// One owner-attended paid Entry Review bound to the selected listing and the
/// exact operator bundle. Its candidate remains checked-untrusted and cannot
/// enter or spend until the separate exact-entry production root is pinned.
pub struct CheckedUntrustedMtgoCompetitiveOperatorPaidEntryReviewV1 {
    resources: OpaqueMtgoCompetitiveOperatorResourcesV1,
    review: CheckedUntrustedMtgoSelectedListingBoundCompetitiveEntryReviewV1,
}

impl CheckedUntrustedMtgoCompetitiveOperatorPaidEntryReviewV1 {
    pub fn resource_commitments_v1(&self) -> MtgoCompetitiveOperatorResourceCommitmentsV1 {
        self.resources.commitments_v1()
    }

    pub fn entry_review_commitments_v1(
        &self,
    ) -> MtgoSelectedListingBoundCompetitiveEntryReviewCommitmentsV1 {
        self.review.commitments_v1()
    }

    pub fn ratification_candidate_v2(
        &self,
    ) -> MtgoReviewedSelectedListingCompetitiveEntryRatificationCandidateV2 {
        self.review.ratification_candidate_v2()
    }

    pub fn permits_event_entry_v1(&self) -> bool {
        false
    }

    pub fn permits_spending_v1(&self) -> bool {
        false
    }
}

/// Exact compile-ratified paid-entry authority retained with the same operator
/// resources. Construction remains impossible while the production root is
/// empty. The value still cannot send input without immediate recapture.
pub struct RatifiedMtgoCompetitiveOperatorEntryV1 {
    resources: OpaqueMtgoCompetitiveOperatorResourcesV1,
    authorization: RatifiedMtgoCompetitiveEntryAuthorizationV1,
}

impl RatifiedMtgoCompetitiveOperatorEntryV1 {
    pub fn resource_commitments_v1(&self) -> MtgoCompetitiveOperatorResourceCommitmentsV1 {
        self.resources.commitments_v1()
    }

    pub fn entry_authorization_commitments_v1(
        &self,
    ) -> MtgoReviewedCompetitiveEntryRatificationCandidateV1 {
        self.authorization.commitments_v1()
    }

    pub fn safe_for_live_input_v1(&self) -> bool {
        false
    }
}

pub struct OpaqueMtgoPreparedCompetitiveOperatorEntryV1 {
    resources: OpaqueMtgoCompetitiveOperatorResourcesV1,
    prepared: OpaqueMtgoPreparedCompetitiveEntryV1,
}

impl OpaqueMtgoPreparedCompetitiveOperatorEntryV1 {
    pub fn resource_commitments_v1(&self) -> MtgoCompetitiveOperatorResourceCommitmentsV1 {
        self.resources.commitments_v1()
    }

    pub fn entry_commitments_v1(&self) -> MtgoPreparedCompetitiveEntryCommitmentsV1 {
        self.prepared.commitments_v1()
    }

    pub fn permits_spending_v1(&self) -> bool {
        false
    }
}

pub struct OpaqueMtgoPendingCompetitiveOperatorEntryV1 {
    resources: OpaqueMtgoCompetitiveOperatorResourcesV1,
    pending: OpaqueMtgoPendingCompetitiveEntryV1,
}

impl OpaqueMtgoPendingCompetitiveOperatorEntryV1 {
    pub fn resource_commitments_v1(&self) -> MtgoCompetitiveOperatorResourceCommitmentsV1 {
        self.resources.commitments_v1()
    }

    pub fn input_commitments_v1(&self) -> MtgoCompetitiveEntryInputReceiptCommitmentsV1 {
        self.pending.commitments_v1()
    }

    pub fn safe_for_next_input_v1(&self) -> bool {
        false
    }
}

pub struct OpaqueMtgoConfirmedCompetitiveOperatorEntryV1 {
    resources: OpaqueMtgoCompetitiveOperatorResourcesV1,
    confirmed: OpaqueMtgoConfirmedCompetitiveEntryV1,
}

impl OpaqueMtgoConfirmedCompetitiveOperatorEntryV1 {
    pub fn resource_commitments_v1(&self) -> MtgoCompetitiveOperatorResourceCommitmentsV1 {
        self.resources.commitments_v1()
    }

    pub fn entry_commitments_v1(&self) -> MtgoConfirmedCompetitiveEntryCommitmentsV1 {
        self.confirmed.commitments_v1()
    }

    pub fn permits_additional_entry_v1(&self) -> bool {
        false
    }

    pub fn permits_additional_spending_v1(&self) -> bool {
        false
    }
}

/// Starts the pre-entry ownership chain from one already classified and
/// evaluated visible listing. The listing, the Open Entry Review permission,
/// and the retained operator bundle must agree on account, navigation runtime,
/// evaluation, deck list, deck manifest, format, policy, and one event mode.
pub fn prepare_competitive_operator_open_entry_review_v1(
    resources: OpaqueMtgoCompetitiveOperatorResourcesV1,
    authorization: RatifiedMtgoCompetitiveOpenEntryReviewAuthorizationV1,
    evaluated_listing: OpaqueMtgoEvaluatedCompetitiveEventListingV1,
) -> Result<OpaqueMtgoPreparedCompetitiveOperatorOpenEntryReviewV1, String> {
    validate_operator_open_review_join_v1(
        &resources.commitments_v1(),
        &authorization.commitments_v1(),
        &evaluated_listing.commitments_v1(),
    )?;
    let prepared =
        prepare_ratified_competitive_open_entry_review_v1(authorization, evaluated_listing)?;
    Ok(OpaqueMtgoPreparedCompetitiveOperatorOpenEntryReviewV1 {
        resources,
        prepared,
    })
}

pub fn execute_prepared_competitive_operator_open_entry_review_v1(
    value: OpaqueMtgoPreparedCompetitiveOperatorOpenEntryReviewV1,
) -> Result<OpaqueMtgoPendingCompetitiveOperatorOpenEntryReviewV1, String> {
    let pending = execute_prepared_competitive_open_entry_review_v1(value.prepared)?;
    Ok(OpaqueMtgoPendingCompetitiveOperatorOpenEntryReviewV1 {
        resources: value.resources,
        pending,
    })
}

pub fn confirm_pending_competitive_operator_open_entry_review_v1(
    value: OpaqueMtgoPendingCompetitiveOperatorOpenEntryReviewV1,
    after: OpaqueMtgoClassifiedCompetitiveNavigationFrameV1,
) -> Result<OpaqueMtgoConfirmedCompetitiveOperatorOpenEntryReviewV1, String> {
    let pending_commitments = value.pending.commitments_v1();
    let confirmed = confirm_pending_competitive_open_entry_review_v1(value.pending, after)?;
    let confirmed_commitments = confirmed.commitments_v1();
    if confirmed_commitments.event_kind != pending_commitments.event_kind
        || confirmed_commitments.event_identity_sha256 != pending_commitments.event_identity_sha256
        || confirmed_commitments.after_frame_sequence <= pending_commitments.source_frame_sequence
    {
        return Err(
            "competitive Open Entry Review confirmation changed the pre-entry lineage".to_owned(),
        );
    }
    Ok(OpaqueMtgoConfirmedCompetitiveOperatorOpenEntryReviewV1 {
        resources: value.resources,
        confirmed,
    })
}

pub fn bind_competitive_operator_paid_entry_review_v1(
    value: OpaqueMtgoConfirmedCompetitiveOperatorOpenEntryReviewV1,
    review: CheckedUntrustedMtgoControlBoundCompetitiveEntryReviewV4,
    visible_account_alias: String,
) -> Result<CheckedUntrustedMtgoCompetitiveOperatorPaidEntryReviewV1, String> {
    let review = bind_confirmed_competitive_open_entry_review_to_entry_review_v1(
        value.confirmed,
        review,
        visible_account_alias,
    )?;
    validate_operator_paid_review_join_v1(
        &value.resources.commitments_v1(),
        &review.commitments_v1(),
    )?;
    Ok(CheckedUntrustedMtgoCompetitiveOperatorPaidEntryReviewV1 {
        resources: value.resources,
        review,
    })
}

/// Consumes the exact owner-reviewed terms through the existing empty-root
/// ratifier. No runtime boolean or broad League-plus-Challenge claim can make
/// this succeed.
pub fn ratify_competitive_operator_paid_entry_v1(
    value: CheckedUntrustedMtgoCompetitiveOperatorPaidEntryReviewV1,
) -> Result<RatifiedMtgoCompetitiveOperatorEntryV1, String> {
    let authorization =
        ratify_competitive_entry_authorization_from_selected_listing_v2(value.review)?;
    validate_operator_entry_authorization_join_v1(
        &value.resources.commitments_v1(),
        &authorization.commitments_v1(),
    )?;
    Ok(RatifiedMtgoCompetitiveOperatorEntryV1 {
        resources: value.resources,
        authorization,
    })
}

pub fn prepare_ratified_competitive_operator_entry_v1(
    value: RatifiedMtgoCompetitiveOperatorEntryV1,
    immediate_identity: MtgoCompetitiveNavigationFrameIdentityV1,
    capture_timeout_ms: u32,
    classifier_timeout_ms: u32,
) -> Result<OpaqueMtgoPreparedCompetitiveOperatorEntryV1, String> {
    let prepared = prepare_ratified_competitive_entry_v1(
        value.authorization,
        value.resources.navigation_profile_v1(),
        value.resources.navigation_runtime_v1(),
        immediate_identity,
        capture_timeout_ms,
        classifier_timeout_ms,
    )?;
    validate_operator_prepared_entry_v1(
        &value.resources.commitments_v1(),
        &prepared.commitments_v1(),
    )?;
    Ok(OpaqueMtgoPreparedCompetitiveOperatorEntryV1 {
        resources: value.resources,
        prepared,
    })
}

pub fn execute_prepared_competitive_operator_entry_v1(
    value: OpaqueMtgoPreparedCompetitiveOperatorEntryV1,
) -> Result<OpaqueMtgoPendingCompetitiveOperatorEntryV1, String> {
    let pending = execute_prepared_competitive_entry_v1(value.prepared)?;
    Ok(OpaqueMtgoPendingCompetitiveOperatorEntryV1 {
        resources: value.resources,
        pending,
    })
}

pub fn confirm_pending_competitive_operator_entry_v1(
    value: OpaqueMtgoPendingCompetitiveOperatorEntryV1,
    timeout_ms: u32,
) -> Result<OpaqueMtgoConfirmedCompetitiveOperatorEntryV1, String> {
    let confirmed = confirm_pending_competitive_entry_v1(
        value.pending,
        value.resources.navigation_profile_v1(),
        value.resources.navigation_runtime_v1(),
        timeout_ms,
    )?;
    validate_operator_confirmed_entry_v1(
        &value.resources.commitments_v1(),
        &confirmed.commitments_v1(),
    )?;
    Ok(OpaqueMtgoConfirmedCompetitiveOperatorEntryV1 {
        resources: value.resources,
        confirmed,
    })
}

/// Hands one exact visibly confirmed entry into the existing post-entry
/// operator. The event runtime and resource bundle are cross-checked again at
/// that final join. This function cannot authorize another entry or spend.
pub fn begin_competitive_post_entry_operator_from_confirmed_entry_v1(
    value: OpaqueMtgoConfirmedCompetitiveOperatorEntryV1,
    lifecycle_authorization: RatifiedMtgoCompetitiveLifecycleAuthorizationV1,
) -> Result<OpaqueMtgoCompetitivePostEntryOperatorV1, String> {
    let runtime = begin_competitive_event_runtime_after_entry_v1(
        value.confirmed,
        lifecycle_authorization,
        value.resources.deck_manifest_v1(),
    )?;
    begin_competitive_post_entry_operator_v1(value.resources, runtime)
}

fn validate_operator_open_review_join_v1(
    resources: &MtgoCompetitiveOperatorResourceCommitmentsV1,
    authorization: &crate::MtgoReviewedCompetitiveOpenEntryReviewRatificationCandidateV1,
    evaluated: &MtgoEvaluatedCompetitiveEventListingCommitmentsV1,
) -> Result<(), String> {
    let listing = &evaluated.classified_listing.source_listing;
    if resources.navigation_profile_commitment_sha256
        != listing
            .source_navigation
            .source_frame
            .profile_commitment_sha256
        || resources.navigation_profile_admission_commitment_sha256
            != listing
                .source_navigation
                .source_frame
                .profile_admission_commitment_sha256
        || resources.approved_account_alias_sha256
            != listing
                .source_navigation
                .source_frame
                .approved_account_alias_sha256
        || resources.navigation_runtime_identity_commitment_sha256
            != evaluated
                .classified_listing
                .runtime_identity_commitment_sha256
        || resources.event_listing_evaluation_ratification_commitment_sha256
            != evaluated.evaluation_ratification_commitment_sha256
        || resources.event_listing_evaluation_admission_commitment_sha256
            != evaluated.evaluation_admission_commitment_sha256
        || resources.deck_list_sha256 != listing.deck_list_sha256
        || resources.deck_manifest_commitment_sha256 != listing.deck_manifest_commitment_sha256
        || resources.deck_format_sha256 != listing.deck_format_sha256
        || resources.policy_deployment_commitment_sha256
            != listing.policy_deployment_commitment_sha256
        || authorization.navigation_profile_commitment_sha256
            != resources.navigation_profile_commitment_sha256
        || authorization.navigation_profile_admission_commitment_sha256
            != resources.navigation_profile_admission_commitment_sha256
        || authorization.approved_account_alias_sha256 != resources.approved_account_alias_sha256
        || authorization.listing_evaluation_ratification_commitment_sha256
            != resources.event_listing_evaluation_ratification_commitment_sha256
        || authorization.listing_evaluation_admission_commitment_sha256
            != resources.event_listing_evaluation_admission_commitment_sha256
        || authorization.deck_list_sha256 != resources.deck_list_sha256
        || authorization.deck_manifest_commitment_sha256
            != resources.deck_manifest_commitment_sha256
        || authorization.deck_format_sha256 != resources.deck_format_sha256
        || authorization.policy_deployment_commitment_sha256
            != resources.policy_deployment_commitment_sha256
        || authorization.event_kind != listing.event_kind
    {
        return Err(
            "competitive pre-entry resources, evaluated listing, and Open Entry Review authority are crossed"
                .to_owned(),
        );
    }
    Ok(())
}

fn validate_operator_paid_review_join_v1(
    resources: &MtgoCompetitiveOperatorResourceCommitmentsV1,
    review: &MtgoSelectedListingBoundCompetitiveEntryReviewCommitmentsV1,
) -> Result<(), String> {
    if review.navigation_profile_commitment_sha256 != resources.navigation_profile_commitment_sha256
        || review.navigation_profile_admission_commitment_sha256
            != resources.navigation_profile_admission_commitment_sha256
        || review.approved_account_alias_sha256 != resources.approved_account_alias_sha256
        || review.listing_evaluation_admission_commitment_sha256
            != resources.event_listing_evaluation_admission_commitment_sha256
        || review.deck_list_sha256 != resources.deck_list_sha256
        || review.deck_manifest_commitment_sha256 != resources.deck_manifest_commitment_sha256
        || review.deck_format_sha256 != resources.deck_format_sha256
        || review.policy_deployment_commitment_sha256
            != resources.policy_deployment_commitment_sha256
    {
        return Err(
            "competitive paid Entry Review changed the exact operator resource lineage".to_owned(),
        );
    }
    Ok(())
}

fn validate_operator_entry_authorization_join_v1(
    resources: &MtgoCompetitiveOperatorResourceCommitmentsV1,
    authorization: &MtgoReviewedCompetitiveEntryRatificationCandidateV1,
) -> Result<(), String> {
    if authorization.account_alias_sha256 != resources.approved_account_alias_sha256
        || authorization.deck_manifest_sha256 != resources.deck_manifest_commitment_sha256
        || authorization.deck_format_sha256 != resources.deck_format_sha256
        || authorization.policy_deployment_commitment_sha256
            != resources.policy_deployment_commitment_sha256
    {
        return Err("competitive paid-entry authority changed operator resources".to_owned());
    }
    Ok(())
}

fn validate_operator_prepared_entry_v1(
    resources: &MtgoCompetitiveOperatorResourceCommitmentsV1,
    prepared: &MtgoPreparedCompetitiveEntryCommitmentsV1,
) -> Result<(), String> {
    let recapture = &prepared.immediate_recapture;
    if recapture.navigation_profile_commitment_sha256
        != resources.navigation_profile_commitment_sha256
        || recapture.navigation_profile_admission_commitment_sha256
            != resources.navigation_profile_admission_commitment_sha256
        || recapture.approved_account_alias_sha256 != resources.approved_account_alias_sha256
        || recapture.runtime_identity_commitment_sha256
            != resources.navigation_runtime_identity_commitment_sha256
        || recapture.deck_manifest_sha256 != resources.deck_manifest_commitment_sha256
        || recapture.deck_format_sha256 != resources.deck_format_sha256
        || recapture.policy_deployment_commitment_sha256
            != resources.policy_deployment_commitment_sha256
    {
        return Err("competitive entry recapture changed operator resources".to_owned());
    }
    Ok(())
}

fn validate_operator_confirmed_entry_v1(
    _resources: &MtgoCompetitiveOperatorResourceCommitmentsV1,
    confirmed: &MtgoConfirmedCompetitiveEntryCommitmentsV1,
) -> Result<(), String> {
    if confirmed.after_frame_sequence == 0 || confirmed.postcondition_candidate_count == 0 {
        return Err("competitive entry confirmation is incomplete".to_owned());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use mtgo_blackbox_v1::{MtgoCompetitiveEntryResourceV1, MtgoCompetitiveEventKindV1};

    fn digest(value: char) -> String {
        value.to_string().repeat(64)
    }

    fn resources_v1() -> MtgoCompetitiveOperatorResourceCommitmentsV1 {
        MtgoCompetitiveOperatorResourceCommitmentsV1 {
            navigation_profile_commitment_sha256: digest('0'),
            navigation_profile_admission_commitment_sha256: digest('1'),
            approved_account_alias_sha256: digest('2'),
            navigation_runtime_identity_commitment_sha256: digest('3'),
            event_listing_evaluation_ratification_commitment_sha256: digest('4'),
            event_listing_evaluation_admission_commitment_sha256: digest('5'),
            event_record_evaluation_ratification_commitment_sha256: digest('6'),
            event_record_evaluation_admission_commitment_sha256: digest('7'),
            deck_list_sha256: digest('8'),
            deck_manifest_commitment_sha256: digest('9'),
            deck_format_sha256: digest('a'),
            policy_deployment_commitment_sha256: digest('b'),
            checkpoint_competitive_capabilities_commitment_sha256: digest('d'),
            duel_perception_profile_commitment_sha256: digest('c'),
            duel_perception_profile_admission_commitment_sha256: digest('d'),
            duel_perception_runtime_identity_commitment_sha256: digest('e'),
            duel_lifecycle_evaluation_commitment_sha256: digest('f'),
            duel_lifecycle_admission_commitment_sha256: digest('0'),
            duel_gesture_evaluation_commitment_sha256: digest('1'),
            duel_gesture_profile_admission_commitment_sha256: digest('2'),
            duel_gesture_runtime_identity_commitment_sha256: digest('3'),
            changed_sideboard_evaluation_ratification_commitment_sha256: None,
            changed_sideboard_evaluation_admission_commitment_sha256: None,
            resource_bundle_commitment_sha256: digest('c'),
        }
    }

    fn paid_review_v1() -> MtgoSelectedListingBoundCompetitiveEntryReviewCommitmentsV1 {
        MtgoSelectedListingBoundCompetitiveEntryReviewCommitmentsV1 {
            open_entry_review_authorization_ratification_commitment_sha256: digest('0'),
            open_entry_review_confirmation_receipt_sha256: digest('1'),
            open_entry_review_visible_confirmation_commitment_sha256: digest('2'),
            control_bound_entry_review_commitment_sha256: digest('3'),
            legacy_entry_ratification_candidate_commitment_sha256: digest('4'),
            correspondence_sha256: digest('5'),
            permission_review_commitment_sha256: digest('6'),
            mode_authorization_commitment_sha256: digest('7'),
            approved_account_alias_sha256: digest('2'),
            navigation_profile_commitment_sha256: digest('0'),
            navigation_profile_admission_commitment_sha256: digest('1'),
            listing_evaluation_admission_commitment_sha256: digest('5'),
            event_kind: MtgoCompetitiveEventKindV1::League,
            event_identity_sha256: digest('d'),
            deck_list_sha256: digest('8'),
            deck_manifest_commitment_sha256: digest('9'),
            deck_format_sha256: digest('a'),
            policy_deployment_commitment_sha256: digest('b'),
            entry_terms_sha256: digest('e'),
            resource: MtgoCompetitiveEntryResourceV1::ExistingPlayPoints,
            amount: 100,
            open_arrival_frame_id: 1,
            open_arrival_frame_sequence: 2,
            open_arrival_captured_at_unix_millis: 3,
            entry_review_frame_id: 2,
            entry_review_frame_sequence: 3,
            entry_review_captured_at_unix_millis: 4,
            entry_review_source_capture_commitment_sha256: digest('f'),
            entry_review_window_continuity_commitment_sha256: digest('0'),
            binding_commitment_sha256: digest('1'),
        }
    }

    #[test]
    fn paid_review_accepts_both_exact_modes_and_rejects_resource_substitution() {
        let resources = resources_v1();
        let mut review = paid_review_v1();
        validate_operator_paid_review_join_v1(&resources, &review).unwrap();
        review.event_kind = MtgoCompetitiveEventKindV1::Challenge;
        validate_operator_paid_review_join_v1(&resources, &review).unwrap();

        for mutate in [
            |value: &mut MtgoSelectedListingBoundCompetitiveEntryReviewCommitmentsV1| {
                value.approved_account_alias_sha256 = digest('f')
            },
            |value: &mut MtgoSelectedListingBoundCompetitiveEntryReviewCommitmentsV1| {
                value.deck_list_sha256 = digest('f')
            },
            |value: &mut MtgoSelectedListingBoundCompetitiveEntryReviewCommitmentsV1| {
                value.deck_manifest_commitment_sha256 = digest('f')
            },
            |value: &mut MtgoSelectedListingBoundCompetitiveEntryReviewCommitmentsV1| {
                value.deck_format_sha256 = digest('f')
            },
            |value: &mut MtgoSelectedListingBoundCompetitiveEntryReviewCommitmentsV1| {
                value.policy_deployment_commitment_sha256 = digest('f')
            },
        ] {
            let mut crossed = paid_review_v1();
            mutate(&mut crossed);
            assert!(validate_operator_paid_review_join_v1(&resources, &crossed).is_err());
        }
    }
}
