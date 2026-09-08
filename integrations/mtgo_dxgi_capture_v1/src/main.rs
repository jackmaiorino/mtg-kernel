#[cfg(not(target_os = "windows"))]
compile_error!("mtgo-dxgi-capture-v1 is Windows-only");

fn main() {
    match mtgo_dxgi_capture_v1::run_cli_v3() {
        Ok(output) => println!("{}", output.display()),
        Err(error) => {
            eprintln!("MTGO_DXGI_CAPTURE_REJECTED:{error}");
            std::process::exit(1);
        }
    }
}
