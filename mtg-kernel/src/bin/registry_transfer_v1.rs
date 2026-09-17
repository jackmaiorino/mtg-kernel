//! JSON-config entry point for the explicit registry transfer CLI. All logic
//! lives in `mtg_kernel::phase1_registry_transfer_v1`; this file is only I/O
//! plumbing, mirroring `expanded_deck_training_v1.rs`'s CLI shape.

use mtg_kernel::phase1_registry_transfer_v1::{
    run_registry_transfer_cli_v1, RegistryTransferCliRequestV1,
};

fn main() {
    if let Err(error) = run() {
        eprintln!("registry-transfer: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut args = std::env::args_os().skip(1);
    let path = args
        .next()
        .ok_or("usage: registry_transfer_v1 CONFIG.json")?;
    if args.next().is_some() {
        return Err("expected one config path".into());
    }
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    if bytes.len() > 16 * 1024 * 1024 {
        return Err("config exceeds 16 MiB".into());
    }
    let request: RegistryTransferCliRequestV1 =
        serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    let result = run_registry_transfer_cli_v1(&request)?;
    println!(
        "{}",
        serde_json::to_string(&result).map_err(|e| e.to_string())?
    );
    Ok(())
}
