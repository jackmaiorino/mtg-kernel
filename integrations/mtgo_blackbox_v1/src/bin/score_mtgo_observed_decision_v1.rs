use mtgo_blackbox_v1::{
    load_mtgo_native_checkpoint_deployment_v1, make_scored_offline_intent_v1,
    validate_observed_decision_v1, MtgoExpectedModelDeploymentV1, MtgoObservedDecisionV1,
};
use serde::{de::DeserializeOwned, Deserialize};
use serde_json::json;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

const MAX_DEPLOYMENT_JSON_BYTES_V1: u64 = 65_536;
const MAX_OBSERVED_DECISION_JSON_BYTES_V1: u64 = 64 * 1_048_576;
const OFFLINE_MOCK_DECISION_INPUT_SCHEMA_V1: &str = "mtgo-offline-mock-observed-decision-input/v1";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MtgoOfflineMockDecisionInputV1 {
    schema: String,
    source_kind: MtgoOfflineDecisionSourceKindV1,
    decision: serde_json::Value,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum MtgoOfflineDecisionSourceKindV1 {
    SyntheticMockV1,
}

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
    let input: MtgoOfflineMockDecisionInputV1 = read_strict_json_v1(
        &decision_path,
        MAX_OBSERVED_DECISION_JSON_BYTES_V1,
        "offline mock decision envelope",
    )?;
    let record = unpack_offline_mock_decision_v1(input)?;
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
        "declared_input_scope": "offline_synthetic_mock_v1",
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

fn unpack_offline_mock_decision_v1(
    input: MtgoOfflineMockDecisionInputV1,
) -> Result<MtgoObservedDecisionV1, String> {
    if input.schema != OFFLINE_MOCK_DECISION_INPUT_SCHEMA_V1 {
        return Err("offline mock decision envelope schema mismatch".to_owned());
    }
    match input.source_kind {
        MtgoOfflineDecisionSourceKindV1::SyntheticMockV1 => {}
    }
    serde_json::from_value(input.decision)
        .map_err(|_| "offline mock decision is not strict contract JSON".to_owned())
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
    "usage: score_mtgo_observed_decision_v1 <native-store-root> <expected-deployment.json> <offline-mock-decision-envelope.json>".to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_decision_and_unknown_source_cannot_enter_offline_scorer() {
        let raw_decision = json!({
            "schema_version": 1,
            "decision_id": "not-an-envelope"
        });
        assert!(serde_json::from_value::<MtgoOfflineMockDecisionInputV1>(raw_decision).is_err());

        let unknown_source = json!({
            "schema": OFFLINE_MOCK_DECISION_INPUT_SCHEMA_V1,
            "source_kind": "checked_dxgi_acting_player_duel_v1",
            "decision": {}
        });
        assert!(serde_json::from_value::<MtgoOfflineMockDecisionInputV1>(unknown_source).is_err());
    }

    #[test]
    fn schema_and_inner_decision_are_both_strict() {
        let wrong_schema: MtgoOfflineMockDecisionInputV1 = serde_json::from_value(json!({
            "schema": "mtgo-offline-mock-observed-decision-input/v2",
            "source_kind": "synthetic_mock_v1",
            "decision": {}
        }))
        .unwrap();
        assert_eq!(
            unpack_offline_mock_decision_v1(wrong_schema).unwrap_err(),
            "offline mock decision envelope schema mismatch"
        );

        let malformed_inner: MtgoOfflineMockDecisionInputV1 = serde_json::from_value(json!({
            "schema": OFFLINE_MOCK_DECISION_INPUT_SCHEMA_V1,
            "source_kind": "synthetic_mock_v1",
            "decision": { "schema_version": 1, "unexpected": true }
        }))
        .unwrap();
        assert_eq!(
            unpack_offline_mock_decision_v1(malformed_inner).unwrap_err(),
            "offline mock decision is not strict contract JSON"
        );
    }
}
