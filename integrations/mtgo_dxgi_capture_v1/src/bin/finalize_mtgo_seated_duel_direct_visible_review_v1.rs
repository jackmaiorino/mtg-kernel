#[cfg(not(target_os = "windows"))]
compile_error!("finalize_mtgo_seated_duel_direct_visible_review_v1 is Windows-only");

fn main() {
    match mtgo_dxgi_capture_v1::run_seated_duel_direct_visible_review_finalization_cli_v1() {
        Ok(receipt) => match serde_json::to_string_pretty(&receipt) {
            Ok(json) => println!("{json}"),
            Err(error) => {
                eprintln!("MTGO_SEATED_DUEL_REVIEW_FINALIZATION_REJECTED:{error}");
                std::process::exit(1);
            }
        },
        Err(error) => {
            eprintln!("MTGO_SEATED_DUEL_REVIEW_FINALIZATION_REJECTED:{error}");
            std::process::exit(1);
        }
    }
}
