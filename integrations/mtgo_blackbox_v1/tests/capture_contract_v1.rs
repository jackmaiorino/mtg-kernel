use mtgo_blackbox_v1::{
    calibration_profile_commitment_v1, check_untrusted_calibration_profile_v1,
    check_untrusted_real_visible_frame_v1, preview_output_identity_commitment_v1,
    CheckedUntrustedMtgoCalibrationProfileV1, MtgoCalibrationAnchorV1,
    MtgoCalibrationProfilePayloadV1, MtgoCalibrationReviewV1, MtgoCanonicalPixelFormatV1,
    MtgoCaptureBackendV1, MtgoRealVisibleFrameCandidateV1, MtgoRectPxV1, MtgoSignedRectDesktopPxV1,
    MtgoSizePxV1, MTGO_CALIBRATION_PROFILE_SCHEMA_V1, MTGO_CALIBRATION_REVIEW_SCHEMA_V1,
    MTGO_REAL_VISIBLE_FRAME_SCHEMA_V1,
};
use sha2::{Digest, Sha256};

const PRODUCT_VERSION: &str = "3.4.158.4689";
const SIGNER_SUBJECT: &str =
    "CN=Daybreak Game Company LLC, O=Daybreak Game Company LLC, L=San Diego, S=California, C=US";
const WINDOW_TITLE: &str = "Magic: The Gathering Online";
const MONITOR_DEVICE: &str = r"\\.\DISPLAY2";
const VALIDATION_NOW: &str = "2026-08-09T16:46:05Z";

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn crop_digest(
    pixels: &[u8],
    client_width: usize,
    x: usize,
    y: usize,
    width: usize,
    height: usize,
) -> String {
    let stride = client_width * 4;
    let mut hasher = Sha256::new();
    for row in y..(y + height) {
        let start = row * stride + x * 4;
        hasher.update(&pixels[start..start + width * 4]);
    }
    format!("{:x}", hasher.finalize())
}

fn sample_pixels() -> Box<[u8]> {
    let mut pixels = Vec::with_capacity(16 * 16 * 4);
    for index in 0..(16 * 16) {
        pixels.extend_from_slice(&[
            (index % 251) as u8,
            ((index * 3) % 251) as u8,
            ((index * 7) % 251) as u8,
            255,
        ]);
    }
    pixels.into_boxed_slice()
}

fn encode_preview_png(pixels: &[u8]) -> Vec<u8> {
    let mut rgba = Vec::with_capacity(pixels.len());
    for pixel in pixels.chunks_exact(4) {
        rgba.extend_from_slice(&[pixel[2], pixel[1], pixel[0], pixel[3]]);
    }
    let mut frame = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut frame, 16, 16);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(&rgba).unwrap();
    }
    frame
}

fn rect(left: i32, top: i32, width: u32, height: u32) -> serde_json::Value {
    serde_json::json!({
        "left": left,
        "top": top,
        "right": i64::from(left) + i64::from(width),
        "bottom": i64::from(top) + i64::from(height),
        "width": width,
        "height": height,
    })
}

fn sample_preview_value(frame: &[u8]) -> serde_json::Value {
    serde_json::json!({
        "schema_version": 1,
        "artifact_kind": "mtgo_visible_desktop_calibration_preview_v1",
        "status": "pending_visual_review",
        "captured_at_utc": "2026-08-09T16:44:59.1234567Z",
        "capture_backend": "system_drawing_copy_from_composed_screen_v1",
        "pixel_source": "visible_desktop_client_crop_only",
        "profile_id": null,
        "safe_for_semantic_evidence": false,
        "safe_for_ocr": false,
        "safe_for_policy_scoring": false,
        "safe_for_input": false,
        "expected_identity": {
            "product_version": PRODUCT_VERSION,
            "executable_sha256": "3".repeat(64),
            "signer_thumbprint": "4".repeat(40),
            "signer_subject": SIGNER_SUBJECT,
            "dpi": 120,
            "window_title": WINDOW_TITLE,
        },
        "observed_identity": {
            "process_id": 42704,
            "process_start_utc": "2026-08-09T16:00:00.0000000Z",
            "product_version": PRODUCT_VERSION,
            "file_version": PRODUCT_VERSION,
            "executable_sha256": "3".repeat(64).to_uppercase(),
            "signer_thumbprint": "4".repeat(40).to_uppercase(),
            "signer_subject": SIGNER_SUBJECT,
        },
        "window": {
            "title": WINDOW_TITLE,
            "foreground": true,
            "visible": true,
            "minimized": false,
            "cloaked": false,
            "display_affinity": 0,
            "desktop_composition_enabled": true,
            "dpi": 120,
            "client_bounds_desktop_px": rect(-1000, 100, 16, 16),
            "extended_frame_bounds_desktop_px": rect(-1002, 98, 20, 20),
        },
        "monitor": {
            "device_name": MONITOR_DEVICE,
            "is_primary": false,
            "bounds_desktop_px": rect(-1920, 0, 1920, 1080),
            "work_area_desktop_px": rect(-1920, 0, 1920, 1040),
        },
        "occlusion_audit": {
            "target_found": true,
            "windows_examined_above_target": 2,
            "intersecting_windows_above_target": 0,
            "cursor_showing": true,
            "cursor_inside_client": false,
        },
        "frame": {
            "file": "frame.png",
            "width": 16,
            "height": 16,
            "format": "png",
            "sha256": digest(frame).to_uppercase(),
        },
        "review_requirements": [
            "Confirm client-only pixels.",
            "Confirm the client is unobscured and cursor-free."
        ],
    })
}

fn sample_preview_artifacts(pixels: &[u8]) -> (Vec<u8>, Vec<u8>) {
    let frame = encode_preview_png(pixels);
    let manifest = serde_json::to_vec(&sample_preview_value(&frame)).unwrap();
    (manifest, frame)
}

fn sample_profile(pixels: &[u8]) -> MtgoCalibrationProfilePayloadV1 {
    let (manifest, frame) = sample_preview_artifacts(pixels);
    let output_bounds_desktop_px = MtgoSignedRectDesktopPxV1 {
        left: -1_920,
        top: 0,
        width: 1_920,
        height: 1_080,
    };
    MtgoCalibrationProfilePayloadV1 {
        schema_version: MTGO_CALIBRATION_PROFILE_SCHEMA_V1,
        profile_id: "mtgo-3.4.158.4689-dpi120-16x16-v1".to_owned(),
        source_preview_manifest_sha256: digest(&manifest),
        source_preview_frame_sha256: digest(&frame),
        source_preview_canonical_bgra8_sha256: digest(pixels),
        product_version: PRODUCT_VERSION.to_owned(),
        file_version: PRODUCT_VERSION.to_owned(),
        executable_sha256: "3".repeat(64),
        signer_thumbprint: "4".repeat(40),
        signer_subject_sha256: digest(SIGNER_SUBJECT.as_bytes()),
        window_title_sha256: digest(WINDOW_TITLE.as_bytes()),
        dpi: 120,
        client_size_px: MtgoSizePxV1 {
            width: 16,
            height: 16,
        },
        output_identity_sha256: preview_output_identity_commitment_v1(
            MONITOR_DEVICE,
            &output_bounds_desktop_px,
        )
        .unwrap(),
        output_device_name_sha256: digest(MONITOR_DEVICE.as_bytes()),
        output_bounds_desktop_px,
        canonical_pixel_format: MtgoCanonicalPixelFormatV1::Bgra8UnormTopDownTightlyPackedV1,
        anchors: vec![
            MtgoCalibrationAnchorV1 {
                anchor_id: "bottom_left".to_owned(),
                rect_client_px: MtgoRectPxV1 {
                    x: 0,
                    y: 12,
                    width: 4,
                    height: 4,
                },
                reference_bgra8_sha256: crop_digest(pixels, 16, 0, 12, 4, 4),
            },
            MtgoCalibrationAnchorV1 {
                anchor_id: "bottom_right".to_owned(),
                rect_client_px: MtgoRectPxV1 {
                    x: 12,
                    y: 12,
                    width: 4,
                    height: 4,
                },
                reference_bgra8_sha256: crop_digest(pixels, 16, 12, 12, 4, 4),
            },
            MtgoCalibrationAnchorV1 {
                anchor_id: "top_left".to_owned(),
                rect_client_px: MtgoRectPxV1 {
                    x: 0,
                    y: 0,
                    width: 4,
                    height: 4,
                },
                reference_bgra8_sha256: crop_digest(pixels, 16, 0, 0, 4, 4),
            },
            MtgoCalibrationAnchorV1 {
                anchor_id: "top_right".to_owned(),
                rect_client_px: MtgoRectPxV1 {
                    x: 12,
                    y: 0,
                    width: 4,
                    height: 4,
                },
                reference_bgra8_sha256: crop_digest(pixels, 16, 12, 0, 4, 4),
            },
        ],
    }
}

fn sample_review(profile: &MtgoCalibrationProfilePayloadV1) -> MtgoCalibrationReviewV1 {
    MtgoCalibrationReviewV1 {
        schema_version: MTGO_CALIBRATION_REVIEW_SCHEMA_V1,
        profile_sha256: calibration_profile_commitment_v1(profile).unwrap(),
        source_preview_manifest_sha256: profile.source_preview_manifest_sha256.clone(),
        source_preview_frame_sha256: profile.source_preview_frame_sha256.clone(),
        reviewer_alias_sha256: "9".repeat(64),
        reviewed_at_utc: "2026-08-09T16:45:00Z".to_owned(),
        client_only_confirmed: true,
        unobscured_confirmed: true,
        cursor_absent_confirmed: true,
        identity_confirmed: true,
        anchors_confirmed: true,
    }
}

fn check_profile_with_manifest_value(
    mut payload: MtgoCalibrationProfilePayloadV1,
    pixels: &[u8],
    mut manifest_value: serde_json::Value,
) -> Result<CheckedUntrustedMtgoCalibrationProfileV1, mtgo_blackbox_v1::MtgoContractErrorV1> {
    let frame = encode_preview_png(pixels);
    manifest_value["frame"]["sha256"] = serde_json::Value::String(digest(&frame));
    let manifest = serde_json::to_vec(&manifest_value).unwrap();
    payload.source_preview_manifest_sha256 = digest(&manifest);
    payload.source_preview_frame_sha256 = digest(&frame);
    payload.source_preview_canonical_bgra8_sha256 = digest(pixels);
    let review = sample_review(&payload);
    check_untrusted_calibration_profile_v1(payload, review, &manifest, &frame, pixels)
}

fn check_profile(
    payload: MtgoCalibrationProfilePayloadV1,
    review: MtgoCalibrationReviewV1,
    pixels: &[u8],
) -> Result<CheckedUntrustedMtgoCalibrationProfileV1, mtgo_blackbox_v1::MtgoContractErrorV1> {
    let (manifest, frame) = sample_preview_artifacts(pixels);
    check_untrusted_calibration_profile_v1(payload, review, &manifest, &frame, pixels)
}

fn check_frame(
    profile: &CheckedUntrustedMtgoCalibrationProfileV1,
    candidate: MtgoRealVisibleFrameCandidateV1,
    pixels: Box<[u8]>,
) -> Result<
    mtgo_blackbox_v1::CheckedUntrustedMtgoRealVisibleFrameV1,
    mtgo_blackbox_v1::MtgoContractErrorV1,
> {
    check_untrusted_real_visible_frame_v1(profile, candidate, pixels, VALIDATION_NOW)
}

fn sample_candidate(
    profile: &CheckedUntrustedMtgoCalibrationProfileV1,
    pixels: &[u8],
) -> MtgoRealVisibleFrameCandidateV1 {
    let expected = profile.payload();
    MtgoRealVisibleFrameCandidateV1 {
        schema_version: MTGO_REAL_VISIBLE_FRAME_SCHEMA_V1,
        frame_id: 1,
        sequence: 1,
        backend: MtgoCaptureBackendV1::DxgiDesktopDuplicationV1,
        profile_sha256: profile.profile_sha256().to_owned(),
        profile_review_sha256: profile.review_sha256().to_owned(),
        product_version: expected.product_version.clone(),
        file_version: expected.file_version.clone(),
        executable_sha256: expected.executable_sha256.clone(),
        signer_thumbprint: expected.signer_thumbprint.clone(),
        signer_subject_sha256: expected.signer_subject_sha256.clone(),
        window_title_sha256: expected.window_title_sha256.clone(),
        dpi: expected.dpi,
        client_rect_desktop_px: MtgoSignedRectDesktopPxV1 {
            left: -1_000,
            top: 100,
            width: 16,
            height: 16,
        },
        client_size_px: expected.client_size_px.clone(),
        output_identity_sha256: expected.output_identity_sha256.clone(),
        output_device_name_sha256: expected.output_device_name_sha256.clone(),
        output_bounds_desktop_px: expected.output_bounds_desktop_px.clone(),
        canonical_pixel_format: expected.canonical_pixel_format,
        canonical_client_pixels_sha256: digest(pixels),
        captured_at_utc: "2026-08-09T16:46:00Z".to_owned(),
        last_present_time_qpc: 10,
        accumulated_frames: 1,
        capture_time_window_identity_confirmed: true,
        capture_time_geometry_confirmed: true,
        foreground_confirmed: true,
        visible_confirmed: true,
        uncloaked_confirmed: true,
        unminimized_confirmed: true,
        output_contained_confirmed: true,
        stable_pre_post_confirmed: true,
        occlusion_free_confirmed: true,
        cursor_absent_confirmed: true,
        desktop_present_confirmed: true,
        protected_content_absent_confirmed: true,
    }
}

fn validated_fixture() -> (CheckedUntrustedMtgoCalibrationProfileV1, Box<[u8]>) {
    let pixels = sample_pixels();
    let payload = sample_profile(&pixels);
    let review = sample_review(&payload);
    let validated = check_profile(payload, review, &pixels).unwrap();
    (validated, pixels)
}

#[test]
fn reviewed_profile_and_matching_dxgi_frame_are_structurally_checked_only() {
    let (profile, pixels) = validated_fixture();
    let candidate = sample_candidate(&profile, &pixels);
    let checked =
        check_untrusted_real_visible_frame_v1(&profile, candidate, pixels.clone(), VALIDATION_NOW)
            .unwrap();

    assert_eq!(checked.frame_id(), 1);
    assert_eq!(checked.sequence(), 1);
    assert_eq!(checked.profile_sha256(), profile.profile_sha256());
    assert_eq!(checked.canonical_pixels_sha256(), digest(&pixels));
    assert_eq!(checked.frame_commitment_sha256().len(), 64);
}

#[test]
fn review_bound_to_another_profile_fails() {
    let pixels = sample_pixels();
    let payload = sample_profile(&pixels);
    let mut review = sample_review(&payload);
    review.profile_sha256 = "a".repeat(64);
    assert_eq!(
        check_profile(payload, review, &pixels).unwrap_err().code(),
        "review_profile_binding"
    );
}

#[test]
fn review_and_profile_must_bind_the_same_preview() {
    let pixels = sample_pixels();
    let payload = sample_profile(&pixels);
    let mut review = sample_review(&payload);
    review.source_preview_frame_sha256 = "b".repeat(64);
    assert_eq!(
        check_profile(payload, review, &pixels).unwrap_err().code(),
        "review_preview_binding"
    );
}

#[test]
fn preview_bytes_and_non_actionable_manifest_fields_are_recomputed() {
    let pixels = sample_pixels();
    let mut payload = sample_profile(&pixels);
    let (manifest, frame) = sample_preview_artifacts(&pixels);
    let mut corrupted_manifest = manifest.clone();
    corrupted_manifest[0] ^= 1;
    let review = sample_review(&payload);
    assert_eq!(
        check_untrusted_calibration_profile_v1(
            payload.clone(),
            review,
            &corrupted_manifest,
            &frame,
            &pixels,
        )
        .unwrap_err()
        .code(),
        "profile_preview_manifest_hash"
    );

    let mut manifest_value: serde_json::Value = serde_json::from_slice(&manifest).unwrap();
    manifest_value["safe_for_ocr"] = serde_json::Value::Bool(true);
    let unsafe_manifest = serde_json::to_vec(&manifest_value).unwrap();
    payload.source_preview_manifest_sha256 = digest(&unsafe_manifest);
    let review = sample_review(&payload);
    assert_eq!(
        check_untrusted_calibration_profile_v1(payload, review, &unsafe_manifest, &frame, &pixels,)
            .unwrap_err()
            .code(),
        "profile_preview_manifest_value"
    );
}

#[test]
fn preview_png_is_the_exact_source_of_canonical_pixels() {
    let pixels = sample_pixels();
    let payload = sample_profile(&pixels);
    let review = sample_review(&payload);
    let (manifest, frame) = sample_preview_artifacts(&pixels);
    let mut different_pixels = pixels.to_vec();
    different_pixels[0] ^= 1;
    assert_eq!(
        check_untrusted_calibration_profile_v1(
            payload,
            review,
            &manifest,
            &frame,
            &different_pixels,
        )
        .unwrap_err()
        .code(),
        "profile_preview_png_pixel_mismatch"
    );

    let invalid_frame = b"not-a-png".to_vec();
    let invalid_manifest = serde_json::to_vec(&sample_preview_value(&invalid_frame)).unwrap();
    let mut payload = sample_profile(&pixels);
    payload.source_preview_manifest_sha256 = digest(&invalid_manifest);
    payload.source_preview_frame_sha256 = digest(&invalid_frame);
    let review = sample_review(&payload);
    assert_eq!(
        check_untrusted_calibration_profile_v1(
            payload,
            review,
            &invalid_manifest,
            &invalid_frame,
            &pixels,
        )
        .unwrap_err()
        .code(),
        "profile_preview_png_decode"
    );

    let mut translucent_pixels = pixels.to_vec();
    translucent_pixels[3] = 254;
    let payload = sample_profile(&translucent_pixels);
    let review = sample_review(&payload);
    assert_eq!(
        check_profile(payload, review, &translucent_pixels)
            .unwrap_err()
            .code(),
        "profile_preview_png_alpha"
    );
}

#[test]
fn preview_capture_cannot_follow_its_review() {
    let pixels = sample_pixels();
    let frame = encode_preview_png(&pixels);

    let mut at_review = sample_preview_value(&frame);
    at_review["captured_at_utc"] =
        serde_json::Value::String("2026-08-09T16:45:00.0000000Z".to_owned());
    assert!(check_profile_with_manifest_value(sample_profile(&pixels), &pixels, at_review).is_ok());

    let mut after_review = sample_preview_value(&frame);
    after_review["captured_at_utc"] =
        serde_json::Value::String("2026-08-09T16:45:00.0000001Z".to_owned());
    assert_eq!(
        check_profile_with_manifest_value(sample_profile(&pixels), &pixels, after_review)
            .unwrap_err()
            .code(),
        "profile_preview_after_review"
    );
}

#[test]
fn preview_window_safety_facts_are_required() {
    let pixels = sample_pixels();
    let frame = encode_preview_png(&pixels);
    for (field, unsafe_value) in [
        ("foreground", serde_json::Value::Bool(false)),
        ("visible", serde_json::Value::Bool(false)),
        ("minimized", serde_json::Value::Bool(true)),
        ("cloaked", serde_json::Value::Bool(true)),
        ("display_affinity", serde_json::json!(1)),
        (
            "desktop_composition_enabled",
            serde_json::Value::Bool(false),
        ),
    ] {
        let mut manifest = sample_preview_value(&frame);
        manifest["window"][field] = unsafe_value;
        assert_eq!(
            check_profile_with_manifest_value(sample_profile(&pixels), &pixels, manifest)
                .unwrap_err()
                .code(),
            "profile_preview_manifest_value",
            "unsafe window field {field} was accepted"
        );
    }
}

#[test]
fn preview_occlusion_and_cursor_facts_are_required() {
    let pixels = sample_pixels();
    let frame = encode_preview_png(&pixels);
    for (field, unsafe_value) in [
        ("target_found", serde_json::Value::Bool(false)),
        ("intersecting_windows_above_target", serde_json::json!(1)),
        ("cursor_inside_client", serde_json::Value::Bool(true)),
    ] {
        let mut manifest = sample_preview_value(&frame);
        manifest["occlusion_audit"][field] = unsafe_value;
        assert_eq!(
            check_profile_with_manifest_value(sample_profile(&pixels), &pixels, manifest)
                .unwrap_err()
                .code(),
            "profile_preview_manifest_value",
            "unsafe occlusion field {field} was accepted"
        );
    }
}

#[test]
fn preview_monitor_identity_geometry_and_negative_origin_are_bound() {
    let pixels = sample_pixels();
    let frame = encode_preview_png(&pixels);
    assert!(check_profile_with_manifest_value(
        sample_profile(&pixels),
        &pixels,
        sample_preview_value(&frame),
    )
    .is_ok());

    let mut wrong_device = sample_preview_value(&frame);
    wrong_device["monitor"]["device_name"] = serde_json::Value::String("DISPLAY9".to_owned());
    assert_eq!(
        check_profile_with_manifest_value(sample_profile(&pixels), &pixels, wrong_device)
            .unwrap_err()
            .code(),
        "profile_preview_output_device"
    );

    let mut wrong_bounds = sample_preview_value(&frame);
    wrong_bounds["monitor"]["bounds_desktop_px"] = rect(-1920, 0, 1919, 1080);
    assert_eq!(
        check_profile_with_manifest_value(sample_profile(&pixels), &pixels, wrong_bounds)
            .unwrap_err()
            .code(),
        "profile_preview_output_bounds"
    );

    let mut outside = sample_preview_value(&frame);
    outside["window"]["client_bounds_desktop_px"] = rect(100, 100, 16, 16);
    outside["window"]["extended_frame_bounds_desktop_px"] = rect(98, 98, 20, 20);
    assert_eq!(
        check_profile_with_manifest_value(sample_profile(&pixels), &pixels, outside)
            .unwrap_err()
            .code(),
        "profile_preview_output_containment"
    );
}

#[test]
fn preview_frame_metadata_and_timestamps_are_strict() {
    let pixels = sample_pixels();
    let frame = encode_preview_png(&pixels);

    let mut wrong_file = sample_preview_value(&frame);
    wrong_file["frame"]["file"] = serde_json::Value::String("other.png".to_owned());
    assert_eq!(
        check_profile_with_manifest_value(sample_profile(&pixels), &pixels, wrong_file)
            .unwrap_err()
            .code(),
        "profile_preview_manifest_value"
    );

    let mut invalid_capture_time = sample_preview_value(&frame);
    invalid_capture_time["captured_at_utc"] =
        serde_json::Value::String("2026-99-99T99:99:99.9999999Z".to_owned());
    assert_eq!(
        check_profile_with_manifest_value(sample_profile(&pixels), &pixels, invalid_capture_time)
            .unwrap_err()
            .code(),
        "profile_preview_captured_at_utc"
    );

    let mut process_after_capture = sample_preview_value(&frame);
    process_after_capture["observed_identity"]["process_start_utc"] =
        serde_json::Value::String("2026-08-09T16:45:00.0000000Z".to_owned());
    assert_eq!(
        check_profile_with_manifest_value(sample_profile(&pixels), &pixels, process_after_capture)
            .unwrap_err()
            .code(),
        "profile_preview_process_chronology"
    );
}

#[test]
fn identity_dpi_size_and_output_drift_fail() {
    let (profile, pixels) = validated_fixture();
    let mut candidate = sample_candidate(&profile, &pixels);
    candidate.product_version = "3.4.159.0".to_owned();
    assert_eq!(
        check_frame(&profile, candidate, pixels.clone())
            .unwrap_err()
            .code(),
        "real_frame_identity_drift"
    );

    let mut candidate = sample_candidate(&profile, &pixels);
    candidate.dpi = 144;
    assert_eq!(
        check_frame(&profile, candidate, pixels.clone())
            .unwrap_err()
            .code(),
        "real_frame_layout_drift"
    );

    let mut candidate = sample_candidate(&profile, &pixels);
    candidate.client_rect_desktop_px.left = 1;
    assert_eq!(
        check_frame(&profile, candidate, pixels).unwrap_err().code(),
        "real_frame_output_containment"
    );
}

#[test]
fn invalid_duplicate_overlapping_and_out_of_bounds_anchors_fail() {
    let pixels = sample_pixels();
    let mut payload = sample_profile(&pixels);
    payload.anchors[1].anchor_id = "bottom_left".to_owned();
    let review = sample_review(&payload);
    assert_eq!(
        check_profile(payload, review, &pixels).unwrap_err().code(),
        "profile_anchor_duplicate"
    );

    let mut payload = sample_profile(&pixels);
    payload.anchors[1].rect_client_px = MtgoRectPxV1 {
        x: 0,
        y: 12,
        width: 4,
        height: 4,
    };
    payload.anchors[1].reference_bgra8_sha256 = crop_digest(&pixels, 16, 0, 12, 4, 4);
    let review = sample_review(&payload);
    assert_eq!(
        check_profile(payload, review, &pixels).unwrap_err().code(),
        "profile_anchor_overlap"
    );

    let mut payload = sample_profile(&pixels);
    payload.anchors[1].rect_client_px.x = 16;
    let review = sample_review(&payload);
    assert_eq!(
        check_profile(payload, review, &pixels).unwrap_err().code(),
        "profile_anchor_rect"
    );
}

#[test]
fn changed_anchor_pixel_fails_even_with_updated_full_frame_hash() {
    let (profile, pixels) = validated_fixture();
    let mut changed = pixels.into_vec();
    changed[0] ^= 0xff;
    let changed = changed.into_boxed_slice();
    let candidate = sample_candidate(&profile, &changed);
    assert_eq!(
        check_frame(&profile, candidate, changed)
            .unwrap_err()
            .code(),
        "real_frame_anchor_mismatch"
    );
}

#[test]
fn pixel_length_hash_format_and_backend_are_strict() {
    let (profile, pixels) = validated_fixture();
    let candidate = sample_candidate(&profile, &pixels);
    assert_eq!(
        check_frame(
            &profile,
            candidate,
            pixels[..pixels.len() - 1].to_vec().into_boxed_slice(),
        )
        .unwrap_err()
        .code(),
        "real_frame_pixel_length"
    );

    let mut candidate = sample_candidate(&profile, &pixels);
    candidate.canonical_client_pixels_sha256 = "c".repeat(64);
    assert_eq!(
        check_frame(&profile, candidate, pixels).unwrap_err().code(),
        "real_frame_pixel_hash"
    );

    assert!(serde_json::from_str::<MtgoCanonicalPixelFormatV1>("\"rgba8\"").is_err());
    assert!(serde_json::from_str::<MtgoCaptureBackendV1>("\"window_capture\"").is_err());
}

#[test]
fn every_capture_time_assertion_and_current_desktop_frame_are_required() {
    let (profile, pixels) = validated_fixture();
    let mut candidate = sample_candidate(&profile, &pixels);
    candidate.occlusion_free_confirmed = false;
    assert_eq!(
        check_frame(&profile, candidate, pixels.clone())
            .unwrap_err()
            .code(),
        "real_frame_safety_assertion"
    );

    let mut candidate = sample_candidate(&profile, &pixels);
    candidate.protected_content_absent_confirmed = false;
    assert_eq!(
        check_frame(&profile, candidate, pixels.clone())
            .unwrap_err()
            .code(),
        "real_frame_safety_assertion"
    );

    let mut candidate = sample_candidate(&profile, &pixels);
    candidate.last_present_time_qpc = 0;
    assert_eq!(
        check_frame(&profile, candidate, pixels).unwrap_err().code(),
        "real_frame_desktop_present"
    );
}

#[test]
fn capture_time_must_follow_review_and_be_validated_immediately() {
    let (profile, pixels) = validated_fixture();
    let mut candidate = sample_candidate(&profile, &pixels);
    candidate.captured_at_utc = "2026-08-09T16:44:59Z".to_owned();
    assert_eq!(
        check_frame(&profile, candidate, pixels.clone())
            .unwrap_err()
            .code(),
        "real_frame_before_review"
    );

    let mut candidate = sample_candidate(&profile, &pixels);
    candidate.captured_at_utc = "2026-08-09T16:45:30Z".to_owned();
    assert_eq!(
        check_frame(&profile, candidate, pixels.clone())
            .unwrap_err()
            .code(),
        "real_frame_freshness"
    );

    let mut candidate = sample_candidate(&profile, &pixels);
    candidate.captured_at_utc = "2026-99-99T99:99:99Z".to_owned();
    assert_eq!(
        check_frame(&profile, candidate, pixels).unwrap_err().code(),
        "captured_at_utc"
    );
}
