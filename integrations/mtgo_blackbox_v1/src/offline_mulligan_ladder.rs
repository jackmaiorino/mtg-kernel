use crate::{
    CheckedUntrustedMtgoDxgiCaptureArtifactV1, MtgoContractErrorV1, MtgoDxgiCaptureRoleV2,
    MtgoPregameActionSemanticV1, MtgoRectPxV1, MtgoSizePxV1,
};
use sha2::{Digest, Sha256};

const OFFLINE_MULLIGAN_LADDER_PROFILE_SET_ID_V1: &str =
    "freeform-solitaire-london-mulligan-ladder-binary-ink-20260810-v2";
const OFFLINE_MULLIGAN_LADDER_PROFILE_DOMAIN_V1: &[u8] =
    b"mtgo-offline-mulligan-ladder-profile-set-v2";
const OFFLINE_MULLIGAN_LADDER_CANDIDATE_DOMAIN_V1: &[u8] =
    b"mtgo-offline-mulligan-ladder-candidate-v2";
const OFFLINE_MULLIGAN_PROMPT_INK_MASK_DOMAIN_V1: &[u8] =
    b"mtgo-offline-mulligan-prompt-ink-mask-v1";
const OFFLINE_MULLIGAN_LADDER_OUTPUT_IDENTITY_V1: &str =
    "89c86876d12827c79ef4d746b9cd88c8decaf3e4fae33b41f6ab57c94bf222a6";
const OFFLINE_MULLIGAN_LADDER_WIDTH_V1: u32 = 1550;
const OFFLINE_MULLIGAN_LADDER_HEIGHT_V1: u32 = 925;
const OFFLINE_MULLIGAN_PROMPT_INK_SUM_THRESHOLD_V1: u16 = 3 * 128;
const OFFLINE_MULLIGAN_LADDER_PROFILE_SET_ID_V2: &str =
    "freeform-solitaire-london-mulligan-ladder-binary-ink-20260810-v3";
const OFFLINE_MULLIGAN_LADDER_PROFILE_DOMAIN_V2: &[u8] =
    b"mtgo-offline-mulligan-ladder-profile-set-v3";
const OFFLINE_MULLIGAN_LADDER_CANDIDATE_DOMAIN_V2: &[u8] =
    b"mtgo-offline-mulligan-ladder-candidate-v3";
const OFFLINE_MULLIGAN_PROMPT_INK_MASK_DOMAIN_V2: &[u8] =
    b"mtgo-offline-mulligan-prompt-ink-mask-v2";
const OFFLINE_MULLIGAN_PROMPT_INK_SUM_THRESHOLD_V2: u16 = 182;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MtgoOfflineMulliganLadderClassificationV1 {
    Match,
    NoMatch,
    Ambiguous,
}

#[derive(Clone)]
struct OfflineMulliganPromptProfileV1 {
    prospective_keep_size: u8,
    prompt_rect: MtgoRectPxV1,
    expected_binary_ink_sha256: String,
}

#[derive(Clone)]
struct OfflineMulliganLadderProfileSetV1 {
    profile_set_id: &'static str,
    profile_domain: &'static [u8],
    candidate_domain: &'static [u8],
    prompt_mask_domain: &'static [u8],
    prompt_ink_sum_threshold: u16,
    client_size_px: MtgoSizePxV1,
    output_identity_sha256: String,
    profiles: Vec<OfflineMulliganPromptProfileV1>,
}

/// A checked-untrusted, offline-only classification of an MTGO London
/// mulligan prompt.
///
/// The classifier evaluates exact binary prompt-ink templates after the
/// capture artifact has independently bound the acting-player Solitaire role,
/// client size, output identity, and canonical raw pixels. A fixed BGR sum
/// threshold removes only same-side grayscale anti-alias intensity variation.
/// Any changed binary mask bit fails the template. Exactly one matching
/// template is required before a prospective keep size or ordered adapter-local
/// pregame actions are exposed. Zero or multiple matches expose neither.
///
/// This type retains no pixels or coordinates and grants no observation, model,
/// or input authority. It intentionally implements neither `Debug` nor `Clone`
/// and is not serializable.
///
/// ```compile_fail
/// use mtgo_blackbox_v1::CheckedUntrustedMtgoOfflineMulliganLadderCandidateV1;
/// fn pixel_escape(value: &CheckedUntrustedMtgoOfflineMulliganLadderCandidateV1) {
///     let _ = value.canonical_pixels_v1();
/// }
/// ```
///
/// ```compile_fail
/// use mtgo_blackbox_v1::CheckedUntrustedMtgoOfflineMulliganLadderCandidateV1;
/// fn coordinate_escape(value: &CheckedUntrustedMtgoOfflineMulliganLadderCandidateV1) {
///     let _ = value.prompt_rect_v1();
/// }
/// ```
pub struct CheckedUntrustedMtgoOfflineMulliganLadderCandidateV1 {
    classification: MtgoOfflineMulliganLadderClassificationV1,
    profile_set_commitment_sha256: String,
    source_manifest_sha256: String,
    source_frame_sha256: String,
    matched_profile_count: usize,
    evaluated_profile_count: usize,
    prospective_keep_size: Option<u8>,
    ordered_actions: Vec<MtgoPregameActionSemanticV1>,
    candidate_commitment_sha256: String,
}

impl CheckedUntrustedMtgoOfflineMulliganLadderCandidateV1 {
    pub fn classification(&self) -> MtgoOfflineMulliganLadderClassificationV1 {
        self.classification
    }

    pub fn profile_set_commitment_sha256(&self) -> &str {
        &self.profile_set_commitment_sha256
    }

    pub fn source_manifest_sha256(&self) -> &str {
        &self.source_manifest_sha256
    }

    pub fn source_frame_sha256(&self) -> &str {
        &self.source_frame_sha256
    }

    pub fn matched_profile_count(&self) -> usize {
        self.matched_profile_count
    }

    pub fn evaluated_profile_count(&self) -> usize {
        self.evaluated_profile_count
    }

    pub fn prospective_keep_size(&self) -> Option<u8> {
        self.prospective_keep_size
    }

    pub fn ordered_actions(&self) -> &[MtgoPregameActionSemanticV1] {
        &self.ordered_actions
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

/// Classifies one later, checked-untrusted DXGI artifact against the fixed
/// seven-state London mulligan ladder. The result is for offline wiring and
/// brittleness measurement only.
pub fn classify_untrusted_offline_mulligan_ladder_candidate_v1(
    checked: &CheckedUntrustedMtgoDxgiCaptureArtifactV1,
    canonical_bgra8: &[u8],
) -> Result<CheckedUntrustedMtgoOfflineMulliganLadderCandidateV1, MtgoContractErrorV1> {
    classify_with_profile_set_v1(checked, canonical_bgra8, &production_profile_set_v1(), true)
}

/// Classifies one later, checked-untrusted DXGI artifact with the revised
/// prompt threshold. The revised threshold retains only the dark core of the
/// visible glyphs and sits at the midpoint of the widest same-label stability
/// interval observed in the development corpus. The result remains offline
/// measurement only and grants no runtime authority.
pub fn classify_untrusted_offline_mulligan_ladder_candidate_v2(
    checked: &CheckedUntrustedMtgoDxgiCaptureArtifactV1,
    canonical_bgra8: &[u8],
) -> Result<CheckedUntrustedMtgoOfflineMulliganLadderCandidateV1, MtgoContractErrorV1> {
    classify_with_profile_set_v1(checked, canonical_bgra8, &production_profile_set_v2(), true)
}

fn production_profile_set_v1() -> OfflineMulliganLadderProfileSetV1 {
    let prompt_rect = MtgoRectPxV1 {
        x: 25,
        y: 48,
        width: 170,
        height: 90,
    };
    let prompt_hashes = [
        (
            7,
            "2754759f2fb99a15061ca334f3ec9729cd4a140ee6be96379387716dcb9c5ef9",
        ),
        (
            6,
            "b2190ec108954d1113c3de026fc3a5f8ebb527d00ae028c38153851e7fa71e9b",
        ),
        (
            5,
            "490c98b908e90c96f275c257fe35790a4ccd5d455097a137f604fce8c5324775",
        ),
        (
            4,
            "b2da242f6594deaa80d421b6c10380a06522a53571d88c930a11a4e3ef449c37",
        ),
        (
            3,
            "8b2a04fa8e36b3c96385f81fe576bb86f88dead2ebce7ae1ff67ef3bc1c0c6d7",
        ),
        (
            2,
            "6c2cfb341ce26856b5ca04b8bf095c87a91d63173dfbbdee896ee0706aab1818",
        ),
        (
            1,
            "4e0408989dccd55b3b5d0f71f2e126ad66c7292d2f18a7aa5e0a603ce0cd962e",
        ),
    ];
    OfflineMulliganLadderProfileSetV1 {
        profile_set_id: OFFLINE_MULLIGAN_LADDER_PROFILE_SET_ID_V1,
        profile_domain: OFFLINE_MULLIGAN_LADDER_PROFILE_DOMAIN_V1,
        candidate_domain: OFFLINE_MULLIGAN_LADDER_CANDIDATE_DOMAIN_V1,
        prompt_mask_domain: OFFLINE_MULLIGAN_PROMPT_INK_MASK_DOMAIN_V1,
        prompt_ink_sum_threshold: OFFLINE_MULLIGAN_PROMPT_INK_SUM_THRESHOLD_V1,
        client_size_px: MtgoSizePxV1 {
            width: OFFLINE_MULLIGAN_LADDER_WIDTH_V1,
            height: OFFLINE_MULLIGAN_LADDER_HEIGHT_V1,
        },
        output_identity_sha256: OFFLINE_MULLIGAN_LADDER_OUTPUT_IDENTITY_V1.to_owned(),
        profiles: prompt_hashes
            .into_iter()
            .map(|(prospective_keep_size, expected_binary_ink_sha256)| {
                OfflineMulliganPromptProfileV1 {
                    prospective_keep_size,
                    prompt_rect: prompt_rect.clone(),
                    expected_binary_ink_sha256: expected_binary_ink_sha256.to_owned(),
                }
            })
            .collect(),
    }
}

fn production_profile_set_v2() -> OfflineMulliganLadderProfileSetV1 {
    let prompt_rect = MtgoRectPxV1 {
        x: 25,
        y: 48,
        width: 170,
        height: 90,
    };
    let prompt_hashes = [
        (
            7,
            "1b5e66aac82589dbd8807c2351a440233a5c4b8927eab7f1273a8bd091d03135",
        ),
        (
            6,
            "a86a8314fea7c0846450e42e55b7e1053a5d1a4b75b0f4abbd8c6557cd91faff",
        ),
        (
            5,
            "6b13119de016b2f663ecfd6fe5c39dfdba6820343a914fa35b8e8e857306da83",
        ),
        (
            4,
            "ecd4819b7fa708d268196fdbfad078dad7ff821d1e1bb94bc0187d4a1128317d",
        ),
        (
            3,
            "b3979b0a1d6711ebc9b480637d25eb990b2d0e53f63e0d3318a4c7849c45aece",
        ),
        (
            2,
            "152407cc7cb11edb51f1b40866914de969c78b34a3863f473840b5663d85c443",
        ),
        (
            1,
            "72669824b221d5102099654c6dd4c69b77915fe558dc2abe4711b732179e7a62",
        ),
    ];
    OfflineMulliganLadderProfileSetV1 {
        profile_set_id: OFFLINE_MULLIGAN_LADDER_PROFILE_SET_ID_V2,
        profile_domain: OFFLINE_MULLIGAN_LADDER_PROFILE_DOMAIN_V2,
        candidate_domain: OFFLINE_MULLIGAN_LADDER_CANDIDATE_DOMAIN_V2,
        prompt_mask_domain: OFFLINE_MULLIGAN_PROMPT_INK_MASK_DOMAIN_V2,
        prompt_ink_sum_threshold: OFFLINE_MULLIGAN_PROMPT_INK_SUM_THRESHOLD_V2,
        client_size_px: MtgoSizePxV1 {
            width: OFFLINE_MULLIGAN_LADDER_WIDTH_V1,
            height: OFFLINE_MULLIGAN_LADDER_HEIGHT_V1,
        },
        output_identity_sha256: OFFLINE_MULLIGAN_LADDER_OUTPUT_IDENTITY_V1.to_owned(),
        profiles: prompt_hashes
            .into_iter()
            .map(|(prospective_keep_size, expected_binary_ink_sha256)| {
                OfflineMulliganPromptProfileV1 {
                    prospective_keep_size,
                    prompt_rect: prompt_rect.clone(),
                    expected_binary_ink_sha256: expected_binary_ink_sha256.to_owned(),
                }
            })
            .collect(),
    }
}

fn classify_with_profile_set_v1(
    checked: &CheckedUntrustedMtgoDxgiCaptureArtifactV1,
    canonical_bgra8: &[u8],
    profile_set: &OfflineMulliganLadderProfileSetV1,
    require_full_ladder: bool,
) -> Result<CheckedUntrustedMtgoOfflineMulliganLadderCandidateV1, MtgoContractErrorV1> {
    let profile_set_commitment_sha256 =
        validate_and_commit_profile_set_v1(profile_set, require_full_ladder)?;
    if checked.capture_role() != MtgoDxgiCaptureRoleV2::ActingPlayerSolitaire {
        return Err(error_v1(
            "offline_mulligan_ladder_candidate_role",
            "candidate must have the acting-player Solitaire role",
        ));
    }
    if checked.client_size_px() != &profile_set.client_size_px
        || checked.output_identity_sha256() != profile_set.output_identity_sha256
    {
        return Err(error_v1(
            "offline_mulligan_ladder_candidate_layout",
            "candidate must match the reviewed client and output layout",
        ));
    }
    validate_pixel_length_v1(checked.client_size_px(), canonical_bgra8)?;
    let canonical_bgra8_sha256 = format!("{:x}", Sha256::digest(canonical_bgra8));
    if canonical_bgra8_sha256 != checked.canonical_bgra8_sha256() {
        return Err(error_v1(
            "offline_mulligan_ladder_candidate_pixels",
            "candidate raw pixels do not match the checked artifact",
        ));
    }

    let observed_hashes: Vec<_> = profile_set
        .profiles
        .iter()
        .map(|profile| {
            hash_prompt_binary_ink_with_profile_v1(
                canonical_bgra8,
                checked.client_size_px(),
                &profile.prompt_rect,
                profile_set.prompt_ink_sum_threshold,
                profile_set.prompt_mask_domain,
            )
        })
        .collect::<Result<_, _>>()?;
    let matched_indices: Vec<_> = observed_hashes
        .iter()
        .zip(&profile_set.profiles)
        .enumerate()
        .filter_map(|(index, (observed, profile))| {
            (observed == &profile.expected_binary_ink_sha256).then_some(index)
        })
        .collect();

    let (classification, prospective_keep_size, ordered_actions) =
        classify_matched_profiles_v1(profile_set, &matched_indices);

    let mut hasher = Sha256::new();
    hasher.update(profile_set.candidate_domain);
    for part in [
        profile_set_commitment_sha256.as_bytes(),
        checked.manifest_sha256().as_bytes(),
        checked.canonical_bgra8_sha256().as_bytes(),
        checked.output_identity_sha256().as_bytes(),
        classification_tag_v1(classification),
        &[u8::try_from(matched_indices.len()).unwrap_or(u8::MAX)],
        &[prospective_keep_size.unwrap_or(0)],
    ] {
        update_hash_part_v1(&mut hasher, part);
    }
    for observed in &observed_hashes {
        update_hash_part_v1(&mut hasher, observed.as_bytes());
    }

    Ok(CheckedUntrustedMtgoOfflineMulliganLadderCandidateV1 {
        classification,
        profile_set_commitment_sha256,
        source_manifest_sha256: checked.manifest_sha256().to_owned(),
        source_frame_sha256: checked.canonical_bgra8_sha256().to_owned(),
        matched_profile_count: matched_indices.len(),
        evaluated_profile_count: profile_set.profiles.len(),
        prospective_keep_size,
        ordered_actions,
        candidate_commitment_sha256: format!("{:x}", hasher.finalize()),
    })
}

fn classify_matched_profiles_v1(
    profile_set: &OfflineMulliganLadderProfileSetV1,
    matched_indices: &[usize],
) -> (
    MtgoOfflineMulliganLadderClassificationV1,
    Option<u8>,
    Vec<MtgoPregameActionSemanticV1>,
) {
    match matched_indices {
        [] => (
            MtgoOfflineMulliganLadderClassificationV1::NoMatch,
            None,
            Vec::new(),
        ),
        [index] => {
            let prospective_keep_size = profile_set.profiles[*index].prospective_keep_size;
            (
                MtgoOfflineMulliganLadderClassificationV1::Match,
                Some(prospective_keep_size),
                vec![
                    MtgoPregameActionSemanticV1::Mulligan {
                        next_hand_size: prospective_keep_size - 1,
                    },
                    MtgoPregameActionSemanticV1::KeepOpeningHand,
                ],
            )
        }
        _ => (
            MtgoOfflineMulliganLadderClassificationV1::Ambiguous,
            None,
            Vec::new(),
        ),
    }
}

fn validate_and_commit_profile_set_v1(
    profile_set: &OfflineMulliganLadderProfileSetV1,
    require_full_ladder: bool,
) -> Result<String, MtgoContractErrorV1> {
    if profile_set.profile_set_id.is_empty()
        || profile_set.profile_set_id.len() > 96
        || !profile_set
            .profile_set_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        || !is_lower_sha256_v1(&profile_set.output_identity_sha256)
        || profile_set.client_size_px.width == 0
        || profile_set.client_size_px.height == 0
        || profile_set.profiles.is_empty()
        || profile_set.profiles.len() > 7
    {
        return Err(error_v1(
            "offline_mulligan_ladder_profile",
            "profile set identity, size, or profile count is invalid",
        ));
    }
    let keep_sizes: Vec<_> = profile_set
        .profiles
        .iter()
        .map(|profile| profile.prospective_keep_size)
        .collect();
    if keep_sizes
        .iter()
        .any(|keep_size| !(1..=7).contains(keep_size))
        || keep_sizes.windows(2).any(|pair| pair[0] <= pair[1])
        || (require_full_ladder && keep_sizes != [7, 6, 5, 4, 3, 2, 1])
    {
        return Err(error_v1(
            "offline_mulligan_ladder_profile",
            "profiles must use unique descending prospective keep sizes from seven through one",
        ));
    }

    let mut hasher = Sha256::new();
    hasher.update(profile_set.profile_domain);
    for part in [
        profile_set.profile_set_id.as_bytes(),
        &profile_set.client_size_px.width.to_be_bytes(),
        &profile_set.client_size_px.height.to_be_bytes(),
        profile_set.output_identity_sha256.as_bytes(),
        &[u8::try_from(profile_set.profiles.len()).unwrap_or(u8::MAX)],
    ] {
        update_hash_part_v1(&mut hasher, part);
    }
    for profile in &profile_set.profiles {
        validate_region_v1(&profile_set.client_size_px, &profile.prompt_rect)?;
        if !is_lower_sha256_v1(&profile.expected_binary_ink_sha256) {
            return Err(error_v1(
                "offline_mulligan_ladder_profile",
                "every prompt template requires a lowercase SHA-256",
            ));
        }
        update_hash_part_v1(&mut hasher, &[profile.prospective_keep_size]);
        update_hash_part_v1(
            &mut hasher,
            &profile_set.prompt_ink_sum_threshold.to_be_bytes(),
        );
        for value in [
            profile.prompt_rect.x,
            profile.prompt_rect.y,
            profile.prompt_rect.width,
            profile.prompt_rect.height,
        ] {
            update_hash_part_v1(&mut hasher, &value.to_be_bytes());
        }
        update_hash_part_v1(&mut hasher, profile.expected_binary_ink_sha256.as_bytes());
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn classification_tag_v1(
    classification: MtgoOfflineMulliganLadderClassificationV1,
) -> &'static [u8] {
    match classification {
        MtgoOfflineMulliganLadderClassificationV1::Match => b"match",
        MtgoOfflineMulliganLadderClassificationV1::NoMatch => b"no_match",
        MtgoOfflineMulliganLadderClassificationV1::Ambiguous => b"ambiguous",
    }
}

fn validate_pixel_length_v1(size: &MtgoSizePxV1, pixels: &[u8]) -> Result<(), MtgoContractErrorV1> {
    let expected = usize::try_from(size.width)
        .ok()
        .and_then(|width| {
            usize::try_from(size.height)
                .ok()
                .and_then(|height| width.checked_mul(height))
        })
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or_else(|| error_v1("offline_mulligan_ladder_pixels", "pixel length overflow"))?;
    if pixels.len() != expected {
        return Err(error_v1(
            "offline_mulligan_ladder_pixels",
            "canonical BGRA8 byte length does not match the client size",
        ));
    }
    Ok(())
}

#[cfg(test)]
fn hash_prompt_binary_ink_v1(
    pixels: &[u8],
    size: &MtgoSizePxV1,
    rect: &MtgoRectPxV1,
) -> Result<String, MtgoContractErrorV1> {
    hash_prompt_binary_ink_with_profile_v1(
        pixels,
        size,
        rect,
        OFFLINE_MULLIGAN_PROMPT_INK_SUM_THRESHOLD_V1,
        OFFLINE_MULLIGAN_PROMPT_INK_MASK_DOMAIN_V1,
    )
}

fn hash_prompt_binary_ink_with_profile_v1(
    pixels: &[u8],
    size: &MtgoSizePxV1,
    rect: &MtgoRectPxV1,
    threshold: u16,
    mask_domain: &[u8],
) -> Result<String, MtgoContractErrorV1> {
    validate_pixel_length_v1(size, pixels)?;
    validate_region_v1(size, rect)?;
    let frame_width = usize::try_from(size.width).map_err(|_| {
        error_v1(
            "offline_mulligan_ladder_pixels",
            "frame width conversion overflow",
        )
    })?;
    let mask_len =
        usize::try_from(u64::from(rect.width) * u64::from(rect.height)).map_err(|_| {
            error_v1(
                "offline_mulligan_ladder_pixels",
                "prompt mask length overflow",
            )
        })?;
    let mut mask = Vec::with_capacity(mask_len);
    for row in 0..rect.height {
        for column in 0..rect.width {
            let pixel_index = usize::try_from(rect.y + row)
                .ok()
                .and_then(|y| y.checked_mul(frame_width))
                .and_then(|base| {
                    usize::try_from(rect.x + column)
                        .ok()
                        .and_then(|x| base.checked_add(x))
                })
                .and_then(|index| index.checked_mul(4))
                .ok_or_else(|| {
                    error_v1(
                        "offline_mulligan_ladder_pixels",
                        "prompt pixel offset overflow",
                    )
                })?;
            let bgr_sum = u16::from(pixels[pixel_index])
                + u16::from(pixels[pixel_index + 1])
                + u16::from(pixels[pixel_index + 2]);
            mask.push(u8::from(bgr_sum < threshold));
        }
    }
    let mut hasher = Sha256::new();
    hasher.update(mask_domain);
    for part in [
        threshold.to_be_bytes().as_slice(),
        rect.width.to_be_bytes().as_slice(),
        rect.height.to_be_bytes().as_slice(),
        mask.as_slice(),
    ] {
        update_hash_part_v1(&mut hasher, part);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn validate_region_v1(size: &MtgoSizePxV1, rect: &MtgoRectPxV1) -> Result<(), MtgoContractErrorV1> {
    let right = rect.x.checked_add(rect.width);
    let bottom = rect.y.checked_add(rect.height);
    if rect.width == 0
        || rect.height == 0
        || u64::from(rect.width) * u64::from(rect.height) < 16
        || right.is_none_or(|right| right > size.width)
        || bottom.is_none_or(|bottom| bottom > size.height)
    {
        return Err(error_v1(
            "offline_mulligan_ladder_region_geometry",
            "prompt region must be substantial and inside the client",
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

    fn synthetic_pixels_v1() -> (Vec<u8>, MtgoSizePxV1) {
        let size = MtgoSizePxV1 {
            width: 32,
            height: 16,
        };
        let mut pixels = vec![0_u8; 32 * 16 * 4];
        for (index, pixel) in pixels.chunks_exact_mut(4).enumerate() {
            pixel[0] = (index % 251) as u8;
            pixel[1] = ((index * 3) % 251) as u8;
            pixel[2] = ((index * 7) % 251) as u8;
            pixel[3] = 255;
        }
        (pixels, size)
    }

    fn synthetic_profile_set_v1(
        pixels: &[u8],
        size: &MtgoSizePxV1,
    ) -> OfflineMulliganLadderProfileSetV1 {
        let rects = [
            MtgoRectPxV1 {
                x: 1,
                y: 1,
                width: 4,
                height: 4,
            },
            MtgoRectPxV1 {
                x: 8,
                y: 1,
                width: 4,
                height: 4,
            },
        ];
        OfflineMulliganLadderProfileSetV1 {
            profile_set_id: "synthetic-mulligan-ladder-v1",
            profile_domain: OFFLINE_MULLIGAN_LADDER_PROFILE_DOMAIN_V1,
            candidate_domain: OFFLINE_MULLIGAN_LADDER_CANDIDATE_DOMAIN_V1,
            prompt_mask_domain: OFFLINE_MULLIGAN_PROMPT_INK_MASK_DOMAIN_V1,
            prompt_ink_sum_threshold: OFFLINE_MULLIGAN_PROMPT_INK_SUM_THRESHOLD_V1,
            client_size_px: size.clone(),
            output_identity_sha256: "a".repeat(64),
            profiles: [2_u8, 1_u8]
                .into_iter()
                .zip(rects)
                .map(|(prospective_keep_size, prompt_rect)| {
                    let expected_binary_ink_sha256 =
                        hash_prompt_binary_ink_v1(pixels, size, &prompt_rect).unwrap();
                    OfflineMulliganPromptProfileV1 {
                        prospective_keep_size,
                        prompt_rect,
                        expected_binary_ink_sha256,
                    }
                })
                .collect(),
        }
    }

    #[test]
    fn production_profile_set_is_complete_and_committed() {
        let profile_set = production_profile_set_v1();
        assert_eq!(
            profile_set
                .profiles
                .iter()
                .map(|profile| profile.prospective_keep_size)
                .collect::<Vec<_>>(),
            [7, 6, 5, 4, 3, 2, 1]
        );
        assert_eq!(
            validate_and_commit_profile_set_v1(&profile_set, true).unwrap(),
            "70869ef8cbf9fd38e3b660d9ce03d7258d9e9f556bfd17d939cd8df86dce440a"
        );
    }

    #[test]
    fn revised_production_profile_set_is_complete_and_distinct() {
        let predecessor = production_profile_set_v1();
        let revised = production_profile_set_v2();
        assert_eq!(
            revised
                .profiles
                .iter()
                .map(|profile| profile.prospective_keep_size)
                .collect::<Vec<_>>(),
            [7, 6, 5, 4, 3, 2, 1]
        );
        assert_eq!(revised.prompt_ink_sum_threshold, 182);
        assert_ne!(
            validate_and_commit_profile_set_v1(&predecessor, true).unwrap(),
            validate_and_commit_profile_set_v1(&revised, true).unwrap()
        );
    }

    #[test]
    fn synthetic_profile_matches_zero_one_or_multiple_templates() {
        let (pixels, size) = synthetic_pixels_v1();
        let profile_set = synthetic_profile_set_v1(&pixels, &size);
        let (ambiguous, keep_size, actions) = classify_matched_profiles_v1(&profile_set, &[0, 1]);
        assert_eq!(
            ambiguous,
            MtgoOfflineMulliganLadderClassificationV1::Ambiguous
        );
        assert_eq!(keep_size, None);
        assert!(actions.is_empty());

        let (matched, keep_size, actions) = classify_matched_profiles_v1(&profile_set, &[0]);
        assert_eq!(matched, MtgoOfflineMulliganLadderClassificationV1::Match);
        assert_eq!(keep_size, Some(2));
        assert_eq!(
            actions,
            [
                MtgoPregameActionSemanticV1::Mulligan { next_hand_size: 1 },
                MtgoPregameActionSemanticV1::KeepOpeningHand,
            ]
        );

        let (no_match, keep_size, actions) = classify_matched_profiles_v1(&profile_set, &[]);
        assert_eq!(no_match, MtgoOfflineMulliganLadderClassificationV1::NoMatch);
        assert_eq!(keep_size, None);
        assert!(actions.is_empty());
    }

    #[test]
    fn binary_ink_hash_ignores_same_side_antialias_and_rejects_threshold_crossing() {
        let size = MtgoSizePxV1 {
            width: 4,
            height: 4,
        };
        let rect = MtgoRectPxV1 {
            x: 0,
            y: 0,
            width: 4,
            height: 4,
        };
        let mut dark = vec![0_u8; 4 * 4 * 4];
        for pixel in dark.chunks_exact_mut(4) {
            pixel.copy_from_slice(&[77, 77, 77, 255]);
        }
        let mut same_side = dark.clone();
        same_side[..4].copy_from_slice(&[120, 120, 120, 1]);
        assert_eq!(
            hash_prompt_binary_ink_v1(&dark, &size, &rect).unwrap(),
            hash_prompt_binary_ink_v1(&same_side, &size, &rect).unwrap()
        );

        let mut threshold_crossing = dark.clone();
        threshold_crossing[..4].copy_from_slice(&[128, 128, 128, 255]);
        assert_ne!(
            hash_prompt_binary_ink_v1(&dark, &size, &rect).unwrap(),
            hash_prompt_binary_ink_v1(&threshold_crossing, &size, &rect).unwrap()
        );
    }

    #[test]
    fn malformed_profile_set_fails_closed() {
        let (pixels, size) = synthetic_pixels_v1();
        let mut profile_set = synthetic_profile_set_v1(&pixels, &size);
        profile_set.profiles[1].prospective_keep_size = 2;
        assert_eq!(
            validate_and_commit_profile_set_v1(&profile_set, false)
                .err()
                .unwrap()
                .code(),
            "offline_mulligan_ladder_profile"
        );

        let mut profile_set = synthetic_profile_set_v1(&pixels, &size);
        profile_set.profiles[0].prompt_rect.width = u32::MAX;
        assert_eq!(
            validate_and_commit_profile_set_v1(&profile_set, false)
                .err()
                .unwrap()
                .code(),
            "offline_mulligan_ladder_region_geometry"
        );
    }
}
