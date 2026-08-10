use crate::capture_contract::{preview_utc_instant_v1, PreviewUtcInstantV1};
use crate::offline_bottom_six_reflow::{
    card_art_regions_v1, pairwise_mean_absolute_difference_milli_v1,
    select_unique_order_preserving_deletion_v1,
};
use crate::offline_visible_card_identity::first_main_card_art_regions_v1;
use crate::{
    CheckedUntrustedMtgoVisibleObjectActionCalibrationV1, MtgoContractErrorV1, MtgoSizePxV1,
    MtgoVisibleObjectCalibrationActionV1,
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::io::Cursor;

const CANDIDATE_DOMAIN_V1: &[u8] = b"mtgo-offline-play-land-hand-reflow-v1";
const SOURCE_ID_PREFIX_V1: &str = "before-frame:hand-slot-";
const BEFORE_HAND_COUNT_V1: u8 = 8;
const AFTER_HAND_COUNT_V1: u8 = 7;
// The retained 1550x925 legacy preview pair needs 26_989 for its widest
// surviving-card correspondence. The next deletion hypothesis needs 66_022.
// This ceiling is scoped to this exact legacy layout and remains offline only.
const MAX_MATCH_MEAN_ABSOLUTE_DIFFERENCE_MILLI_V1: u32 = 30_000;
const MIN_DELETION_HYPOTHESIS_MARGIN_MILLI_V1: u32 = 10_000;
const EXPECTED_CLIENT_SIZE_V1: MtgoSizePxV1 = MtgoSizePxV1 {
    width: 1_550,
    height: 925,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MtgoOfflinePlayLandHandReflowClassificationV1 {
    Match,
    NoMatch,
}

/// A checked-untrusted offline measurement of one retained PlayLand hand
/// reflow.
///
/// A Match requires the exact manifest and PNG bytes named by the checked
/// calibration, two strictly ordered player-visible Solitaire previews, and
/// exactly one order-preserving 8-to-7 card-art deletion below the fixed
/// visual-distance ceiling. The unique visual deletion must equal the source
/// slot declared by the PlayLand calibration.
///
/// The result retains no pixels or coordinates and grants no observation,
/// semantic-evidence, scoring, or input authority.
///
/// ```compile_fail
/// use mtgo_blackbox_v1::CheckedUntrustedMtgoOfflinePlayLandHandReflowV1;
/// fn pixels_cannot_escape(value: &CheckedUntrustedMtgoOfflinePlayLandHandReflowV1) {
///     let _ = value.canonical_bgra8_v1();
/// }
/// ```
///
/// ```compile_fail
/// use mtgo_blackbox_v1::CheckedUntrustedMtgoOfflinePlayLandHandReflowV1;
/// fn coordinates_cannot_escape(value: &CheckedUntrustedMtgoOfflinePlayLandHandReflowV1) {
///     let _ = value.card_art_regions_v1();
/// }
/// ```
pub struct CheckedUntrustedMtgoOfflinePlayLandHandReflowV1 {
    classification: MtgoOfflinePlayLandHandReflowClassificationV1,
    calibration_commitment_sha256: String,
    before_manifest_sha256: String,
    before_frame_sha256: String,
    after_manifest_sha256: String,
    after_frame_sha256: String,
    expected_source_ordinal: u8,
    unique_visual_removed_ordinal: Option<u8>,
    matched_pair_mean_absolute_difference_milli: Vec<u32>,
    passing_deletion_candidate_count: u8,
    runner_up_deletion_maximum_difference_milli: Option<u32>,
    deletion_hypothesis_margin_milli: Option<u32>,
    candidate_commitment_sha256: String,
}

impl CheckedUntrustedMtgoOfflinePlayLandHandReflowV1 {
    pub fn classification(&self) -> MtgoOfflinePlayLandHandReflowClassificationV1 {
        self.classification
    }

    pub fn calibration_commitment_sha256(&self) -> &str {
        &self.calibration_commitment_sha256
    }

    pub fn before_manifest_sha256(&self) -> &str {
        &self.before_manifest_sha256
    }

    pub fn before_frame_sha256(&self) -> &str {
        &self.before_frame_sha256
    }

    pub fn after_manifest_sha256(&self) -> &str {
        &self.after_manifest_sha256
    }

    pub fn after_frame_sha256(&self) -> &str {
        &self.after_frame_sha256
    }

    pub fn before_hand_count(&self) -> u8 {
        BEFORE_HAND_COUNT_V1
    }

    pub fn after_hand_count(&self) -> u8 {
        AFTER_HAND_COUNT_V1
    }

    pub fn expected_source_ordinal(&self) -> u8 {
        self.expected_source_ordinal
    }

    pub fn unique_visual_removed_ordinal(&self) -> Option<u8> {
        self.unique_visual_removed_ordinal
    }

    pub fn matched_pair_mean_absolute_difference_milli(&self) -> &[u32] {
        &self.matched_pair_mean_absolute_difference_milli
    }

    pub fn passing_deletion_candidate_count(&self) -> u8 {
        self.passing_deletion_candidate_count
    }

    pub fn runner_up_deletion_maximum_difference_milli(&self) -> Option<u32> {
        self.runner_up_deletion_maximum_difference_milli
    }

    pub fn deletion_hypothesis_margin_milli(&self) -> Option<u32> {
        self.deletion_hypothesis_margin_milli
    }

    pub fn minimum_deletion_hypothesis_margin_milli(&self) -> u32 {
        MIN_DELETION_HYPOTHESIS_MARGIN_MILLI_V1
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

pub fn classify_untrusted_offline_play_land_hand_reflow_v1(
    calibration: &CheckedUntrustedMtgoVisibleObjectActionCalibrationV1,
    before_manifest_json: &[u8],
    before_frame_png: &[u8],
    after_manifest_json: &[u8],
    after_frame_png: &[u8],
) -> Result<CheckedUntrustedMtgoOfflinePlayLandHandReflowV1, MtgoContractErrorV1> {
    if calibration.client_size_px() != &EXPECTED_CLIENT_SIZE_V1 {
        return Err(error_v1(
            "offline_play_land_reflow_layout",
            "v1 requires the reviewed 1550x925 first-main layout",
        ));
    }
    let expected_source_ordinal = match calibration.action() {
        MtgoVisibleObjectCalibrationActionV1::PlayLand {
            source_adapter_object_id,
            ..
        } => parse_source_ordinal_v1(source_adapter_object_id)?,
        MtgoVisibleObjectCalibrationActionV1::ActivateManaAbility { .. } => {
            return Err(error_v1(
                "offline_play_land_reflow_action",
                "the calibration action must be PlayLand",
            ));
        }
    };

    let before = check_endpoint_v1(
        before_manifest_json,
        before_frame_png,
        calibration.before_manifest_sha256(),
        calibration.before_frame_sha256(),
        calibration.client_size_px(),
    )?;
    let after = check_endpoint_v1(
        after_manifest_json,
        after_frame_png,
        calibration.after_manifest_sha256(),
        calibration.after_frame_sha256(),
        calibration.client_size_px(),
    )?;
    if before.captured_at >= after.captured_at
        || before.process_identity_sha256 != after.process_identity_sha256
        || before.layout_identity_sha256 != after.layout_identity_sha256
    {
        return Err(error_v1(
            "offline_play_land_reflow_source_order",
            "endpoints must be strictly ordered and share one process and visible layout",
        ));
    }

    let before_rects = first_main_card_art_regions_v1();
    let after_rects = card_art_regions_v1(AFTER_HAND_COUNT_V1)?;
    let matrix = pairwise_mean_absolute_difference_milli_v1(
        &before.canonical_bgra8,
        &after.canonical_bgra8,
        calibration.client_size_px(),
        &before_rects,
        &after_rects,
    )?;
    let (unique_visual_removed_ordinal, matched_distances, passing_count) =
        select_unique_order_preserving_deletion_v1(
            &matrix,
            MAX_MATCH_MEAN_ABSOLUTE_DIFFERENCE_MILLI_V1,
        );
    let (runner_up_deletion_maximum_difference_milli, deletion_hypothesis_margin_milli) =
        deletion_hypothesis_margin_v1(&matrix, unique_visual_removed_ordinal, &matched_distances);
    let classification = if unique_visual_removed_ordinal == Some(expected_source_ordinal)
        && deletion_hypothesis_margin_milli
            .is_some_and(|margin| margin >= MIN_DELETION_HYPOTHESIS_MARGIN_MILLI_V1)
    {
        MtgoOfflinePlayLandHandReflowClassificationV1::Match
    } else {
        MtgoOfflinePlayLandHandReflowClassificationV1::NoMatch
    };

    let mut hasher = Sha256::new();
    hasher.update(CANDIDATE_DOMAIN_V1);
    for part in [
        calibration.transition_commitment_sha256().as_bytes(),
        calibration.before_manifest_sha256().as_bytes(),
        calibration.before_frame_sha256().as_bytes(),
        before.canonical_bgra8_sha256.as_bytes(),
        calibration.after_manifest_sha256().as_bytes(),
        calibration.after_frame_sha256().as_bytes(),
        after.canonical_bgra8_sha256.as_bytes(),
        match classification {
            MtgoOfflinePlayLandHandReflowClassificationV1::Match => b"match".as_slice(),
            MtgoOfflinePlayLandHandReflowClassificationV1::NoMatch => b"no_match".as_slice(),
        },
        &[expected_source_ordinal],
        &[unique_visual_removed_ordinal.unwrap_or(u8::MAX)],
        &[passing_count],
        &MAX_MATCH_MEAN_ABSOLUTE_DIFFERENCE_MILLI_V1.to_be_bytes(),
        &MIN_DELETION_HYPOTHESIS_MARGIN_MILLI_V1.to_be_bytes(),
        &runner_up_deletion_maximum_difference_milli
            .unwrap_or(u32::MAX)
            .to_be_bytes(),
        &deletion_hypothesis_margin_milli
            .unwrap_or(u32::MAX)
            .to_be_bytes(),
    ] {
        update_hash_part_v1(&mut hasher, part);
    }
    for distance in &matched_distances {
        update_hash_part_v1(&mut hasher, &distance.to_be_bytes());
    }

    Ok(CheckedUntrustedMtgoOfflinePlayLandHandReflowV1 {
        classification,
        calibration_commitment_sha256: calibration.transition_commitment_sha256().to_owned(),
        before_manifest_sha256: calibration.before_manifest_sha256().to_owned(),
        before_frame_sha256: calibration.before_frame_sha256().to_owned(),
        after_manifest_sha256: calibration.after_manifest_sha256().to_owned(),
        after_frame_sha256: calibration.after_frame_sha256().to_owned(),
        expected_source_ordinal,
        unique_visual_removed_ordinal,
        matched_pair_mean_absolute_difference_milli: matched_distances,
        passing_deletion_candidate_count: passing_count,
        runner_up_deletion_maximum_difference_milli,
        deletion_hypothesis_margin_milli,
        candidate_commitment_sha256: format!("{:x}", hasher.finalize()),
    })
}

fn deletion_hypothesis_margin_v1(
    matrix: &[Vec<u32>],
    selected_ordinal: Option<u8>,
    selected_distances: &[u32],
) -> (Option<u32>, Option<u32>) {
    let Some(selected_ordinal) = selected_ordinal.map(usize::from) else {
        return (None, None);
    };
    let Some(selected_maximum) = selected_distances.iter().copied().max() else {
        return (None, None);
    };
    if matrix.len() != 8 || matrix.iter().any(|row| row.len() != 7) {
        return (None, None);
    }
    let runner_up = (0..matrix.len())
        .filter(|deleted| *deleted != selected_ordinal)
        .filter_map(|deleted| {
            (0..matrix.len() - 1)
                .map(|after| {
                    let before = if after < deleted { after } else { after + 1 };
                    matrix[before][after]
                })
                .max()
        })
        .min();
    (
        runner_up,
        runner_up.map(|runner_up| runner_up.saturating_sub(selected_maximum)),
    )
}

struct CheckedEndpointV1 {
    captured_at: PreviewUtcInstantV1,
    process_identity_sha256: String,
    layout_identity_sha256: String,
    canonical_bgra8_sha256: String,
    canonical_bgra8: Vec<u8>,
}

fn check_endpoint_v1(
    manifest_json: &[u8],
    frame_png: &[u8],
    expected_manifest_sha256: &str,
    expected_frame_sha256: &str,
    size: &MtgoSizePxV1,
) -> Result<CheckedEndpointV1, MtgoContractErrorV1> {
    if sha256_v1(manifest_json) != expected_manifest_sha256 {
        return Err(error_v1(
            "offline_play_land_reflow_manifest_hash",
            "manifest bytes do not match the calibration",
        ));
    }
    if sha256_v1(frame_png) != expected_frame_sha256 {
        return Err(error_v1(
            "offline_play_land_reflow_frame_hash",
            "PNG bytes do not match the calibration",
        ));
    }
    let expected_len = usize::try_from(size.width)
        .ok()
        .and_then(|width| {
            usize::try_from(size.height)
                .ok()
                .and_then(|height| width.checked_mul(height))
        })
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or_else(|| error_v1("offline_play_land_reflow_pixels", "pixel length overflow"))?;
    let canonical_bgra8 =
        decode_legacy_gdi_preview_png_to_canonical_bgra8_v1(frame_png, size, expected_len)?;
    let canonical_bgra8_sha256 = sha256_v1(&canonical_bgra8);

    let manifest: Value = serde_json::from_slice(manifest_json)
        .map_err(|error| error_v1("offline_play_land_reflow_manifest_json", error.to_string()))?;
    require_u64_v1(&manifest, &["schema_version"], 1)?;
    require_string_v1(
        &manifest,
        &["artifact_kind"],
        "mtgo_visible_solitaire_gameplay_calibration_preview_v1",
    )?;
    require_string_v1(&manifest, &["status"], "pending_visual_review")?;
    require_string_v1(
        &manifest,
        &["capture_backend"],
        "system_drawing_copy_from_composed_screen_v1",
    )?;
    require_string_v1(
        &manifest,
        &["pixel_source"],
        "visible_desktop_client_crop_only",
    )?;
    if !json_value_v1(&manifest, &["profile_id"])?.is_null() {
        return Err(error_v1(
            "offline_play_land_reflow_manifest_value",
            "profile_id must remain null for this pending preview",
        ));
    }
    for field in [
        "safe_for_semantic_evidence",
        "safe_for_ocr",
        "safe_for_policy_scoring",
        "safe_for_input",
    ] {
        require_bool_v1(&manifest, &[field], false)?;
    }

    let captured_at = preview_utc_instant_v1(
        "offline_play_land_reflow_capture_time",
        json_string_v1(&manifest, &["captured_at_utc"])?,
    )?;
    require_string_v1(
        &manifest,
        &["expected_identity", "target_window_mode"],
        "ForegroundSolitaireGame",
    )?;
    require_string_v1(
        &manifest,
        &["expected_identity", "capture_role"],
        "acting_player_solitaire",
    )?;
    require_string_v1(
        &manifest,
        &["window", "target_window_mode"],
        "ForegroundSolitaireGame",
    )?;
    require_string_v1(
        &manifest,
        &["window", "capture_role"],
        "acting_player_solitaire",
    )?;
    require_bool_v1(&manifest, &["window", "foreground"], true)?;
    require_bool_v1(&manifest, &["window", "visible"], true)?;
    require_bool_v1(&manifest, &["window", "minimized"], false)?;
    require_bool_v1(&manifest, &["window", "cloaked"], false)?;
    require_u64_v1(&manifest, &["window", "display_affinity"], 0)?;
    require_bool_v1(&manifest, &["window", "desktop_composition_enabled"], true)?;
    require_bool_v1(&manifest, &["occlusion_audit", "target_found"], true)?;
    require_u64_v1(
        &manifest,
        &["occlusion_audit", "intersecting_windows_above_target"],
        0,
    )?;
    require_bool_v1(
        &manifest,
        &["occlusion_audit", "cursor_inside_client"],
        false,
    )?;
    require_string_v1(&manifest, &["frame", "file"], "frame.png")?;
    require_string_v1(&manifest, &["frame", "format"], "png")?;
    require_u64_v1(&manifest, &["frame", "width"], u64::from(size.width))?;
    require_u64_v1(&manifest, &["frame", "height"], u64::from(size.height))?;
    if !json_string_v1(&manifest, &["frame", "sha256"])?.eq_ignore_ascii_case(expected_frame_sha256)
    {
        return Err(error_v1(
            "offline_play_land_reflow_manifest_frame_hash",
            "manifest frame hash does not match the calibration",
        ));
    }

    let client_width = json_u64_v1(&manifest, &["window", "client_bounds_desktop_px", "width"])?;
    let client_height = json_u64_v1(&manifest, &["window", "client_bounds_desktop_px", "height"])?;
    if client_width != u64::from(size.width) || client_height != u64::from(size.height) {
        return Err(error_v1(
            "offline_play_land_reflow_client_size",
            "manifest client dimensions do not match the calibration",
        ));
    }
    let process_id = json_u64_v1(&manifest, &["observed_identity", "process_id"])?;
    if process_id == 0 {
        return Err(error_v1(
            "offline_play_land_reflow_process_id",
            "process ID must be positive",
        ));
    }
    let process_start = json_string_v1(&manifest, &["observed_identity", "process_start_utc"])?;
    let process_started_at =
        preview_utc_instant_v1("offline_play_land_reflow_process_start", process_start)?;
    if process_started_at > captured_at {
        return Err(error_v1(
            "offline_play_land_reflow_process_time",
            "process start must not follow capture",
        ));
    }

    let process_identity_sha256 = commitment_v1(
        b"mtgo-offline-play-land-process-identity-v1",
        &[
            process_id.to_string().as_bytes(),
            process_start.as_bytes(),
            json_string_v1(&manifest, &["observed_identity", "product_version"])?.as_bytes(),
            json_string_v1(&manifest, &["observed_identity", "file_version"])?.as_bytes(),
            json_string_v1(&manifest, &["observed_identity", "executable_sha256"])?.as_bytes(),
            json_string_v1(&manifest, &["observed_identity", "signer_thumbprint"])?.as_bytes(),
            json_string_v1(&manifest, &["observed_identity", "signer_subject"])?.as_bytes(),
        ],
    );
    let layout_identity_sha256 = commitment_v1(
        b"mtgo-offline-play-land-layout-identity-v1",
        &[
            json_string_v1(&manifest, &["window", "title"])?.as_bytes(),
            json_u64_v1(&manifest, &["window", "dpi"])?
                .to_string()
                .as_bytes(),
            json_i64_v1(&manifest, &["window", "client_bounds_desktop_px", "left"])?
                .to_string()
                .as_bytes(),
            json_i64_v1(&manifest, &["window", "client_bounds_desktop_px", "top"])?
                .to_string()
                .as_bytes(),
            client_width.to_string().as_bytes(),
            client_height.to_string().as_bytes(),
            json_string_v1(&manifest, &["monitor", "device_name"])?.as_bytes(),
            json_i64_v1(&manifest, &["monitor", "bounds_desktop_px", "left"])?
                .to_string()
                .as_bytes(),
            json_i64_v1(&manifest, &["monitor", "bounds_desktop_px", "top"])?
                .to_string()
                .as_bytes(),
            json_u64_v1(&manifest, &["monitor", "bounds_desktop_px", "width"])?
                .to_string()
                .as_bytes(),
            json_u64_v1(&manifest, &["monitor", "bounds_desktop_px", "height"])?
                .to_string()
                .as_bytes(),
        ],
    );

    Ok(CheckedEndpointV1 {
        captured_at,
        process_identity_sha256,
        layout_identity_sha256,
        canonical_bgra8_sha256,
        canonical_bgra8,
    })
}

fn parse_source_ordinal_v1(value: &str) -> Result<u8, MtgoContractErrorV1> {
    let suffix = value.strip_prefix(SOURCE_ID_PREFIX_V1).ok_or_else(|| {
        error_v1(
            "offline_play_land_reflow_source_id",
            "source ID must identify one canonical before-frame hand slot",
        )
    })?;
    let ordinal = suffix.parse::<u8>().map_err(|_| {
        error_v1(
            "offline_play_land_reflow_source_id",
            "source hand-slot ordinal is invalid",
        )
    })?;
    if ordinal >= BEFORE_HAND_COUNT_V1 || suffix != ordinal.to_string() {
        return Err(error_v1(
            "offline_play_land_reflow_source_id",
            "source hand-slot ordinal must be canonical and between zero and seven",
        ));
    }
    Ok(ordinal)
}

fn decode_legacy_gdi_preview_png_to_canonical_bgra8_v1(
    frame_png: &[u8],
    expected_size: &MtgoSizePxV1,
    expected_len: usize,
) -> Result<Vec<u8>, MtgoContractErrorV1> {
    let mut decoder = png::Decoder::new(Cursor::new(frame_png));
    decoder.set_transformations(png::Transformations::IDENTITY);
    let mut reader = decoder
        .read_info()
        .map_err(|error| error_v1("offline_play_land_reflow_png_decode", error.to_string()))?;
    let (width, height, color_type, bit_depth, interlaced, animated) = {
        let info = reader.info();
        (
            info.width,
            info.height,
            info.color_type,
            info.bit_depth,
            info.interlaced,
            info.animation_control.is_some(),
        )
    };
    if width != expected_size.width || height != expected_size.height {
        return Err(error_v1(
            "offline_play_land_reflow_png_dimensions",
            "PNG dimensions do not match the calibration",
        ));
    }
    if interlaced || animated || bit_depth != png::BitDepth::Eight {
        return Err(error_v1(
            "offline_play_land_reflow_png_format",
            "legacy preview must be one non-interlaced 8-bit RGB or RGBA PNG",
        ));
    }
    let channels = match color_type {
        png::ColorType::Rgb => 3_usize,
        png::ColorType::Rgba => 4_usize,
        _ => {
            return Err(error_v1(
                "offline_play_land_reflow_png_format",
                "legacy preview must use RGB or RGBA pixels",
            ));
        }
    };
    let source_len = usize::try_from(width)
        .ok()
        .and_then(|width| {
            usize::try_from(height)
                .ok()
                .and_then(|height| width.checked_mul(height))
        })
        .and_then(|pixels| pixels.checked_mul(channels))
        .ok_or_else(|| {
            error_v1(
                "offline_play_land_reflow_png_dimensions",
                "decoded PNG length overflow",
            )
        })?;
    let mut source = vec![0_u8; source_len];
    let frame = reader
        .next_frame(&mut source)
        .map_err(|error| error_v1("offline_play_land_reflow_png_decode", error.to_string()))?;
    if frame.width != width
        || frame.height != height
        || frame.color_type != color_type
        || frame.bit_depth != bit_depth
        || frame.buffer_size() != source_len
    {
        return Err(error_v1(
            "offline_play_land_reflow_png_format",
            "decoded PNG frame does not match its header",
        ));
    }

    let mut canonical = Vec::with_capacity(expected_len);
    for pixel in source.chunks_exact(channels) {
        canonical.extend_from_slice(&[pixel[2], pixel[1], pixel[0], 255]);
    }
    if canonical.len() != expected_len {
        return Err(error_v1(
            "offline_play_land_reflow_png_dimensions",
            "decoded PNG length does not match the calibration",
        ));
    }
    Ok(canonical)
}

fn json_value_v1<'a>(value: &'a Value, path: &[&str]) -> Result<&'a Value, MtgoContractErrorV1> {
    let mut current = value;
    for segment in path {
        current = current.get(*segment).ok_or_else(|| {
            error_v1(
                "offline_play_land_reflow_manifest_field",
                format!("missing field {}", path.join(".")),
            )
        })?;
    }
    Ok(current)
}

fn json_string_v1<'a>(value: &'a Value, path: &[&str]) -> Result<&'a str, MtgoContractErrorV1> {
    json_value_v1(value, path)?.as_str().ok_or_else(|| {
        error_v1(
            "offline_play_land_reflow_manifest_type",
            format!("field {} must be a string", path.join(".")),
        )
    })
}

fn json_u64_v1(value: &Value, path: &[&str]) -> Result<u64, MtgoContractErrorV1> {
    json_value_v1(value, path)?.as_u64().ok_or_else(|| {
        error_v1(
            "offline_play_land_reflow_manifest_type",
            format!("field {} must be an unsigned integer", path.join(".")),
        )
    })
}

fn json_i64_v1(value: &Value, path: &[&str]) -> Result<i64, MtgoContractErrorV1> {
    json_value_v1(value, path)?.as_i64().ok_or_else(|| {
        error_v1(
            "offline_play_land_reflow_manifest_type",
            format!("field {} must be an integer", path.join(".")),
        )
    })
}

fn json_bool_v1(value: &Value, path: &[&str]) -> Result<bool, MtgoContractErrorV1> {
    json_value_v1(value, path)?.as_bool().ok_or_else(|| {
        error_v1(
            "offline_play_land_reflow_manifest_type",
            format!("field {} must be a boolean", path.join(".")),
        )
    })
}

fn require_string_v1(
    value: &Value,
    path: &[&str],
    expected: &str,
) -> Result<(), MtgoContractErrorV1> {
    if json_string_v1(value, path)? != expected {
        return Err(error_v1(
            "offline_play_land_reflow_manifest_value",
            format!("field {} has an unexpected value", path.join(".")),
        ));
    }
    Ok(())
}

fn require_u64_v1(value: &Value, path: &[&str], expected: u64) -> Result<(), MtgoContractErrorV1> {
    if json_u64_v1(value, path)? != expected {
        return Err(error_v1(
            "offline_play_land_reflow_manifest_value",
            format!("field {} has an unexpected value", path.join(".")),
        ));
    }
    Ok(())
}

fn require_bool_v1(
    value: &Value,
    path: &[&str],
    expected: bool,
) -> Result<(), MtgoContractErrorV1> {
    if json_bool_v1(value, path)? != expected {
        return Err(error_v1(
            "offline_play_land_reflow_manifest_value",
            format!("field {} has an unexpected value", path.join(".")),
        ));
    }
    Ok(())
}

fn commitment_v1(domain: &[u8], parts: &[&[u8]]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(domain);
    for part in parts {
        update_hash_part_v1(&mut hasher, part);
    }
    format!("{:x}", hasher.finalize())
}

fn update_hash_part_v1(hasher: &mut Sha256, part: &[u8]) {
    hasher.update((part.len() as u64).to_be_bytes());
    hasher.update(part);
}

fn sha256_v1(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn error_v1(code: &'static str, detail: impl Into<String>) -> MtgoContractErrorV1 {
    MtgoContractErrorV1::new(code, detail)
}

#[cfg(test)]
pub(crate) fn matched_offline_play_land_reflow_for_test_v1(
    calibration: &CheckedUntrustedMtgoVisibleObjectActionCalibrationV1,
) -> CheckedUntrustedMtgoOfflinePlayLandHandReflowV1 {
    let source_ordinal = match calibration.action() {
        MtgoVisibleObjectCalibrationActionV1::PlayLand {
            source_adapter_object_id,
            ..
        } => parse_source_ordinal_v1(source_adapter_object_id).unwrap(),
        MtgoVisibleObjectCalibrationActionV1::ActivateManaAbility { .. } => {
            panic!("test calibration must be PlayLand")
        }
    };
    CheckedUntrustedMtgoOfflinePlayLandHandReflowV1 {
        classification: MtgoOfflinePlayLandHandReflowClassificationV1::Match,
        calibration_commitment_sha256: calibration.transition_commitment_sha256().to_owned(),
        before_manifest_sha256: calibration.before_manifest_sha256().to_owned(),
        before_frame_sha256: calibration.before_frame_sha256().to_owned(),
        after_manifest_sha256: calibration.after_manifest_sha256().to_owned(),
        after_frame_sha256: calibration.after_frame_sha256().to_owned(),
        expected_source_ordinal: source_ordinal,
        unique_visual_removed_ordinal: Some(source_ordinal),
        matched_pair_mean_absolute_difference_milli: vec![0; 7],
        passing_deletion_candidate_count: 1,
        runner_up_deletion_maximum_difference_milli: Some(90_000),
        deletion_hypothesis_margin_milli: Some(90_000),
        candidate_commitment_sha256: "a".repeat(64),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_source_slot_is_bounded() {
        assert_eq!(
            parse_source_ordinal_v1("before-frame:hand-slot-0").unwrap(),
            0
        );
        assert_eq!(
            parse_source_ordinal_v1("before-frame:hand-slot-7").unwrap(),
            7
        );
        for invalid in [
            "before-frame:hand-slot-08",
            "before-frame:hand-slot-8",
            "before-frame:hand-slot-x",
            "after-frame:hand-slot-0",
        ] {
            assert!(parse_source_ordinal_v1(invalid).is_err(), "{invalid}");
        }
    }

    #[test]
    fn deletion_margin_compares_the_selected_path_to_the_best_alternative() {
        let mut matrix = vec![vec![90_000; 7]; 8];
        let selected = [15_278, 13_018, 13_295, 26_989, 16_524, 17_821, 22_899];
        for (after, distance) in selected.into_iter().enumerate() {
            matrix[after + 1][after] = distance;
        }
        matrix[0][0] = 66_022;
        assert_eq!(
            deletion_hypothesis_margin_v1(&matrix, Some(0), &selected),
            (Some(66_022), Some(39_033))
        );
    }

    #[test]
    fn legacy_decoder_normalizes_stored_alpha_without_weakening_the_live_decoder() {
        let mut png_bytes = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut png_bytes, 1, 1);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().unwrap();
            writer.write_image_data(&[10, 20, 30, 0]).unwrap();
        }
        assert_eq!(
            decode_legacy_gdi_preview_png_to_canonical_bgra8_v1(
                &png_bytes,
                &MtgoSizePxV1 {
                    width: 1,
                    height: 1,
                },
                4,
            )
            .unwrap(),
            [30, 20, 10, 255]
        );
    }
}
