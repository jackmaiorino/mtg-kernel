//! Inspect a pinned fresh model and exact Adam0 bootstrap. No game or update.
use mtg_kernel::expanded_deck_training_v1::inspect_fresh_initialization_json_v1;
use std::{io::Read, path::PathBuf};

fn main() {
    let result = std::thread::Builder::new()
        .stack_size(16 * 1024 * 1024)
        .spawn(run)
        .map_err(|e| e.to_string())
        .and_then(|h| {
            h.join()
                .map_err(|_| "inspection thread panicked".to_owned())
                .and_then(|r| r)
        });
    if let Err(error) = result {
        eprintln!("fresh initialization inspection: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut args = std::env::args_os().skip(1);
    let path = PathBuf::from(
        args.next()
            .ok_or("usage: phase1_fresh_initialization_v1 ABS_CONFIG.json")?,
    );
    if args.next().is_some() || !path.is_absolute() {
        return Err("one absolute config path required".into());
    }
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    if !file.metadata().map_err(|e| e.to_string())?.is_file() {
        return Err("regular config file required".into());
    }
    let mut bytes = Vec::new();
    file.take(1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > 1024 * 1024 {
        return Err("config exceeds 1 MiB".into());
    }
    let text = std::str::from_utf8(&bytes).map_err(|e| e.to_string())?;
    let result = inspect_fresh_initialization_json_v1(text)?;
    println!(
        "{}",
        serde_json::to_string(&result).map_err(|e| e.to_string())?
    );
    Ok(())
}
