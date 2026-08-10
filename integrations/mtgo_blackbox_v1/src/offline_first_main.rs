use crate::{
    hash_bgra_region_v1, CheckedUntrustedMtgoDxgiCaptureArtifactV1, MtgoContractErrorV1,
    MtgoDxgiCaptureRoleV2, MtgoRectPxV1, MtgoSizePxV1,
};
use sha2::{Digest, Sha256};

const OFFLINE_FIRST_MAIN_PROFILE_ID_V1: &str = "freeform-solitaire-first-main-turn-one-20260810-v1";
const OFFLINE_FIRST_MAIN_PROFILE_DOMAIN_V1: &[u8] = b"mtgo-offline-first-main-profile-v1";
const OFFLINE_FIRST_MAIN_CANDIDATE_DOMAIN_V1: &[u8] = b"mtgo-offline-first-main-candidate-v1";
const OFFLINE_FIRST_MAIN_OUTPUT_IDENTITY_V1: &str =
    "89c86876d12827c79ef4d746b9cd88c8decaf3e4fae33b41f6ab57c94bf222a6";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MtgoOfflineFirstMainClassificationV1 {
    Match,
    NoMatch,
}

#[derive(Clone)]
struct OfflineFirstMainRegionProfileV1 {
    label: &'static str,
    rect: MtgoRectPxV1,
    expected_bgra8_sha256: String,
}

#[derive(Clone)]
struct OfflineFirstMainProfileV1 {
    profile_id: &'static str,
    client_size_px: MtgoSizePxV1,
    output_identity_sha256: String,
    regions: Vec<OfflineFirstMainRegionProfileV1>,
}

/// An exact-template, checked-untrusted classification of the visible Turn 1
/// first-main state in acting-player Freeform Solitaire.
///
/// The fixed profile covers the first-main prompt, Combat control, Turn 1
/// label, and empty upper battlefield. It was observed identically after a
/// seven-card Keep and after a six-card London bottoming sequence. The result
/// retains no pixels or coordinates and grants no observation, policy, or
/// input authority.
///
/// ```compile_fail
/// use mtgo_blackbox_v1::CheckedUntrustedMtgoOfflineFirstMainCandidateV1;
/// fn pixel_escape(value: &CheckedUntrustedMtgoOfflineFirstMainCandidateV1) {
///     let _ = value.canonical_pixels_v1();
/// }
/// ```
///
/// ```compile_fail
/// use mtgo_blackbox_v1::CheckedUntrustedMtgoOfflineFirstMainCandidateV1;
/// fn coordinate_escape(value: &CheckedUntrustedMtgoOfflineFirstMainCandidateV1) {
///     let _ = value.region_rects_v1();
/// }
/// ```
pub struct CheckedUntrustedMtgoOfflineFirstMainCandidateV1 {
    classification: MtgoOfflineFirstMainClassificationV1,
    profile_commitment_sha256: String,
    source_manifest_sha256: String,
    source_frame_sha256: String,
    matched_region_count: usize,
    region_count: usize,
    candidate_commitment_sha256: String,
}

impl CheckedUntrustedMtgoOfflineFirstMainCandidateV1 {
    pub fn classification(&self) -> MtgoOfflineFirstMainClassificationV1 {
        self.classification
    }

    pub fn profile_commitment_sha256(&self) -> &str {
        &self.profile_commitment_sha256
    }

    pub fn source_manifest_sha256(&self) -> &str {
        &self.source_manifest_sha256
    }

    pub fn source_frame_sha256(&self) -> &str {
        &self.source_frame_sha256
    }

    pub fn matched_region_count(&self) -> usize {
        self.matched_region_count
    }

    pub fn region_count(&self) -> usize {
        self.region_count
    }

    pub fn candidate_commitment_sha256(&self) -> &str {
        &self.candidate_commitment_sha256
    }

    pub fn safe_for_live_frame(&self) -> bool {
        false
    }

    pub fn safe_for_semantic_evidence(&self) -> bool {
        false
    }

    pub fn safe_for_observation_v5(&self) -> bool {
        false
    }

    pub fn safe_for_policy_scoring(&self) -> bool {
        false
    }

    pub fn safe_for_input(&self) -> bool {
        false
    }
}

pub fn classify_untrusted_offline_first_main_candidate_v1(
    checked: &CheckedUntrustedMtgoDxgiCaptureArtifactV1,
    canonical_bgra8: &[u8],
) -> Result<CheckedUntrustedMtgoOfflineFirstMainCandidateV1, MtgoContractErrorV1> {
    let profile = production_profile_v1();
    let profile_commitment_sha256 = validate_and_commit_profile_v1(&profile)?;
    if checked.capture_role() != MtgoDxgiCaptureRoleV2::ActingPlayerSolitaire
        || checked.client_size_px() != &profile.client_size_px
        || checked.output_identity_sha256() != profile.output_identity_sha256
    {
        return Err(error_v1(
            "offline_first_main_source_layout",
            "candidate must match the reviewed acting-player Solitaire layout",
        ));
    }
    validate_pixel_binding_v1(checked, canonical_bgra8)?;

    let observed_region_hashes = profile
        .regions
        .iter()
        .map(|region| hash_bgra_region_v1(canonical_bgra8, checked.client_size_px(), &region.rect))
        .collect::<Result<Vec<_>, _>>()?;
    let matched_region_count = observed_region_hashes
        .iter()
        .zip(&profile.regions)
        .filter(|(observed, region)| observed.as_str() == region.expected_bgra8_sha256)
        .count();
    let classification = if matched_region_count == profile.regions.len() {
        MtgoOfflineFirstMainClassificationV1::Match
    } else {
        MtgoOfflineFirstMainClassificationV1::NoMatch
    };

    let mut hasher = Sha256::new();
    hasher.update(OFFLINE_FIRST_MAIN_CANDIDATE_DOMAIN_V1);
    for part in [
        profile_commitment_sha256.as_bytes(),
        checked.manifest_sha256().as_bytes(),
        checked.canonical_bgra8_sha256().as_bytes(),
        checked.output_identity_sha256().as_bytes(),
        match classification {
            MtgoOfflineFirstMainClassificationV1::Match => b"match".as_slice(),
            MtgoOfflineFirstMainClassificationV1::NoMatch => b"no_match".as_slice(),
        },
        &[u8::try_from(matched_region_count).unwrap_or(u8::MAX)],
    ] {
        update_hash_part_v1(&mut hasher, part);
    }
    for observed in &observed_region_hashes {
        update_hash_part_v1(&mut hasher, observed.as_bytes());
    }

    Ok(CheckedUntrustedMtgoOfflineFirstMainCandidateV1 {
        classification,
        profile_commitment_sha256,
        source_manifest_sha256: checked.manifest_sha256().to_owned(),
        source_frame_sha256: checked.canonical_bgra8_sha256().to_owned(),
        matched_region_count,
        region_count: profile.regions.len(),
        candidate_commitment_sha256: format!("{:x}", hasher.finalize()),
    })
}

fn production_profile_v1() -> OfflineFirstMainProfileV1 {
    OfflineFirstMainProfileV1 {
        profile_id: OFFLINE_FIRST_MAIN_PROFILE_ID_V1,
        client_size_px: MtgoSizePxV1 {
            width: 1_550,
            height: 925,
        },
        output_identity_sha256: OFFLINE_FIRST_MAIN_OUTPUT_IDENTITY_V1.to_owned(),
        regions: vec![
            OfflineFirstMainRegionProfileV1 {
                label: "first_main_prompt",
                rect: MtgoRectPxV1 {
                    x: 25,
                    y: 48,
                    width: 170,
                    height: 90,
                },
                expected_bgra8_sha256:
                    "259afeb14367f2d833fbf17ebd417ac7333b0d07ed53c194d8a5ba91f9a23b85".to_owned(),
            },
            OfflineFirstMainRegionProfileV1 {
                label: "combat_control",
                rect: MtgoRectPxV1 {
                    x: 29,
                    y: 153,
                    width: 82,
                    height: 33,
                },
                expected_bgra8_sha256:
                    "906928df7a7e0e1e4a8114a86dbaa3f8aa863d9116cb63c2a015ba6c6c2a7de3".to_owned(),
            },
            OfflineFirstMainRegionProfileV1 {
                label: "turn_one_label",
                rect: MtgoRectPxV1 {
                    x: 311,
                    y: 696,
                    width: 100,
                    height: 30,
                },
                expected_bgra8_sha256:
                    "ef7e60faba58a8d5f0b5d58304f7e83a764b4f9845d1987e8db90927dd3a9471".to_owned(),
            },
            OfflineFirstMainRegionProfileV1 {
                label: "empty_upper_battlefield",
                rect: MtgoRectPxV1 {
                    x: 311,
                    y: 221,
                    width: 852,
                    height: 155,
                },
                expected_bgra8_sha256:
                    "436574fbb868e7c2b37f6680225faf8b53cffe5109b54b1cbde090db4c658821".to_owned(),
            },
        ],
    }
}

fn validate_and_commit_profile_v1(
    profile: &OfflineFirstMainProfileV1,
) -> Result<String, MtgoContractErrorV1> {
    if profile.profile_id.is_empty()
        || profile.profile_id.len() > 96
        || !profile
            .profile_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        || !is_lower_sha256_v1(&profile.output_identity_sha256)
        || profile.client_size_px.width == 0
        || profile.client_size_px.height == 0
        || profile.regions.len() != 4
    {
        return Err(error_v1(
            "offline_first_main_profile",
            "profile identity, layout, or region count is invalid",
        ));
    }
    let expected_labels = [
        "first_main_prompt",
        "combat_control",
        "turn_one_label",
        "empty_upper_battlefield",
    ];
    if profile
        .regions
        .iter()
        .map(|region| region.label)
        .ne(expected_labels)
    {
        return Err(error_v1(
            "offline_first_main_profile",
            "profile regions must use the canonical order and labels",
        ));
    }

    let mut hasher = Sha256::new();
    hasher.update(OFFLINE_FIRST_MAIN_PROFILE_DOMAIN_V1);
    for part in [
        profile.profile_id.as_bytes(),
        profile.output_identity_sha256.as_bytes(),
        &profile.client_size_px.width.to_be_bytes(),
        &profile.client_size_px.height.to_be_bytes(),
    ] {
        update_hash_part_v1(&mut hasher, part);
    }
    for region in &profile.regions {
        validate_region_v1(&profile.client_size_px, &region.rect)?;
        if !is_lower_sha256_v1(&region.expected_bgra8_sha256) {
            return Err(error_v1(
                "offline_first_main_profile",
                "every region requires a lowercase SHA-256",
            ));
        }
        update_hash_part_v1(&mut hasher, region.label.as_bytes());
        for value in [
            region.rect.x,
            region.rect.y,
            region.rect.width,
            region.rect.height,
        ] {
            update_hash_part_v1(&mut hasher, &value.to_be_bytes());
        }
        update_hash_part_v1(&mut hasher, region.expected_bgra8_sha256.as_bytes());
    }
    for (index, left) in profile.regions.iter().enumerate() {
        for right in profile.regions.iter().skip(index + 1) {
            if rects_overlap_v1(&left.rect, &right.rect) {
                return Err(error_v1(
                    "offline_first_main_profile",
                    "profile regions must not overlap",
                ));
            }
        }
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn rects_overlap_v1(left: &MtgoRectPxV1, right: &MtgoRectPxV1) -> bool {
    let left_right = left.x.saturating_add(left.width);
    let left_bottom = left.y.saturating_add(left.height);
    let right_right = right.x.saturating_add(right.width);
    let right_bottom = right.y.saturating_add(right.height);
    left.x < right_right && right.x < left_right && left.y < right_bottom && right.y < left_bottom
}

fn validate_pixel_binding_v1(
    checked: &CheckedUntrustedMtgoDxgiCaptureArtifactV1,
    pixels: &[u8],
) -> Result<(), MtgoContractErrorV1> {
    let expected = usize::try_from(checked.client_size_px().width)
        .ok()
        .and_then(|width| {
            usize::try_from(checked.client_size_px().height)
                .ok()
                .and_then(|height| width.checked_mul(height))
        })
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or_else(|| error_v1("offline_first_main_pixels", "pixel length overflow"))?;
    if pixels.len() != expected
        || format!("{:x}", Sha256::digest(pixels)) != checked.canonical_bgra8_sha256()
    {
        return Err(error_v1(
            "offline_first_main_pixels",
            "pixels must bind the checked artifact",
        ));
    }
    Ok(())
}

fn validate_region_v1(size: &MtgoSizePxV1, rect: &MtgoRectPxV1) -> Result<(), MtgoContractErrorV1> {
    let right = rect.x.checked_add(rect.width);
    let bottom = rect.y.checked_add(rect.height);
    if rect.width == 0
        || rect.height == 0
        || right.is_none_or(|right| right > size.width)
        || bottom.is_none_or(|bottom| bottom > size.height)
    {
        return Err(error_v1(
            "offline_first_main_region",
            "profile region is outside the client",
        ));
    }
    Ok(())
}

fn is_lower_sha256_v1(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

fn update_hash_part_v1(hasher: &mut Sha256, part: &[u8]) {
    hasher.update((part.len() as u64).to_be_bytes());
    hasher.update(part);
}

fn error_v1(code: &'static str, detail: impl Into<String>) -> MtgoContractErrorV1 {
    MtgoContractErrorV1::new(code, detail)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn production_profile_is_exact_and_committed() {
        let profile = production_profile_v1();
        assert_eq!(profile.regions.len(), 4);
        assert_eq!(
            profile
                .regions
                .iter()
                .map(|region| region.label)
                .collect::<Vec<_>>(),
            [
                "first_main_prompt",
                "combat_control",
                "turn_one_label",
                "empty_upper_battlefield"
            ]
        );
        assert_eq!(
            validate_and_commit_profile_v1(&profile).unwrap(),
            "886531d4dd6efd4d59759f3a2ac1429ad2116967ddb03479eb5ee8fea078eb6a"
        );
    }

    #[test]
    fn malformed_profile_fails_closed() {
        let mut profile = production_profile_v1();
        profile.regions[0].rect.width = u32::MAX;
        assert_eq!(
            validate_and_commit_profile_v1(&profile)
                .err()
                .unwrap()
                .code(),
            "offline_first_main_region"
        );

        let mut profile = production_profile_v1();
        profile.regions.swap(0, 1);
        assert_eq!(
            validate_and_commit_profile_v1(&profile)
                .err()
                .unwrap()
                .code(),
            "offline_first_main_profile"
        );

        let mut profile = production_profile_v1();
        profile.regions[1].rect = profile.regions[0].rect.clone();
        assert_eq!(
            validate_and_commit_profile_v1(&profile)
                .err()
                .unwrap()
                .code(),
            "offline_first_main_profile"
        );
    }
}
