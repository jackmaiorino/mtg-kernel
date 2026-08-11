#[cfg(not(target_os = "windows"))]
compile_error!("probe_mtgo_preview_ocr_v1 is Windows-only");

#[cfg(target_os = "windows")]
use png::{BitDepth, ColorType, Transformations};
#[cfg(target_os = "windows")]
use serde::Serialize;
#[cfg(target_os = "windows")]
use sha2::{Digest, Sha256};
#[cfg(target_os = "windows")]
use std::{
    collections::HashSet,
    ffi::OsString,
    fs,
    io::Cursor,
    path::{Path, PathBuf},
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
const MAX_PNG_BYTES_V1: u64 = 64 * 1_048_576;
#[cfg(target_os = "windows")]
const MAX_IMAGE_DIMENSION_V1: u32 = 16_384;
#[cfg(target_os = "windows")]
const MAX_EXPECTED_LABELS_V1: usize = 64;
#[cfg(target_os = "windows")]
const MAX_EXPECTED_LABEL_BYTES_V1: usize = 256;

#[cfg(target_os = "windows")]
#[derive(Debug, Serialize)]
struct PreviewOcrReportV1 {
    schema_version: u32,
    purpose: &'static str,
    source_png_sha256: String,
    canonical_bgra8_sha256: String,
    width: u32,
    height: u32,
    expected_labels: Vec<PreviewOcrExpectedLabelResultV1>,
    raw_ocr_text_emitted: bool,
    captures_live_client: bool,
    safe_for_semantic_evidence: bool,
    safe_for_policy_scoring: bool,
    safe_for_input: bool,
    permits_event_entry: bool,
    permits_spending: bool,
}

#[cfg(target_os = "windows")]
#[derive(Debug, Serialize)]
struct PreviewOcrExpectedLabelResultV1 {
    expected_label_sha256: String,
    exact_normalized_match_count: u32,
    matched_rects_client_px: Vec<PreviewOcrRectV1>,
}

#[cfg(target_os = "windows")]
#[derive(Debug, Clone, Copy, Serialize)]
struct PreviewOcrRectV1 {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

#[cfg(target_os = "windows")]
#[derive(Debug)]
struct OcrWordV1 {
    normalized: String,
    rect: PreviewOcrRectV1,
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
    match run_v1(std::env::args_os().collect()) {
        Ok(report) => match serde_json::to_string_pretty(&report) {
            Ok(json) => {
                println!("{json}");
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("MTGO_PREVIEW_OCR_REJECTED:serialize report: {error}");
                ExitCode::FAILURE
            }
        },
        Err(error) => {
            eprintln!("MTGO_PREVIEW_OCR_REJECTED:{error}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(target_os = "windows")]
fn run_v1(args: Vec<OsString>) -> Result<PreviewOcrReportV1, String> {
    if args.len() < 3 || args.len() > MAX_EXPECTED_LABELS_V1 + 2 {
        return Err(
            "usage: probe_mtgo_preview_ocr_v1 <frame.png> <expected-visible-label> [...]"
                .to_owned(),
        );
    }
    let path = PathBuf::from(&args[1]);
    let expected_labels = validate_expected_labels_v1(&args[2..])?;
    let png_bytes = read_bounded_png_v1(&path)?;
    let source_png_sha256 = sha256_hex_v1(&png_bytes);
    let (width, height, canonical_bgra8) = decode_png_to_bgra8_v1(&png_bytes)?;
    let canonical_bgra8_sha256 = sha256_hex_v1(&canonical_bgra8);
    let words = recognize_words_v1(width, height, &canonical_bgra8)?;

    let expected_labels = expected_labels
        .into_iter()
        .map(|expected| {
            let expected_label_sha256 = sha256_hex_v1(expected.as_bytes());
            let expected_tokens = normalized_tokens_v1(&expected);
            let matched_rects_client_px = find_exact_token_sequence_v1(&words, &expected_tokens);
            let exact_normalized_match_count = u32::try_from(matched_rects_client_px.len())
                .map_err(|_| "OCR match count exceeds u32".to_owned())?;
            Ok(PreviewOcrExpectedLabelResultV1 {
                expected_label_sha256,
                exact_normalized_match_count,
                matched_rects_client_px,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;

    Ok(PreviewOcrReportV1 {
        schema_version: 1,
        purpose: "offline_visible_preview_ocr_feasibility_v1",
        source_png_sha256,
        canonical_bgra8_sha256,
        width,
        height,
        expected_labels,
        raw_ocr_text_emitted: false,
        captures_live_client: false,
        safe_for_semantic_evidence: false,
        safe_for_policy_scoring: false,
        safe_for_input: false,
        permits_event_entry: false,
        permits_spending: false,
    })
}

#[cfg(target_os = "windows")]
fn validate_expected_labels_v1(values: &[OsString]) -> Result<Vec<String>, String> {
    let mut labels = Vec::with_capacity(values.len());
    let mut unique = HashSet::new();
    for value in values {
        let label = value
            .to_str()
            .ok_or("expected visible labels must be valid UTF-8")?
            .to_owned();
        if label.is_empty()
            || label.len() > MAX_EXPECTED_LABEL_BYTES_V1
            || label.trim() != label
            || label.chars().any(char::is_control)
            || !unique.insert(label.clone())
        {
            return Err(
                "expected visible labels must be unique, trimmed, nonempty, bounded UTF-8 without control characters"
                    .to_owned(),
            );
        }
        labels.push(label);
    }
    Ok(labels)
}

#[cfg(target_os = "windows")]
fn read_bounded_png_v1(path: &Path) -> Result<Vec<u8>, String> {
    let metadata = fs::metadata(path).map_err(|error| format!("inspect PNG: {error}"))?;
    if !metadata.is_file() || metadata.len() == 0 || metadata.len() > MAX_PNG_BYTES_V1 {
        return Err(format!(
            "PNG must be a nonempty regular file no larger than {MAX_PNG_BYTES_V1} bytes"
        ));
    }
    let bytes = fs::read(path).map_err(|error| format!("read PNG: {error}"))?;
    if bytes.len() as u64 != metadata.len() {
        return Err("PNG changed while it was read".to_owned());
    }
    Ok(bytes)
}

#[cfg(target_os = "windows")]
fn decode_png_to_bgra8_v1(bytes: &[u8]) -> Result<(u32, u32, Vec<u8>), String> {
    let mut decoder = png::Decoder::new(Cursor::new(bytes));
    decoder.set_transformations(Transformations::IDENTITY);
    let mut reader = decoder
        .read_info()
        .map_err(|error| format!("decode PNG header: {error}"))?;
    let info = reader.info();
    if info.width == 0
        || info.height == 0
        || info.width > MAX_IMAGE_DIMENSION_V1
        || info.height > MAX_IMAGE_DIMENSION_V1
        || info.interlaced
        || info.animation_control.is_some()
        || info.bit_depth != BitDepth::Eight
        || !matches!(info.color_type, ColorType::Rgb | ColorType::Rgba)
    {
        return Err(
            "PNG must be one non-interlaced 8-bit RGB or opaque RGBA image within bounds"
                .to_owned(),
        );
    }
    let width = info.width;
    let height = info.height;
    let color_type = info.color_type;
    let channels = if color_type == ColorType::Rgb { 3 } else { 4 };
    let source_len = usize::try_from(width)
        .ok()
        .and_then(|width| {
            usize::try_from(height)
                .ok()
                .and_then(|height| width.checked_mul(height))
        })
        .and_then(|pixels| pixels.checked_mul(channels))
        .ok_or("decoded PNG size overflow")?;
    let canonical_len = usize::try_from(width)
        .ok()
        .and_then(|width| {
            usize::try_from(height)
                .ok()
                .and_then(|height| width.checked_mul(height))
        })
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or("canonical PNG size overflow")?;
    let mut source = vec![0_u8; source_len];
    let frame = reader
        .next_frame(&mut source)
        .map_err(|error| format!("decode PNG frame: {error}"))?;
    if frame.width != width
        || frame.height != height
        || frame.color_type != color_type
        || frame.bit_depth != BitDepth::Eight
        || frame.buffer_size() != source_len
    {
        return Err("decoded PNG differs from its admitted header".to_owned());
    }
    let mut canonical = Vec::with_capacity(canonical_len);
    match color_type {
        ColorType::Rgb => {
            for pixel in source.chunks_exact(3) {
                canonical.extend_from_slice(&[pixel[2], pixel[1], pixel[0], 255]);
            }
        }
        ColorType::Rgba => {
            for pixel in source.chunks_exact(4) {
                if pixel[3] != 255 {
                    return Err("composed preview PNG must be fully opaque".to_owned());
                }
                canonical.extend_from_slice(&[pixel[2], pixel[1], pixel[0], 255]);
            }
        }
        _ => unreachable!("PNG color type was checked above"),
    }
    if canonical.len() != canonical_len {
        return Err("canonical BGRA8 length mismatch".to_owned());
    }
    Ok((width, height, canonical))
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
    let ocr_max_image_dimension =
        OcrEngine::MaxImageDimension().map_err(|error| format!("read OCR image limit: {error}"))?;
    if width.max(height) > ocr_max_image_dimension {
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
            let rect = word
                .BoundingRect()
                .map_err(|error| format!("read OCR word rectangle: {error}"))?;
            let rect = checked_rect_v1(rect.X, rect.Y, rect.Width, rect.Height, width, height)?;
            words.push(OcrWordV1 { normalized, rect });
        }
    }
    Ok(words)
}

#[cfg(target_os = "windows")]
fn normalize_visible_text_v1(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(target_os = "windows")]
fn normalized_tokens_v1(value: &str) -> Vec<String> {
    normalize_visible_text_v1(value)
        .split(' ')
        .map(str::to_owned)
        .collect()
}

#[cfg(target_os = "windows")]
fn find_exact_token_sequence_v1(words: &[OcrWordV1], expected: &[String]) -> Vec<PreviewOcrRectV1> {
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
        .filter_map(union_rects_v1)
        .collect()
}

#[cfg(target_os = "windows")]
fn union_rects_v1(words: &[OcrWordV1]) -> Option<PreviewOcrRectV1> {
    let first = words.first()?.rect;
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
    Some(PreviewOcrRectV1 {
        x: left,
        y: top,
        width: right.checked_sub(left)?,
        height: bottom.checked_sub(top)?,
    })
}

#[cfg(target_os = "windows")]
fn checked_rect_v1(
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    image_width: u32,
    image_height: u32,
) -> Result<PreviewOcrRectV1, String> {
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
    Ok(PreviewOcrRectV1 {
        x: left,
        y: top,
        width: right - left,
        height: bottom - top,
    })
}

#[cfg(target_os = "windows")]
fn sha256_hex_v1(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[cfg(all(test, target_os = "windows"))]
mod tests {
    use super::*;

    #[test]
    fn exact_token_match_unions_only_the_matching_words() {
        let words = vec![
            OcrWordV1 {
                normalized: "Modern".to_owned(),
                rect: PreviewOcrRectV1 {
                    x: 10,
                    y: 20,
                    width: 40,
                    height: 10,
                },
            },
            OcrWordV1 {
                normalized: "Challenge".to_owned(),
                rect: PreviewOcrRectV1 {
                    x: 12,
                    y: 31,
                    width: 60,
                    height: 10,
                },
            },
            OcrWordV1 {
                normalized: "64".to_owned(),
                rect: PreviewOcrRectV1 {
                    x: 74,
                    y: 31,
                    width: 12,
                    height: 10,
                },
            },
        ];
        assert_eq!(
            find_exact_token_sequence_v1(&words, &normalized_tokens_v1("Modern Challenge 64"))
                .len(),
            1
        );
        assert!(
            find_exact_token_sequence_v1(&words, &normalized_tokens_v1("Modern League")).is_empty()
        );
    }
}
