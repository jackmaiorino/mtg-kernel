#[cfg(not(target_os = "windows"))]
compile_error!("bind_mtgo_approved_client_for_two_local_clients_v1 is Windows-only");

use mtgo_dxgi_capture_v1::bind_foreground_approved_mtgo_client_for_two_local_clients_v1;
use std::path::Path;

fn main() {
    if let Err(error) = run() {
        eprintln!("MTGO_APPROVED_CLIENT_TWO_LOCAL_CLIENT_BINDING_REJECTED:{error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    if args.len() != 1 {
        return Err("expected OUTPUT_BINDING_FILE".to_owned());
    }
    let receipt =
        bind_foreground_approved_mtgo_client_for_two_local_clients_v1(Path::new(&args[0]))?;
    println!(
        "{}",
        serde_json::to_string_pretty(&receipt)
            .map_err(|error| format!("serialize approved-client binding receipt: {error}"))?
    );
    Ok(())
}
