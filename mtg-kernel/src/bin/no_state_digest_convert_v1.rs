//! Converts an ordinary model source to the `no-state-digest-v1` or
//! `no-digests-v1` input contract (`expanded_deck_training_v1::no_state_digest_v1`).
//!
//! usage: no_state_digest_convert_v1 source=<source.json> out=<new directory> [contract=no-state-digest-v1|no-digests-v1]

use mtg_kernel::expanded_deck_training_v1::no_state_digest_v1::{
    convert_no_digests_v1, convert_no_state_digest_v1, CONTRACT_NO_DIGESTS_V1, CONTRACT_V1,
};
use std::path::PathBuf;

fn main() {
    let mut source = None;
    let mut out = None;
    let mut contract = CONTRACT_V1.to_owned();
    for arg in std::env::args().skip(1) {
        match arg.split_once('=') {
            Some(("source", value)) => source = Some(PathBuf::from(value)),
            Some(("out", value)) => out = Some(PathBuf::from(value)),
            Some(("contract", value)) => contract = value.to_owned(),
            _ => {
                eprintln!(
                    "usage: no_state_digest_convert_v1 source=<source.json> out=<directory> [contract={CONTRACT_V1}|{CONTRACT_NO_DIGESTS_V1}]"
                );
                std::process::exit(2);
            }
        }
    }
    let (Some(source), Some(out)) = (source, out) else {
        eprintln!(
            "usage: no_state_digest_convert_v1 source=<source.json> out=<directory> [contract={CONTRACT_V1}|{CONTRACT_NO_DIGESTS_V1}]"
        );
        std::process::exit(2);
    };
    let result = match contract.as_str() {
        CONTRACT_V1 => convert_no_state_digest_v1(&source, &out),
        CONTRACT_NO_DIGESTS_V1 => convert_no_digests_v1(&source, &out),
        other => Err(format!("unsupported contract: {other}")),
    };
    match result {
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
