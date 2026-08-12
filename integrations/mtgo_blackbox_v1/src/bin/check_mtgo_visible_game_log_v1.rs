fn main() {
    let (path, emit_visible_text) = match arguments_v1() {
        Ok(arguments) => arguments,
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
    let visible_records = emit_visible_text.then(|| {
        (0..projection.record_count_v1())
            .filter_map(|index| projection.record_v1(index))
            .map(|record| record.visible_text_v1().to_owned())
            .collect::<Vec<_>>()
    });
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
        "visible_records": visible_records,
    });
    match serde_json::to_string(&summary) {
        Ok(json) => println!("{json}"),
        Err(error) => {
            eprintln!("serialize visible Game Log summary: {error}");
            std::process::exit(1);
        }
    }
}

fn arguments_v1() -> Result<(std::path::PathBuf, bool), String> {
    let arguments = std::env::args_os().skip(1).collect::<Vec<_>>();
    match arguments.as_slice() {
        [path] => Ok((path.into(), false)),
        [flag, path] if flag == "--emit-visible-text" => Ok((path.into(), true)),
        _ => Err(
            "usage: check_mtgo_visible_game_log_v1 [--emit-visible-text] <Match_GameLog_*.dat>"
                .to_owned(),
        ),
    }
}
