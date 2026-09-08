#[cfg(not(target_os = "windows"))]
compile_error!("probe_mtgo_visible_game_log_v1 is Windows-only");

#[cfg(target_os = "windows")]
use mtgo_dxgi_capture_v1::{
    probe_mtgo_process_epoch_visible_game_log_v1, CaptureWindowModeV2, MtgoDxgiCaptureRequestV3,
};
#[cfg(target_os = "windows")]
use serde::Serialize;
#[cfg(target_os = "windows")]
use std::{env, process::ExitCode};

#[cfg(target_os = "windows")]
#[derive(Serialize)]
struct VisibleGameLogProbeSummaryV1 {
    schema_version: u32,
    record_count: usize,
    event_count: usize,
    classified_source_record_count: usize,
    unclassified_source_record_count: usize,
    source_bound_to_stable_game_window_and_process_epoch: bool,
    transport_commitments_emitted: bool,
    player_aliases_emitted: bool,
    source_identifiers_emitted: bool,
    complete_for_current_state_reconstruction: bool,
    safe_for_model_scoring: bool,
    safe_for_input: bool,
}

#[cfg(target_os = "windows")]
fn main() -> ExitCode {
    match run_v1() {
        Ok(summary) => match serde_json::to_string_pretty(&summary) {
            Ok(json) => {
                println!("{json}");
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("serialize visible Game Log summary: {error}");
                ExitCode::FAILURE
            }
        },
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(target_os = "windows")]
fn run_v1() -> Result<VisibleGameLogProbeSummaryV1, String> {
    let args = env::args().skip(1).collect::<Vec<_>>();
    if args.len() != 6 {
        return Err(
            "usage: probe_mtgo_visible_game_log_v1 <solitaire_game|duel_game> <game-format> <account-alias> <expected-executable-sha256> <expected-signer-thumbprint> <expected-signer-subject-sha256>"
                .to_owned(),
        );
    }
    let window_mode = match args[0].as_str() {
        "solitaire_game" => CaptureWindowModeV2::SolitaireGame,
        "duel_game" => CaptureWindowModeV2::DuelGame,
        _ => return Err("window mode must be solitaire_game or duel_game".to_owned()),
    };
    let source = probe_mtgo_process_epoch_visible_game_log_v1(MtgoDxgiCaptureRequestV3 {
        expected_executable_sha256: args[3].clone(),
        expected_signer_thumbprint: args[4].clone(),
        expected_signer_subject_sha256: args[5].clone(),
        window_mode,
        expected_game_format: Some(args[1].clone()),
        expected_title_contains: None,
        timeout_ms: 1_000,
    })?;
    let record_count = source.record_count_v1();
    let semantics = source.into_visible_semantics_v1(&args[2])?;
    let summary = VisibleGameLogProbeSummaryV1 {
        schema_version: 1,
        record_count,
        event_count: semantics.event_count_v1(),
        classified_source_record_count: semantics.classified_source_record_count_v1(),
        unclassified_source_record_count: semantics.unclassified_source_record_count_v1(),
        source_bound_to_stable_game_window_and_process_epoch: semantics
            .source_bound_to_stable_game_window_and_process_epoch_v1(),
        transport_commitments_emitted: false,
        player_aliases_emitted: false,
        source_identifiers_emitted: false,
        complete_for_current_state_reconstruction: semantics
            .complete_for_current_state_reconstruction_v1(),
        safe_for_model_scoring: semantics.safe_for_model_scoring_v1(),
        safe_for_input: semantics.safe_for_input_v1(),
    };
    Ok(summary)
}
