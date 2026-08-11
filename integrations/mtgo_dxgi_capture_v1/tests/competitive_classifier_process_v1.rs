#![cfg(target_os = "windows")]

use mtg_kernel::rl::ActionSemanticV1;
use mtg_kernel::rl_session::{RlEpisodeSessionV1, RlSessionResponseV1};
use mtgo_blackbox_v1::{
    local_metadata_commitment_v1, payload_leaf_inventory_v1, validate_competitive_deck_manifest_v1,
    validate_observed_decision_v1, validate_visible_competitive_lifecycle_snapshot_v1,
    visible_frame_region_content_sha256_v1, MtgoCalibrationCaptureRoleV1,
    MtgoCalibrationFrameReferenceV1, MtgoCalibrationPreviewKindV1, MtgoCalibrationPreviewStatusV1,
    MtgoCompetitiveDeckCardCountV1, MtgoCompetitiveDeckConfigurationV1,
    MtgoCompetitiveDeckManifestV1, MtgoCompetitiveDeckPartitionV1, MtgoCompetitiveEventKindV1,
    MtgoCompetitiveEventProgressV1, MtgoCompetitiveEventRecordVisibleFactKindV1,
    MtgoCompetitiveEventVisibleStatusV1, MtgoCompetitiveLifecyclePhaseV1,
    MtgoCompetitiveMatchRecordV1, MtgoDecisionReadinessV1, MtgoEvidenceSourceV1,
    MtgoLeafProvenanceV1, MtgoLifecycleVisibleFactKindV1, MtgoLifecycleVisibleFactV1,
    MtgoMockFrameV1, MtgoObjectBindingV1, MtgoObservationReconstructionAuditV1,
    MtgoObservationReconstructionGroupAuditV1, MtgoObservationReconstructionGroupV1,
    MtgoObservedDecisionV1, MtgoReconstructionStatusV1, MtgoReconstructionTopologyV1, MtgoRectPxV1,
    MtgoSemanticDecisionPayloadV1, MtgoSizePxV1, MtgoVisibleActionControlCandidateV1,
    MtgoVisibleActionControlSetV1, MtgoVisibleCompetitiveLifecycleSnapshotV1,
    MtgoVisibleControlKindV1, MtgoVisibleEvidenceV1, MtgoVisibleRegionCommitmentV1,
    MTGO_COMPETITIVE_SIDEBOARD_SCHEMA_V1,
};
use mtgo_dxgi_capture_v1::{
    MtgoCompetitiveEventRecordClassifierProcessResponseV1,
    MtgoCompetitiveEventRecordClassifierRequestHeaderV1,
    MtgoCompetitiveNavigationClassifierProcessResponseV1,
    MtgoCompetitiveNavigationClassifierRequestHeaderV1,
    MtgoCompetitiveSideboardClassifierProcessResponseV1,
    MtgoCompetitiveSideboardClassifierRequestHeaderV1, MtgoDuelPerceptionProcessResponseV1,
    MtgoDuelPerceptionRequestHeaderV1,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::io::Write;
use std::process::{Command, Stdio};

const MAGIC_V1: &[u8] = b"MTGO_VISIBLE_COMPETITIVE_NAVIGATION_V1\0";
const EVENT_RECORD_MAGIC_V1: &[u8] = b"MTGO_VISIBLE_COMPETITIVE_EVENT_RECORD_V1\0";
const SIDEBOARD_MAGIC_V1: &[u8] = b"MTGO_VISIBLE_COMPETITIVE_SIDEBOARD_V1\0";
const DUEL_PERCEPTION_MAGIC_V1: &[u8] = b"MTGO_VISIBLE_DUEL_PERCEPTION_V1\0";

#[derive(Serialize)]
struct AssetsV1 {
    schema_version: u32,
    scope: String,
    canonical_pixel_format: String,
    profiles: Vec<serde_json::Value>,
    navigation_profiles: Vec<NavigationProfileV1>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    event_record_profiles: Vec<EventRecordProfileV1>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    sideboard_profiles: Vec<SideboardProfileV1>,
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

#[derive(Serialize)]
struct SideboardProfileV1 {
    profile_id: String,
    navigation_profile_id: String,
    deck_manifest: MtgoCompetitiveDeckManifestV1,
    mainboard_zone: SideboardZoneProfileV1,
    sideboard_zone: SideboardZoneProfileV1,
    cards: Vec<SideboardCardProfileV1>,
}

#[derive(Serialize)]
struct SideboardZoneProfileV1 {
    rect_client_px: MtgoRectPxV1,
    accepted_reference_sha256s: Vec<String>,
    empty_drop_rect_client_px: MtgoRectPxV1,
    accepted_empty_drop_reference_sha256s: Vec<String>,
    confidence_bps: u16,
}

#[derive(Serialize)]
struct SideboardCardProfileV1 {
    partition: MtgoCompetitiveDeckPartitionV1,
    card_db_id: u16,
    card_name: String,
    count: u16,
    rect_client_px: MtgoRectPxV1,
    accepted_reference_sha256s: Vec<String>,
    confidence_bps: u16,
}

#[derive(Serialize)]
struct DuelPerceptionAssetsV1 {
    schema_version: u32,
    scope: String,
    canonical_pixel_format: String,
    profiles: Vec<DuelPerceptionProfileV1>,
}

#[derive(Serialize)]
struct DuelPerceptionProfileV1 {
    profile_id: String,
    perception_profile_commitment_sha256: String,
    perception_profile_admission_commitment_sha256: String,
    client_size_px: MtgoSizePxV1,
    decision_template: MtgoObservedDecisionV1,
    reconstruction_audit_template: MtgoObservationReconstructionAuditV1,
    visible_controls_template: MtgoVisibleActionControlSetV1,
    competitive_lifecycle_template: Option<MtgoVisibleCompetitiveLifecycleSnapshotV1>,
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
        sideboard_profiles: Vec::new(),
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
        sideboard_profiles: Vec::new(),
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
    let navigation_header = MtgoCompetitiveNavigationClassifierRequestHeaderV1 {
        schema_version: 1,
        protocol: "mtgo_visible_competitive_navigation_v1".to_owned(),
        frame_id: header.frame_id,
        frame_sequence: header.frame_sequence,
        captured_at_unix_millis: header.captured_at_unix_millis,
        canonical_width: width,
        canonical_height: height,
        canonical_stride: width * 4,
        canonical_byte_length: pixels.len() as u64,
        canonical_bgra8_sha256: header.canonical_bgra8_sha256.clone(),
        source_manifest_sha256: digest('0'),
        source_capture_commitment_sha256: header.source_capture_commitment_sha256.clone(),
        source_profile_binding_sha256: digest('9'),
        source_frame_profile_binding_sha256: header.source_frame_profile_binding_sha256.clone(),
        navigation_profile_commitment_sha256: header.navigation_profile_commitment_sha256.clone(),
        navigation_profile_admission_commitment_sha256: header
            .navigation_profile_admission_commitment_sha256
            .clone(),
        approved_account_alias_sha256: header.approved_account_alias_sha256.clone(),
        runtime_identity_commitment_sha256: header.runtime_identity_commitment_sha256.clone(),
        classifier_binary_sha256: header.classifier_binary_sha256.clone(),
        classifier_assets_manifest_sha256: header.classifier_assets_manifest_sha256.clone(),
    };
    let navigation_header_json = serde_json::to_vec(&navigation_header).unwrap();
    let navigation_output = invoke_classifier_v1(
        executable,
        "--mtgo-visible-competitive-navigation-v1",
        MAGIC_V1,
        &navigation_header_json,
        &assets_json,
        &pixels,
    );
    assert!(
        navigation_output.status.success(),
        "navigation classifier stderr: {}",
        String::from_utf8_lossy(&navigation_output.stderr)
    );
    let navigation_response: MtgoCompetitiveNavigationClassifierProcessResponseV1 =
        serde_json::from_slice(&navigation_output.stdout).unwrap();
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
        response.request_commitment_sha256,
        commitment_hex(
            b"mtgo-competitive-event-record-classifier-request-v1",
            &[&header_json, &assets_json, &pixels],
        )
    );
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
    assert_eq!(response.lifecycle, navigation_response.lifecycle);
    assert_eq!(response.record.facts.len(), 2);
}

#[test]
fn sideboard_mode_completes_the_exact_framed_child_process_exchange() {
    let width = 64;
    let height = 48;
    let size = MtgoSizePxV1 { width, height };
    let pixels = (0..width * height * 4)
        .map(|index| ((index * 31 + 37) % 251) as u8)
        .collect::<Vec<_>>();
    let lifecycle_specs = [
        (
            MtgoLifecycleVisibleFactKindV1::SideboardSurfaceVisible,
            MtgoRectPxV1 {
                x: 1,
                y: 1,
                width: 10,
                height: 6,
            },
        ),
        (
            MtgoLifecycleVisibleFactKindV1::SideboardTimerVisible,
            MtgoRectPxV1 {
                x: 13,
                y: 1,
                width: 10,
                height: 6,
            },
        ),
        (
            MtgoLifecycleVisibleFactKindV1::SideboardConfigurationVisible,
            MtgoRectPxV1 {
                x: 25,
                y: 1,
                width: 10,
                height: 6,
            },
        ),
        (
            MtgoLifecycleVisibleFactKindV1::SideboardSubmitControlEnabled,
            MtgoRectPxV1 {
                x: 37,
                y: 1,
                width: 10,
                height: 6,
            },
        ),
    ];
    let mainboard_zone_rect = MtgoRectPxV1 {
        x: 0,
        y: 12,
        width: 64,
        height: 15,
    };
    let mainboard_empty_rect = MtgoRectPxV1 {
        x: 50,
        y: 14,
        width: 8,
        height: 8,
    };
    let sideboard_zone_rect = MtgoRectPxV1 {
        x: 0,
        y: 30,
        width: 64,
        height: 15,
    };
    let sideboard_empty_rect = MtgoRectPxV1 {
        x: 50,
        y: 32,
        width: 8,
        height: 8,
    };
    let mainboard_card_rect = MtgoRectPxV1 {
        x: 2,
        y: 14,
        width: 8,
        height: 8,
    };
    let sideboard_card_rect = MtgoRectPxV1 {
        x: 2,
        y: 32,
        width: 8,
        height: 8,
    };
    let region_hash =
        |rect: &MtgoRectPxV1| visible_frame_region_content_sha256_v1(&pixels, &size, rect).unwrap();
    let deck_manifest = MtgoCompetitiveDeckManifestV1 {
        schema_version: MTGO_COMPETITIVE_SIDEBOARD_SCHEMA_V1,
        deck_list_sha256: digest('1'),
        format_sha256: digest('2'),
        starting_mainboard_count: 1,
        starting_sideboard_count: 1,
        configuration: MtgoCompetitiveDeckConfigurationV1 {
            mainboard: vec![MtgoCompetitiveDeckCardCountV1 {
                card_db_id: 66,
                card_name: "Lightning Bolt".to_owned(),
                count: 1,
            }],
            sideboard: vec![MtgoCompetitiveDeckCardCountV1 {
                card_db_id: 101,
                card_name: "Searing Blaze".to_owned(),
                count: 1,
            }],
        },
    };
    let assets = AssetsV1 {
        schema_version: 1,
        scope: "league_and_challenge_navigation_and_listing_exact_visible_regions_v1".to_owned(),
        canonical_pixel_format: "bgra8_unorm_top_down_tightly_packed_v1".to_owned(),
        profiles: Vec::new(),
        navigation_profiles: vec![NavigationProfileV1 {
            profile_id: "league-sideboard-64x48-process-v1".to_owned(),
            event_kind: MtgoCompetitiveEventKindV1::League,
            phase: MtgoCompetitiveLifecyclePhaseV1::Sideboarding,
            client_size_px: size.clone(),
            event_identity_sha256: Some(digest('a')),
            match_identity_sha256: Some(digest('b')),
            game_number: Some(1),
            entry_terms: None,
            facts: lifecycle_specs
                .iter()
                .map(|(kind, rect)| NavigationFactV1 {
                    kind: *kind,
                    rect_client_px: rect.clone(),
                    accepted_reference_sha256s: vec![region_hash(rect)],
                    confidence_bps: 9_500,
                })
                .collect(),
        }],
        event_record_profiles: Vec::new(),
        sideboard_profiles: vec![SideboardProfileV1 {
            profile_id: "league-sideboard-deck-64x48-process-v1".to_owned(),
            navigation_profile_id: "league-sideboard-64x48-process-v1".to_owned(),
            deck_manifest: deck_manifest.clone(),
            mainboard_zone: SideboardZoneProfileV1 {
                rect_client_px: mainboard_zone_rect.clone(),
                accepted_reference_sha256s: vec![region_hash(&mainboard_zone_rect)],
                empty_drop_rect_client_px: mainboard_empty_rect.clone(),
                accepted_empty_drop_reference_sha256s: vec![region_hash(&mainboard_empty_rect)],
                confidence_bps: 9_500,
            },
            sideboard_zone: SideboardZoneProfileV1 {
                rect_client_px: sideboard_zone_rect.clone(),
                accepted_reference_sha256s: vec![region_hash(&sideboard_zone_rect)],
                empty_drop_rect_client_px: sideboard_empty_rect.clone(),
                accepted_empty_drop_reference_sha256s: vec![region_hash(&sideboard_empty_rect)],
                confidence_bps: 9_500,
            },
            cards: vec![
                SideboardCardProfileV1 {
                    partition: MtgoCompetitiveDeckPartitionV1::Mainboard,
                    card_db_id: 66,
                    card_name: "Lightning Bolt".to_owned(),
                    count: 1,
                    rect_client_px: mainboard_card_rect.clone(),
                    accepted_reference_sha256s: vec![region_hash(&mainboard_card_rect)],
                    confidence_bps: 9_500,
                },
                SideboardCardProfileV1 {
                    partition: MtgoCompetitiveDeckPartitionV1::Sideboard,
                    card_db_id: 101,
                    card_name: "Searing Blaze".to_owned(),
                    count: 1,
                    rect_client_px: sideboard_card_rect.clone(),
                    accepted_reference_sha256s: vec![region_hash(&sideboard_card_rect)],
                    confidence_bps: 9_500,
                },
            ],
        }],
    };
    let assets_json = serde_json::to_vec(&assets).unwrap();
    let executable = env!("CARGO_BIN_EXE_mtgo_visible_competitive_classifier_v1");
    let executable_bytes = std::fs::read(executable).unwrap();
    let navigation_header = MtgoCompetitiveNavigationClassifierRequestHeaderV1 {
        schema_version: 1,
        protocol: "mtgo_visible_competitive_navigation_v1".to_owned(),
        frame_id: 307,
        frame_sequence: 311,
        captured_at_unix_millis: 313,
        canonical_width: width,
        canonical_height: height,
        canonical_stride: width * 4,
        canonical_byte_length: pixels.len() as u64,
        canonical_bgra8_sha256: sha256_hex(&pixels),
        source_manifest_sha256: digest('3'),
        source_capture_commitment_sha256: digest('4'),
        source_profile_binding_sha256: digest('5'),
        source_frame_profile_binding_sha256: digest('6'),
        navigation_profile_commitment_sha256: digest('7'),
        navigation_profile_admission_commitment_sha256: digest('8'),
        approved_account_alias_sha256: digest('9'),
        runtime_identity_commitment_sha256: digest('c'),
        classifier_binary_sha256: sha256_hex(&executable_bytes),
        classifier_assets_manifest_sha256: sha256_hex(&assets_json),
    };
    let navigation_header_json = serde_json::to_vec(&navigation_header).unwrap();
    let navigation_output = invoke_classifier_v1(
        executable,
        "--mtgo-visible-competitive-navigation-v1",
        MAGIC_V1,
        &navigation_header_json,
        &assets_json,
        &pixels,
    );
    assert!(
        navigation_output.status.success(),
        "navigation classifier stderr: {}",
        String::from_utf8_lossy(&navigation_output.stderr)
    );
    let navigation_response: MtgoCompetitiveNavigationClassifierProcessResponseV1 =
        serde_json::from_slice(&navigation_output.stdout).unwrap();
    let lifecycle =
        validate_visible_competitive_lifecycle_snapshot_v1(navigation_response.lifecycle.clone())
            .unwrap();
    let manifest = validate_competitive_deck_manifest_v1(deck_manifest).unwrap();
    let sideboard_header = MtgoCompetitiveSideboardClassifierRequestHeaderV1 {
        schema_version: 1,
        protocol: "mtgo_visible_competitive_sideboard_v1".to_owned(),
        parser_scope:
            "league_or_challenge_exact_deck_policy_between_game_sideboard_checked_untrusted_v1"
                .to_owned(),
        frame_id: navigation_header.frame_id,
        frame_sequence: navigation_header.frame_sequence,
        captured_at_unix_millis: navigation_header.captured_at_unix_millis,
        canonical_width: width,
        canonical_height: height,
        canonical_stride: width * 4,
        canonical_byte_length: pixels.len() as u64,
        canonical_bgra8_sha256: navigation_header.canonical_bgra8_sha256.clone(),
        source_capture_commitment_sha256: navigation_header
            .source_capture_commitment_sha256
            .clone(),
        source_frame_profile_binding_sha256: navigation_header
            .source_frame_profile_binding_sha256
            .clone(),
        source_navigation_classification_result_commitment_sha256: digest('d'),
        source_lifecycle_snapshot_commitment_sha256: lifecycle
            .snapshot_commitment_sha256()
            .to_owned(),
        navigation_profile_commitment_sha256: navigation_header
            .navigation_profile_commitment_sha256
            .clone(),
        navigation_profile_admission_commitment_sha256: navigation_header
            .navigation_profile_admission_commitment_sha256
            .clone(),
        approved_account_alias_sha256: navigation_header.approved_account_alias_sha256.clone(),
        runtime_identity_commitment_sha256: navigation_header
            .runtime_identity_commitment_sha256
            .clone(),
        classifier_binary_sha256: navigation_header.classifier_binary_sha256.clone(),
        classifier_assets_manifest_sha256: navigation_header
            .classifier_assets_manifest_sha256
            .clone(),
        deck_list_sha256: manifest.deck_list_sha256().to_owned(),
        deck_manifest_commitment_sha256: manifest.manifest_commitment_sha256().to_owned(),
        deck_format_sha256: manifest.format_sha256().to_owned(),
        policy_deployment_commitment_sha256: digest('f'),
    };
    let sideboard_header_json = serde_json::to_vec(&sideboard_header).unwrap();
    let sideboard_output = invoke_classifier_v1(
        executable,
        "--mtgo-visible-competitive-sideboard-v1",
        SIDEBOARD_MAGIC_V1,
        &sideboard_header_json,
        &assets_json,
        &pixels,
    );
    assert!(
        sideboard_output.status.success(),
        "sideboard classifier stderr: {}",
        String::from_utf8_lossy(&sideboard_output.stderr)
    );
    let response: MtgoCompetitiveSideboardClassifierProcessResponseV1 =
        serde_json::from_slice(&sideboard_output.stdout).unwrap();
    assert_eq!(response.schema_version, 1);
    assert_eq!(
        response.request_commitment_sha256,
        commitment_hex(
            b"mtgo-competitive-sideboard-classifier-request-v1",
            &[
                &sideboard_header_json,
                &assets_json,
                &pixels,
                b"checked_untrusted_sideboard_request_no_live_classification_no_input",
            ],
        )
    );
    assert_eq!(response.lifecycle, navigation_response.lifecycle);
    assert_eq!(
        response.sideboard.event_kind,
        MtgoCompetitiveEventKindV1::League
    );
    assert_eq!(response.sideboard.game_number, 1);
    assert_eq!(response.sideboard.cards.len(), 2);
}

#[test]
fn duel_perception_mode_completes_the_exact_pinned_artifact_exchange() {
    let width = 64;
    let height = 64;
    let size = MtgoSizePxV1 { width, height };
    let pixels = (0..width * height * 4)
        .map(|index| ((index * 37 + 41) % 251) as u8)
        .collect::<Vec<_>>();
    let (decision, audit, controls, lifecycle) = duel_fixture_v1(&pixels, &size);
    let profile_commitment = digest('3');
    let profile_admission = digest('4');
    let assets = DuelPerceptionAssetsV1 {
        schema_version: 1,
        scope: "acting_player_duel_exact_visible_semantic_reference_checked_untrusted_v1"
            .to_owned(),
        canonical_pixel_format: "bgra8_unorm_top_down_tightly_packed_v1".to_owned(),
        profiles: vec![DuelPerceptionProfileV1 {
            profile_id: "league-duel-pass-land-64x64-process-v1".to_owned(),
            perception_profile_commitment_sha256: profile_commitment.clone(),
            perception_profile_admission_commitment_sha256: profile_admission.clone(),
            client_size_px: size,
            decision_template: decision,
            reconstruction_audit_template: audit,
            visible_controls_template: controls,
            competitive_lifecycle_template: Some(lifecycle),
        }],
    };
    let assets_json = serde_json::to_vec(&assets).unwrap();
    let card_database_bytes = b"synthetic-exact-card-database-profile-v1";
    let temp_root = std::env::temp_dir().join(format!(
        "mtgo-duel-classifier-process-v1-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&temp_root).unwrap();
    let assets_path = temp_root.join("classifier-assets.json");
    let card_database_path = temp_root.join("card-database.json");
    std::fs::write(&assets_path, &assets_json).unwrap();
    std::fs::write(&card_database_path, card_database_bytes).unwrap();

    let executable = env!("CARGO_BIN_EXE_mtgo_visible_competitive_classifier_v1");
    let executable_bytes = std::fs::read(executable).unwrap();
    let header = MtgoDuelPerceptionRequestHeaderV1 {
        schema_version: 1,
        protocol: "mtgo_visible_duel_perception_v1".to_owned(),
        frame_id: 409,
        frame_sequence: 419,
        canonical_width: width,
        canonical_height: height,
        canonical_stride: width * 4,
        canonical_byte_length: pixels.len(),
        canonical_bgra8_sha256: sha256_hex(&pixels),
        source_manifest_sha256: digest('5'),
        source_capture_commitment_sha256: digest('6'),
        source_frame_profile_binding_sha256: digest('7'),
        perception_profile_commitment_sha256: profile_commitment,
        perception_profile_admission_commitment_sha256: profile_admission,
        runtime_identity_commitment_sha256: digest('8'),
        perception_pipeline_binary_sha256: sha256_hex(&executable_bytes),
        classifier_assets_manifest_sha256: sha256_hex(&assets_json),
        card_database_profile_sha256: sha256_hex(card_database_bytes),
    };
    let header_json = serde_json::to_vec(&header).unwrap();
    let output = invoke_duel_classifier_v1(
        executable,
        &assets_path,
        &card_database_path,
        &header_json,
        &pixels,
    );
    assert!(
        output.status.success(),
        "duel classifier stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let response: MtgoDuelPerceptionProcessResponseV1 =
        serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(response.schema_version, 1);
    assert_eq!(
        response.request_commitment_sha256,
        commitment_hex(b"mtgo-duel-perception-request-v1", &[&header_json, &pixels])
    );
    assert_eq!(response.decision.frame_id, header.frame_id);
    assert_eq!(response.decision.frames[0].sequence, header.frame_sequence);
    assert_eq!(
        response.reconstruction_audit.frame.manifest_sha256,
        header.source_manifest_sha256
    );
    assert_eq!(
        response.reconstruction_audit.frame.frame_sha256,
        header.canonical_bgra8_sha256
    );
    let checked_decision = validate_observed_decision_v1(response.decision.clone()).unwrap();
    assert_eq!(
        response.visible_controls.decision_commitment_sha256,
        checked_decision.decision_commitment_sha256()
    );
    let lifecycle = response.competitive_lifecycle.unwrap();
    assert_eq!(lifecycle.event_kind, MtgoCompetitiveEventKindV1::League);
    assert_eq!(
        lifecycle.phase,
        MtgoCompetitiveLifecyclePhaseV1::MatchInProgress
    );
    assert_eq!(lifecycle.frame_id, header.frame_id);
    assert_eq!(lifecycle.frame_sequence, header.frame_sequence);

    let mut changed_pixels = pixels.clone();
    changed_pixels[(10 * width + 10) as usize * 4] ^= 1;
    let mut changed_header = header;
    changed_header.canonical_bgra8_sha256 = sha256_hex(&changed_pixels);
    let changed_header_json = serde_json::to_vec(&changed_header).unwrap();
    let rejected = invoke_duel_classifier_v1(
        executable,
        &assets_path,
        &card_database_path,
        &changed_header_json,
        &changed_pixels,
    );
    assert!(!rejected.status.success());
    assert!(String::from_utf8_lossy(&rejected.stderr)
        .contains("expected exactly one reviewed duel perception profile match"));

    std::fs::remove_dir_all(temp_root).unwrap();
}

fn duel_fixture_v1(
    pixels: &[u8],
    size: &MtgoSizePxV1,
) -> (
    MtgoObservedDecisionV1,
    MtgoObservationReconstructionAuditV1,
    MtgoVisibleActionControlSetV1,
    MtgoVisibleCompetitiveLifecycleSnapshotV1,
) {
    let (observation, pass, land) = (1..=128)
        .find_map(|seed| {
            let session = RlEpisodeSessionV1::reset_with_limits(7, seed, 128, 16_384);
            let RlSessionResponseV1::Decision(current) = session.current_response() else {
                return None;
            };
            let pass = current
                .legal_actions
                .iter()
                .find(|action| matches!(action.semantic, ActionSemanticV1::Pass { .. }))?
                .semantic
                .clone();
            let land = current
                .legal_actions
                .iter()
                .find(|action| matches!(action.semantic, ActionSemanticV1::PlayLand { .. }))?
                .semantic
                .clone();
            Some(((*current.observation).clone(), pass, land))
        })
        .expect("a deterministic seed supplies Pass and PlayLand");
    let source_ref = match &land {
        ActionSemanticV1::PlayLand { source, .. } => source.clone(),
        _ => unreachable!(),
    };
    let payload = MtgoSemanticDecisionPayloadV1 {
        observation,
        legal_actions: vec![pass.clone(), land.clone()],
        object_bindings: vec![MtgoObjectBindingV1 {
            adapter_object_id: "duel-process:hand:0".to_owned(),
            kernel_ref: source_ref,
        }],
    };
    let prompt_rect = MtgoRectPxV1 {
        x: 1,
        y: 1,
        width: 8,
        height: 8,
    };
    let pass_rect = MtgoRectPxV1 {
        x: 12,
        y: 1,
        width: 8,
        height: 8,
    };
    let land_rect = MtgoRectPxV1 {
        x: 23,
        y: 1,
        width: 8,
        height: 8,
    };
    let whole_rect = MtgoRectPxV1 {
        x: 0,
        y: 0,
        width: size.width,
        height: size.height,
    };
    let evidence = [
        (10, prompt_rect.clone()),
        (20, pass_rect.clone()),
        (30, land_rect.clone()),
        (40, whole_rect.clone()),
    ]
    .into_iter()
    .enumerate()
    .map(|(index, (evidence_id, rect))| MtgoVisibleEvidenceV1 {
        evidence_id,
        sequence: u64::try_from(index + 1).unwrap(),
        source: MtgoEvidenceSourceV1::FrameRegion {
            frame_id: 1,
            content_sha256: visible_frame_region_content_sha256_v1(pixels, size, &rect).unwrap(),
            rect,
        },
    })
    .collect::<Vec<_>>();
    let provenance = payload_leaf_inventory_v1(&payload)
        .unwrap()
        .into_iter()
        .filter(|leaf| leaf.requires_visible_evidence)
        .map(|leaf| MtgoLeafProvenanceV1 {
            json_pointer: leaf.json_pointer,
            value_sha256: leaf.value_sha256,
            evidence_ids: vec![10, 20, 30, 40],
            confidence_bps: 10_000,
        })
        .collect();
    let mut decision = MtgoObservedDecisionV1 {
        schema_version: 1,
        decision_id: "duel-process-template-v1".to_owned(),
        frame_id: 1,
        local_metadata_sha256: local_metadata_commitment_v1(&payload).unwrap(),
        payload,
        frames: vec![MtgoMockFrameV1 {
            frame_id: 1,
            sequence: 1,
            sha256: sha256_hex(pixels),
            client_bounds: whole_rect.clone(),
        }],
        evidence,
        provenance,
        readiness: MtgoDecisionReadinessV1 {
            observation_complete: true,
            legal_action_set_complete: true,
            client_prompt_reconciled: true,
        },
    };
    let checked = validate_observed_decision_v1(decision.clone()).unwrap();
    let controls = MtgoVisibleActionControlSetV1 {
        schema_version: 1,
        decision_commitment_sha256: checked.decision_commitment_sha256().to_owned(),
        frame_id: 1,
        frame_sequence: 1,
        prompt_frame_region_evidence_id: 10,
        prompt_reconciled: true,
        candidate_set_complete: true,
        controls: vec![
            MtgoVisibleActionControlCandidateV1 {
                control_id: "priority-pass".to_owned(),
                control_kind: MtgoVisibleControlKindV1::PromptButton,
                frame_region_evidence_id: 20,
                semantic: pass,
                confidence_bps: 10_000,
                visibly_enabled: true,
            },
            MtgoVisibleActionControlCandidateV1 {
                control_id: "play-land".to_owned(),
                control_kind: MtgoVisibleControlKindV1::Card,
                frame_region_evidence_id: 30,
                semantic: land,
                confidence_bps: 10_000,
                visibly_enabled: true,
            },
        ],
    };
    decision.decision_id = "duel-process-template-v1".to_owned();
    let region = MtgoVisibleRegionCommitmentV1 {
        rect_client_px: whole_rect.clone(),
        bgra8_sha256: visible_frame_region_content_sha256_v1(pixels, size, &whole_rect).unwrap(),
    };
    let groups = [
        MtgoObservationReconstructionGroupV1::DuelParticipants,
        MtgoObservationReconstructionGroupV1::TurnPhaseAndPriority,
        MtgoObservationReconstructionGroupV1::PlayerPublicState,
        MtgoObservationReconstructionGroupV1::PublicObjectsAndZones,
        MtgoObservationReconstructionGroupV1::ActingPlayerPrivateKnowledge,
        MtgoObservationReconstructionGroupV1::StackCombatAndPendingChoices,
        MtgoObservationReconstructionGroupV1::KernelDecisionHistoryContext,
        MtgoObservationReconstructionGroupV1::ObjectIncarnationsAndCardDb,
        MtgoObservationReconstructionGroupV1::CompleteOrderedLegalActions,
        MtgoObservationReconstructionGroupV1::KernelContractMetadata,
    ]
    .into_iter()
    .map(|group| {
        let local = matches!(
            group,
            MtgoObservationReconstructionGroupV1::KernelDecisionHistoryContext
                | MtgoObservationReconstructionGroupV1::ObjectIncarnationsAndCardDb
                | MtgoObservationReconstructionGroupV1::KernelContractMetadata
        );
        MtgoObservationReconstructionGroupAuditV1 {
            group,
            status: if local {
                MtgoReconstructionStatusV1::LocalDerivedComplete
            } else {
                MtgoReconstructionStatusV1::VisibleComplete
            },
            visible_regions: if local {
                Vec::new()
            } else {
                vec![region.clone()]
            },
            missing_reason_codes: Vec::new(),
        }
    })
    .collect();
    let audit = MtgoObservationReconstructionAuditV1 {
        schema_version: 1,
        audit_id: "duel-process-audit-template-v1".to_owned(),
        topology: MtgoReconstructionTopologyV1::TwoPlayerDuel,
        frame: MtgoCalibrationFrameReferenceV1 {
            sequence: 1,
            manifest_sha256: digest('5'),
            frame_sha256: sha256_hex(pixels),
            client_size_px: size.clone(),
            artifact_kind:
                MtgoCalibrationPreviewKindV1::ActingPlayerDuelGameplayCalibrationPreviewV1,
            capture_role: MtgoCalibrationCaptureRoleV1::ActingPlayerDuel,
            status: MtgoCalibrationPreviewStatusV1::PendingVisualReview,
            safe_for_semantic_evidence: false,
            safe_for_ocr: false,
            safe_for_policy_scoring: false,
            safe_for_input: false,
        },
        groups,
        observation_complete: true,
        legal_action_set_complete: true,
        ready_for_model_scoring: false,
    };
    let lifecycle_rects = [whole_rect, prompt_rect, pass_rect];
    let lifecycle_kinds = [
        MtgoLifecycleVisibleFactKindV1::MatchSurfaceVisible,
        MtgoLifecycleVisibleFactKindV1::LocalClockVisible,
        MtgoLifecycleVisibleFactKindV1::OpponentClockVisible,
    ];
    let lifecycle = MtgoVisibleCompetitiveLifecycleSnapshotV1 {
        schema_version: 1,
        snapshot_id: "duel-process-lifecycle-template-v1".to_owned(),
        event_kind: MtgoCompetitiveEventKindV1::League,
        phase: MtgoCompetitiveLifecyclePhaseV1::MatchInProgress,
        frame_id: 1,
        frame_sequence: 1,
        frame_sha256: sha256_hex(pixels),
        client_bounds: MtgoRectPxV1 {
            x: 0,
            y: 0,
            width: size.width,
            height: size.height,
        },
        event_identity_sha256: Some(digest('a')),
        match_identity_sha256: Some(digest('b')),
        game_number: Some(1),
        entry_terms: None,
        visible_state_complete: true,
        facts: lifecycle_kinds
            .into_iter()
            .zip(lifecycle_rects)
            .map(|(kind, rect)| MtgoLifecycleVisibleFactV1 {
                kind,
                content_sha256: visible_frame_region_content_sha256_v1(pixels, size, &rect)
                    .unwrap(),
                rect_client_px: rect,
                confidence_bps: 10_000,
            })
            .collect(),
    };
    validate_visible_competitive_lifecycle_snapshot_v1(lifecycle.clone()).unwrap();
    (decision, audit, controls, lifecycle)
}

fn invoke_duel_classifier_v1(
    executable: &str,
    assets_path: &std::path::Path,
    card_database_path: &std::path::Path,
    header_json: &[u8],
    pixels: &[u8],
) -> std::process::Output {
    let mut child = Command::new(executable)
        .arg("--mtgo-visible-duel-perception-v1")
        .arg("--classifier-assets-manifest")
        .arg(assets_path)
        .arg("--card-database-profile")
        .arg(card_database_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    {
        let stdin = child.stdin.as_mut().unwrap();
        stdin.write_all(DUEL_PERCEPTION_MAGIC_V1).unwrap();
        stdin
            .write_all(&(header_json.len() as u64).to_be_bytes())
            .unwrap();
        stdin.write_all(header_json).unwrap();
        stdin.write_all(pixels).unwrap();
    }
    drop(child.stdin.take());
    child.wait_with_output().unwrap()
}

fn invoke_classifier_v1(
    executable: &str,
    mode: &str,
    magic: &[u8],
    header_json: &[u8],
    assets_json: &[u8],
    pixels: &[u8],
) -> std::process::Output {
    let mut child = Command::new(executable)
        .arg(mode)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    {
        let stdin = child.stdin.as_mut().unwrap();
        stdin.write_all(magic).unwrap();
        stdin
            .write_all(&(header_json.len() as u64).to_be_bytes())
            .unwrap();
        stdin
            .write_all(&(assets_json.len() as u64).to_be_bytes())
            .unwrap();
        stdin.write_all(header_json).unwrap();
        stdin.write_all(assets_json).unwrap();
        stdin.write_all(pixels).unwrap();
    }
    drop(child.stdin.take());
    child.wait_with_output().unwrap()
}

fn digest(byte: char) -> String {
    byte.to_string().repeat(64)
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn commitment_hex(domain: &[u8], parts: &[&[u8]]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(domain);
    for part in parts {
        hasher.update((part.len() as u64).to_be_bytes());
        hasher.update(part);
    }
    format!("{:x}", hasher.finalize())
}
