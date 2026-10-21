//! Converts an ordinary model source to the no-state-digest input contract
//! (`expanded_deck_training_v1::no_state_digest_v1`).
//!
//! usage: no_state_digest_convert_v1 source=<source.json> out=<new directory>

use mtg_kernel::expanded_deck_training_v1::no_state_digest_v1::convert_no_state_digest_v1;
use std::path::PathBuf;

fn main() {
    let mut source = None;
    let mut out = None;
    for arg in std::env::args().skip(1) {
        match arg.split_once('=') {
            Some(("source", value)) => source = Some(PathBuf::from(value)),
            Some(("out", value)) => out = Some(PathBuf::from(value)),
            _ => {
                eprintln!("usage: no_state_digest_convert_v1 source=<source.json> out=<directory>");
                std::process::exit(2);
            }
        }
    }
    let (Some(source), Some(out)) = (source, out) else {
        eprintln!("usage: no_state_digest_convert_v1 source=<source.json> out=<directory>");
        std::process::exit(2);
    };
    match convert_no_state_digest_v1(&source, &out) {
        Ok(outputs) => println!(
            "{}",
            serde_json::to_string_pretty(&outputs).expect("outputs serialize")
        ),
        Err(error) => {
            eprintln!("no-state-digest conversion failed: {error}");
            std::process::exit(1);
        }
    }
}
