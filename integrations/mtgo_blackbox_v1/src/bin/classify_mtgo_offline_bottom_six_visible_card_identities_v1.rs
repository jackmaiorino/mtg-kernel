use mtgo_blackbox_v1::{
    check_untrusted_dxgi_capture_artifact_v1,
    check_untrusted_offline_visible_card_template_profile_v1,
    classify_untrusted_offline_bottom_six_visible_card_identities_v1,
    MtgoOfflineVisibleCardTemplateProfileV1,
};
use serde_json::json;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

const MAX_PROFILE_BYTES_V1: u64 = 8 * 1_048_576;

struct ArtifactBytesV1 {
    manifest: Vec<u8>,
    canonical: Vec<u8>,
    preview: Vec<u8>,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("MTGO_OFFLINE_VISIBLE_CARD_IDENTITIES_REJECTED:{error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut arguments = env::args_os();
    let _program = arguments.next();
    let artifact_directory = PathBuf::from(arguments.next().ok_or(
        "usage: classify_mtgo_offline_bottom_six_visible_card_identities_v1 <artifact-directory> <profile-json>",
    )?);
    let profile_path = PathBuf::from(arguments.next().ok_or(
        "usage: classify_mtgo_offline_bottom_six_visible_card_identities_v1 <artifact-directory> <profile-json>",
    )?);
    if arguments.next().is_some() {
        return Err("exactly one artifact directory and one profile JSON are required".to_owned());
    }

    let artifact = load_artifact_v1(&artifact_directory)?;
    let profile_bytes = read_bounded_absolute_file_v1(&profile_path, MAX_PROFILE_BYTES_V1)?;
    let profile: MtgoOfflineVisibleCardTemplateProfileV1 =
        serde_json::from_slice(&profile_bytes)
            .map_err(|error| format!("parse profile JSON: {error}"))?;
    let checked_profile = check_untrusted_offline_visible_card_template_profile_v1(profile)
        .map_err(|error| format!("check template profile: {error}"))?;
    let checked_artifact = check_untrusted_dxgi_capture_artifact_v1(
        &artifact.manifest,
        &artifact.canonical,
        &artifact.preview,
    )
    .map_err(|error| format!("check capture artifact: {error}"))?;
    let candidate = classify_untrusted_offline_bottom_six_visible_card_identities_v1(
        &checked_artifact,
        &artifact.canonical,
        &checked_profile,
    )
    .map_err(|error| error.to_string())?;
    let identities: Vec<_> = candidate
        .identities()
        .iter()
        .map(|identity| {
            json!({
                "ordinal": identity.ordinal(),
                "visible_card_name": identity.visible_card_name(),
                "winning_template_id": identity.winning_template_id(),
                "mean_absolute_difference_milli": identity.mean_absolute_difference_milli(),
                "runner_up_distinct_name": identity.runner_up_distinct_name(),
                "runner_up_mean_absolute_difference_milli": identity.runner_up_mean_absolute_difference_milli(),
                "distinct_name_margin_milli": identity.distinct_name_margin_milli()
            })
        })
        .collect();

    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "status": "checked_untrusted_offline_measurement_only",
            "classification": format!("{:?}", candidate.classification()),
            "source_manifest_sha256": candidate.source_manifest_sha256(),
            "source_stage_commitment_sha256": candidate.source_stage_commitment_sha256(),
            "profile_commitment_sha256": candidate.profile_commitment_sha256(),
            "visible_hand_count": candidate.visible_hand_count(),
            "matched_identity_count": candidate.matched_identity_count(),
            "identities": identities,
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
    Ok(ArtifactBytesV1 {
        manifest: fs::read(directory.join("manifest.json"))
            .map_err(|error| format!("read manifest.json: {error}"))?,
        canonical: fs::read(directory.join("frame.bgra"))
            .map_err(|error| format!("read frame.bgra: {error}"))?,
        preview: fs::read(directory.join("frame.png"))
            .map_err(|error| format!("read frame.png: {error}"))?,
    })
}

fn read_bounded_absolute_file_v1(path: &Path, maximum_bytes: u64) -> Result<Vec<u8>, String> {
    if !path.is_absolute() || !path.is_file() {
        return Err("profile JSON must be an existing absolute file".to_owned());
    }
    let length = fs::metadata(path)
        .map_err(|error| format!("inspect profile JSON: {error}"))?
        .len();
    if length == 0 || length > maximum_bytes {
        return Err(format!(
            "profile JSON must contain 1 through {maximum_bytes} bytes"
        ));
    }
    fs::read(path).map_err(|error| format!("read profile JSON: {error}"))
}
