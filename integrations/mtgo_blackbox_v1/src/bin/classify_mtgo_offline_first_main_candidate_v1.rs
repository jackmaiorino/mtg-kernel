use mtgo_blackbox_v1::{
    check_untrusted_dxgi_capture_artifact_v1, classify_untrusted_offline_first_main_candidate_v1,
};
use serde_json::json;
use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    if let Err(error) = run() {
        eprintln!("MTGO_OFFLINE_FIRST_MAIN_CANDIDATE_REJECTED:{error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut arguments = env::args_os();
    let _program = arguments.next();
    let directory = PathBuf::from(
        arguments
            .next()
            .ok_or("usage: classify_mtgo_offline_first_main_candidate_v1 <artifact-directory>")?,
    );
    if arguments.next().is_some() {
        return Err("exactly one artifact directory is required".to_owned());
    }
    if !directory.is_absolute() || !directory.is_dir() {
        return Err("artifact directory must be an existing absolute directory".to_owned());
    }

    let manifest = fs::read(directory.join("manifest.json"))
        .map_err(|error| format!("read manifest.json: {error}"))?;
    let canonical = fs::read(directory.join("frame.bgra"))
        .map_err(|error| format!("read frame.bgra: {error}"))?;
    let preview = fs::read(directory.join("frame.png"))
        .map_err(|error| format!("read frame.png: {error}"))?;
    let checked = check_untrusted_dxgi_capture_artifact_v1(&manifest, &canonical, &preview)
        .map_err(|error| error.to_string())?;
    let candidate = classify_untrusted_offline_first_main_candidate_v1(&checked, &canonical)
        .map_err(|error| error.to_string())?;

    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "status": "checked_untrusted_offline_measurement_only",
            "classification": format!("{:?}", candidate.classification()),
            "profile_commitment_sha256": candidate.profile_commitment_sha256(),
            "source_manifest_sha256": candidate.source_manifest_sha256(),
            "source_frame_sha256": candidate.source_frame_sha256(),
            "matched_region_count": candidate.matched_region_count(),
            "region_count": candidate.region_count(),
            "candidate_commitment_sha256": candidate.candidate_commitment_sha256(),
            "safe_for_live_frame": candidate.safe_for_live_frame(),
            "safe_for_semantic_evidence": candidate.safe_for_semantic_evidence(),
            "safe_for_observation_v5": candidate.safe_for_observation_v5(),
            "safe_for_policy_scoring": candidate.safe_for_policy_scoring(),
            "safe_for_input": candidate.safe_for_input()
        }))
        .map_err(|error| format!("serialize result: {error}"))?
    );
    Ok(())
}
