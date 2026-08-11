#[cfg(not(target_os = "windows"))]
compile_error!("mtgo_visible_competitive_classifier_v1 is Windows-only");

#[cfg(target_os = "windows")]
use mtgo_blackbox_v1::{
    visible_frame_region_content_sha256_v1, MtgoCompetitiveEventKindV1,
    MtgoCompetitiveEventListingTargetV1, MtgoRectPxV1, MtgoSizePxV1,
    MtgoVisibleCompetitiveEventListingSelectionV1, MTGO_COMPETITIVE_EVENT_LISTING_SCHEMA_V1,
};
#[cfg(target_os = "windows")]
use mtgo_dxgi_capture_v1::{
    MtgoCompetitiveEventListingClassifierProcessResponseV1,
    MtgoCompetitiveEventListingClassifierRequestHeaderV1,
};
#[cfg(target_os = "windows")]
use serde::{Deserialize, Serialize};
#[cfg(target_os = "windows")]
use sha2::{Digest, Sha256};
#[cfg(target_os = "windows")]
use std::{
    collections::HashSet,
    fs::File,
    io::{self, Read, Write},
    path::Path,
    process::ExitCode,
};
#[cfg(target_os = "windows")]
use windows::{
    Graphics::Imaging::{BitmapPixelFormat, SoftwareBitmap},
    Media::Ocr::OcrEngine,
    Storage::Streams::DataWriter,
    Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED},
};

#[cfg(target_os = "windows")]
const MODE_ARGUMENT_V1: &str = "--mtgo-visible-competitive-event-listing-v1";
#[cfg(target_os = "windows")]
const PROTOCOL_MAGIC_V1: &[u8] = b"MTGO_VISIBLE_COMPETITIVE_EVENT_LISTING_V1\0";
#[cfg(target_os = "windows")]
const REQUEST_PROTOCOL_V1: &str = "mtgo_visible_competitive_event_listing_v1";
#[cfg(target_os = "windows")]
const REQUEST_SCOPE_V1: &str =
    "league_and_challenge_selected_listing_exact_semantics_checked_untrusted_v1";
#[cfg(target_os = "windows")]
const ASSET_SCOPE_V1: &str = "league_and_challenge_selected_listing_windows_ocr_exact_control_v1";
#[cfg(target_os = "windows")]
const PIXEL_FORMAT_V1: &str = "bgra8_unorm_top_down_tightly_packed_v1";
#[cfg(target_os = "windows")]
const REQUEST_COMMITMENT_DOMAIN_V1: &[u8] = b"mtgo-competitive-event-listing-classifier-request-v1";
#[cfg(target_os = "windows")]
const TARGET_COMMITMENT_DOMAIN_V1: &[u8] = b"mtgo-competitive-event-listing-target-v1";
#[cfg(target_os = "windows")]
const MAX_HEADER_BYTES_V1: usize = 1024 * 1024;
#[cfg(target_os = "windows")]
const MAX_ASSETS_BYTES_V1: usize = 16 * 1024 * 1024;
#[cfg(target_os = "windows")]
const MAX_CANONICAL_BYTES_V1: u64 = 512 * 1024 * 1024;
#[cfg(target_os = "windows")]
const MAX_RUNTIME_ARTIFACT_BYTES_V1: u64 = 512 * 1024 * 1024;
#[cfg(target_os = "windows")]
const MAX_PROFILES_V1: usize = 256;
#[cfg(target_os = "windows")]
const MAX_CONTROL_REFERENCES_V1: usize = 64;
#[cfg(target_os = "windows")]
const MAX_EXPECTED_LABEL_BYTES_V1: usize = 256;

#[cfg(target_os = "windows")]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MtgoCompetitiveEventListingClassifierAssetsV1 {
    schema_version: u32,
    scope: String,
    canonical_pixel_format: String,
    profiles: Vec<MtgoCompetitiveEventListingOcrProfileV1>,
}

#[cfg(target_os = "windows")]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MtgoCompetitiveEventListingOcrProfileV1 {
    profile_id: String,
    event_kind: MtgoCompetitiveEventKindV1,
    event_display_label_sha256: String,
    expected_visible_label: String,
    client_size_px: MtgoSizePxV1,
    label_search_rect_client_px: MtgoRectPxV1,
    open_entry_review_control_rect_client_px: MtgoRectPxV1,
    enabled_control_reference_sha256s: Vec<String>,
    confidence_bps: u16,
}

#[cfg(target_os = "windows")]
#[derive(Debug, Clone, PartialEq, Eq)]
struct OcrWordV1 {
    normalized: String,
    rect: MtgoRectPxV1,
}

#[cfg(target_os = "windows")]
struct ComGuardV1;

#[cfg(target_os = "windows")]
impl ComGuardV1 {
    fn initialize_v1() -> Result<Self, String> {
        unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }
            .ok()
            .map_err(|error| format!("initialize Windows Runtime apartment: {error}"))?;
        Ok(Self)
    }
}

#[cfg(target_os = "windows")]
impl Drop for ComGuardV1 {
    fn drop(&mut self) {
        unsafe { CoUninitialize() };
    }
}

#[cfg(target_os = "windows")]
fn main() -> ExitCode {
    match run_v1() {
        Ok(response) => match serde_json::to_vec(&response) {
            Ok(bytes) => match io::stdout().write_all(&bytes) {
                Ok(()) => ExitCode::SUCCESS,
                Err(error) => {
                    eprintln!("MTGO_EVENT_LISTING_CLASSIFIER_REJECTED:write response: {error}");
                    ExitCode::FAILURE
                }
            },
            Err(error) => {
                eprintln!("MTGO_EVENT_LISTING_CLASSIFIER_REJECTED:serialize response: {error}");
                ExitCode::FAILURE
            }
        },
        Err(error) => {
            eprintln!("MTGO_EVENT_LISTING_CLASSIFIER_REJECTED:{error}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(target_os = "windows")]
fn run_v1() -> Result<MtgoCompetitiveEventListingClassifierProcessResponseV1, String> {
    let args = std::env::args().collect::<Vec<_>>();
    if args.len() != 2 || args[1] != MODE_ARGUMENT_V1 {
        return Err("only the bounded selected-listing protocol mode is supported".to_owned());
    }
    let current_exe = std::env::current_exe()
        .map_err(|error| format!("resolve classifier executable: {error}"))?;
    let actual_classifier_sha256 = hash_bounded_file_v1(
        &current_exe,
        MAX_RUNTIME_ARTIFACT_BYTES_V1,
        "classifier executable",
    )?;
    let mut stdin = io::stdin().lock();
    let mut magic = vec![0_u8; PROTOCOL_MAGIC_V1.len()];
    stdin
        .read_exact(&mut magic)
        .map_err(|error| format!("read protocol magic: {error}"))?;
    if magic != PROTOCOL_MAGIC_V1 {
        return Err("selected-listing protocol magic differs".to_owned());
    }
    let header_length = read_u64_be_v1(&mut stdin, "header length")?;
    let assets_length = read_u64_be_v1(&mut stdin, "assets length")?;
    let header_length = bounded_usize_v1(header_length, MAX_HEADER_BYTES_V1, "header")?;
    let assets_length = bounded_usize_v1(assets_length, MAX_ASSETS_BYTES_V1, "assets")?;
    let header_json = read_exact_vec_v1(&mut stdin, header_length, "header")?;
    let assets_json = read_exact_vec_v1(&mut stdin, assets_length, "assets")?;
    let header = parse_canonical_json_v1::<MtgoCompetitiveEventListingClassifierRequestHeaderV1>(
        &header_json,
        "request header",
    )?;
    validate_header_identity_v1(&header, &assets_json, &actual_classifier_sha256)?;
    let pixel_length = bounded_usize_v1(
        header.canonical_byte_length,
        usize::try_from(MAX_CANONICAL_BYTES_V1)
            .map_err(|_| "classifier byte bound does not fit this process".to_owned())?,
        "canonical pixels",
    )?;
    let canonical_bgra8 = read_exact_vec_v1(&mut stdin, pixel_length, "canonical pixels")?;
    let mut trailing = [0_u8; 1];
    if stdin
        .read(&mut trailing)
        .map_err(|error| format!("check request end: {error}"))?
        != 0
    {
        return Err("selected-listing request has trailing bytes".to_owned());
    }
    validate_pixels_v1(&header, &canonical_bgra8)?;
    let assets = parse_canonical_json_v1::<MtgoCompetitiveEventListingClassifierAssetsV1>(
        &assets_json,
        "classifier assets",
    )?;
    let profile = validate_assets_and_select_profile_v1(&assets, &header)?;
    let request_commitment_sha256 = commitment_v1(
        REQUEST_COMMITMENT_DOMAIN_V1,
        &[&header_json, &assets_json, &canonical_bgra8],
    );
    let words = recognize_words_v1(
        header.canonical_width,
        header.canonical_height,
        &canonical_bgra8,
    )?;
    classify_from_words_v1(
        &header,
        profile,
        &canonical_bgra8,
        &words,
        request_commitment_sha256,
    )
}

#[cfg(target_os = "windows")]
fn classify_from_words_v1(
    header: &MtgoCompetitiveEventListingClassifierRequestHeaderV1,
    profile: &MtgoCompetitiveEventListingOcrProfileV1,
    canonical_bgra8: &[u8],
    words: &[OcrWordV1],
    request_commitment_sha256: String,
) -> Result<MtgoCompetitiveEventListingClassifierProcessResponseV1, String> {
    let expected_tokens = normalized_tokens_v1(&profile.expected_visible_label);
    let matches = find_exact_token_sequence_v1(words, &expected_tokens)
        .into_iter()
        .filter(|rect| rect_inside_v1(rect, &profile.label_search_rect_client_px))
        .collect::<Vec<_>>();
    if matches.len() != 1 {
        return Err(format!(
            "expected exactly one target label inside its reviewed card region, found {}",
            matches.len()
        ));
    }
    let event_label_rect_client_px = matches[0].clone();
    let size = profile.client_size_px.clone();
    let event_label_region_sha256 =
        visible_frame_region_content_sha256_v1(canonical_bgra8, &size, &event_label_rect_client_px)
            .map_err(|error| format!("hash selected event label pixels: {error}"))?;
    let control_region_sha256 = visible_frame_region_content_sha256_v1(
        canonical_bgra8,
        &size,
        &profile.open_entry_review_control_rect_client_px,
    )
    .map_err(|error| format!("hash Open Entry Review control pixels: {error}"))?;
    if profile
        .enabled_control_reference_sha256s
        .binary_search(&control_region_sha256)
        .is_err()
    {
        return Err(
            "Open Entry Review control does not match any reviewed enabled reference".to_owned(),
        );
    }
    if rects_overlap_v1(
        &event_label_rect_client_px,
        &profile.open_entry_review_control_rect_client_px,
    ) {
        return Err("selected label overlaps the reviewed Open Entry Review control".to_owned());
    }
    let selection = MtgoVisibleCompetitiveEventListingSelectionV1 {
        schema_version: MTGO_COMPETITIVE_EVENT_LISTING_SCHEMA_V1,
        selection_id: format!(
            "windows-ocr-listing-v1:{}",
            &request_commitment_sha256[..24]
        ),
        target_commitment_sha256: header.target_commitment_sha256.clone(),
        event_kind: header.target.event_kind,
        event_identity_sha256: header.target.event_identity_sha256.clone(),
        event_display_label_sha256: header.target.event_display_label_sha256.clone(),
        source_lifecycle_snapshot_commitment_sha256: header
            .source_lifecycle_snapshot_commitment_sha256
            .clone(),
        frame_id: header.frame_id,
        frame_sequence: header.frame_sequence,
        frame_sha256: header.canonical_bgra8_sha256.clone(),
        client_bounds: MtgoRectPxV1 {
            x: 0,
            y: 0,
            width: header.canonical_width,
            height: header.canonical_height,
        },
        event_label_rect_client_px,
        event_label_region_sha256,
        open_entry_review_control_rect_client_px: profile
            .open_entry_review_control_rect_client_px
            .clone(),
        open_entry_review_control_region_sha256: control_region_sha256,
        open_entry_review_control_enabled: true,
        confidence_bps: profile.confidence_bps,
    };
    Ok(MtgoCompetitiveEventListingClassifierProcessResponseV1 {
        schema_version: 1,
        request_commitment_sha256,
        selection,
    })
}

#[cfg(target_os = "windows")]
fn validate_header_identity_v1(
    header: &MtgoCompetitiveEventListingClassifierRequestHeaderV1,
    assets_json: &[u8],
    actual_classifier_sha256: &str,
) -> Result<(), String> {
    if header.schema_version != 1
        || header.protocol != REQUEST_PROTOCOL_V1
        || header.parser_scope != REQUEST_SCOPE_V1
        || header.frame_id == 0
        || header.frame_sequence == 0
        || header.captured_at_unix_millis == 0
        || header.canonical_width == 0
        || header.canonical_height == 0
        || header.canonical_width > 16_384
        || header.canonical_height > 16_384
    {
        return Err("selected-listing request identity is invalid".to_owned());
    }
    let expected_stride = header
        .canonical_width
        .checked_mul(4)
        .ok_or("selected-listing stride overflow")?;
    let expected_length = u64::from(expected_stride)
        .checked_mul(u64::from(header.canonical_height))
        .ok_or("selected-listing pixel length overflow")?;
    if header.canonical_stride != expected_stride
        || header.canonical_byte_length != expected_length
        || expected_length == 0
        || expected_length > MAX_CANONICAL_BYTES_V1
        || header.classifier_assets_manifest_sha256 != sha256_hex_v1(assets_json)
        || header.classifier_binary_sha256 != actual_classifier_sha256
    {
        return Err("selected-listing runtime, geometry, or assets differ".to_owned());
    }
    for digest in header_digests_v1(header) {
        validate_sha256_v1(digest, "request commitment")?;
    }
    validate_target_v1(&header.target)?;
    if header.target.approved_account_alias_sha256 != header.approved_account_alias_sha256
        || header.target_commitment_sha256 != target_commitment_v1(&header.target)?
    {
        return Err("selected-listing request changed its exact account or target".to_owned());
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn validate_pixels_v1(
    header: &MtgoCompetitiveEventListingClassifierRequestHeaderV1,
    canonical_bgra8: &[u8],
) -> Result<(), String> {
    if u64::try_from(canonical_bgra8.len()).ok() != Some(header.canonical_byte_length)
        || sha256_hex_v1(canonical_bgra8) != header.canonical_bgra8_sha256
    {
        return Err("selected-listing pixels differ from the request header".to_owned());
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn validate_assets_and_select_profile_v1<'a>(
    assets: &'a MtgoCompetitiveEventListingClassifierAssetsV1,
    header: &MtgoCompetitiveEventListingClassifierRequestHeaderV1,
) -> Result<&'a MtgoCompetitiveEventListingOcrProfileV1, String> {
    if assets.schema_version != 1
        || assets.scope != ASSET_SCOPE_V1
        || assets.canonical_pixel_format != PIXEL_FORMAT_V1
        || assets.profiles.is_empty()
        || assets.profiles.len() > MAX_PROFILES_V1
    {
        return Err("selected-listing assets identity or profile count is invalid".to_owned());
    }
    let client_bounds = MtgoRectPxV1 {
        x: 0,
        y: 0,
        width: header.canonical_width,
        height: header.canonical_height,
    };
    let mut profile_ids = HashSet::new();
    let mut label_hashes = HashSet::new();
    let mut previous_label_hash: Option<&str> = None;
    for profile in &assets.profiles {
        validate_identifier_v1(&profile.profile_id, "profile id")?;
        validate_sha256_v1(&profile.event_display_label_sha256, "profile label")?;
        if profile.expected_visible_label.is_empty()
            || profile.expected_visible_label.len() > MAX_EXPECTED_LABEL_BYTES_V1
            || profile.expected_visible_label.trim() != profile.expected_visible_label
            || profile.expected_visible_label.chars().any(char::is_control)
            || sha256_hex_v1(profile.expected_visible_label.as_bytes())
                != profile.event_display_label_sha256
            || profile.client_size_px.width != header.canonical_width
            || profile.client_size_px.height != header.canonical_height
            || !(9_500..=10_000).contains(&profile.confidence_bps)
            || !profile_ids.insert(profile.profile_id.as_str())
            || !label_hashes.insert(profile.event_display_label_sha256.as_str())
            || previous_label_hash
                .is_some_and(|previous| previous >= profile.event_display_label_sha256.as_str())
        {
            return Err("selected-listing OCR profile identity is invalid or unordered".to_owned());
        }
        previous_label_hash = Some(profile.event_display_label_sha256.as_str());
        if !rect_inside_v1(&profile.label_search_rect_client_px, &client_bounds)
            || !rect_inside_v1(
                &profile.open_entry_review_control_rect_client_px,
                &client_bounds,
            )
            || rects_overlap_v1(
                &profile.label_search_rect_client_px,
                &profile.open_entry_review_control_rect_client_px,
            )
            || profile.enabled_control_reference_sha256s.is_empty()
            || profile.enabled_control_reference_sha256s.len() > MAX_CONTROL_REFERENCES_V1
        {
            return Err("selected-listing OCR profile geometry is invalid".to_owned());
        }
        let mut previous_reference: Option<&str> = None;
        for reference in &profile.enabled_control_reference_sha256s {
            validate_sha256_v1(reference, "enabled control reference")?;
            if previous_reference.is_some_and(|previous| previous >= reference.as_str()) {
                return Err("enabled control references must be unique and sorted".to_owned());
            }
            previous_reference = Some(reference.as_str());
        }
    }
    let matches = assets
        .profiles
        .iter()
        .filter(|profile| {
            profile.event_kind == header.target.event_kind
                && profile.event_display_label_sha256 == header.target.event_display_label_sha256
        })
        .collect::<Vec<_>>();
    if matches.len() != 1 {
        return Err("no unique OCR profile matches the exact target label and mode".to_owned());
    }
    Ok(matches[0])
}

#[cfg(target_os = "windows")]
fn recognize_words_v1(
    width: u32,
    height: u32,
    canonical_bgra8: &[u8],
) -> Result<Vec<OcrWordV1>, String> {
    let _com = ComGuardV1::initialize_v1()?;
    let writer = DataWriter::new().map_err(|error| format!("create OCR buffer: {error}"))?;
    writer
        .WriteBytes(canonical_bgra8)
        .map_err(|error| format!("write OCR buffer: {error}"))?;
    let buffer = writer
        .DetachBuffer()
        .map_err(|error| format!("detach OCR buffer: {error}"))?;
    let bitmap = SoftwareBitmap::CreateCopyFromBuffer(
        &buffer,
        BitmapPixelFormat::Bgra8,
        i32::try_from(width).map_err(|_| "OCR width exceeds i32".to_owned())?,
        i32::try_from(height).map_err(|_| "OCR height exceeds i32".to_owned())?,
    )
    .map_err(|error| format!("create OCR software bitmap: {error}"))?;
    let engine = OcrEngine::TryCreateFromUserProfileLanguages()
        .map_err(|error| format!("create Windows OCR engine: {error}"))?;
    let maximum =
        OcrEngine::MaxImageDimension().map_err(|error| format!("read OCR image limit: {error}"))?;
    if width.max(height) > maximum {
        return Err("image exceeds the Windows OCR dimension limit".to_owned());
    }
    let result = engine
        .RecognizeAsync(&bitmap)
        .map_err(|error| format!("start Windows OCR: {error}"))?
        .join()
        .map_err(|error| format!("complete Windows OCR: {error}"))?;
    let lines = result
        .Lines()
        .map_err(|error| format!("read OCR lines: {error}"))?;
    let mut words = Vec::new();
    for line_index in 0..lines
        .Size()
        .map_err(|error| format!("read OCR line count: {error}"))?
    {
        let line = lines
            .GetAt(line_index)
            .map_err(|error| format!("read OCR line {line_index}: {error}"))?;
        let line_words = line
            .Words()
            .map_err(|error| format!("read OCR words for line {line_index}: {error}"))?;
        for word_index in 0..line_words
            .Size()
            .map_err(|error| format!("read OCR word count for line {line_index}: {error}"))?
        {
            let word = line_words
                .GetAt(word_index)
                .map_err(|error| format!("read OCR word {line_index}:{word_index}: {error}"))?;
            let normalized = normalize_visible_text_v1(
                &word
                    .Text()
                    .map_err(|error| format!("read OCR word text: {error}"))?
                    .to_string(),
            );
            if normalized.is_empty() {
                continue;
            }
            let raw_rect = word
                .BoundingRect()
                .map_err(|error| format!("read OCR word rectangle: {error}"))?;
            words.push(OcrWordV1 {
                normalized,
                rect: checked_ocr_rect_v1(
                    raw_rect.X,
                    raw_rect.Y,
                    raw_rect.Width,
                    raw_rect.Height,
                    width,
                    height,
                )?,
            });
        }
    }
    Ok(words)
}

#[cfg(target_os = "windows")]
fn find_exact_token_sequence_v1(words: &[OcrWordV1], expected: &[String]) -> Vec<MtgoRectPxV1> {
    if expected.is_empty() || expected.len() > words.len() {
        return Vec::new();
    }
    words
        .windows(expected.len())
        .filter(|window| {
            window
                .iter()
                .zip(expected)
                .all(|(word, expected)| word.normalized == *expected)
        })
        .filter_map(union_word_rects_v1)
        .collect()
}

#[cfg(target_os = "windows")]
fn union_word_rects_v1(words: &[OcrWordV1]) -> Option<MtgoRectPxV1> {
    let first = &words.first()?.rect;
    let mut left = first.x;
    let mut top = first.y;
    let mut right = first.x.checked_add(first.width)?;
    let mut bottom = first.y.checked_add(first.height)?;
    for word in &words[1..] {
        left = left.min(word.rect.x);
        top = top.min(word.rect.y);
        right = right.max(word.rect.x.checked_add(word.rect.width)?);
        bottom = bottom.max(word.rect.y.checked_add(word.rect.height)?);
    }
    Some(MtgoRectPxV1 {
        x: left,
        y: top,
        width: right.checked_sub(left)?,
        height: bottom.checked_sub(top)?,
    })
}

#[cfg(target_os = "windows")]
fn checked_ocr_rect_v1(
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    image_width: u32,
    image_height: u32,
) -> Result<MtgoRectPxV1, String> {
    if !x.is_finite()
        || !y.is_finite()
        || !width.is_finite()
        || !height.is_finite()
        || x < 0.0
        || y < 0.0
        || width <= 0.0
        || height <= 0.0
    {
        return Err("Windows OCR returned an invalid word rectangle".to_owned());
    }
    let left = x.floor() as u32;
    let top = y.floor() as u32;
    let right = (x + width).ceil() as u32;
    let bottom = (y + height).ceil() as u32;
    if right > image_width || bottom > image_height || right <= left || bottom <= top {
        return Err("Windows OCR returned an out-of-bounds word rectangle".to_owned());
    }
    Ok(MtgoRectPxV1 {
        x: left,
        y: top,
        width: right - left,
        height: bottom - top,
    })
}

#[cfg(target_os = "windows")]
fn normalized_tokens_v1(value: &str) -> Vec<String> {
    normalize_visible_text_v1(value)
        .split(' ')
        .map(str::to_owned)
        .collect()
}

#[cfg(target_os = "windows")]
fn normalize_visible_text_v1(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(target_os = "windows")]
fn rect_inside_v1(rect: &MtgoRectPxV1, bounds: &MtgoRectPxV1) -> bool {
    let Some(right) = rect.x.checked_add(rect.width) else {
        return false;
    };
    let Some(bottom) = rect.y.checked_add(rect.height) else {
        return false;
    };
    let Some(bounds_right) = bounds.x.checked_add(bounds.width) else {
        return false;
    };
    let Some(bounds_bottom) = bounds.y.checked_add(bounds.height) else {
        return false;
    };
    rect.width > 0
        && rect.height > 0
        && rect.x >= bounds.x
        && rect.y >= bounds.y
        && right <= bounds_right
        && bottom <= bounds_bottom
}

#[cfg(target_os = "windows")]
fn rects_overlap_v1(first: &MtgoRectPxV1, second: &MtgoRectPxV1) -> bool {
    let Some(first_right) = first.x.checked_add(first.width) else {
        return true;
    };
    let Some(first_bottom) = first.y.checked_add(first.height) else {
        return true;
    };
    let Some(second_right) = second.x.checked_add(second.width) else {
        return true;
    };
    let Some(second_bottom) = second.y.checked_add(second.height) else {
        return true;
    };
    first.x < second_right
        && second.x < first_right
        && first.y < second_bottom
        && second.y < first_bottom
}

#[cfg(target_os = "windows")]
fn target_commitment_v1(target: &MtgoCompetitiveEventListingTargetV1) -> Result<String, String> {
    let target_json = serde_json::to_vec(target)
        .map_err(|error| format!("serialize selected-listing target: {error}"))?;
    Ok(commitment_v1(
        TARGET_COMMITMENT_DOMAIN_V1,
        &[target_json.as_slice()],
    ))
}

#[cfg(target_os = "windows")]
fn validate_target_v1(target: &MtgoCompetitiveEventListingTargetV1) -> Result<(), String> {
    if target.schema_version != MTGO_COMPETITIVE_EVENT_LISTING_SCHEMA_V1 {
        return Err("selected-listing target schema is invalid".to_owned());
    }
    validate_identifier_v1(&target.target_id, "target id")?;
    for digest in [
        target.approved_account_alias_sha256.as_str(),
        target.event_identity_sha256.as_str(),
        target.event_display_label_sha256.as_str(),
        target.deck_list_sha256.as_str(),
        target.deck_manifest_commitment_sha256.as_str(),
        target.deck_format_sha256.as_str(),
        target.policy_deployment_commitment_sha256.as_str(),
    ] {
        validate_sha256_v1(digest, "target commitment")?;
    }
    if target.policy_deployment_commitment_sha256 == target.deck_list_sha256
        || target.policy_deployment_commitment_sha256 == target.deck_manifest_commitment_sha256
        || target.policy_deployment_commitment_sha256 == target.deck_format_sha256
    {
        return Err("selected-listing target deployment identities are not distinct".to_owned());
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn header_digests_v1(header: &MtgoCompetitiveEventListingClassifierRequestHeaderV1) -> [&str; 12] {
    [
        &header.canonical_bgra8_sha256,
        &header.source_capture_commitment_sha256,
        &header.source_frame_profile_binding_sha256,
        &header.source_navigation_classification_result_commitment_sha256,
        &header.source_lifecycle_snapshot_commitment_sha256,
        &header.navigation_profile_commitment_sha256,
        &header.navigation_profile_admission_commitment_sha256,
        &header.approved_account_alias_sha256,
        &header.runtime_identity_commitment_sha256,
        &header.classifier_binary_sha256,
        &header.classifier_assets_manifest_sha256,
        &header.target_commitment_sha256,
    ]
}

#[cfg(target_os = "windows")]
fn parse_canonical_json_v1<T>(bytes: &[u8], label: &str) -> Result<T, String>
where
    T: for<'de> Deserialize<'de> + Serialize,
{
    let value = serde_json::from_slice(bytes).map_err(|error| format!("parse {label}: {error}"))?;
    let canonical = serde_json::to_vec(&value)
        .map_err(|error| format!("serialize canonical {label}: {error}"))?;
    if canonical != bytes {
        return Err(format!("{label} is not canonical JSON"));
    }
    Ok(value)
}

#[cfg(target_os = "windows")]
fn read_u64_be_v1(reader: &mut impl Read, label: &str) -> Result<u64, String> {
    let mut bytes = [0_u8; 8];
    reader
        .read_exact(&mut bytes)
        .map_err(|error| format!("read {label}: {error}"))?;
    Ok(u64::from_be_bytes(bytes))
}

#[cfg(target_os = "windows")]
fn bounded_usize_v1(value: u64, maximum: usize, label: &str) -> Result<usize, String> {
    let value = usize::try_from(value).map_err(|_| format!("{label} length does not fit"))?;
    if value == 0 || value > maximum {
        return Err(format!("{label} length is outside bounds"));
    }
    Ok(value)
}

#[cfg(target_os = "windows")]
fn read_exact_vec_v1(
    reader: &mut impl Read,
    length: usize,
    label: &str,
) -> Result<Vec<u8>, String> {
    let mut bytes = vec![0_u8; length];
    reader
        .read_exact(&mut bytes)
        .map_err(|error| format!("read {label}: {error}"))?;
    Ok(bytes)
}

#[cfg(target_os = "windows")]
fn hash_bounded_file_v1(path: &Path, maximum: u64, label: &str) -> Result<String, String> {
    let mut file = File::open(path).map_err(|error| format!("open {label}: {error}"))?;
    let length = file
        .metadata()
        .map_err(|error| format!("inspect {label}: {error}"))?
        .len();
    if length == 0 || length > maximum {
        return Err(format!("{label} length is outside bounds"));
    }
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| format!("read {label}: {error}"))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

#[cfg(target_os = "windows")]
fn validate_identifier_v1(value: &str, label: &str) -> Result<(), String> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
    {
        return Err(format!("{label} is invalid"));
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn validate_sha256_v1(value: &str, label: &str) -> Result<(), String> {
    if !looks_like_sha256_v1(value) {
        return Err(format!("{label} must be lowercase SHA-256"));
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn looks_like_sha256_v1(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[cfg(target_os = "windows")]
fn sha256_hex_v1(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[cfg(target_os = "windows")]
fn commitment_v1(domain: &[u8], parts: &[&[u8]]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(domain);
    for part in parts {
        hasher.update((part.len() as u64).to_le_bytes());
        hasher.update(part);
    }
    format!("{:x}", hasher.finalize())
}

#[cfg(all(test, target_os = "windows"))]
mod tests {
    use super::*;

    fn digest(byte: char) -> String {
        byte.to_string().repeat(64)
    }

    fn target_v1(
        label: &str,
        kind: MtgoCompetitiveEventKindV1,
    ) -> MtgoCompetitiveEventListingTargetV1 {
        MtgoCompetitiveEventListingTargetV1 {
            schema_version: 1,
            target_id: "test-target-v1".to_owned(),
            event_kind: kind,
            approved_account_alias_sha256: digest('a'),
            event_identity_sha256: digest('b'),
            event_display_label_sha256: sha256_hex_v1(label.as_bytes()),
            deck_list_sha256: digest('c'),
            deck_manifest_commitment_sha256: digest('d'),
            deck_format_sha256: digest('e'),
            policy_deployment_commitment_sha256: digest('f'),
        }
    }

    fn header_v1(
        pixels: &[u8],
        assets_sha256: String,
        label: &str,
    ) -> MtgoCompetitiveEventListingClassifierRequestHeaderV1 {
        let target = target_v1(label, MtgoCompetitiveEventKindV1::League);
        MtgoCompetitiveEventListingClassifierRequestHeaderV1 {
            schema_version: 1,
            protocol: REQUEST_PROTOCOL_V1.to_owned(),
            parser_scope: REQUEST_SCOPE_V1.to_owned(),
            frame_id: 7,
            frame_sequence: 9,
            captured_at_unix_millis: 11,
            canonical_width: 32,
            canonical_height: 16,
            canonical_stride: 128,
            canonical_byte_length: pixels.len() as u64,
            canonical_bgra8_sha256: sha256_hex_v1(pixels),
            source_capture_commitment_sha256: digest('1'),
            source_frame_profile_binding_sha256: digest('2'),
            source_navigation_classification_result_commitment_sha256: digest('3'),
            source_lifecycle_snapshot_commitment_sha256: digest('4'),
            navigation_profile_commitment_sha256: digest('5'),
            navigation_profile_admission_commitment_sha256: digest('6'),
            approved_account_alias_sha256: target.approved_account_alias_sha256.clone(),
            runtime_identity_commitment_sha256: digest('7'),
            classifier_binary_sha256: digest('8'),
            classifier_assets_manifest_sha256: assets_sha256,
            target_commitment_sha256: target_commitment_v1(&target).unwrap(),
            target,
        }
    }

    fn profile_v1(pixels: &[u8], label: &str) -> MtgoCompetitiveEventListingOcrProfileV1 {
        let control = MtgoRectPxV1 {
            x: 20,
            y: 4,
            width: 8,
            height: 4,
        };
        MtgoCompetitiveEventListingOcrProfileV1 {
            profile_id: "modern-league-32x16-v1".to_owned(),
            event_kind: MtgoCompetitiveEventKindV1::League,
            event_display_label_sha256: sha256_hex_v1(label.as_bytes()),
            expected_visible_label: label.to_owned(),
            client_size_px: MtgoSizePxV1 {
                width: 32,
                height: 16,
            },
            label_search_rect_client_px: MtgoRectPxV1 {
                x: 0,
                y: 0,
                width: 16,
                height: 16,
            },
            open_entry_review_control_rect_client_px: control.clone(),
            enabled_control_reference_sha256s: vec![visible_frame_region_content_sha256_v1(
                pixels,
                &MtgoSizePxV1 {
                    width: 32,
                    height: 16,
                },
                &control,
            )
            .unwrap()],
            confidence_bps: 9_500,
        }
    }

    fn words_v1() -> Vec<OcrWordV1> {
        vec![
            OcrWordV1 {
                normalized: "Modern".to_owned(),
                rect: MtgoRectPxV1 {
                    x: 2,
                    y: 4,
                    width: 5,
                    height: 3,
                },
            },
            OcrWordV1 {
                normalized: "League".to_owned(),
                rect: MtgoRectPxV1 {
                    x: 8,
                    y: 4,
                    width: 6,
                    height: 3,
                },
            },
        ]
    }

    #[test]
    fn exact_label_and_reviewed_enabled_control_produce_one_bound_selection() {
        let pixels = (0..32 * 16 * 4)
            .map(|index| ((index * 13 + 7) % 251) as u8)
            .collect::<Vec<_>>();
        let label = "Modern League";
        let profile = profile_v1(&pixels, label);
        let header = header_v1(&pixels, digest('9'), label);
        let response =
            classify_from_words_v1(&header, &profile, &pixels, &words_v1(), digest('0')).unwrap();
        assert_eq!(
            response.selection.event_kind,
            MtgoCompetitiveEventKindV1::League
        );
        assert_eq!(response.selection.confidence_bps, 9_500);
        assert!(response.selection.open_entry_review_control_enabled);
        assert_eq!(response.selection.event_label_rect_client_px.x, 2);
    }

    #[test]
    fn duplicate_label_or_unreviewed_control_fails_closed() {
        let pixels = vec![17_u8; 32 * 16 * 4];
        let label = "Modern League";
        let mut profile = profile_v1(&pixels, label);
        let header = header_v1(&pixels, digest('9'), label);
        let mut duplicate = words_v1();
        duplicate.extend(words_v1());
        assert!(
            classify_from_words_v1(&header, &profile, &pixels, &duplicate, digest('0')).is_err()
        );

        profile.enabled_control_reference_sha256s = vec![digest('1')];
        assert!(
            classify_from_words_v1(&header, &profile, &pixels, &words_v1(), digest('0')).is_err()
        );
    }

    #[test]
    fn assets_require_unique_sorted_exact_target_profiles() {
        let pixels = vec![23_u8; 32 * 16 * 4];
        let label = "Modern League";
        let mut assets = MtgoCompetitiveEventListingClassifierAssetsV1 {
            schema_version: 1,
            scope: ASSET_SCOPE_V1.to_owned(),
            canonical_pixel_format: PIXEL_FORMAT_V1.to_owned(),
            profiles: vec![profile_v1(&pixels, label)],
        };
        let assets_bytes = serde_json::to_vec(&assets).unwrap();
        let header = header_v1(&pixels, sha256_hex_v1(&assets_bytes), label);
        assert!(validate_assets_and_select_profile_v1(&assets, &header).is_ok());

        assets.profiles.push(assets.profiles[0].clone());
        assert!(validate_assets_and_select_profile_v1(&assets, &header).is_err());
        assets.profiles.pop();
        assets.profiles[0].label_search_rect_client_px = assets.profiles[0]
            .open_entry_review_control_rect_client_px
            .clone();
        assert!(validate_assets_and_select_profile_v1(&assets, &header).is_err());
    }

    #[test]
    fn header_binds_exact_binary_assets_target_and_pixels() {
        let pixels = vec![31_u8; 32 * 16 * 4];
        let label = "Modern League";
        let assets = MtgoCompetitiveEventListingClassifierAssetsV1 {
            schema_version: 1,
            scope: ASSET_SCOPE_V1.to_owned(),
            canonical_pixel_format: PIXEL_FORMAT_V1.to_owned(),
            profiles: vec![profile_v1(&pixels, label)],
        };
        let assets_bytes = serde_json::to_vec(&assets).unwrap();
        let header = header_v1(&pixels, sha256_hex_v1(&assets_bytes), label);
        assert!(validate_header_identity_v1(
            &header,
            &assets_bytes,
            &header.classifier_binary_sha256
        )
        .is_ok());
        assert!(validate_pixels_v1(&header, &pixels).is_ok());

        let mut changed_pixels = pixels;
        changed_pixels[0] ^= 1;
        assert!(validate_pixels_v1(&header, &changed_pixels).is_err());
        assert!(
            validate_header_identity_v1(&header, b"{}", &header.classifier_binary_sha256).is_err()
        );
        assert!(validate_header_identity_v1(&header, &assets_bytes, &digest('9')).is_err());
    }
}
