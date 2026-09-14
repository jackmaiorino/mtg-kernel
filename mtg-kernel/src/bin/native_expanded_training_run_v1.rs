use mtg_kernel::native_expanded_training_run_v1::{
    run_native_expanded_training_v1, NativeExpandedTrainingRunV1,
};
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::PathBuf;

const MAX_VALIDATION_CONFIG_BYTES: usize = 16 * 1024 * 1024;

fn main() {
    if let Err(error) = run() {
        eprintln!("native expanded training: {error}");
        std::process::exit(1);
    }
}
fn run() -> Result<(), String> {
    if std::env::args_os().nth(1).as_deref() == Some(std::ffi::OsStr::new("--validate-config")) {
        // Only the new read-only parser uses this stack. Preserve the legacy
        // entry point, parsing semantics and training execution below.
        return std::thread::Builder::new()
            .name("expanded-config-validation".into())
            .stack_size(16 * 1024 * 1024)
            .spawn(validate_config_mode_v1)
            .map_err(|error| error.to_string())?
            .join()
            .map_err(|_| "config validation thread panicked".to_owned())?;
    }
    let mut args = std::env::args_os().skip(1);
    let path = PathBuf::from(
        args.next()
            .ok_or("usage: native_expanded_training_run_v1 CONFIG.json [--max-new-iterations N]")?,
    );
    let limit = if let Some(flag) = args.next() {
        if flag != "--max-new-iterations" {
            return Err("unknown option".into());
        }
        Some(
            args.next()
                .ok_or("missing iteration limit")?
                .to_str()
                .ok_or("invalid iteration limit")?
                .parse::<usize>()
                .map_err(|e| e.to_string())?,
        )
    } else {
        None
    };
    if args.next().is_some() {
        return Err("unexpected arguments".into());
    }
    if !path.is_absolute() {
        return Err("absolute config path required".into());
    }
    let metadata = std::fs::metadata(&path).map_err(|e| e.to_string())?;
    if !metadata.is_file() || metadata.len() > 16 * 1024 * 1024 {
        return Err("config exceeds 16 MiB".into());
    }
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    if bytes.len() > 16 * 1024 * 1024 {
        return Err("config exceeds 16 MiB".into());
    }
    let config: NativeExpandedTrainingRunV1 =
        serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    let result = run_native_expanded_training_v1(&config, limit)?;
    println!(
        "{}",
        serde_json::to_string(&result).map_err(|e| e.to_string())?
    );
    Ok(())
}

/// Config/card validation only. This path never follows a model/checkpoint
/// pin, initializes a learner, opens a GPU, creates output paths or runs games.
/// Backend availability and source contents remain execution-time checks.
fn validate_config_mode_v1() -> Result<(), String> {
    let mut args = std::env::args_os().skip(2);
    let path = PathBuf::from(args.next().ok_or(
        "usage: native_expanded_training_run_v1 --validate-config ABS_CONFIG.json",
    )?);
    if args.next().is_some() {
        return Err("unexpected validation arguments".into());
    }
    if !path.is_absolute() {
        return Err("absolute config path required".into());
    }
    let file = std::fs::File::open(&path).map_err(|error| error.to_string())?;
    let metadata = file.metadata().map_err(|error| error.to_string())?;
    if !metadata.is_file() || metadata.len() > MAX_VALIDATION_CONFIG_BYTES as u64 {
        return Err("config exceeds 16 MiB or is not a regular file".into());
    }
    let mut bytes = Vec::new();
    file.take(MAX_VALIDATION_CONFIG_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() > MAX_VALIDATION_CONFIG_BYTES {
        return Err("config exceeds 16 MiB".into());
    }
    let text = std::str::from_utf8(&bytes).map_err(|error| error.to_string())?;
    let config = NativeExpandedTrainingRunV1::from_json_v1(text)?;
    config.validate_v1()?;
    let episode_count = config.iterations.iter().try_fold(0_usize, |total, iteration| {
        total.checked_add(iteration.episodes.len()).ok_or("episode count overflow")
    })?;
    let result = serde_json::json!({
        "schema": "phase1-native-expanded-config-validation/v1",
        "config_sha256": format!("{:x}", Sha256::digest(&bytes)),
        "iterations": config.iterations.len(),
        "episode_count": episode_count,
        "collection_workers": config.collection_workers,
        "preparation_workers": config.preparation_workers,
        "runtime": {
            "engine_commit": env!("MTG_KERNEL_BUILD_GIT_HEAD"),
            "tracked_tree_sha256": env!("MTG_KERNEL_BUILD_TRACKED_TREE_SHA256"),
            "card_db_hash": format!("{:016x}", mtg_kernel::card_def::KERNEL_CARDDB_HASH),
            "card_registry_sha256": format!("{:x}", Sha256::digest(include_bytes!("../../../data/cards_v1.json"))),
            "feature_identity_source_sha256": format!("{:x}", Sha256::digest(include_bytes!("../../../data/flat_policy_v3/feature_identity.rs"))),
        },
        "validation_scope": "config-shape-and-registered-cards-only",
        "native_execution": false,
        "model_loaded": false,
    });
    println!("{}", serde_json::to_string(&result).map_err(|error| error.to_string())?);
    Ok(())
}
