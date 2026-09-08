use mtgo_blackbox_v1::{
    build_checked_untrusted_acting_player_duel_calibration_corpus_v1,
    check_untrusted_dxgi_capture_artifact_v1,
    check_untrusted_player_visible_duel_annotation_set_v1,
    CheckedUntrustedMtgoDxgiCaptureArtifactV1, MtgoPlayerVisibleDuelAnnotationSetV1,
};
use serde_json::json;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

const MAX_ANNOTATION_FILE_BYTES_V1: u64 = 64 * 1_048_576;

fn main() {
    if let Err(error) = run_v1() {
        eprintln!("MTGO_PLAYER_VISIBLE_DUEL_ANNOTATIONS_REJECTED:{error}");
        std::process::exit(1);
    }
}

fn run_v1() -> Result<(), String> {
    let mut arguments = env::args_os();
    let _program = arguments.next();
    let corpus_id = arguments
        .next()
        .ok_or(
            "usage: check_mtgo_player_visible_duel_annotations_v1 <corpus-id> <annotation-set.json> <artifact-directory> [artifact-directory ...]",
        )?
        .into_string()
        .map_err(|_| "corpus ID must be UTF-8")?;
    let annotation_path = PathBuf::from(arguments.next().ok_or(
        "usage: check_mtgo_player_visible_duel_annotations_v1 <corpus-id> <annotation-set.json> <artifact-directory> [artifact-directory ...]",
    )?);
    let artifact_directories = arguments.map(PathBuf::from).collect::<Vec<_>>();
    if !annotation_path.is_absolute() || !annotation_path.is_file() {
        return Err("annotation set must be an existing absolute file".to_owned());
    }
    if artifact_directories.is_empty() {
        return Err("at least one artifact directory is required".to_owned());
    }

    let annotation_size = fs::metadata(&annotation_path)
        .map_err(|error| format!("inspect annotation set: {error}"))?
        .len();
    if annotation_size == 0 || annotation_size > MAX_ANNOTATION_FILE_BYTES_V1 {
        return Err("annotation set must be between 1 byte and 64 MiB".to_owned());
    }
    let annotation_bytes = fs::read(&annotation_path)
        .map_err(|error| format!("read player-visible annotation set: {error}"))?;
    let annotation_manifest: MtgoPlayerVisibleDuelAnnotationSetV1 =
        serde_json::from_slice(&annotation_bytes)
            .map_err(|error| format!("parse player-visible annotation set: {error}"))?;

    let artifacts = artifact_directories
        .iter()
        .map(|directory| load_checked_artifact_v1(directory))
        .collect::<Result<Vec<_>, _>>()?;
    let artifact_refs = artifacts.iter().collect::<Vec<_>>();
    let corpus = build_checked_untrusted_acting_player_duel_calibration_corpus_v1(
        &corpus_id,
        &artifact_refs,
    )
    .map_err(|error| error.to_string())?;
    let checked =
        check_untrusted_player_visible_duel_annotation_set_v1(&corpus, annotation_manifest)
            .map_err(|error| error.to_string())?;

    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "status": "checked_untrusted_offline_evaluation_only",
            "annotation_protocol_sha256": checked.annotation_protocol_sha256(),
            "corpus_manifest_sha256": checked.corpus_manifest_sha256(),
            "corpus_commitment_sha256": checked.corpus_commitment_sha256(),
            "annotation_manifest_sha256": checked.canonical_manifest_sha256(),
            "annotation_set_commitment_sha256": checked.annotation_set_commitment_sha256(),
            "entry_count": checked.entry_count(),
            "raw_annotations_emitted": false,
            "artifact_paths_emitted": false,
            "player_aliases_emitted": false,
            "source_identifiers_emitted": false,
            "safe_for_live_semantic_evidence": checked.safe_for_live_semantic_evidence(),
            "safe_for_model_scoring": checked.safe_for_model_scoring(),
            "safe_for_input": checked.safe_for_input()
        }))
        .map_err(|error| format!("serialize checked annotation summary: {error}"))?
    );
    Ok(())
}

fn load_checked_artifact_v1(
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
