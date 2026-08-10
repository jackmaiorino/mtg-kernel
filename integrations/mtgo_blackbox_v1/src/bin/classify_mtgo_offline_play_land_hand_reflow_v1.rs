use mtgo_blackbox_v1::{
    classify_untrusted_offline_play_land_hand_reflow_v1,
    validate_visible_object_action_calibration_trace_v1, MtgoVisibleObjectActionCalibrationTraceV1,
};
use serde_json::json;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

struct EndpointBytesV1 {
    manifest: Vec<u8>,
    frame_png: Vec<u8>,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("MTGO_OFFLINE_PLAY_LAND_HAND_REFLOW_V1_REJECTED:{error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut arguments = env::args_os();
    let _program = arguments.next();
    let calibration_path = PathBuf::from(arguments.next().ok_or(
        "usage: classify_mtgo_offline_play_land_hand_reflow_v1 <calibration-json> <before-directory> <after-directory>",
    )?);
    let before_directory = PathBuf::from(arguments.next().ok_or(
        "usage: classify_mtgo_offline_play_land_hand_reflow_v1 <calibration-json> <before-directory> <after-directory>",
    )?);
    let after_directory = PathBuf::from(arguments.next().ok_or(
        "usage: classify_mtgo_offline_play_land_hand_reflow_v1 <calibration-json> <before-directory> <after-directory>",
    )?);
    if arguments.next().is_some() {
        return Err("exactly three paths are required".to_owned());
    }
    if !calibration_path.is_absolute() || !calibration_path.is_file() {
        return Err("calibration JSON must be an existing absolute file".to_owned());
    }

    let calibration_bytes =
        fs::read(&calibration_path).map_err(|error| format!("read calibration JSON: {error}"))?;
    let calibration_record: MtgoVisibleObjectActionCalibrationTraceV1 =
        serde_json::from_slice(&calibration_bytes)
            .map_err(|error| format!("parse calibration JSON: {error}"))?;
    let calibration = validate_visible_object_action_calibration_trace_v1(calibration_record)
        .map_err(|error| format!("validate calibration: {error}"))?;
    let before = load_endpoint_v1(&before_directory)?;
    let after = load_endpoint_v1(&after_directory)?;
    let candidate = classify_untrusted_offline_play_land_hand_reflow_v1(
        &calibration,
        &before.manifest,
        &before.frame_png,
        &after.manifest,
        &after.frame_png,
    )
    .map_err(|error| error.to_string())?;

    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "status": "checked_untrusted_offline_measurement_only",
            "classifier_version": 1,
            "classification": format!("{:?}", candidate.classification()),
            "calibration_commitment_sha256": candidate.calibration_commitment_sha256(),
            "before_manifest_sha256": candidate.before_manifest_sha256(),
            "before_frame_sha256": candidate.before_frame_sha256(),
            "after_manifest_sha256": candidate.after_manifest_sha256(),
            "after_frame_sha256": candidate.after_frame_sha256(),
            "before_hand_count": candidate.before_hand_count(),
            "after_hand_count": candidate.after_hand_count(),
            "expected_source_ordinal": candidate.expected_source_ordinal(),
            "unique_visual_removed_ordinal": candidate.unique_visual_removed_ordinal(),
            "matched_pair_mean_absolute_difference_milli": candidate.matched_pair_mean_absolute_difference_milli(),
            "maximum_match_mean_absolute_difference_milli": candidate.maximum_match_mean_absolute_difference_milli(),
            "passing_deletion_candidate_count": candidate.passing_deletion_candidate_count(),
            "runner_up_deletion_maximum_difference_milli": candidate.runner_up_deletion_maximum_difference_milli(),
            "deletion_hypothesis_margin_milli": candidate.deletion_hypothesis_margin_milli(),
            "minimum_deletion_hypothesis_margin_milli": candidate.minimum_deletion_hypothesis_margin_milli(),
            "candidate_commitment_sha256": candidate.candidate_commitment_sha256(),
            "safe_for_semantic_evidence": candidate.safe_for_semantic_evidence(),
            "safe_for_observation_v5": candidate.safe_for_observation_v5(),
            "safe_for_policy_scoring": candidate.safe_for_policy_scoring(),
            "safe_for_input": candidate.safe_for_input()
        }))
        .map_err(|error| format!("serialize result: {error}"))?
    );
    Ok(())
}

fn load_endpoint_v1(directory: &Path) -> Result<EndpointBytesV1, String> {
    if !directory.is_absolute() || !directory.is_dir() {
        return Err("artifact directory must be an existing absolute directory".to_owned());
    }
    let manifest = fs::read(directory.join("manifest.json"))
        .map_err(|error| format!("read manifest.json: {error}"))?;
    let frame_png = fs::read(directory.join("frame.png"))
        .map_err(|error| format!("read frame.png: {error}"))?;
    Ok(EndpointBytesV1 {
        manifest,
        frame_png,
    })
}
