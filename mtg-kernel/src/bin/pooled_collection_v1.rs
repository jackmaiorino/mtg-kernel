//! Pooled collection commands (pooled-lineage stage 1).
//!
//! `pooled_collection_v1 shard COLLECT_COMMAND.json PLAN.json OUT_DIR` writes
//! one `collect_parallel` shard command per plan entry as
//! `OUT_DIR/shard-NNN.json`; `expanded_deck_training_v1 shard-NNN.json`
//! executes one on any host. `pooled_collection_v1 merge COLLECT_COMMAND.json
//! SHARD_DIR...` merges the shard outputs, in schedule order, into the
//! command's output directory and prints the receipt.

mtg_kernel::fast_global_allocator!();

use mtg_kernel::expanded_deck_training_v1::pooled_collection_v1::{
    merge_shards_v1, shard_commands_v1, ShardPlanV1,
};
use mtg_kernel::expanded_deck_training_v1::ExpandedTrainingCommandV1;
use std::path::{Path, PathBuf};

const USAGE: &str = "usage: pooled_collection_v1 shard COLLECT_COMMAND.json PLAN.json OUT_DIR | \
                     pooled_collection_v1 merge COLLECT_COMMAND.json SHARD_DIR...";

fn main() {
    mtg_kernel::opt_out_of_power_throttling_v1();
    if let Err(error) = run() {
        eprintln!("pooled collection: {error}");
        std::process::exit(1);
    }
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if bytes.len() > 16 * 1024 * 1024 {
        return Err(format!("{} exceeds 16 MiB", path.display()));
    }
    serde_json::from_slice(&bytes).map_err(|e| format!("{}: {e}", path.display()))
}

fn run() -> Result<(), String> {
    let args: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    let mode = args
        .first()
        .and_then(|mode| mode.to_str())
        .ok_or(USAGE)?
        .to_owned();
    match (mode.as_str(), args.len()) {
        ("shard", 4) => {
            let command: ExpandedTrainingCommandV1 = read_json(&args[1])?;
            let plan: ShardPlanV1 = read_json(&args[2])?;
            let shards = shard_commands_v1(&command, &plan.shards)?;
            std::fs::create_dir(&args[3]).map_err(|e| format!("{}: {e}", args[3].display()))?;
            for (index, shard) in shards.iter().enumerate() {
                let bytes = serde_json::to_vec(shard).map_err(|e| e.to_string())?;
                let path = args[3].join(format!("shard-{index:03}.json"));
                std::fs::write(&path, bytes).map_err(|e| format!("{}: {e}", path.display()))?;
            }
            println!("{}", shards.len());
            Ok(())
        }
        ("merge", n) if n >= 3 => {
            let command: ExpandedTrainingCommandV1 = read_json(&args[1])?;
            let receipt = merge_shards_v1(&command, &args[2..])?;
            println!(
                "{}",
                serde_json::to_string(&receipt).map_err(|e| e.to_string())?
            );
            Ok(())
        }
        _ => Err(USAGE.into()),
    }
}
