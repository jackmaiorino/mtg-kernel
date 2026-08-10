use super::{
    build_card_aware_pregame_action_plan_v4, canonical_json_commitment_v3, capture_commitment_v3,
    capture_mtgo_dxgi_frame_candidate_v3, measure_mtgo_dxgi_first_main_candidate_v3,
    measure_mtgo_dxgi_first_main_visible_hand_candidate_v1,
    measure_mtgo_dxgi_mulligan_ladder_candidate_v3,
    measure_mtgo_dxgi_mulligan_visible_hand_candidate_v3,
    score_and_select_card_aware_pregame_model_v4, sha256_hex_v1, CaptureManifestV2,
    CaptureWindowModeV2, MtgoDxgiCaptureRequestV3, MtgoDxgiFrameCommitmentsV3,
    MtgoExpectedModelDeploymentV1, MtgoExternalCardAwarePregameScorerV4,
    MtgoOfflineFirstMainClassificationV1, MtgoOfflineMulliganLadderClassificationV1,
    MtgoOfflineVisibleCardIdentityClassificationV1, MtgoOfflineVisibleCardIdentityV1,
    MtgoPlannedPregamePostconditionV3, MtgoPregameActionSemanticV1,
    OpaqueMtgoCardAwarePregameModelSelectionV4, OpaqueMtgoDxgiFirstMainMeasurementV3,
    OpaqueMtgoDxgiFirstMainVisibleHandMeasurementV1, OpaqueMtgoDxgiFrameCandidateV3,
    OpaqueMtgoDxgiMulliganMeasurementV3, OpaqueMtgoDxgiMulliganVisibleHandMeasurementV3,
    OpaqueMtgoPregameActionPlanV3, SignedRectV1,
};
use mtgo_blackbox_v1::CheckedUntrustedMtgoOfflineVisibleCardTemplateProfileV1;
use serde::Serialize;

const PINNED_SOLITAIRE_PROFILE_ID_V1: &str =
    "mtgo-freeform-solitaire-visible-capture-identity-layout-20260810-v1";
const PINNED_SOLITAIRE_PROFILE_DOMAIN_V1: &[u8] =
    b"mtgo-pinned-solitaire-visible-capture-profile-v1";
const PINNED_SOLITAIRE_PROFILE_COMMITMENT_V1: &str =
    "45f73bf432bbed42e1896c4f02e0670115bb891b69fdc7037c89dc780ac91fac";

const PINNED_EXECUTABLE_SHA256_V1: &str =
    "a672755dad7fe8cd08c7986216d0d0fb2c4dbafe669ad3d2aff2bfa2c21b9c69";
const PINNED_SIGNER_THUMBPRINT_V1: &str = "e9d9e2b989f90555b04c506fddf889c7aba7ac30";
const PINNED_SIGNER_SUBJECT_V1: &str =
    "CN=Daybreak Game Company LLC, O=Daybreak Game Company LLC, L=San Diego, S=California, C=US";
const PINNED_SIGNER_SUBJECT_SHA256_V1: &str =
    "89e095d976048cdd8da11e2ff312231867f79e521fa3b5aa6415d2aa59b79cfc";
const PINNED_VISIBLE_TITLE_V1: &str = "(Solitaire): Freeform: Vs. UnbuckledPie";
const PINNED_OUTPUT_DEVICE_V1: &str = r"\\.\DISPLAY2";

/// Copyable telemetry for one profile-bound visible frame. Possessing a copy
/// does not prove capture. Downstream trusted code must accept the opaque frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoPinnedSolitaireVisibleFrameCommitmentsV1 {
    pub profile_id: &'static str,
    pub profile_commitment_sha256: String,
    pub source_capture: MtgoDxgiFrameCommitmentsV3,
}

/// A real in-process DXGI capture bound to the exact currently inspected MTGO
/// identity and 1550 by 925 Freeform Solitaire layout.
///
/// This type proves only capture origin and identity/layout-profile matching.
/// It deliberately grants no card label, prompt label, semantic evidence,
/// observation, policy, or input authority. The transient-occluder limitation
/// of the underlying Desktop Duplication capture remains.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoPinnedSolitaireVisibleFrameV1;
/// let _forged = OpaqueMtgoPinnedSolitaireVisibleFrameV1 {};
/// ```
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoPinnedSolitaireVisibleFrameV1;
/// fn require_debug<T: std::fmt::Debug>() {}
/// require_debug::<OpaqueMtgoPinnedSolitaireVisibleFrameV1>();
/// ```
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoPinnedSolitaireVisibleFrameV1;
/// fn require_clone<T: Clone>() {}
/// require_clone::<OpaqueMtgoPinnedSolitaireVisibleFrameV1>();
/// ```
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoPinnedSolitaireVisibleFrameV1;
/// fn require_serialize<T: serde::Serialize>() {}
/// require_serialize::<OpaqueMtgoPinnedSolitaireVisibleFrameV1>();
/// ```
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoPinnedSolitaireVisibleFrameV1;
/// fn expose(frame: &OpaqueMtgoPinnedSolitaireVisibleFrameV1) {
///     let _ = frame.canonical_bgra8_v1();
/// }
/// ```
pub struct OpaqueMtgoPinnedSolitaireVisibleFrameV1 {
    source_frame: OpaqueMtgoDxgiFrameCandidateV3,
    profile_commitment_sha256: String,
}

impl OpaqueMtgoPinnedSolitaireVisibleFrameV1 {
    pub fn commitments_v1(&self) -> MtgoPinnedSolitaireVisibleFrameCommitmentsV1 {
        MtgoPinnedSolitaireVisibleFrameCommitmentsV1 {
            profile_id: PINNED_SOLITAIRE_PROFILE_ID_V1,
            profile_commitment_sha256: self.profile_commitment_sha256.clone(),
            source_capture: self.source_frame.commitments_v3(),
        }
    }

    pub fn matches_pinned_visible_capture_profile_v1(&self) -> bool {
        true
    }

    pub fn safe_for_semantic_evidence_v1(&self) -> bool {
        false
    }

    pub fn safe_for_observation_v5_v1(&self) -> bool {
        false
    }

    pub fn safe_for_policy_scoring_v1(&self) -> bool {
        false
    }

    pub fn safe_for_input_v1(&self) -> bool {
        false
    }

    /// Deliberately drops the profile-bound wrapper so existing perception can
    /// continue operating only at its checked-untrusted authority level.
    pub fn into_checked_untrusted_perception_candidate_v1(self) -> OpaqueMtgoDxgiFrameCandidateV3 {
        self.source_frame
    }
}

/// A Turn 1 first-main measurement that preserves possession of the exact
/// source-pinned Solitaire capture profile.
///
/// This is the first gameplay-state consumer of the pinned frame. The current
/// classifier recognizes only the reviewed empty-battlefield first-main
/// layout. The opaque value grants no semantic, observation, policy-scoring,
/// or input authority.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoPinnedSolitaireFirstMainMeasurementV1;
/// let _forged = OpaqueMtgoPinnedSolitaireFirstMainMeasurementV1 {};
/// ```
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoPinnedSolitaireFirstMainMeasurementV1;
/// fn require_debug<T: std::fmt::Debug>() {}
/// require_debug::<OpaqueMtgoPinnedSolitaireFirstMainMeasurementV1>();
/// ```
pub struct OpaqueMtgoPinnedSolitaireFirstMainMeasurementV1 {
    profile_commitment_sha256: String,
    measurement: OpaqueMtgoDxgiFirstMainMeasurementV3,
}

impl OpaqueMtgoPinnedSolitaireFirstMainMeasurementV1 {
    pub fn profile_commitment_sha256_v1(&self) -> &str {
        &self.profile_commitment_sha256
    }

    pub fn source_capture_commitments_v1(&self) -> MtgoDxgiFrameCommitmentsV3 {
        self.measurement.source_capture_commitments_v3()
    }

    pub fn classification_v1(&self) -> MtgoOfflineFirstMainClassificationV1 {
        self.measurement.classification_v3()
    }

    pub fn perception_profile_commitment_sha256_v1(&self) -> &str {
        self.measurement.profile_commitment_sha256_v3()
    }

    pub fn measurement_commitment_sha256_v1(&self) -> &str {
        self.measurement.measurement_commitment_sha256_v3()
    }

    pub fn safe_for_semantic_evidence_v1(&self) -> bool {
        false
    }

    pub fn safe_for_observation_v5_v1(&self) -> bool {
        false
    }

    pub fn safe_for_policy_scoring_v1(&self) -> bool {
        false
    }

    pub fn safe_for_input_v1(&self) -> bool {
        false
    }
}

pub fn measure_pinned_current_solitaire_first_main_v1(
    source: OpaqueMtgoPinnedSolitaireVisibleFrameV1,
) -> Result<OpaqueMtgoPinnedSolitaireFirstMainMeasurementV1, String> {
    let OpaqueMtgoPinnedSolitaireVisibleFrameV1 {
        source_frame,
        profile_commitment_sha256,
    } = source;
    require_current_pinned_profile_commitment_v1(&profile_commitment_sha256)?;
    let measurement = measure_mtgo_dxgi_first_main_candidate_v3(source_frame)?;
    Ok(OpaqueMtgoPinnedSolitaireFirstMainMeasurementV1 {
        profile_commitment_sha256,
        measurement,
    })
}

/// A complete checked-untrusted visible-hand measurement that retains both the
/// exact pinned capture profile and the caller-supplied template profile.
///
/// The eight labels are exposed only when the exact first-main state and every
/// fixed card-art region match. This opaque value grants no semantic evidence,
/// observation, model-scoring, or input authority.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoPinnedSolitaireFirstMainVisibleHandV1;
/// let _forged = OpaqueMtgoPinnedSolitaireFirstMainVisibleHandV1 {};
/// ```
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoPinnedSolitaireFirstMainVisibleHandV1;
/// fn require_clone<T: Clone>() {}
/// require_clone::<OpaqueMtgoPinnedSolitaireFirstMainVisibleHandV1>();
/// ```
pub struct OpaqueMtgoPinnedSolitaireFirstMainVisibleHandV1 {
    profile_commitment_sha256: String,
    measurement: OpaqueMtgoDxgiFirstMainVisibleHandMeasurementV1,
}

impl OpaqueMtgoPinnedSolitaireFirstMainVisibleHandV1 {
    pub fn profile_commitment_sha256_v1(&self) -> &str {
        &self.profile_commitment_sha256
    }

    pub fn source_capture_commitments_v1(&self) -> MtgoDxgiFrameCommitmentsV3 {
        self.measurement.source_capture_commitments_v1()
    }

    pub fn classification_v1(&self) -> MtgoOfflineVisibleCardIdentityClassificationV1 {
        self.measurement.classification_v1()
    }

    pub fn visible_hand_count_v1(&self) -> Option<u8> {
        self.measurement.visible_hand_count_v1()
    }

    pub fn matched_identity_count_v1(&self) -> u8 {
        self.measurement.matched_identity_count_v1()
    }

    pub fn identities_v1(&self) -> &[MtgoOfflineVisibleCardIdentityV1] {
        self.measurement.identities_v1()
    }

    pub fn visible_card_profile_commitment_sha256_v1(&self) -> &str {
        self.measurement.visible_card_profile_commitment_sha256_v1()
    }

    pub fn visible_identity_measurement_commitment_sha256_v1(&self) -> &str {
        self.measurement
            .visible_identity_measurement_commitment_sha256_v1()
    }

    pub fn safe_for_semantic_evidence_v1(&self) -> bool {
        false
    }

    pub fn safe_for_observation_v5_v1(&self) -> bool {
        false
    }

    pub fn safe_for_policy_scoring_v1(&self) -> bool {
        false
    }

    pub fn safe_for_input_v1(&self) -> bool {
        false
    }
}

pub fn measure_pinned_current_solitaire_first_main_visible_hand_v1(
    source: OpaqueMtgoPinnedSolitaireFirstMainMeasurementV1,
    profile: CheckedUntrustedMtgoOfflineVisibleCardTemplateProfileV1,
) -> Result<OpaqueMtgoPinnedSolitaireFirstMainVisibleHandV1, String> {
    let OpaqueMtgoPinnedSolitaireFirstMainMeasurementV1 {
        profile_commitment_sha256,
        measurement,
    } = source;
    require_current_pinned_profile_commitment_v1(&profile_commitment_sha256)?;
    let measurement = measure_mtgo_dxgi_first_main_visible_hand_candidate_v1(measurement, profile)?;
    Ok(OpaqueMtgoPinnedSolitaireFirstMainVisibleHandV1 {
        profile_commitment_sha256,
        measurement,
    })
}

/// A mulligan-prompt measurement that preserves possession of the exact
/// source-pinned Solitaire capture profile.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoPinnedSolitaireMulliganMeasurementV1;
/// let _forged = OpaqueMtgoPinnedSolitaireMulliganMeasurementV1 {};
/// ```
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoPinnedSolitaireMulliganMeasurementV1;
/// fn require_debug<T: std::fmt::Debug>() {}
/// require_debug::<OpaqueMtgoPinnedSolitaireMulliganMeasurementV1>();
/// ```
pub struct OpaqueMtgoPinnedSolitaireMulliganMeasurementV1 {
    profile_commitment_sha256: String,
    measurement: OpaqueMtgoDxgiMulliganMeasurementV3,
}

impl OpaqueMtgoPinnedSolitaireMulliganMeasurementV1 {
    pub fn profile_commitment_sha256_v1(&self) -> &str {
        &self.profile_commitment_sha256
    }

    pub fn source_capture_commitments_v1(&self) -> MtgoDxgiFrameCommitmentsV3 {
        self.measurement.source_capture_commitments_v3()
    }

    pub fn classification_v1(&self) -> MtgoOfflineMulliganLadderClassificationV1 {
        self.measurement.classification_v3()
    }

    pub fn prospective_keep_size_v1(&self) -> Option<u8> {
        self.measurement.prospective_keep_size_v3()
    }

    pub fn ordered_actions_v1(&self) -> &[MtgoPregameActionSemanticV1] {
        self.measurement.ordered_actions_v3()
    }

    pub fn measurement_commitment_sha256_v1(&self) -> &str {
        self.measurement.measurement_commitment_sha256_v3()
    }

    pub fn safe_for_semantic_evidence_v1(&self) -> bool {
        false
    }

    pub fn safe_for_observation_v5_v1(&self) -> bool {
        false
    }

    pub fn safe_for_policy_scoring_v1(&self) -> bool {
        false
    }

    pub fn safe_for_input_v1(&self) -> bool {
        false
    }
}

/// A complete visible-hand measurement that preserves the exact pinned
/// capture profile and retains pixels plus template bytes privately.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoPinnedSolitaireMulliganVisibleHandV1;
/// let _forged = OpaqueMtgoPinnedSolitaireMulliganVisibleHandV1 {};
/// ```
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoPinnedSolitaireMulliganVisibleHandV1;
/// fn require_clone<T: Clone>() {}
/// require_clone::<OpaqueMtgoPinnedSolitaireMulliganVisibleHandV1>();
/// ```
pub struct OpaqueMtgoPinnedSolitaireMulliganVisibleHandV1 {
    profile_commitment_sha256: String,
    measurement: OpaqueMtgoDxgiMulliganVisibleHandMeasurementV3,
}

impl OpaqueMtgoPinnedSolitaireMulliganVisibleHandV1 {
    pub fn profile_commitment_sha256_v1(&self) -> &str {
        &self.profile_commitment_sha256
    }

    pub fn classification_v1(&self) -> MtgoOfflineVisibleCardIdentityClassificationV1 {
        self.measurement.classification_v3()
    }

    pub fn prospective_keep_size_v1(&self) -> Option<u8> {
        self.measurement.prospective_keep_size_v3()
    }

    pub fn ordered_actions_v1(&self) -> &[MtgoPregameActionSemanticV1] {
        self.measurement.ordered_actions_v3()
    }

    pub fn identities_v1(&self) -> &[MtgoOfflineVisibleCardIdentityV1] {
        self.measurement.identities_v3()
    }

    pub fn visible_identity_measurement_commitment_sha256_v1(&self) -> &str {
        self.measurement
            .visible_identity_measurement_commitment_sha256_v3()
    }

    pub fn safe_for_semantic_evidence_v1(&self) -> bool {
        false
    }

    pub fn safe_for_observation_v5_v1(&self) -> bool {
        false
    }

    pub fn safe_for_policy_scoring_v1(&self) -> bool {
        false
    }

    pub fn safe_for_input_v1(&self) -> bool {
        false
    }
}

/// A card-aware scorer selection that remains bound to the exact pinned
/// capture profile. It cannot be downgraded to a generic selection.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoPinnedSolitairePregameSelectionV1;
/// let _forged = OpaqueMtgoPinnedSolitairePregameSelectionV1 {};
/// ```
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoPinnedSolitairePregameSelectionV1;
/// fn require_serialize<T: serde::Serialize>() {}
/// require_serialize::<OpaqueMtgoPinnedSolitairePregameSelectionV1>();
/// ```
pub struct OpaqueMtgoPinnedSolitairePregameSelectionV1 {
    profile_commitment_sha256: String,
    selection: OpaqueMtgoCardAwarePregameModelSelectionV4,
}

impl OpaqueMtgoPinnedSolitairePregameSelectionV1 {
    pub fn profile_commitment_sha256_v1(&self) -> &str {
        &self.profile_commitment_sha256
    }

    pub fn selected_index_v1(&self) -> usize {
        self.selection.selected_index_v4()
    }

    pub fn selected_semantic_v1(&self) -> &MtgoPregameActionSemanticV1 {
        self.selection.selected_semantic_v4()
    }

    pub fn selected_logit_f32_bits_v1(&self) -> u32 {
        self.selection.selected_logit_f32_bits_v4()
    }

    pub fn value_f32_bits_v1(&self) -> u32 {
        self.selection.value_f32_bits_v4()
    }

    pub fn selection_commitment_sha256_v1(&self) -> &str {
        self.selection.selection_commitment_sha256_v4()
    }

    pub fn safe_for_live_input_v1(&self) -> bool {
        false
    }
}

/// A coordinate-private pregame plan that retains the exact pinned capture
/// profile and has no conversion to the production actuator.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoPinnedSolitairePregameActionPlanV1;
/// let _forged = OpaqueMtgoPinnedSolitairePregameActionPlanV1 {};
/// ```
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoPinnedSolitairePregameActionPlanV1;
/// fn coordinate_escape(plan: &OpaqueMtgoPinnedSolitairePregameActionPlanV1) {
///     let _ = plan.target_point_client_px_v1();
/// }
/// ```
pub struct OpaqueMtgoPinnedSolitairePregameActionPlanV1 {
    profile_commitment_sha256: String,
    plan: OpaqueMtgoPregameActionPlanV3,
}

impl OpaqueMtgoPinnedSolitairePregameActionPlanV1 {
    pub fn profile_commitment_sha256_v1(&self) -> &str {
        &self.profile_commitment_sha256
    }

    pub fn selected_semantic_v1(&self) -> &MtgoPregameActionSemanticV1 {
        self.plan.selected_semantic_v3()
    }

    pub fn planned_postcondition_v1(&self) -> &MtgoPlannedPregamePostconditionV3 {
        self.plan.planned_postcondition_v3()
    }

    pub fn action_plan_commitment_sha256_v1(&self) -> &str {
        self.plan.action_plan_commitment_sha256_v3()
    }

    pub fn safe_for_live_input_v1(&self) -> bool {
        false
    }

    pub fn safe_for_purchase_v1(&self) -> bool {
        false
    }

    pub fn safe_for_queue_entry_v1(&self) -> bool {
        false
    }
}

pub fn measure_pinned_current_solitaire_mulligan_ladder_v1(
    source: OpaqueMtgoPinnedSolitaireVisibleFrameV1,
) -> Result<OpaqueMtgoPinnedSolitaireMulliganMeasurementV1, String> {
    let OpaqueMtgoPinnedSolitaireVisibleFrameV1 {
        source_frame,
        profile_commitment_sha256,
    } = source;
    require_current_pinned_profile_commitment_v1(&profile_commitment_sha256)?;
    let measurement = measure_mtgo_dxgi_mulligan_ladder_candidate_v3(source_frame)?;
    Ok(OpaqueMtgoPinnedSolitaireMulliganMeasurementV1 {
        profile_commitment_sha256,
        measurement,
    })
}

pub fn measure_pinned_current_solitaire_mulligan_visible_hand_v1(
    source: OpaqueMtgoPinnedSolitaireMulliganMeasurementV1,
    profile: CheckedUntrustedMtgoOfflineVisibleCardTemplateProfileV1,
) -> Result<OpaqueMtgoPinnedSolitaireMulliganVisibleHandV1, String> {
    let OpaqueMtgoPinnedSolitaireMulliganMeasurementV1 {
        profile_commitment_sha256,
        measurement,
    } = source;
    require_current_pinned_profile_commitment_v1(&profile_commitment_sha256)?;
    let measurement = measure_mtgo_dxgi_mulligan_visible_hand_candidate_v3(measurement, profile)?;
    Ok(OpaqueMtgoPinnedSolitaireMulliganVisibleHandV1 {
        profile_commitment_sha256,
        measurement,
    })
}

pub fn score_and_select_pinned_current_solitaire_pregame_v1<
    S: MtgoExternalCardAwarePregameScorerV4,
>(
    source: OpaqueMtgoPinnedSolitaireMulliganVisibleHandV1,
    deployment: &MtgoExpectedModelDeploymentV1,
    scorer: &mut S,
) -> Result<OpaqueMtgoPinnedSolitairePregameSelectionV1, String> {
    let OpaqueMtgoPinnedSolitaireMulliganVisibleHandV1 {
        profile_commitment_sha256,
        measurement,
    } = source;
    require_current_pinned_profile_commitment_v1(&profile_commitment_sha256)?;
    let selection = score_and_select_card_aware_pregame_model_v4(measurement, deployment, scorer)?;
    Ok(OpaqueMtgoPinnedSolitairePregameSelectionV1 {
        profile_commitment_sha256,
        selection,
    })
}

pub fn build_pinned_current_solitaire_pregame_action_plan_v1(
    source: OpaqueMtgoPinnedSolitairePregameSelectionV1,
) -> Result<OpaqueMtgoPinnedSolitairePregameActionPlanV1, String> {
    let OpaqueMtgoPinnedSolitairePregameSelectionV1 {
        profile_commitment_sha256,
        selection,
    } = source;
    require_current_pinned_profile_commitment_v1(&profile_commitment_sha256)?;
    let plan = build_card_aware_pregame_action_plan_v4(selection)?;
    Ok(OpaqueMtgoPinnedSolitairePregameActionPlanV1 {
        profile_commitment_sha256,
        plan,
    })
}

fn require_current_pinned_profile_commitment_v1(observed: &str) -> Result<(), String> {
    if observed != PINNED_SOLITAIRE_PROFILE_COMMITMENT_V1 {
        return Err("the opaque value lost the exact pinned Solitaire profile".to_owned());
    }
    Ok(())
}

/// Captures the visible foreground client using constants pinned in reviewed
/// source, then binds the opaque result to the exact identity/layout profile.
/// This function does not focus MTGO and never sends input.
pub fn capture_pinned_current_solitaire_visible_frame_v1(
    timeout_ms: u32,
) -> Result<OpaqueMtgoPinnedSolitaireVisibleFrameV1, String> {
    let source_frame = capture_mtgo_dxgi_frame_candidate_v3(pinned_capture_request_v1(timeout_ms))?;
    bind_pinned_current_solitaire_visible_frame_v1(source_frame)
}

fn pinned_capture_request_v1(timeout_ms: u32) -> MtgoDxgiCaptureRequestV3 {
    MtgoDxgiCaptureRequestV3 {
        expected_executable_sha256: PINNED_EXECUTABLE_SHA256_V1.to_owned(),
        expected_signer_thumbprint: PINNED_SIGNER_THUMBPRINT_V1.to_owned(),
        expected_signer_subject_sha256: PINNED_SIGNER_SUBJECT_SHA256_V1.to_owned(),
        window_mode: CaptureWindowModeV2::SolitaireGame,
        expected_game_format: Some("Freeform".to_owned()),
        expected_title_contains: Some(PINNED_VISIBLE_TITLE_V1.to_owned()),
        timeout_ms,
    }
}

fn bind_pinned_current_solitaire_visible_frame_v1(
    source_frame: OpaqueMtgoDxgiFrameCandidateV3,
) -> Result<OpaqueMtgoPinnedSolitaireVisibleFrameV1, String> {
    let observed = profile_facts_from_manifest_v1(&source_frame.manifest)?;
    let profile_commitment_sha256 = validate_pinned_profile_facts_v1(&observed)?;

    let expected_pixel_length = source_frame.manifest.frame.canonical_byte_length;
    if source_frame.canonical_bgra8.len() != expected_pixel_length
        || sha256_hex_v1(&source_frame.canonical_bgra8)
            != source_frame.manifest.frame.canonical_bgra8_sha256
        || sha256_hex_v1(&source_frame.preview_png)
            != source_frame.manifest.frame.preview_png_sha256
        || capture_commitment_v3(
            &source_frame.manifest,
            &source_frame.canonical_bgra8,
            &source_frame.preview_png,
        )? != source_frame.capture_commitment_sha256
    {
        return Err("the opaque DXGI frame bytes or commitment changed before binding".to_owned());
    }

    Ok(OpaqueMtgoPinnedSolitaireVisibleFrameV1 {
        source_frame,
        profile_commitment_sha256,
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct PinnedSolitaireProfileFactsV1 {
    profile_id: String,
    manifest_schema: String,
    artifact_kind: String,
    status: String,
    capture_backend: String,
    window_mode: String,
    capture_role: String,
    expected_game_format: String,
    title_rule_version: String,
    executable_sha256: String,
    signer_thumbprint: String,
    signer_subject: String,
    signer_subject_sha256: String,
    visible_title: String,
    dpi: u32,
    client_width: u32,
    client_height: u32,
    adapter_index: u32,
    output_index: u32,
    adapter_luid_low: u32,
    adapter_luid_high: i32,
    output_device_name: String,
    output_bounds_desktop_px: SignedRectV1,
    output_rotation: i32,
    output_color_space: i32,
    source_texture_width: u32,
    source_texture_height: u32,
    source_texture_format: i32,
    canonical_width: u32,
    canonical_height: u32,
    canonical_stride: u32,
    canonical_byte_length: u64,
    safe_for_semantic_evidence: bool,
    safe_for_ocr: bool,
    safe_for_policy_scoring: bool,
    safe_for_input: bool,
    authenticode_verified_in_probe: bool,
}

fn pinned_profile_facts_v1() -> PinnedSolitaireProfileFactsV1 {
    PinnedSolitaireProfileFactsV1 {
        profile_id: PINNED_SOLITAIRE_PROFILE_ID_V1.to_owned(),
        manifest_schema: "mtgo-dxgi-visible-frame-candidate/v2".to_owned(),
        artifact_kind: "mtgo_untrusted_dxgi_visible_frame_candidate_v2".to_owned(),
        status: "checked_untrusted_not_admitted".to_owned(),
        capture_backend: "dxgi_desktop_duplication_v1".to_owned(),
        window_mode: "solitaire_game".to_owned(),
        capture_role: "acting_player_solitaire".to_owned(),
        expected_game_format: "Freeform".to_owned(),
        title_rule_version: "mtgo_visible_title_rule_v2".to_owned(),
        executable_sha256: PINNED_EXECUTABLE_SHA256_V1.to_owned(),
        signer_thumbprint: PINNED_SIGNER_THUMBPRINT_V1.to_owned(),
        signer_subject: PINNED_SIGNER_SUBJECT_V1.to_owned(),
        signer_subject_sha256: PINNED_SIGNER_SUBJECT_SHA256_V1.to_owned(),
        visible_title: PINNED_VISIBLE_TITLE_V1.to_owned(),
        dpi: 120,
        client_width: 1_550,
        client_height: 925,
        adapter_index: 0,
        output_index: 0,
        adapter_luid_low: 59_989,
        adapter_luid_high: 0,
        output_device_name: PINNED_OUTPUT_DEVICE_V1.to_owned(),
        output_bounds_desktop_px: SignedRectV1 {
            left: 0,
            top: 0,
            right: 2_560,
            bottom: 1_440,
        },
        output_rotation: 1,
        output_color_space: 0,
        source_texture_width: 2_560,
        source_texture_height: 1_440,
        source_texture_format: 87,
        canonical_width: 1_550,
        canonical_height: 925,
        canonical_stride: 6_200,
        canonical_byte_length: 5_735_000,
        safe_for_semantic_evidence: false,
        safe_for_ocr: false,
        safe_for_policy_scoring: false,
        safe_for_input: false,
        authenticode_verified_in_probe: true,
    }
}

fn profile_facts_from_manifest_v1(
    manifest: &CaptureManifestV2,
) -> Result<PinnedSolitaireProfileFactsV1, String> {
    if manifest.pre != manifest.post {
        return Err("the DXGI frame pre/post window snapshots differ".to_owned());
    }
    if !manifest
        .output
        .bounds_desktop_px
        .contains(manifest.pre.client_rect_desktop_px)
    {
        return Err("the client crop is not contained in the captured output".to_owned());
    }
    let client_width = manifest
        .pre
        .client_rect_desktop_px
        .width()
        .map_err(str::to_owned)?;
    let client_height = manifest
        .pre
        .client_rect_desktop_px
        .height()
        .map_err(str::to_owned)?;
    let canonical_byte_length = u64::try_from(manifest.frame.canonical_byte_length)
        .map_err(|_| "canonical byte length does not fit u64")?;

    Ok(PinnedSolitaireProfileFactsV1 {
        profile_id: PINNED_SOLITAIRE_PROFILE_ID_V1.to_owned(),
        manifest_schema: manifest.schema.to_owned(),
        artifact_kind: manifest.artifact_kind.to_owned(),
        status: manifest.status.to_owned(),
        capture_backend: manifest.capture_backend.to_owned(),
        window_mode: manifest.window_mode.to_owned(),
        capture_role: manifest.capture_role.to_owned(),
        expected_game_format: manifest.expected_game_format.clone(),
        title_rule_version: manifest.title_rule_version.to_owned(),
        executable_sha256: manifest.pre.executable_sha256.clone(),
        signer_thumbprint: manifest.pre.signer_thumbprint.clone(),
        signer_subject: manifest.pre.signer_subject.clone(),
        signer_subject_sha256: manifest.pre.signer_subject_sha256.clone(),
        visible_title: manifest.pre.title.clone(),
        dpi: manifest.pre.dpi,
        client_width,
        client_height,
        adapter_index: manifest.output.adapter_index,
        output_index: manifest.output.output_index,
        adapter_luid_low: manifest.output.adapter_luid_low,
        adapter_luid_high: manifest.output.adapter_luid_high,
        output_device_name: manifest.output.device_name.clone(),
        output_bounds_desktop_px: manifest.output.bounds_desktop_px,
        output_rotation: manifest.output.rotation,
        output_color_space: manifest.output.color_space,
        source_texture_width: manifest.frame.source_texture_width,
        source_texture_height: manifest.frame.source_texture_height,
        source_texture_format: manifest.frame.source_texture_format,
        canonical_width: manifest.frame.canonical_width,
        canonical_height: manifest.frame.canonical_height,
        canonical_stride: manifest.frame.canonical_stride,
        canonical_byte_length,
        safe_for_semantic_evidence: manifest.safety.safe_for_semantic_evidence,
        safe_for_ocr: manifest.safety.safe_for_ocr,
        safe_for_policy_scoring: manifest.safety.safe_for_policy_scoring,
        safe_for_input: manifest.safety.safe_for_input,
        authenticode_verified_in_probe: manifest.safety.authenticode_verified_in_probe,
    })
}

fn pinned_profile_commitment_v1() -> Result<String, String> {
    canonical_json_commitment_v3(
        PINNED_SOLITAIRE_PROFILE_DOMAIN_V1,
        &pinned_profile_facts_v1(),
    )
}

fn validate_pinned_profile_facts_v1(
    observed: &PinnedSolitaireProfileFactsV1,
) -> Result<String, String> {
    if observed != &pinned_profile_facts_v1() {
        return Err(
            "the DXGI frame does not match the pinned current MTGO identity and layout profile"
                .to_owned(),
        );
    }
    let profile_commitment_sha256 = pinned_profile_commitment_v1()?;
    if profile_commitment_sha256 != PINNED_SOLITAIRE_PROFILE_COMMITMENT_V1 {
        return Err("the compiled MTGO identity/layout profile commitment drifted".to_owned());
    }
    Ok(profile_commitment_sha256)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pinned_profile_commitment_is_stable() {
        assert_eq!(
            pinned_profile_commitment_v1().unwrap(),
            PINNED_SOLITAIRE_PROFILE_COMMITMENT_V1
        );
    }

    #[test]
    fn representative_identity_layout_and_authority_drift_fails_closed() {
        let expected = pinned_profile_facts_v1();
        let mut drifts = Vec::new();

        let mut value = expected.clone();
        value.executable_sha256.replace_range(0..1, "0");
        drifts.push(value);
        let mut value = expected.clone();
        value.signer_thumbprint.replace_range(0..1, "0");
        drifts.push(value);
        let mut value = expected.clone();
        value.signer_subject.push('!');
        drifts.push(value);
        let mut value = expected.clone();
        value.visible_title.push_str(" Match #1 - Game #1");
        drifts.push(value);
        let mut value = expected.clone();
        value.window_mode = "spectator_game".to_owned();
        drifts.push(value);
        let mut value = expected.clone();
        value.expected_game_format = "Standard".to_owned();
        drifts.push(value);
        let mut value = expected.clone();
        value.dpi += 1;
        drifts.push(value);
        let mut value = expected.clone();
        value.client_width += 1;
        drifts.push(value);
        let mut value = expected.clone();
        value.output_device_name = r"\\.\DISPLAY1".to_owned();
        drifts.push(value);
        let mut value = expected.clone();
        value.output_bounds_desktop_px.right += 1;
        drifts.push(value);
        let mut value = expected.clone();
        value.adapter_luid_low += 1;
        drifts.push(value);
        let mut value = expected.clone();
        value.output_rotation += 1;
        drifts.push(value);
        let mut value = expected.clone();
        value.output_color_space += 1;
        drifts.push(value);
        let mut value = expected.clone();
        value.source_texture_format += 1;
        drifts.push(value);
        let mut value = expected.clone();
        value.canonical_stride += 4;
        drifts.push(value);
        let mut value = expected.clone();
        value.safe_for_input = true;
        drifts.push(value);

        assert!(drifts
            .iter()
            .all(|observed| validate_pinned_profile_facts_v1(observed).is_err()));
        assert_eq!(
            validate_pinned_profile_facts_v1(&expected).unwrap(),
            PINNED_SOLITAIRE_PROFILE_COMMITMENT_V1
        );
    }

    #[test]
    fn capture_request_is_source_pinned_and_role_exact() {
        let request = pinned_capture_request_v1(1_500);
        assert_eq!(
            request.expected_executable_sha256,
            PINNED_EXECUTABLE_SHA256_V1
        );
        assert_eq!(
            request.expected_signer_thumbprint,
            PINNED_SIGNER_THUMBPRINT_V1
        );
        assert_eq!(
            request.expected_signer_subject_sha256,
            PINNED_SIGNER_SUBJECT_SHA256_V1
        );
        assert_eq!(request.window_mode, CaptureWindowModeV2::SolitaireGame);
        assert_eq!(request.expected_game_format.as_deref(), Some("Freeform"));
        assert_eq!(
            request.expected_title_contains.as_deref(),
            Some(PINNED_VISIBLE_TITLE_V1)
        );
        assert_eq!(request.timeout_ms, 1_500);
    }

    #[test]
    fn authority_is_deliberately_absent_from_the_profile() {
        let profile = pinned_profile_facts_v1();
        assert!(!profile.safe_for_semantic_evidence);
        assert!(!profile.safe_for_ocr);
        assert!(!profile.safe_for_policy_scoring);
        assert!(!profile.safe_for_input);
        assert!(profile.authenticode_verified_in_probe);
    }

    #[test]
    #[ignore = "requires the pinned MTGO Solitaire window to be foreground and unobscured"]
    fn live_capture_returns_only_the_profile_bound_opaque_frame() {
        let frame = capture_pinned_current_solitaire_visible_frame_v1(2_000).unwrap();
        let commitments = frame.commitments_v1();
        assert_eq!(commitments.profile_id, PINNED_SOLITAIRE_PROFILE_ID_V1);
        assert_eq!(
            commitments.profile_commitment_sha256,
            PINNED_SOLITAIRE_PROFILE_COMMITMENT_V1
        );
        assert!(frame.matches_pinned_visible_capture_profile_v1());
        assert!(!frame.safe_for_semantic_evidence_v1());
        assert!(!frame.safe_for_observation_v5_v1());
        assert!(!frame.safe_for_policy_scoring_v1());
        assert!(!frame.safe_for_input_v1());
    }

    #[test]
    #[ignore = "requires the pinned MTGO Solitaire first-main window plus MTGO_VISIBLE_CARD_TEMPLATE_PROFILE_V1"]
    fn live_capture_reaches_profile_bound_first_main_measurement() {
        let frame = capture_pinned_current_solitaire_visible_frame_v1(2_000).unwrap();
        let measurement = measure_pinned_current_solitaire_first_main_v1(frame).unwrap();
        assert_eq!(
            measurement.profile_commitment_sha256_v1(),
            PINNED_SOLITAIRE_PROFILE_COMMITMENT_V1
        );
        assert_eq!(
            measurement.classification_v1(),
            MtgoOfflineFirstMainClassificationV1::Match
        );
        assert_eq!(measurement.measurement_commitment_sha256_v1().len(), 64);
        assert!(!measurement.safe_for_semantic_evidence_v1());
        assert!(!measurement.safe_for_observation_v5_v1());
        assert!(!measurement.safe_for_policy_scoring_v1());
        assert!(!measurement.safe_for_input_v1());

        let profile_path = std::env::var("MTGO_VISIBLE_CARD_TEMPLATE_PROFILE_V1").unwrap();
        let profile_bytes = std::fs::read(profile_path).unwrap();
        let profile: mtgo_blackbox_v1::MtgoOfflineVisibleCardTemplateProfileV1 =
            serde_json::from_slice(&profile_bytes).unwrap();
        let profile =
            mtgo_blackbox_v1::check_untrusted_offline_visible_card_template_profile_v1(profile)
                .unwrap();
        let visible_hand =
            measure_pinned_current_solitaire_first_main_visible_hand_v1(measurement, profile)
                .unwrap();
        assert_eq!(
            visible_hand.classification_v1(),
            MtgoOfflineVisibleCardIdentityClassificationV1::Match
        );
        assert_eq!(visible_hand.visible_hand_count_v1(), Some(8));
        assert_eq!(visible_hand.matched_identity_count_v1(), 8);
        assert_eq!(
            visible_hand
                .identities_v1()
                .iter()
                .map(MtgoOfflineVisibleCardIdentityV1::visible_card_name)
                .collect::<Vec<_>>(),
            ["Island", "Island", "Plains", "Island", "Plains", "Plains", "Island", "Island"]
        );
        assert!(!visible_hand.safe_for_semantic_evidence_v1());
        assert!(!visible_hand.safe_for_observation_v5_v1());
        assert!(!visible_hand.safe_for_policy_scoring_v1());
        assert!(!visible_hand.safe_for_input_v1());
    }
}
