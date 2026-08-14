use super::{
    competitive_entry_window_continuity_commitment_for_frame_v1,
    competitive_navigation_classifier_assets_manifest_bytes_v1,
    invoke_verified_competitive_event_listing_classifier_process_v1,
    resolve_admitted_competitive_navigation_pointer_target_v1, sha256_hex_v1,
    verify_runtime_identity_now_v1, MtgoClassifiedCompetitiveNavigationFrameCommitmentsV1,
    MtgoCompetitiveEntryPointerTargetV1, OpaqueMtgoAdmittedCompetitiveNavigationFrameV1,
    OpaqueMtgoClassifiedCompetitiveNavigationFrameV1,
    OpaqueMtgoRetainedCompetitiveNavigationClassificationV1,
    OpaqueMtgoVerifiedCompetitiveNavigationClassifierRuntimeV1,
};
use mtgo_blackbox_v1::{
    competitive_event_listing_target_commitment_v1,
    confirm_checked_competitive_event_listing_opened_v1,
    make_offline_competitive_event_listing_open_intent_v1,
    validate_visible_competitive_event_listing_selection_v1,
    visible_frame_region_content_sha256_v1, AdmittedMtgoCompetitiveEventListingEvaluationV1,
    CheckedUntrustedMtgoCompetitiveEntryReviewArrivalV1,
    CheckedUntrustedMtgoCompetitiveEventListingOpenIntentV1,
    CheckedUntrustedMtgoCompetitiveEventListingSelectionV1,
    CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1, MtgoAuthorizationScopeV1,
    MtgoCompetitiveEventKindV1, MtgoCompetitiveEventListingTargetV1,
    MtgoCompetitiveLifecyclePhaseV1, MtgoRectPxV1, MtgoSizePxV1,
    MtgoVisibleCompetitiveEventListingSelectionV1, ValidatedMtgoCompetitiveDeckManifestV1,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::time::Duration;

const SOURCE_BOUND_EVENT_LISTING_DOMAIN_V1: &[u8] =
    b"mtgo-source-bound-competitive-event-listing-v1";
const EVENT_LISTING_CLASSIFIER_REQUEST_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-event-listing-classifier-request-v1";
const EVENT_LISTING_CLASSIFIER_RESULT_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-event-listing-classifier-result-v1";
const EVALUATED_EVENT_LISTING_BINDING_DOMAIN_V1: &[u8] =
    b"mtgo-evaluated-competitive-event-listing-binding-v1";
const PREPARED_EVENT_LISTING_OPEN_SOURCE_DOMAIN_V1: &[u8] =
    b"mtgo-prepared-competitive-event-listing-open-source-v1";
const EVENT_LISTING_OPEN_VISIBLE_CONFIRMATION_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-event-listing-open-visible-confirmation-v1";
const MAX_EVENT_LISTING_REQUEST_HEADER_BYTES_V1: usize = 1024 * 1024;
const MAX_EVENT_LISTING_ASSETS_MANIFEST_BYTES_V1: usize = 16 * 1024 * 1024;

/// Canonical header followed by exact classifier assets and tightly packed
/// BGRA8 pixels in the private selected-listing parser protocol.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoCompetitiveEventListingClassifierRequestHeaderV1 {
    pub schema_version: u32,
    pub protocol: String,
    pub parser_scope: String,
    pub frame_id: u64,
    pub frame_sequence: u64,
    pub captured_at_unix_millis: u128,
    pub canonical_width: u32,
    pub canonical_height: u32,
    pub canonical_stride: u32,
    pub canonical_byte_length: u64,
    pub canonical_bgra8_sha256: String,
    pub source_capture_commitment_sha256: String,
    pub source_frame_profile_binding_sha256: String,
    pub source_navigation_classification_result_commitment_sha256: String,
    pub source_lifecycle_snapshot_commitment_sha256: String,
    pub navigation_profile_commitment_sha256: String,
    pub navigation_profile_admission_commitment_sha256: String,
    pub approved_account_alias_sha256: String,
    pub runtime_identity_commitment_sha256: String,
    pub classifier_binary_sha256: String,
    pub classifier_assets_manifest_sha256: String,
    pub target_commitment_sha256: String,
    pub target: MtgoCompetitiveEventListingTargetV1,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoCompetitiveEventListingClassifierProcessResponseV1 {
    pub schema_version: u32,
    pub request_commitment_sha256: String,
    pub selection: MtgoVisibleCompetitiveEventListingSelectionV1,
}

/// Structurally checked request bytes. This value retains no pixels and does
/// not prove that the bytes came from the opaque capture path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckedUntrustedMtgoCompetitiveEventListingClassifierRequestV1 {
    header: MtgoCompetitiveEventListingClassifierRequestHeaderV1,
    request_commitment_sha256: String,
}

impl CheckedUntrustedMtgoCompetitiveEventListingClassifierRequestV1 {
    pub fn header_v1(&self) -> &MtgoCompetitiveEventListingClassifierRequestHeaderV1 {
        &self.header
    }

    pub fn request_commitment_sha256_v1(&self) -> &str {
        &self.request_commitment_sha256
    }

    pub fn safe_for_live_classification_v1(&self) -> bool {
        false
    }

    pub fn permits_open_entry_review_v1(&self) -> bool {
        false
    }

    pub fn permits_event_entry_v1(&self) -> bool {
        false
    }

    pub fn permits_spending_v1(&self) -> bool {
        false
    }

    pub fn safe_for_input_v1(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoClassifiedCompetitiveEventListingCommitmentsV1 {
    pub source_listing: MtgoSourceBoundCompetitiveEventListingCommitmentsV1,
    pub runtime_identity_commitment_sha256: String,
    pub request_commitment_sha256: String,
    pub classifier_response_sha256: String,
    pub classification_result_commitment_sha256: String,
}

/// One source-bound Event Browser selection produced by the exact
/// navigation-profile-pinned parser process. It remains unratified and has no
/// action conversion.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoClassifiedCompetitiveEventListingV1;
/// fn require_clone<T: Clone>() {}
/// require_clone::<OpaqueMtgoClassifiedCompetitiveEventListingV1>();
/// ```
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoClassifiedCompetitiveEventListingV1;
/// fn cannot_control(value: &OpaqueMtgoClassifiedCompetitiveEventListingV1) {
///     let _ = value.canonical_bgra8();
///     let _ = value.control_rect_client_px();
///     let _ = value.open_entry_review();
///     let _ = value.input_command();
/// }
/// ```
pub struct OpaqueMtgoClassifiedCompetitiveEventListingV1 {
    _source_listing: OpaqueMtgoSourceBoundCompetitiveEventListingV1,
    commitments: MtgoClassifiedCompetitiveEventListingCommitmentsV1,
}

impl OpaqueMtgoClassifiedCompetitiveEventListingV1 {
    pub fn commitments_v1(&self) -> MtgoClassifiedCompetitiveEventListingCommitmentsV1 {
        self.commitments.clone()
    }

    pub fn event_kind_v1(&self) -> MtgoCompetitiveEventKindV1 {
        self.commitments.source_listing.event_kind
    }

    pub fn safe_for_live_classification_v1(&self) -> bool {
        false
    }

    pub fn permits_open_entry_review_v1(&self) -> bool {
        false
    }

    pub fn permits_event_entry_v1(&self) -> bool {
        false
    }

    pub fn permits_spending_v1(&self) -> bool {
        false
    }

    pub fn safe_for_input_v1(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoEvaluatedCompetitiveEventListingCommitmentsV1 {
    pub classified_listing: MtgoClassifiedCompetitiveEventListingCommitmentsV1,
    pub evaluation_ratification_commitment_sha256: String,
    pub evaluation_admission_commitment_sha256: String,
    pub evaluated_binding_commitment_sha256: String,
}

/// A classified listing bound to the exact separately admitted two-mode
/// evaluation. Capture admission remains insufficient for live classification,
/// and this wrapper still has no Open Entry Review or input authority.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoEvaluatedCompetitiveEventListingV1;
/// fn require_clone<T: Clone>() {}
/// require_clone::<OpaqueMtgoEvaluatedCompetitiveEventListingV1>();
/// ```
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoEvaluatedCompetitiveEventListingV1;
/// fn cannot_control(value: &OpaqueMtgoEvaluatedCompetitiveEventListingV1) {
///     let _ = value.canonical_bgra8();
///     let _ = value.open_entry_review();
///     let _ = value.enter_event();
///     let _ = value.input_command();
/// }
/// ```
pub struct OpaqueMtgoEvaluatedCompetitiveEventListingV1 {
    _classified: OpaqueMtgoClassifiedCompetitiveEventListingV1,
    _evaluation: AdmittedMtgoCompetitiveEventListingEvaluationV1,
    commitments: MtgoEvaluatedCompetitiveEventListingCommitmentsV1,
}

impl OpaqueMtgoEvaluatedCompetitiveEventListingV1 {
    pub fn commitments_v1(&self) -> MtgoEvaluatedCompetitiveEventListingCommitmentsV1 {
        self.commitments.clone()
    }

    pub fn safe_for_live_classification_v1(&self) -> bool {
        false
    }

    pub fn permits_open_entry_review_v1(&self) -> bool {
        false
    }

    pub fn permits_event_entry_v1(&self) -> bool {
        false
    }

    pub fn permits_spending_v1(&self) -> bool {
        false
    }

    pub fn safe_for_input_v1(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MtgoPreparedCompetitiveEventListingOpenSourceCommitmentsV1 {
    pub(crate) evaluated_listing: MtgoEvaluatedCompetitiveEventListingCommitmentsV1,
    pub(crate) mode_authorization_commitment_sha256: String,
    pub(crate) open_intent_commitment_sha256: String,
    pub(crate) source_preparation_commitment_sha256: String,
    pub(crate) event_kind: MtgoCompetitiveEventKindV1,
    pub(crate) event_identity_sha256: String,
    pub(crate) source_frame_id: u64,
    pub(crate) source_frame_sequence: u64,
    pub(crate) source_captured_at_unix_millis: u128,
}

/// Private source for exactly one click on a freshly rehashed visible Open
/// Entry Review control. It retains the semantic intent and opaque capture
/// lineage but exposes neither coordinates nor an input primitive.
pub(crate) struct OpaqueMtgoPreparedCompetitiveEventListingOpenSourceV1 {
    _source_frame: OpaqueMtgoAdmittedCompetitiveNavigationFrameV1,
    _navigation_classification: OpaqueMtgoRetainedCompetitiveNavigationClassificationV1,
    _evaluation: AdmittedMtgoCompetitiveEventListingEvaluationV1,
    intent: CheckedUntrustedMtgoCompetitiveEventListingOpenIntentV1,
    commitments: MtgoPreparedCompetitiveEventListingOpenSourceCommitmentsV1,
}

impl OpaqueMtgoPreparedCompetitiveEventListingOpenSourceV1 {
    pub(crate) fn commitments_v1(
        &self,
    ) -> MtgoPreparedCompetitiveEventListingOpenSourceCommitmentsV1 {
        self.commitments.clone()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoCompetitiveEventListingOpenVisibleConfirmationCommitmentsV1 {
    pub source_preparation_commitment_sha256: String,
    pub arrival_commitment_sha256: String,
    pub after_capture_commitment_sha256: String,
    pub after_classification_result_commitment_sha256: String,
    pub after_lifecycle_snapshot_commitment_sha256: String,
    pub visible_confirmation_commitment_sha256: String,
    pub event_kind: MtgoCompetitiveEventKindV1,
    pub event_identity_sha256: String,
    pub after_frame_id: u64,
    pub after_frame_sequence: u64,
    pub after_captured_at_unix_millis: u128,
}

pub(crate) struct OpaqueMtgoConfirmedCompetitiveEventListingOpenV1 {
    _source_frame: OpaqueMtgoAdmittedCompetitiveNavigationFrameV1,
    _source_navigation_classification: OpaqueMtgoRetainedCompetitiveNavigationClassificationV1,
    _evaluation: AdmittedMtgoCompetitiveEventListingEvaluationV1,
    _after_frame: OpaqueMtgoAdmittedCompetitiveNavigationFrameV1,
    _after_navigation_classification: OpaqueMtgoRetainedCompetitiveNavigationClassificationV1,
    after_navigation: MtgoClassifiedCompetitiveNavigationFrameCommitmentsV1,
    _arrival: CheckedUntrustedMtgoCompetitiveEntryReviewArrivalV1,
    commitments: MtgoCompetitiveEventListingOpenVisibleConfirmationCommitmentsV1,
}

impl OpaqueMtgoConfirmedCompetitiveEventListingOpenV1 {
    pub(crate) fn commitments_v1(
        &self,
    ) -> MtgoCompetitiveEventListingOpenVisibleConfirmationCommitmentsV1 {
        self.commitments.clone()
    }

    pub(crate) fn after_source_lineage_v1(
        &self,
    ) -> Result<
        (
            MtgoClassifiedCompetitiveNavigationFrameCommitmentsV1,
            String,
        ),
        String,
    > {
        Ok((
            self.after_navigation.clone(),
            competitive_entry_window_continuity_commitment_for_frame_v1(
                &self._after_frame.source_frame,
            )?,
        ))
    }
}

/// Coordinate-free telemetry for one selected Event Browser listing whose
/// declared regions were rehashed from an opaque classified navigation frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoSourceBoundCompetitiveEventListingCommitmentsV1 {
    pub source_navigation: MtgoClassifiedCompetitiveNavigationFrameCommitmentsV1,
    pub target_commitment_sha256: String,
    pub selection_commitment_sha256: String,
    pub source_binding_commitment_sha256: String,
    pub event_kind: MtgoCompetitiveEventKindV1,
    pub event_identity_sha256: String,
    pub deck_list_sha256: String,
    pub deck_manifest_commitment_sha256: String,
    pub deck_format_sha256: String,
    pub policy_deployment_commitment_sha256: String,
}

/// One move-only Event Browser selection retained with its exact opaque
/// composed-desktop source and navigation-classifier lineage.
///
/// This type exposes commitments only. It has no pixel, rectangle, click,
/// entry-confirmation, spending, or input accessor.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoSourceBoundCompetitiveEventListingV1;
/// fn require_clone<T: Clone>() {}
/// require_clone::<OpaqueMtgoSourceBoundCompetitiveEventListingV1>();
/// ```
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoSourceBoundCompetitiveEventListingV1;
/// fn cannot_control(value: &OpaqueMtgoSourceBoundCompetitiveEventListingV1) {
///     let _ = value.canonical_bgra8();
///     let _ = value.control_rect_client_px();
///     let _ = value.click();
///     let _ = value.confirm_entry();
/// }
/// ```
pub struct OpaqueMtgoSourceBoundCompetitiveEventListingV1 {
    _source_frame: OpaqueMtgoAdmittedCompetitiveNavigationFrameV1,
    _navigation_classification: OpaqueMtgoRetainedCompetitiveNavigationClassificationV1,
    selection: CheckedUntrustedMtgoCompetitiveEventListingSelectionV1,
    _open_entry_review_control_rect_client_px: MtgoRectPxV1,
    commitments: MtgoSourceBoundCompetitiveEventListingCommitmentsV1,
}

impl OpaqueMtgoSourceBoundCompetitiveEventListingV1 {
    pub fn commitments_v1(&self) -> MtgoSourceBoundCompetitiveEventListingCommitmentsV1 {
        self.commitments.clone()
    }

    pub fn event_kind_v1(&self) -> MtgoCompetitiveEventKindV1 {
        self.selection.event_kind_v1()
    }

    pub fn event_identity_sha256_v1(&self) -> &str {
        self.selection.event_identity_sha256_v1()
    }

    pub fn safe_for_event_listing_perception_v1(&self) -> bool {
        false
    }

    pub fn permits_open_entry_review_v1(&self) -> bool {
        false
    }

    pub fn permits_event_entry_v1(&self) -> bool {
        false
    }

    pub fn permits_spending_v1(&self) -> bool {
        false
    }

    pub fn safe_for_input_v1(&self) -> bool {
        false
    }
}

/// Converts one freshly captured, evaluated Event Browser listing into a
/// coordinate-private Open Entry Review source. The mode scope is checked by
/// the blackbox semantic contract. The returned point is crate-private and can
/// only be consumed by the separately ratified shared input gate.
pub(crate) fn prepare_opaque_competitive_event_listing_open_source_v1(
    evaluated: OpaqueMtgoEvaluatedCompetitiveEventListingV1,
    authorization: &MtgoAuthorizationScopeV1,
) -> Result<
    (
        OpaqueMtgoPreparedCompetitiveEventListingOpenSourceV1,
        MtgoCompetitiveEntryPointerTargetV1,
    ),
    String,
> {
    let evaluated_commitments = evaluated.commitments_v1();
    let OpaqueMtgoEvaluatedCompetitiveEventListingV1 {
        _classified,
        _evaluation,
        commitments: _,
    } = evaluated;
    let OpaqueMtgoClassifiedCompetitiveEventListingV1 {
        _source_listing,
        commitments: _,
    } = _classified;
    let OpaqueMtgoSourceBoundCompetitiveEventListingV1 {
        _source_frame,
        _navigation_classification,
        selection,
        _open_entry_review_control_rect_client_px,
        commitments: _,
    } = _source_listing;
    let navigation = &evaluated_commitments
        .classified_listing
        .source_listing
        .source_navigation;
    let listing = &evaluated_commitments.classified_listing.source_listing;
    if navigation.phase != MtgoCompetitiveLifecyclePhaseV1::EventBrowser
        || navigation.event_kind != listing.event_kind
        || navigation.frame_id == 0
        || navigation.frame_sequence == 0
        || navigation
            .source_frame
            .source_capture
            .captured_at_unix_millis
            == 0
    {
        return Err("evaluated listing is not one complete Event Browser frame".to_owned());
    }
    let intent = make_offline_competitive_event_listing_open_intent_v1(selection, authorization)
        .map_err(|error| format!("build exact selected-listing open intent: {error}"))?;
    let mode_authorization_commitment_sha256 =
        intent.mode_authorization_commitment_sha256_v1().to_owned();
    let open_intent_commitment_sha256 = intent.open_intent_commitment_sha256_v1().to_owned();
    let pointer_target = resolve_admitted_competitive_navigation_pointer_target_v1(
        &_source_frame,
        &_open_entry_review_control_rect_client_px,
        "selected Open Entry Review control",
    )?;
    let source_preparation_commitment_sha256 = commitment_v1(
        PREPARED_EVENT_LISTING_OPEN_SOURCE_DOMAIN_V1,
        &[
            evaluated_commitments
                .evaluated_binding_commitment_sha256
                .as_bytes(),
            listing.source_binding_commitment_sha256.as_bytes(),
            listing.selection_commitment_sha256.as_bytes(),
            mode_authorization_commitment_sha256.as_bytes(),
            open_intent_commitment_sha256.as_bytes(),
            navigation
                .source_frame
                .source_capture
                .capture_commitment_sha256
                .as_bytes(),
            navigation.classification_result_commitment_sha256.as_bytes(),
            navigation.lifecycle_snapshot_commitment_sha256.as_bytes(),
            listing.event_identity_sha256.as_bytes(),
            navigation.frame_id.to_be_bytes().as_slice(),
            navigation.frame_sequence.to_be_bytes().as_slice(),
            navigation
                .source_frame
                .source_capture
                .captured_at_unix_millis
                .to_be_bytes()
                .as_slice(),
            b"fresh_rehashed_event_browser_control_no_entry_confirmation_no_spending_no_public_coordinates",
        ],
    );
    let commitments = MtgoPreparedCompetitiveEventListingOpenSourceCommitmentsV1 {
        evaluated_listing: evaluated_commitments.clone(),
        mode_authorization_commitment_sha256,
        open_intent_commitment_sha256,
        source_preparation_commitment_sha256,
        event_kind: listing.event_kind,
        event_identity_sha256: listing.event_identity_sha256.clone(),
        source_frame_id: navigation.frame_id,
        source_frame_sequence: navigation.frame_sequence,
        source_captured_at_unix_millis: navigation
            .source_frame
            .source_capture
            .captured_at_unix_millis,
    };
    Ok((
        OpaqueMtgoPreparedCompetitiveEventListingOpenSourceV1 {
            _source_frame,
            _navigation_classification,
            _evaluation,
            intent,
            commitments,
        },
        pointer_target,
    ))
}

/// Confirms that the one pending Open Entry Review click reached a strictly
/// newer visible Entry Review frame for the exact selected event. Any caller
/// must halt the shared gate when this returns an error.
pub(crate) fn confirm_opaque_competitive_event_listing_opened_v1(
    source: OpaqueMtgoPreparedCompetitiveEventListingOpenSourceV1,
    after: OpaqueMtgoClassifiedCompetitiveNavigationFrameV1,
    authorization: &MtgoAuthorizationScopeV1,
    input_sent_at_unix_millis: u128,
) -> Result<OpaqueMtgoConfirmedCompetitiveEventListingOpenV1, String> {
    let source_commitments = source.commitments_v1();
    let before = &source_commitments
        .evaluated_listing
        .classified_listing
        .source_listing
        .source_navigation;
    let after_commitments = after.commitments_v1();
    if input_sent_at_unix_millis == 0
        || before.source_frame.profile_commitment_sha256
            != after_commitments.source_frame.profile_commitment_sha256
        || before.source_frame.profile_admission_commitment_sha256
            != after_commitments
                .source_frame
                .profile_admission_commitment_sha256
        || before.source_frame.approved_account_alias_sha256
            != after_commitments.source_frame.approved_account_alias_sha256
        || before.runtime_identity_commitment_sha256
            != after_commitments.runtime_identity_commitment_sha256
        || before.event_kind != after_commitments.event_kind
        || after_commitments.phase != MtgoCompetitiveLifecyclePhaseV1::EntryReview
        || after_commitments.frame_id == before.frame_id
        || after_commitments.frame_sequence <= before.frame_sequence
    {
        return Err(
            "Open Entry Review confirmation changed profile, account, runtime, mode, phase, or frame order"
                .to_owned(),
        );
    }
    let before_raw = &source._source_frame.source_frame;
    let after_raw = &after._source_frame.source_frame;
    if before_raw.manifest.pre.process_id != after_raw.manifest.pre.process_id
        || before_raw.manifest.pre.process_start_filetime_100ns
            != after_raw.manifest.pre.process_start_filetime_100ns
        || before_raw.manifest.pre.hwnd != after_raw.manifest.pre.hwnd
        || before_raw.manifest.pre.dpi != after_raw.manifest.pre.dpi
        || before_raw.manifest.pre.client_rect_desktop_px
            != after_raw.manifest.pre.client_rect_desktop_px
        || before_raw.manifest.output.device_name != after_raw.manifest.output.device_name
        || before_raw.manifest.output.bounds_desktop_px
            != after_raw.manifest.output.bounds_desktop_px
        || before_raw.manifest.captured_at_unix_millis > input_sent_at_unix_millis
        || after_raw.manifest.captured_at_unix_millis <= input_sent_at_unix_millis
        || before.source_frame.source_capture.capture_commitment_sha256
            == after_commitments
                .source_frame
                .source_capture
                .capture_commitment_sha256
        || before_raw.manifest.frame.canonical_bgra8_sha256
            == after_raw.manifest.frame.canonical_bgra8_sha256
        || before.classification_result_commitment_sha256
            == after_commitments.classification_result_commitment_sha256
        || before.lifecycle_snapshot_commitment_sha256
            == after_commitments.lifecycle_snapshot_commitment_sha256
    {
        return Err(
            "Open Entry Review confirmation changed the client incarnation or lacks a changed post-input frame"
                .to_owned(),
        );
    }
    let after_captured_at_unix_millis = after_raw.manifest.captured_at_unix_millis;
    let after_navigation = after_commitments.clone();
    let (after_frame, after_lifecycle, after_navigation_classification) =
        after.into_event_listing_parts_v1();
    let OpaqueMtgoPreparedCompetitiveEventListingOpenSourceV1 {
        _source_frame,
        _navigation_classification,
        _evaluation,
        intent,
        commitments: _,
    } = source;
    let arrival =
        confirm_checked_competitive_event_listing_opened_v1(intent, authorization, after_lifecycle)
            .map_err(|error| format!("confirm exact selected Entry Review arrival: {error}"))?;
    if arrival.event_kind_v1() != source_commitments.event_kind
        || arrival.event_identity_sha256_v1() != source_commitments.event_identity_sha256
    {
        return Err("visible Entry Review arrival changed the selected event identity".to_owned());
    }
    let arrival_commitment_sha256 = arrival.arrival_commitment_sha256_v1().to_owned();
    let after_capture_commitment_sha256 = after_commitments
        .source_frame
        .source_capture
        .capture_commitment_sha256;
    let visible_confirmation_commitment_sha256 = commitment_v1(
        EVENT_LISTING_OPEN_VISIBLE_CONFIRMATION_DOMAIN_V1,
        &[
            source_commitments
                .source_preparation_commitment_sha256
                .as_bytes(),
            source_commitments.open_intent_commitment_sha256.as_bytes(),
            arrival_commitment_sha256.as_bytes(),
            after_capture_commitment_sha256.as_bytes(),
            after_commitments
                .classification_result_commitment_sha256
                .as_bytes(),
            after_commitments
                .lifecycle_snapshot_commitment_sha256
                .as_bytes(),
            source_commitments.event_identity_sha256.as_bytes(),
            after_commitments.frame_id.to_be_bytes().as_slice(),
            after_commitments.frame_sequence.to_be_bytes().as_slice(),
            after_captured_at_unix_millis.to_be_bytes().as_slice(),
            b"strictly_newer_exact_entry_review_visible_no_entry_confirmation_no_spending_no_input",
        ],
    );
    let commitments = MtgoCompetitiveEventListingOpenVisibleConfirmationCommitmentsV1 {
        source_preparation_commitment_sha256: source_commitments
            .source_preparation_commitment_sha256,
        arrival_commitment_sha256,
        after_capture_commitment_sha256,
        after_classification_result_commitment_sha256: after_commitments
            .classification_result_commitment_sha256,
        after_lifecycle_snapshot_commitment_sha256: after_commitments
            .lifecycle_snapshot_commitment_sha256,
        visible_confirmation_commitment_sha256,
        event_kind: source_commitments.event_kind,
        event_identity_sha256: source_commitments.event_identity_sha256,
        after_frame_id: after_commitments.frame_id,
        after_frame_sequence: after_commitments.frame_sequence,
        after_captured_at_unix_millis,
    };
    Ok(OpaqueMtgoConfirmedCompetitiveEventListingOpenV1 {
        _source_frame,
        _source_navigation_classification: _navigation_classification,
        _evaluation,
        _after_frame: after_frame,
        _after_navigation_classification: after_navigation_classification,
        after_navigation,
        _arrival: arrival,
        commitments,
    })
}

/// Checks the canonical selected-listing parser request, exact asset bytes,
/// target identity, and exact BGRA8 payload. Success remains structural and
/// non-authorizing.
pub fn check_untrusted_competitive_event_listing_classifier_request_v1(
    canonical_header_json: &[u8],
    classifier_assets_manifest: &[u8],
    canonical_bgra8: &[u8],
    deck: &ValidatedMtgoCompetitiveDeckManifestV1,
) -> Result<CheckedUntrustedMtgoCompetitiveEventListingClassifierRequestV1, String> {
    if canonical_header_json.is_empty()
        || canonical_header_json.len() > MAX_EVENT_LISTING_REQUEST_HEADER_BYTES_V1
        || classifier_assets_manifest.is_empty()
        || classifier_assets_manifest.len() > MAX_EVENT_LISTING_ASSETS_MANIFEST_BYTES_V1
    {
        return Err(
            "event-listing classifier request header or assets are outside bounds".to_owned(),
        );
    }
    let header: MtgoCompetitiveEventListingClassifierRequestHeaderV1 =
        serde_json::from_slice(canonical_header_json)
            .map_err(|error| format!("parse event-listing classifier request header: {error}"))?;
    let canonical = serde_json::to_vec(&header)
        .map_err(|error| format!("serialize event-listing classifier request header: {error}"))?;
    if canonical != canonical_header_json {
        return Err("event-listing classifier request header is not canonical JSON".to_owned());
    }
    if header.schema_version != 1
        || header.protocol != "mtgo_visible_competitive_event_listing_v1"
        || header.parser_scope
            != "league_and_challenge_selected_listing_exact_semantics_checked_untrusted_v1"
        || header.frame_id == 0
        || header.frame_sequence == 0
        || header.captured_at_unix_millis == 0
        || header.canonical_width == 0
        || header.canonical_height == 0
        || header.canonical_width > 16_384
        || header.canonical_height > 16_384
    {
        return Err("event-listing classifier request identity is invalid".to_owned());
    }
    let expected_stride = header
        .canonical_width
        .checked_mul(4)
        .ok_or("event-listing classifier stride overflow")?;
    let expected_length = u64::from(expected_stride)
        .checked_mul(u64::from(header.canonical_height))
        .ok_or("event-listing classifier byte length overflow")?;
    if header.canonical_stride != expected_stride
        || header.canonical_byte_length != expected_length
        || usize::try_from(expected_length).ok() != Some(canonical_bgra8.len())
        || header.canonical_bgra8_sha256 != sha256_hex_v1(canonical_bgra8)
        || header.classifier_assets_manifest_sha256 != sha256_hex_v1(classifier_assets_manifest)
    {
        return Err("event-listing classifier pixels or assets differ from the header".to_owned());
    }
    for value in [
        header.canonical_bgra8_sha256.as_str(),
        header.source_capture_commitment_sha256.as_str(),
        header.source_frame_profile_binding_sha256.as_str(),
        header
            .source_navigation_classification_result_commitment_sha256
            .as_str(),
        header.source_lifecycle_snapshot_commitment_sha256.as_str(),
        header.navigation_profile_commitment_sha256.as_str(),
        header
            .navigation_profile_admission_commitment_sha256
            .as_str(),
        header.approved_account_alias_sha256.as_str(),
        header.runtime_identity_commitment_sha256.as_str(),
        header.classifier_binary_sha256.as_str(),
        header.classifier_assets_manifest_sha256.as_str(),
        header.target_commitment_sha256.as_str(),
    ] {
        if !looks_like_lower_sha256_v1(value) {
            return Err(
                "event-listing classifier request contains an invalid commitment".to_owned(),
            );
        }
    }
    let target_commitment_sha256 =
        competitive_event_listing_target_commitment_v1(&header.target, deck)
            .map_err(|error| format!("validate event-listing classifier target: {error}"))?;
    if header.target_commitment_sha256 != target_commitment_sha256
        || header.target.approved_account_alias_sha256 != header.approved_account_alias_sha256
        || header.target.deck_list_sha256 != deck.deck_list_sha256()
        || header.target.deck_manifest_commitment_sha256 != deck.manifest_commitment_sha256()
        || header.target.deck_format_sha256 != deck.format_sha256()
    {
        return Err(
            "event-listing classifier target differs from the exact account or deck".to_owned(),
        );
    }
    let request_commitment_sha256 = commitment_v1(
        EVENT_LISTING_CLASSIFIER_REQUEST_DOMAIN_V1,
        &[
            canonical_header_json,
            classifier_assets_manifest,
            canonical_bgra8,
        ],
    );
    Ok(
        CheckedUntrustedMtgoCompetitiveEventListingClassifierRequestV1 {
            header,
            request_commitment_sha256,
        },
    )
}

/// Runs the exact navigation-profile-pinned binary in selected-listing mode
/// over one opaque Event Browser frame. It accepts only the exact caller target
/// and rehashes all lifecycle, label, and enabled-control regions before
/// returning a move-only checked-untrusted result.
pub fn classify_checked_untrusted_competitive_event_listing_v1(
    source: OpaqueMtgoClassifiedCompetitiveNavigationFrameV1,
    deck: &ValidatedMtgoCompetitiveDeckManifestV1,
    target: MtgoCompetitiveEventListingTargetV1,
    runtime: &OpaqueMtgoVerifiedCompetitiveNavigationClassifierRuntimeV1,
    timeout_ms: u32,
) -> Result<OpaqueMtgoClassifiedCompetitiveEventListingV1, String> {
    if !(100..=60_000).contains(&timeout_ms) {
        return Err("event-listing classifier timeout must be between 100 and 60000 ms".to_owned());
    }
    let source_commitments = source.commitments_v1();
    let runtime_commitments = runtime.commitments_v1();
    if source.phase_v1() != MtgoCompetitiveLifecyclePhaseV1::EventBrowser
        || source.lifecycle_snapshot_v1().phase() != MtgoCompetitiveLifecyclePhaseV1::EventBrowser
        || source_commitments.runtime_identity_commitment_sha256
            != runtime_commitments.runtime_identity_commitment_sha256
        || source_commitments.source_frame.profile_commitment_sha256
            != runtime_commitments.navigation_profile_commitment_sha256
        || source_commitments
            .source_frame
            .profile_admission_commitment_sha256
            != runtime_commitments.navigation_profile_admission_commitment_sha256
        || source_commitments
            .source_frame
            .approved_account_alias_sha256
            != runtime_commitments.approved_account_alias_sha256
        || target.approved_account_alias_sha256
            != source_commitments
                .source_frame
                .approved_account_alias_sha256
        || target.event_kind != source_commitments.event_kind
    {
        return Err(
            "event-listing source, runtime, profile, account, target, or phase differs".to_owned(),
        );
    }
    let target_commitment_sha256 = competitive_event_listing_target_commitment_v1(&target, deck)
        .map_err(|error| format!("validate selected-listing target: {error}"))?;
    verify_runtime_identity_now_v1(runtime)?;

    let raw_source = &source._source_frame.source_frame;
    let width = raw_source.manifest.frame.canonical_width;
    let height = raw_source.manifest.frame.canonical_height;
    let stride = width
        .checked_mul(4)
        .ok_or("event-listing classifier canonical stride overflow")?;
    let byte_length = u64::try_from(raw_source.canonical_bgra8.len())
        .map_err(|_| "event-listing classifier canonical byte length overflow")?;
    if raw_source.manifest.frame.canonical_stride != stride
        || raw_source.manifest.frame.canonical_byte_length != raw_source.canonical_bgra8.len()
        || raw_source.manifest.frame.canonical_bgra8_sha256
            != sha256_hex_v1(&raw_source.canonical_bgra8)
        || raw_source.capture_commitment_sha256
            != source_commitments
                .source_frame
                .source_capture
                .capture_commitment_sha256
    {
        return Err(
            "opaque event-listing source pixels no longer match capture metadata".to_owned(),
        );
    }
    let assets = competitive_navigation_classifier_assets_manifest_bytes_v1(runtime);
    let header = MtgoCompetitiveEventListingClassifierRequestHeaderV1 {
        schema_version: 1,
        protocol: "mtgo_visible_competitive_event_listing_v1".to_owned(),
        parser_scope: "league_and_challenge_selected_listing_exact_semantics_checked_untrusted_v1"
            .to_owned(),
        frame_id: source_commitments.frame_id,
        frame_sequence: source_commitments.frame_sequence,
        captured_at_unix_millis: raw_source.manifest.captured_at_unix_millis,
        canonical_width: width,
        canonical_height: height,
        canonical_stride: stride,
        canonical_byte_length: byte_length,
        canonical_bgra8_sha256: raw_source.manifest.frame.canonical_bgra8_sha256.clone(),
        source_capture_commitment_sha256: raw_source.capture_commitment_sha256.clone(),
        source_frame_profile_binding_sha256: source_commitments
            .source_frame
            .frame_profile_binding_sha256
            .clone(),
        source_navigation_classification_result_commitment_sha256: source_commitments
            .classification_result_commitment_sha256
            .clone(),
        source_lifecycle_snapshot_commitment_sha256: source_commitments
            .lifecycle_snapshot_commitment_sha256
            .clone(),
        navigation_profile_commitment_sha256: source_commitments
            .source_frame
            .profile_commitment_sha256
            .clone(),
        navigation_profile_admission_commitment_sha256: source_commitments
            .source_frame
            .profile_admission_commitment_sha256
            .clone(),
        approved_account_alias_sha256: source_commitments
            .source_frame
            .approved_account_alias_sha256
            .clone(),
        runtime_identity_commitment_sha256: runtime_commitments
            .runtime_identity_commitment_sha256
            .clone(),
        classifier_binary_sha256: runtime_commitments.classifier_binary_sha256.clone(),
        classifier_assets_manifest_sha256: runtime_commitments
            .classifier_assets_manifest_sha256
            .clone(),
        target_commitment_sha256,
        target: target.clone(),
    };
    let header_json = serde_json::to_vec(&header)
        .map_err(|error| format!("serialize event-listing classifier request: {error}"))?;
    let checked_request = check_untrusted_competitive_event_listing_classifier_request_v1(
        &header_json,
        assets,
        &raw_source.canonical_bgra8,
        deck,
    )?;
    let request_commitment_sha256 = checked_request.request_commitment_sha256_v1().to_owned();
    let response_bytes = invoke_verified_competitive_event_listing_classifier_process_v1(
        runtime,
        &header_json,
        assets,
        &raw_source.canonical_bgra8,
        Duration::from_millis(u64::from(timeout_ms)),
    )?;
    verify_runtime_identity_now_v1(runtime)?;
    let response =
        parse_event_listing_classifier_response_v1(&response_bytes, &request_commitment_sha256)?;
    if response.selection.frame_id != source_commitments.frame_id
        || response.selection.frame_sequence != source_commitments.frame_sequence
        || response.selection.frame_sha256
            != source_commitments
                .source_frame
                .source_capture
                .canonical_bgra8_sha256
        || response
            .selection
            .source_lifecycle_snapshot_commitment_sha256
            != source_commitments.lifecycle_snapshot_commitment_sha256
        || response.selection.target_commitment_sha256 != header.target_commitment_sha256
    {
        return Err(
            "event-listing classifier response changed the exact source or target".to_owned(),
        );
    }
    let classifier_response_sha256 = sha256_hex_v1(&response_bytes);
    let source_listing = bind_classified_navigation_frame_to_competitive_event_listing_v1(
        source,
        deck,
        target,
        response.selection,
    )?;
    let source_listing_commitments = source_listing.commitments_v1();
    let classification_result_commitment_sha256 = commitment_v1(
        EVENT_LISTING_CLASSIFIER_RESULT_DOMAIN_V1,
        &[
            source_listing_commitments
                .source_navigation
                .classification_result_commitment_sha256
                .as_bytes(),
            source_listing_commitments
                .source_binding_commitment_sha256
                .as_bytes(),
            runtime_commitments
                .runtime_identity_commitment_sha256
                .as_bytes(),
            request_commitment_sha256.as_bytes(),
            classifier_response_sha256.as_bytes(),
            source_listing_commitments
                .selection_commitment_sha256
                .as_bytes(),
            b"bounded_selected_listing_parser_unratified_no_open_review_no_entry_no_spending_no_input",
        ],
    );
    Ok(OpaqueMtgoClassifiedCompetitiveEventListingV1 {
        commitments: MtgoClassifiedCompetitiveEventListingCommitmentsV1 {
            source_listing: source_listing_commitments,
            runtime_identity_commitment_sha256: runtime_commitments
                .runtime_identity_commitment_sha256,
            request_commitment_sha256,
            classifier_response_sha256,
            classification_result_commitment_sha256,
        },
        _source_listing: source_listing,
    })
}

/// Consumes one parser result and the separately admitted exact two-mode
/// evaluation only when profile, account, deck, format, and policy identities
/// all match. This is an evaluation binding, not an actuation grant.
pub fn bind_classified_competitive_event_listing_to_evaluation_v1(
    classified: OpaqueMtgoClassifiedCompetitiveEventListingV1,
    evaluation: AdmittedMtgoCompetitiveEventListingEvaluationV1,
) -> Result<OpaqueMtgoEvaluatedCompetitiveEventListingV1, String> {
    let classified_commitments = classified.commitments_v1();
    let listing = &classified_commitments.source_listing;
    let evaluated = evaluation.commitments_v1();
    if evaluated.profile_commitment_sha256
        != listing
            .source_navigation
            .source_frame
            .profile_commitment_sha256
        || evaluated.approved_account_alias_sha256
            != listing
                .source_navigation
                .source_frame
                .approved_account_alias_sha256
        || evaluated.deck_list_sha256 != listing.deck_list_sha256
        || evaluated.deck_manifest_commitment_sha256 != listing.deck_manifest_commitment_sha256
        || evaluated.deck_format_sha256 != listing.deck_format_sha256
        || evaluated.policy_deployment_commitment_sha256
            != listing.policy_deployment_commitment_sha256
    {
        return Err(
            "classified event listing does not match the admitted evaluation identity".to_owned(),
        );
    }
    let evaluation_admission_commitment_sha256 =
        evaluation.admission_commitment_sha256_v1().to_owned();
    let evaluated_binding_commitment_sha256 = commitment_v1(
        EVALUATED_EVENT_LISTING_BINDING_DOMAIN_V1,
        &[
            classified_commitments
                .classification_result_commitment_sha256
                .as_bytes(),
            evaluated.ratification_commitment_sha256.as_bytes(),
            evaluation_admission_commitment_sha256.as_bytes(),
            listing.target_commitment_sha256.as_bytes(),
            listing.selection_commitment_sha256.as_bytes(),
            b"evaluated_listing_still_no_open_review_no_entry_no_spending_no_input",
        ],
    );
    Ok(OpaqueMtgoEvaluatedCompetitiveEventListingV1 {
        commitments: MtgoEvaluatedCompetitiveEventListingCommitmentsV1 {
            classified_listing: classified_commitments,
            evaluation_ratification_commitment_sha256: evaluated.ratification_commitment_sha256,
            evaluation_admission_commitment_sha256,
            evaluated_binding_commitment_sha256,
        },
        _classified: classified,
        _evaluation: evaluation,
    })
}

fn parse_event_listing_classifier_response_v1(
    response_bytes: &[u8],
    request_commitment_sha256: &str,
) -> Result<MtgoCompetitiveEventListingClassifierProcessResponseV1, String> {
    let response: MtgoCompetitiveEventListingClassifierProcessResponseV1 =
        serde_json::from_slice(response_bytes).map_err(|error| {
            format!("event-listing classifier response is not one strict JSON value: {error}")
        })?;
    let canonical = serde_json::to_vec(&response)
        .map_err(|error| format!("serialize event-listing classifier response: {error}"))?;
    if canonical != response_bytes {
        return Err("event-listing classifier response is not canonical JSON".to_owned());
    }
    if response.schema_version != 1
        || response.request_commitment_sha256 != request_commitment_sha256
    {
        return Err("event-listing classifier response does not bind the exact request".to_owned());
    }
    Ok(response)
}

fn looks_like_lower_sha256_v1(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

/// Recomputes the lifecycle frame, every lifecycle fact, and both selected
/// listing regions from caller-supplied pixels before applying the blackbox
/// semantic contract. Success remains checked-untrusted because this function
/// cannot prove that the bytes came from the desktop capture backend.
pub fn check_untrusted_competitive_event_listing_pixels_v1(
    lifecycle: CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1,
    deck: &ValidatedMtgoCompetitiveDeckManifestV1,
    target: MtgoCompetitiveEventListingTargetV1,
    raw: MtgoVisibleCompetitiveEventListingSelectionV1,
    expected_approved_account_alias_sha256: &str,
    canonical_bgra8: &[u8],
) -> Result<CheckedUntrustedMtgoCompetitiveEventListingSelectionV1, String> {
    let bounds = lifecycle.client_bounds_v1();
    let size = MtgoSizePxV1 {
        width: bounds.width,
        height: bounds.height,
    };
    let expected_length = usize::try_from(
        u64::from(size.width)
            .checked_mul(u64::from(size.height))
            .and_then(|value| value.checked_mul(4))
            .ok_or("event-listing pixel length overflow")?,
    )
    .map_err(|_| "event-listing pixel length does not fit this process")?;
    if bounds.x != 0
        || bounds.y != 0
        || canonical_bgra8.len() != expected_length
        || sha256_hex_v1(canonical_bgra8) != lifecycle.frame_sha256_v1()
    {
        return Err(
            "event-listing pixels do not match the exact lifecycle frame and geometry".to_owned(),
        );
    }
    for fact in lifecycle.visible_facts_v1() {
        let actual =
            visible_frame_region_content_sha256_v1(canonical_bgra8, &size, &fact.rect_client_px)
                .map_err(|error| format!("rehash event-listing lifecycle fact: {error}"))?;
        if actual != fact.content_sha256 {
            return Err(
                "event-listing lifecycle fact does not match the supplied pixels".to_owned(),
            );
        }
    }
    let actual_label = visible_frame_region_content_sha256_v1(
        canonical_bgra8,
        &size,
        &raw.event_label_rect_client_px,
    )
    .map_err(|error| format!("rehash selected event label: {error}"))?;
    let actual_control = visible_frame_region_content_sha256_v1(
        canonical_bgra8,
        &size,
        &raw.open_entry_review_control_rect_client_px,
    )
    .map_err(|error| format!("rehash selected event control: {error}"))?;
    if actual_label != raw.event_label_region_sha256
        || actual_control != raw.open_entry_review_control_region_sha256
    {
        return Err(
            "selected event label or enabled control does not match the supplied pixels".to_owned(),
        );
    }
    let checked =
        validate_visible_competitive_event_listing_selection_v1(lifecycle, deck, target, raw)
            .map_err(|error| format!("validate selected competitive event listing: {error}"))?;
    if checked.approved_account_alias_sha256_v1() != expected_approved_account_alias_sha256 {
        return Err("selected event listing does not bind the approved account".to_owned());
    }
    Ok(checked)
}

/// Consumes one exact opaque navigation-classifier result and retains the
/// selected listing only after its visible regions are rehashed from the same
/// private composed-desktop pixels. This is a perception boundary only.
pub fn bind_classified_navigation_frame_to_competitive_event_listing_v1(
    classified: OpaqueMtgoClassifiedCompetitiveNavigationFrameV1,
    deck: &ValidatedMtgoCompetitiveDeckManifestV1,
    target: MtgoCompetitiveEventListingTargetV1,
    raw: MtgoVisibleCompetitiveEventListingSelectionV1,
) -> Result<OpaqueMtgoSourceBoundCompetitiveEventListingV1, String> {
    let open_entry_review_control_rect_client_px =
        raw.open_entry_review_control_rect_client_px.clone();
    let source_navigation = classified.commitments_v1();
    let (source_frame, lifecycle, navigation_classification) =
        classified.into_event_listing_parts_v1();
    let source_frame_commitments = source_frame.commitments_v1();
    if source_navigation.source_frame != source_frame_commitments {
        return Err("selected event listing source-frame lineage changed".to_owned());
    }
    let selection = check_untrusted_competitive_event_listing_pixels_v1(
        lifecycle,
        deck,
        target,
        raw,
        &source_frame.approved_account_alias_sha256,
        &source_frame.source_frame.canonical_bgra8,
    )?;
    let target_commitment_sha256 = selection.target_commitment_sha256_v1().to_owned();
    let selection_commitment_sha256 = selection.selection_commitment_sha256_v1().to_owned();
    let event_kind = selection.event_kind_v1();
    let event_identity_sha256 = selection.event_identity_sha256_v1().to_owned();
    let deck_list_sha256 = selection.deck_list_sha256_v1().to_owned();
    let deck_manifest_commitment_sha256 = selection.deck_manifest_commitment_sha256_v1().to_owned();
    let deck_format_sha256 = selection.deck_format_sha256_v1().to_owned();
    let policy_deployment_commitment_sha256 = selection
        .policy_deployment_commitment_sha256_v1()
        .to_owned();
    let event_kind_json = serde_json::to_vec(&event_kind)
        .map_err(|error| format!("serialize selected event kind: {error}"))?;
    let source_binding_commitment_sha256 = commitment_v1(
        SOURCE_BOUND_EVENT_LISTING_DOMAIN_V1,
        &[
            source_navigation
                .source_frame
                .profile_commitment_sha256
                .as_bytes(),
            source_navigation
                .source_frame
                .profile_admission_commitment_sha256
                .as_bytes(),
            source_navigation
                .source_frame
                .approved_account_alias_sha256
                .as_bytes(),
            source_navigation
                .source_frame
                .frame_profile_binding_sha256
                .as_bytes(),
            source_navigation
                .classification_result_commitment_sha256
                .as_bytes(),
            target_commitment_sha256.as_bytes(),
            selection_commitment_sha256.as_bytes(),
            deck_list_sha256.as_bytes(),
            deck_manifest_commitment_sha256.as_bytes(),
            deck_format_sha256.as_bytes(),
            policy_deployment_commitment_sha256.as_bytes(),
            event_kind_json.as_slice(),
            b"opaque_current_pixels_no_open_review_no_entry_no_spending_no_input",
        ],
    );
    Ok(OpaqueMtgoSourceBoundCompetitiveEventListingV1 {
        _source_frame: source_frame,
        _navigation_classification: navigation_classification,
        selection,
        _open_entry_review_control_rect_client_px: open_entry_review_control_rect_client_px,
        commitments: MtgoSourceBoundCompetitiveEventListingCommitmentsV1 {
            source_navigation,
            target_commitment_sha256,
            selection_commitment_sha256,
            source_binding_commitment_sha256,
            event_kind,
            event_identity_sha256,
            deck_list_sha256,
            deck_manifest_commitment_sha256,
            deck_format_sha256,
            policy_deployment_commitment_sha256,
        },
    })
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

#[cfg(test)]
mod tests {
    use super::*;
    use mtgo_blackbox_v1::{
        competitive_event_listing_target_commitment_v1, validate_competitive_deck_manifest_v1,
        validate_visible_competitive_lifecycle_snapshot_v1, MtgoCompetitiveDeckCardCountV1,
        MtgoCompetitiveDeckConfigurationV1, MtgoCompetitiveDeckManifestV1,
        MtgoCompetitiveLifecyclePhaseV1, MtgoLifecycleVisibleFactKindV1,
        MtgoLifecycleVisibleFactV1, MtgoRectPxV1, MtgoVisibleCompetitiveLifecycleSnapshotV1,
        MTGO_COMPETITIVE_EVENT_LISTING_SCHEMA_V1, MTGO_COMPETITIVE_LIFECYCLE_SCHEMA_V1,
        MTGO_COMPETITIVE_SIDEBOARD_SCHEMA_V1,
    };

    fn digest(byte: char) -> String {
        byte.to_string().repeat(64)
    }

    fn pixels_v1() -> Vec<u8> {
        (0..32 * 16 * 4)
            .map(|index| ((index * 29 + 17) % 251) as u8)
            .collect()
    }

    fn rect_hash_v1(pixels: &[u8], rect: &MtgoRectPxV1) -> String {
        visible_frame_region_content_sha256_v1(
            pixels,
            &MtgoSizePxV1 {
                width: 32,
                height: 16,
            },
            rect,
        )
        .unwrap()
    }

    fn deck_v1() -> ValidatedMtgoCompetitiveDeckManifestV1 {
        validate_competitive_deck_manifest_v1(MtgoCompetitiveDeckManifestV1 {
            schema_version: MTGO_COMPETITIVE_SIDEBOARD_SCHEMA_V1,
            deck_list_sha256: digest('1'),
            format_sha256: digest('2'),
            starting_mainboard_count: 5,
            starting_sideboard_count: 2,
            configuration: MtgoCompetitiveDeckConfigurationV1 {
                mainboard: vec![
                    MtgoCompetitiveDeckCardCountV1 {
                        card_db_id: 66,
                        card_name: "Lightning Bolt".to_owned(),
                        count: 2,
                    },
                    MtgoCompetitiveDeckCardCountV1 {
                        card_db_id: 76,
                        card_name: "Mountain".to_owned(),
                        count: 3,
                    },
                ],
                sideboard: vec![MtgoCompetitiveDeckCardCountV1 {
                    card_db_id: 101,
                    card_name: "Searing Blaze".to_owned(),
                    count: 2,
                }],
            },
        })
        .unwrap()
    }

    struct FixtureV1 {
        lifecycle: CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1,
        deck: ValidatedMtgoCompetitiveDeckManifestV1,
        target: MtgoCompetitiveEventListingTargetV1,
        raw: MtgoVisibleCompetitiveEventListingSelectionV1,
        pixels: Vec<u8>,
    }

    fn fixture_v1() -> FixtureV1 {
        fixture_with_browser_fact_v1(None)
    }

    fn fixture_with_browser_fact_v1(browser_fact_sha256: Option<String>) -> FixtureV1 {
        let pixels = pixels_v1();
        let client_bounds = MtgoRectPxV1 {
            x: 0,
            y: 0,
            width: 32,
            height: 16,
        };
        let browser_rect = MtgoRectPxV1 {
            x: 0,
            y: 0,
            width: 4,
            height: 4,
        };
        let label_rect = MtgoRectPxV1 {
            x: 5,
            y: 4,
            width: 8,
            height: 4,
        };
        let control_rect = MtgoRectPxV1 {
            x: 20,
            y: 4,
            width: 8,
            height: 4,
        };
        let lifecycle = validate_visible_competitive_lifecycle_snapshot_v1(
            MtgoVisibleCompetitiveLifecycleSnapshotV1 {
                schema_version: MTGO_COMPETITIVE_LIFECYCLE_SCHEMA_V1,
                snapshot_id: "source-bound-event-browser-v1".to_owned(),
                event_kind: MtgoCompetitiveEventKindV1::League,
                phase: MtgoCompetitiveLifecyclePhaseV1::EventBrowser,
                frame_id: 7,
                frame_sequence: 11,
                frame_sha256: sha256_hex_v1(&pixels),
                client_bounds: client_bounds.clone(),
                event_identity_sha256: None,
                match_identity_sha256: None,
                game_number: None,
                entry_terms: None,
                visible_state_complete: true,
                facts: vec![MtgoLifecycleVisibleFactV1 {
                    kind: MtgoLifecycleVisibleFactKindV1::EventBrowserVisible,
                    rect_client_px: browser_rect.clone(),
                    content_sha256: browser_fact_sha256
                        .unwrap_or_else(|| rect_hash_v1(&pixels, &browser_rect)),
                    confidence_bps: 10_000,
                }],
            },
        )
        .unwrap();
        let deck = deck_v1();
        let target = MtgoCompetitiveEventListingTargetV1 {
            schema_version: MTGO_COMPETITIVE_EVENT_LISTING_SCHEMA_V1,
            target_id: "source-bound-modern-league-v1".to_owned(),
            event_kind: MtgoCompetitiveEventKindV1::League,
            approved_account_alias_sha256: digest('a'),
            event_identity_sha256: digest('3'),
            event_display_label_sha256: digest('4'),
            deck_display_label_sha256: digest('6'),
            deck_list_sha256: deck.deck_list_sha256().to_owned(),
            deck_manifest_commitment_sha256: deck.manifest_commitment_sha256().to_owned(),
            deck_format_sha256: deck.format_sha256().to_owned(),
            policy_deployment_commitment_sha256: digest('5'),
        };
        let target_commitment_sha256 =
            competitive_event_listing_target_commitment_v1(&target, &deck).unwrap();
        let raw = MtgoVisibleCompetitiveEventListingSelectionV1 {
            schema_version: MTGO_COMPETITIVE_EVENT_LISTING_SCHEMA_V1,
            selection_id: "source-bound-event-listing-v1".to_owned(),
            target_commitment_sha256,
            event_kind: MtgoCompetitiveEventKindV1::League,
            event_identity_sha256: target.event_identity_sha256.clone(),
            event_display_label_sha256: target.event_display_label_sha256.clone(),
            source_lifecycle_snapshot_commitment_sha256: lifecycle
                .snapshot_commitment_sha256()
                .to_owned(),
            frame_id: lifecycle.frame_id_v1(),
            frame_sequence: lifecycle.frame_sequence(),
            frame_sha256: lifecycle.frame_sha256_v1().to_owned(),
            client_bounds,
            event_label_rect_client_px: label_rect.clone(),
            event_label_region_sha256: rect_hash_v1(&pixels, &label_rect),
            open_entry_review_control_rect_client_px: control_rect.clone(),
            open_entry_review_control_region_sha256: rect_hash_v1(&pixels, &control_rect),
            open_entry_review_control_enabled: true,
            confidence_bps: 10_000,
        };
        FixtureV1 {
            lifecycle,
            deck,
            target,
            raw,
            pixels,
        }
    }

    fn request_header_v1(
        fixture: &FixtureV1,
        assets: &[u8],
    ) -> MtgoCompetitiveEventListingClassifierRequestHeaderV1 {
        MtgoCompetitiveEventListingClassifierRequestHeaderV1 {
            schema_version: 1,
            protocol: "mtgo_visible_competitive_event_listing_v1".to_owned(),
            parser_scope:
                "league_and_challenge_selected_listing_exact_semantics_checked_untrusted_v1"
                    .to_owned(),
            frame_id: fixture.raw.frame_id,
            frame_sequence: fixture.raw.frame_sequence,
            captured_at_unix_millis: 1_786_400_000_000,
            canonical_width: 32,
            canonical_height: 16,
            canonical_stride: 128,
            canonical_byte_length: 2_048,
            canonical_bgra8_sha256: sha256_hex_v1(&fixture.pixels),
            source_capture_commitment_sha256: digest('6'),
            source_frame_profile_binding_sha256: digest('7'),
            source_navigation_classification_result_commitment_sha256: digest('8'),
            source_lifecycle_snapshot_commitment_sha256: fixture
                .raw
                .source_lifecycle_snapshot_commitment_sha256
                .clone(),
            navigation_profile_commitment_sha256: digest('9'),
            navigation_profile_admission_commitment_sha256: digest('b'),
            approved_account_alias_sha256: fixture.target.approved_account_alias_sha256.clone(),
            runtime_identity_commitment_sha256: digest('c'),
            classifier_binary_sha256: digest('d'),
            classifier_assets_manifest_sha256: sha256_hex_v1(assets),
            target_commitment_sha256: fixture.raw.target_commitment_sha256.clone(),
            target: fixture.target.clone(),
        }
    }

    #[test]
    fn exact_current_pixels_create_only_a_checked_untrusted_selection() {
        let fixture = fixture_v1();
        let checked = check_untrusted_competitive_event_listing_pixels_v1(
            fixture.lifecycle,
            &fixture.deck,
            fixture.target,
            fixture.raw,
            &digest('a'),
            &fixture.pixels,
        )
        .unwrap();
        assert_eq!(checked.event_kind_v1(), MtgoCompetitiveEventKindV1::League);
        assert!(!checked.safe_for_input_v1());
        assert!(!checked.permits_event_entry_v1());
        assert!(!checked.permits_spending_v1());
    }

    #[test]
    fn full_frame_lifecycle_fact_and_selected_regions_are_rehashed() {
        let mut full_frame = fixture_v1();
        full_frame.pixels[100] ^= 0xff;
        assert!(check_untrusted_competitive_event_listing_pixels_v1(
            full_frame.lifecycle,
            &full_frame.deck,
            full_frame.target,
            full_frame.raw,
            &digest('a'),
            &full_frame.pixels,
        )
        .err()
        .unwrap()
        .contains("exact lifecycle frame"));

        let lifecycle_fact = fixture_with_browser_fact_v1(Some(digest('d')));
        assert!(check_untrusted_competitive_event_listing_pixels_v1(
            lifecycle_fact.lifecycle,
            &lifecycle_fact.deck,
            lifecycle_fact.target,
            lifecycle_fact.raw,
            &digest('a'),
            &lifecycle_fact.pixels,
        )
        .err()
        .unwrap()
        .contains("lifecycle fact"));

        let mut label = fixture_v1();
        label.raw.event_label_region_sha256 = digest('f');
        assert!(check_untrusted_competitive_event_listing_pixels_v1(
            label.lifecycle,
            &label.deck,
            label.target,
            label.raw,
            &digest('a'),
            &label.pixels,
        )
        .err()
        .unwrap()
        .contains("selected event label"));

        let mut control = fixture_v1();
        control.raw.open_entry_review_control_region_sha256 = digest('e');
        assert!(check_untrusted_competitive_event_listing_pixels_v1(
            control.lifecycle,
            &control.deck,
            control.target,
            control.raw,
            &digest('a'),
            &control.pixels,
        )
        .err()
        .unwrap()
        .contains("enabled control"));
    }

    #[test]
    fn exact_approved_account_is_required() {
        let fixture = fixture_v1();
        assert!(check_untrusted_competitive_event_listing_pixels_v1(
            fixture.lifecycle,
            &fixture.deck,
            fixture.target,
            fixture.raw,
            &digest('b'),
            &fixture.pixels,
        )
        .err()
        .unwrap()
        .contains("approved account"));
    }

    #[test]
    fn canonical_parser_request_binds_pixels_assets_target_and_deck_without_authority() {
        let fixture = fixture_v1();
        let assets = b"event-listing-assets-v1";
        let header = request_header_v1(&fixture, assets);
        let bytes = serde_json::to_vec(&header).unwrap();
        let checked = check_untrusted_competitive_event_listing_classifier_request_v1(
            &bytes,
            assets,
            &fixture.pixels,
            &fixture.deck,
        )
        .unwrap();
        assert_eq!(checked.header_v1(), &header);
        assert!(!checked.safe_for_live_classification_v1());
        assert!(!checked.permits_open_entry_review_v1());
        assert!(!checked.permits_event_entry_v1());
        assert!(!checked.permits_spending_v1());
        assert!(!checked.safe_for_input_v1());
    }

    #[test]
    fn parser_request_rejects_noncanonical_pixels_assets_and_target_substitution() {
        let fixture = fixture_v1();
        let assets = b"event-listing-assets-v1";
        let header = request_header_v1(&fixture, assets);
        let bytes = serde_json::to_vec(&header).unwrap();

        let mut padded = bytes.clone();
        padded.push(b' ');
        assert!(
            check_untrusted_competitive_event_listing_classifier_request_v1(
                &padded,
                assets,
                &fixture.pixels,
                &fixture.deck,
            )
            .err()
            .unwrap()
            .contains("canonical JSON")
        );

        let mut pixels = fixture.pixels.clone();
        pixels[0] ^= 0xff;
        assert!(
            check_untrusted_competitive_event_listing_classifier_request_v1(
                &bytes,
                assets,
                &pixels,
                &fixture.deck,
            )
            .err()
            .unwrap()
            .contains("pixels or assets")
        );

        assert!(
            check_untrusted_competitive_event_listing_classifier_request_v1(
                &bytes,
                b"changed-assets-v1",
                &fixture.pixels,
                &fixture.deck,
            )
            .err()
            .unwrap()
            .contains("pixels or assets")
        );

        let mut changed = header;
        changed.target.event_identity_sha256 = digest('e');
        let changed_bytes = serde_json::to_vec(&changed).unwrap();
        assert!(
            check_untrusted_competitive_event_listing_classifier_request_v1(
                &changed_bytes,
                assets,
                &fixture.pixels,
                &fixture.deck,
            )
            .err()
            .unwrap()
            .contains("target differs")
        );
    }

    #[test]
    fn parser_response_requires_one_canonical_exact_request_value() {
        let fixture = fixture_v1();
        let request_commitment = digest('f');
        let response = MtgoCompetitiveEventListingClassifierProcessResponseV1 {
            schema_version: 1,
            request_commitment_sha256: request_commitment.clone(),
            selection: fixture.raw,
        };
        let bytes = serde_json::to_vec(&response).unwrap();
        assert_eq!(
            parse_event_listing_classifier_response_v1(&bytes, &request_commitment).unwrap(),
            response
        );

        let mut padded = bytes.clone();
        padded.push(b'\n');
        assert!(parse_event_listing_classifier_response_v1(&padded, &request_commitment).is_err());
        assert!(parse_event_listing_classifier_response_v1(&bytes, &digest('e')).is_err());

        let mut value = serde_json::to_value(&response).unwrap();
        value["fabricated_input_authority"] = serde_json::json!(true);
        let unknown = serde_json::to_vec(&value).unwrap();
        assert!(parse_event_listing_classifier_response_v1(&unknown, &request_commitment).is_err());
    }
}
