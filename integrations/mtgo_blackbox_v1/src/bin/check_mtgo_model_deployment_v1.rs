use mtgo_blackbox_v1::{load_mtgo_native_checkpoint_deployment_v1, MtgoExpectedModelDeploymentV1};
use serde_json::json;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

const MAX_DEPLOYMENT_JSON_BYTES_V1: u64 = 65_536;

fn main() -> ExitCode {
    match run_v1() {
        Ok(summary) => {
            println!("{}", serde_json::to_string(&summary).unwrap());
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

fn run_v1() -> Result<serde_json::Value, String> {
    let mut args = env::args_os().skip(1);
    let store_root = args.next().map(PathBuf::from).ok_or_else(usage_v1)?;
    let deployment_path = args.next().map(PathBuf::from).ok_or_else(usage_v1)?;
    if args.next().is_some() {
        return Err(usage_v1());
    }

    let metadata = fs::metadata(&deployment_path)
        .map_err(|_| "deployment manifest metadata could not be read".to_owned())?;
    if !metadata.is_file() || metadata.len() > MAX_DEPLOYMENT_JSON_BYTES_V1 {
        return Err("deployment manifest is absent or exceeds the fixed byte cap".to_owned());
    }
    let bytes = fs::read(&deployment_path)
        .map_err(|_| "deployment manifest could not be read".to_owned())?;
    let expected: MtgoExpectedModelDeploymentV1 = serde_json::from_slice(&bytes)
        .map_err(|_| "deployment manifest is not strict expected-deployment JSON".to_owned())?;
    let loaded = load_mtgo_native_checkpoint_deployment_v1(&store_root, expected)
        .map_err(|error| error.code().to_owned())?;
    loaded
        .scorer_v1()
        .map_err(|error| error.code().to_owned())?;

    Ok(json!({
        "schema_version": 1,
        "status": "loaded",
        "deployment_id": loaded.deployment_id(),
        "generation_index": loaded.generation_index(),
        "deployment_commitment_sha256": loaded.deployment_commitment_sha256(),
        "safe_for_live_input": loaded.safe_for_live_input(),
        "permits_match_entry": loaded.permits_match_entry()
    }))
}

fn usage_v1() -> String {
    "usage: check_mtgo_model_deployment_v1 <native-store-root> <expected-deployment.json>"
        .to_owned()
}
