//! Carries one expanded-deck trainer checkpoint across an appended card wave
//! into this build (`expanded_deck_training_v1::card_wave_transfer_v1`).
//! Reads one request JSON path and prints the result JSON. Starts no
//! training and never modifies its inputs.

use mtg_kernel::expanded_deck_training_v1::card_wave_transfer_v1::{
    run_card_wave_transfer_cli_v1, CardWaveTransferCliRequestV1,
};

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
    let bytes = std::fs::read(path).map_err(err)?;
    let request: CardWaveTransferCliRequestV1 = serde_json::from_slice(&bytes).map_err(err)?;
    let result = run_card_wave_transfer_cli_v1(&request)?;
    println!("{result}");
    Ok(())
}
