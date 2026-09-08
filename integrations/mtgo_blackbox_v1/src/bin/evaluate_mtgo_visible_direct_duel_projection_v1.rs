use mtgo_blackbox_v1::{
    build_checked_untrusted_acting_player_duel_calibration_corpus_v1,
    check_untrusted_dxgi_capture_artifact_v1,
    check_untrusted_player_visible_direct_duel_projection_set_v1,
    check_untrusted_player_visible_duel_annotation_set_v1,
    evaluate_untrusted_player_visible_direct_duel_projection_v1,
    CheckedUntrustedMtgoDxgiCaptureArtifactV1, MtgoPlayerVisibleDirectDuelProjectionSetV1,
    MtgoPlayerVisibleDuelAnnotationSetV1,
};
use serde::de::DeserializeOwned;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

const MAX_JSON_FILE_BYTES_V1: u64 = 64 * 1_048_576;

fn main() {
    if let Err(error) = run_v1() {
        eprintln!("MTGO_VISIBLE_DIRECT_DUEL_PROJECTION_REJECTED:{error}");
        std::process::exit(1);
    }
}

fn run_v1() -> Result<(), String> {
    let mut arguments = env::args_os();
    let _program = arguments.next();
    let corpus_id = arguments
        .next()
        .ok_or(usage_v1())?
        .into_string()
        .map_err(|_| "corpus ID must be UTF-8")?;
    let annotation_path = PathBuf::from(arguments.next().ok_or(usage_v1())?);
    let projection_path = PathBuf::from(arguments.next().ok_or(usage_v1())?);
    let artifact_directories = arguments.map(PathBuf::from).collect::<Vec<_>>();
    if artifact_directories.is_empty() {
        return Err("at least one artifact directory is required".to_owned());
    }

    let annotations: MtgoPlayerVisibleDuelAnnotationSetV1 =
        read_bounded_json_v1(&annotation_path, "player-visible annotation set")?;
    let projections: MtgoPlayerVisibleDirectDuelProjectionSetV1 =
        read_bounded_json_v1(&projection_path, "player-visible direct projection set")?;
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
    let checked_annotations =
        check_untrusted_player_visible_duel_annotation_set_v1(&corpus, annotations)
            .map_err(|error| error.to_string())?;
    let checked_projections =
        check_untrusted_player_visible_direct_duel_projection_set_v1(&corpus, projections)
            .map_err(|error| error.to_string())?;
    let evaluator_binary_sha256 = sha256_file_v1(
        &env::current_exe().map_err(|error| format!("resolve evaluator executable: {error}"))?,
    )?;
    let evaluation = evaluate_untrusted_player_visible_direct_duel_projection_v1(
        &corpus,
        &checked_annotations,
        &checked_projections,
        &evaluator_binary_sha256,
    )
    .map_err(|error| error.to_string())?;

    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "status": "checked_untrusted_offline_exact_visible_agreement_only",
            "projection_protocol_sha256": checked_projections.projection_protocol_sha256_v1(),
            "producer_execution_attested": checked_projections.producer_execution_attested_v1(),
            "source_frame_association_attested": checked_projections.source_frame_association_attested_v1(),
            "evaluator_binary_sha256": evaluator_binary_sha256,
            "corpus_manifest_sha256": checked_projections.corpus_manifest_sha256_v1(),
            "annotation_set_commitment_sha256": checked_annotations.annotation_set_commitment_sha256(),
            "projection_set_commitment_sha256": checked_projections.projection_set_commitment_sha256_v1(),
            "evaluation_commitment_sha256": evaluation.evaluation_commitment_sha256_v1(),
            "case_count": evaluation.case_count_v1(),
            "prediction_count": evaluation.prediction_count_v1(),
            "prediction_coverage_bps": evaluation.prediction_coverage_bps_v1(),
            "exact_visible_state_count": evaluation.exact_visible_state_count_v1(),
            "exact_visible_legal_action_count": evaluation.exact_visible_legal_action_count_v1(),
            "exact_visible_decision_input_count": evaluation.exact_visible_decision_input_count_v1(),
            "annotated_action_families": evaluation.annotated_action_families_v1(),
            "exact_complete_corpus_agreement": evaluation.exact_complete_corpus_agreement_v1(),
            "raw_direct_source_emitted": false,
            "internal_identifiers_emitted": false,
            "artifact_paths_emitted": false,
            "safe_for_live_semantic_evidence": evaluation.safe_for_live_semantic_evidence_v1(),
            "safe_for_model_scoring": evaluation.safe_for_model_scoring_v1(),
            "safe_for_input": evaluation.safe_for_input_v1()
        }))
        .map_err(|error| format!("serialize direct-source projection evaluation: {error}"))?
    );
    Ok(())
}

fn usage_v1() -> &'static str {
    "usage: evaluate_mtgo_visible_direct_duel_projection_v1 <corpus-id> <annotation-set.json> <projection-set.json> <artifact-directory> [artifact-directory ...]"
}

fn read_bounded_json_v1<T: DeserializeOwned>(path: &Path, label: &str) -> Result<T, String> {
    if !path.is_absolute() || !path.is_file() {
        return Err(format!("{label} must be an existing absolute file"));
    }
    let length = fs::metadata(path)
        .map_err(|error| format!("inspect {label}: {error}"))?
        .len();
    if length == 0 || length > MAX_JSON_FILE_BYTES_V1 {
        return Err(format!("{label} must be between 1 byte and 64 MiB"));
    }
    let bytes = fs::read(path).map_err(|error| format!("read {label}: {error}"))?;
    serde_json::from_slice(&bytes).map_err(|error| format!("parse {label}: {error}"))
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

fn sha256_file_v1(path: &Path) -> Result<String, String> {
    let bytes = fs::read(path).map_err(|error| format!("read evaluator executable: {error}"))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}
