use mtgo_blackbox_v1::{
    load_mtgo_native_checkpoint_deployment_v1, make_scored_offline_intent_v1,
    validate_observed_decision_v1, MtgoExpectedModelDeploymentV1, MtgoObservedDecisionV1,
};
use serde::de::DeserializeOwned;
use serde_json::json;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

const MAX_DEPLOYMENT_JSON_BYTES_V1: u64 = 65_536;
const MAX_OBSERVED_DECISION_JSON_BYTES_V1: u64 = 64 * 1_048_576;

fn main() -> ExitCode {
    match run_v1() {
        Ok(summary) => match serde_json::to_string(&summary) {
            Ok(encoded) => {
                println!("{encoded}");
                ExitCode::SUCCESS
            }
            Err(_) => {
                eprintln!("offline scoring summary serialization failed");
                ExitCode::FAILURE
            }
        },
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
    let decision_path = args.next().map(PathBuf::from).ok_or_else(usage_v1)?;
    if args.next().is_some() {
        return Err(usage_v1());
    }

    let expected: MtgoExpectedModelDeploymentV1 = read_strict_json_v1(
        &deployment_path,
        MAX_DEPLOYMENT_JSON_BYTES_V1,
        "deployment manifest",
    )?;
    let record: MtgoObservedDecisionV1 = read_strict_json_v1(
        &decision_path,
        MAX_OBSERVED_DECISION_JSON_BYTES_V1,
        "observed decision",
    )?;
    let decision =
        validate_observed_decision_v1(record).map_err(|error| error.code().to_owned())?;
    let loaded = load_mtgo_native_checkpoint_deployment_v1(&store_root, expected)
        .map_err(|error| error.code().to_owned())?;
    let selection = loaded
        .score_validated_decision_v1(&decision)
        .map_err(|error| error.code().to_owned())?;
    let intent = make_scored_offline_intent_v1(&decision, &selection)
        .map_err(|error| error.code().to_owned())?;

    Ok(json!({
        "schema_version": 1,
        "status": "scored_offline",
        "deployment_id": loaded.deployment_id(),
        "deployment_commitment_sha256": loaded.deployment_commitment_sha256(),
        "decision_commitment_sha256": selection.decision_commitment_sha256(),
        "request_commitment_sha256": selection.request_commitment_sha256(),
        "selection_commitment_sha256": selection.selection_commitment_sha256(),
        "frame_id": intent.frame_id,
        "frame_sequence": intent.frame_sequence,
        "selected_index": intent.selected_index,
        "selected_semantic": intent.semantic,
        "selected_logit_f32_bits": selection.selected_logit_f32_bits(),
        "value_f32_bits": selection.value_f32_bits(),
        "safe_for_live_input": selection.safe_for_live_input(),
        "permits_match_entry": loaded.permits_match_entry()
    }))
}

fn read_strict_json_v1<T: DeserializeOwned>(
    path: &Path,
    maximum_bytes: u64,
    label: &'static str,
) -> Result<T, String> {
    let metadata = fs::metadata(path).map_err(|_| format!("{label} metadata could not be read"))?;
    if !metadata.is_file() || metadata.len() > maximum_bytes {
        return Err(format!("{label} is absent or exceeds the fixed byte cap"));
    }
    let bytes = fs::read(path).map_err(|_| format!("{label} could not be read"))?;
    serde_json::from_slice(&bytes).map_err(|_| format!("{label} is not strict contract JSON"))
}

fn usage_v1() -> String {
    "usage: score_mtgo_observed_decision_v1 <native-store-root> <expected-deployment.json> <observed-decision.json>".to_owned()
}
