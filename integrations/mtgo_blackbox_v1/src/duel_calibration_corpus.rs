use crate::{
    CheckedUntrustedMtgoDxgiCaptureArtifactV1, MtgoContractErrorV1, MtgoDxgiCaptureRoleV2,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;

pub const MTGO_ACTING_PLAYER_DUEL_CALIBRATION_CORPUS_SCHEMA_V1: u32 = 1;

const CORPUS_KIND_V1: &str = "mtgo_acting_player_duel_visible_capture_corpus_v1";
const INFORMATION_BOUNDARY_V1: &str = "player_visible_ui_facts_only_v1";
const CORPUS_COMMITMENT_DOMAIN_V1: &[u8] = b"mtgo-acting-player-duel-calibration-corpus-v1";
const MAX_CORPUS_SAMPLES_V1: usize = 4_096;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoActingPlayerDuelCalibrationCorpusSampleV1 {
    pub sample_id: String,
    pub corpus_sequence: u64,
    pub source_manifest_sha256: String,
    pub source_canonical_bgra8_sha256: String,
    pub source_preview_png_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoActingPlayerDuelCalibrationCorpusManifestV1 {
    pub schema_version: u32,
    pub corpus_kind: String,
    pub corpus_id: String,
    pub information_boundary: String,
    pub capture_role: MtgoDxgiCaptureRoleV2,
    pub game_format: String,
    pub samples: Vec<MtgoActingPlayerDuelCalibrationCorpusSampleV1>,
    pub safe_for_semantic_evidence: bool,
    pub safe_for_ocr: bool,
    pub safe_for_policy_scoring: bool,
    pub safe_for_input: bool,
}

/// A deterministic, path-free index over structurally checked acting-player
/// duel captures. This type has no pixels, visible title, participant alias,
/// match identifier, process identifier, process path, observation, action, or
/// authority conversion.
///
/// It remains checked-untrusted. Human annotation and a separately reviewed
/// perception profile are still required before any source frame can become
/// semantic evidence.
///
/// ```compile_fail
/// use mtgo_blackbox_v1::CheckedUntrustedMtgoActingPlayerDuelCalibrationCorpusV1;
/// let _forged = CheckedUntrustedMtgoActingPlayerDuelCalibrationCorpusV1 {};
/// ```
///
/// ```compile_fail
/// use mtgo_blackbox_v1::CheckedUntrustedMtgoActingPlayerDuelCalibrationCorpusV1;
/// fn require_debug<T: std::fmt::Debug>() {}
/// require_debug::<CheckedUntrustedMtgoActingPlayerDuelCalibrationCorpusV1>();
/// ```
pub struct CheckedUntrustedMtgoActingPlayerDuelCalibrationCorpusV1 {
    manifest: MtgoActingPlayerDuelCalibrationCorpusManifestV1,
    corpus_commitment_sha256: String,
}

impl CheckedUntrustedMtgoActingPlayerDuelCalibrationCorpusV1 {
    pub fn manifest_v1(&self) -> &MtgoActingPlayerDuelCalibrationCorpusManifestV1 {
        &self.manifest
    }

    pub fn corpus_commitment_sha256(&self) -> &str {
        &self.corpus_commitment_sha256
    }

    pub fn sample_count(&self) -> usize {
        self.manifest.samples.len()
    }

    pub fn safe_for_semantic_evidence(&self) -> bool {
        false
    }

    pub fn safe_for_ocr(&self) -> bool {
        false
    }

    pub fn safe_for_policy_scoring(&self) -> bool {
        false
    }

    pub fn safe_for_input(&self) -> bool {
        false
    }
}

pub fn build_checked_untrusted_acting_player_duel_calibration_corpus_v1(
    corpus_id: &str,
    artifacts: &[&CheckedUntrustedMtgoDxgiCaptureArtifactV1],
) -> Result<CheckedUntrustedMtgoActingPlayerDuelCalibrationCorpusV1, MtgoContractErrorV1> {
    validate_identifier_v1(corpus_id)?;
    if artifacts.is_empty() || artifacts.len() > MAX_CORPUS_SAMPLES_V1 {
        return Err(error_v1(
            "duel_calibration_corpus_sample_count",
            format!("corpus must contain between 1 and {MAX_CORPUS_SAMPLES_V1} source artifacts"),
        ));
    }

    let mut ordered = artifacts.to_vec();
    ordered.sort_by(|left, right| {
        left.canonical_bgra8_sha256()
            .cmp(right.canonical_bgra8_sha256())
            .then_with(|| left.preview_png_sha256().cmp(right.preview_png_sha256()))
    });
    let first = ordered[0];
    require_acting_player_duel_v1(first)?;
    let game_format = first
        .game_format()
        .ok_or_else(|| error_v1("duel_calibration_corpus_format", "duel format is missing"))?;

    let mut pixel_hashes = HashSet::new();
    let mut preview_hashes = HashSet::new();
    let mut manifest_hashes = HashSet::new();
    let mut samples = Vec::with_capacity(ordered.len());
    for (index, artifact) in ordered.into_iter().enumerate() {
        require_acting_player_duel_v1(artifact)?;
        require_same_capture_identity_v1(first, artifact)?;
        if artifact.game_format() != Some(game_format) {
            return Err(error_v1(
                "duel_calibration_corpus_format",
                "all source artifacts must use the same visible game format",
            ));
        }
        if !manifest_hashes.insert(artifact.manifest_sha256())
            || !pixel_hashes.insert(artifact.canonical_bgra8_sha256())
            || !preview_hashes.insert(artifact.preview_png_sha256())
        {
            return Err(error_v1(
                "duel_calibration_corpus_duplicate_source",
                "visible frame pixel commitments must be unique",
            ));
        }
        let corpus_sequence = u64::try_from(index + 1).map_err(|_| {
            error_v1(
                "duel_calibration_corpus_sequence",
                "source sequence does not fit u64",
            )
        })?;
        samples.push(MtgoActingPlayerDuelCalibrationCorpusSampleV1 {
            sample_id: format!("duel-frame-{corpus_sequence:06}"),
            corpus_sequence,
            source_manifest_sha256: artifact.manifest_sha256().to_owned(),
            source_canonical_bgra8_sha256: artifact.canonical_bgra8_sha256().to_owned(),
            source_preview_png_sha256: artifact.preview_png_sha256().to_owned(),
        });
    }

    let manifest = MtgoActingPlayerDuelCalibrationCorpusManifestV1 {
        schema_version: MTGO_ACTING_PLAYER_DUEL_CALIBRATION_CORPUS_SCHEMA_V1,
        corpus_kind: CORPUS_KIND_V1.to_owned(),
        corpus_id: corpus_id.to_owned(),
        information_boundary: INFORMATION_BOUNDARY_V1.to_owned(),
        capture_role: MtgoDxgiCaptureRoleV2::ActingPlayerDuel,
        game_format: game_format.to_owned(),
        samples,
        safe_for_semantic_evidence: false,
        safe_for_ocr: false,
        safe_for_policy_scoring: false,
        safe_for_input: false,
    };
    validate_untrusted_acting_player_duel_calibration_corpus_v1(&manifest, artifacts)
}

pub fn validate_untrusted_acting_player_duel_calibration_corpus_v1(
    manifest: &MtgoActingPlayerDuelCalibrationCorpusManifestV1,
    artifacts: &[&CheckedUntrustedMtgoDxgiCaptureArtifactV1],
) -> Result<CheckedUntrustedMtgoActingPlayerDuelCalibrationCorpusV1, MtgoContractErrorV1> {
    validate_identifier_v1(&manifest.corpus_id)?;
    if manifest.schema_version != MTGO_ACTING_PLAYER_DUEL_CALIBRATION_CORPUS_SCHEMA_V1
        || manifest.corpus_kind != CORPUS_KIND_V1
        || manifest.information_boundary != INFORMATION_BOUNDARY_V1
        || manifest.capture_role != MtgoDxgiCaptureRoleV2::ActingPlayerDuel
    {
        return Err(error_v1(
            "duel_calibration_corpus_header",
            "schema, kind, information boundary, and acting-player duel role must be exact",
        ));
    }
    if manifest.safe_for_semantic_evidence
        || manifest.safe_for_ocr
        || manifest.safe_for_policy_scoring
        || manifest.safe_for_input
    {
        return Err(error_v1(
            "duel_calibration_corpus_authority",
            "an unreviewed calibration corpus grants no evidence, OCR, scoring, or input authority",
        ));
    }
    if manifest.samples.is_empty()
        || manifest.samples.len() > MAX_CORPUS_SAMPLES_V1
        || manifest.samples.len() != artifacts.len()
    {
        return Err(error_v1(
            "duel_calibration_corpus_sample_count",
            "manifest and source artifact counts must match and be within bounds",
        ));
    }

    let mut ordered = artifacts.to_vec();
    ordered.sort_by(|left, right| {
        left.canonical_bgra8_sha256()
            .cmp(right.canonical_bgra8_sha256())
            .then_with(|| left.preview_png_sha256().cmp(right.preview_png_sha256()))
    });
    let first = ordered[0];
    require_acting_player_duel_v1(first)?;
    if Some(manifest.game_format.as_str()) != first.game_format() {
        return Err(error_v1(
            "duel_calibration_corpus_format",
            "corpus format must match the exact checked source format",
        ));
    }

    let mut pixel_hashes = HashSet::new();
    let mut preview_hashes = HashSet::new();
    let mut manifest_hashes = HashSet::new();
    for (index, (sample, artifact)) in manifest.samples.iter().zip(ordered).enumerate() {
        require_acting_player_duel_v1(artifact)?;
        require_same_capture_identity_v1(first, artifact)?;
        if artifact.game_format() != Some(manifest.game_format.as_str()) {
            return Err(error_v1(
                "duel_calibration_corpus_format",
                "all source artifacts must use the manifest game format",
            ));
        }
        let expected_sequence = u64::try_from(index + 1).map_err(|_| {
            error_v1(
                "duel_calibration_corpus_sequence",
                "source sequence does not fit u64",
            )
        })?;
        if sample.sample_id != format!("duel-frame-{expected_sequence:06}")
            || sample.corpus_sequence != expected_sequence
            || sample.source_manifest_sha256 != artifact.manifest_sha256()
            || sample.source_canonical_bgra8_sha256 != artifact.canonical_bgra8_sha256()
            || sample.source_preview_png_sha256 != artifact.preview_png_sha256()
        {
            return Err(error_v1(
                "duel_calibration_corpus_source_binding",
                "every sample must bind its exact ordered checked source artifact",
            ));
        }
        if !manifest_hashes.insert(sample.source_manifest_sha256.as_str())
            || !pixel_hashes.insert(sample.source_canonical_bgra8_sha256.as_str())
            || !preview_hashes.insert(sample.source_preview_png_sha256.as_str())
        {
            return Err(error_v1(
                "duel_calibration_corpus_duplicate_source",
                "visible frame pixel commitments must be unique",
            ));
        }
    }

    let corpus_commitment_sha256 = corpus_commitment_v1(manifest)?;
    Ok(CheckedUntrustedMtgoActingPlayerDuelCalibrationCorpusV1 {
        manifest: manifest.clone(),
        corpus_commitment_sha256,
    })
}

fn require_acting_player_duel_v1(
    artifact: &CheckedUntrustedMtgoDxgiCaptureArtifactV1,
) -> Result<(), MtgoContractErrorV1> {
    if artifact.capture_role() != MtgoDxgiCaptureRoleV2::ActingPlayerDuel
        || artifact.game_format().is_none()
        || artifact.safe_for_semantic_evidence()
        || artifact.safe_for_ocr()
        || artifact.safe_for_policy_scoring()
        || artifact.safe_for_input()
    {
        return Err(error_v1(
            "duel_calibration_corpus_source_role",
            "every source must be a checked-untrusted acting-player duel capture with no runtime authority",
        ));
    }
    Ok(())
}

fn require_same_capture_identity_v1(
    first: &CheckedUntrustedMtgoDxgiCaptureArtifactV1,
    candidate: &CheckedUntrustedMtgoDxgiCaptureArtifactV1,
) -> Result<(), MtgoContractErrorV1> {
    if first.executable_sha256() != candidate.executable_sha256()
        || first.signer_thumbprint() != candidate.signer_thumbprint()
        || first.signer_subject_sha256() != candidate.signer_subject_sha256()
        || first.dpi() != candidate.dpi()
        || first.client_size_px() != candidate.client_size_px()
        || first.output_identity_sha256() != candidate.output_identity_sha256()
    {
        return Err(error_v1(
            "duel_calibration_corpus_identity",
            "all sources must share executable, signer, DPI, client size, and output identity",
        ));
    }
    Ok(())
}

fn corpus_commitment_v1(
    manifest: &MtgoActingPlayerDuelCalibrationCorpusManifestV1,
) -> Result<String, MtgoContractErrorV1> {
    let bytes = serde_json::to_vec(manifest)
        .map_err(|error| error_v1("duel_calibration_corpus_serialization", error.to_string()))?;
    let mut hasher = Sha256::new();
    hasher.update(CORPUS_COMMITMENT_DOMAIN_V1);
    hasher.update((bytes.len() as u64).to_be_bytes());
    hasher.update(bytes);
    Ok(format!("{:x}", hasher.finalize()))
}

fn validate_identifier_v1(value: &str) -> Result<(), MtgoContractErrorV1> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err(error_v1(
            "duel_calibration_corpus_id",
            "corpus ID must be a bounded ASCII identifier",
        ));
    }
    Ok(())
}

fn error_v1(code: &'static str, detail: impl Into<String>) -> MtgoContractErrorV1 {
    MtgoContractErrorV1::new(code, detail)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checked_untrusted_dxgi_artifact_for_test_v1;

    #[test]
    fn single_acting_player_duel_source_builds_path_free_non_authorizing_corpus() {
        let source =
            checked_untrusted_dxgi_artifact_for_test_v1(MtgoDxgiCaptureRoleV2::ActingPlayerDuel);
        let built = build_checked_untrusted_acting_player_duel_calibration_corpus_v1(
            "duel-visible-corpus-v1",
            &[&source],
        )
        .unwrap();

        assert_eq!(built.sample_count(), 1);
        assert_eq!(
            built.manifest_v1().information_boundary,
            "player_visible_ui_facts_only_v1"
        );
        assert_eq!(
            built.manifest_v1().capture_role,
            MtgoDxgiCaptureRoleV2::ActingPlayerDuel
        );
        assert_eq!(
            built.manifest_v1().samples[0].sample_id,
            "duel-frame-000001"
        );
        assert!(!built.safe_for_semantic_evidence());
        assert!(!built.safe_for_ocr());
        assert!(!built.safe_for_policy_scoring());
        assert!(!built.safe_for_input());

        let json = serde_json::to_string(built.manifest_v1()).unwrap();
        for forbidden in [
            "artifact_directory",
            "window_title",
            "participant",
            "opponent",
            "match_id",
            "process_id",
            "process_image",
            "executable_sha256",
            "signer_thumbprint",
            "signer_subject_sha256",
            "output_identity_sha256",
            "captured_at_unix_millis",
            "dpi",
            "frame.bgra",
            "frame.png",
        ] {
            assert!(!json.contains(forbidden));
        }
    }

    #[test]
    fn corpus_commitment_is_deterministic() {
        let source =
            checked_untrusted_dxgi_artifact_for_test_v1(MtgoDxgiCaptureRoleV2::ActingPlayerDuel);
        let first = build_checked_untrusted_acting_player_duel_calibration_corpus_v1(
            "duel-visible-corpus-v1",
            &[&source],
        )
        .unwrap();
        let second = build_checked_untrusted_acting_player_duel_calibration_corpus_v1(
            "duel-visible-corpus-v1",
            &[&source],
        )
        .unwrap();
        assert_eq!(
            first.corpus_commitment_sha256(),
            second.corpus_commitment_sha256()
        );
    }

    #[test]
    fn non_duel_and_duplicate_sources_are_rejected() {
        let solitaire = checked_untrusted_dxgi_artifact_for_test_v1(
            MtgoDxgiCaptureRoleV2::ActingPlayerSolitaire,
        );
        let error = build_checked_untrusted_acting_player_duel_calibration_corpus_v1(
            "duel-visible-corpus-v1",
            &[&solitaire],
        )
        .err()
        .unwrap();
        assert_eq!(error.code(), "duel_calibration_corpus_source_role");

        let duel =
            checked_untrusted_dxgi_artifact_for_test_v1(MtgoDxgiCaptureRoleV2::ActingPlayerDuel);
        let error = build_checked_untrusted_acting_player_duel_calibration_corpus_v1(
            "duel-visible-corpus-v1",
            &[&duel, &duel],
        )
        .err()
        .unwrap();
        assert_eq!(error.code(), "duel_calibration_corpus_duplicate_source");
    }

    #[test]
    fn manifest_tampering_and_authority_claims_are_rejected() {
        let source =
            checked_untrusted_dxgi_artifact_for_test_v1(MtgoDxgiCaptureRoleV2::ActingPlayerDuel);
        let built = build_checked_untrusted_acting_player_duel_calibration_corpus_v1(
            "duel-visible-corpus-v1",
            &[&source],
        )
        .unwrap();

        let mut authority = built.manifest_v1().clone();
        authority.safe_for_policy_scoring = true;
        let error =
            validate_untrusted_acting_player_duel_calibration_corpus_v1(&authority, &[&source])
                .err()
                .unwrap();
        assert_eq!(error.code(), "duel_calibration_corpus_authority");

        let mut binding = built.manifest_v1().clone();
        binding.samples[0].source_canonical_bgra8_sha256 = "f".repeat(64);
        let error =
            validate_untrusted_acting_player_duel_calibration_corpus_v1(&binding, &[&source])
                .err()
                .unwrap();
        assert_eq!(error.code(), "duel_calibration_corpus_source_binding");
    }
}
