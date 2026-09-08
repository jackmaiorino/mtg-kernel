use crate::{
    hash_bgra_region_v1, CheckedUntrustedMtgoDxgiCaptureArtifactV1, MtgoContractErrorV1,
    MtgoDxgiCaptureRoleV2, MtgoRectPxV1, MtgoSizePxV1,
};
use sha2::{Digest, Sha256};

const PROFILE_ID_V1: &str = "freeform-solitaire-bottom-six-selection-state-20260810-v1";
const PROFILE_DOMAIN_V1: &[u8] = b"mtgo-offline-bottom-six-state-profile-v1";
const CANDIDATE_DOMAIN_V1: &[u8] = b"mtgo-offline-bottom-six-state-candidate-v1";
const OUTPUT_IDENTITY_V1: &str = "89c86876d12827c79ef4d746b9cd88c8decaf3e4fae33b41f6ab57c94bf222a6";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MtgoOfflineBottomSixStateClassificationV1 {
    Match,
    NoMatch,
}

#[derive(Clone)]
struct ExactRegionV1 {
    label: &'static str,
    rect: MtgoRectPxV1,
    expected_bgra8_sha256: String,
}

#[derive(Clone)]
struct OccupancyProbeV1 {
    hand_count: u8,
    rect: MtgoRectPxV1,
}

#[derive(Clone)]
struct BottomSixStateProfileV1 {
    profile_id: &'static str,
    client_size_px: MtgoSizePxV1,
    output_identity_sha256: String,
    exact_regions: Vec<ExactRegionV1>,
    controls_rect: MtgoRectPxV1,
    cancel_only_sha256: String,
    done_and_cancel_sha256: String,
    occupancy_channel_threshold_exclusive: u8,
    occupancy_minimum_bright_pixel_count: u32,
    occupancy_probes: Vec<OccupancyProbeV1>,
}

/// A checked-untrusted structural measurement of one reviewed MTGO London
/// bottom-six selection screen.
///
/// Exact prompt, Turn 1, and control templates are combined with seven
/// ordered hand-occupancy probes. A Match requires a thermometer pattern:
/// exactly `visible_hand_count` consecutive probes are occupied and every
/// later probe is empty. The sole one-card state must expose Done plus Cancel;
/// all earlier states must expose Cancel only. The result retains no pixels or
/// coordinates and grants no observation, scoring, or input authority.
///
/// ```compile_fail
/// use mtgo_blackbox_v1::CheckedUntrustedMtgoOfflineBottomSixStateCandidateV1;
/// fn pixel_escape(value: &CheckedUntrustedMtgoOfflineBottomSixStateCandidateV1) {
///     let _ = value.canonical_pixels_v1();
/// }
/// ```
///
/// ```compile_fail
/// use mtgo_blackbox_v1::CheckedUntrustedMtgoOfflineBottomSixStateCandidateV1;
/// fn coordinate_escape(value: &CheckedUntrustedMtgoOfflineBottomSixStateCandidateV1) {
///     let _ = value.occupancy_probe_rects_v1();
/// }
/// ```
pub struct CheckedUntrustedMtgoOfflineBottomSixStateCandidateV1 {
    classification: MtgoOfflineBottomSixStateClassificationV1,
    profile_commitment_sha256: String,
    source_manifest_sha256: String,
    source_frame_sha256: String,
    matched_exact_region_count: usize,
    observed_control_sha256: String,
    occupancy_bright_pixel_counts: Vec<u32>,
    visible_hand_count: Option<u8>,
    selected_count: Option<u8>,
    legal_action_count: Option<u8>,
    candidate_commitment_sha256: String,
}

impl CheckedUntrustedMtgoOfflineBottomSixStateCandidateV1 {
    pub fn classification(&self) -> MtgoOfflineBottomSixStateClassificationV1 {
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

    pub fn observed_control_sha256(&self) -> &str {
        &self.observed_control_sha256
    }

    pub fn occupancy_bright_pixel_counts(&self) -> &[u32] {
        &self.occupancy_bright_pixel_counts
    }

    pub fn required_bottom_count(&self) -> Option<u8> {
        (self.classification == MtgoOfflineBottomSixStateClassificationV1::Match).then_some(6)
    }

    pub fn visible_hand_count(&self) -> Option<u8> {
        self.visible_hand_count
    }

    pub fn selected_count(&self) -> Option<u8> {
        self.selected_count
    }

    pub fn done_visible(&self) -> Option<bool> {
        self.selected_count.map(|count| count == 6)
    }

    pub fn legal_action_count(&self) -> Option<u8> {
        self.legal_action_count
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

pub fn classify_untrusted_offline_bottom_six_state_candidate_v1(
    checked: &CheckedUntrustedMtgoDxgiCaptureArtifactV1,
    canonical_bgra8: &[u8],
) -> Result<CheckedUntrustedMtgoOfflineBottomSixStateCandidateV1, MtgoContractErrorV1> {
    let profile = production_profile_v1();
    let profile_commitment_sha256 = validate_and_commit_profile_v1(&profile)?;
    if checked.capture_role() != MtgoDxgiCaptureRoleV2::ActingPlayerSolitaire
        || checked.client_size_px() != &profile.client_size_px
        || checked.output_identity_sha256() != profile.output_identity_sha256
    {
        return Err(error_v1(
            "offline_bottom_six_state_source_layout",
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
    let observed_control_sha256 = hash_bgra_region_v1(
        canonical_bgra8,
        checked.client_size_px(),
        &profile.controls_rect,
    )?;
    let occupancy_bright_pixel_counts = profile
        .occupancy_probes
        .iter()
        .map(|probe| {
            count_bright_pixels_v1(
                canonical_bgra8,
                checked.client_size_px(),
                &probe.rect,
                profile.occupancy_channel_threshold_exclusive,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    let occupied: Vec<_> = occupancy_bright_pixel_counts
        .iter()
        .map(|count| *count >= profile.occupancy_minimum_bright_pixel_count)
        .collect();
    let structural_hand_count = thermometer_hand_count_v1(&occupied);
    let control_matches = structural_hand_count.is_some_and(|hand_count| {
        let expected = if hand_count == 1 {
            &profile.done_and_cancel_sha256
        } else {
            &profile.cancel_only_sha256
        };
        observed_control_sha256 == *expected
    });
    let classification = if matched_exact_region_count == profile.exact_regions.len()
        && structural_hand_count.is_some()
        && control_matches
    {
        MtgoOfflineBottomSixStateClassificationV1::Match
    } else {
        MtgoOfflineBottomSixStateClassificationV1::NoMatch
    };
    let visible_hand_count = (classification == MtgoOfflineBottomSixStateClassificationV1::Match)
        .then_some(structural_hand_count)
        .flatten();
    let selected_count = visible_hand_count.map(|count| 7 - count);
    let legal_action_count = visible_hand_count.map(|count| if count == 1 { 2 } else { count + 1 });

    let mut hasher = Sha256::new();
    hasher.update(CANDIDATE_DOMAIN_V1);
    for part in [
        profile_commitment_sha256.as_bytes(),
        checked.manifest_sha256().as_bytes(),
        checked.canonical_bgra8_sha256().as_bytes(),
        checked.output_identity_sha256().as_bytes(),
        match classification {
            MtgoOfflineBottomSixStateClassificationV1::Match => b"match".as_slice(),
            MtgoOfflineBottomSixStateClassificationV1::NoMatch => b"no_match".as_slice(),
        },
        &[u8::try_from(matched_exact_region_count).unwrap_or(u8::MAX)],
        &[visible_hand_count.unwrap_or(0)],
        &[selected_count.unwrap_or(u8::MAX)],
        &[legal_action_count.unwrap_or(0)],
    ] {
        update_hash_part_v1(&mut hasher, part);
    }
    for observed in &observed_exact_hashes {
        update_hash_part_v1(&mut hasher, observed.as_bytes());
    }
    update_hash_part_v1(&mut hasher, observed_control_sha256.as_bytes());
    for count in &occupancy_bright_pixel_counts {
        update_hash_part_v1(&mut hasher, &count.to_be_bytes());
    }

    Ok(CheckedUntrustedMtgoOfflineBottomSixStateCandidateV1 {
        classification,
        profile_commitment_sha256,
        source_manifest_sha256: checked.manifest_sha256().to_owned(),
        source_frame_sha256: checked.canonical_bgra8_sha256().to_owned(),
        matched_exact_region_count,
        observed_control_sha256,
        occupancy_bright_pixel_counts,
        visible_hand_count,
        selected_count,
        legal_action_count,
        candidate_commitment_sha256: format!("{:x}", hasher.finalize()),
    })
}

fn production_profile_v1() -> BottomSixStateProfileV1 {
    BottomSixStateProfileV1 {
        profile_id: PROFILE_ID_V1,
        client_size_px: MtgoSizePxV1 {
            width: 1_550,
            height: 925,
        },
        output_identity_sha256: OUTPUT_IDENTITY_V1.to_owned(),
        exact_regions: vec![
            ExactRegionV1 {
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
            ExactRegionV1 {
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
        controls_rect: MtgoRectPxV1 {
            x: 30,
            y: 175,
            width: 128,
            height: 28,
        },
        cancel_only_sha256: "a7a432dcc18049bdb15473230b86e90cb846bd9b37bf4f9bba34231dbda19314"
            .to_owned(),
        done_and_cancel_sha256: "58001493d751fdfee7b29bcbe8b05eed9a15255cb2e63892b1e58cbb22ad6935"
            .to_owned(),
        occupancy_channel_threshold_exclusive: 48,
        occupancy_minimum_bright_pixel_count: 900,
        occupancy_probes: [326, 454, 582, 710, 838, 966, 1_038]
            .into_iter()
            .enumerate()
            .map(|(index, x)| OccupancyProbeV1 {
                hand_count: u8::try_from(index + 1).unwrap_or(u8::MAX),
                rect: MtgoRectPxV1 {
                    x,
                    y: 740,
                    width: 108,
                    height: 20,
                },
            })
            .collect(),
    }
}

fn validate_and_commit_profile_v1(
    profile: &BottomSixStateProfileV1,
) -> Result<String, MtgoContractErrorV1> {
    if profile.profile_id.is_empty()
        || profile.profile_id.len() > 96
        || !profile
            .profile_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        || !is_lower_sha256_v1(&profile.output_identity_sha256)
        || !is_lower_sha256_v1(&profile.cancel_only_sha256)
        || !is_lower_sha256_v1(&profile.done_and_cancel_sha256)
        || profile.cancel_only_sha256 == profile.done_and_cancel_sha256
        || profile.exact_regions.len() != 2
        || profile.occupancy_probes.len() != 7
        || profile.occupancy_channel_threshold_exclusive == u8::MAX
    {
        return Err(error_v1(
            "offline_bottom_six_state_profile",
            "profile identity, hashes, or feature shape is invalid",
        ));
    }
    if profile
        .exact_regions
        .iter()
        .map(|region| region.label)
        .ne(["bottom_six_prompt", "turn_one_label"])
        || profile
            .occupancy_probes
            .iter()
            .map(|probe| probe.hand_count)
            .ne(1_u8..=7)
    {
        return Err(error_v1(
            "offline_bottom_six_state_profile",
            "profile features are not in canonical order",
        ));
    }
    validate_region_v1(&profile.client_size_px, &profile.controls_rect)?;
    let probe_area = profile.occupancy_probes[0]
        .rect
        .width
        .checked_mul(profile.occupancy_probes[0].rect.height)
        .ok_or_else(|| error_v1("offline_bottom_six_state_profile", "probe area overflow"))?;
    if profile.occupancy_minimum_bright_pixel_count == 0
        || profile.occupancy_minimum_bright_pixel_count > probe_area
    {
        return Err(error_v1(
            "offline_bottom_six_state_profile",
            "occupancy threshold is outside the probe area",
        ));
    }

    let mut hasher = Sha256::new();
    hasher.update(PROFILE_DOMAIN_V1);
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
                "offline_bottom_six_state_profile",
                "every exact region requires a lowercase SHA-256",
            ));
        }
        commit_region_v1(&mut hasher, region.label, &region.rect);
        update_hash_part_v1(&mut hasher, region.expected_bgra8_sha256.as_bytes());
    }
    commit_region_v1(&mut hasher, "bottoming_controls", &profile.controls_rect);
    update_hash_part_v1(&mut hasher, profile.cancel_only_sha256.as_bytes());
    update_hash_part_v1(&mut hasher, profile.done_and_cancel_sha256.as_bytes());
    update_hash_part_v1(
        &mut hasher,
        &[profile.occupancy_channel_threshold_exclusive],
    );
    update_hash_part_v1(
        &mut hasher,
        &profile.occupancy_minimum_bright_pixel_count.to_be_bytes(),
    );
    for probe in &profile.occupancy_probes {
        validate_region_v1(&profile.client_size_px, &probe.rect)?;
        if probe.rect.width != 108 || probe.rect.height != 20 || probe.rect.y != 740 {
            return Err(error_v1(
                "offline_bottom_six_state_profile",
                "occupancy probe geometry is not canonical",
            ));
        }
        update_hash_part_v1(&mut hasher, &[probe.hand_count]);
        commit_region_v1(&mut hasher, "hand_occupancy_probe", &probe.rect);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn thermometer_hand_count_v1(occupied: &[bool]) -> Option<u8> {
    if occupied.len() != 7 || !occupied[0] {
        return None;
    }
    let count = occupied.iter().take_while(|value| **value).count();
    if occupied[count..].iter().any(|value| *value) {
        return None;
    }
    u8::try_from(count)
        .ok()
        .filter(|count| (1..=7).contains(count))
}

fn count_bright_pixels_v1(
    pixels: &[u8],
    size: &MtgoSizePxV1,
    rect: &MtgoRectPxV1,
    threshold_exclusive: u8,
) -> Result<u32, MtgoContractErrorV1> {
    validate_region_v1(size, rect)?;
    let width = usize::try_from(size.width)
        .map_err(|_| error_v1("offline_bottom_six_state_pixels", "width overflow"))?;
    let mut count = 0_u32;
    for y in rect.y..rect.y + rect.height {
        for x in rect.x..rect.x + rect.width {
            let offset = usize::try_from(y)
                .ok()
                .and_then(|y| y.checked_mul(width))
                .and_then(|row| usize::try_from(x).ok().and_then(|x| row.checked_add(x)))
                .and_then(|pixel| pixel.checked_mul(4))
                .ok_or_else(|| {
                    error_v1("offline_bottom_six_state_pixels", "pixel offset overflow")
                })?;
            let max_channel = pixels[offset..offset + 3]
                .iter()
                .copied()
                .max()
                .unwrap_or(0);
            if max_channel > threshold_exclusive {
                count = count.checked_add(1).ok_or_else(|| {
                    error_v1(
                        "offline_bottom_six_state_pixels",
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
        .ok_or_else(|| error_v1("offline_bottom_six_state_pixels", "pixel length overflow"))?;
    if pixels.len() != expected
        || format!("{:x}", Sha256::digest(pixels)) != checked.canonical_bgra8_sha256()
    {
        return Err(error_v1(
            "offline_bottom_six_state_pixels",
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
            "offline_bottom_six_state_region",
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
    fn production_profile_is_canonical_and_committed() {
        let profile = production_profile_v1();
        assert_eq!(profile.occupancy_probes.len(), 7);
        assert_eq!(profile.occupancy_minimum_bright_pixel_count, 900);
        let commitment = validate_and_commit_profile_v1(&profile).unwrap();
        assert_eq!(
            commitment,
            "72a52b5f1cca1785f7f923780fa4b0a4bf2724f3dcd4c4529df96c0c1d41763b"
        );
    }

    #[test]
    fn thermometer_requires_one_nonempty_prefix() {
        assert_eq!(
            thermometer_hand_count_v1(&[true, true, true, false, false, false, false]),
            Some(3)
        );
        assert_eq!(
            thermometer_hand_count_v1(&[true, true, true, true, true, true, true]),
            Some(7)
        );
        assert_eq!(
            thermometer_hand_count_v1(&[true, false, true, false, false, false, false]),
            None
        );
        assert_eq!(
            thermometer_hand_count_v1(&[false, false, false, false, false, false, false]),
            None
        );
    }

    #[test]
    fn malformed_profile_fails_closed() {
        let mut profile = production_profile_v1();
        profile.occupancy_probes.swap(0, 1);
        assert_eq!(
            validate_and_commit_profile_v1(&profile)
                .err()
                .unwrap()
                .code(),
            "offline_bottom_six_state_profile"
        );

        let mut profile = production_profile_v1();
        profile.occupancy_probes[0].rect.width = u32::MAX;
        assert_eq!(
            validate_and_commit_profile_v1(&profile)
                .err()
                .unwrap()
                .code(),
            "offline_bottom_six_state_profile"
        );

        let mut profile = production_profile_v1();
        profile.occupancy_minimum_bright_pixel_count = 2_161;
        assert_eq!(
            validate_and_commit_profile_v1(&profile)
                .err()
                .unwrap()
                .code(),
            "offline_bottom_six_state_profile"
        );
    }
}
