use mtgo_blackbox_v1::{
    check_untrusted_dxgi_capture_artifact_v1, check_untrusted_dxgi_keep_transition_v1,
    MtgoDxgiKeepTransitionV1,
};
use serde_json::json;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

struct ArtifactFilesV1 {
    manifest: Vec<u8>,
    canonical: Vec<u8>,
    preview: Vec<u8>,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("MTGO_DXGI_KEEP_TRANSITION_REJECTED:{error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut arguments = env::args_os();
    let _program = arguments.next();
    let record_path = PathBuf::from(arguments.next().ok_or(
        "usage: check_mtgo_dxgi_keep_transition_v1 <record.json> <before-dir> <after-dir>",
    )?);
    let before_directory = PathBuf::from(arguments.next().ok_or(
        "usage: check_mtgo_dxgi_keep_transition_v1 <record.json> <before-dir> <after-dir>",
    )?);
    let after_directory = PathBuf::from(arguments.next().ok_or(
        "usage: check_mtgo_dxgi_keep_transition_v1 <record.json> <before-dir> <after-dir>",
    )?);
    if arguments.next().is_some() {
        return Err("exactly one record and two artifact directories are required".to_owned());
    }
    if !record_path.is_absolute() || !record_path.is_file() {
        return Err("transition record must be an existing absolute file".to_owned());
    }

    let record_bytes =
        fs::read(&record_path).map_err(|error| format!("read transition record: {error}"))?;
    let record: MtgoDxgiKeepTransitionV1 = serde_json::from_slice(&record_bytes)
        .map_err(|error| format!("parse transition record: {error}"))?;
    let before = read_artifact_v1(&before_directory)?;
    let after = read_artifact_v1(&after_directory)?;
    let before_checked = check_untrusted_dxgi_capture_artifact_v1(
        &before.manifest,
        &before.canonical,
        &before.preview,
    )
    .map_err(|error| format!("check before artifact: {error}"))?;
    let after_checked =
        check_untrusted_dxgi_capture_artifact_v1(&after.manifest, &after.canonical, &after.preview)
            .map_err(|error| format!("check after artifact: {error}"))?;
    let checked = check_untrusted_dxgi_keep_transition_v1(
        record,
        &before_checked,
        &before.canonical,
        &after_checked,
        &after.canonical,
    )
    .map_err(|error| error.to_string())?;

    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "status": "checked_untrusted_calibration_only",
            "action": format!("{:?}", checked.action()),
            "before_manifest_sha256": checked.before_manifest_sha256(),
            "after_manifest_sha256": checked.after_manifest_sha256(),
            "changed_region_count": checked.changed_region_count(),
            "transition_commitment_sha256": checked.transition_commitment_sha256(),
            "safe_for_semantic_evidence": checked.safe_for_semantic_evidence(),
            "safe_for_policy_scoring": checked.safe_for_policy_scoring(),
            "safe_for_input": checked.safe_for_input()
        }))
        .map_err(|error| format!("serialize result: {error}"))?
    );
    Ok(())
}

fn read_artifact_v1(directory: &Path) -> Result<ArtifactFilesV1, String> {
    if !directory.is_absolute() || !directory.is_dir() {
        return Err("artifact directory must be an existing absolute directory".to_owned());
    }
    Ok(ArtifactFilesV1 {
        manifest: fs::read(directory.join("manifest.json"))
            .map_err(|error| format!("read manifest.json: {error}"))?,
        canonical: fs::read(directory.join("frame.bgra"))
            .map_err(|error| format!("read frame.bgra: {error}"))?,
        preview: fs::read(directory.join("frame.png"))
            .map_err(|error| format!("read frame.png: {error}"))?,
    })
}
