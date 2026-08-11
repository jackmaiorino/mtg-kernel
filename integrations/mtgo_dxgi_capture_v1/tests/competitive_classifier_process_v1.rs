#![cfg(target_os = "windows")]

use mtgo_blackbox_v1::{
    visible_frame_region_content_sha256_v1, MtgoCompetitiveEventKindV1,
    MtgoCompetitiveEventProgressV1, MtgoCompetitiveEventRecordVisibleFactKindV1,
    MtgoCompetitiveEventVisibleStatusV1, MtgoCompetitiveLifecyclePhaseV1,
    MtgoCompetitiveMatchRecordV1, MtgoLifecycleVisibleFactKindV1, MtgoRectPxV1, MtgoSizePxV1,
};
use mtgo_dxgi_capture_v1::{
    MtgoCompetitiveEventRecordClassifierProcessResponseV1,
    MtgoCompetitiveEventRecordClassifierRequestHeaderV1,
    MtgoCompetitiveNavigationClassifierProcessResponseV1,
    MtgoCompetitiveNavigationClassifierRequestHeaderV1,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::io::Write;
use std::process::{Command, Stdio};

const MAGIC_V1: &[u8] = b"MTGO_VISIBLE_COMPETITIVE_NAVIGATION_V1\0";
const EVENT_RECORD_MAGIC_V1: &[u8] = b"MTGO_VISIBLE_COMPETITIVE_EVENT_RECORD_V1\0";

#[derive(Serialize)]
struct AssetsV1 {
    schema_version: u32,
    scope: String,
    canonical_pixel_format: String,
    profiles: Vec<serde_json::Value>,
    navigation_profiles: Vec<NavigationProfileV1>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    event_record_profiles: Vec<EventRecordProfileV1>,
}

#[derive(Serialize)]
struct NavigationProfileV1 {
    profile_id: String,
    event_kind: MtgoCompetitiveEventKindV1,
    phase: MtgoCompetitiveLifecyclePhaseV1,
    client_size_px: MtgoSizePxV1,
    event_identity_sha256: Option<String>,
    match_identity_sha256: Option<String>,
    game_number: Option<u8>,
    entry_terms: Option<serde_json::Value>,
    facts: Vec<NavigationFactV1>,
}

#[derive(Serialize)]
struct NavigationFactV1 {
    kind: MtgoLifecycleVisibleFactKindV1,
    rect_client_px: MtgoRectPxV1,
    accepted_reference_sha256s: Vec<String>,
    confidence_bps: u16,
}

#[derive(Serialize)]
struct EventRecordProfileV1 {
    profile_id: String,
    navigation_profile_id: String,
    event_kind: MtgoCompetitiveEventKindV1,
    lifecycle_phase: MtgoCompetitiveLifecyclePhaseV1,
    event_identity_sha256: String,
    status: MtgoCompetitiveEventVisibleStatusV1,
    progress: MtgoCompetitiveEventProgressV1,
    completion: Option<serde_json::Value>,
    facts: Vec<EventRecordFactV1>,
}

#[derive(Serialize)]
struct EventRecordFactV1 {
    kind: MtgoCompetitiveEventRecordVisibleFactKindV1,
    rect_client_px: MtgoRectPxV1,
    accepted_reference_sha256s: Vec<String>,
    confidence_bps: u16,
}

#[test]
fn navigation_mode_completes_the_exact_framed_child_process_exchange() {
    let width = 32;
    let height = 16;
    let size = MtgoSizePxV1 { width, height };
    let pixels = (0..width * height * 4)
        .map(|index| ((index * 19 + 13) % 251) as u8)
        .collect::<Vec<_>>();
    let pairing_rect = MtgoRectPxV1 {
        x: 1,
        y: 1,
        width: 12,
        height: 5,
    };
    let accept_rect = MtgoRectPxV1 {
        x: 20,
        y: 4,
        width: 8,
        height: 4,
    };
    let assets = AssetsV1 {
        schema_version: 1,
        scope: "league_and_challenge_navigation_and_listing_exact_visible_regions_v1".to_owned(),
        canonical_pixel_format: "bgra8_unorm_top_down_tightly_packed_v1".to_owned(),
        profiles: Vec::new(),
        navigation_profiles: vec![NavigationProfileV1 {
            profile_id: "challenge-pairing-ready-32x16-process-v1".to_owned(),
            event_kind: MtgoCompetitiveEventKindV1::Challenge,
            phase: MtgoCompetitiveLifecyclePhaseV1::PairingReady,
            client_size_px: size.clone(),
            event_identity_sha256: Some(digest('a')),
            match_identity_sha256: Some(digest('b')),
            game_number: None,
            entry_terms: None,
            facts: vec![
                NavigationFactV1 {
                    kind: MtgoLifecycleVisibleFactKindV1::PairingVisible,
                    rect_client_px: pairing_rect.clone(),
                    accepted_reference_sha256s: vec![visible_frame_region_content_sha256_v1(
                        &pixels,
                        &size,
                        &pairing_rect,
                    )
                    .unwrap()],
                    confidence_bps: 9_500,
                },
                NavigationFactV1 {
                    kind: MtgoLifecycleVisibleFactKindV1::PairingAcceptControlEnabled,
                    rect_client_px: accept_rect.clone(),
                    accepted_reference_sha256s: vec![visible_frame_region_content_sha256_v1(
                        &pixels,
                        &size,
                        &accept_rect,
                    )
                    .unwrap()],
                    confidence_bps: 9_500,
                },
            ],
        }],
        event_record_profiles: Vec::new(),
    };
    let assets_json = serde_json::to_vec(&assets).unwrap();
    let executable = env!("CARGO_BIN_EXE_mtgo_visible_competitive_classifier_v1");
    let executable_bytes = std::fs::read(executable).unwrap();
    let header = MtgoCompetitiveNavigationClassifierRequestHeaderV1 {
        schema_version: 1,
        protocol: "mtgo_visible_competitive_navigation_v1".to_owned(),
        frame_id: 101,
        frame_sequence: 103,
        captured_at_unix_millis: 107,
        canonical_width: width,
        canonical_height: height,
        canonical_stride: width * 4,
        canonical_byte_length: pixels.len() as u64,
        canonical_bgra8_sha256: sha256_hex(&pixels),
        source_manifest_sha256: digest('1'),
        source_capture_commitment_sha256: digest('2'),
        source_profile_binding_sha256: digest('3'),
        source_frame_profile_binding_sha256: digest('4'),
        navigation_profile_commitment_sha256: digest('5'),
        navigation_profile_admission_commitment_sha256: digest('6'),
        approved_account_alias_sha256: digest('7'),
        runtime_identity_commitment_sha256: digest('8'),
        classifier_binary_sha256: sha256_hex(&executable_bytes),
        classifier_assets_manifest_sha256: sha256_hex(&assets_json),
    };
    let header_json = serde_json::to_vec(&header).unwrap();
    let mut child = Command::new(executable)
        .arg("--mtgo-visible-competitive-navigation-v1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    {
        let stdin = child.stdin.as_mut().unwrap();
        stdin.write_all(MAGIC_V1).unwrap();
        stdin
            .write_all(&(header_json.len() as u64).to_be_bytes())
            .unwrap();
        stdin
            .write_all(&(assets_json.len() as u64).to_be_bytes())
            .unwrap();
        stdin.write_all(&header_json).unwrap();
        stdin.write_all(&assets_json).unwrap();
        stdin.write_all(&pixels).unwrap();
    }
    drop(child.stdin.take());
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "classifier stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let response: MtgoCompetitiveNavigationClassifierProcessResponseV1 =
        serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(response.schema_version, 1);
    assert_eq!(
        response.lifecycle.phase,
        MtgoCompetitiveLifecyclePhaseV1::PairingReady
    );
    assert_eq!(
        response.lifecycle.event_kind,
        MtgoCompetitiveEventKindV1::Challenge
    );
    assert_eq!(response.lifecycle.frame_id, 101);
    assert_eq!(response.lifecycle.frame_sequence, 103);
    assert_eq!(response.lifecycle.facts.len(), 2);
}

#[test]
fn event_record_mode_completes_the_exact_framed_child_process_exchange() {
    let width = 32;
    let height = 16;
    let size = MtgoSizePxV1 { width, height };
    let pixels = (0..width * height * 4)
        .map(|index| ((index * 19 + 13) % 251) as u8)
        .collect::<Vec<_>>();
    let pairing_rect = MtgoRectPxV1 {
        x: 1,
        y: 1,
        width: 12,
        height: 5,
    };
    let accept_rect = MtgoRectPxV1 {
        x: 20,
        y: 4,
        width: 8,
        height: 4,
    };
    let status_rect = MtgoRectPxV1 {
        x: 1,
        y: 10,
        width: 12,
        height: 3,
    };
    let progress_rect = MtgoRectPxV1 {
        x: 16,
        y: 10,
        width: 12,
        height: 3,
    };
    let navigation_profile_id = "challenge-pairing-ready-32x16-process-v1";
    let assets = AssetsV1 {
        schema_version: 1,
        scope: "league_and_challenge_navigation_and_listing_exact_visible_regions_v1".to_owned(),
        canonical_pixel_format: "bgra8_unorm_top_down_tightly_packed_v1".to_owned(),
        profiles: Vec::new(),
        navigation_profiles: vec![NavigationProfileV1 {
            profile_id: navigation_profile_id.to_owned(),
            event_kind: MtgoCompetitiveEventKindV1::Challenge,
            phase: MtgoCompetitiveLifecyclePhaseV1::PairingReady,
            client_size_px: size.clone(),
            event_identity_sha256: Some(digest('a')),
            match_identity_sha256: Some(digest('b')),
            game_number: None,
            entry_terms: None,
            facts: vec![
                NavigationFactV1 {
                    kind: MtgoLifecycleVisibleFactKindV1::PairingVisible,
                    rect_client_px: pairing_rect.clone(),
                    accepted_reference_sha256s: vec![visible_frame_region_content_sha256_v1(
                        &pixels,
                        &size,
                        &pairing_rect,
                    )
                    .unwrap()],
                    confidence_bps: 9_500,
                },
                NavigationFactV1 {
                    kind: MtgoLifecycleVisibleFactKindV1::PairingAcceptControlEnabled,
                    rect_client_px: accept_rect.clone(),
                    accepted_reference_sha256s: vec![visible_frame_region_content_sha256_v1(
                        &pixels,
                        &size,
                        &accept_rect,
                    )
                    .unwrap()],
                    confidence_bps: 9_500,
                },
            ],
        }],
        event_record_profiles: vec![EventRecordProfileV1 {
            profile_id: "challenge-pairing-record-32x16-process-v1".to_owned(),
            navigation_profile_id: navigation_profile_id.to_owned(),
            event_kind: MtgoCompetitiveEventKindV1::Challenge,
            lifecycle_phase: MtgoCompetitiveLifecyclePhaseV1::PairingReady,
            event_identity_sha256: digest('a'),
            status: MtgoCompetitiveEventVisibleStatusV1::PairingReady,
            progress: MtgoCompetitiveEventProgressV1::Challenge {
                match_record: MtgoCompetitiveMatchRecordV1 {
                    wins: 0,
                    losses: 0,
                    draws: 0,
                    matches_completed: 0,
                },
                rounds_completed: 0,
                rounds_total: 8,
                match_points: 0,
                standing_rank: None,
                field_size: None,
            },
            completion: None,
            facts: vec![
                EventRecordFactV1 {
                    kind: MtgoCompetitiveEventRecordVisibleFactKindV1::EventStatusVisible,
                    rect_client_px: status_rect.clone(),
                    accepted_reference_sha256s: vec![visible_frame_region_content_sha256_v1(
                        &pixels,
                        &size,
                        &status_rect,
                    )
                    .unwrap()],
                    confidence_bps: 9_500,
                },
                EventRecordFactV1 {
                    kind: MtgoCompetitiveEventRecordVisibleFactKindV1::EventProgressVisible,
                    rect_client_px: progress_rect.clone(),
                    accepted_reference_sha256s: vec![visible_frame_region_content_sha256_v1(
                        &pixels,
                        &size,
                        &progress_rect,
                    )
                    .unwrap()],
                    confidence_bps: 9_500,
                },
            ],
        }],
    };
    let assets_json = serde_json::to_vec(&assets).unwrap();
    let executable = env!("CARGO_BIN_EXE_mtgo_visible_competitive_classifier_v1");
    let executable_bytes = std::fs::read(executable).unwrap();
    let header = MtgoCompetitiveEventRecordClassifierRequestHeaderV1 {
        schema_version: 1,
        protocol: "mtgo_visible_competitive_event_record_v1".to_owned(),
        frame_id: 211,
        frame_sequence: 223,
        captured_at_unix_millis: 227,
        canonical_width: width,
        canonical_height: height,
        canonical_stride: width * 4,
        canonical_byte_length: pixels.len() as u64,
        canonical_bgra8_sha256: sha256_hex(&pixels),
        source_capture_commitment_sha256: digest('1'),
        source_frame_profile_binding_sha256: digest('2'),
        parser_scope: "league_and_challenge_eight_slice_event_record_checked_untrusted_v1"
            .to_owned(),
        navigation_profile_commitment_sha256: digest('3'),
        navigation_profile_admission_commitment_sha256: digest('4'),
        approved_account_alias_sha256: digest('5'),
        runtime_identity_commitment_sha256: digest('6'),
        classifier_binary_sha256: sha256_hex(&executable_bytes),
        classifier_assets_manifest_sha256: sha256_hex(&assets_json),
    };
    let header_json = serde_json::to_vec(&header).unwrap();
    let mut child = Command::new(executable)
        .arg("--mtgo-visible-competitive-event-record-v1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    {
        let stdin = child.stdin.as_mut().unwrap();
        stdin.write_all(EVENT_RECORD_MAGIC_V1).unwrap();
        stdin
            .write_all(&(header_json.len() as u64).to_be_bytes())
            .unwrap();
        stdin
            .write_all(&(assets_json.len() as u64).to_be_bytes())
            .unwrap();
        stdin.write_all(&header_json).unwrap();
        stdin.write_all(&assets_json).unwrap();
        stdin.write_all(&pixels).unwrap();
    }
    drop(child.stdin.take());
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "classifier stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let response: MtgoCompetitiveEventRecordClassifierProcessResponseV1 =
        serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(response.schema_version, 1);
    assert_eq!(
        response.lifecycle.phase,
        MtgoCompetitiveLifecyclePhaseV1::PairingReady
    );
    assert_eq!(response.record.frame_id, 211);
    assert_eq!(response.record.frame_sequence, 223);
    assert_eq!(
        response.record.status,
        MtgoCompetitiveEventVisibleStatusV1::PairingReady
    );
    assert_eq!(response.record.facts.len(), 2);
}

fn digest(byte: char) -> String {
    byte.to_string().repeat(64)
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
