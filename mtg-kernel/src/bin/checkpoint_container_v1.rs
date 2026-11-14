//! Checkpoint container converter and exporter (bookkeeping part 2, design
//! section 5).
//!
//! ```text
//! checkpoint_container_v1 convert --source SOURCE.json --output OUT
//! checkpoint_container_v1 export --input CONTAINER --sha256 HEX --output OUT.json
//! checkpoint_container_v1 weights-snapshot --input CHECKPOINT --sha256 HEX --output OUT
//! checkpoint_container_v1 materialize --template SOURCE.json --input SNAPSHOT --sha256 HEX --output OUT.json
//! checkpoint_container_v1 s-document --input S_CONTAINER --sha256 HEX
//! checkpoint_container_v1 widen --source SOURCE.json --seed-label LABEL --optimizer keep|fresh [--rows TRAJECTORIES.json] --output FRESH_DIR
//! checkpoint_container_v1 w-movement --from W_CONTAINER --from-sha256 HEX --to W_CONTAINER --to-sha256 HEX
//! ```
//!
//! `widen` writes a wide (net-8w128) learner's starting train state from a
//! net-8 checkpoint source by the function-preserving widening, with its
//! receipt (`w_learner_v1::widen_source_v1`).
//!
//! `s-document` prints a native S train-state container's document (config,
//! step, hashes, parameter names and shapes) after its full decode, for
//! Python tools that need an S layout.
//!
//! `weights-snapshot` and `materialize` are W5's weights-only snapshots
//! (`weights_snapshot_v1`): the parameters alone, and a playable checkpoint
//! with zero moments rebuilt from them against a same-lineage template.
//!
//! `SOURCE.json` is the model source (`ExpandedModelSourceV1`) whose
//! `checkpoint` pin is the v1 checkpoint to convert, as a run config names
//! it. `convert` admits only a canonical v1 checkpoint, restores it and the
//! published container through the continuation loader and refuses unless
//! they are the same state; `export` verifies the container against its
//! SHA-256 and writes its v1 JSON bytes. Both print a receipt naming the
//! input and output pins and the state digest.

mtg_kernel::fast_global_allocator!();

use mtg_kernel::expanded_deck_training_v1::w_learner_v1::{
    w_movement_v1, widen_source_with_rows_v1, WideningOptimizerV1,
};
use mtg_kernel::expanded_deck_training_v1::{
    convert_checkpoint_v1_to_v2, export_checkpoint_v2_to_v1, materialize_weights_snapshot_v1,
    s_container_document_v1, weights_snapshot_v1, ExpandedModelSourceV1, PinnedFileV1,
};
use std::path::PathBuf;

const USAGE: &str = "usage: checkpoint_container_v1 convert --source SOURCE.json --output FRESH_ABSOLUTE_PATH | export --input PATH --sha256 HEX --output FRESH_ABSOLUTE_PATH | weights-snapshot --input PATH --sha256 HEX --output FRESH_ABSOLUTE_PATH | materialize --template SOURCE.json --input PATH --sha256 HEX --output FRESH_ABSOLUTE_PATH | s-document --input PATH --sha256 HEX | widen --source SOURCE.json --seed-label LABEL --optimizer keep|fresh [--rows TRAJECTORIES.json] --output FRESH_ABSOLUTE_PATH | w-movement --from PATH --from-sha256 HEX --to PATH --to-sha256 HEX";

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
        "weights-snapshot" => {
            let values = flags_v1(rest, &["--input", "--sha256", "--output"])?;
            let pin = PinnedFileV1 {
                path: PathBuf::from(&values[0]),
                sha256: values[1].clone(),
            };
            weights_snapshot_v1(&pin, &PathBuf::from(&values[2]))
        }
        "materialize" => {
            let values = flags_v1(rest, &["--template", "--input", "--sha256", "--output"])?;
            let pin = PinnedFileV1 {
                path: PathBuf::from(&values[1]),
                sha256: values[2].clone(),
            };
            std::fs::read(&values[0])
                .map_err(|e| e.to_string())
                .and_then(|text| {
                    serde_json::from_slice::<ExpandedModelSourceV1>(&text)
                        .map_err(|e| e.to_string())
                })
                .and_then(|template| {
                    materialize_weights_snapshot_v1(&template, &pin, &PathBuf::from(&values[3]))
                })
        }
        "s-document" => {
            let values = flags_v1(rest, &["--input", "--sha256"])?;
            s_container_document_v1(&PinnedFileV1 {
                path: PathBuf::from(&values[0]),
                sha256: values[1].clone(),
            })
        }
        "widen" => {
            // `--rows` (optional): a JSON list of trajectory pins the parent
            // state itself played, for the equality check.
            let (values, rows) = match flags_v1(
                rest,
                &[
                    "--source",
                    "--seed-label",
                    "--optimizer",
                    "--rows",
                    "--output",
                ],
            ) {
                Some(v) => (
                    vec![v[0].clone(), v[1].clone(), v[2].clone(), v[4].clone()],
                    Some(v[3].clone()),
                ),
                None => (
                    flags_v1(
                        rest,
                        &["--source", "--seed-label", "--optimizer", "--output"],
                    )?,
                    None,
                ),
            };
            let optimizer = match values[2].as_str() {
                "keep" => WideningOptimizerV1::Keep,
                "fresh" => WideningOptimizerV1::Fresh,
                _ => return None,
            };
            let read = |path: &str| std::fs::read(path).map_err(|e| e.to_string());
            read(&values[0])
                .and_then(|text| {
                    serde_json::from_slice::<ExpandedModelSourceV1>(&text)
                        .map_err(|e| e.to_string())
                })
                .and_then(|source| {
                    let rows = rows
                        .map(|path| {
                            read(&path).and_then(|text| {
                                serde_json::from_slice::<Vec<PinnedFileV1>>(&text)
                                    .map_err(|e| e.to_string())
                            })
                        })
                        .transpose()?;
                    widen_source_with_rows_v1(
                        &source,
                        &values[1],
                        optimizer,
                        rows.as_deref(),
                        &PathBuf::from(&values[3]),
                    )
                })
        }
        "w-movement" => {
            let values = flags_v1(rest, &["--from", "--from-sha256", "--to", "--to-sha256"])?;
            w_movement_v1(
                &PinnedFileV1 {
                    path: PathBuf::from(&values[0]),
                    sha256: values[1].clone(),
                },
                &PinnedFileV1 {
                    path: PathBuf::from(&values[2]),
                    sha256: values[3].clone(),
                },
            )
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
