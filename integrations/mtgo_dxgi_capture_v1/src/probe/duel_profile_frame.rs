use super::{
    capture_mtgo_dxgi_frame_candidate_v3, sha256_hex_v1, CaptureWindowModeV2,
    MtgoDxgiCaptureRequestV3, MtgoDxgiFrameCommitmentsV3, OpaqueMtgoDxgiFrameCandidateV3,
};
use mtgo_blackbox_v1::{
    preview_output_identity_commitment_v1, AdmittedMtgoDuelPerceptionProfileV1,
    MtgoDuelPerceptionProfileScopeV1, MtgoSignedRectDesktopPxV1,
};
use sha2::{Digest, Sha256};

const DUEL_VISIBLE_FRAME_PROFILE_BINDING_DOMAIN_V1: &[u8] =
    b"mtgo-admitted-duel-visible-frame-profile-binding-v1";

/// Copyable commitments for one in-process duel capture bound to one admitted
/// perception profile. Possessing this telemetry does not prove capture.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoAdmittedDuelVisibleFrameCommitmentsV1 {
    pub perception_profile_commitment_sha256: String,
    pub perception_profile_admission_commitment_sha256: String,
    pub frame_profile_binding_sha256: String,
    pub source_capture: MtgoDxgiFrameCommitmentsV3,
}

/// An in-process DXGI frame whose client identity, signer, geometry, monitor
/// output, and visible game format match one admitted duel perception profile.
///
/// The frame remains an unclassified pixel source. It has no public fields,
/// raw-pixel accessor, serde implementation, model-scoring conversion, action
/// conversion, coordinate accessor, or input conversion.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoAdmittedDuelVisibleFrameV1;
/// let _forged = OpaqueMtgoAdmittedDuelVisibleFrameV1 {};
/// ```
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoAdmittedDuelVisibleFrameV1;
/// fn require_debug<T: std::fmt::Debug>() {}
/// require_debug::<OpaqueMtgoAdmittedDuelVisibleFrameV1>();
/// ```
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoAdmittedDuelVisibleFrameV1;
/// fn require_clone<T: Clone>() {}
/// require_clone::<OpaqueMtgoAdmittedDuelVisibleFrameV1>();
/// ```
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoAdmittedDuelVisibleFrameV1;
/// fn require_serialize<T: serde::Serialize>() {}
/// require_serialize::<OpaqueMtgoAdmittedDuelVisibleFrameV1>();
/// ```
pub struct OpaqueMtgoAdmittedDuelVisibleFrameV1 {
    pub(super) source_frame: OpaqueMtgoDxgiFrameCandidateV3,
    pub(super) perception_profile_commitment_sha256: String,
    pub(super) perception_profile_admission_commitment_sha256: String,
    pub(super) frame_profile_binding_sha256: String,
}

impl OpaqueMtgoAdmittedDuelVisibleFrameV1 {
    pub fn commitments_v1(&self) -> MtgoAdmittedDuelVisibleFrameCommitmentsV1 {
        MtgoAdmittedDuelVisibleFrameCommitmentsV1 {
            perception_profile_commitment_sha256: self.perception_profile_commitment_sha256.clone(),
            perception_profile_admission_commitment_sha256: self
                .perception_profile_admission_commitment_sha256
                .clone(),
            frame_profile_binding_sha256: self.frame_profile_binding_sha256.clone(),
            source_capture: self.source_frame.commitments_v3(),
        }
    }

    pub fn matches_admitted_duel_perception_profile_v1(&self) -> bool {
        true
    }

    pub fn safe_for_semantic_evidence_v1(&self) -> bool {
        false
    }

    pub fn safe_for_model_scoring_v1(&self) -> bool {
        false
    }

    pub fn safe_for_input_v1(&self) -> bool {
        false
    }
}

/// Captures the foreground acting-player duel through Desktop Duplication and
/// binds the opaque result to the exact facts in an admitted perception
/// profile. The production profile ratification root is currently empty, so
/// callers cannot reach capture through this function until a measured corpus
/// has been separately reviewed and ratified.
pub fn capture_admitted_mtgo_duel_visible_frame_v1(
    profile: &AdmittedMtgoDuelPerceptionProfileV1,
    timeout_ms: u32,
) -> Result<OpaqueMtgoAdmittedDuelVisibleFrameV1, String> {
    if profile.scope() != MtgoDuelPerceptionProfileScopeV1::ActingPlayerDuelSemanticPerception {
        return Err("perception profile is not admitted for acting-player duel capture".to_owned());
    }
    let request = MtgoDxgiCaptureRequestV3 {
        expected_executable_sha256: profile.executable_sha256().to_owned(),
        expected_signer_thumbprint: profile.signer_thumbprint().to_owned(),
        expected_signer_subject_sha256: profile.signer_subject_sha256().to_owned(),
        window_mode: CaptureWindowModeV2::DuelGame,
        expected_game_format: Some(profile.game_format().to_owned()),
        expected_title_contains: None,
        timeout_ms,
    };
    let source_frame = capture_mtgo_dxgi_frame_candidate_v3(request)?;
    bind_captured_duel_frame_to_profile_v1(profile, source_frame)
}

fn bind_captured_duel_frame_to_profile_v1(
    profile: &AdmittedMtgoDuelPerceptionProfileV1,
    source_frame: OpaqueMtgoDxgiFrameCandidateV3,
) -> Result<OpaqueMtgoAdmittedDuelVisibleFrameV1, String> {
    let manifest = &source_frame.manifest;
    if manifest.window_mode != "duel_game"
        || manifest.capture_role != "acting_player_duel"
        || manifest.expected_game_format != profile.game_format()
    {
        return Err("captured frame is not the admitted acting-player duel format".to_owned());
    }
    for snapshot in [&manifest.pre, &manifest.post] {
        if snapshot.executable_sha256 != profile.executable_sha256()
            || snapshot.signer_thumbprint != profile.signer_thumbprint()
            || snapshot.signer_subject_sha256 != profile.signer_subject_sha256()
            || snapshot.dpi != profile.dpi()
        {
            return Err(
                "captured MTGO identity or DPI differs from the admitted profile".to_owned(),
            );
        }
        if snapshot.client_rect_desktop_px.width()? != profile.client_size_px().width
            || snapshot.client_rect_desktop_px.height()? != profile.client_size_px().height
        {
            return Err("captured client geometry differs from the admitted profile".to_owned());
        }
    }
    if manifest.frame.canonical_width != profile.client_size_px().width
        || manifest.frame.canonical_height != profile.client_size_px().height
        || manifest.frame.canonical_stride
            != manifest
                .frame
                .canonical_width
                .checked_mul(4)
                .ok_or("canonical stride overflow")?
        || manifest.frame.canonical_byte_length != source_frame.canonical_bgra8.len()
        || manifest.frame.canonical_bgra8_sha256 != sha256_hex_v1(&source_frame.canonical_bgra8)
    {
        return Err("captured canonical pixels differ from the admitted geometry".to_owned());
    }

    let output_bounds = MtgoSignedRectDesktopPxV1 {
        left: manifest.output.bounds_desktop_px.left,
        top: manifest.output.bounds_desktop_px.top,
        width: manifest.output.bounds_desktop_px.width()?,
        height: manifest.output.bounds_desktop_px.height()?,
    };
    let output_identity_sha256 =
        preview_output_identity_commitment_v1(&manifest.output.device_name, &output_bounds)
            .map_err(|error| format!("captured output identity is invalid: {error}"))?;
    if output_identity_sha256 != profile.output_identity_sha256() {
        return Err("captured monitor output differs from the admitted profile".to_owned());
    }

    let frame_profile_binding_sha256 = frame_profile_binding_commitment_v1(
        profile.perception_profile_commitment_sha256(),
        profile.admission_commitment_sha256(),
        &source_frame.capture_commitment_sha256,
    );
    Ok(OpaqueMtgoAdmittedDuelVisibleFrameV1 {
        source_frame,
        perception_profile_commitment_sha256: profile
            .perception_profile_commitment_sha256()
            .to_owned(),
        perception_profile_admission_commitment_sha256: profile
            .admission_commitment_sha256()
            .to_owned(),
        frame_profile_binding_sha256,
    })
}

fn frame_profile_binding_commitment_v1(
    profile_commitment_sha256: &str,
    profile_admission_commitment_sha256: &str,
    source_capture_commitment_sha256: &str,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(DUEL_VISIBLE_FRAME_PROFILE_BINDING_DOMAIN_V1);
    for part in [
        profile_commitment_sha256.as_bytes(),
        profile_admission_commitment_sha256.as_bytes(),
        source_capture_commitment_sha256.as_bytes(),
    ] {
        hasher.update((part.len() as u64).to_le_bytes());
        hasher.update(part);
    }
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_profile_binding_commits_profile_admission_and_capture() {
        let baseline =
            frame_profile_binding_commitment_v1(&"a".repeat(64), &"b".repeat(64), &"c".repeat(64));
        assert_eq!(baseline.len(), 64);
        assert_ne!(
            baseline,
            frame_profile_binding_commitment_v1(&"a".repeat(64), &"b".repeat(64), &"d".repeat(64),)
        );
        assert_ne!(
            baseline,
            frame_profile_binding_commitment_v1(&"a".repeat(64), &"e".repeat(64), &"c".repeat(64),)
        );
    }
}
