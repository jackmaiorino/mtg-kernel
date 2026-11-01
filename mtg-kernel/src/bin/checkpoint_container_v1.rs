//! Checkpoint container converter and exporter (bookkeeping part 2, design
//! section 5).
//!
//! ```text
//! checkpoint_container_v1 convert --source SOURCE.json --output OUT
//! checkpoint_container_v1 export --input CONTAINER --sha256 HEX --output OUT.json
//! ```
//!
//! `SOURCE.json` is the model source (`ExpandedModelSourceV1`) whose
//! `checkpoint` pin is the v1 checkpoint to convert, as a run config names
//! it. `convert` admits only a canonical v1 checkpoint, restores it and the
//! published container through the continuation loader and refuses unless
//! they are the same state; `export` verifies the container against its
//! SHA-256 and writes its v1 JSON bytes. Both print a receipt naming the
//! input and output pins and the state digest.

mtg_kernel::fast_global_allocator!();

use mtg_kernel::expanded_deck_training_v1::{
    convert_checkpoint_v1_to_v2, export_checkpoint_v2_to_v1, ExpandedModelSourceV1, PinnedFileV1,
};
use std::path::PathBuf;

const USAGE: &str = "usage: checkpoint_container_v1 convert --source SOURCE.json --output FRESH_ABSOLUTE_PATH | export --input PATH --sha256 HEX --output FRESH_ABSOLUTE_PATH";

fn flags_v1(rest: &[String], names: &[&str]) -> Option<Vec<String>> {
    if rest.len() != 2 * names.len() {
        return None;
    }
    let mut values: Vec<Option<String>> = vec![None; names.len()];
    for pair in rest.chunks_exact(2) {
        let index = names.iter().position(|name| *name == pair[0])?;
        if values[index].replace(pair[1].clone()).is_some() {
            return None;
        }
    }
    values.into_iter().collect()
}

fn run_v1(args: &[String]) -> Option<Result<String, String>> {
    let (mode, rest) = args.split_first()?;
    let result = match mode.as_str() {
        "convert" => {
            let values = flags_v1(rest, &["--source", "--output"])?;
            std::fs::read(&values[0])
                .map_err(|e| e.to_string())
                .and_then(|text| {
                    serde_json::from_slice::<ExpandedModelSourceV1>(&text)
                        .map_err(|e| e.to_string())
                })
                .and_then(|source| convert_checkpoint_v1_to_v2(&source, &PathBuf::from(&values[1])))
        }
        "export" => {
            let values = flags_v1(rest, &["--input", "--sha256", "--output"])?;
            let pin = PinnedFileV1 {
                path: PathBuf::from(&values[0]),
                sha256: values[1].clone(),
            };
            export_checkpoint_v2_to_v1(&pin, &PathBuf::from(&values[2]))
        }
        _ => return None,
    };
    Some(result.map(|receipt| receipt.to_string()))
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run_v1(&args) {
        None => {
            eprintln!("{USAGE}");
            std::process::exit(2);
        }
        Some(Ok(receipt)) => println!("{receipt}"),
        Some(Err(error)) => {
            eprintln!("CHECKPOINT_CONTAINER_FAILED {error}");
            std::process::exit(1);
        }
    }
}
