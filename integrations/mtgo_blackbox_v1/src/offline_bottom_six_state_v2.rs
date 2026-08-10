use crate::{
    classify_untrusted_offline_bottom_six_state_candidate_v1, hash_bgra_region_v1,
    CheckedUntrustedMtgoDxgiCaptureArtifactV1, MtgoContractErrorV1, MtgoRectPxV1, MtgoSizePxV1,
};
use sha2::{Digest, Sha256};

const PROFILE_ID_V2: &str = "freeform-solitaire-bottom-six-selection-state-binary-ink-20260810-v2";
const PROFILE_DOMAIN_V2: &[u8] = b"mtgo-offline-bottom-six-state-profile-v2";
const CANDIDATE_DOMAIN_V2: &[u8] = b"mtgo-offline-bottom-six-state-candidate-v2";
const PROMPT_INK_DOMAIN_V1: &[u8] = b"mtgo-offline-bottom-six-prompt-ink-mask-v1";
const PROMPT_INK_SUM_THRESHOLD_V1: u16 = 3 * 128;
const EXPECTED_PROMPT_INK_SHA256_V2: &str =
    "0f2bf3a465a3b2537f56dd59475b7ef916af3996e99ba1041d675ce11fb9d847";
const EXPECTED_TURN_ONE_SHA256_V1: &str =
    "ef7e60faba58a8d5f0b5d58304f7e83a764b4f9845d1987e8db90927dd3a9471";
const CANCEL_ONLY_SHA256_V1: &str =
    "a7a432dcc18049bdb15473230b86e90cb846bd9b37bf4f9bba34231dbda19314";
const DONE_AND_CANCEL_SHA256_V1: &str =
    "58001493d751fdfee7b29bcbe8b05eed9a15255cb2e63892b1e58cbb22ad6935";
const OCCUPANCY_MINIMUM_BRIGHT_PIXEL_COUNT_V1: u32 = 900;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MtgoOfflineBottomSixStateClassificationV2 {
    Match,
    NoMatch,
}

/// A checked-untrusted offline measurement of the reviewed MTGO bottom-six
/// screen using an exact binary prompt-ink mask.
///
/// V2 preserves the v1 raw-frame, role, layout, control, and hand-occupancy
/// measurements. It changes only the instruction-text feature. A fixed BGR
/// sum threshold removes same-side grayscale antialias intensity variation,
/// while any changed mask bit fails closed. The result retains no pixels or
/// coordinates and grants no observation, scoring, or input authority.
pub struct CheckedUntrustedMtgoOfflineBottomSixStateCandidateV2 {
    classification: MtgoOfflineBottomSixStateClassificationV2,
    profile_commitment_sha256: String,
    source_manifest_sha256: String,
    source_frame_sha256: String,
    legacy_candidate_commitment_sha256: String,
    observed_prompt_ink_sha256: String,
    prompt_matches: bool,
    turn_one_matches: bool,
    observed_control_sha256: String,
    occupancy_bright_pixel_counts: Vec<u32>,
    visible_hand_count: Option<u8>,
    selected_count: Option<u8>,
    legal_action_count: Option<u8>,
    candidate_commitment_sha256: String,
}

impl CheckedUntrustedMtgoOfflineBottomSixStateCandidateV2 {
    pub fn classification(&self) -> MtgoOfflineBottomSixStateClassificationV2 {
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

    pub fn legacy_candidate_commitment_sha256(&self) -> &str {
        &self.legacy_candidate_commitment_sha256
    }

    pub fn observed_prompt_ink_sha256(&self) -> &str {
        &self.observed_prompt_ink_sha256
    }

    pub fn prompt_matches(&self) -> bool {
        self.prompt_matches
    }

    pub fn turn_one_matches(&self) -> bool {
        self.turn_one_matches
    }

    pub fn observed_control_sha256(&self) -> &str {
        &self.observed_control_sha256
    }

    pub fn occupancy_bright_pixel_counts(&self) -> &[u32] {
        &self.occupancy_bright_pixel_counts
    }

    pub fn required_bottom_count(&self) -> Option<u8> {
        (self.classification == MtgoOfflineBottomSixStateClassificationV2::Match).then_some(6)
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

pub fn classify_untrusted_offline_bottom_six_state_candidate_v2(
    checked: &CheckedUntrustedMtgoDxgiCaptureArtifactV1,
    canonical_bgra8: &[u8],
) -> Result<CheckedUntrustedMtgoOfflineBottomSixStateCandidateV2, MtgoContractErrorV1> {
    let legacy =
        classify_untrusted_offline_bottom_six_state_candidate_v1(checked, canonical_bgra8)?;
    let prompt_rect = prompt_rect_v2();
    let turn_one_rect = turn_one_rect_v2();
    let profile_commitment_sha256 = profile_commitment_v2(
        legacy.profile_commitment_sha256(),
        &prompt_rect,
        &turn_one_rect,
    )?;
    let observed_prompt_ink_sha256 =
        hash_prompt_binary_ink_v2(canonical_bgra8, checked.client_size_px(), &prompt_rect)?;
    let observed_turn_one_sha256 =
        hash_bgra_region_v1(canonical_bgra8, checked.client_size_px(), &turn_one_rect)?;
    let prompt_matches = observed_prompt_ink_sha256 == EXPECTED_PROMPT_INK_SHA256_V2;
    let turn_one_matches = observed_turn_one_sha256 == EXPECTED_TURN_ONE_SHA256_V1;
    let occupancy_bright_pixel_counts = legacy.occupancy_bright_pixel_counts().to_vec();
    let occupied: Vec<_> = occupancy_bright_pixel_counts
        .iter()
        .map(|count| *count >= OCCUPANCY_MINIMUM_BRIGHT_PIXEL_COUNT_V1)
        .collect();
    let structural_hand_count = thermometer_hand_count_v2(&occupied);
    let control_matches = structural_hand_count.is_some_and(|hand_count| {
        legacy.observed_control_sha256()
            == if hand_count == 1 {
                DONE_AND_CANCEL_SHA256_V1
            } else {
                CANCEL_ONLY_SHA256_V1
            }
    });
    let classification =
        if prompt_matches && turn_one_matches && structural_hand_count.is_some() && control_matches
        {
            MtgoOfflineBottomSixStateClassificationV2::Match
        } else {
            MtgoOfflineBottomSixStateClassificationV2::NoMatch
        };
    let visible_hand_count = (classification == MtgoOfflineBottomSixStateClassificationV2::Match)
        .then_some(structural_hand_count)
        .flatten();
    let selected_count = visible_hand_count.map(|count| 7 - count);
    let legal_action_count = visible_hand_count.map(|count| if count == 1 { 2 } else { count + 1 });

    let mut hasher = Sha256::new();
    hasher.update(CANDIDATE_DOMAIN_V2);
    for part in [
        profile_commitment_sha256.as_bytes(),
        legacy.source_manifest_sha256().as_bytes(),
        legacy.source_frame_sha256().as_bytes(),
        legacy.candidate_commitment_sha256().as_bytes(),
        observed_prompt_ink_sha256.as_bytes(),
        observed_turn_one_sha256.as_bytes(),
        legacy.observed_control_sha256().as_bytes(),
        match classification {
            MtgoOfflineBottomSixStateClassificationV2::Match => b"match".as_slice(),
            MtgoOfflineBottomSixStateClassificationV2::NoMatch => b"no_match".as_slice(),
        },
        &[u8::from(prompt_matches)],
        &[u8::from(turn_one_matches)],
        &[visible_hand_count.unwrap_or(0)],
        &[selected_count.unwrap_or(u8::MAX)],
        &[legal_action_count.unwrap_or(0)],
    ] {
        update_hash_part_v2(&mut hasher, part);
    }
    for count in &occupancy_bright_pixel_counts {
        update_hash_part_v2(&mut hasher, &count.to_be_bytes());
    }

    Ok(CheckedUntrustedMtgoOfflineBottomSixStateCandidateV2 {
        classification,
        profile_commitment_sha256,
        source_manifest_sha256: legacy.source_manifest_sha256().to_owned(),
        source_frame_sha256: legacy.source_frame_sha256().to_owned(),
        legacy_candidate_commitment_sha256: legacy.candidate_commitment_sha256().to_owned(),
        observed_prompt_ink_sha256,
        prompt_matches,
        turn_one_matches,
        observed_control_sha256: legacy.observed_control_sha256().to_owned(),
        occupancy_bright_pixel_counts,
        visible_hand_count,
        selected_count,
        legal_action_count,
        candidate_commitment_sha256: format!("{:x}", hasher.finalize()),
    })
}

fn prompt_rect_v2() -> MtgoRectPxV1 {
    MtgoRectPxV1 {
        x: 28,
        y: 52,
        width: 160,
        height: 103,
    }
}

fn turn_one_rect_v2() -> MtgoRectPxV1 {
    MtgoRectPxV1 {
        x: 311,
        y: 696,
        width: 100,
        height: 30,
    }
}

fn profile_commitment_v2(
    legacy_profile_commitment_sha256: &str,
    prompt_rect: &MtgoRectPxV1,
    turn_one_rect: &MtgoRectPxV1,
) -> Result<String, MtgoContractErrorV1> {
    if !is_lower_sha256_v2(legacy_profile_commitment_sha256)
        || !is_lower_sha256_v2(EXPECTED_PROMPT_INK_SHA256_V2)
        || !is_lower_sha256_v2(EXPECTED_TURN_ONE_SHA256_V1)
        || !is_lower_sha256_v2(CANCEL_ONLY_SHA256_V1)
        || !is_lower_sha256_v2(DONE_AND_CANCEL_SHA256_V1)
        || CANCEL_ONLY_SHA256_V1 == DONE_AND_CANCEL_SHA256_V1
    {
        return Err(error_v2(
            "offline_bottom_six_state_v2_profile",
            "profile digests are invalid",
        ));
    }
    let mut hasher = Sha256::new();
    hasher.update(PROFILE_DOMAIN_V2);
    for part in [
        PROFILE_ID_V2.as_bytes(),
        legacy_profile_commitment_sha256.as_bytes(),
        PROMPT_INK_DOMAIN_V1,
        PROMPT_INK_SUM_THRESHOLD_V1.to_be_bytes().as_slice(),
        EXPECTED_PROMPT_INK_SHA256_V2.as_bytes(),
        EXPECTED_TURN_ONE_SHA256_V1.as_bytes(),
        CANCEL_ONLY_SHA256_V1.as_bytes(),
        DONE_AND_CANCEL_SHA256_V1.as_bytes(),
        OCCUPANCY_MINIMUM_BRIGHT_PIXEL_COUNT_V1
            .to_be_bytes()
            .as_slice(),
    ] {
        update_hash_part_v2(&mut hasher, part);
    }
    for (label, rect) in [("prompt", prompt_rect), ("turn_one", turn_one_rect)] {
        update_hash_part_v2(&mut hasher, label.as_bytes());
        for value in [rect.x, rect.y, rect.width, rect.height] {
            update_hash_part_v2(&mut hasher, &value.to_be_bytes());
        }
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn hash_prompt_binary_ink_v2(
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
            error_v2(
                "offline_bottom_six_state_v2_pixels",
                "pixel length overflow",
            )
        })?;
    let right = rect.x.checked_add(rect.width);
    let bottom = rect.y.checked_add(rect.height);
    if pixels.len() != expected_len
        || rect.width == 0
        || rect.height == 0
        || right.is_none_or(|right| right > size.width)
        || bottom.is_none_or(|bottom| bottom > size.height)
    {
        return Err(error_v2(
            "offline_bottom_six_state_v2_pixels",
            "prompt pixels or geometry are invalid",
        ));
    }
    let frame_width = usize::try_from(size.width)
        .map_err(|_| error_v2("offline_bottom_six_state_v2_pixels", "frame width overflow"))?;
    let mask_len = usize::try_from(u64::from(rect.width) * u64::from(rect.height))
        .map_err(|_| error_v2("offline_bottom_six_state_v2_pixels", "mask length overflow"))?;
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
                    error_v2(
                        "offline_bottom_six_state_v2_pixels",
                        "pixel offset overflow",
                    )
                })?;
            let bgr_sum = u16::from(pixels[offset])
                + u16::from(pixels[offset + 1])
                + u16::from(pixels[offset + 2]);
            mask.push(u8::from(bgr_sum < PROMPT_INK_SUM_THRESHOLD_V1));
        }
    }
    let mut hasher = Sha256::new();
    hasher.update(PROMPT_INK_DOMAIN_V1);
    for part in [
        PROMPT_INK_SUM_THRESHOLD_V1.to_be_bytes().as_slice(),
        rect.width.to_be_bytes().as_slice(),
        rect.height.to_be_bytes().as_slice(),
        mask.as_slice(),
    ] {
        update_hash_part_v2(&mut hasher, part);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn thermometer_hand_count_v2(occupied: &[bool]) -> Option<u8> {
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

fn is_lower_sha256_v2(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

fn update_hash_part_v2(hasher: &mut Sha256, part: &[u8]) {
    hasher.update((part.len() as u64).to_be_bytes());
    hasher.update(part);
}

fn error_v2(code: &'static str, detail: impl Into<String>) -> MtgoContractErrorV1 {
    MtgoContractErrorV1::new(code, detail)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binary_ink_hash_ignores_same_side_antialias_and_rejects_threshold_crossing() {
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
            hash_prompt_binary_ink_v2(&dark, &size, &rect).unwrap(),
            hash_prompt_binary_ink_v2(&same_side, &size, &rect).unwrap()
        );

        let mut crossing = dark.clone();
        crossing[..4].copy_from_slice(&[128, 128, 128, 255]);
        assert_ne!(
            hash_prompt_binary_ink_v2(&dark, &size, &rect).unwrap(),
            hash_prompt_binary_ink_v2(&crossing, &size, &rect).unwrap()
        );
    }

    #[test]
    fn thermometer_requires_one_nonempty_prefix() {
        assert_eq!(
            thermometer_hand_count_v2(&[true, true, true, false, false, false, false]),
            Some(3)
        );
        assert_eq!(
            thermometer_hand_count_v2(&[true, false, true, false, false, false, false]),
            None
        );
        assert_eq!(
            thermometer_hand_count_v2(&[false, false, false, false, false, false, false]),
            None
        );
    }

    #[test]
    fn production_profile_is_committed() {
        let commitment = profile_commitment_v2(
            "72a52b5f1cca1785f7f923780fa4b0a4bf2724f3dcd4c4529df96c0c1d41763b",
            &prompt_rect_v2(),
            &turn_one_rect_v2(),
        )
        .unwrap();
        assert_eq!(
            commitment,
            "e9004b0f808271bcb2cb885d7af5173459830e9980cf9bcc4501c664d08a8be2"
        );
    }

    #[test]
    fn v1_classification_tag_remains_separate() {
        assert_ne!(
            format!(
                "{:?}",
                crate::MtgoOfflineBottomSixStateClassificationV1::Match
            ),
            format!("{:?}", MtgoOfflineBottomSixStateClassificationV2::NoMatch)
        );
    }
}
