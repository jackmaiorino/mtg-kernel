fn main() {
    let path = match one_path_argument_v1() {
        Ok(path) => path,
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(2);
        }
    };
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) => {
            eprintln!("read visible Game Log candidate: {error}");
            std::process::exit(1);
        }
    };
    let projection =
        match mtgo_blackbox_v1::parse_checked_untrusted_mtgo_visible_game_log_v1(&bytes) {
            Ok(projection) => projection,
            Err(error) => {
                eprintln!("{}: {}", error.code(), error.detail());
                std::process::exit(1);
            }
        };
    let summary = serde_json::json!({
        "schema_version": mtgo_blackbox_v1::MTGO_VISIBLE_GAME_LOG_FILE_PROJECTION_SCHEMA_V1,
        "source_file_sha256": projection.source_file_sha256_v1(),
        "source_match_id_commitment_sha256": projection.source_match_id_commitment_sha256_v1(),
        "projection_commitment_sha256": projection.projection_commitment_sha256_v1(),
        "record_count": projection.record_count_v1(),
        "nonrendered_source_metadata_discarded": projection.nonrendered_source_metadata_discarded_v1(),
        "safe_for_current_game_semantic_evidence": projection.safe_for_current_game_semantic_evidence_v1(),
        "safe_for_model_scoring": projection.safe_for_model_scoring_v1(),
        "safe_for_input": projection.safe_for_input_v1(),
    });
    match serde_json::to_string(&summary) {
        Ok(json) => println!("{json}"),
        Err(error) => {
            eprintln!("serialize visible Game Log summary: {error}");
            std::process::exit(1);
        }
    }
}

fn one_path_argument_v1() -> Result<std::path::PathBuf, String> {
    let mut arguments = std::env::args_os().skip(1);
    let path = arguments
        .next()
        .ok_or("usage: check_mtgo_visible_game_log_v1 <Match_GameLog_*.dat>")?;
    if arguments.next().is_some() {
        return Err("exactly one visible Game Log path is required".to_owned());
    }
    Ok(path.into())
}
