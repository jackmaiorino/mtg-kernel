#[cfg(not(target_os = "windows"))]
compile_error!("check_mtgo_competitive_wiring_readiness_v1 is Windows-only");

fn main() {
    let report = mtgo_dxgi_capture_v1::check_competitive_wiring_static_readiness_v1();
    match serde_json::to_string_pretty(&report) {
        Ok(json) => println!("{json}"),
        Err(error) => {
            eprintln!("MTGO_COMPETITIVE_READINESS_REJECTED:{error}");
            std::process::exit(1);
        }
    }
}
