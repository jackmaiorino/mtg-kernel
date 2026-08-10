use mtgo_blackbox_v1::{
    check_untrusted_dxgi_capture_artifact_v1, validate_untrusted_offline_london_bottoming_trace_v1,
    CheckedUntrustedMtgoDxgiCaptureArtifactV1, MtgoOfflineLondonBottomingTraceV1,
};
use serde_json::json;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    if let Err(error) = run() {
        eprintln!("MTGO_OFFLINE_LONDON_BOTTOMING_TRACE_REJECTED:{error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut arguments = env::args_os();
    let _program = arguments.next();
    let trace_path = PathBuf::from(arguments.next().ok_or(
        "usage: check_mtgo_offline_london_bottoming_trace_v1 <trace.json> <state0> ... <stateN> <completion>",
    )?);
    let artifact_directories: Vec<_> = arguments.map(PathBuf::from).collect();
    if !trace_path.is_absolute() || !trace_path.is_file() {
        return Err("the trace file must be existing and absolute".to_owned());
    }

    let trace_bytes = fs::read(&trace_path).map_err(|error| format!("read trace: {error}"))?;
    let record: MtgoOfflineLondonBottomingTraceV1 =
        serde_json::from_slice(&trace_bytes).map_err(|error| format!("parse trace: {error}"))?;
    let state_count = record.states.len();
    if artifact_directories.len() != state_count + 1 {
        return Err(format!(
            "the trace declares {state_count} states, so exactly {state_count} state directories and one completion directory are required"
        ));
    }
    let checked: Vec<_> = artifact_directories
        .iter()
        .map(|directory| read_checked_artifact_v1(directory))
        .collect::<Result<_, _>>()?;
    let state_references: Vec<&CheckedUntrustedMtgoDxgiCaptureArtifactV1> =
        checked[..state_count].iter().collect();
    let completion = &checked[state_count];
    let validated =
        validate_untrusted_offline_london_bottoming_trace_v1(record, &state_references, completion)
            .map_err(|error| error.to_string())?;
    let action_counts: Vec<_> = (0..validated.state_count())
        .map(|index| {
            validated
                .legal_actions_for_state(index)
                .map(|actions| actions.len())
                .unwrap_or(0)
        })
        .collect();

    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "status": "checked_untrusted_offline_calibration_only",
            "trace_commitment_sha256": validated.trace_commitment_sha256(),
            "required_bottom_count": validated.required_bottom_count(),
            "began_game_hand_size": validated.began_game_hand_size(),
            "state_count": validated.state_count(),
            "legal_action_counts_by_state": action_counts,
            "selected_for_bottom_click_order": validated.selected_for_bottom_click_order(),
            "safe_for_live_frame": validated.safe_for_live_frame(),
            "safe_for_semantic_evidence": validated.safe_for_semantic_evidence(),
            "safe_for_observation_v5": validated.safe_for_observation_v5(),
            "safe_for_policy_scoring": validated.safe_for_policy_scoring(),
            "safe_for_input": validated.safe_for_input()
        }))
        .map_err(|error| format!("serialize result: {error}"))?
    );
    Ok(())
}

fn read_checked_artifact_v1(
    directory: &Path,
) -> Result<CheckedUntrustedMtgoDxgiCaptureArtifactV1, String> {
    if !directory.is_absolute() || !directory.is_dir() {
        return Err(format!(
            "artifact directory must be existing and absolute: {}",
            directory.display()
        ));
    }
    let manifest = fs::read(directory.join("manifest.json"))
        .map_err(|error| format!("read manifest.json from {}: {error}", directory.display()))?;
    let canonical = fs::read(directory.join("frame.bgra"))
        .map_err(|error| format!("read frame.bgra from {}: {error}", directory.display()))?;
    let preview = fs::read(directory.join("frame.png"))
        .map_err(|error| format!("read frame.png from {}: {error}", directory.display()))?;
    check_untrusted_dxgi_capture_artifact_v1(&manifest, &canonical, &preview)
        .map_err(|error| format!("check {}: {error}", directory.display()))
}
