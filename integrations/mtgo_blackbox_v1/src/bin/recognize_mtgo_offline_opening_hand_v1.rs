use mtgo_blackbox_v1::{
    admit_ratified_dxgi_offline_calibration_artifact_v1, recognize_ratified_offline_opening_hand_v1,
};
use serde_json::json;
use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    if let Err(error) = run() {
        eprintln!("MTGO_OFFLINE_OPENING_HAND_REJECTED:{error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut arguments = env::args_os();
    let _program = arguments.next();
    let directory = PathBuf::from(
        arguments
            .next()
            .ok_or("usage: recognize_mtgo_offline_opening_hand_v1 <artifact-directory>")?,
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
    let admitted =
        admit_ratified_dxgi_offline_calibration_artifact_v1(&manifest, canonical.into(), &preview)
            .map_err(|error| error.to_string())?;
    let recognized =
        recognize_ratified_offline_opening_hand_v1(&admitted).map_err(|error| error.to_string())?;
    let actions: Vec<_> = recognized
        .actions()
        .iter()
        .map(|action| format!("{action:?}"))
        .collect();

    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "status": "recognized_exact_offline_calibration_only",
            "kind": format!("{:?}", recognized.kind()),
            "profile_id": recognized.profile_id(),
            "profile_commitment_sha256": recognized.profile_commitment_sha256(),
            "source_manifest_sha256": recognized.source_manifest_sha256(),
            "recognition_commitment_sha256": recognized.recognition_commitment_sha256(),
            "hand_size": recognized.hand_size(),
            "ordered_actions": actions,
            "safe_for_live_frame": recognized.safe_for_live_frame(),
            "safe_for_semantic_evidence": recognized.safe_for_semantic_evidence(),
            "safe_for_observation_v5": recognized.safe_for_observation_v5(),
            "safe_for_policy_scoring": recognized.safe_for_policy_scoring(),
            "safe_for_input": recognized.safe_for_input()
        }))
        .map_err(|error| format!("serialize result: {error}"))?
    );
    Ok(())
}
