#[cfg(not(target_os = "windows"))]
compile_error!("check_mtgo_reviewed_deck_gate_partial_corpus_v1 is Windows-only");

#[cfg(target_os = "windows")]
use mtgo_dxgi_capture_v1::check_built_in_reviewed_deck_gate_partial_corpus_v1;
#[cfg(target_os = "windows")]
use std::process::ExitCode;

#[cfg(target_os = "windows")]
fn main() -> ExitCode {
    match check_built_in_reviewed_deck_gate_partial_corpus_v1() {
        Ok(report) => match serde_json::to_string_pretty(&report) {
            Ok(json) => {
                println!("{json}");
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("serialize partial deck-gate qualification: {error}");
                ExitCode::FAILURE
            }
        },
        Err(error) => {
            eprintln!("partial deck-gate qualification failed: {error}");
            ExitCode::FAILURE
        }
    }
}
