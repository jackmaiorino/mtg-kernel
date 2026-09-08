use crate::{
    hash_bgra_region_v1, CheckedUntrustedMtgoDxgiCaptureArtifactV1, MtgoContractErrorV1,
    MtgoDxgiCaptureRoleV2, MtgoRectPxV1, MtgoSizePxV1,
};
use sha2::{Digest, Sha256};

const OFFLINE_BOTTOM_SIX_INITIAL_PROFILE_ID_V1: &str =
    "freeform-solitaire-bottom-six-zero-selected-20260810-v1";
const OFFLINE_BOTTOM_SIX_INITIAL_PROFILE_DOMAIN_V1: &[u8] =
    b"mtgo-offline-bottom-six-initial-profile-v1";
const OFFLINE_BOTTOM_SIX_INITIAL_CANDIDATE_DOMAIN_V1: &[u8] =
    b"mtgo-offline-bottom-six-initial-candidate-v1";
const OFFLINE_BOTTOM_SIX_INITIAL_OUTPUT_IDENTITY_V1: &str =
    "89c86876d12827c79ef4d746b9cd88c8decaf3e4fae33b41f6ab57c94bf222a6";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MtgoOfflineBottomSixInitialClassificationV1 {
    Match,
    NoMatch,
}

#[derive(Clone)]
struct OfflineBottomSixExactRegionProfileV1 {
    label: &'static str,
    rect: MtgoRectPxV1,
    expected_bgra8_sha256: String,
}

#[derive(Clone)]
struct OfflineBottomSixOccupancyProfileV1 {
    label: &'static str,
    rect: MtgoRectPxV1,
    channel_threshold_exclusive: u8,
    minimum_bright_pixel_count: u32,
}

#[derive(Clone)]
struct OfflineBottomSixInitialProfileV1 {
    profile_id: &'static str,
    client_size_px: MtgoSizePxV1,
    output_identity_sha256: String,
    exact_regions: Vec<OfflineBottomSixExactRegionProfileV1>,
    occupancy: OfflineBottomSixOccupancyProfileV1,
}

/// A checked-untrusted recognition of one exact MTGO state: the visible
/// Freeform Solitaire prompt that requires six London-bottom selections before
/// any card has been selected.
///
/// Three exact visible templates bind the prompt, Cancel control, and Turn 1
/// label. A fourth structural feature requires visible occupancy in the seventh
/// hand slot. This distinguishes the reviewed zero-selected frame from later
/// card-reflow states without binding the classifier to one card's artwork.
/// The result retains no pixels or coordinates and grants no observation,
/// policy, or input authority.
///
/// ```compile_fail
/// use mtgo_blackbox_v1::CheckedUntrustedMtgoOfflineBottomSixInitialCandidateV1;
/// fn pixel_escape(value: &CheckedUntrustedMtgoOfflineBottomSixInitialCandidateV1) {
///     let _ = value.canonical_pixels_v1();
/// }
/// ```
///
/// ```compile_fail
/// use mtgo_blackbox_v1::CheckedUntrustedMtgoOfflineBottomSixInitialCandidateV1;
/// fn coordinate_escape(value: &CheckedUntrustedMtgoOfflineBottomSixInitialCandidateV1) {
///     let _ = value.region_rects_v1();
/// }
/// ```
pub struct CheckedUntrustedMtgoOfflineBottomSixInitialCandidateV1 {
    classification: MtgoOfflineBottomSixInitialClassificationV1,
    profile_commitment_sha256: String,
    source_manifest_sha256: String,
    source_frame_sha256: String,
    matched_exact_region_count: usize,
    exact_region_count: usize,
    bright_occupancy_pixel_count: u32,
    occupancy_matches: bool,
    candidate_commitment_sha256: String,
}

impl CheckedUntrustedMtgoOfflineBottomSixInitialCandidateV1 {
    pub fn classification(&self) -> MtgoOfflineBottomSixInitialClassificationV1 {
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

    pub fn matched_exact_region_count(&self) -> usize {
        self.matched_exact_region_count
    }

    pub fn exact_region_count(&self) -> usize {
        self.exact_region_count
    }

    pub fn bright_occupancy_pixel_count(&self) -> u32 {
        self.bright_occupancy_pixel_count
    }

    pub fn occupancy_matches(&self) -> bool {
        self.occupancy_matches
    }

    pub fn required_bottom_count(&self) -> Option<u8> {
        (self.classification == MtgoOfflineBottomSixInitialClassificationV1::Match).then_some(6)
    }

    pub fn selected_count(&self) -> Option<u8> {
        (self.classification == MtgoOfflineBottomSixInitialClassificationV1::Match).then_some(0)
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

pub fn classify_untrusted_offline_bottom_six_initial_candidate_v1(
    checked: &CheckedUntrustedMtgoDxgiCaptureArtifactV1,
    canonical_bgra8: &[u8],
) -> Result<CheckedUntrustedMtgoOfflineBottomSixInitialCandidateV1, MtgoContractErrorV1> {
    let profile = production_profile_v1();
    let profile_commitment_sha256 = validate_and_commit_profile_v1(&profile)?;
    if checked.capture_role() != MtgoDxgiCaptureRoleV2::ActingPlayerSolitaire
        || checked.client_size_px() != &profile.client_size_px
        || checked.output_identity_sha256() != profile.output_identity_sha256
    {
        return Err(error_v1(
            "offline_bottom_six_initial_source_layout",
            "candidate must match the reviewed acting-player Solitaire layout",
        ));
    }
    validate_pixel_binding_v1(checked, canonical_bgra8)?;

    let observed_exact_hashes = profile
        .exact_regions
        .iter()
        .map(|region| hash_bgra_region_v1(canonical_bgra8, checked.client_size_px(), &region.rect))
        .collect::<Result<Vec<_>, _>>()?;
    let matched_exact_region_count = observed_exact_hashes
        .iter()
        .zip(&profile.exact_regions)
        .filter(|(observed, region)| observed.as_str() == region.expected_bgra8_sha256)
        .count();
    let bright_occupancy_pixel_count = count_bright_pixels_v1(
        canonical_bgra8,
        checked.client_size_px(),
        &profile.occupancy.rect,
        profile.occupancy.channel_threshold_exclusive,
    )?;
    let occupancy_matches =
        bright_occupancy_pixel_count >= profile.occupancy.minimum_bright_pixel_count;
    let classification =
        if matched_exact_region_count == profile.exact_regions.len() && occupancy_matches {
            MtgoOfflineBottomSixInitialClassificationV1::Match
        } else {
            MtgoOfflineBottomSixInitialClassificationV1::NoMatch
        };

    let mut hasher = Sha256::new();
    hasher.update(OFFLINE_BOTTOM_SIX_INITIAL_CANDIDATE_DOMAIN_V1);
    for part in [
        profile_commitment_sha256.as_bytes(),
        checked.manifest_sha256().as_bytes(),
        checked.canonical_bgra8_sha256().as_bytes(),
        checked.output_identity_sha256().as_bytes(),
        match classification {
            MtgoOfflineBottomSixInitialClassificationV1::Match => b"match".as_slice(),
            MtgoOfflineBottomSixInitialClassificationV1::NoMatch => b"no_match".as_slice(),
        },
        &[u8::try_from(matched_exact_region_count).unwrap_or(u8::MAX)],
        &bright_occupancy_pixel_count.to_be_bytes(),
        &[u8::from(occupancy_matches)],
    ] {
        update_hash_part_v1(&mut hasher, part);
    }
    for observed in &observed_exact_hashes {
        update_hash_part_v1(&mut hasher, observed.as_bytes());
    }

    Ok(CheckedUntrustedMtgoOfflineBottomSixInitialCandidateV1 {
        classification,
        profile_commitment_sha256,
        source_manifest_sha256: checked.manifest_sha256().to_owned(),
        source_frame_sha256: checked.canonical_bgra8_sha256().to_owned(),
        matched_exact_region_count,
        exact_region_count: profile.exact_regions.len(),
        bright_occupancy_pixel_count,
        occupancy_matches,
        candidate_commitment_sha256: format!("{:x}", hasher.finalize()),
    })
}

fn production_profile_v1() -> OfflineBottomSixInitialProfileV1 {
    OfflineBottomSixInitialProfileV1 {
        profile_id: OFFLINE_BOTTOM_SIX_INITIAL_PROFILE_ID_V1,
        client_size_px: MtgoSizePxV1 {
            width: 1_550,
            height: 925,
        },
        output_identity_sha256: OFFLINE_BOTTOM_SIX_INITIAL_OUTPUT_IDENTITY_V1.to_owned(),
        exact_regions: vec![
            OfflineBottomSixExactRegionProfileV1 {
                label: "bottom_six_prompt",
                rect: MtgoRectPxV1 {
                    x: 28,
                    y: 52,
                    width: 160,
                    height: 103,
                },
                expected_bgra8_sha256:
                    "5482a278829a9a87017b223d593c7815b86614d0388613b05d7b60210e5b67d2".to_owned(),
            },
            OfflineBottomSixExactRegionProfileV1 {
                label: "cancel_only_control",
                rect: MtgoRectPxV1 {
                    x: 30,
                    y: 175,
                    width: 62,
                    height: 28,
                },
                expected_bgra8_sha256:
                    "06bbd97b92f84bdfdf4fad8d9cab9d964a643dacef64ba80fc7e7b52340a6615".to_owned(),
            },
            OfflineBottomSixExactRegionProfileV1 {
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
        ],
        occupancy: OfflineBottomSixOccupancyProfileV1 {
            label: "seventh_hand_slot_occupancy",
            rect: MtgoRectPxV1 {
                x: 1_120,
                y: 740,
                width: 30,
                height: 30,
            },
            channel_threshold_exclusive: 48,
            minimum_bright_pixel_count: 200,
        },
    }
}

fn validate_and_commit_profile_v1(
    profile: &OfflineBottomSixInitialProfileV1,
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
        || profile.exact_regions.len() != 3
        || profile.occupancy.label != "seventh_hand_slot_occupancy"
    {
        return Err(error_v1(
            "offline_bottom_six_initial_profile",
            "profile identity, layout, or feature count is invalid",
        ));
    }
    let expected_labels = ["bottom_six_prompt", "cancel_only_control", "turn_one_label"];
    if profile
        .exact_regions
        .iter()
        .map(|region| region.label)
        .ne(expected_labels)
    {
        return Err(error_v1(
            "offline_bottom_six_initial_profile",
            "exact regions must use the canonical order and labels",
        ));
    }
    validate_region_v1(&profile.client_size_px, &profile.occupancy.rect)?;
    let occupancy_area = profile
        .occupancy
        .rect
        .width
        .checked_mul(profile.occupancy.rect.height)
        .ok_or_else(|| {
            error_v1(
                "offline_bottom_six_initial_profile",
                "occupancy region area overflows",
            )
        })?;
    if profile.occupancy.channel_threshold_exclusive == u8::MAX
        || profile.occupancy.minimum_bright_pixel_count == 0
        || profile.occupancy.minimum_bright_pixel_count > occupancy_area
    {
        return Err(error_v1(
            "offline_bottom_six_initial_profile",
            "occupancy threshold is invalid",
        ));
    }

    let mut hasher = Sha256::new();
    hasher.update(OFFLINE_BOTTOM_SIX_INITIAL_PROFILE_DOMAIN_V1);
    for part in [
        profile.profile_id.as_bytes(),
        profile.output_identity_sha256.as_bytes(),
        &profile.client_size_px.width.to_be_bytes(),
        &profile.client_size_px.height.to_be_bytes(),
    ] {
        update_hash_part_v1(&mut hasher, part);
    }
    for region in &profile.exact_regions {
        validate_region_v1(&profile.client_size_px, &region.rect)?;
        if !is_lower_sha256_v1(&region.expected_bgra8_sha256) {
            return Err(error_v1(
                "offline_bottom_six_initial_profile",
                "every exact region requires a lowercase SHA-256",
            ));
        }
        commit_region_v1(&mut hasher, region.label, &region.rect);
        update_hash_part_v1(&mut hasher, region.expected_bgra8_sha256.as_bytes());
    }
    commit_region_v1(
        &mut hasher,
        profile.occupancy.label,
        &profile.occupancy.rect,
    );
    update_hash_part_v1(
        &mut hasher,
        &[profile.occupancy.channel_threshold_exclusive],
    );
    update_hash_part_v1(
        &mut hasher,
        &profile.occupancy.minimum_bright_pixel_count.to_be_bytes(),
    );

    let mut rects: Vec<_> = profile
        .exact_regions
        .iter()
        .map(|region| &region.rect)
        .collect();
    rects.push(&profile.occupancy.rect);
    for (index, left) in rects.iter().enumerate() {
        for right in rects.iter().skip(index + 1) {
            if rects_overlap_v1(left, right) {
                return Err(error_v1(
                    "offline_bottom_six_initial_profile",
                    "profile regions must not overlap",
                ));
            }
        }
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn count_bright_pixels_v1(
    pixels: &[u8],
    size: &MtgoSizePxV1,
    rect: &MtgoRectPxV1,
    threshold_exclusive: u8,
) -> Result<u32, MtgoContractErrorV1> {
    validate_region_v1(size, rect)?;
    let width = usize::try_from(size.width)
        .map_err(|_| error_v1("offline_bottom_six_initial_pixels", "width overflow"))?;
    let mut count = 0_u32;
    for y in rect.y..rect.y + rect.height {
        for x in rect.x..rect.x + rect.width {
            let offset = usize::try_from(y)
                .ok()
                .and_then(|y| y.checked_mul(width))
                .and_then(|row| usize::try_from(x).ok().and_then(|x| row.checked_add(x)))
                .and_then(|pixel| pixel.checked_mul(4))
                .ok_or_else(|| {
                    error_v1("offline_bottom_six_initial_pixels", "pixel offset overflow")
                })?;
            let max_channel = pixels[offset..offset + 3]
                .iter()
                .copied()
                .max()
                .unwrap_or(0);
            if max_channel > threshold_exclusive {
                count = count.checked_add(1).ok_or_else(|| {
                    error_v1(
                        "offline_bottom_six_initial_pixels",
                        "occupancy count overflow",
                    )
                })?;
            }
        }
    }
    Ok(count)
}

fn commit_region_v1(hasher: &mut Sha256, label: &str, rect: &MtgoRectPxV1) {
    update_hash_part_v1(hasher, label.as_bytes());
    for value in [rect.x, rect.y, rect.width, rect.height] {
        update_hash_part_v1(hasher, &value.to_be_bytes());
    }
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
        .ok_or_else(|| error_v1("offline_bottom_six_initial_pixels", "pixel length overflow"))?;
    if pixels.len() != expected
        || format!("{:x}", Sha256::digest(pixels)) != checked.canonical_bgra8_sha256()
    {
        return Err(error_v1(
            "offline_bottom_six_initial_pixels",
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
            "offline_bottom_six_initial_region",
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
        assert_eq!(profile.exact_regions.len(), 3);
        assert_eq!(profile.occupancy.channel_threshold_exclusive, 48);
        assert_eq!(profile.occupancy.minimum_bright_pixel_count, 200);
        assert_eq!(
            validate_and_commit_profile_v1(&profile).unwrap(),
            "caafef8397e55e58f97ae24bca403390cb0d86a7ac2dd2fa23f30021f95479a8"
        );
    }

    #[test]
    fn structural_occupancy_is_card_art_independent() {
        let size = MtgoSizePxV1 {
            width: 8,
            height: 4,
        };
        let rect = MtgoRectPxV1 {
            x: 2,
            y: 1,
            width: 3,
            height: 2,
        };
        let mut pixels = vec![0_u8; 8 * 4 * 4];
        for (index, (b, g, r)) in [(49, 0, 0), (0, 75, 0), (0, 0, 200)]
            .into_iter()
            .enumerate()
        {
            let offset = (8 + 2 + index) * 4;
            pixels[offset..offset + 4].copy_from_slice(&[b, g, r, 255]);
        }
        assert_eq!(
            count_bright_pixels_v1(&pixels, &size, &rect, 48).unwrap(),
            3
        );
    }

    #[test]
    fn malformed_profile_fails_closed() {
        let mut profile = production_profile_v1();
        profile.occupancy.minimum_bright_pixel_count = 901;
        assert_eq!(
            validate_and_commit_profile_v1(&profile)
                .err()
                .unwrap()
                .code(),
            "offline_bottom_six_initial_profile"
        );

        let mut profile = production_profile_v1();
        profile.exact_regions[0].rect.width = u32::MAX;
        assert_eq!(
            validate_and_commit_profile_v1(&profile)
                .err()
                .unwrap()
                .code(),
            "offline_bottom_six_initial_region"
        );

        let mut profile = production_profile_v1();
        profile.occupancy.rect = profile.exact_regions[0].rect.clone();
        assert_eq!(
            validate_and_commit_profile_v1(&profile)
                .err()
                .unwrap()
                .code(),
            "offline_bottom_six_initial_profile"
        );
    }
}
