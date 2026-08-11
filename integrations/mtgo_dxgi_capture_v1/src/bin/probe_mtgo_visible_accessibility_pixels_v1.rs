#[cfg(target_os = "windows")]
fn main() -> std::process::ExitCode {
    match mtgo_dxgi_capture_v1::run_visible_accessibility_pixel_corroboration_cli_v1() {
        Ok(summary) => match serde_json::to_string_pretty(&summary) {
            Ok(json) => {
                println!("{json}");
                std::process::ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("serialize visible accessibility pixel summary: {error}");
                std::process::ExitCode::FAILURE
            }
        },
        Err(error) => {
            eprintln!("{error}");
            std::process::ExitCode::FAILURE
        }
    }
}

#[cfg(not(target_os = "windows"))]
fn main() -> std::process::ExitCode {
    eprintln!("this visible accessibility pixel probe requires Windows");
    std::process::ExitCode::FAILURE
}
