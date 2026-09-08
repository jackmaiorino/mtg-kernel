use mtgo_blackbox_v1::{
    check_untrusted_dxgi_capture_artifact_v1,
    classify_untrusted_offline_mulligan_ladder_candidate_v1,
    validate_untrusted_offline_london_bottoming_trace_v1,
    validate_untrusted_offline_london_pregame_episode_v1,
    CheckedUntrustedMtgoDxgiCaptureArtifactV1, MtgoOfflineLondonBottomingTraceV1,
    MtgoOfflineLondonPregameEpisodeV1,
};
use serde_json::json;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    if let Err(error) = run() {
        eprintln!("MTGO_OFFLINE_LONDON_PREGAME_EPISODE_REJECTED:{error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut arguments = env::args_os();
    let _program = arguments.next();
    let episode_path = PathBuf::from(arguments.next().ok_or(
        "usage: check_mtgo_offline_london_pregame_episode_v1 <episode.json> <bottoming.json> <choice7> ... <choice1> <bottom0> ... <bottom6> <completion>",
    )?);
    let bottoming_path = PathBuf::from(
        arguments
            .next()
            .ok_or("a bottoming trace path is required after the episode path")?,
    );
    let artifact_directories: Vec<_> = arguments.map(PathBuf::from).collect();
    if !episode_path.is_absolute()
        || !episode_path.is_file()
        || !bottoming_path.is_absolute()
        || !bottoming_path.is_file()
        || artifact_directories.len() != 15
    {
        return Err(
            "two absolute record files plus exactly fifteen artifact directories are required"
                .to_owned(),
        );
    }

    let episode: MtgoOfflineLondonPregameEpisodeV1 = read_json_v1(&episode_path, "episode")?;
    let bottoming_record: MtgoOfflineLondonBottomingTraceV1 =
        read_json_v1(&bottoming_path, "bottoming trace")?;
    let checked: Vec<_> = artifact_directories
        .iter()
        .map(|directory| read_checked_artifact_v1(directory))
        .collect::<Result<_, _>>()?;

    let choices: Vec<_> = checked[..7]
        .iter()
        .zip(&artifact_directories[..7])
        .map(|(artifact, directory)| {
            let pixels = fs::read(directory.join("frame.bgra")).map_err(|error| {
                format!("read choice pixels from {}: {error}", directory.display())
            })?;
            classify_untrusted_offline_mulligan_ladder_candidate_v1(artifact, &pixels)
                .map_err(|error| error.to_string())
        })
        .collect::<Result<_, _>>()?;
    let choice_references: Vec<_> = choices.iter().collect();

    let bottom_state_references: Vec<_> = checked[7..14].iter().collect();
    let bottoming = validate_untrusted_offline_london_bottoming_trace_v1(
        bottoming_record,
        &bottom_state_references,
        &checked[14],
    )
    .map_err(|error| error.to_string())?;
    let frame_references: Vec<_> = checked.iter().collect();
    let validated = validate_untrusted_offline_london_pregame_episode_v1(
        episode,
        &choice_references,
        &bottoming,
        &frame_references,
    )
    .map_err(|error| error.to_string())?;

    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "status": "checked_untrusted_offline_calibration_only",
            "episode_commitment_sha256": validated.episode_commitment_sha256(),
            "stage_count": validated.stage_count(),
            "declared_transition_count": validated.declared_transition_count(),
            "stage_action_counts": validated.stage_action_counts(),
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

fn read_json_v1<T: serde::de::DeserializeOwned>(path: &Path, label: &str) -> Result<T, String> {
    let bytes = fs::read(path).map_err(|error| format!("read {label}: {error}"))?;
    serde_json::from_slice(&bytes).map_err(|error| format!("parse {label}: {error}"))
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
