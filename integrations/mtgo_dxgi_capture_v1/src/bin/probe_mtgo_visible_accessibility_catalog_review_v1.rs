fn main() {
    match mtgo_dxgi_capture_v1::run_visible_accessibility_known_label_catalog_review_artifact_cli_v1(
    ) {
        Ok(receipt) => match serde_json::to_string_pretty(&receipt) {
            Ok(json) => println!("{json}"),
            Err(error) => {
                eprintln!(
                    "MTGO_VISIBLE_ACCESSIBILITY_CATALOG_REVIEW_REJECTED:serialize receipt: {error}"
                );
                std::process::exit(1);
            }
        },
        Err(error) => {
            eprintln!("MTGO_VISIBLE_ACCESSIBILITY_CATALOG_REVIEW_REJECTED:{error}");
            std::process::exit(1);
        }
    }
}
