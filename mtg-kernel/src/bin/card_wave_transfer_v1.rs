//! Carries one expanded-deck trainer checkpoint across an appended card wave
//! into this build (`expanded_deck_training_v1::card_wave_transfer_v1`).
//! Reads one request JSON path and prints the result JSON. Starts no
//! training and never modifies its inputs. All logic, including the strict
//! decoding of the request, lives in the library; this file is only I/O
//! plumbing.

mtg_kernel::fast_global_allocator!();

use mtg_kernel::expanded_deck_training_v1::card_wave_transfer_v1::run_card_wave_transfer_cli_json_v1;
use std::io::Read;

/// The library's bound on a request document.
const MAX_REQUEST_BYTES: u64 = 1024 * 1024;

fn main() {
    if let Err(error) = run() {
        eprintln!("card-wave transfer: {error}");
        std::process::exit(1);
    }
}

fn err(error: impl std::fmt::Display) -> String {
    error.to_string()
}

fn run() -> Result<(), String> {
    let mut args = std::env::args_os().skip(1);
    let path = args
        .next()
        .ok_or("usage: card_wave_transfer_v1 REQUEST.json")?;
    if args.next().is_some() {
        return Err("expected one request path".into());
    }
    // One byte past the bound is enough for the library to refuse the
    // request, so a wrong path never reads a large file whole.
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .map_err(err)?
        .take(MAX_REQUEST_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(err)?;
    let result = run_card_wave_transfer_cli_json_v1(&bytes)?;
    println!("{result}");
    Ok(())
}
