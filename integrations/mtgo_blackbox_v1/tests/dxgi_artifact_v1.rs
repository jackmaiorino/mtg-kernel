use mtgo_blackbox_v1::check_untrusted_dxgi_capture_artifact_v1;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn encode_png(raw_bgra: &[u8], width: u32, height: u32) -> Vec<u8> {
    let mut rgba = Vec::with_capacity(raw_bgra.len());
    for pixel in raw_bgra.chunks_exact(4) {
        rgba.extend_from_slice(&[pixel[2], pixel[1], pixel[0], pixel[3]]);
    }
    let mut output = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut output, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(&rgba).unwrap();
    }
    output
}

fn fixture() -> (Value, Vec<u8>, Vec<u8>) {
    let width = 8_u32;
    let height = 8_u32;
    let mut raw = Vec::new();
    for index in 0..(width * height) {
        raw.extend_from_slice(&[
            u8::try_from(index * 3).unwrap(),
            u8::try_from(index * 2).unwrap(),
            u8::try_from(index).unwrap(),
            255,
        ]);
    }
    let png = encode_png(&raw, width, height);
    let signer_subject = "CN=Daybreak Game Company LLC, O=Daybreak Game Company LLC";
    let snapshot = json!({
        "hwnd": 99,
        "process_id": 42,
        "mtgo_process_count": 1,
        "process_start_filetime_100ns": 133_000_000_000_000_000_u64,
        "process_image": "C:\\Apps\\MTGO.exe",
        "executable_sha256": "11".repeat(32),
        "authenticode_valid": true,
        "signer_thumbprint": "22".repeat(20),
        "signer_subject": signer_subject,
        "signer_subject_sha256": sha256(signer_subject.as_bytes()),
        "title": "Magic: The Gathering Online",
        "dpi": 120,
        "client_rect_desktop_px": { "left": -90, "top": 12, "right": -82, "bottom": 20 },
        "extended_frame_rect_desktop_px": { "left": -91, "top": 11, "right": -81, "bottom": 21 },
        "foreground": true,
        "visible": true,
        "minimized": false,
        "cloaked": false,
        "hung": false,
        "display_affinity": 0,
        "desktop_composition_enabled": true,
        "cursor_showing": true,
        "cursor_x": -100,
        "cursor_y": 0,
        "cursor_inside_client": false,
        "occlusion_target_found": true,
        "occluding_windows_above": 0,
        "z_order_sha256": "33".repeat(32)
    });
    let manifest = json!({
        "schema": "mtgo-dxgi-visible-frame-candidate/v1",
        "artifact_kind": "mtgo_untrusted_dxgi_visible_frame_candidate_v1",
        "status": "checked_untrusted_not_admitted",
        "capture_backend": "dxgi_desktop_duplication_v1",
        "captured_at_unix_millis": 1_786_333_555_000_u64,
        "safety": {
            "safe_for_semantic_evidence": false,
            "safe_for_ocr": false,
            "safe_for_policy_scoring": false,
            "safe_for_input": false,
            "authenticode_verified_in_probe": true
        },
        "pre": snapshot.clone(),
        "post": snapshot,
        "output": {
            "adapter_index": 0,
            "output_index": 1,
            "adapter_luid_low": 1234,
            "adapter_luid_high": -1,
            "device_name": "\\\\.\\DISPLAY2",
            "bounds_desktop_px": { "left": -100, "top": 0, "right": -68, "bottom": 32 },
            "rotation": 1,
            "color_space": 0
        },
        "frame": {
            "last_present_time_qpc": 987_654,
            "last_mouse_update_time_qpc": 123,
            "accumulated_frames": 1,
            "protected_content_masked_out": false,
            "pointer_visible": true,
            "pointer_x": -100,
            "pointer_y": 0,
            "source_texture_width": 32,
            "source_texture_height": 32,
            "source_texture_format": 87,
            "canonical_width": width,
            "canonical_height": height,
            "canonical_stride": width * 4,
            "canonical_byte_length": raw.len(),
            "canonical_bgra8_sha256": sha256(&raw),
            "preview_png_sha256": sha256(&png)
        },
        "files": {
            "canonical_pixels": "frame.bgra",
            "preview_png": "frame.png",
            "manifest": "manifest.json"
        }
    });
    (manifest, raw, png)
}

fn check(manifest: &Value, raw: &[u8], png: &[u8]) -> Result<(), &'static str> {
    let bytes = serde_json::to_vec(manifest).unwrap();
    check_untrusted_dxgi_capture_artifact_v1(&bytes, raw, png)
        .map(|_| ())
        .map_err(|error| error.code())
}

#[test]
fn valid_artifact_is_structurally_checked_but_never_actionable() {
    let (manifest, raw, png) = fixture();
    let bytes = serde_json::to_vec(&manifest).unwrap();
    let checked = check_untrusted_dxgi_capture_artifact_v1(&bytes, &raw, &png).unwrap();
    assert_eq!(checked.manifest_sha256(), sha256(&bytes));
    assert_eq!(checked.canonical_bgra8_sha256(), sha256(&raw));
    assert_eq!(checked.preview_png_sha256(), sha256(&png));
    assert_eq!(checked.output_identity_sha256().len(), 64);
    assert_eq!(checked.client_size_px().width, 8);
    assert_eq!(checked.client_size_px().height, 8);
    assert_eq!(checked.captured_at_unix_millis(), 1_786_333_555_000);
    assert!(!checked.safe_for_semantic_evidence());
    assert!(!checked.safe_for_ocr());
    assert!(!checked.safe_for_policy_scoring());
    assert!(!checked.safe_for_input());
}

#[test]
fn raw_png_hash_and_decoded_pixel_tampering_fail() {
    let (manifest, raw, png) = fixture();

    let mut changed_raw = raw.clone();
    changed_raw[0] ^= 1;
    assert_eq!(
        check(&manifest, &changed_raw, &png),
        Err("dxgi_artifact_file_hash")
    );

    let mut changed_png = png.clone();
    changed_png.push(0);
    assert_eq!(
        check(&manifest, &raw, &changed_png),
        Err("dxgi_artifact_file_hash")
    );

    let mut other_raw = raw.clone();
    other_raw[4] ^= 1;
    let other_png = encode_png(&other_raw, 8, 8);
    let mut mismatch_manifest = manifest.clone();
    mismatch_manifest["frame"]["preview_png_sha256"] = json!(sha256(&other_png));
    assert_eq!(
        check(&mismatch_manifest, &raw, &other_png),
        Err("dxgi_artifact_png_pixel_mismatch")
    );
}

#[test]
fn header_and_safety_policy_fail_closed() {
    let (manifest, raw, png) = fixture();
    let mutations = [
        ("/schema", json!("v2")),
        ("/artifact_kind", json!("other")),
        ("/status", json!("admitted")),
        ("/capture_backend", json!("print_window")),
        ("/captured_at_unix_millis", json!(0)),
        ("/safety/safe_for_semantic_evidence", json!(true)),
        ("/safety/safe_for_ocr", json!(true)),
        ("/safety/safe_for_policy_scoring", json!(true)),
        ("/safety/safe_for_input", json!(true)),
        ("/safety/authenticode_verified_in_probe", json!(false)),
    ];
    for (pointer, value) in mutations {
        let mut changed = manifest.clone();
        *changed.pointer_mut(pointer).unwrap() = value;
        assert!(check(&changed, &raw, &png).is_err(), "accepted {pointer}");
    }
}

#[test]
fn snapshot_drift_and_process_cardinality_fail() {
    let (manifest, raw, png) = fixture();
    let mut drift = manifest.clone();
    drift["post"]["process_id"] = json!(43);
    assert_eq!(
        check(&drift, &raw, &png),
        Err("dxgi_artifact_snapshot_drift")
    );

    for (pointer, value) in [
        ("/pre/hwnd", json!(0)),
        ("/pre/process_id", json!(0)),
        ("/pre/mtgo_process_count", json!(2)),
        ("/pre/process_start_filetime_100ns", json!(0)),
    ] {
        let mut changed = manifest.clone();
        *changed.pointer_mut(pointer).unwrap() = value.clone();
        let post_pointer = pointer.replacen("/pre/", "/post/", 1);
        *changed.pointer_mut(&post_pointer).unwrap() = value;
        assert_eq!(
            check(&changed, &raw, &png),
            Err("dxgi_artifact_process_identity")
        );
    }
}

#[test]
fn visible_window_geometry_cursor_and_occlusion_fail() {
    let (manifest, raw, png) = fixture();
    let mutations = [
        ("/foreground", json!(false)),
        ("/visible", json!(false)),
        ("/minimized", json!(true)),
        ("/cloaked", json!(true)),
        ("/hung", json!(true)),
        ("/display_affinity", json!(1)),
        ("/desktop_composition_enabled", json!(false)),
        ("/cursor_inside_client", json!(true)),
        ("/occlusion_target_found", json!(false)),
        ("/occluding_windows_above", json!(1)),
    ];
    for (suffix, value) in mutations {
        let mut changed = manifest.clone();
        *changed.pointer_mut(&format!("/pre{suffix}")).unwrap() = value.clone();
        *changed.pointer_mut(&format!("/post{suffix}")).unwrap() = value;
        assert!(check(&changed, &raw, &png).is_err(), "accepted {suffix}");
    }

    let mut bad_geometry = manifest.clone();
    for side in ["pre", "post"] {
        bad_geometry[side]["extended_frame_rect_desktop_px"]["right"] = json!(-85);
    }
    assert_eq!(
        check(&bad_geometry, &raw, &png),
        Err("dxgi_artifact_window_geometry")
    );

    let mut pointer_inside = manifest.clone();
    pointer_inside["frame"]["pointer_x"] = json!(-85);
    pointer_inside["frame"]["pointer_y"] = json!(15);
    assert_eq!(
        check(&pointer_inside, &raw, &png),
        Err("dxgi_artifact_pointer")
    );
}

#[test]
fn output_frame_format_stride_and_presence_fail() {
    let (manifest, raw, png) = fixture();
    let mutations = [
        ("/output/rotation", json!(2)),
        ("/output/color_space", json!(12)),
        ("/frame/last_present_time_qpc", json!(0)),
        ("/frame/last_mouse_update_time_qpc", json!(-1)),
        ("/frame/accumulated_frames", json!(0)),
        ("/frame/protected_content_masked_out", json!(true)),
        ("/frame/source_texture_width", json!(31)),
        ("/frame/source_texture_format", json!(28)),
        ("/frame/canonical_width", json!(7)),
        ("/frame/canonical_stride", json!(31)),
        ("/frame/canonical_byte_length", json!(255)),
        ("/files/canonical_pixels", json!("other.bgra")),
        ("/output/device_name", json!("DISPLAY2")),
    ];
    for (pointer, value) in mutations {
        let mut changed = manifest.clone();
        *changed.pointer_mut(pointer).unwrap() = value;
        assert!(check(&changed, &raw, &png).is_err(), "accepted {pointer}");
    }

    let mut outside = manifest.clone();
    outside["output"]["bounds_desktop_px"]["right"] = json!(-85);
    assert_eq!(
        check(&outside, &raw, &png),
        Err("dxgi_artifact_output_identity")
    );
}

#[test]
fn executable_and_signer_identity_tampering_fail() {
    let (manifest, raw, png) = fixture();
    let mutations = [
        ("/executable_sha256", json!("AA".repeat(32))),
        ("/signer_thumbprint", json!("22".repeat(19))),
        ("/signer_subject_sha256", json!("44".repeat(32))),
        ("/authenticode_valid", json!(false)),
        ("/process_image", json!("C:\\Apps\\other.exe")),
        ("/process_image", json!("folder\\MTGO.exe")),
        ("/title", json!("Other client")),
        ("/z_order_sha256", json!("invalid")),
    ];
    for (suffix, value) in mutations {
        let mut changed = manifest.clone();
        *changed.pointer_mut(&format!("/pre{suffix}")).unwrap() = value.clone();
        *changed.pointer_mut(&format!("/post{suffix}")).unwrap() = value;
        assert!(check(&changed, &raw, &png).is_err(), "accepted {suffix}");
    }
}

#[test]
fn unknown_fields_and_hidden_input_channels_are_rejected() {
    let (manifest, raw, png) = fixture();
    for (field, value) in [
        ("input_coordinates", json!({"x": 1, "y": 2})),
        ("process_memory", json!("hidden")),
        ("network_capture", json!(true)),
    ] {
        let mut changed = manifest.clone();
        changed
            .as_object_mut()
            .unwrap()
            .insert(field.to_owned(), value);
        assert_eq!(check(&changed, &raw, &png), Err("dxgi_artifact_manifest"));
    }
}
