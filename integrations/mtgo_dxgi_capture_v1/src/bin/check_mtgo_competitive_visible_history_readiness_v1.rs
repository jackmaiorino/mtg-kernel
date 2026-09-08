#[cfg(not(target_os = "windows"))]
compile_error!("check_mtgo_competitive_visible_history_readiness_v1 is Windows-only");

#[cfg(target_os = "windows")]
fn main() {
    let report = mtgo_dxgi_capture_v1::check_competitive_visible_history_readiness_v1();
    match serde_json::to_string_pretty(&report) {
        Ok(json) => println!("{json}"),
        Err(error) => {
            eprintln!("serialize competitive visible-history readiness report: {error}");
            std::process::exit(1);
        }
    }
}
