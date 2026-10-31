mtg_kernel::fast_global_allocator!();

use mtg_kernel::native_expanded_training_run_v1::{
    run_native_expanded_training_v1, NativeExpandedTrainingRunV1,
};
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::PathBuf;

const MAX_VALIDATION_CONFIG_BYTES: usize = 16 * 1024 * 1024;

fn main() {
    // SAFETY: first statement of main; no other thread exists yet.
    unsafe { mtg_kernel::retain_freed_memory_v1() };
    mtg_kernel::opt_out_of_power_throttling_v1();
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
    let usage = concat!(
        "usage: native_expanded_training_run_v1 CONFIG.json [--max-new-iterations N] ",
        "[--gpu-exact-inference ORDINAL [--gpu-exact-latency-us N]] [--gpu-exact-learner ORDINAL]"
    );
    let mut args = std::env::args_os().skip(1);
    let path = PathBuf::from(args.next().ok_or(usage)?);
    let mut limit = None;
    let mut gpu_inference: Option<usize> = None;
    let mut gpu_latency_us: Option<u64> = None;
    let mut gpu_learner: Option<usize> = None;
    let number = |value: Option<std::ffi::OsString>, what: &str| -> Result<u64, String> {
        value
            .ok_or(format!("missing {what}"))?
            .to_str()
            .ok_or(format!("invalid {what}"))?
            .parse::<u64>()
            .map_err(|e| format!("{what}: {e}"))
    };
    while let Some(flag) = args.next() {
        match flag.to_str() {
            Some("--max-new-iterations") if limit.is_none() => {
                limit = Some(number(args.next(), "iteration limit")? as usize);
            }
            Some("--gpu-exact-inference") if gpu_inference.is_none() => {
                gpu_inference = Some(number(args.next(), "GPU ordinal")? as usize);
            }
            Some("--gpu-exact-learner") if gpu_learner.is_none() => {
                gpu_learner = Some(number(args.next(), "GPU ordinal")? as usize);
            }
            Some("--gpu-exact-latency-us") if gpu_latency_us.is_none() => {
                gpu_latency_us = Some(number(args.next(), "latency")?);
            }
            _ => return Err(format!("unknown option; {usage}")),
        }
    }
    if gpu_latency_us.is_some() && gpu_inference.is_none() {
        return Err("--gpu-exact-latency-us needs --gpu-exact-inference".into());
    }
    let gpu_latency_us = gpu_latency_us.unwrap_or(200);
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
    // Exact GPU inference changes timing only: replies equal the CPU
    // forward's bits, so it is an execution option outside the run identity.
    // Its receipt goes to stderr and to the output's gpu-exact-receipts/.
    if let Some(ordinal) = gpu_inference {
        #[cfg(feature = "gpu-exact-v1")]
        mtg_kernel::gpu_exact_v1::install_inference_service_v1(ordinal, gpu_latency_us)?;
        #[cfg(not(feature = "gpu-exact-v1"))]
        return Err(format!(
            "--gpu-exact-inference {ordinal} ({gpu_latency_us} us) needs a build with gpu-exact-v1"
        ));
    }
    // The exact GPU learner likewise gives the CPU backward's gradients.
    if let Some(ordinal) = gpu_learner {
        #[cfg(feature = "gpu-exact-v1")]
        mtg_kernel::gpu_exact_v1::install_learner_v1(ordinal)?;
        #[cfg(not(feature = "gpu-exact-v1"))]
        return Err(format!("--gpu-exact-learner {ordinal} needs a build with gpu-exact-v1"));
    }
    let result = run_native_expanded_training_v1(&config, limit);
    #[cfg(feature = "gpu-exact-v1")]
    if gpu_inference.is_some() || gpu_learner.is_some() {
        // Written for failed invocations too: which backend ran is provenance.
        match mtg_kernel::gpu_exact_v1::invocation_receipt_v1(
            &config.output_directory,
            result.as_ref().map_err(String::as_str),
        ) {
            Ok(receipt) => eprintln!("{receipt}"),
            // A run error stays the reported failure; an otherwise complete
            // run without its provenance receipt fails.
            Err(error) if result.is_err() => eprintln!("gpu-exact receipt not written: {error}"),
            Err(error) => return Err(format!("gpu-exact receipt not written: {error}")),
        }
    }
    let result = result?;
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
    let path = PathBuf::from(
        args.next()
            .ok_or("usage: native_expanded_training_run_v1 --validate-config ABS_CONFIG.json")?,
    );
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
    let episode_count = config
        .iterations
        .iter()
        .try_fold(0_usize, |total, iteration| {
            total
                .checked_add(iteration.episodes.len())
                .ok_or("episode count overflow")
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
    println!(
        "{}",
        serde_json::to_string(&result).map_err(|error| error.to_string())?
    );
    Ok(())
}
