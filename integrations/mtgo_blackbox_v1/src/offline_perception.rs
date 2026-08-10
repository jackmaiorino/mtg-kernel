use crate::{
    hash_bgra_region_v1, AdmittedMtgoDxgiOfflineCalibrationFrameV1, MtgoContractErrorV1,
    MtgoPregameActionSemanticV1, MtgoRectPxV1, MtgoSizePxV1,
};
use sha2::{Digest, Sha256};

const OFFLINE_PREGAME_PROFILE_ID_V1: &str = "freeform-solitaire-opening-hand-20260810-v1";
const OFFLINE_PREGAME_PROFILE_DOMAIN_V1: &[u8] = b"mtgo-offline-pregame-profile-v1";
const OFFLINE_PREGAME_RECOGNITION_DOMAIN_V1: &[u8] = b"mtgo-offline-pregame-recognition-v1";
const OFFLINE_PREGAME_SOURCE_ADMISSION_V1: &str =
    "9b0aef61a6fc31ee6050d9e381fba4c3e1a6b887c62319c8b3e1e13e9523e291";
const OFFLINE_PREGAME_WIDTH_V1: u32 = 1550;
const OFFLINE_PREGAME_HEIGHT_V1: u32 = 925;
const OFFLINE_PREGAME_HAND_SIZE_V1: u8 = 7;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MtgoOfflinePregameDecisionKindV1 {
    OpeningHandKeepOrMulligan,
}

#[derive(Clone)]
struct OfflinePregameRegionProfileV1 {
    label: &'static str,
    rect: MtgoRectPxV1,
    expected_bgra8_sha256: String,
}

#[derive(Clone)]
struct OfflinePregameProfileV1 {
    profile_id: &'static str,
    source_admission_commitment_sha256: String,
    client_size_px: MtgoSizePxV1,
    hand_size: u8,
    regions: Vec<OfflinePregameRegionProfileV1>,
}

struct OfflinePregameSourceV1<'a> {
    admission_commitment_sha256: &'a str,
    manifest_sha256: &'a str,
    canonical_bgra8_sha256: &'a str,
    client_size_px: &'a MtgoSizePxV1,
    canonical_bgra8: &'a [u8],
}

/// An exact-artifact, offline-only semantic reading of one reviewed MTGO frame.
///
/// The fixed profile maps four reviewed pixel regions to an opening-hand prompt,
/// a seven-card hand, and the ordered adapter-local Mulligan then Keep actions.
/// This is one calibration example, not a measured recognizer. The value retains
/// no pixels or coordinates and cannot become an `ObservationV5`, a model request,
/// or an input command.
///
/// It intentionally implements neither `Debug` nor `Clone` and is not serializable.
///
/// ```compile_fail
/// use mtgo_blackbox_v1::RecognizedOfflineMtgoPregameDecisionV1;
/// fn pixel_escape(value: &RecognizedOfflineMtgoPregameDecisionV1) {
///     let _ = value.canonical_pixels_v1();
/// }
/// ```
///
/// ```compile_fail
/// use mtgo_blackbox_v1::RecognizedOfflineMtgoPregameDecisionV1;
/// fn coordinate_escape(value: &RecognizedOfflineMtgoPregameDecisionV1) {
///     let _ = value.control_rects_v1();
/// }
/// ```
pub struct RecognizedOfflineMtgoPregameDecisionV1 {
    profile_id: &'static str,
    profile_commitment_sha256: String,
    source_manifest_sha256: String,
    recognition_commitment_sha256: String,
    actions: Vec<MtgoPregameActionSemanticV1>,
    hand_size: u8,
}

impl RecognizedOfflineMtgoPregameDecisionV1 {
    pub fn kind(&self) -> MtgoOfflinePregameDecisionKindV1 {
        MtgoOfflinePregameDecisionKindV1::OpeningHandKeepOrMulligan
    }

    pub fn profile_id(&self) -> &str {
        self.profile_id
    }

    pub fn profile_commitment_sha256(&self) -> &str {
        &self.profile_commitment_sha256
    }

    pub fn source_manifest_sha256(&self) -> &str {
        &self.source_manifest_sha256
    }

    pub fn recognition_commitment_sha256(&self) -> &str {
        &self.recognition_commitment_sha256
    }

    pub fn hand_size(&self) -> u8 {
        self.hand_size
    }

    pub fn actions(&self) -> &[MtgoPregameActionSemanticV1] {
        &self.actions
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

/// Recognizes only the exact reviewed opening-hand frame already admitted for
/// offline calibration. No caller-selected profile or semantic label is accepted.
pub fn recognize_ratified_offline_opening_hand_v1(
    frame: &AdmittedMtgoDxgiOfflineCalibrationFrameV1,
) -> Result<RecognizedOfflineMtgoPregameDecisionV1, MtgoContractErrorV1> {
    let profile = production_profile_v1();
    let source = OfflinePregameSourceV1 {
        admission_commitment_sha256: frame.admission_commitment_sha256(),
        manifest_sha256: frame.manifest_sha256(),
        canonical_bgra8_sha256: frame.canonical_bgra8_sha256(),
        client_size_px: frame.client_size_px(),
        canonical_bgra8: frame.canonical_pixels_for_offline_calibration_v1(),
    };
    recognize_with_profile_v1(&source, &profile)
}

fn production_profile_v1() -> OfflinePregameProfileV1 {
    OfflinePregameProfileV1 {
        profile_id: OFFLINE_PREGAME_PROFILE_ID_V1,
        source_admission_commitment_sha256: OFFLINE_PREGAME_SOURCE_ADMISSION_V1.to_owned(),
        client_size_px: MtgoSizePxV1 {
            width: OFFLINE_PREGAME_WIDTH_V1,
            height: OFFLINE_PREGAME_HEIGHT_V1,
        },
        hand_size: OFFLINE_PREGAME_HAND_SIZE_V1,
        regions: vec![
            OfflinePregameRegionProfileV1 {
                label: "prompt_text",
                rect: MtgoRectPxV1 {
                    x: 18,
                    y: 42,
                    width: 180,
                    height: 70,
                },
                expected_bgra8_sha256:
                    "fdb8826ca1932edff9aa99731c7abbcf9ac57d07f82f5b7a0467ba6b4cd53593".to_owned(),
            },
            OfflinePregameRegionProfileV1 {
                label: "mulligan_control",
                rect: MtgoRectPxV1 {
                    x: 29,
                    y: 153,
                    width: 82,
                    height: 33,
                },
                expected_bgra8_sha256:
                    "0177e2d0c071f0eefe5a56a0ecfddce0e9eb3d9640137dd8eb5bc941ad9571a7".to_owned(),
            },
            OfflinePregameRegionProfileV1 {
                label: "keep_control",
                rect: MtgoRectPxV1 {
                    x: 116,
                    y: 153,
                    width: 55,
                    height: 33,
                },
                expected_bgra8_sha256:
                    "bb8cb3743f99be44e18fc92738c89d2647169ea206a5e54981562348f8ce0b60".to_owned(),
            },
            OfflinePregameRegionProfileV1 {
                label: "hand_count",
                rect: MtgoRectPxV1 {
                    x: 148,
                    y: 207,
                    width: 33,
                    height: 34,
                },
                expected_bgra8_sha256:
                    "8ce52c08751a17ac67168d1c452dadcb393811bba59759b0747d02d4ee4c180e".to_owned(),
            },
        ],
    }
}

fn recognize_with_profile_v1(
    source: &OfflinePregameSourceV1<'_>,
    profile: &OfflinePregameProfileV1,
) -> Result<RecognizedOfflineMtgoPregameDecisionV1, MtgoContractErrorV1> {
    let profile_commitment_sha256 = validate_and_commit_profile_v1(profile)?;
    if source.admission_commitment_sha256 != profile.source_admission_commitment_sha256 {
        return Err(error_v1(
            "offline_pregame_source",
            "source admission commitment does not match the fixed profile",
        ));
    }
    if source.client_size_px != &profile.client_size_px {
        return Err(error_v1(
            "offline_pregame_source",
            "source client size does not match the fixed profile",
        ));
    }
    validate_pixel_length_v1(source.client_size_px, source.canonical_bgra8)?;

    let mut observed_region_hashes = Vec::with_capacity(profile.regions.len());
    for region in &profile.regions {
        let observed =
            hash_bgra_region_v1(source.canonical_bgra8, source.client_size_px, &region.rect)?;
        if observed != region.expected_bgra8_sha256 {
            return Err(error_v1(
                "offline_pregame_region_hash",
                format!(
                    "{} region does not match the reviewed template",
                    region.label
                ),
            ));
        }
        observed_region_hashes.push(observed);
    }

    let actions = vec![
        MtgoPregameActionSemanticV1::Mulligan { next_hand_size: 6 },
        MtgoPregameActionSemanticV1::KeepOpeningHand,
    ];
    let mut hasher = Sha256::new();
    hasher.update(OFFLINE_PREGAME_RECOGNITION_DOMAIN_V1);
    for part in [
        profile_commitment_sha256.as_bytes(),
        source.admission_commitment_sha256.as_bytes(),
        source.manifest_sha256.as_bytes(),
        source.canonical_bgra8_sha256.as_bytes(),
        b"mulligan:6",
        b"keep",
    ] {
        update_hash_part_v1(&mut hasher, part);
    }
    for observed in &observed_region_hashes {
        update_hash_part_v1(&mut hasher, observed.as_bytes());
    }

    Ok(RecognizedOfflineMtgoPregameDecisionV1 {
        profile_id: profile.profile_id,
        profile_commitment_sha256,
        source_manifest_sha256: source.manifest_sha256.to_owned(),
        recognition_commitment_sha256: format!("{:x}", hasher.finalize()),
        actions,
        hand_size: profile.hand_size,
    })
}

fn validate_and_commit_profile_v1(
    profile: &OfflinePregameProfileV1,
) -> Result<String, MtgoContractErrorV1> {
    if profile.profile_id.is_empty()
        || profile.profile_id.len() > 96
        || !profile
            .profile_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(error_v1(
            "offline_pregame_profile",
            "profile ID must use 1 to 96 safe identifier characters",
        ));
    }
    if !is_lower_sha256_v1(&profile.source_admission_commitment_sha256)
        || profile.client_size_px.width == 0
        || profile.client_size_px.height == 0
        || profile.hand_size != 7
    {
        return Err(error_v1(
            "offline_pregame_profile",
            "profile source, size, or hand size is invalid",
        ));
    }
    let expected_labels = [
        "prompt_text",
        "mulligan_control",
        "keep_control",
        "hand_count",
    ];
    let actual_labels: Vec<_> = profile.regions.iter().map(|region| region.label).collect();
    if actual_labels.as_slice() != expected_labels {
        return Err(error_v1(
            "offline_pregame_profile",
            "profile requires the canonical four reviewed regions in order",
        ));
    }

    let mut hasher = Sha256::new();
    hasher.update(OFFLINE_PREGAME_PROFILE_DOMAIN_V1);
    for part in [
        profile.profile_id.as_bytes(),
        profile.source_admission_commitment_sha256.as_bytes(),
        &profile.client_size_px.width.to_be_bytes(),
        &profile.client_size_px.height.to_be_bytes(),
        &[profile.hand_size],
        b"mulligan:6",
        b"keep",
    ] {
        update_hash_part_v1(&mut hasher, part);
    }
    for (index, region) in profile.regions.iter().enumerate() {
        validate_region_v1(&profile.client_size_px, &region.rect)?;
        if !is_lower_sha256_v1(&region.expected_bgra8_sha256) {
            return Err(error_v1(
                "offline_pregame_profile",
                "every reviewed region requires a lowercase SHA-256",
            ));
        }
        for previous in &profile.regions[..index] {
            if rects_overlap_v1(&previous.rect, &region.rect) {
                return Err(error_v1(
                    "offline_pregame_profile",
                    "reviewed regions must not overlap",
                ));
            }
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
    Ok(format!("{:x}", hasher.finalize()))
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
        .ok_or_else(|| error_v1("offline_pregame_pixels", "pixel length overflow"))?;
    if pixels.len() != expected {
        return Err(error_v1(
            "offline_pregame_pixels",
            "canonical BGRA8 byte length does not match the client size",
        ));
    }
    Ok(())
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
            "offline_pregame_region_geometry",
            "reviewed region must be substantial and inside the client",
        ));
    }
    Ok(())
}

fn rects_overlap_v1(left: &MtgoRectPxV1, right: &MtgoRectPxV1) -> bool {
    let Some(left_right) = left.x.checked_add(left.width) else {
        return true;
    };
    let Some(left_bottom) = left.y.checked_add(left.height) else {
        return true;
    };
    let Some(right_right) = right.x.checked_add(right.width) else {
        return true;
    };
    let Some(right_bottom) = right.y.checked_add(right.height) else {
        return true;
    };
    left.x < right_right && right.x < left_right && left.y < right_bottom && right.y < left_bottom
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

    fn digest(byte: char) -> String {
        std::iter::repeat_n(byte, 64).collect()
    }

    fn synthetic_fixture_v1() -> (Vec<u8>, MtgoSizePxV1, OfflinePregameProfileV1) {
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
        let mut regions = vec![
            OfflinePregameRegionProfileV1 {
                label: "prompt_text",
                rect: MtgoRectPxV1 {
                    x: 1,
                    y: 1,
                    width: 4,
                    height: 4,
                },
                expected_bgra8_sha256: String::new(),
            },
            OfflinePregameRegionProfileV1 {
                label: "mulligan_control",
                rect: MtgoRectPxV1 {
                    x: 8,
                    y: 1,
                    width: 4,
                    height: 4,
                },
                expected_bgra8_sha256: String::new(),
            },
            OfflinePregameRegionProfileV1 {
                label: "keep_control",
                rect: MtgoRectPxV1 {
                    x: 15,
                    y: 1,
                    width: 4,
                    height: 4,
                },
                expected_bgra8_sha256: String::new(),
            },
            OfflinePregameRegionProfileV1 {
                label: "hand_count",
                rect: MtgoRectPxV1 {
                    x: 22,
                    y: 1,
                    width: 4,
                    height: 4,
                },
                expected_bgra8_sha256: String::new(),
            },
        ];
        for region in &mut regions {
            region.expected_bgra8_sha256 =
                hash_bgra_region_v1(&pixels, &size, &region.rect).unwrap();
        }
        let profile = OfflinePregameProfileV1 {
            profile_id: "synthetic-opening-hand-v1",
            source_admission_commitment_sha256: digest('a'),
            client_size_px: size.clone(),
            hand_size: 7,
            regions,
        };
        (pixels, size, profile)
    }

    #[test]
    fn exact_reviewed_regions_create_offline_actions_only() {
        let (pixels, size, profile) = synthetic_fixture_v1();
        let source = OfflinePregameSourceV1 {
            admission_commitment_sha256: &profile.source_admission_commitment_sha256,
            manifest_sha256: &digest('b'),
            canonical_bgra8_sha256: &digest('c'),
            client_size_px: &size,
            canonical_bgra8: &pixels,
        };
        let recognized = recognize_with_profile_v1(&source, &profile).unwrap();

        assert_eq!(recognized.hand_size(), 7);
        assert_eq!(
            recognized.actions(),
            &[
                MtgoPregameActionSemanticV1::Mulligan { next_hand_size: 6 },
                MtgoPregameActionSemanticV1::KeepOpeningHand,
            ]
        );
        assert_eq!(recognized.profile_commitment_sha256().len(), 64);
        assert_eq!(recognized.recognition_commitment_sha256().len(), 64);
        assert!(!recognized.safe_for_live_frame());
        assert!(!recognized.safe_for_semantic_evidence());
        assert!(!recognized.safe_for_observation_v5());
        assert!(!recognized.safe_for_policy_scoring());
        assert!(!recognized.safe_for_input());
    }

    #[test]
    fn every_reviewed_region_fails_closed_on_one_changed_pixel() {
        let (pixels, size, profile) = synthetic_fixture_v1();
        for region in &profile.regions {
            let mut changed = pixels.clone();
            let offset = ((region.rect.y * size.width + region.rect.x) * 4) as usize;
            changed[offset] ^= 1;
            let source = OfflinePregameSourceV1 {
                admission_commitment_sha256: &profile.source_admission_commitment_sha256,
                manifest_sha256: &digest('b'),
                canonical_bgra8_sha256: &digest('c'),
                client_size_px: &size,
                canonical_bgra8: &changed,
            };
            assert_eq!(
                recognize_with_profile_v1(&source, &profile)
                    .err()
                    .unwrap()
                    .code(),
                "offline_pregame_region_hash"
            );
        }
    }

    #[test]
    fn source_identity_size_and_profile_geometry_fail_closed() {
        let (pixels, size, profile) = synthetic_fixture_v1();
        let wrong_source = OfflinePregameSourceV1 {
            admission_commitment_sha256: &digest('f'),
            manifest_sha256: &digest('b'),
            canonical_bgra8_sha256: &digest('c'),
            client_size_px: &size,
            canonical_bgra8: &pixels,
        };
        assert_eq!(
            recognize_with_profile_v1(&wrong_source, &profile)
                .err()
                .unwrap()
                .code(),
            "offline_pregame_source"
        );

        let mut overlap = profile.clone();
        overlap.regions[3].rect = overlap.regions[2].rect.clone();
        let source = OfflinePregameSourceV1 {
            admission_commitment_sha256: &profile.source_admission_commitment_sha256,
            manifest_sha256: &digest('b'),
            canonical_bgra8_sha256: &digest('c'),
            client_size_px: &size,
            canonical_bgra8: &pixels,
        };
        assert_eq!(
            recognize_with_profile_v1(&source, &overlap)
                .err()
                .unwrap()
                .code(),
            "offline_pregame_profile"
        );

        let short_pixels = &pixels[..pixels.len() - 4];
        let short_source = OfflinePregameSourceV1 {
            canonical_bgra8: short_pixels,
            ..source
        };
        assert_eq!(
            recognize_with_profile_v1(&short_source, &profile)
                .err()
                .unwrap()
                .code(),
            "offline_pregame_pixels"
        );
    }
}
