use mtgo_blackbox_v1::{
    build_checked_untrusted_acting_player_duel_calibration_corpus_v1,
    check_untrusted_dxgi_capture_artifact_v1, CheckedUntrustedMtgoDxgiCaptureArtifactV1,
};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    if let Err(error) = run() {
        eprintln!("MTGO_DUEL_CALIBRATION_CORPUS_REJECTED:{error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut arguments = env::args_os();
    let _program = arguments.next();
    let corpus_id = arguments
        .next()
        .ok_or(
            "usage: build_mtgo_acting_player_duel_calibration_corpus_v1 <corpus-id> <artifact-directory> [artifact-directory ...]",
        )?
        .into_string()
        .map_err(|_| "corpus ID must be UTF-8")?;
    let artifact_directories: Vec<PathBuf> = arguments.map(PathBuf::from).collect();
    if artifact_directories.is_empty() {
        return Err("at least one artifact directory is required".to_owned());
    }

    let checked_artifacts: Vec<CheckedUntrustedMtgoDxgiCaptureArtifactV1> = artifact_directories
        .iter()
        .map(|directory| load_checked_artifact(directory))
        .collect::<Result<_, _>>()?;
    let checked_refs: Vec<&CheckedUntrustedMtgoDxgiCaptureArtifactV1> =
        checked_artifacts.iter().collect();
    let corpus =
        build_checked_untrusted_acting_player_duel_calibration_corpus_v1(&corpus_id, &checked_refs)
            .map_err(|error| error.to_string())?;
    let manifest_bytes = serde_json::to_vec_pretty(corpus.manifest_v1())
        .map_err(|error| format!("serialize corpus manifest: {error}"))?;
    let manifest_sha256 = format!("{:x}", Sha256::digest(&manifest_bytes));

    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "status": "checked_untrusted_not_admitted",
            "corpus_manifest": corpus.manifest_v1(),
            "corpus_manifest_sha256": manifest_sha256,
            "corpus_commitment_sha256": corpus.corpus_commitment_sha256(),
            "sample_count": corpus.sample_count(),
            "safe_for_semantic_evidence": corpus.safe_for_semantic_evidence(),
            "safe_for_ocr": corpus.safe_for_ocr(),
            "safe_for_policy_scoring": corpus.safe_for_policy_scoring(),
            "safe_for_input": corpus.safe_for_input()
        }))
        .map_err(|error| format!("serialize result: {error}"))?
    );
    Ok(())
}

fn load_checked_artifact(
    artifact_directory: &Path,
) -> Result<CheckedUntrustedMtgoDxgiCaptureArtifactV1, String> {
    if !artifact_directory.is_absolute() || !artifact_directory.is_dir() {
        return Err("each artifact directory must be an existing absolute directory".to_owned());
    }
    let manifest = fs::read(artifact_directory.join("manifest.json"))
        .map_err(|error| format!("read manifest.json from checked source: {error}"))?;
    let canonical = fs::read(artifact_directory.join("frame.bgra"))
        .map_err(|error| format!("read frame.bgra from checked source: {error}"))?;
    let preview = fs::read(artifact_directory.join("frame.png"))
        .map_err(|error| format!("read frame.png from checked source: {error}"))?;
    check_untrusted_dxgi_capture_artifact_v1(&manifest, &canonical, &preview)
        .map_err(|error| error.to_string())
}
