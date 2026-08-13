fn main() {
    match mtgo_dxgi_capture_v1::run_visible_accessibility_catalog_corpus_evaluation_cli_v1() {
        Ok(summary) => match serde_json::to_string_pretty(&summary) {
            Ok(json) => println!("{json}"),
            Err(error) => {
                eprintln!(
                    "MTGO_VISIBLE_ACCESSIBILITY_CATALOG_CORPUS_REJECTED:serialize summary: {error}"
                );
                std::process::exit(1);
            }
        },
        Err(error) => {
            eprintln!("MTGO_VISIBLE_ACCESSIBILITY_CATALOG_CORPUS_REJECTED:{error}");
            std::process::exit(1);
        }
    }
}
