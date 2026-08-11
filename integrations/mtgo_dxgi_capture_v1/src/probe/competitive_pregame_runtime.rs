use super::{
    competitive_entry_window_continuity_commitment_for_frame_v1,
    frame_id_from_capture_commitment_v1, invoke_verified_competitive_pregame_process_v1,
    mtgo_process_continuity_commitment_for_frame_v1, sha256_hex_v1,
    verify_duel_perception_runtime_identity_now_v1, MtgoAdmittedDuelVisibleFrameCommitmentsV1,
    MtgoDuelPerceptionFrameIdentityV1, OpaqueMtgoAdmittedDuelVisibleFrameV1,
    OpaqueMtgoVerifiedDuelPerceptionRuntimeV1,
};
use mtgo_blackbox_v1::{
    check_untrusted_competitive_pregame_classifier_request_v1,
    check_untrusted_competitive_pregame_classifier_response_v1,
    AdmittedMtgoCompetitivePregameProfileV1, AdmittedMtgoDuelPerceptionProfileV1,
    CheckedUntrustedMtgoCompetitivePregameClassificationV1,
    MtgoCompetitivePregameClassifierRequestHeaderV1, MtgoCompetitivePregameClassifierResponseV1,
    MtgoCompetitivePregameStageLabelV1, MTGO_COMPETITIVE_PREGAME_CLASSIFIER_PROTOCOL_V1,
    MTGO_COMPETITIVE_PREGAME_CLASSIFIER_SCHEMA_V1,
};
use std::time::Duration;

/// Copyable commitments for one exact duel frame retained through the pinned
/// pregame classifier protocol. This telemetry is not a capture or input
/// capability.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoClassifiedCompetitivePregameFrameCommitmentsV1 {
    pub source_frame: MtgoAdmittedDuelVisibleFrameCommitmentsV1,
    pub classifier_runtime_identity_commitment_sha256: String,
    pub pregame_evaluation_commitment_sha256: String,
    pub pregame_profile_admission_commitment_sha256: String,
    pub request_commitment_sha256: String,
    pub classification_commitment_sha256: String,
    pub frame_id: u64,
    pub frame_sequence: u64,
    pub captured_at_unix_millis: u128,
    pub stage: MtgoCompetitivePregameStageLabelV1,
}

/// One in-process DXGI frame retained through the exact profile-pinned
/// classifier and pixel-backed pregame protocol. It is move-only and exposes
/// no pixels, rectangles, path, process handle, event entry, or input method.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoClassifiedCompetitivePregameFrameV1;
/// fn require_clone<T: Clone>() {}
/// require_clone::<OpaqueMtgoClassifiedCompetitivePregameFrameV1>();
/// ```
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoClassifiedCompetitivePregameFrameV1;
/// fn cannot_extract(value: &OpaqueMtgoClassifiedCompetitivePregameFrameV1) {
///     let _ = value.pixels();
///     let _ = value.control_rect();
/// }
/// ```
pub struct OpaqueMtgoClassifiedCompetitivePregameFrameV1 {
    pub(super) _source_frame: OpaqueMtgoAdmittedDuelVisibleFrameV1,
    pub(super) _checked_classification: CheckedUntrustedMtgoCompetitivePregameClassificationV1,
    pub(super) _response: MtgoCompetitivePregameClassifierResponseV1,
    commitments: MtgoClassifiedCompetitivePregameFrameCommitmentsV1,
}

impl OpaqueMtgoClassifiedCompetitivePregameFrameV1 {
    pub fn commitments_v1(&self) -> MtgoClassifiedCompetitivePregameFrameCommitmentsV1 {
        self.commitments.clone()
    }

    pub fn stage_v1(&self) -> MtgoCompetitivePregameStageLabelV1 {
        self.commitments.stage
    }

    pub fn safe_for_live_classification_v1(&self) -> bool {
        false
    }

    pub fn safe_for_input_v1(&self) -> bool {
        false
    }

    pub fn permits_event_entry_v1(&self) -> bool {
        false
    }

    pub(crate) fn process_continuity_commitment_sha256_v1(&self) -> String {
        mtgo_process_continuity_commitment_for_frame_v1(&self._source_frame.source_frame)
    }

    pub(crate) fn window_continuity_commitment_sha256_v1(&self) -> Result<String, String> {
        competitive_entry_window_continuity_commitment_for_frame_v1(
            &self._source_frame.source_frame,
        )
    }

    pub(crate) fn require_immediate_successor_v1(
        &self,
        prior_frame_id: u64,
        prior_frame_sequence: u64,
        prior_source_capture_commitment_sha256: &str,
        prior_captured_at_unix_millis: u128,
    ) -> Result<(), String> {
        validate_immediate_successor_identity_v1(
            ImmediateFrameIdentityViewV1 {
                source_capture_commitment_sha256: &self
                    .commitments
                    .source_frame
                    .source_capture
                    .capture_commitment_sha256,
                captured_at_unix_millis: self.commitments.captured_at_unix_millis,
                frame_id: self.commitments.frame_id,
                frame_sequence: self.commitments.frame_sequence,
            },
            ImmediateFrameIdentityViewV1 {
                source_capture_commitment_sha256: prior_source_capture_commitment_sha256,
                captured_at_unix_millis: prior_captured_at_unix_millis,
                frame_id: prior_frame_id,
                frame_sequence: prior_frame_sequence,
            },
        )
    }
}

#[derive(Clone, Copy)]
struct ImmediateFrameIdentityViewV1<'a> {
    source_capture_commitment_sha256: &'a str,
    captured_at_unix_millis: u128,
    frame_id: u64,
    frame_sequence: u64,
}

fn validate_immediate_successor_identity_v1(
    current: ImmediateFrameIdentityViewV1<'_>,
    prior: ImmediateFrameIdentityViewV1<'_>,
) -> Result<(), String> {
    let expected_frame_sequence = prior
        .frame_sequence
        .checked_add(1)
        .ok_or("competitive pregame frame sequence overflow")?;
    let expected_frame_id = frame_id_from_capture_commitment_v1(
        current.source_capture_commitment_sha256,
        prior.frame_id,
    )?;
    if current.source_capture_commitment_sha256 == prior.source_capture_commitment_sha256
        || current.captured_at_unix_millis <= prior.captured_at_unix_millis
        || current.frame_sequence != expected_frame_sequence
        || current.frame_id != expected_frame_id
    {
        return Err(
            "competitive pregame frame is not the immediate capture-bound successor".to_owned(),
        );
    }
    Ok(())
}

/// Invokes the exact duel-profile classifier in its competitive-pregame mode.
/// Production cannot reach this function while either profile's independent
/// ratification root is empty. The returned source remains non-actionable;
/// event identity and input require later exact-lineage bindings.
pub fn classify_admitted_mtgo_competitive_pregame_frame_v1(
    source_frame: OpaqueMtgoAdmittedDuelVisibleFrameV1,
    duel_profile: &AdmittedMtgoDuelPerceptionProfileV1,
    pregame_profile: &AdmittedMtgoCompetitivePregameProfileV1,
    runtime: &OpaqueMtgoVerifiedDuelPerceptionRuntimeV1,
    identity: MtgoDuelPerceptionFrameIdentityV1,
    timeout_ms: u32,
) -> Result<OpaqueMtgoClassifiedCompetitivePregameFrameV1, String> {
    if identity.frame_id == 0 || identity.frame_sequence == 0 {
        return Err("competitive pregame frame identity must be nonzero".to_owned());
    }
    if !(100..=60_000).contains(&timeout_ms) {
        return Err("competitive pregame timeout must be between 100 and 60000 ms".to_owned());
    }

    let source_commitments = source_frame.commitments_v1();
    let runtime_commitments = runtime.commitments_v1();
    if source_commitments.perception_profile_commitment_sha256
        != duel_profile.perception_profile_commitment_sha256()
        || source_commitments.perception_profile_admission_commitment_sha256
            != duel_profile.admission_commitment_sha256()
        || runtime_commitments.perception_profile_commitment_sha256
            != duel_profile.perception_profile_commitment_sha256()
        || runtime_commitments.perception_profile_admission_commitment_sha256
            != duel_profile.admission_commitment_sha256()
        || pregame_profile.duel_perception_profile_commitment_sha256()
            != duel_profile.perception_profile_commitment_sha256()
        || pregame_profile.duel_perception_profile_admission_commitment_sha256()
            != duel_profile.admission_commitment_sha256()
    {
        return Err(
            "competitive pregame source, runtime, duel profile, and pregame profile differ"
                .to_owned(),
        );
    }
    verify_duel_perception_runtime_identity_now_v1(runtime)?;

    let source = &source_frame.source_frame;
    let width = source.manifest.frame.canonical_width;
    let height = source.manifest.frame.canonical_height;
    let stride = width
        .checked_mul(4)
        .ok_or("competitive pregame canonical stride overflow")?;
    if width != duel_profile.client_size_px().width
        || height != duel_profile.client_size_px().height
        || source.manifest.frame.canonical_stride != stride
        || source.manifest.frame.canonical_byte_length != source.canonical_bgra8.len()
        || source.manifest.frame.canonical_bgra8_sha256 != sha256_hex_v1(&source.canonical_bgra8)
    {
        return Err(
            "competitive pregame source pixels differ from the admitted duel profile".to_owned(),
        );
    }

    let header = MtgoCompetitivePregameClassifierRequestHeaderV1 {
        schema_version: MTGO_COMPETITIVE_PREGAME_CLASSIFIER_SCHEMA_V1,
        protocol: MTGO_COMPETITIVE_PREGAME_CLASSIFIER_PROTOCOL_V1.to_owned(),
        frame_id: identity.frame_id,
        frame_sequence: identity.frame_sequence,
        canonical_width: width,
        canonical_height: height,
        canonical_stride: stride,
        canonical_byte_length: source.canonical_bgra8.len(),
        canonical_bgra8_sha256: source.manifest.frame.canonical_bgra8_sha256.clone(),
        source_capture_commitment_sha256: source.capture_commitment_sha256.clone(),
        source_frame_profile_binding_sha256: source_frame.frame_profile_binding_sha256.clone(),
        duel_perception_profile_commitment_sha256: duel_profile
            .perception_profile_commitment_sha256()
            .to_owned(),
        duel_perception_profile_admission_commitment_sha256: duel_profile
            .admission_commitment_sha256()
            .to_owned(),
        pregame_evaluation_commitment_sha256: pregame_profile
            .evaluation_commitment_sha256()
            .to_owned(),
        pregame_profile_admission_commitment_sha256: pregame_profile
            .admission_commitment_sha256()
            .to_owned(),
        classifier_runtime_identity_commitment_sha256: runtime_commitments
            .runtime_identity_commitment_sha256
            .clone(),
    };
    let header_json = serde_json::to_vec(&header)
        .map_err(|error| format!("serialize competitive pregame request: {error}"))?;
    let checked_request = check_untrusted_competitive_pregame_classifier_request_v1(
        duel_profile,
        pregame_profile,
        &header_json,
        &source.canonical_bgra8,
    )
    .map_err(|error| format!("check competitive pregame request: {error}"))?;
    let response = invoke_verified_competitive_pregame_process_v1(
        runtime,
        &header_json,
        &source.canonical_bgra8,
        Duration::from_millis(u64::from(timeout_ms)),
    )?;
    verify_duel_perception_runtime_identity_now_v1(runtime)?;
    let checked_classification = check_untrusted_competitive_pregame_classifier_response_v1(
        pregame_profile,
        &checked_request,
        &source.canonical_bgra8,
        &response,
    )
    .map_err(|error| format!("check competitive pregame response: {error}"))?;
    let response_record: MtgoCompetitivePregameClassifierResponseV1 =
        serde_json::from_slice(&response)
            .map_err(|error| format!("parse checked competitive pregame response: {error}"))?;

    if checked_classification.source_capture_commitment_sha256() != source.capture_commitment_sha256
        || checked_classification.source_frame_profile_binding_sha256()
            != source_frame.frame_profile_binding_sha256
        || checked_classification.classifier_runtime_identity_commitment_sha256()
            != runtime_commitments.runtime_identity_commitment_sha256
        || checked_classification.frame_id() != identity.frame_id
        || checked_classification.frame_sequence() != identity.frame_sequence
    {
        return Err("competitive pregame classification changed source lineage".to_owned());
    }
    let commitments = MtgoClassifiedCompetitivePregameFrameCommitmentsV1 {
        source_frame: source_commitments,
        classifier_runtime_identity_commitment_sha256: runtime_commitments
            .runtime_identity_commitment_sha256,
        pregame_evaluation_commitment_sha256: checked_classification
            .pregame_evaluation_commitment_sha256()
            .to_owned(),
        pregame_profile_admission_commitment_sha256: checked_classification
            .pregame_profile_admission_commitment_sha256()
            .to_owned(),
        request_commitment_sha256: checked_classification
            .request_commitment_sha256()
            .to_owned(),
        classification_commitment_sha256: checked_classification
            .classification_commitment_sha256()
            .to_owned(),
        frame_id: identity.frame_id,
        frame_sequence: identity.frame_sequence,
        captured_at_unix_millis: source.manifest.captured_at_unix_millis,
        stage: checked_classification.stage(),
    };
    Ok(OpaqueMtgoClassifiedCompetitivePregameFrameV1 {
        _source_frame: source_frame,
        _checked_classification: checked_classification,
        _response: response_record,
        commitments,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn immediate_successor_identity_is_capture_bound_and_strictly_newer() {
        let prior_capture = "1".repeat(64);
        let current_capture = "0123456789abcdef".to_owned() + &"0".repeat(48);
        let prior_frame_id = 77;
        let expected_frame_id =
            frame_id_from_capture_commitment_v1(&current_capture, prior_frame_id).unwrap();
        validate_immediate_successor_identity_v1(
            ImmediateFrameIdentityViewV1 {
                source_capture_commitment_sha256: &current_capture,
                captured_at_unix_millis: 1_001,
                frame_id: expected_frame_id,
                frame_sequence: 41,
            },
            ImmediateFrameIdentityViewV1 {
                source_capture_commitment_sha256: &prior_capture,
                captured_at_unix_millis: 1_000,
                frame_id: prior_frame_id,
                frame_sequence: 40,
            },
        )
        .unwrap();

        for (capture, captured_at, frame_id, frame_sequence) in [
            (prior_capture.as_str(), 1_001, expected_frame_id, 41),
            (current_capture.as_str(), 1_000, expected_frame_id, 41),
            (current_capture.as_str(), 1_001, expected_frame_id ^ 1, 41),
            (current_capture.as_str(), 1_001, expected_frame_id, 42),
        ] {
            assert!(validate_immediate_successor_identity_v1(
                ImmediateFrameIdentityViewV1 {
                    source_capture_commitment_sha256: capture,
                    captured_at_unix_millis: captured_at,
                    frame_id,
                    frame_sequence,
                },
                ImmediateFrameIdentityViewV1 {
                    source_capture_commitment_sha256: &prior_capture,
                    captured_at_unix_millis: 1_000,
                    frame_id: prior_frame_id,
                    frame_sequence: 40,
                },
            )
            .is_err());
        }
    }
}
