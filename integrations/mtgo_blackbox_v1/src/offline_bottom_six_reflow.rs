use crate::{
    classify_untrusted_offline_bottom_six_state_candidate_v1,
    CheckedUntrustedMtgoDxgiCaptureArtifactV1, MtgoContractErrorV1,
    MtgoOfflineBottomSixStateClassificationV1, MtgoRectPxV1, MtgoSizePxV1,
};
use sha2::{Digest, Sha256};

const CANDIDATE_DOMAIN_V1: &[u8] = b"mtgo-offline-bottom-six-reflow-candidate-v1";
const MAX_MATCH_MEAN_ABSOLUTE_DIFFERENCE_MILLI_V1: u32 = 25_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MtgoOfflineBottomSixReflowClassificationV1 {
    Match,
    NoMatch,
}

/// A checked-untrusted visual transition between two consecutive reviewed
/// bottom-six selection stages.
///
/// A Match means exactly one order-preserving deletion aligns every remaining
/// card-art crop below the fixed visual-distance ceiling. Multiple plausible
/// deletions, including indistinguishable duplicate cards, fail closed. The
/// result retains no pixels or coordinates and grants no observation, scoring,
/// or input authority.
///
/// ```compile_fail
/// use mtgo_blackbox_v1::CheckedUntrustedMtgoOfflineBottomSixReflowCandidateV1;
/// fn pixel_escape(value: &CheckedUntrustedMtgoOfflineBottomSixReflowCandidateV1) {
///     let _ = value.before_pixels_v1();
/// }
/// ```
///
/// ```compile_fail
/// use mtgo_blackbox_v1::CheckedUntrustedMtgoOfflineBottomSixReflowCandidateV1;
/// fn coordinate_escape(value: &CheckedUntrustedMtgoOfflineBottomSixReflowCandidateV1) {
///     let _ = value.card_rects_v1();
/// }
/// ```
pub struct CheckedUntrustedMtgoOfflineBottomSixReflowCandidateV1 {
    classification: MtgoOfflineBottomSixReflowClassificationV1,
    before_manifest_sha256: String,
    after_manifest_sha256: String,
    before_stage_commitment_sha256: String,
    after_stage_commitment_sha256: String,
    before_selected_count: Option<u8>,
    after_selected_count: Option<u8>,
    removed_before_ordinal: Option<u8>,
    matched_pair_mean_absolute_difference_milli: Vec<u32>,
    passing_deletion_candidate_count: u8,
    candidate_commitment_sha256: String,
}

impl CheckedUntrustedMtgoOfflineBottomSixReflowCandidateV1 {
    pub fn classification(&self) -> MtgoOfflineBottomSixReflowClassificationV1 {
        self.classification
    }

    pub fn before_manifest_sha256(&self) -> &str {
        &self.before_manifest_sha256
    }

    pub fn after_manifest_sha256(&self) -> &str {
        &self.after_manifest_sha256
    }

    pub fn before_stage_commitment_sha256(&self) -> &str {
        &self.before_stage_commitment_sha256
    }

    pub fn after_stage_commitment_sha256(&self) -> &str {
        &self.after_stage_commitment_sha256
    }

    pub fn before_selected_count(&self) -> Option<u8> {
        self.before_selected_count
    }

    pub fn after_selected_count(&self) -> Option<u8> {
        self.after_selected_count
    }

    pub fn removed_before_ordinal(&self) -> Option<u8> {
        self.removed_before_ordinal
    }

    pub fn matched_pair_mean_absolute_difference_milli(&self) -> &[u32] {
        &self.matched_pair_mean_absolute_difference_milli
    }

    pub fn passing_deletion_candidate_count(&self) -> u8 {
        self.passing_deletion_candidate_count
    }

    pub fn maximum_match_mean_absolute_difference_milli(&self) -> u32 {
        MAX_MATCH_MEAN_ABSOLUTE_DIFFERENCE_MILLI_V1
    }

    pub fn candidate_commitment_sha256(&self) -> &str {
        &self.candidate_commitment_sha256
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

pub fn classify_untrusted_offline_bottom_six_reflow_candidate_v1(
    before_checked: &CheckedUntrustedMtgoDxgiCaptureArtifactV1,
    before_canonical_bgra8: &[u8],
    after_checked: &CheckedUntrustedMtgoDxgiCaptureArtifactV1,
    after_canonical_bgra8: &[u8],
) -> Result<CheckedUntrustedMtgoOfflineBottomSixReflowCandidateV1, MtgoContractErrorV1> {
    if before_checked.client_size_px() != after_checked.client_size_px()
        || before_checked.output_identity_sha256() != after_checked.output_identity_sha256()
        || before_checked.captured_at_unix_millis() >= after_checked.captured_at_unix_millis()
        || before_checked.manifest_sha256() == after_checked.manifest_sha256()
        || before_checked.canonical_bgra8_sha256() == after_checked.canonical_bgra8_sha256()
    {
        return Err(error_v1(
            "offline_bottom_six_reflow_source_order",
            "frames must be distinct, strictly ordered, and share one reviewed layout",
        ));
    }

    let before_stage = classify_untrusted_offline_bottom_six_state_candidate_v1(
        before_checked,
        before_canonical_bgra8,
    )?;
    let after_stage = classify_untrusted_offline_bottom_six_state_candidate_v1(
        after_checked,
        after_canonical_bgra8,
    )?;
    let before_selected_count = before_stage.selected_count();
    let after_selected_count = after_stage.selected_count();
    let before_hand_count = before_stage.visible_hand_count();
    let after_hand_count = after_stage.visible_hand_count();

    let stages_are_consecutive = before_stage.classification()
        == MtgoOfflineBottomSixStateClassificationV1::Match
        && after_stage.classification() == MtgoOfflineBottomSixStateClassificationV1::Match
        && before_selected_count
            .is_some_and(|before| after_selected_count == before.checked_add(1) && before < 6)
        && before_hand_count.is_some_and(|before| after_hand_count == before.checked_sub(1));

    let (removed_before_ordinal, matched_distances, passing_count) = if stages_are_consecutive {
        let before_rects = card_art_regions_v1(before_hand_count.unwrap_or(0))?;
        let after_rects = card_art_regions_v1(after_hand_count.unwrap_or(0))?;
        let matrix = pairwise_mean_absolute_difference_milli_v1(
            before_canonical_bgra8,
            after_canonical_bgra8,
            before_checked.client_size_px(),
            &before_rects,
            &after_rects,
        )?;
        select_unique_order_preserving_deletion_v1(
            &matrix,
            MAX_MATCH_MEAN_ABSOLUTE_DIFFERENCE_MILLI_V1,
        )
    } else {
        (None, Vec::new(), 0)
    };
    let classification = if stages_are_consecutive && removed_before_ordinal.is_some() {
        MtgoOfflineBottomSixReflowClassificationV1::Match
    } else {
        MtgoOfflineBottomSixReflowClassificationV1::NoMatch
    };

    let mut hasher = Sha256::new();
    hasher.update(CANDIDATE_DOMAIN_V1);
    for part in [
        before_checked.manifest_sha256().as_bytes(),
        before_checked.canonical_bgra8_sha256().as_bytes(),
        after_checked.manifest_sha256().as_bytes(),
        after_checked.canonical_bgra8_sha256().as_bytes(),
        before_stage.candidate_commitment_sha256().as_bytes(),
        after_stage.candidate_commitment_sha256().as_bytes(),
        match classification {
            MtgoOfflineBottomSixReflowClassificationV1::Match => b"match".as_slice(),
            MtgoOfflineBottomSixReflowClassificationV1::NoMatch => b"no_match".as_slice(),
        },
        &[before_selected_count.unwrap_or(u8::MAX)],
        &[after_selected_count.unwrap_or(u8::MAX)],
        &[removed_before_ordinal.unwrap_or(u8::MAX)],
        &[passing_count],
        &MAX_MATCH_MEAN_ABSOLUTE_DIFFERENCE_MILLI_V1.to_be_bytes(),
    ] {
        update_hash_part_v1(&mut hasher, part);
    }
    for distance in &matched_distances {
        update_hash_part_v1(&mut hasher, &distance.to_be_bytes());
    }

    Ok(CheckedUntrustedMtgoOfflineBottomSixReflowCandidateV1 {
        classification,
        before_manifest_sha256: before_checked.manifest_sha256().to_owned(),
        after_manifest_sha256: after_checked.manifest_sha256().to_owned(),
        before_stage_commitment_sha256: before_stage.candidate_commitment_sha256().to_owned(),
        after_stage_commitment_sha256: after_stage.candidate_commitment_sha256().to_owned(),
        before_selected_count,
        after_selected_count,
        removed_before_ordinal,
        matched_pair_mean_absolute_difference_milli: matched_distances,
        passing_deletion_candidate_count: passing_count,
        candidate_commitment_sha256: format!("{:x}", hasher.finalize()),
    })
}

fn card_art_regions_v1(hand_count: u8) -> Result<Vec<MtgoRectPxV1>, MtgoContractErrorV1> {
    let left_edges: Vec<u32> = match hand_count {
        7 => vec![323, 442, 561, 679, 798, 917, 1_035],
        1..=6 => (0..u32::from(hand_count))
            .map(|index| 323 + 128 * index)
            .collect(),
        _ => {
            return Err(error_v1(
                "offline_bottom_six_reflow_hand_count",
                "hand count must be between one and seven",
            ))
        }
    };
    Ok(left_edges
        .into_iter()
        .map(|left| MtgoRectPxV1 {
            x: left + 7,
            y: 760,
            width: 96,
            height: 60,
        })
        .collect())
}

fn pairwise_mean_absolute_difference_milli_v1(
    before_pixels: &[u8],
    after_pixels: &[u8],
    size: &MtgoSizePxV1,
    before_rects: &[MtgoRectPxV1],
    after_rects: &[MtgoRectPxV1],
) -> Result<Vec<Vec<u32>>, MtgoContractErrorV1> {
    let expected = pixel_len_v1(size)?;
    if before_pixels.len() != expected || after_pixels.len() != expected {
        return Err(error_v1(
            "offline_bottom_six_reflow_pixels",
            "both pixel buffers must match the reviewed frame size",
        ));
    }
    before_rects
        .iter()
        .map(|before_rect| {
            after_rects
                .iter()
                .map(|after_rect| {
                    mean_absolute_difference_milli_v1(
                        before_pixels,
                        after_pixels,
                        size,
                        before_rect,
                        after_rect,
                    )
                })
                .collect()
        })
        .collect()
}

fn mean_absolute_difference_milli_v1(
    before_pixels: &[u8],
    after_pixels: &[u8],
    size: &MtgoSizePxV1,
    before_rect: &MtgoRectPxV1,
    after_rect: &MtgoRectPxV1,
) -> Result<u32, MtgoContractErrorV1> {
    if before_rect.width != after_rect.width || before_rect.height != after_rect.height {
        return Err(error_v1(
            "offline_bottom_six_reflow_geometry",
            "paired card regions must have identical dimensions",
        ));
    }
    validate_region_v1(size, before_rect)?;
    validate_region_v1(size, after_rect)?;
    let stride = usize::try_from(size.width)
        .map_err(|_| error_v1("offline_bottom_six_reflow_pixels", "frame width overflow"))?;
    let mut total = 0_u64;
    for row in 0..before_rect.height {
        for column in 0..before_rect.width {
            let before_offset =
                pixel_offset_v1(stride, before_rect.x + column, before_rect.y + row)?;
            let after_offset = pixel_offset_v1(stride, after_rect.x + column, after_rect.y + row)?;
            for channel in 0..3 {
                total += u64::from(before_pixels[before_offset + channel])
                    .abs_diff(u64::from(after_pixels[after_offset + channel]));
            }
        }
    }
    let samples = u64::from(before_rect.width) * u64::from(before_rect.height) * 3;
    u32::try_from((total * 1_000 + samples / 2) / samples).map_err(|_| {
        error_v1(
            "offline_bottom_six_reflow_distance",
            "mean absolute difference overflow",
        )
    })
}

fn select_unique_order_preserving_deletion_v1(
    matrix: &[Vec<u32>],
    threshold: u32,
) -> (Option<u8>, Vec<u32>, u8) {
    if matrix.len() < 2
        || matrix.len() > 7
        || matrix.iter().any(|row| row.len() + 1 != matrix.len())
    {
        return (None, Vec::new(), 0);
    }
    let mut passing = Vec::new();
    for deleted in 0..matrix.len() {
        let distances: Vec<_> = (0..matrix.len() - 1)
            .map(|after| {
                let before = if after < deleted { after } else { after + 1 };
                matrix[before][after]
            })
            .collect();
        if distances.iter().all(|distance| *distance <= threshold) {
            passing.push((deleted, distances));
        }
    }
    let passing_count = u8::try_from(passing.len()).unwrap_or(u8::MAX);
    if passing.len() == 1 {
        let (deleted, distances) = passing.remove(0);
        (u8::try_from(deleted).ok(), distances, passing_count)
    } else {
        (None, Vec::new(), passing_count)
    }
}

fn pixel_len_v1(size: &MtgoSizePxV1) -> Result<usize, MtgoContractErrorV1> {
    usize::try_from(size.width)
        .ok()
        .and_then(|width| {
            usize::try_from(size.height)
                .ok()
                .and_then(|height| width.checked_mul(height))
        })
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or_else(|| error_v1("offline_bottom_six_reflow_pixels", "pixel length overflow"))
}

fn pixel_offset_v1(stride: usize, x: u32, y: u32) -> Result<usize, MtgoContractErrorV1> {
    usize::try_from(y)
        .ok()
        .and_then(|y| y.checked_mul(stride))
        .and_then(|row| usize::try_from(x).ok().and_then(|x| row.checked_add(x)))
        .and_then(|pixel| pixel.checked_mul(4))
        .ok_or_else(|| error_v1("offline_bottom_six_reflow_pixels", "pixel offset overflow"))
}

fn validate_region_v1(size: &MtgoSizePxV1, rect: &MtgoRectPxV1) -> Result<(), MtgoContractErrorV1> {
    if rect.width == 0
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
        return Err(error_v1(
            "offline_bottom_six_reflow_geometry",
            "card region is outside the reviewed client",
        ));
    }
    Ok(())
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
    fn canonical_card_regions_follow_the_reviewed_reflow_layout() {
        let seven = card_art_regions_v1(7).unwrap();
        assert_eq!(seven.len(), 7);
        assert_eq!(seven[0].x, 330);
        assert_eq!(seven[6].x, 1_042);
        let six = card_art_regions_v1(6).unwrap();
        assert_eq!(six.len(), 6);
        assert_eq!(six[0].x, 330);
        assert_eq!(six[5].x, 970);
        assert!(card_art_regions_v1(0).is_err());
    }

    #[test]
    fn exactly_one_order_preserving_deletion_is_selected() {
        let delete_first = vec![
            vec![80_000, 90_000],
            vec![1_000, 70_000],
            vec![75_000, 2_000],
        ];
        assert_eq!(
            select_unique_order_preserving_deletion_v1(&delete_first, 25_000),
            (Some(0), vec![1_000, 2_000], 1)
        );
        let delete_middle = vec![
            vec![1_000, 90_000],
            vec![80_000, 70_000],
            vec![75_000, 2_000],
        ];
        assert_eq!(
            select_unique_order_preserving_deletion_v1(&delete_middle, 25_000),
            (Some(1), vec![1_000, 2_000], 1)
        );
    }

    #[test]
    fn absent_or_ambiguous_visual_deletions_fail_closed() {
        let none = vec![vec![30_000], vec![40_000]];
        assert_eq!(
            select_unique_order_preserving_deletion_v1(&none, 25_000),
            (None, Vec::new(), 0)
        );
        let ambiguous_duplicates = vec![vec![1_000], vec![1_000]];
        assert_eq!(
            select_unique_order_preserving_deletion_v1(&ambiguous_duplicates, 25_000),
            (None, Vec::new(), 2)
        );
        assert_eq!(
            select_unique_order_preserving_deletion_v1(&[vec![1_000]], 25_000),
            (None, Vec::new(), 0)
        );
    }
}
