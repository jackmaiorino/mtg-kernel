use mtgo_blackbox_v1::{
    check_untrusted_dxgi_capture_artifact_v1,
    classify_untrusted_offline_bottom_six_reflow_candidate_v1,
};
use serde_json::json;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

struct ArtifactBytesV1 {
    manifest: Vec<u8>,
    canonical: Vec<u8>,
    preview: Vec<u8>,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("MTGO_OFFLINE_BOTTOM_SIX_REFLOW_CANDIDATE_REJECTED:{error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut arguments = env::args_os();
    let _program = arguments.next();
    let before_directory = PathBuf::from(arguments.next().ok_or(
        "usage: classify_mtgo_offline_bottom_six_reflow_candidate_v1 <before-directory> <after-directory>",
    )?);
    let after_directory = PathBuf::from(arguments.next().ok_or(
        "usage: classify_mtgo_offline_bottom_six_reflow_candidate_v1 <before-directory> <after-directory>",
    )?);
    if arguments.next().is_some() {
        return Err("exactly two artifact directories are required".to_owned());
    }
    let before = load_artifact_v1(&before_directory)?;
    let after = load_artifact_v1(&after_directory)?;
    let before_checked = check_untrusted_dxgi_capture_artifact_v1(
        &before.manifest,
        &before.canonical,
        &before.preview,
    )
    .map_err(|error| format!("check before artifact: {error}"))?;
    let after_checked =
        check_untrusted_dxgi_capture_artifact_v1(&after.manifest, &after.canonical, &after.preview)
            .map_err(|error| format!("check after artifact: {error}"))?;
    let candidate = classify_untrusted_offline_bottom_six_reflow_candidate_v1(
        &before_checked,
        &before.canonical,
        &after_checked,
        &after.canonical,
    )
    .map_err(|error| error.to_string())?;

    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "status": "checked_untrusted_offline_measurement_only",
            "classification": format!("{:?}", candidate.classification()),
            "before_manifest_sha256": candidate.before_manifest_sha256(),
            "after_manifest_sha256": candidate.after_manifest_sha256(),
            "before_stage_commitment_sha256": candidate.before_stage_commitment_sha256(),
            "after_stage_commitment_sha256": candidate.after_stage_commitment_sha256(),
            "before_selected_count": candidate.before_selected_count(),
            "after_selected_count": candidate.after_selected_count(),
            "removed_before_ordinal": candidate.removed_before_ordinal(),
            "matched_pair_mean_absolute_difference_milli": candidate.matched_pair_mean_absolute_difference_milli(),
            "maximum_match_mean_absolute_difference_milli": candidate.maximum_match_mean_absolute_difference_milli(),
            "passing_deletion_candidate_count": candidate.passing_deletion_candidate_count(),
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

fn load_artifact_v1(directory: &Path) -> Result<ArtifactBytesV1, String> {
    if !directory.is_absolute() || !directory.is_dir() {
        return Err("artifact directory must be an existing absolute directory".to_owned());
    }
    let manifest = fs::read(directory.join("manifest.json"))
        .map_err(|error| format!("read manifest.json: {error}"))?;
    let canonical = fs::read(directory.join("frame.bgra"))
        .map_err(|error| format!("read frame.bgra: {error}"))?;
    let preview = fs::read(directory.join("frame.png"))
        .map_err(|error| format!("read frame.png: {error}"))?;
    Ok(ArtifactBytesV1 {
        manifest,
        canonical,
        preview,
    })
}
