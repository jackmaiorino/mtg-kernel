use super::{
    capture_admitted_mtgo_competitive_navigation_frame_v1, capture_commitment_v3,
    choose_cursor_park_point_v3, classify_admitted_mtgo_competitive_navigation_frame_v1,
    MtgoCompetitiveNavigationFrameIdentityV1, OpaqueMtgoClassifiedCompetitiveNavigationFrameV1,
    OpaqueMtgoDxgiFrameCandidateV3, OpaqueMtgoRetainedCompetitiveNavigationClassificationV1,
    OpaqueMtgoVerifiedCompetitiveNavigationClassifierRuntimeV1,
};
use crate::{sha256_hex_v1, SignedRectV1};
use mtgo_blackbox_v1::{
    visible_frame_region_content_sha256_v1, AdmittedMtgoCompetitiveNavigationProfileV1,
    CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1, MtgoCompetitiveEntryResourceV1,
    MtgoCompetitiveEntryTermsV1, MtgoCompetitiveEventKindV1, MtgoCompetitiveLifecyclePhaseV1,
    MtgoLifecycleVisibleFactKindV1, MtgoLifecycleVisibleFactV1, MtgoRectPxV1, MtgoSizePxV1,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::thread;
use std::time::{Duration, Instant};

const OPAQUE_COMPETITIVE_ENTRY_REVIEW_IDENTITY_DOMAIN_V1: &[u8] =
    b"mtgo-opaque-competitive-entry-review-identity-v1";
const OPAQUE_COMPETITIVE_CLASSIFIER_BOUND_ENTRY_REVIEW_IDENTITY_DOMAIN_V2: &[u8] =
    b"mtgo-opaque-competitive-classifier-bound-entry-review-identity-v2";
const OPAQUE_COMPETITIVE_ENTRY_CONTROL_AND_DECK_DRY_RUN_DOMAIN_V2: &[u8] =
    b"mtgo-opaque-competitive-entry-control-and-deck-dry-run-v2";
const COMPETITIVE_ENTRY_FRAME_TRANSITION_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-entry-frame-transition-v1";
const COMPETITIVE_ENTRY_WINDOW_CONTINUITY_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-entry-window-continuity-v1";
const COMPETITIVE_ENTRY_IMMEDIATE_RECAPTURE_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-entry-immediate-recapture-v1";
const COMPETITIVE_ENTRY_VISIBLE_CONFIRMATION_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-entry-visible-confirmation-v1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoOpaqueCompetitiveEntryReviewIdentityCommitmentsV1 {
    pub source_capture_commitment_sha256: String,
    pub source_lifecycle_snapshot_commitment_sha256: String,
    pub source_window_title_sha256: String,
    pub event_label_region_sha256: String,
    pub source_identity_commitment_sha256: String,
    pub source_navigation_classification_result_commitment_sha256: Option<String>,
    pub event_kind: MtgoCompetitiveEventKindV1,
    pub frame_id: u64,
    pub frame_sequence: u64,
    pub resource: MtgoCompetitiveEntryResourceV1,
    pub amount: u32,
}

/// Exact visible entry-review identity bound to one retained composed-desktop
/// navigation frame. The label remains a human-reviewed interpretation of its
/// committed pixel region. This value is move-only and has no input, entry, or
/// spending conversion.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoCompetitiveEntryReviewIdentityV1;
/// let _forged = OpaqueMtgoCompetitiveEntryReviewIdentityV1 {};
/// ```
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoCompetitiveEntryReviewIdentityV1;
/// fn require_clone<T: Clone>() {}
/// require_clone::<OpaqueMtgoCompetitiveEntryReviewIdentityV1>();
/// ```
pub struct OpaqueMtgoCompetitiveEntryReviewIdentityV1 {
    _source_frame: OpaqueMtgoDxgiFrameCandidateV3,
    _navigation_classification: Option<OpaqueMtgoRetainedCompetitiveNavigationClassificationV1>,
    _event_label_rect_client_px: MtgoRectPxV1,
    lifecycle: CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1,
    event_display_label: String,
    commitments: MtgoOpaqueCompetitiveEntryReviewIdentityCommitmentsV1,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoOpaqueCompetitiveEntryControlDryRunCommitmentsV1 {
    pub source_identity_commitment_sha256: String,
    pub source_capture_commitment_sha256: String,
    pub source_lifecycle_snapshot_commitment_sha256: String,
    pub source_navigation_classification_result_commitment_sha256: String,
    pub visible_control_label_sha256: String,
    pub visible_control_region_sha256: String,
    pub selected_deck_label_sha256: String,
    pub selected_deck_region_sha256: String,
    pub deck_manifest_sha256: String,
    pub deck_format_sha256: String,
    pub policy_deployment_commitment_sha256: String,
    pub visibly_enabled_confirmed: bool,
    pub dry_run_commitment_sha256: String,
    pub event_kind: MtgoCompetitiveEventKindV1,
    pub frame_id: u64,
    pub frame_sequence: u64,
    pub resource: MtgoCompetitiveEntryResourceV1,
    pub amount: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoCompetitiveEntryDeckSelectionReviewInputV1 {
    pub selected_deck_label: String,
    pub selected_deck_rect_client_px: MtgoRectPxV1,
    pub deck_manifest_sha256: String,
    pub deck_format_sha256: String,
    pub policy_deployment_commitment_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoCompetitiveEntryFrameTransitionCommitmentsV1 {
    pub navigation_profile_commitment_sha256: String,
    pub navigation_profile_admission_commitment_sha256: String,
    pub approved_account_alias_sha256: String,
    pub runtime_identity_commitment_sha256: String,
    pub window_continuity_commitment_sha256: String,
    pub source_identity_commitment_sha256: String,
    pub before_capture_commitment_sha256: String,
    pub before_classification_result_commitment_sha256: String,
    pub before_lifecycle_snapshot_commitment_sha256: String,
    pub after_capture_commitment_sha256: String,
    pub after_classification_result_commitment_sha256: String,
    pub after_lifecycle_snapshot_commitment_sha256: String,
    pub event_kind: MtgoCompetitiveEventKindV1,
    pub event_identity_sha256: String,
    pub before_frame_id: u64,
    pub before_frame_sequence: u64,
    pub after_frame_id: u64,
    pub after_frame_sequence: u64,
    pub transition_commitment_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoCompetitiveEntryImmediateRecaptureCommitmentsV1 {
    pub navigation_profile_commitment_sha256: String,
    pub navigation_profile_admission_commitment_sha256: String,
    pub approved_account_alias_sha256: String,
    pub runtime_identity_commitment_sha256: String,
    pub window_continuity_commitment_sha256: String,
    pub source_identity_commitment_sha256: String,
    pub source_capture_commitment_sha256: String,
    pub source_classification_result_commitment_sha256: String,
    pub source_lifecycle_snapshot_commitment_sha256: String,
    pub immediate_capture_commitment_sha256: String,
    pub immediate_classification_result_commitment_sha256: String,
    pub immediate_lifecycle_snapshot_commitment_sha256: String,
    pub event_label_region_sha256: String,
    pub visible_control_label_sha256: String,
    pub visible_control_region_sha256: String,
    pub selected_deck_label_sha256: String,
    pub selected_deck_region_sha256: String,
    pub deck_manifest_sha256: String,
    pub deck_format_sha256: String,
    pub policy_deployment_commitment_sha256: String,
    pub event_identity_sha256: String,
    pub entry_terms_sha256: String,
    pub event_kind: MtgoCompetitiveEventKindV1,
    pub resource: MtgoCompetitiveEntryResourceV1,
    pub amount: u32,
    pub source_frame_id: u64,
    pub source_frame_sequence: u64,
    pub source_captured_at_unix_millis: u128,
    pub immediate_frame_id: u64,
    pub immediate_frame_sequence: u64,
    pub immediate_captured_at_unix_millis: u128,
    pub recapture_commitment_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MtgoCompetitiveEntryPointerTargetV1 {
    pub(crate) hwnd: u64,
    pub(crate) process_id: u32,
    pub(crate) process_start_filetime_100ns: u64,
    pub(crate) dpi: u32,
    pub(crate) client_rect_desktop_px: SignedRectV1,
    pub(crate) target_x_desktop_px: i32,
    pub(crate) target_y_desktop_px: i32,
    pub(crate) park_x_desktop_px: i32,
    pub(crate) park_y_desktop_px: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MtgoCompetitiveEntryVisibleConfirmationCommitmentsV1 {
    pub(crate) frame_transition: MtgoCompetitiveEntryFrameTransitionCommitmentsV1,
    pub(crate) after_capture_commitment_sha256: String,
    pub(crate) after_classification_result_commitment_sha256: String,
    pub(crate) after_lifecycle_snapshot_commitment_sha256: String,
    pub(crate) after_captured_at_unix_millis: u128,
    pub(crate) postcondition_candidate_count: u32,
    pub(crate) confirmation_commitment_sha256: String,
}

pub(crate) struct OpaqueMtgoConfirmedCompetitiveEntryPostconditionV1 {
    _before_frame: OpaqueMtgoClassifiedCompetitiveNavigationFrameV1,
    _after_frame: OpaqueMtgoClassifiedCompetitiveNavigationFrameV1,
    commitments: MtgoCompetitiveEntryVisibleConfirmationCommitmentsV1,
}

impl OpaqueMtgoConfirmedCompetitiveEntryPostconditionV1 {
    pub(crate) fn commitments_v1(&self) -> MtgoCompetitiveEntryVisibleConfirmationCommitmentsV1 {
        self.commitments.clone()
    }

    pub(crate) fn into_after_frame_v1(self) -> OpaqueMtgoClassifiedCompetitiveNavigationFrameV1 {
        self._after_frame
    }
}

#[derive(Clone)]
struct MtgoCompetitiveEntryFrameTransitionViewV1 {
    navigation_profile_commitment_sha256: String,
    navigation_profile_admission_commitment_sha256: String,
    approved_account_alias_sha256: String,
    runtime_identity_commitment_sha256: String,
    window_continuity_commitment_sha256: String,
    source_identity_commitment_sha256: Option<String>,
    capture_commitment_sha256: String,
    canonical_bgra8_sha256: String,
    classification_result_commitment_sha256: String,
    lifecycle_snapshot_commitment_sha256: String,
    event_kind: MtgoCompetitiveEventKindV1,
    phase: MtgoCompetitiveLifecyclePhaseV1,
    event_identity_sha256: String,
    frame_id: u64,
    frame_sequence: u64,
    captured_at_unix_millis: u128,
}

/// One human-reviewed visible Confirm Entry control bound to the exact opaque
/// classifier-backed entry-review frame. The control rectangle, source pixels,
/// classifier result, and source identity remain retained and private.
///
/// This is a dry-run calibration only. It cannot create a lifecycle intent,
/// expose coordinates, click Join, spend resources, or send input.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoCompetitiveEntryControlDryRunV1;
/// let _forged = OpaqueMtgoCompetitiveEntryControlDryRunV1 {};
/// ```
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoCompetitiveEntryControlDryRunV1;
/// fn cannot_extract(value: &OpaqueMtgoCompetitiveEntryControlDryRunV1) {
///     let _ = value.rect_client_px();
///     let _ = value.input_command();
/// }
/// ```
pub struct OpaqueMtgoCompetitiveEntryControlDryRunV1 {
    _source_identity: OpaqueMtgoCompetitiveEntryReviewIdentityV1,
    _visible_control_label: String,
    _control_rect_client_px: MtgoRectPxV1,
    _selected_deck_label: String,
    _selected_deck_rect_client_px: MtgoRectPxV1,
    commitments: MtgoOpaqueCompetitiveEntryControlDryRunCommitmentsV1,
}

pub(crate) struct MtgoCompetitiveEntryControlDryRunPartsV1 {
    pub source_identity: OpaqueMtgoCompetitiveEntryReviewIdentityV1,
    pub visible_control_label: String,
    pub control_rect_client_px: MtgoRectPxV1,
    pub selected_deck_label: String,
    pub selected_deck_rect_client_px: MtgoRectPxV1,
    pub commitments: MtgoOpaqueCompetitiveEntryControlDryRunCommitmentsV1,
}

impl OpaqueMtgoCompetitiveEntryControlDryRunV1 {
    pub fn commitments_v1(&self) -> MtgoOpaqueCompetitiveEntryControlDryRunCommitmentsV1 {
        self.commitments.clone()
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

    pub(crate) fn into_parts_v1(self) -> MtgoCompetitiveEntryControlDryRunPartsV1 {
        MtgoCompetitiveEntryControlDryRunPartsV1 {
            source_identity: self._source_identity,
            visible_control_label: self._visible_control_label,
            control_rect_client_px: self._control_rect_client_px,
            selected_deck_label: self._selected_deck_label,
            selected_deck_rect_client_px: self._selected_deck_rect_client_px,
            commitments: self.commitments,
        }
    }
}

impl OpaqueMtgoCompetitiveEntryReviewIdentityV1 {
    pub fn commitments_v1(&self) -> MtgoOpaqueCompetitiveEntryReviewIdentityCommitmentsV1 {
        self.commitments.clone()
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

    pub(crate) fn lifecycle_v1(&self) -> &CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1 {
        &self.lifecycle
    }

    pub(crate) fn event_display_label_v1(&self) -> &str {
        &self.event_display_label
    }

    pub(crate) fn event_label_rect_client_px_v1(&self) -> &MtgoRectPxV1 {
        &self._event_label_rect_client_px
    }
}

/// Binds one checked entry-review interpretation to the exact retained pixels
/// of one composed-desktop main-client frame. Every lifecycle fact is rehashed
/// from those pixels. The output remains checked-untrusted because this does
/// not establish classifier accuracy or eliminate the transient-occluder race.
pub fn bind_opaque_navigation_frame_to_competitive_entry_review_identity_v1(
    source_frame: OpaqueMtgoDxgiFrameCandidateV3,
    lifecycle: CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1,
    event_display_label: String,
    event_label_rect_client_px: MtgoRectPxV1,
) -> Result<OpaqueMtgoCompetitiveEntryReviewIdentityV1, String> {
    bind_opaque_navigation_frame_to_competitive_entry_review_identity_with_classification_v1(
        source_frame,
        lifecycle,
        event_display_label,
        event_label_rect_client_px,
        None,
    )
}

pub(super) fn bind_opaque_navigation_frame_to_competitive_entry_review_identity_with_classification_v1(
    source_frame: OpaqueMtgoDxgiFrameCandidateV3,
    lifecycle: CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1,
    event_display_label: String,
    event_label_rect_client_px: MtgoRectPxV1,
    navigation_classification: Option<OpaqueMtgoRetainedCompetitiveNavigationClassificationV1>,
) -> Result<OpaqueMtgoCompetitiveEntryReviewIdentityV1, String> {
    let recomputed_capture_commitment = capture_commitment_v3(
        &source_frame.manifest,
        &source_frame.canonical_bgra8,
        &source_frame.preview_png,
    )?;
    if recomputed_capture_commitment != source_frame.capture_commitment_sha256 {
        return Err("competitive entry source capture commitment changed".to_owned());
    }
    let manifest = &source_frame.manifest;
    if manifest.window_mode != "main_client"
        || manifest.capture_role != "navigation"
        || !manifest.expected_game_format.is_empty()
        || manifest.pre.title != manifest.post.title
        || manifest.safety.safe_for_semantic_evidence
        || manifest.safety.safe_for_ocr
        || manifest.safety.safe_for_policy_scoring
        || manifest.safety.safe_for_input
        || !manifest.safety.authenticode_verified_in_probe
    {
        return Err(
            "competitive entry review requires one exact non-actionable main-client navigation capture"
                .to_owned(),
        );
    }
    let source_capture = source_frame.commitments_v3();
    let navigation_classification_commitment = navigation_classification.as_ref().map(|value| {
        value
            .commitments_v1()
            .classification_result_commitment_sha256
    });
    let commitments = bind_competitive_entry_review_source_parts_v1(
        &source_capture.capture_commitment_sha256,
        &source_capture.canonical_bgra8_sha256,
        source_capture.canonical_width,
        source_capture.canonical_height,
        &source_frame.canonical_bgra8,
        &manifest.pre.title,
        "main_client",
        "navigation",
        &lifecycle,
        &event_display_label,
        &event_label_rect_client_px,
        navigation_classification_commitment.as_deref(),
    )?;
    Ok(OpaqueMtgoCompetitiveEntryReviewIdentityV1 {
        _source_frame: source_frame,
        _navigation_classification: navigation_classification,
        _event_label_rect_client_px: event_label_rect_client_px,
        lifecycle,
        event_display_label,
        commitments,
    })
}

/// Adds a coordinate-private, human-reviewed Confirm Entry control and exact
/// visible selected-deck calibration to one classifier-backed entry-review
/// identity. This function only records a dry run and cannot enter the event.
pub fn bind_classifier_backed_competitive_entry_control_and_deck_dry_run_v2(
    source_identity: OpaqueMtgoCompetitiveEntryReviewIdentityV1,
    visible_control_label: String,
    control_rect_client_px: MtgoRectPxV1,
    visibly_enabled_confirmed: bool,
    deck_selection: MtgoCompetitiveEntryDeckSelectionReviewInputV1,
) -> Result<OpaqueMtgoCompetitiveEntryControlDryRunV1, String> {
    let source_commitments = source_identity.commitments_v1();
    let retained_classifier_commitment = source_identity
        ._navigation_classification
        .as_ref()
        .ok_or("competitive entry control dry run requires retained classifier lineage")?
        .commitments_v1()
        .classification_result_commitment_sha256;
    if source_commitments
        .source_navigation_classification_result_commitment_sha256
        .as_deref()
        != Some(retained_classifier_commitment.as_str())
    {
        return Err("competitive entry control classifier lineage changed".to_owned());
    }
    let recomputed_capture_commitment = capture_commitment_v3(
        &source_identity._source_frame.manifest,
        &source_identity._source_frame.canonical_bgra8,
        &source_identity._source_frame.preview_png,
    )?;
    if recomputed_capture_commitment != source_commitments.source_capture_commitment_sha256
        || recomputed_capture_commitment != source_identity._source_frame.capture_commitment_sha256
    {
        return Err("competitive entry control source capture changed".to_owned());
    }
    let source_size = MtgoSizePxV1 {
        width: source_identity._source_frame.manifest.frame.canonical_width,
        height: source_identity
            ._source_frame
            .manifest
            .frame
            .canonical_height,
    };
    let commitments = bind_competitive_entry_control_dry_run_parts_v1(
        &source_commitments,
        source_identity.lifecycle_v1(),
        &source_identity._source_frame.canonical_bgra8,
        &source_size,
        &source_identity._event_label_rect_client_px,
        &visible_control_label,
        &control_rect_client_px,
        visibly_enabled_confirmed,
        &deck_selection.selected_deck_label,
        &deck_selection.selected_deck_rect_client_px,
        &deck_selection.deck_manifest_sha256,
        &deck_selection.deck_format_sha256,
        &deck_selection.policy_deployment_commitment_sha256,
    )?;
    Ok(OpaqueMtgoCompetitiveEntryControlDryRunV1 {
        _source_identity: source_identity,
        _visible_control_label: visible_control_label,
        _control_rect_client_px: control_rect_client_px,
        _selected_deck_label: deck_selection.selected_deck_label,
        _selected_deck_rect_client_px: deck_selection.selected_deck_rect_client_px,
        commitments,
    })
}

pub(crate) fn validate_classifier_backed_competitive_entry_frame_transition_v1(
    before: &OpaqueMtgoCompetitiveEntryReviewIdentityV1,
    after: &OpaqueMtgoClassifiedCompetitiveNavigationFrameV1,
) -> Result<MtgoCompetitiveEntryFrameTransitionCommitmentsV1, String> {
    let before_view = entry_review_transition_view_v1(before)?;
    let after_view = entered_waiting_transition_view_v1(after)?;
    bind_competitive_entry_frame_transition_views_v1(&before_view, &after_view)
}

pub(crate) fn validate_classifier_backed_competitive_entry_immediate_recapture_v1(
    source: &OpaqueMtgoCompetitiveEntryReviewIdentityV1,
    control_rect_client_px: &MtgoRectPxV1,
    selected_deck_rect_client_px: &MtgoRectPxV1,
    expected_control: &MtgoOpaqueCompetitiveEntryControlDryRunCommitmentsV1,
    immediate: &OpaqueMtgoClassifiedCompetitiveNavigationFrameV1,
) -> Result<MtgoCompetitiveEntryImmediateRecaptureCommitmentsV1, String> {
    let source_view = entry_review_transition_view_v1(source)?;
    let immediate_classification = immediate.commitments_v1();
    if immediate_classification.phase != MtgoCompetitiveLifecyclePhaseV1::EntryReview
        || immediate._lifecycle.phase() != MtgoCompetitiveLifecyclePhaseV1::EntryReview
        || immediate_classification.event_kind != immediate._lifecycle.event_kind()
        || immediate_classification.frame_id != immediate._lifecycle.frame_id_v1()
        || immediate_classification.frame_sequence != immediate._lifecycle.frame_sequence()
        || immediate_classification.lifecycle_snapshot_commitment_sha256
            != immediate._lifecycle.snapshot_commitment_sha256()
    {
        return Err(
            "competitive entry immediate recapture is not the exact entry-review lifecycle"
                .to_owned(),
        );
    }
    let immediate_frame = &immediate._source_frame.source_frame;
    let immediate_capture_commitment_sha256 = capture_commitment_v3(
        &immediate_frame.manifest,
        &immediate_frame.canonical_bgra8,
        &immediate_frame.preview_png,
    )?;
    if immediate_capture_commitment_sha256 != immediate_frame.capture_commitment_sha256
        || immediate_capture_commitment_sha256
            != immediate_classification
                .source_frame
                .source_capture
                .capture_commitment_sha256
        || immediate._source_frame.commitments_v1() != immediate_classification.source_frame
    {
        return Err("competitive entry immediate recapture changed its capture lineage".to_owned());
    }
    let immediate_view = classified_transition_view_v1(
        immediate_frame,
        &immediate_classification,
        &immediate._lifecycle,
        None,
    )?;
    validate_competitive_entry_immediate_recapture_views_v1(&source_view, &immediate_view)?;
    let source_identity = source.commitments_v1();
    let source_terms = source
        .lifecycle_v1()
        .entry_terms_v1()
        .ok_or("competitive entry source has no exact visible entry terms")?;
    let immediate_terms = immediate
        ._lifecycle
        .entry_terms_v1()
        .ok_or("competitive entry immediate recapture has no exact visible entry terms")?;
    let source_size = MtgoSizePxV1 {
        width: source._source_frame.manifest.frame.canonical_width,
        height: source._source_frame.manifest.frame.canonical_height,
    };
    let immediate_size = MtgoSizePxV1 {
        width: immediate_frame.manifest.frame.canonical_width,
        height: immediate_frame.manifest.frame.canonical_height,
    };
    if source_size != immediate_size {
        return Err(
            "competitive entry immediate recapture changed client pixel geometry".to_owned(),
        );
    }
    let source_event_label_region_sha256 = visible_frame_region_content_sha256_v1(
        &source._source_frame.canonical_bgra8,
        &source_size,
        &source._event_label_rect_client_px,
    )
    .map_err(|error| format!("rehash source entry event label: {error}"))?;
    let immediate_event_label_region_sha256 = visible_frame_region_content_sha256_v1(
        &immediate_frame.canonical_bgra8,
        &immediate_size,
        &source._event_label_rect_client_px,
    )
    .map_err(|error| format!("rehash immediate entry event label: {error}"))?;
    let source_control_region_sha256 = visible_frame_region_content_sha256_v1(
        &source._source_frame.canonical_bgra8,
        &source_size,
        control_rect_client_px,
    )
    .map_err(|error| format!("rehash source entry control: {error}"))?;
    let immediate_control_region_sha256 = visible_frame_region_content_sha256_v1(
        &immediate_frame.canonical_bgra8,
        &immediate_size,
        control_rect_client_px,
    )
    .map_err(|error| format!("rehash immediate entry control: {error}"))?;
    let source_selected_deck_region_sha256 = visible_frame_region_content_sha256_v1(
        &source._source_frame.canonical_bgra8,
        &source_size,
        selected_deck_rect_client_px,
    )
    .map_err(|error| format!("rehash source selected deck: {error}"))?;
    let immediate_selected_deck_region_sha256 = visible_frame_region_content_sha256_v1(
        &immediate_frame.canonical_bgra8,
        &immediate_size,
        selected_deck_rect_client_px,
    )
    .map_err(|error| format!("rehash immediate selected deck: {error}"))?;
    validate_competitive_entry_immediate_visible_state_v1(
        &source_view,
        &source_identity,
        expected_control,
        source_terms,
        immediate_terms,
        source.lifecycle_v1().visible_facts_v1(),
        immediate._lifecycle.visible_facts_v1(),
        &source_event_label_region_sha256,
        &immediate_event_label_region_sha256,
        &source_control_region_sha256,
        &immediate_control_region_sha256,
        &source_selected_deck_region_sha256,
        &immediate_selected_deck_region_sha256,
    )?;
    for digest in [
        source_identity.source_identity_commitment_sha256.as_str(),
        source_view.navigation_profile_commitment_sha256.as_str(),
        source_view
            .navigation_profile_admission_commitment_sha256
            .as_str(),
        source_view.approved_account_alias_sha256.as_str(),
        source_view.runtime_identity_commitment_sha256.as_str(),
        source_view.window_continuity_commitment_sha256.as_str(),
        source_view.capture_commitment_sha256.as_str(),
        source_view.classification_result_commitment_sha256.as_str(),
        source_view.lifecycle_snapshot_commitment_sha256.as_str(),
        immediate_view.capture_commitment_sha256.as_str(),
        immediate_view
            .classification_result_commitment_sha256
            .as_str(),
        immediate_view.lifecycle_snapshot_commitment_sha256.as_str(),
        source_identity.event_label_region_sha256.as_str(),
        expected_control.visible_control_label_sha256.as_str(),
        expected_control.visible_control_region_sha256.as_str(),
        expected_control.selected_deck_label_sha256.as_str(),
        expected_control.selected_deck_region_sha256.as_str(),
        expected_control.deck_manifest_sha256.as_str(),
        expected_control.deck_format_sha256.as_str(),
        expected_control
            .policy_deployment_commitment_sha256
            .as_str(),
        source_view.event_identity_sha256.as_str(),
        source_terms.terms_sha256.as_str(),
    ] {
        if !looks_like_lower_sha256_v1(digest) {
            return Err(
                "competitive entry immediate recapture contains an invalid commitment".to_owned(),
            );
        }
    }
    let event_kind = canonical_json_v1(&source_view.event_kind, "entry recapture mode")?;
    let resource = canonical_json_v1(&source_terms.resource, "entry recapture resource")?;
    let lifecycle_facts = canonical_json_v1(
        source.lifecycle_v1().visible_facts_v1(),
        "entry recapture lifecycle facts",
    )?;
    let recapture_commitment_sha256 = commitment_v1(
        COMPETITIVE_ENTRY_IMMEDIATE_RECAPTURE_DOMAIN_V1,
        &[
            source_identity.source_identity_commitment_sha256.as_bytes(),
            source_view.navigation_profile_commitment_sha256.as_bytes(),
            source_view
                .navigation_profile_admission_commitment_sha256
                .as_bytes(),
            source_view.approved_account_alias_sha256.as_bytes(),
            source_view.runtime_identity_commitment_sha256.as_bytes(),
            source_view.window_continuity_commitment_sha256.as_bytes(),
            source_view.capture_commitment_sha256.as_bytes(),
            source_view.classification_result_commitment_sha256.as_bytes(),
            source_view.lifecycle_snapshot_commitment_sha256.as_bytes(),
            immediate_view.capture_commitment_sha256.as_bytes(),
            immediate_view
                .classification_result_commitment_sha256
                .as_bytes(),
            immediate_view
                .lifecycle_snapshot_commitment_sha256
                .as_bytes(),
            source_identity.event_label_region_sha256.as_bytes(),
            expected_control.visible_control_label_sha256.as_bytes(),
            expected_control.visible_control_region_sha256.as_bytes(),
            expected_control.selected_deck_label_sha256.as_bytes(),
            expected_control.selected_deck_region_sha256.as_bytes(),
            expected_control.deck_manifest_sha256.as_bytes(),
            expected_control.deck_format_sha256.as_bytes(),
            expected_control
                .policy_deployment_commitment_sha256
                .as_bytes(),
            &lifecycle_facts,
            &event_kind,
            source_view.event_identity_sha256.as_bytes(),
            source_terms.terms_sha256.as_bytes(),
            &resource,
            source_terms.amount.to_be_bytes().as_slice(),
            source_view.frame_id.to_be_bytes().as_slice(),
            source_view.frame_sequence.to_be_bytes().as_slice(),
            immediate_view.frame_id.to_be_bytes().as_slice(),
            immediate_view.frame_sequence.to_be_bytes().as_slice(),
            source_view.captured_at_unix_millis.to_be_bytes().as_slice(),
            immediate_view
                .captured_at_unix_millis
                .to_be_bytes()
                .as_slice(),
            b"fresh_entry_review_event_terms_enabled_control_and_selected_deck_exact_no_input_no_entry_no_spending",
        ],
    );
    Ok(MtgoCompetitiveEntryImmediateRecaptureCommitmentsV1 {
        navigation_profile_commitment_sha256: source_view.navigation_profile_commitment_sha256,
        navigation_profile_admission_commitment_sha256: source_view
            .navigation_profile_admission_commitment_sha256,
        approved_account_alias_sha256: source_view.approved_account_alias_sha256,
        runtime_identity_commitment_sha256: source_view.runtime_identity_commitment_sha256,
        window_continuity_commitment_sha256: source_view.window_continuity_commitment_sha256,
        source_identity_commitment_sha256: source_identity.source_identity_commitment_sha256,
        source_capture_commitment_sha256: source_view.capture_commitment_sha256,
        source_classification_result_commitment_sha256: source_view
            .classification_result_commitment_sha256,
        source_lifecycle_snapshot_commitment_sha256: source_view
            .lifecycle_snapshot_commitment_sha256,
        immediate_capture_commitment_sha256: immediate_view.capture_commitment_sha256,
        immediate_classification_result_commitment_sha256: immediate_view
            .classification_result_commitment_sha256,
        immediate_lifecycle_snapshot_commitment_sha256: immediate_view
            .lifecycle_snapshot_commitment_sha256,
        event_label_region_sha256: source_identity.event_label_region_sha256,
        visible_control_label_sha256: expected_control.visible_control_label_sha256.clone(),
        visible_control_region_sha256: expected_control.visible_control_region_sha256.clone(),
        selected_deck_label_sha256: expected_control.selected_deck_label_sha256.clone(),
        selected_deck_region_sha256: expected_control.selected_deck_region_sha256.clone(),
        deck_manifest_sha256: expected_control.deck_manifest_sha256.clone(),
        deck_format_sha256: expected_control.deck_format_sha256.clone(),
        policy_deployment_commitment_sha256: expected_control
            .policy_deployment_commitment_sha256
            .clone(),
        event_identity_sha256: source_view.event_identity_sha256,
        entry_terms_sha256: source_terms.terms_sha256.clone(),
        event_kind: source_view.event_kind,
        resource: source_terms.resource,
        amount: source_terms.amount,
        source_frame_id: source_view.frame_id,
        source_frame_sequence: source_view.frame_sequence,
        source_captured_at_unix_millis: source_view.captured_at_unix_millis,
        immediate_frame_id: immediate_view.frame_id,
        immediate_frame_sequence: immediate_view.frame_sequence,
        immediate_captured_at_unix_millis: immediate_view.captured_at_unix_millis,
        recapture_commitment_sha256,
    })
}

pub(crate) fn resolve_competitive_entry_pointer_target_v1(
    immediate: &OpaqueMtgoClassifiedCompetitiveNavigationFrameV1,
    control_rect_client_px: &MtgoRectPxV1,
) -> Result<MtgoCompetitiveEntryPointerTargetV1, String> {
    if immediate.phase_v1() != MtgoCompetitiveLifecyclePhaseV1::EntryReview {
        return Err("competitive entry pointer target is not an Entry Review frame".to_owned());
    }
    let frame = &immediate._source_frame.source_frame;
    let width = frame.manifest.frame.canonical_width;
    let height = frame.manifest.frame.canonical_height;
    let right = control_rect_client_px
        .x
        .checked_add(control_rect_client_px.width)
        .ok_or("competitive entry control x overflow")?;
    let bottom = control_rect_client_px
        .y
        .checked_add(control_rect_client_px.height)
        .ok_or("competitive entry control y overflow")?;
    if control_rect_client_px.width == 0
        || control_rect_client_px.height == 0
        || right > width
        || bottom > height
    {
        return Err("competitive entry control is outside the immediate client".to_owned());
    }
    let target_x_client_px = control_rect_client_px
        .x
        .checked_add(control_rect_client_px.width / 2)
        .ok_or("competitive entry target x overflow")?;
    let target_y_client_px = control_rect_client_px
        .y
        .checked_add(control_rect_client_px.height / 2)
        .ok_or("competitive entry target y overflow")?;
    let client_rect = frame.manifest.pre.client_rect_desktop_px;
    let target_x_desktop_px = client_rect
        .left
        .checked_add(
            i32::try_from(target_x_client_px)
                .map_err(|_| "competitive entry target x does not fit the desktop")?,
        )
        .ok_or("competitive entry desktop x overflow")?;
    let target_y_desktop_px = client_rect
        .top
        .checked_add(
            i32::try_from(target_y_client_px)
                .map_err(|_| "competitive entry target y does not fit the desktop")?,
        )
        .ok_or("competitive entry desktop y overflow")?;
    if !client_rect.contains_point(target_x_desktop_px, target_y_desktop_px) {
        return Err("competitive entry target is outside the current client".to_owned());
    }
    let (park_x_desktop_px, park_y_desktop_px) =
        choose_cursor_park_point_v3(&client_rect, &frame.manifest.output.bounds_desktop_px)?;
    let pre = &frame.manifest.pre;
    Ok(MtgoCompetitiveEntryPointerTargetV1 {
        hwnd: pre.hwnd,
        process_id: pre.process_id,
        process_start_filetime_100ns: pre.process_start_filetime_100ns,
        dpi: pre.dpi,
        client_rect_desktop_px: client_rect,
        target_x_desktop_px,
        target_y_desktop_px,
        park_x_desktop_px,
        park_y_desktop_px,
    })
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn confirm_opaque_competitive_entry_postcondition_v1(
    before: OpaqueMtgoClassifiedCompetitiveNavigationFrameV1,
    source_identity_commitment_sha256: String,
    event_label_rect_client_px: MtgoRectPxV1,
    control_rect_client_px: MtgoRectPxV1,
    expected_event_label_region_sha256: String,
    expected_control_region_sha256: String,
    preparation_commitment_sha256: String,
    profile: &AdmittedMtgoCompetitiveNavigationProfileV1,
    runtime: &OpaqueMtgoVerifiedCompetitiveNavigationClassifierRuntimeV1,
    input_sent_at_unix_millis: u128,
    timeout_ms: u32,
) -> Result<OpaqueMtgoConfirmedCompetitiveEntryPostconditionV1, String> {
    if !(100..=10_000).contains(&timeout_ms) {
        return Err(
            "competitive entry confirmation timeout must be between 100 and 10000 ms".to_owned(),
        );
    }
    for digest in [
        source_identity_commitment_sha256.as_str(),
        expected_event_label_region_sha256.as_str(),
        expected_control_region_sha256.as_str(),
        preparation_commitment_sha256.as_str(),
    ] {
        if !looks_like_lower_sha256_v1(digest) {
            return Err("competitive entry confirmation has an invalid commitment".to_owned());
        }
    }
    if input_sent_at_unix_millis == 0 {
        return Err("competitive entry confirmation has no input timestamp".to_owned());
    }
    let before_view = classified_entry_review_transition_view_v1(
        &before,
        Some(source_identity_commitment_sha256),
    )?;
    if before_view.captured_at_unix_millis > input_sent_at_unix_millis
        || input_sent_at_unix_millis - before_view.captured_at_unix_millis > 2_000
    {
        return Err("competitive entry confirmation baseline was not fresh at input".to_owned());
    }
    let before_frame = &before._source_frame.source_frame;
    let before_size = MtgoSizePxV1 {
        width: before_frame.manifest.frame.canonical_width,
        height: before_frame.manifest.frame.canonical_height,
    };
    let before_event_label_region_sha256 = visible_frame_region_content_sha256_v1(
        &before_frame.canonical_bgra8,
        &before_size,
        &event_label_rect_client_px,
    )
    .map_err(|error| format!("rehash entry confirmation event label: {error}"))?;
    let before_control_region_sha256 = visible_frame_region_content_sha256_v1(
        &before_frame.canonical_bgra8,
        &before_size,
        &control_rect_client_px,
    )
    .map_err(|error| format!("rehash entry confirmation control: {error}"))?;
    if before_event_label_region_sha256 != expected_event_label_region_sha256
        || before_control_region_sha256 != expected_control_region_sha256
    {
        return Err(
            "competitive entry confirmation baseline changed its event label or control pixels"
                .to_owned(),
        );
    }
    let before_terms = before
        ._lifecycle
        .entry_terms_v1()
        .ok_or("competitive entry confirmation baseline has no exact entry terms")?
        .clone();
    let before_facts = before._lifecycle.visible_facts_v1().to_vec();
    let deadline = Instant::now()
        .checked_add(Duration::from_millis(u64::from(timeout_ms)))
        .ok_or("competitive entry confirmation deadline overflow")?;
    let mut postcondition_candidate_count = 0_u32;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining < Duration::from_millis(100) {
            return Err(format!(
                "timed out waiting for entered-and-waiting after {postcondition_candidate_count} admitted candidates"
            ));
        }
        let candidate_timeout_ms = u32::try_from(remaining.as_millis())
            .unwrap_or(u32::MAX)
            .clamp(100, 10_000);
        let candidate_source =
            capture_admitted_mtgo_competitive_navigation_frame_v1(profile, candidate_timeout_ms)?;
        postcondition_candidate_count = postcondition_candidate_count
            .checked_add(1)
            .ok_or("competitive entry postcondition candidate count overflow")?;
        let candidate_capture = candidate_source.commitments_v1();
        let candidate_captured_at_unix_millis =
            candidate_capture.source_capture.captured_at_unix_millis;
        if candidate_captured_at_unix_millis < before_view.captured_at_unix_millis {
            return Err("competitive entry postcondition clock moved backwards".to_owned());
        }
        if candidate_captured_at_unix_millis <= input_sent_at_unix_millis
            || candidate_capture.source_capture.capture_commitment_sha256
                == before_view.capture_commitment_sha256
        {
            thread::sleep(Duration::from_millis(25));
            continue;
        }
        let candidate_frame_sequence = before_view
            .frame_sequence
            .checked_add(u64::from(postcondition_candidate_count))
            .ok_or("competitive entry postcondition frame sequence overflow")?;
        let candidate_frame_id = competitive_entry_frame_id_from_capture_commitment_v1(
            &candidate_capture.source_capture.capture_commitment_sha256,
            before_view.frame_id,
        )?;
        let classifier_remaining = deadline.saturating_duration_since(Instant::now());
        if classifier_remaining < Duration::from_millis(100) {
            return Err(format!(
                "timed out before classifying competitive entry candidate {postcondition_candidate_count}"
            ));
        }
        let classifier_timeout_ms = u32::try_from(classifier_remaining.as_millis())
            .unwrap_or(u32::MAX)
            .clamp(100, 10_000);
        let candidate = classify_admitted_mtgo_competitive_navigation_frame_v1(
            candidate_source,
            profile,
            runtime,
            MtgoCompetitiveNavigationFrameIdentityV1 {
                frame_id: candidate_frame_id,
                frame_sequence: candidate_frame_sequence,
            },
            classifier_timeout_ms,
        )?;
        match candidate.phase_v1() {
            MtgoCompetitiveLifecyclePhaseV1::EntryReview => {
                let candidate_view = classified_entry_review_transition_view_v1(&candidate, None)?;
                validate_competitive_entry_immediate_recapture_views_v1(
                    &before_view,
                    &candidate_view,
                )?;
                let candidate_terms = candidate
                    ._lifecycle
                    .entry_terms_v1()
                    .ok_or("pending entry-review candidate has no exact entry terms")?;
                if candidate_terms != &before_terms
                    || candidate._lifecycle.visible_facts_v1() != before_facts.as_slice()
                {
                    return Err(
                        "pending entry-review candidate changed visible lifecycle facts or terms"
                            .to_owned(),
                    );
                }
                let candidate_frame = &candidate._source_frame.source_frame;
                let candidate_size = MtgoSizePxV1 {
                    width: candidate_frame.manifest.frame.canonical_width,
                    height: candidate_frame.manifest.frame.canonical_height,
                };
                let candidate_event_label_region_sha256 = visible_frame_region_content_sha256_v1(
                    &candidate_frame.canonical_bgra8,
                    &candidate_size,
                    &event_label_rect_client_px,
                )
                .map_err(|error| format!("rehash pending entry-review event label: {error}"))?;
                let candidate_control_region_sha256 = visible_frame_region_content_sha256_v1(
                    &candidate_frame.canonical_bgra8,
                    &candidate_size,
                    &control_rect_client_px,
                )
                .map_err(|error| format!("rehash pending entry-review control: {error}"))?;
                if candidate_event_label_region_sha256 != expected_event_label_region_sha256
                    || candidate_control_region_sha256 != expected_control_region_sha256
                {
                    return Err(
                        "pending entry-review candidate changed event-label or control pixels"
                            .to_owned(),
                    );
                }
                thread::sleep(Duration::from_millis(25));
            }
            MtgoCompetitiveLifecyclePhaseV1::EnteredWaitingForPairing => {
                let after_view = entered_waiting_transition_view_v1(&candidate)?;
                let frame_transition =
                    bind_competitive_entry_frame_transition_views_v1(&before_view, &after_view)?;
                let candidate_commitments = candidate.commitments_v1();
                let confirmation_commitment_sha256 = commitment_v1(
                    COMPETITIVE_ENTRY_VISIBLE_CONFIRMATION_DOMAIN_V1,
                    &[
                        preparation_commitment_sha256.as_bytes(),
                        frame_transition.transition_commitment_sha256.as_bytes(),
                        candidate_commitments
                            .source_frame
                            .source_capture
                            .capture_commitment_sha256
                            .as_bytes(),
                        candidate_commitments
                            .classification_result_commitment_sha256
                            .as_bytes(),
                        candidate_commitments
                            .lifecycle_snapshot_commitment_sha256
                            .as_bytes(),
                        input_sent_at_unix_millis.to_be_bytes().as_slice(),
                        candidate_captured_at_unix_millis.to_be_bytes().as_slice(),
                        postcondition_candidate_count.to_be_bytes().as_slice(),
                        b"entered_waiting_confirmed_after_exactly_one_entry_input",
                    ],
                );
                return Ok(OpaqueMtgoConfirmedCompetitiveEntryPostconditionV1 {
                    _before_frame: before,
                    _after_frame: candidate,
                    commitments: MtgoCompetitiveEntryVisibleConfirmationCommitmentsV1 {
                        frame_transition,
                        after_capture_commitment_sha256: candidate_commitments
                            .source_frame
                            .source_capture
                            .capture_commitment_sha256,
                        after_classification_result_commitment_sha256: candidate_commitments
                            .classification_result_commitment_sha256,
                        after_lifecycle_snapshot_commitment_sha256: candidate_commitments
                            .lifecycle_snapshot_commitment_sha256,
                        after_captured_at_unix_millis: candidate_captured_at_unix_millis,
                        postcondition_candidate_count,
                        confirmation_commitment_sha256,
                    },
                });
            }
            _ => {
                return Err(
                    "competitive entry postcondition entered an unexpected visible lifecycle phase"
                        .to_owned(),
                );
            }
        }
    }
}

fn validate_competitive_entry_immediate_recapture_views_v1(
    source: &MtgoCompetitiveEntryFrameTransitionViewV1,
    immediate: &MtgoCompetitiveEntryFrameTransitionViewV1,
) -> Result<(), String> {
    if source.phase != MtgoCompetitiveLifecyclePhaseV1::EntryReview
        || immediate.phase != MtgoCompetitiveLifecyclePhaseV1::EntryReview
    {
        return Err(
            "competitive entry immediate recapture requires two entry-review frames".to_owned(),
        );
    }
    if source.navigation_profile_commitment_sha256 != immediate.navigation_profile_commitment_sha256
        || source.navigation_profile_admission_commitment_sha256
            != immediate.navigation_profile_admission_commitment_sha256
        || source.approved_account_alias_sha256 != immediate.approved_account_alias_sha256
        || source.runtime_identity_commitment_sha256 != immediate.runtime_identity_commitment_sha256
        || source.window_continuity_commitment_sha256
            != immediate.window_continuity_commitment_sha256
        || source.event_kind != immediate.event_kind
        || source.event_identity_sha256 != immediate.event_identity_sha256
    {
        return Err(
            "competitive entry immediate recapture changed profile, account, runtime, window, mode, or event"
                .to_owned(),
        );
    }
    if immediate.frame_id == source.frame_id
        || immediate.frame_sequence <= source.frame_sequence
        || immediate.captured_at_unix_millis <= source.captured_at_unix_millis
        || immediate.capture_commitment_sha256 == source.capture_commitment_sha256
        || immediate.classification_result_commitment_sha256
            == source.classification_result_commitment_sha256
        || immediate.lifecycle_snapshot_commitment_sha256
            == source.lifecycle_snapshot_commitment_sha256
    {
        return Err(
            "competitive entry immediate recapture is not a strictly newer classified frame"
                .to_owned(),
        );
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn validate_competitive_entry_immediate_visible_state_v1(
    source_view: &MtgoCompetitiveEntryFrameTransitionViewV1,
    source_identity: &MtgoOpaqueCompetitiveEntryReviewIdentityCommitmentsV1,
    expected_control: &MtgoOpaqueCompetitiveEntryControlDryRunCommitmentsV1,
    source_terms: &MtgoCompetitiveEntryTermsV1,
    immediate_terms: &MtgoCompetitiveEntryTermsV1,
    source_visible_facts: &[MtgoLifecycleVisibleFactV1],
    immediate_visible_facts: &[MtgoLifecycleVisibleFactV1],
    source_event_label_region_sha256: &str,
    immediate_event_label_region_sha256: &str,
    source_control_region_sha256: &str,
    immediate_control_region_sha256: &str,
    source_selected_deck_region_sha256: &str,
    immediate_selected_deck_region_sha256: &str,
) -> Result<(), String> {
    if expected_control.source_identity_commitment_sha256
        != source_identity.source_identity_commitment_sha256
        || expected_control.source_capture_commitment_sha256
            != source_identity.source_capture_commitment_sha256
        || expected_control.source_lifecycle_snapshot_commitment_sha256
            != source_identity.source_lifecycle_snapshot_commitment_sha256
        || expected_control.source_navigation_classification_result_commitment_sha256
            != source_view.classification_result_commitment_sha256
        || expected_control.event_kind != source_view.event_kind
        || expected_control.frame_id != source_view.frame_id
        || expected_control.frame_sequence != source_view.frame_sequence
        || expected_control.resource != source_identity.resource
        || expected_control.amount != source_identity.amount
        || !expected_control.visibly_enabled_confirmed
    {
        return Err(
            "competitive entry immediate recapture changed the exact enabled control lineage"
                .to_owned(),
        );
    }
    if source_terms != immediate_terms
        || source_terms.resource != expected_control.resource
        || source_terms.amount != expected_control.amount
        || source_visible_facts != immediate_visible_facts
    {
        return Err(
            "competitive entry immediate recapture changed visible lifecycle facts or entry terms"
                .to_owned(),
        );
    }
    if source_event_label_region_sha256 != source_identity.event_label_region_sha256
        || immediate_event_label_region_sha256 != source_identity.event_label_region_sha256
        || source_control_region_sha256 != expected_control.visible_control_region_sha256
        || immediate_control_region_sha256 != expected_control.visible_control_region_sha256
        || source_selected_deck_region_sha256 != expected_control.selected_deck_region_sha256
        || immediate_selected_deck_region_sha256 != expected_control.selected_deck_region_sha256
    {
        return Err(
            "competitive entry immediate recapture changed event-label, enabled-control, or selected-deck pixels"
                .to_owned(),
        );
    }
    Ok(())
}

fn entry_review_transition_view_v1(
    source: &OpaqueMtgoCompetitiveEntryReviewIdentityV1,
) -> Result<MtgoCompetitiveEntryFrameTransitionViewV1, String> {
    let identity = source.commitments_v1();
    let classification = source
        ._navigation_classification
        .as_ref()
        .ok_or("competitive entry transition requires retained before-frame classifier lineage")?
        .commitments_v1();
    if classification.phase != MtgoCompetitiveLifecyclePhaseV1::EntryReview
        || source.lifecycle.phase() != MtgoCompetitiveLifecyclePhaseV1::EntryReview
        || classification.event_kind != source.lifecycle.event_kind()
        || classification.frame_id != source.lifecycle.frame_id_v1()
        || classification.frame_sequence != source.lifecycle.frame_sequence()
        || classification.lifecycle_snapshot_commitment_sha256
            != source.lifecycle.snapshot_commitment_sha256()
        || identity
            .source_navigation_classification_result_commitment_sha256
            .as_deref()
            != Some(
                classification
                    .classification_result_commitment_sha256
                    .as_str(),
            )
        || identity.source_capture_commitment_sha256
            != classification
                .source_frame
                .source_capture
                .capture_commitment_sha256
        || identity.source_lifecycle_snapshot_commitment_sha256
            != classification.lifecycle_snapshot_commitment_sha256
    {
        return Err("competitive entry transition before-frame lineage changed".to_owned());
    }
    let capture_commitment = capture_commitment_v3(
        &source._source_frame.manifest,
        &source._source_frame.canonical_bgra8,
        &source._source_frame.preview_png,
    )?;
    if capture_commitment != source._source_frame.capture_commitment_sha256
        || capture_commitment != identity.source_capture_commitment_sha256
    {
        return Err("competitive entry transition before capture changed".to_owned());
    }
    classified_transition_view_v1(
        &source._source_frame,
        &classification,
        source.lifecycle_v1(),
        Some(identity.source_identity_commitment_sha256),
    )
}

fn classified_entry_review_transition_view_v1(
    source: &OpaqueMtgoClassifiedCompetitiveNavigationFrameV1,
    source_identity_commitment_sha256: Option<String>,
) -> Result<MtgoCompetitiveEntryFrameTransitionViewV1, String> {
    let classification = source.commitments_v1();
    if classification.phase != MtgoCompetitiveLifecyclePhaseV1::EntryReview
        || source._lifecycle.phase() != MtgoCompetitiveLifecyclePhaseV1::EntryReview
        || classification.event_kind != source._lifecycle.event_kind()
        || classification.frame_id != source._lifecycle.frame_id_v1()
        || classification.frame_sequence != source._lifecycle.frame_sequence()
        || classification.lifecycle_snapshot_commitment_sha256
            != source._lifecycle.snapshot_commitment_sha256()
    {
        return Err(
            "competitive entry confirmation baseline is not the exact Entry Review lifecycle"
                .to_owned(),
        );
    }
    let frame = &source._source_frame.source_frame;
    let capture_commitment =
        capture_commitment_v3(&frame.manifest, &frame.canonical_bgra8, &frame.preview_png)?;
    if capture_commitment != frame.capture_commitment_sha256
        || capture_commitment
            != classification
                .source_frame
                .source_capture
                .capture_commitment_sha256
        || source._source_frame.commitments_v1() != classification.source_frame
    {
        return Err("competitive entry confirmation baseline capture changed".to_owned());
    }
    classified_transition_view_v1(
        frame,
        &classification,
        &source._lifecycle,
        source_identity_commitment_sha256,
    )
}

fn entered_waiting_transition_view_v1(
    source: &OpaqueMtgoClassifiedCompetitiveNavigationFrameV1,
) -> Result<MtgoCompetitiveEntryFrameTransitionViewV1, String> {
    let classification = source.commitments_v1();
    if classification.phase != MtgoCompetitiveLifecyclePhaseV1::EnteredWaitingForPairing
        || source._lifecycle.phase() != MtgoCompetitiveLifecyclePhaseV1::EnteredWaitingForPairing
        || classification.event_kind != source._lifecycle.event_kind()
        || classification.frame_id != source._lifecycle.frame_id_v1()
        || classification.frame_sequence != source._lifecycle.frame_sequence()
        || classification.lifecycle_snapshot_commitment_sha256
            != source._lifecycle.snapshot_commitment_sha256()
    {
        return Err(
            "competitive entry transition after frame is not the exact entered-waiting lifecycle"
                .to_owned(),
        );
    }
    let frame = &source._source_frame.source_frame;
    let capture_commitment =
        capture_commitment_v3(&frame.manifest, &frame.canonical_bgra8, &frame.preview_png)?;
    if capture_commitment != frame.capture_commitment_sha256
        || capture_commitment
            != classification
                .source_frame
                .source_capture
                .capture_commitment_sha256
        || source._source_frame.commitments_v1() != classification.source_frame
    {
        return Err("competitive entry transition after capture changed".to_owned());
    }
    classified_transition_view_v1(frame, &classification, &source._lifecycle, None)
}

fn classified_transition_view_v1(
    frame: &OpaqueMtgoDxgiFrameCandidateV3,
    classification: &super::MtgoClassifiedCompetitiveNavigationFrameCommitmentsV1,
    lifecycle: &CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1,
    source_identity_commitment_sha256: Option<String>,
) -> Result<MtgoCompetitiveEntryFrameTransitionViewV1, String> {
    let event_identity_sha256 = lifecycle
        .event_identity_sha256_v1()
        .ok_or("competitive entry transition requires one visible event identity")?
        .to_owned();
    if !looks_like_lower_sha256_v1(&event_identity_sha256)
        || classification
            .source_frame
            .source_capture
            .canonical_bgra8_sha256
            != frame.manifest.frame.canonical_bgra8_sha256
        || classification.source_frame.source_capture.canonical_width
            != frame.manifest.frame.canonical_width
        || classification.source_frame.source_capture.canonical_height
            != frame.manifest.frame.canonical_height
        || classification.frame_id == 0
        || classification.frame_sequence == 0
    {
        return Err("competitive entry transition frame identity is incomplete".to_owned());
    }
    let output = canonical_json_v1(&frame.manifest.output, "entry transition output identity")?;
    let pre = &frame.manifest.pre;
    let window_continuity_commitment_sha256 = commitment_v1(
        COMPETITIVE_ENTRY_WINDOW_CONTINUITY_DOMAIN_V1,
        &[
            pre.hwnd.to_be_bytes().as_slice(),
            pre.process_id.to_be_bytes().as_slice(),
            pre.process_start_filetime_100ns.to_be_bytes().as_slice(),
            pre.process_image.as_bytes(),
            pre.executable_sha256.as_bytes(),
            pre.signer_thumbprint.as_bytes(),
            pre.signer_subject_sha256.as_bytes(),
            pre.title.as_bytes(),
            pre.dpi.to_be_bytes().as_slice(),
            &canonical_json_v1(
                &pre.client_rect_desktop_px,
                "entry transition client rectangle",
            )?,
            &canonical_json_v1(
                &pre.extended_frame_rect_desktop_px,
                "entry transition extended-frame rectangle",
            )?,
            &output,
            b"same_process_window_geometry_and_output_no_entry_no_spending_no_input",
        ],
    );
    Ok(MtgoCompetitiveEntryFrameTransitionViewV1 {
        navigation_profile_commitment_sha256: classification
            .source_frame
            .profile_commitment_sha256
            .clone(),
        navigation_profile_admission_commitment_sha256: classification
            .source_frame
            .profile_admission_commitment_sha256
            .clone(),
        approved_account_alias_sha256: classification
            .source_frame
            .approved_account_alias_sha256
            .clone(),
        runtime_identity_commitment_sha256: classification
            .runtime_identity_commitment_sha256
            .clone(),
        window_continuity_commitment_sha256,
        source_identity_commitment_sha256,
        capture_commitment_sha256: classification
            .source_frame
            .source_capture
            .capture_commitment_sha256
            .clone(),
        canonical_bgra8_sha256: classification
            .source_frame
            .source_capture
            .canonical_bgra8_sha256
            .clone(),
        classification_result_commitment_sha256: classification
            .classification_result_commitment_sha256
            .clone(),
        lifecycle_snapshot_commitment_sha256: classification
            .lifecycle_snapshot_commitment_sha256
            .clone(),
        event_kind: classification.event_kind,
        phase: classification.phase,
        event_identity_sha256,
        frame_id: classification.frame_id,
        frame_sequence: classification.frame_sequence,
        captured_at_unix_millis: classification
            .source_frame
            .source_capture
            .captured_at_unix_millis,
    })
}

fn bind_competitive_entry_frame_transition_views_v1(
    before: &MtgoCompetitiveEntryFrameTransitionViewV1,
    after: &MtgoCompetitiveEntryFrameTransitionViewV1,
) -> Result<MtgoCompetitiveEntryFrameTransitionCommitmentsV1, String> {
    let source_identity_commitment_sha256 = before
        .source_identity_commitment_sha256
        .as_deref()
        .ok_or("competitive entry transition requires the exact before-frame source identity")?;
    if before.phase != MtgoCompetitiveLifecyclePhaseV1::EntryReview
        || after.phase != MtgoCompetitiveLifecyclePhaseV1::EnteredWaitingForPairing
        || before.event_kind != after.event_kind
        || before.event_identity_sha256 != after.event_identity_sha256
        || before.navigation_profile_commitment_sha256 != after.navigation_profile_commitment_sha256
        || before.navigation_profile_admission_commitment_sha256
            != after.navigation_profile_admission_commitment_sha256
        || before.approved_account_alias_sha256 != after.approved_account_alias_sha256
        || before.runtime_identity_commitment_sha256 != after.runtime_identity_commitment_sha256
        || before.window_continuity_commitment_sha256 != after.window_continuity_commitment_sha256
    {
        return Err(
            "competitive entry transition changed mode, event, account, profile, runtime, process, window, geometry, or output"
                .to_owned(),
        );
    }
    if after.frame_id == before.frame_id
        || after.frame_sequence <= before.frame_sequence
        || after.captured_at_unix_millis <= before.captured_at_unix_millis
        || after.capture_commitment_sha256 == before.capture_commitment_sha256
        || after.canonical_bgra8_sha256 == before.canonical_bgra8_sha256
        || after.classification_result_commitment_sha256
            == before.classification_result_commitment_sha256
        || after.lifecycle_snapshot_commitment_sha256 == before.lifecycle_snapshot_commitment_sha256
    {
        return Err(
            "competitive entry transition requires a changed strictly newer classified frame"
                .to_owned(),
        );
    }
    for digest in [
        source_identity_commitment_sha256,
        before.navigation_profile_commitment_sha256.as_str(),
        before
            .navigation_profile_admission_commitment_sha256
            .as_str(),
        before.approved_account_alias_sha256.as_str(),
        before.runtime_identity_commitment_sha256.as_str(),
        before.window_continuity_commitment_sha256.as_str(),
        before.capture_commitment_sha256.as_str(),
        before.classification_result_commitment_sha256.as_str(),
        before.lifecycle_snapshot_commitment_sha256.as_str(),
        after.capture_commitment_sha256.as_str(),
        after.classification_result_commitment_sha256.as_str(),
        after.lifecycle_snapshot_commitment_sha256.as_str(),
        before.event_identity_sha256.as_str(),
    ] {
        if !looks_like_lower_sha256_v1(digest) {
            return Err("competitive entry transition contains an invalid commitment".to_owned());
        }
    }
    let event_kind = canonical_json_v1(&before.event_kind, "entry transition mode")?;
    let transition_commitment_sha256 = commitment_v1(
        COMPETITIVE_ENTRY_FRAME_TRANSITION_DOMAIN_V1,
        &[
            source_identity_commitment_sha256.as_bytes(),
            before.navigation_profile_commitment_sha256.as_bytes(),
            before
                .navigation_profile_admission_commitment_sha256
                .as_bytes(),
            before.approved_account_alias_sha256.as_bytes(),
            before.runtime_identity_commitment_sha256.as_bytes(),
            before.window_continuity_commitment_sha256.as_bytes(),
            before.capture_commitment_sha256.as_bytes(),
            before.classification_result_commitment_sha256.as_bytes(),
            before.lifecycle_snapshot_commitment_sha256.as_bytes(),
            after.capture_commitment_sha256.as_bytes(),
            after.classification_result_commitment_sha256.as_bytes(),
            after.lifecycle_snapshot_commitment_sha256.as_bytes(),
            &event_kind,
            before.event_identity_sha256.as_bytes(),
            before.frame_id.to_be_bytes().as_slice(),
            before.frame_sequence.to_be_bytes().as_slice(),
            after.frame_id.to_be_bytes().as_slice(),
            after.frame_sequence.to_be_bytes().as_slice(),
            before.captured_at_unix_millis.to_be_bytes().as_slice(),
            after.captured_at_unix_millis.to_be_bytes().as_slice(),
            b"entry_review_to_entered_waiting_visible_pair_no_causality_no_entry_no_spending_no_input",
        ],
    );
    Ok(MtgoCompetitiveEntryFrameTransitionCommitmentsV1 {
        navigation_profile_commitment_sha256: before.navigation_profile_commitment_sha256.clone(),
        navigation_profile_admission_commitment_sha256: before
            .navigation_profile_admission_commitment_sha256
            .clone(),
        approved_account_alias_sha256: before.approved_account_alias_sha256.clone(),
        runtime_identity_commitment_sha256: before.runtime_identity_commitment_sha256.clone(),
        window_continuity_commitment_sha256: before.window_continuity_commitment_sha256.clone(),
        source_identity_commitment_sha256: source_identity_commitment_sha256.to_owned(),
        before_capture_commitment_sha256: before.capture_commitment_sha256.clone(),
        before_classification_result_commitment_sha256: before
            .classification_result_commitment_sha256
            .clone(),
        before_lifecycle_snapshot_commitment_sha256: before
            .lifecycle_snapshot_commitment_sha256
            .clone(),
        after_capture_commitment_sha256: after.capture_commitment_sha256.clone(),
        after_classification_result_commitment_sha256: after
            .classification_result_commitment_sha256
            .clone(),
        after_lifecycle_snapshot_commitment_sha256: after
            .lifecycle_snapshot_commitment_sha256
            .clone(),
        event_kind: before.event_kind,
        event_identity_sha256: before.event_identity_sha256.clone(),
        before_frame_id: before.frame_id,
        before_frame_sequence: before.frame_sequence,
        after_frame_id: after.frame_id,
        after_frame_sequence: after.frame_sequence,
        transition_commitment_sha256,
    })
}

#[allow(clippy::too_many_arguments)]
fn bind_competitive_entry_control_dry_run_parts_v1(
    source: &MtgoOpaqueCompetitiveEntryReviewIdentityCommitmentsV1,
    lifecycle: &CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1,
    source_pixels: &[u8],
    source_size: &MtgoSizePxV1,
    event_label_rect_client_px: &MtgoRectPxV1,
    visible_control_label: &str,
    control_rect_client_px: &MtgoRectPxV1,
    visibly_enabled_confirmed: bool,
    selected_deck_label: &str,
    selected_deck_rect_client_px: &MtgoRectPxV1,
    deck_manifest_sha256: &str,
    deck_format_sha256: &str,
    policy_deployment_commitment_sha256: &str,
) -> Result<MtgoOpaqueCompetitiveEntryControlDryRunCommitmentsV1, String> {
    let classifier_commitment = source
        .source_navigation_classification_result_commitment_sha256
        .as_deref()
        .ok_or("competitive entry control dry run requires a classifier-backed source")?;
    if !looks_like_lower_sha256_v1(classifier_commitment)
        || lifecycle.phase() != MtgoCompetitiveLifecyclePhaseV1::EntryReview
        || lifecycle.snapshot_commitment_sha256()
            != source.source_lifecycle_snapshot_commitment_sha256
        || lifecycle.event_kind() != source.event_kind
        || lifecycle.frame_id_v1() != source.frame_id
        || lifecycle.frame_sequence() != source.frame_sequence
        || lifecycle.client_bounds_v1().width != source_size.width
        || lifecycle.client_bounds_v1().height != source_size.height
    {
        return Err(
            "competitive entry control dry run source lineage is incomplete or mismatched"
                .to_owned(),
        );
    }
    validate_entry_control_label_v1(visible_control_label)?;
    validate_selected_deck_label_v1(selected_deck_label)?;
    if !looks_like_lower_sha256_v1(deck_manifest_sha256)
        || !looks_like_lower_sha256_v1(deck_format_sha256)
        || !looks_like_lower_sha256_v1(policy_deployment_commitment_sha256)
        || deck_manifest_sha256 == deck_format_sha256
        || deck_manifest_sha256 == policy_deployment_commitment_sha256
        || deck_format_sha256 == policy_deployment_commitment_sha256
    {
        return Err(
            "competitive entry deck manifest, format, and policy deployment must be distinct lowercase SHA-256 commitments"
                .to_owned(),
        );
    }
    if !visibly_enabled_confirmed {
        return Err(
            "competitive entry control dry run requires an explicitly reviewed enabled control"
                .to_owned(),
        );
    }
    if control_rect_client_px.width < 8 || control_rect_client_px.height < 8 {
        return Err("competitive entry control region is too small for human review".to_owned());
    }
    if selected_deck_rect_client_px.width < 8 || selected_deck_rect_client_px.height < 8 {
        return Err("competitive selected-deck region is too small for human review".to_owned());
    }
    let review_surface = lifecycle
        .visible_facts_v1()
        .iter()
        .find(|fact| fact.kind == MtgoLifecycleVisibleFactKindV1::EntryReviewVisible)
        .map(|fact| &fact.rect_client_px)
        .ok_or("competitive entry control source is missing its visible review surface")?;
    if !rect_contains_rect_v1(review_surface, control_rect_client_px)? {
        return Err(
            "competitive entry control region is outside the visible entry-review surface"
                .to_owned(),
        );
    }
    if !rect_contains_rect_v1(review_surface, selected_deck_rect_client_px)? {
        return Err(
            "competitive selected-deck region is outside the visible entry-review surface"
                .to_owned(),
        );
    }
    if rects_intersect_v1(event_label_rect_client_px, control_rect_client_px)? {
        return Err("competitive entry control overlaps the reviewed event label".to_owned());
    }
    if rects_intersect_v1(event_label_rect_client_px, selected_deck_rect_client_px)?
        || rects_intersect_v1(control_rect_client_px, selected_deck_rect_client_px)?
    {
        return Err(
            "competitive selected-deck region overlaps the reviewed event label or entry control"
                .to_owned(),
        );
    }
    let visible_control_region_sha256 =
        visible_frame_region_content_sha256_v1(source_pixels, source_size, control_rect_client_px)
            .map_err(|error| format!("hash competitive entry control pixels: {error}"))?;
    let visible_control_label_sha256 = sha256_hex_v1(visible_control_label.as_bytes());
    let selected_deck_region_sha256 = visible_frame_region_content_sha256_v1(
        source_pixels,
        source_size,
        selected_deck_rect_client_px,
    )
    .map_err(|error| format!("hash competitive selected-deck pixels: {error}"))?;
    let selected_deck_label_sha256 = sha256_hex_v1(selected_deck_label.as_bytes());
    let control_rect_json = canonical_json_v1(control_rect_client_px, "entry control region")?;
    let selected_deck_rect_json =
        canonical_json_v1(selected_deck_rect_client_px, "selected deck region")?;
    let event_kind = canonical_json_v1(&source.event_kind, "entry control mode")?;
    let resource = canonical_json_v1(&source.resource, "entry control resource")?;
    let dry_run_commitment_sha256 = commitment_v1(
        OPAQUE_COMPETITIVE_ENTRY_CONTROL_AND_DECK_DRY_RUN_DOMAIN_V2,
        &[
            source.source_identity_commitment_sha256.as_bytes(),
            source.source_capture_commitment_sha256.as_bytes(),
            source
                .source_lifecycle_snapshot_commitment_sha256
                .as_bytes(),
            classifier_commitment.as_bytes(),
            visible_control_label.as_bytes(),
            visible_control_label_sha256.as_bytes(),
            &control_rect_json,
            visible_control_region_sha256.as_bytes(),
            &[u8::from(visibly_enabled_confirmed)],
            selected_deck_label.as_bytes(),
            selected_deck_label_sha256.as_bytes(),
            &selected_deck_rect_json,
            selected_deck_region_sha256.as_bytes(),
            deck_manifest_sha256.as_bytes(),
            deck_format_sha256.as_bytes(),
            policy_deployment_commitment_sha256.as_bytes(),
            &event_kind,
            source.frame_id.to_be_bytes().as_slice(),
            source.frame_sequence.to_be_bytes().as_slice(),
            &resource,
            source.amount.to_be_bytes().as_slice(),
            b"human_reviewed_confirm_entry_control_and_selected_deck_dry_run_no_join_no_spending_no_input",
        ],
    );
    Ok(MtgoOpaqueCompetitiveEntryControlDryRunCommitmentsV1 {
        source_identity_commitment_sha256: source.source_identity_commitment_sha256.clone(),
        source_capture_commitment_sha256: source.source_capture_commitment_sha256.clone(),
        source_lifecycle_snapshot_commitment_sha256: source
            .source_lifecycle_snapshot_commitment_sha256
            .clone(),
        source_navigation_classification_result_commitment_sha256: classifier_commitment.to_owned(),
        visible_control_label_sha256,
        visible_control_region_sha256,
        selected_deck_label_sha256,
        selected_deck_region_sha256,
        deck_manifest_sha256: deck_manifest_sha256.to_owned(),
        deck_format_sha256: deck_format_sha256.to_owned(),
        policy_deployment_commitment_sha256: policy_deployment_commitment_sha256.to_owned(),
        visibly_enabled_confirmed,
        dry_run_commitment_sha256,
        event_kind: source.event_kind,
        frame_id: source.frame_id,
        frame_sequence: source.frame_sequence,
        resource: source.resource,
        amount: source.amount,
    })
}

#[allow(clippy::too_many_arguments)]
fn bind_competitive_entry_review_source_parts_v1(
    source_capture_commitment_sha256: &str,
    source_frame_sha256: &str,
    source_width: u32,
    source_height: u32,
    source_pixels: &[u8],
    source_window_title: &str,
    source_window_mode: &str,
    source_capture_role: &str,
    lifecycle: &CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1,
    event_display_label: &str,
    event_label_rect_client_px: &MtgoRectPxV1,
    navigation_classification_result_commitment_sha256: Option<&str>,
) -> Result<MtgoOpaqueCompetitiveEntryReviewIdentityCommitmentsV1, String> {
    if source_window_mode != "main_client" || source_capture_role != "navigation" {
        return Err("competitive entry identity requires the navigation capture role".to_owned());
    }
    if lifecycle.phase() != MtgoCompetitiveLifecyclePhaseV1::EntryReview {
        return Err("competitive entry identity requires entry-review pixels".to_owned());
    }
    let client_bounds = lifecycle.client_bounds_v1();
    if lifecycle.frame_sha256_v1() != source_frame_sha256
        || client_bounds.x != 0
        || client_bounds.y != 0
        || client_bounds.width != source_width
        || client_bounds.height != source_height
    {
        return Err(
            "competitive entry lifecycle does not describe the exact opaque navigation frame"
                .to_owned(),
        );
    }
    let source_size = MtgoSizePxV1 {
        width: source_width,
        height: source_height,
    };
    for fact in lifecycle.visible_facts_v1() {
        let actual = visible_frame_region_content_sha256_v1(
            source_pixels,
            &source_size,
            &fact.rect_client_px,
        )
        .map_err(|error| format!("rehash competitive entry lifecycle fact pixels: {error}"))?;
        if actual != fact.content_sha256 {
            return Err(
                "competitive entry lifecycle fact does not match the retained source pixels"
                    .to_owned(),
            );
        }
    }
    validate_entry_display_label_v1(event_display_label, lifecycle.event_kind())?;
    if event_label_rect_client_px.width < 8 || event_label_rect_client_px.height < 8 {
        return Err("competitive entry label region is too small for human review".to_owned());
    }
    let review_surface = lifecycle
        .visible_facts_v1()
        .iter()
        .find(|fact| fact.kind == MtgoLifecycleVisibleFactKindV1::EntryReviewVisible)
        .map(|fact| &fact.rect_client_px)
        .ok_or("competitive entry lifecycle is missing its visible review surface")?;
    if !rect_contains_rect_v1(review_surface, event_label_rect_client_px)? {
        return Err(
            "competitive entry label region is outside the visible entry-review surface".to_owned(),
        );
    }
    let event_label_region_sha256 = visible_frame_region_content_sha256_v1(
        source_pixels,
        &source_size,
        event_label_rect_client_px,
    )
    .map_err(|error| format!("hash competitive entry label pixels: {error}"))?;
    let event_identity_sha256 = lifecycle
        .event_identity_sha256_v1()
        .ok_or("competitive entry lifecycle is missing its event identity")?;
    let entry_terms = lifecycle
        .entry_terms_v1()
        .ok_or("competitive entry lifecycle is missing its exact visible terms")?;
    let event_kind_json = canonical_json_v1(&lifecycle.event_kind(), "entry mode")?;
    let entry_terms_json = canonical_json_v1(entry_terms, "entry terms")?;
    let event_label_rect_json =
        canonical_json_v1(event_label_rect_client_px, "entry label region")?;
    let source_window_title_sha256 = sha256_hex_v1(source_window_title.as_bytes());
    if navigation_classification_result_commitment_sha256
        .is_some_and(|value| !looks_like_lower_sha256_v1(value))
    {
        return Err("navigation classification result commitment is invalid".to_owned());
    }
    let manual_source_identity_commitment_sha256 = commitment_v1(
        OPAQUE_COMPETITIVE_ENTRY_REVIEW_IDENTITY_DOMAIN_V1,
        &[
            source_capture_commitment_sha256.as_bytes(),
            source_frame_sha256.as_bytes(),
            lifecycle.snapshot_commitment_sha256().as_bytes(),
            source_window_title_sha256.as_bytes(),
            event_display_label.as_bytes(),
            &event_label_rect_json,
            event_label_region_sha256.as_bytes(),
            &event_kind_json,
            event_identity_sha256.as_bytes(),
            &entry_terms_json,
            lifecycle.frame_id_v1().to_be_bytes().as_slice(),
            lifecycle.frame_sequence().to_be_bytes().as_slice(),
            b"opaque_composed_navigation_pixels_owner_review_only_no_entry_no_spending_no_input",
        ],
    );
    let source_identity_commitment_sha256 = if let Some(classification_commitment) =
        navigation_classification_result_commitment_sha256
    {
        commitment_v1(
            OPAQUE_COMPETITIVE_CLASSIFIER_BOUND_ENTRY_REVIEW_IDENTITY_DOMAIN_V2,
            &[
                manual_source_identity_commitment_sha256.as_bytes(),
                classification_commitment.as_bytes(),
                b"exact_navigation_classifier_lineage_no_entry_no_spending_no_input",
            ],
        )
    } else {
        manual_source_identity_commitment_sha256
    };
    Ok(MtgoOpaqueCompetitiveEntryReviewIdentityCommitmentsV1 {
        source_capture_commitment_sha256: source_capture_commitment_sha256.to_owned(),
        source_lifecycle_snapshot_commitment_sha256: lifecycle
            .snapshot_commitment_sha256()
            .to_owned(),
        source_window_title_sha256,
        event_label_region_sha256,
        source_identity_commitment_sha256,
        source_navigation_classification_result_commitment_sha256:
            navigation_classification_result_commitment_sha256.map(str::to_owned),
        event_kind: lifecycle.event_kind(),
        frame_id: lifecycle.frame_id_v1(),
        frame_sequence: lifecycle.frame_sequence(),
        resource: entry_terms.resource,
        amount: entry_terms.amount,
    })
}

fn validate_entry_display_label_v1(
    value: &str,
    event_kind: MtgoCompetitiveEventKindV1,
) -> Result<(), String> {
    if value.is_empty()
        || value.len() > 160
        || value.trim() != value
        || !value.bytes().all(|byte| (0x20..=0x7e).contains(&byte))
    {
        return Err(
            "competitive entry label must be nonempty, trimmed, bounded ASCII display text"
                .to_owned(),
        );
    }
    let mode_word = match event_kind {
        MtgoCompetitiveEventKindV1::League => "league",
        MtgoCompetitiveEventKindV1::Challenge => "challenge",
    };
    if !value.to_ascii_lowercase().contains(mode_word) {
        return Err("competitive entry label does not identify the selected mode".to_owned());
    }
    Ok(())
}

fn validate_entry_control_label_v1(value: &str) -> Result<(), String> {
    if value.is_empty()
        || value.len() > 80
        || value.trim() != value
        || !value.bytes().all(|byte| (0x20..=0x7e).contains(&byte))
    {
        return Err(
            "competitive entry control label must be nonempty, trimmed, bounded ASCII display text"
                .to_owned(),
        );
    }
    let normalized = value.to_ascii_lowercase();
    if !normalized.contains("join") && !normalized.contains("enter") {
        return Err(
            "competitive entry control label does not visibly identify Join or Enter".to_owned(),
        );
    }
    Ok(())
}

fn validate_selected_deck_label_v1(value: &str) -> Result<(), String> {
    if value.is_empty()
        || value.len() > 96
        || value.trim() != value
        || !value.bytes().all(|byte| (0x20..=0x7e).contains(&byte))
    {
        return Err(
            "competitive selected-deck label must be nonempty, trimmed, bounded ASCII display text"
                .to_owned(),
        );
    }
    Ok(())
}

fn rect_contains_rect_v1(outer: &MtgoRectPxV1, inner: &MtgoRectPxV1) -> Result<bool, String> {
    if inner.width == 0 || inner.height == 0 {
        return Ok(false);
    }
    let outer_right = outer
        .x
        .checked_add(outer.width)
        .ok_or("competitive entry outer rectangle overflow")?;
    let outer_bottom = outer
        .y
        .checked_add(outer.height)
        .ok_or("competitive entry outer rectangle overflow")?;
    let inner_right = inner
        .x
        .checked_add(inner.width)
        .ok_or("competitive entry inner rectangle overflow")?;
    let inner_bottom = inner
        .y
        .checked_add(inner.height)
        .ok_or("competitive entry inner rectangle overflow")?;
    Ok(inner.x >= outer.x
        && inner.y >= outer.y
        && inner_right <= outer_right
        && inner_bottom <= outer_bottom)
}

fn rects_intersect_v1(left: &MtgoRectPxV1, right: &MtgoRectPxV1) -> Result<bool, String> {
    let left_right = left
        .x
        .checked_add(left.width)
        .ok_or("competitive entry left rectangle overflow")?;
    let left_bottom = left
        .y
        .checked_add(left.height)
        .ok_or("competitive entry left rectangle overflow")?;
    let right_right = right
        .x
        .checked_add(right.width)
        .ok_or("competitive entry right rectangle overflow")?;
    let right_bottom = right
        .y
        .checked_add(right.height)
        .ok_or("competitive entry right rectangle overflow")?;
    Ok(left.x < right_right
        && right.x < left_right
        && left.y < right_bottom
        && right.y < left_bottom)
}

fn canonical_json_v1<T: Serialize + ?Sized>(value: &T, field: &str) -> Result<Vec<u8>, String> {
    serde_json::to_vec(value).map_err(|error| format!("serialize competitive {field}: {error}"))
}

fn competitive_entry_frame_id_from_capture_commitment_v1(
    capture_commitment_sha256: &str,
    source_frame_id: u64,
) -> Result<u64, String> {
    if !looks_like_lower_sha256_v1(capture_commitment_sha256) {
        return Err("competitive entry capture commitment is not lowercase SHA-256".to_owned());
    }
    let mut frame_id = u64::from_str_radix(&capture_commitment_sha256[..16], 16)
        .map_err(|error| format!("derive competitive entry frame id: {error}"))?;
    if frame_id == 0 || frame_id == source_frame_id {
        frame_id ^= 0xa5a5_5a5a_d3d3_3c3c;
    }
    if frame_id == 0 || frame_id == source_frame_id {
        return Err(
            "competitive entry capture cannot derive a distinct nonzero frame id".to_owned(),
        );
    }
    Ok(frame_id)
}

fn looks_like_lower_sha256_v1(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn commitment_v1(domain: &[u8], parts: &[&[u8]]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(domain);
    for part in parts {
        hasher.update((part.len() as u64).to_be_bytes());
        hasher.update(part);
    }
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use mtgo_blackbox_v1::{
        validate_visible_competitive_lifecycle_snapshot_v1, MtgoCompetitiveEntryTermsV1,
        MtgoLifecycleVisibleFactV1, MtgoVisibleCompetitiveLifecycleSnapshotV1,
        MTGO_COMPETITIVE_LIFECYCLE_SCHEMA_V1,
    };

    fn source_v1(
        pixels: &[u8],
        event_kind: MtgoCompetitiveEventKindV1,
        resource: MtgoCompetitiveEntryResourceV1,
        amount: u32,
    ) -> CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1 {
        let size = MtgoSizePxV1 {
            width: 100,
            height: 100,
        };
        let review_rect = MtgoRectPxV1 {
            x: 5,
            y: 5,
            width: 80,
            height: 40,
        };
        let terms_rect = MtgoRectPxV1 {
            x: 10,
            y: 50,
            width: 70,
            height: 30,
        };
        validate_visible_competitive_lifecycle_snapshot_v1(
            MtgoVisibleCompetitiveLifecycleSnapshotV1 {
                schema_version: MTGO_COMPETITIVE_LIFECYCLE_SCHEMA_V1,
                snapshot_id: "opaque-entry-review-source-test-v1".to_owned(),
                event_kind,
                phase: MtgoCompetitiveLifecyclePhaseV1::EntryReview,
                frame_id: 7,
                frame_sequence: 11,
                frame_sha256: sha256_hex_v1(pixels),
                client_bounds: MtgoRectPxV1 {
                    x: 0,
                    y: 0,
                    width: 100,
                    height: 100,
                },
                event_identity_sha256: Some("2".repeat(64)),
                match_identity_sha256: None,
                game_number: None,
                entry_terms: Some(MtgoCompetitiveEntryTermsV1 {
                    terms_sha256: "3".repeat(64),
                    resource,
                    amount,
                }),
                visible_state_complete: true,
                facts: vec![
                    MtgoLifecycleVisibleFactV1 {
                        kind: MtgoLifecycleVisibleFactKindV1::EntryReviewVisible,
                        rect_client_px: review_rect.clone(),
                        content_sha256: visible_frame_region_content_sha256_v1(
                            pixels,
                            &size,
                            &review_rect,
                        )
                        .unwrap(),
                        confidence_bps: 10_000,
                    },
                    MtgoLifecycleVisibleFactV1 {
                        kind: MtgoLifecycleVisibleFactKindV1::EntryTermsVisible,
                        rect_client_px: terms_rect.clone(),
                        content_sha256: visible_frame_region_content_sha256_v1(
                            pixels,
                            &size,
                            &terms_rect,
                        )
                        .unwrap(),
                        confidence_bps: 10_000,
                    },
                ],
            },
        )
        .unwrap()
    }

    #[test]
    fn exact_navigation_pixels_bind_both_entry_modes_without_authority() {
        let pixels = vec![17_u8; 100 * 100 * 4];
        for (event_kind, label) in [
            (MtgoCompetitiveEventKindV1::League, "Modern League"),
            (MtgoCompetitiveEventKindV1::Challenge, "Modern Challenge"),
        ] {
            let source = source_v1(
                &pixels,
                event_kind,
                MtgoCompetitiveEntryResourceV1::ExistingPlayPoints,
                100,
            );
            let commitments = bind_competitive_entry_review_source_parts_v1(
                &"1".repeat(64),
                &sha256_hex_v1(&pixels),
                100,
                100,
                &pixels,
                "Magic: The Gathering Online",
                "main_client",
                "navigation",
                &source,
                label,
                &MtgoRectPxV1 {
                    x: 10,
                    y: 10,
                    width: 50,
                    height: 20,
                },
                None,
            )
            .unwrap();
            assert_eq!(commitments.event_kind, event_kind);
            assert_eq!(
                commitments.resource,
                MtgoCompetitiveEntryResourceV1::ExistingPlayPoints
            );
            assert_eq!(commitments.amount, 100);
            assert_eq!(commitments.frame_id, 7);
            assert_eq!(commitments.frame_sequence, 11);
        }
    }

    #[test]
    fn role_frame_fact_label_and_terms_drift_fail_closed() {
        let mut pixels = vec![19_u8; 100 * 100 * 4];
        let source = source_v1(
            &pixels,
            MtgoCompetitiveEventKindV1::League,
            MtgoCompetitiveEntryResourceV1::ExistingEventTickets,
            10,
        );
        let bind = |pixels: &[u8], role: &str, label: &str, rect: MtgoRectPxV1| {
            bind_competitive_entry_review_source_parts_v1(
                &"1".repeat(64),
                source.frame_sha256_v1(),
                100,
                100,
                pixels,
                "Magic: The Gathering Online",
                "main_client",
                role,
                &source,
                label,
                &rect,
                None,
            )
        };
        let valid_rect = MtgoRectPxV1 {
            x: 10,
            y: 10,
            width: 50,
            height: 20,
        };
        assert!(bind(
            &pixels,
            "acting_player_duel",
            "Modern League",
            valid_rect.clone()
        )
        .is_err());
        assert!(bind(
            &pixels,
            "navigation",
            "Modern Challenge",
            valid_rect.clone()
        )
        .is_err());
        assert!(bind(
            &pixels,
            "navigation",
            "Modern League",
            MtgoRectPxV1 {
                x: 90,
                y: 90,
                width: 10,
                height: 10,
            },
        )
        .is_err());
        pixels[5 * 100 * 4 + 5 * 4] ^= 1;
        assert!(bind(&pixels, "navigation", "Modern League", valid_rect).is_err());
    }

    #[test]
    fn classifier_lineage_is_bound_into_entry_review_identity() {
        let pixels = vec![23_u8; 100 * 100 * 4];
        let source = source_v1(
            &pixels,
            MtgoCompetitiveEventKindV1::Challenge,
            MtgoCompetitiveEntryResourceV1::ExistingEventToken,
            1,
        );
        let rect = MtgoRectPxV1 {
            x: 10,
            y: 10,
            width: 50,
            height: 20,
        };
        let bind = |lineage: Option<&str>| {
            bind_competitive_entry_review_source_parts_v1(
                &"1".repeat(64),
                source.frame_sha256_v1(),
                100,
                100,
                &pixels,
                "Magic: The Gathering Online",
                "main_client",
                "navigation",
                &source,
                "Modern Challenge",
                &rect,
                lineage,
            )
        };
        let unclassified = bind(None).unwrap();
        let classified = bind(Some(&"7".repeat(64))).unwrap();
        assert_eq!(
            classified.source_navigation_classification_result_commitment_sha256,
            Some("7".repeat(64))
        );
        assert_ne!(
            classified.source_identity_commitment_sha256,
            unclassified.source_identity_commitment_sha256
        );
        assert!(bind(Some("not-a-digest")).is_err());
    }

    #[test]
    fn classifier_backed_confirm_entry_control_is_coordinate_private_and_exact() {
        let mut pixels = vec![29_u8; 100 * 100 * 4];
        for y in 35..43 {
            for x in 10..40 {
                pixels[(y * 100 * 4 + x * 4) as usize] = 211;
            }
        }
        for (event_kind, label) in [
            (MtgoCompetitiveEventKindV1::League, "Modern League"),
            (MtgoCompetitiveEventKindV1::Challenge, "Modern Challenge"),
        ] {
            let lifecycle = source_v1(
                &pixels,
                event_kind,
                MtgoCompetitiveEntryResourceV1::ExistingPlayPoints,
                100,
            );
            let event_rect = MtgoRectPxV1 {
                x: 10,
                y: 10,
                width: 50,
                height: 20,
            };
            let source = bind_competitive_entry_review_source_parts_v1(
                &"1".repeat(64),
                lifecycle.frame_sha256_v1(),
                100,
                100,
                &pixels,
                "Magic: The Gathering Online",
                "main_client",
                "navigation",
                &lifecycle,
                label,
                &event_rect,
                Some(&"7".repeat(64)),
            )
            .unwrap();
            let control_rect = MtgoRectPxV1 {
                x: 10,
                y: 35,
                width: 30,
                height: 8,
            };
            let selected_deck_rect = MtgoRectPxV1 {
                x: 50,
                y: 35,
                width: 30,
                height: 8,
            };
            let dry_run = bind_competitive_entry_control_dry_run_parts_v1(
                &source,
                &lifecycle,
                &pixels,
                &MtgoSizePxV1 {
                    width: 100,
                    height: 100,
                },
                &event_rect,
                "Join Event",
                &control_rect,
                true,
                "mtgkernel-modern-v1",
                &selected_deck_rect,
                &"1".repeat(64),
                &"2".repeat(64),
                &"3".repeat(64),
            )
            .unwrap();
            assert_eq!(dry_run.event_kind, event_kind);
            assert_eq!(dry_run.frame_id, 7);
            assert_eq!(dry_run.frame_sequence, 11);
            assert_eq!(dry_run.amount, 100);
            assert_eq!(dry_run.dry_run_commitment_sha256.len(), 64);
            assert_eq!(dry_run.visible_control_label_sha256.len(), 64);
            assert_eq!(dry_run.visible_control_region_sha256.len(), 64);
            assert_eq!(dry_run.selected_deck_label_sha256.len(), 64);
            assert_eq!(dry_run.selected_deck_region_sha256.len(), 64);

            let relabeled = bind_competitive_entry_control_dry_run_parts_v1(
                &source,
                &lifecycle,
                &pixels,
                &MtgoSizePxV1 {
                    width: 100,
                    height: 100,
                },
                &event_rect,
                "Enter Event",
                &control_rect,
                true,
                "mtgkernel-modern-v1",
                &selected_deck_rect,
                &"1".repeat(64),
                &"2".repeat(64),
                &"3".repeat(64),
            )
            .unwrap();
            assert_ne!(
                dry_run.dry_run_commitment_sha256,
                relabeled.dry_run_commitment_sha256
            );
        }
    }

    #[test]
    fn confirm_entry_control_dry_run_rejects_manual_overlap_and_geometry_drift() {
        let pixels = vec![31_u8; 100 * 100 * 4];
        let lifecycle = source_v1(
            &pixels,
            MtgoCompetitiveEventKindV1::League,
            MtgoCompetitiveEntryResourceV1::ExistingEventTickets,
            10,
        );
        let event_rect = MtgoRectPxV1 {
            x: 10,
            y: 10,
            width: 50,
            height: 20,
        };
        let bind_source = |lineage: Option<&str>| {
            bind_competitive_entry_review_source_parts_v1(
                &"1".repeat(64),
                lifecycle.frame_sha256_v1(),
                100,
                100,
                &pixels,
                "Magic: The Gathering Online",
                "main_client",
                "navigation",
                &lifecycle,
                "Modern League",
                &event_rect,
                lineage,
            )
            .unwrap()
        };
        let size = MtgoSizePxV1 {
            width: 100,
            height: 100,
        };
        let valid_control = MtgoRectPxV1 {
            x: 10,
            y: 35,
            width: 30,
            height: 8,
        };
        let valid_deck = MtgoRectPxV1 {
            x: 50,
            y: 35,
            width: 30,
            height: 8,
        };
        let bind = |source: &MtgoOpaqueCompetitiveEntryReviewIdentityCommitmentsV1,
                    label: &str,
                    rect: &MtgoRectPxV1,
                    visibly_enabled: bool| {
            bind_competitive_entry_control_dry_run_parts_v1(
                source,
                &lifecycle,
                &pixels,
                &size,
                &event_rect,
                label,
                rect,
                visibly_enabled,
                "mtgkernel-modern-v1",
                &valid_deck,
                &"1".repeat(64),
                &"2".repeat(64),
                &"3".repeat(64),
            )
        };

        assert!(bind(&bind_source(None), "Join Event", &valid_control, true).is_err());
        let classified = bind_source(Some(&"7".repeat(64)));
        assert!(bind(&classified, "Continue", &valid_control, true).is_err());
        assert!(bind(&classified, "Join Event", &valid_control, false).is_err());
        assert!(bind(&classified, "Join Event", &event_rect, true).is_err());
        assert!(bind(
            &classified,
            "Join Event",
            &MtgoRectPxV1 {
                x: 80,
                y: 80,
                width: 20,
                height: 20,
            },
            true,
        )
        .is_err());
        let mut wrong_frame = classified;
        wrong_frame.frame_sequence = 12;
        assert!(bind(&wrong_frame, "Join Event", &valid_control, true).is_err());
    }

    fn transition_view_v1(
        event_kind: MtgoCompetitiveEventKindV1,
        phase: MtgoCompetitiveLifecyclePhaseV1,
        frame_id: u64,
        frame_sequence: u64,
    ) -> MtgoCompetitiveEntryFrameTransitionViewV1 {
        let before = phase == MtgoCompetitiveLifecyclePhaseV1::EntryReview;
        MtgoCompetitiveEntryFrameTransitionViewV1 {
            navigation_profile_commitment_sha256: "1".repeat(64),
            navigation_profile_admission_commitment_sha256: "2".repeat(64),
            approved_account_alias_sha256: "3".repeat(64),
            runtime_identity_commitment_sha256: "4".repeat(64),
            window_continuity_commitment_sha256: "5".repeat(64),
            source_identity_commitment_sha256: before.then(|| "6".repeat(64)),
            capture_commitment_sha256: if before {
                "7".repeat(64)
            } else {
                "8".repeat(64)
            },
            canonical_bgra8_sha256: if before {
                "9".repeat(64)
            } else {
                "a".repeat(64)
            },
            classification_result_commitment_sha256: if before {
                "b".repeat(64)
            } else {
                "c".repeat(64)
            },
            lifecycle_snapshot_commitment_sha256: if before {
                "d".repeat(64)
            } else {
                "e".repeat(64)
            },
            event_kind,
            phase,
            event_identity_sha256: "f".repeat(64),
            frame_id,
            frame_sequence,
            captured_at_unix_millis: u128::from(frame_sequence) * 10,
        }
    }

    fn immediate_entry_review_view_v1(
        event_kind: MtgoCompetitiveEventKindV1,
    ) -> MtgoCompetitiveEntryFrameTransitionViewV1 {
        let mut immediate = transition_view_v1(
            event_kind,
            MtgoCompetitiveLifecyclePhaseV1::EntryReview,
            8,
            12,
        );
        immediate.capture_commitment_sha256 = "8".repeat(64);
        immediate.classification_result_commitment_sha256 = "c".repeat(64);
        immediate.lifecycle_snapshot_commitment_sha256 = "e".repeat(64);
        immediate
    }

    #[test]
    fn immediate_entry_recapture_requires_exact_new_same_event_review_frame() {
        for event_kind in [
            MtgoCompetitiveEventKindV1::League,
            MtgoCompetitiveEventKindV1::Challenge,
        ] {
            let source = transition_view_v1(
                event_kind,
                MtgoCompetitiveLifecyclePhaseV1::EntryReview,
                7,
                11,
            );
            let immediate = immediate_entry_review_view_v1(event_kind);
            validate_competitive_entry_immediate_recapture_views_v1(&source, &immediate).unwrap();
        }
    }

    #[test]
    fn immediate_entry_recapture_rejects_stale_or_cross_identity_frame() {
        let source = transition_view_v1(
            MtgoCompetitiveEventKindV1::League,
            MtgoCompetitiveLifecyclePhaseV1::EntryReview,
            7,
            11,
        );
        let immediate = immediate_entry_review_view_v1(MtgoCompetitiveEventKindV1::League);

        let mut wrong_phase = immediate.clone();
        wrong_phase.phase = MtgoCompetitiveLifecyclePhaseV1::EnteredWaitingForPairing;
        assert!(
            validate_competitive_entry_immediate_recapture_views_v1(&source, &wrong_phase).is_err()
        );

        let mut wrong_account = immediate.clone();
        wrong_account.approved_account_alias_sha256 = "0".repeat(64);
        assert!(
            validate_competitive_entry_immediate_recapture_views_v1(&source, &wrong_account)
                .is_err()
        );

        let mut wrong_event = immediate.clone();
        wrong_event.event_identity_sha256 = "0".repeat(64);
        assert!(
            validate_competitive_entry_immediate_recapture_views_v1(&source, &wrong_event).is_err()
        );

        let mut stale_sequence = immediate.clone();
        stale_sequence.frame_sequence = source.frame_sequence;
        assert!(
            validate_competitive_entry_immediate_recapture_views_v1(&source, &stale_sequence)
                .is_err()
        );

        let mut stale_time = immediate.clone();
        stale_time.captured_at_unix_millis = source.captured_at_unix_millis;
        assert!(
            validate_competitive_entry_immediate_recapture_views_v1(&source, &stale_time).is_err()
        );

        let mut unchanged_capture = immediate;
        unchanged_capture.capture_commitment_sha256 = source.capture_commitment_sha256.clone();
        assert!(validate_competitive_entry_immediate_recapture_views_v1(
            &source,
            &unchanged_capture
        )
        .is_err());
    }

    #[test]
    fn competitive_entry_frame_id_is_capture_bound_nonzero_and_distinct() {
        let source_frame_id = 0x1122_3344_5566_7788;
        let commitment = "1122334455667788".to_owned() + &"0".repeat(48);
        let derived =
            competitive_entry_frame_id_from_capture_commitment_v1(&commitment, source_frame_id)
                .unwrap();
        assert_ne!(derived, 0);
        assert_ne!(derived, source_frame_id);
        assert_eq!(derived, source_frame_id ^ 0xa5a5_5a5a_d3d3_3c3c);
        assert!(competitive_entry_frame_id_from_capture_commitment_v1(
            "not-a-commitment",
            source_frame_id
        )
        .is_err());
    }

    #[test]
    fn immediate_entry_recapture_requires_exact_terms_facts_label_and_control_pixels() {
        let mut pixels = vec![37_u8; 100 * 100 * 4];
        for y in 35..43 {
            for x in 10..40 {
                pixels[(y * 100 * 4 + x * 4) as usize] = 211;
            }
        }
        let lifecycle = source_v1(
            &pixels,
            MtgoCompetitiveEventKindV1::League,
            MtgoCompetitiveEntryResourceV1::ExistingPlayPoints,
            100,
        );
        let event_rect = MtgoRectPxV1 {
            x: 10,
            y: 10,
            width: 50,
            height: 20,
        };
        let control_rect = MtgoRectPxV1 {
            x: 10,
            y: 35,
            width: 30,
            height: 8,
        };
        let selected_deck_rect = MtgoRectPxV1 {
            x: 50,
            y: 35,
            width: 30,
            height: 8,
        };
        let source_identity = bind_competitive_entry_review_source_parts_v1(
            &"1".repeat(64),
            lifecycle.frame_sha256_v1(),
            100,
            100,
            &pixels,
            "Magic: The Gathering Online",
            "main_client",
            "navigation",
            &lifecycle,
            "Modern League",
            &event_rect,
            Some(&"7".repeat(64)),
        )
        .unwrap();
        let control = bind_competitive_entry_control_dry_run_parts_v1(
            &source_identity,
            &lifecycle,
            &pixels,
            &MtgoSizePxV1 {
                width: 100,
                height: 100,
            },
            &event_rect,
            "Join Event",
            &control_rect,
            true,
            "mtgkernel-modern-v1",
            &selected_deck_rect,
            &"1".repeat(64),
            &"2".repeat(64),
            &"3".repeat(64),
        )
        .unwrap();
        let mut source_view = transition_view_v1(
            MtgoCompetitiveEventKindV1::League,
            MtgoCompetitiveLifecyclePhaseV1::EntryReview,
            7,
            11,
        );
        source_view.capture_commitment_sha256 =
            source_identity.source_capture_commitment_sha256.clone();
        source_view.classification_result_commitment_sha256 = "7".repeat(64);
        source_view.lifecycle_snapshot_commitment_sha256 = source_identity
            .source_lifecycle_snapshot_commitment_sha256
            .clone();
        source_view.event_identity_sha256 =
            lifecycle.event_identity_sha256_v1().unwrap().to_owned();
        let terms = lifecycle.entry_terms_v1().unwrap();
        let event_region = source_identity.event_label_region_sha256.clone();
        let control_region = control.visible_control_region_sha256.clone();
        let selected_deck_region = control.selected_deck_region_sha256.clone();
        let check = |candidate_control: &MtgoOpaqueCompetitiveEntryControlDryRunCommitmentsV1,
                     immediate_terms: &MtgoCompetitiveEntryTermsV1,
                     immediate_facts: &[MtgoLifecycleVisibleFactV1],
                     immediate_event_region: &str,
                     immediate_control_region: &str,
                     immediate_selected_deck_region: &str| {
            validate_competitive_entry_immediate_visible_state_v1(
                &source_view,
                &source_identity,
                candidate_control,
                terms,
                immediate_terms,
                lifecycle.visible_facts_v1(),
                immediate_facts,
                &event_region,
                immediate_event_region,
                &control_region,
                immediate_control_region,
                &selected_deck_region,
                immediate_selected_deck_region,
            )
        };
        check(
            &control,
            terms,
            lifecycle.visible_facts_v1(),
            &event_region,
            &control_region,
            &selected_deck_region,
        )
        .unwrap();

        let mut changed_terms = terms.clone();
        changed_terms.amount += 1;
        assert!(check(
            &control,
            &changed_terms,
            lifecycle.visible_facts_v1(),
            &event_region,
            &control_region,
            &selected_deck_region,
        )
        .is_err());

        let mut changed_facts = lifecycle.visible_facts_v1().to_vec();
        changed_facts[0].content_sha256 = "0".repeat(64);
        assert!(check(
            &control,
            terms,
            &changed_facts,
            &event_region,
            &control_region,
            &selected_deck_region,
        )
        .is_err());

        assert!(check(
            &control,
            terms,
            lifecycle.visible_facts_v1(),
            &"0".repeat(64),
            &control_region,
            &selected_deck_region,
        )
        .is_err());

        assert!(check(
            &control,
            terms,
            lifecycle.visible_facts_v1(),
            &event_region,
            &control_region,
            &"0".repeat(64),
        )
        .is_err());

        let mut disabled = control;
        disabled.visibly_enabled_confirmed = false;
        assert!(check(
            &disabled,
            terms,
            lifecycle.visible_facts_v1(),
            &event_region,
            &control_region,
            &selected_deck_region,
        )
        .is_err());
    }

    #[test]
    fn entry_postcondition_pair_requires_exact_new_entered_waiting_state() {
        for event_kind in [
            MtgoCompetitiveEventKindV1::League,
            MtgoCompetitiveEventKindV1::Challenge,
        ] {
            let before = transition_view_v1(
                event_kind,
                MtgoCompetitiveLifecyclePhaseV1::EntryReview,
                7,
                11,
            );
            let after = transition_view_v1(
                event_kind,
                MtgoCompetitiveLifecyclePhaseV1::EnteredWaitingForPairing,
                8,
                12,
            );
            let transition =
                bind_competitive_entry_frame_transition_views_v1(&before, &after).unwrap();
            assert_eq!(transition.event_kind, event_kind);
            assert_eq!(transition.before_frame_sequence, 11);
            assert_eq!(transition.after_frame_sequence, 12);
            assert_eq!(transition.transition_commitment_sha256.len(), 64);
        }
    }

    #[test]
    fn entry_postcondition_pair_rejects_stale_or_cross_identity_frames() {
        let before = transition_view_v1(
            MtgoCompetitiveEventKindV1::League,
            MtgoCompetitiveLifecyclePhaseV1::EntryReview,
            7,
            11,
        );
        let valid_after = transition_view_v1(
            MtgoCompetitiveEventKindV1::League,
            MtgoCompetitiveLifecyclePhaseV1::EnteredWaitingForPairing,
            8,
            12,
        );

        let mut wrong_phase = valid_after.clone();
        wrong_phase.phase = MtgoCompetitiveLifecyclePhaseV1::EventBrowser;
        assert!(bind_competitive_entry_frame_transition_views_v1(&before, &wrong_phase).is_err());

        let mut stale = valid_after.clone();
        stale.frame_sequence = before.frame_sequence;
        assert!(bind_competitive_entry_frame_transition_views_v1(&before, &stale).is_err());

        let mut wrong_event = valid_after.clone();
        wrong_event.event_identity_sha256 = "0".repeat(64);
        assert!(bind_competitive_entry_frame_transition_views_v1(&before, &wrong_event).is_err());

        let mut wrong_profile = valid_after.clone();
        wrong_profile.navigation_profile_commitment_sha256 = "0".repeat(64);
        assert!(bind_competitive_entry_frame_transition_views_v1(&before, &wrong_profile).is_err());

        let mut wrong_window = valid_after.clone();
        wrong_window.window_continuity_commitment_sha256 = "0".repeat(64);
        assert!(bind_competitive_entry_frame_transition_views_v1(&before, &wrong_window).is_err());

        let mut unchanged_pixels = valid_after;
        unchanged_pixels.canonical_bgra8_sha256 = before.canonical_bgra8_sha256.clone();
        assert!(
            bind_competitive_entry_frame_transition_views_v1(&before, &unchanged_pixels).is_err()
        );
    }
}
