//! Checkpoint container converter and exporter (bookkeeping part 2, design
//! section 5).
//!
//! ```text
//! checkpoint_container_v1 convert --input V1.json --sha256 HEX --output OUT
//! checkpoint_container_v1 export --input CONTAINER --sha256 HEX --output OUT.json
//! ```
//!
//! The input is verified against its SHA-256 first. `convert` admits only a
//! canonical v1 checkpoint and refuses unless the container exports exactly
//! the input bytes; `export` writes the v1 JSON bytes of a container. Both
//! print a receipt naming the input and output pins and the state digest.

mtg_kernel::fast_global_allocator!();

use mtg_kernel::expanded_deck_training_v1::{
    convert_checkpoint_v1_to_v2, export_checkpoint_v2_to_v1, PinnedFileV1,
};
use std::path::PathBuf;

const USAGE: &str = "usage: checkpoint_container_v1 (convert|export) --input PATH --sha256 HEX --output FRESH_ABSOLUTE_PATH";

fn parse_v1(args: &[String]) -> Option<(String, PinnedFileV1, PathBuf)> {
    let (mode, rest) = args.split_first()?;
    if rest.len() != 6 {
        return None;
    }
    let (mut input, mut sha256, mut output) = (None, None, None);
    for pair in rest.chunks_exact(2) {
        let slot = match pair[0].as_str() {
            "--input" => &mut input,
            "--sha256" => &mut sha256,
            "--output" => &mut output,
            _ => return None,
        };
        if slot.replace(pair[1].clone()).is_some() {
            return None;
        }
    }
    let pin = PinnedFileV1 {
        path: PathBuf::from(input?),
        sha256: sha256?,
    };
    Some((mode.clone(), pin, PathBuf::from(output?)))
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some((mode, pin, output)) = parse_v1(&args) else {
        eprintln!("{USAGE}");
        std::process::exit(2);
    };
    let result = match mode.as_str() {
        "convert" => convert_checkpoint_v1_to_v2(&pin, &output),
        "export" => export_checkpoint_v2_to_v1(&pin, &output),
        _ => {
            eprintln!("{USAGE}");
            std::process::exit(2);
        }
    };
    match result {
        Ok(receipt) => println!("{receipt}"),
        Err(error) => {
            eprintln!("CHECKPOINT_CONTAINER_FAILED {error}");
            std::process::exit(1);
        }
    }
}
