use crate::{
    classify_untrusted_offline_bottom_six_state_candidate_v2,
    CheckedUntrustedMtgoDxgiCaptureArtifactV1, MtgoContractErrorV1, MtgoRectPxV1, MtgoSizePxV1,
};
use sha2::{Digest, Sha256};

const PROFILE_ID_V3: &str =
    "freeform-solitaire-bottom-six-selection-state-binary-controls-20260810-v3";
const PROFILE_DOMAIN_V3: &[u8] = b"mtgo-offline-bottom-six-state-profile-v3";
const CANDIDATE_DOMAIN_V3: &[u8] = b"mtgo-offline-bottom-six-state-candidate-v3";
const CONTROL_INK_DOMAIN_V1: &[u8] = b"mtgo-offline-bottom-six-control-ink-mask-v1";
const CONTROL_INK_SUM_THRESHOLD_V1: u16 = 3 * 128;
const CANCEL_ONLY_INK_SHA256_V1: &str =
    "f2deda1e14f50f7f841509f5192422db1d324037f80952e03120c42078e11136";
const DONE_AND_CANCEL_INK_SHA256_V1: &str =
    "42e612b6cc4aa1b7ff0a7c9d7d32a245980421b6030cfe462ad455dced36610e";
const OCCUPANCY_MINIMUM_BRIGHT_PIXEL_COUNT_V1: u32 = 900;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MtgoOfflineBottomSixStateClassificationV3 {
    Match,
    NoMatch,
}

/// Checked-untrusted bottom-six state with binary prompt and control masks.
///
/// V3 keeps the v2 prompt, Turn 1, and structural hand checks. It replaces
/// only the raw Cancel and Done control pixels with an exact binary ink mask.
/// This removes same-side grayscale intensity drift while any changed mask bit
/// still fails closed. The result grants no live, semantic, scoring, or input
/// authority.
pub struct CheckedUntrustedMtgoOfflineBottomSixStateCandidateV3 {
    classification: MtgoOfflineBottomSixStateClassificationV3,
    profile_commitment_sha256: String,
    source_manifest_sha256: String,
    source_frame_sha256: String,
    predecessor_candidate_commitment_sha256: String,
    observed_control_ink_sha256: String,
    prompt_matches: bool,
    turn_one_matches: bool,
    control_matches: bool,
    occupancy_bright_pixel_counts: Vec<u32>,
    visible_hand_count: Option<u8>,
    selected_count: Option<u8>,
    legal_action_count: Option<u8>,
    candidate_commitment_sha256: String,
}

impl CheckedUntrustedMtgoOfflineBottomSixStateCandidateV3 {
    pub fn classification(&self) -> MtgoOfflineBottomSixStateClassificationV3 {
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

    pub fn predecessor_candidate_commitment_sha256(&self) -> &str {
        &self.predecessor_candidate_commitment_sha256
    }

    pub fn observed_control_ink_sha256(&self) -> &str {
        &self.observed_control_ink_sha256
    }

    pub fn prompt_matches(&self) -> bool {
        self.prompt_matches
    }

    pub fn turn_one_matches(&self) -> bool {
        self.turn_one_matches
    }

    pub fn control_matches(&self) -> bool {
        self.control_matches
    }

    pub fn occupancy_bright_pixel_counts(&self) -> &[u32] {
        &self.occupancy_bright_pixel_counts
    }

    pub fn required_bottom_count(&self) -> Option<u8> {
        (self.classification == MtgoOfflineBottomSixStateClassificationV3::Match).then_some(6)
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

pub fn classify_untrusted_offline_bottom_six_state_candidate_v3(
    checked: &CheckedUntrustedMtgoDxgiCaptureArtifactV1,
    canonical_bgra8: &[u8],
) -> Result<CheckedUntrustedMtgoOfflineBottomSixStateCandidateV3, MtgoContractErrorV1> {
    let predecessor =
        classify_untrusted_offline_bottom_six_state_candidate_v2(checked, canonical_bgra8)?;
    let controls_rect = controls_rect_v3();
    let profile_commitment_sha256 =
        profile_commitment_v3(predecessor.profile_commitment_sha256(), &controls_rect)?;
    let observed_control_ink_sha256 =
        hash_control_binary_ink_v3(canonical_bgra8, checked.client_size_px(), &controls_rect)?;
    let occupancy_bright_pixel_counts = predecessor.occupancy_bright_pixel_counts().to_vec();
    let occupied: Vec<_> = occupancy_bright_pixel_counts
        .iter()
        .map(|count| *count >= OCCUPANCY_MINIMUM_BRIGHT_PIXEL_COUNT_V1)
        .collect();
    let structural_hand_count = thermometer_hand_count_v3(&occupied);
    let control_matches = structural_hand_count.is_some_and(|hand_count| {
        observed_control_ink_sha256
            == if hand_count == 1 {
                DONE_AND_CANCEL_INK_SHA256_V1
            } else {
                CANCEL_ONLY_INK_SHA256_V1
            }
    });
    let classification = if predecessor.prompt_matches()
        && predecessor.turn_one_matches()
        && structural_hand_count.is_some()
        && control_matches
    {
        MtgoOfflineBottomSixStateClassificationV3::Match
    } else {
        MtgoOfflineBottomSixStateClassificationV3::NoMatch
    };
    let visible_hand_count = (classification == MtgoOfflineBottomSixStateClassificationV3::Match)
        .then_some(structural_hand_count)
        .flatten();
    let selected_count = visible_hand_count.map(|count| 7 - count);
    let legal_action_count = visible_hand_count.map(|count| if count == 1 { 2 } else { count + 1 });

    let mut hasher = Sha256::new();
    hasher.update(CANDIDATE_DOMAIN_V3);
    for part in [
        profile_commitment_sha256.as_bytes(),
        predecessor.source_manifest_sha256().as_bytes(),
        predecessor.source_frame_sha256().as_bytes(),
        predecessor.candidate_commitment_sha256().as_bytes(),
        observed_control_ink_sha256.as_bytes(),
        match classification {
            MtgoOfflineBottomSixStateClassificationV3::Match => b"match".as_slice(),
            MtgoOfflineBottomSixStateClassificationV3::NoMatch => b"no_match".as_slice(),
        },
        &[u8::from(predecessor.prompt_matches())],
        &[u8::from(predecessor.turn_one_matches())],
        &[u8::from(control_matches)],
        &[visible_hand_count.unwrap_or(0)],
        &[selected_count.unwrap_or(u8::MAX)],
        &[legal_action_count.unwrap_or(0)],
    ] {
        update_hash_part_v3(&mut hasher, part);
    }
    for count in &occupancy_bright_pixel_counts {
        update_hash_part_v3(&mut hasher, &count.to_be_bytes());
    }

    Ok(CheckedUntrustedMtgoOfflineBottomSixStateCandidateV3 {
        classification,
        profile_commitment_sha256,
        source_manifest_sha256: predecessor.source_manifest_sha256().to_owned(),
        source_frame_sha256: predecessor.source_frame_sha256().to_owned(),
        predecessor_candidate_commitment_sha256: predecessor
            .candidate_commitment_sha256()
            .to_owned(),
        observed_control_ink_sha256,
        prompt_matches: predecessor.prompt_matches(),
        turn_one_matches: predecessor.turn_one_matches(),
        control_matches,
        occupancy_bright_pixel_counts,
        visible_hand_count,
        selected_count,
        legal_action_count,
        candidate_commitment_sha256: format!("{:x}", hasher.finalize()),
    })
}

fn controls_rect_v3() -> MtgoRectPxV1 {
    MtgoRectPxV1 {
        x: 30,
        y: 175,
        width: 128,
        height: 28,
    }
}

fn profile_commitment_v3(
    predecessor_profile_commitment_sha256: &str,
    controls_rect: &MtgoRectPxV1,
) -> Result<String, MtgoContractErrorV1> {
    if !is_lower_sha256_v3(predecessor_profile_commitment_sha256)
        || !is_lower_sha256_v3(CANCEL_ONLY_INK_SHA256_V1)
        || !is_lower_sha256_v3(DONE_AND_CANCEL_INK_SHA256_V1)
        || CANCEL_ONLY_INK_SHA256_V1 == DONE_AND_CANCEL_INK_SHA256_V1
    {
        return Err(error_v3(
            "offline_bottom_six_state_v3_profile",
            "profile commitments must be distinct lowercase SHA-256 values",
        ));
    }
    let mut hasher = Sha256::new();
    hasher.update(PROFILE_DOMAIN_V3);
    for part in [
        PROFILE_ID_V3.as_bytes(),
        predecessor_profile_commitment_sha256.as_bytes(),
        CONTROL_INK_DOMAIN_V1,
        CONTROL_INK_SUM_THRESHOLD_V1.to_be_bytes().as_slice(),
        controls_rect.x.to_be_bytes().as_slice(),
        controls_rect.y.to_be_bytes().as_slice(),
        controls_rect.width.to_be_bytes().as_slice(),
        controls_rect.height.to_be_bytes().as_slice(),
        CANCEL_ONLY_INK_SHA256_V1.as_bytes(),
        DONE_AND_CANCEL_INK_SHA256_V1.as_bytes(),
        OCCUPANCY_MINIMUM_BRIGHT_PIXEL_COUNT_V1
            .to_be_bytes()
            .as_slice(),
    ] {
        update_hash_part_v3(&mut hasher, part);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn hash_control_binary_ink_v3(
    pixels: &[u8],
    size: &MtgoSizePxV1,
    rect: &MtgoRectPxV1,
) -> Result<String, MtgoContractErrorV1> {
    let expected_len = usize::try_from(size.width)
        .ok()
        .and_then(|width| {
            usize::try_from(size.height)
                .ok()
                .and_then(|height| width.checked_mul(height))
        })
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or_else(|| {
            error_v3(
                "offline_bottom_six_state_v3_pixels",
                "pixel length overflow",
            )
        })?;
    if pixels.len() != expected_len
        || rect.width == 0
        || rect.height == 0
        || rect
            .x
            .checked_add(rect.width)
            .is_none_or(|right| right > size.width)
        || rect
            .y
            .checked_add(rect.height)
            .is_none_or(|bottom| bottom > size.height)
    {
        return Err(error_v3(
            "offline_bottom_six_state_v3_pixels",
            "control pixels or geometry are invalid",
        ));
    }
    let frame_width = usize::try_from(size.width)
        .map_err(|_| error_v3("offline_bottom_six_state_v3_pixels", "frame width overflow"))?;
    let mask_len = usize::try_from(u64::from(rect.width) * u64::from(rect.height))
        .map_err(|_| error_v3("offline_bottom_six_state_v3_pixels", "mask length overflow"))?;
    let mut mask = Vec::with_capacity(mask_len);
    for row in 0..rect.height {
        for column in 0..rect.width {
            let offset = usize::try_from(rect.y + row)
                .ok()
                .and_then(|y| y.checked_mul(frame_width))
                .and_then(|base| {
                    usize::try_from(rect.x + column)
                        .ok()
                        .and_then(|x| base.checked_add(x))
                })
                .and_then(|pixel| pixel.checked_mul(4))
                .ok_or_else(|| {
                    error_v3(
                        "offline_bottom_six_state_v3_pixels",
                        "pixel offset overflow",
                    )
                })?;
            let bgr_sum = u16::from(pixels[offset])
                + u16::from(pixels[offset + 1])
                + u16::from(pixels[offset + 2]);
            mask.push(u8::from(bgr_sum < CONTROL_INK_SUM_THRESHOLD_V1));
        }
    }
    let mut hasher = Sha256::new();
    hasher.update(CONTROL_INK_DOMAIN_V1);
    for part in [
        CONTROL_INK_SUM_THRESHOLD_V1.to_be_bytes().as_slice(),
        rect.width.to_be_bytes().as_slice(),
        rect.height.to_be_bytes().as_slice(),
        mask.as_slice(),
    ] {
        update_hash_part_v3(&mut hasher, part);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn thermometer_hand_count_v3(occupied: &[bool]) -> Option<u8> {
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

fn is_lower_sha256_v3(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

fn update_hash_part_v3(hasher: &mut Sha256, part: &[u8]) {
    hasher.update((part.len() as u64).to_be_bytes());
    hasher.update(part);
}

fn error_v3(code: &'static str, detail: impl Into<String>) -> MtgoContractErrorV1 {
    MtgoContractErrorV1::new(code, detail)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn control_ink_hash_ignores_same_side_intensity_and_rejects_mask_changes() {
        let size = MtgoSizePxV1 {
            width: 2,
            height: 1,
        };
        let rect = MtgoRectPxV1 {
            x: 0,
            y: 0,
            width: 2,
            height: 1,
        };
        let dark = vec![20, 20, 20, 255, 230, 230, 230, 255];
        let same_side = vec![90, 90, 90, 255, 180, 180, 180, 255];
        assert_eq!(
            hash_control_binary_ink_v3(&dark, &size, &rect).unwrap(),
            hash_control_binary_ink_v3(&same_side, &size, &rect).unwrap()
        );
        let mut crossing = dark.clone();
        crossing[..4].copy_from_slice(&[128, 128, 128, 255]);
        assert_ne!(
            hash_control_binary_ink_v3(&dark, &size, &rect).unwrap(),
            hash_control_binary_ink_v3(&crossing, &size, &rect).unwrap()
        );
    }

    #[test]
    fn thermometer_rejects_gaps() {
        assert_eq!(
            thermometer_hand_count_v3(&[true, true, false, false, false, false, false]),
            Some(2)
        );
        assert_eq!(
            thermometer_hand_count_v3(&[true, false, true, false, false, false, false]),
            None
        );
    }
}
