use super::{
    capture_mtgo_dxgi_frame_candidate_v3, sha256_hex_v1, CaptureWindowModeV2,
    MtgoDxgiCaptureRequestV3, MtgoDxgiFrameCommitmentsV3, OpaqueMtgoDxgiFrameCandidateV3,
};
use mtgo_blackbox_v1::{
    preview_output_identity_commitment_v1, visible_frame_region_content_sha256_v1,
    AdmittedMtgoCompetitiveNavigationProfileV1, MtgoCompetitiveNavigationProfileScopeV1,
    MtgoSignedRectDesktopPxV1,
};
use sha2::{Digest, Sha256};

const COMPETITIVE_NAVIGATION_FRAME_PROFILE_BINDING_DOMAIN_V1: &[u8] =
    b"mtgo-admitted-competitive-navigation-frame-profile-binding-v1";

/// Copyable commitments for one in-process main-client navigation capture
/// bound to one separately admitted four-slice profile. This telemetry does
/// not prove capture and grants no downstream authority.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoAdmittedCompetitiveNavigationFrameCommitmentsV1 {
    pub profile_commitment_sha256: String,
    pub profile_admission_commitment_sha256: String,
    pub approved_account_alias_sha256: String,
    pub frame_profile_binding_sha256: String,
    pub source_capture: MtgoDxgiFrameCommitmentsV3,
}

/// An in-process DXGI main-client frame whose client identity, signer,
/// geometry, and display output match one separately admitted League and
/// Challenge navigation profile.
///
/// The frame remains an unclassified pixel source. It has no public fields,
/// raw-pixel accessor, serde implementation, lifecycle conversion, coordinate
/// accessor, event-entry conversion, spending conversion, or input conversion.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoAdmittedCompetitiveNavigationFrameV1;
/// let _forged = OpaqueMtgoAdmittedCompetitiveNavigationFrameV1 {};
/// ```
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoAdmittedCompetitiveNavigationFrameV1;
/// fn require_debug<T: std::fmt::Debug>() {}
/// require_debug::<OpaqueMtgoAdmittedCompetitiveNavigationFrameV1>();
/// ```
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoAdmittedCompetitiveNavigationFrameV1;
/// fn require_clone<T: Clone>() {}
/// require_clone::<OpaqueMtgoAdmittedCompetitiveNavigationFrameV1>();
/// ```
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoAdmittedCompetitiveNavigationFrameV1;
/// fn require_serialize<T: serde::Serialize>() {}
/// require_serialize::<OpaqueMtgoAdmittedCompetitiveNavigationFrameV1>();
/// ```
pub struct OpaqueMtgoAdmittedCompetitiveNavigationFrameV1 {
    pub(super) source_frame: OpaqueMtgoDxgiFrameCandidateV3,
    pub(super) profile_commitment_sha256: String,
    pub(super) profile_admission_commitment_sha256: String,
    pub(super) approved_account_alias_sha256: String,
    pub(super) frame_profile_binding_sha256: String,
}

impl OpaqueMtgoAdmittedCompetitiveNavigationFrameV1 {
    pub fn commitments_v1(&self) -> MtgoAdmittedCompetitiveNavigationFrameCommitmentsV1 {
        MtgoAdmittedCompetitiveNavigationFrameCommitmentsV1 {
            profile_commitment_sha256: self.profile_commitment_sha256.clone(),
            profile_admission_commitment_sha256: self.profile_admission_commitment_sha256.clone(),
            approved_account_alias_sha256: self.approved_account_alias_sha256.clone(),
            frame_profile_binding_sha256: self.frame_profile_binding_sha256.clone(),
            source_capture: self.source_frame.commitments_v3(),
        }
    }

    pub fn matches_admitted_competitive_navigation_profile_v1(&self) -> bool {
        true
    }

    pub fn safe_for_lifecycle_classification_v1(&self) -> bool {
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

/// Captures the foreground main MTGO client through Desktop Duplication and
/// binds the opaque result to one separately admitted four-slice navigation
/// profile. The profile's production ratification root is currently empty, so
/// application callers cannot reach capture through this function.
pub fn capture_admitted_mtgo_competitive_navigation_frame_v1(
    profile: &AdmittedMtgoCompetitiveNavigationProfileV1,
    timeout_ms: u32,
) -> Result<OpaqueMtgoAdmittedCompetitiveNavigationFrameV1, String> {
    if profile.scope()
        != MtgoCompetitiveNavigationProfileScopeV1::LeagueAndChallengeBrowserAndEntryReviewClassification
    {
        return Err(
            "navigation profile does not cover League and Challenge browser and entry review"
                .to_owned(),
        );
    }
    let runtime = profile.checked_runtime_profile();
    let request = MtgoDxgiCaptureRequestV3 {
        expected_executable_sha256: runtime.executable_sha256().to_owned(),
        expected_signer_thumbprint: runtime.signer_thumbprint().to_owned(),
        expected_signer_subject_sha256: runtime.signer_subject_sha256().to_owned(),
        window_mode: CaptureWindowModeV2::MainClient,
        expected_game_format: None,
        expected_title_contains: None,
        timeout_ms,
    };
    let source_frame = capture_mtgo_dxgi_frame_candidate_v3(request)?;
    bind_captured_competitive_navigation_frame_to_profile_v1(profile, source_frame)
}

fn bind_captured_competitive_navigation_frame_to_profile_v1(
    profile: &AdmittedMtgoCompetitiveNavigationProfileV1,
    source_frame: OpaqueMtgoDxgiFrameCandidateV3,
) -> Result<OpaqueMtgoAdmittedCompetitiveNavigationFrameV1, String> {
    let runtime = profile.checked_runtime_profile();
    let manifest = &source_frame.manifest;
    if manifest.window_mode != "main_client"
        || manifest.capture_role != "navigation"
        || !manifest.expected_game_format.is_empty()
    {
        return Err("captured frame is not a main-client navigation frame".to_owned());
    }
    for snapshot in [&manifest.pre, &manifest.post] {
        if snapshot.executable_sha256 != runtime.executable_sha256()
            || snapshot.signer_thumbprint != runtime.signer_thumbprint()
            || snapshot.signer_subject_sha256 != runtime.signer_subject_sha256()
            || sha256_hex_v1(snapshot.title.as_bytes()) != runtime.window_title_sha256()
            || snapshot.dpi != runtime.dpi()
        {
            return Err(
                "captured MTGO identity or DPI differs from the navigation profile".to_owned(),
            );
        }
        if snapshot.client_rect_desktop_px.width()? != runtime.client_size_px().width
            || snapshot.client_rect_desktop_px.height()? != runtime.client_size_px().height
        {
            return Err("captured client geometry differs from the navigation profile".to_owned());
        }
    }
    if manifest.frame.canonical_width != runtime.client_size_px().width
        || manifest.frame.canonical_height != runtime.client_size_px().height
        || manifest.frame.canonical_stride
            != manifest
                .frame
                .canonical_width
                .checked_mul(4)
                .ok_or("canonical stride overflow")?
        || manifest.frame.canonical_byte_length != source_frame.canonical_bgra8.len()
        || manifest.frame.canonical_bgra8_sha256 != sha256_hex_v1(&source_frame.canonical_bgra8)
    {
        return Err("captured pixels differ from the navigation profile geometry".to_owned());
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
    if output_identity_sha256 != runtime.output_identity_sha256() {
        return Err("captured display output differs from the navigation profile".to_owned());
    }
    let account_identity_region_sha256 = visible_frame_region_content_sha256_v1(
        &source_frame.canonical_bgra8,
        runtime.client_size_px(),
        runtime.account_identity_rect_client_px(),
    )
    .map_err(|error| format!("hash visible account identity region: {error}"))?;
    if account_identity_region_sha256 != runtime.account_identity_region_sha256() {
        return Err(
            "captured visible account identity differs from the approved-account profile"
                .to_owned(),
        );
    }

    let frame_profile_binding_sha256 = competitive_navigation_frame_binding_commitment_v1(
        profile.profile_commitment_sha256(),
        profile.admission_commitment_sha256(),
        runtime.approved_account_alias_sha256(),
        &source_frame.capture_commitment_sha256,
    );
    Ok(OpaqueMtgoAdmittedCompetitiveNavigationFrameV1 {
        source_frame,
        profile_commitment_sha256: profile.profile_commitment_sha256().to_owned(),
        profile_admission_commitment_sha256: profile.admission_commitment_sha256().to_owned(),
        approved_account_alias_sha256: runtime.approved_account_alias_sha256().to_owned(),
        frame_profile_binding_sha256,
    })
}

fn competitive_navigation_frame_binding_commitment_v1(
    profile_commitment_sha256: &str,
    profile_admission_commitment_sha256: &str,
    approved_account_alias_sha256: &str,
    source_capture_commitment_sha256: &str,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(COMPETITIVE_NAVIGATION_FRAME_PROFILE_BINDING_DOMAIN_V1);
    for part in [
        profile_commitment_sha256.as_bytes(),
        profile_admission_commitment_sha256.as_bytes(),
        approved_account_alias_sha256.as_bytes(),
        source_capture_commitment_sha256.as_bytes(),
        b"opaque_main_client_navigation_pixels_no_classification_no_entry_no_spending_no_input",
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
    fn navigation_frame_binding_commits_profile_admission_and_capture() {
        let baseline = competitive_navigation_frame_binding_commitment_v1(
            &"a".repeat(64),
            &"b".repeat(64),
            &"c".repeat(64),
            &"d".repeat(64),
        );
        assert_eq!(baseline.len(), 64);
        assert_ne!(
            baseline,
            competitive_navigation_frame_binding_commitment_v1(
                &"a".repeat(64),
                &"b".repeat(64),
                &"d".repeat(64),
                &"d".repeat(64),
            )
        );
        assert_ne!(
            baseline,
            competitive_navigation_frame_binding_commitment_v1(
                &"a".repeat(64),
                &"e".repeat(64),
                &"c".repeat(64),
                &"d".repeat(64),
            )
        );
        assert_ne!(
            baseline,
            competitive_navigation_frame_binding_commitment_v1(
                &"a".repeat(64),
                &"b".repeat(64),
                &"c".repeat(64),
                &"e".repeat(64),
            )
        );
    }
}
