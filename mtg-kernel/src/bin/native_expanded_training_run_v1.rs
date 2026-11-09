mtg_kernel::fast_global_allocator!();

use mtg_kernel::expanded_deck_training_v1::{portable_chain_v1, ExpandedModelSourceV1};
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
    match std::env::args_os()
        .nth(1)
        .as_deref()
        .and_then(std::ffi::OsStr::to_str)
    {
        Some("--write-portable-chain") => return write_portable_chain(),
        Some("--inspect-portable-chain") => return inspect_portable_chain(),
        Some("--validate-config") => {
            // Only the read-only parser uses this stack.
            return std::thread::Builder::new()
                .name("expanded-config-validation".into())
                .stack_size(16 * 1024 * 1024)
                .spawn(validate_config_mode_v1)
                .map_err(|error| error.to_string())?
                .join()
                .map_err(|_| "config validation thread panicked".to_owned())?;
        }
        _ => {}
    }
    let usage = concat!(
        "usage: native_expanded_training_run_v1 CONFIG.json [--max-new-iterations N] ",
        "[--gpu-exact-inference ORDINAL [--gpu-exact-latency-us N]] [--gpu-exact-learner ORDINAL] ",
        "[--gpu-exact-kernels v1|v2] [--gpu-exact-wait spin|block] ",
        "[--portable-output ABS_PACKAGE_DIRECTORY]"
    );
    let mut args = std::env::args_os().skip(1);
    let path = PathBuf::from(args.next().ok_or(usage)?);
    let mut limit = None;
    let mut gpu_inference: Option<usize> = None;
    let mut gpu_latency_us: Option<u64> = None;
    let mut gpu_learner: Option<usize> = None;
    let mut gpu_kernels: Option<String> = None;
    let mut gpu_wait: Option<String> = None;
    let mut portable_output = None;
    let number = |value: Option<std::ffi::OsString>, what: &str| -> Result<u64, String> {
        value
            .ok_or(format!("missing {what}"))?
            .to_str()
            .ok_or(format!("invalid {what}"))?
            .parse::<u64>()
            .map_err(|e| format!("{what}: {e}"))
    };
    let text = |value: Option<std::ffi::OsString>, what: &str| -> Result<String, String> {
        value
            .ok_or(format!("missing {what}"))?
            .into_string()
            .map_err(|_| format!("invalid {what}"))
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
            Some("--gpu-exact-kernels") if gpu_kernels.is_none() => {
                gpu_kernels = Some(text(args.next(), "kernel version")?);
            }
            Some("--gpu-exact-wait") if gpu_wait.is_none() => {
                gpu_wait = Some(text(args.next(), "wait mode")?);
            }
            Some("--portable-output") if portable_output.is_none() => {
                let output = PathBuf::from(args.next().ok_or("missing portable output path")?);
                if !output.is_absolute() || output.try_exists().map_err(|e| e.to_string())? {
                    return Err("portable output must be an unused absolute directory".into());
                }
                portable_output = Some(output);
            }
            _ => return Err(format!("unknown option; {usage}")),
        }
    }
    if gpu_latency_us.is_some() && gpu_inference.is_none() {
        return Err("--gpu-exact-latency-us needs --gpu-exact-inference".into());
    }
    if (gpu_kernels.is_some() || gpu_wait.is_some())
        && gpu_inference.is_none()
        && gpu_learner.is_none()
    {
        return Err(
            "--gpu-exact-kernels and --gpu-exact-wait need --gpu-exact-inference or --gpu-exact-learner"
                .into(),
        );
    }
    // Zero: eager batching (every queued request, no extra wait).
    let gpu_latency_us = gpu_latency_us.unwrap_or(0);
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
    // The kernel version and wait mode change timing only, like the backends.
    if gpu_kernels.is_some() || gpu_wait.is_some() {
        #[cfg(feature = "gpu-exact-v1")]
        mtg_kernel::gpu_exact_v1::configure_v1(
            gpu_kernels.as_deref().unwrap_or("v2"),
            gpu_wait.as_deref().unwrap_or("spin"),
        )?;
    }
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
        return Err(format!(
            "--gpu-exact-learner {ordinal} needs a build with gpu-exact-v1"
        ));
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
    let mut result = result?;
    if let Some(output) = portable_output.filter(|_| result["complete"] == true) {
        let source: ExpandedModelSourceV1 =
            serde_json::from_value(result["source"].clone()).map_err(|e| e.to_string())?;
        let manifest = portable_chain_v1::write_v1(&source, &output)?;
        result["portable_chain"] = serde_json::to_value(manifest).map_err(|e| e.to_string())?;
    }
    println!(
        "{}",
        serde_json::to_string(&result).map_err(|e| e.to_string())?
    );
    Ok(())
}

fn write_portable_chain() -> Result<(), String> {
    let usage = concat!(
        "usage: native_expanded_training_run_v1 ",
        "--write-portable-chain ABS_SOURCE.json ABS_PACKAGE_DIRECTORY"
    );
    let mut args = std::env::args_os().skip(2);
    let source = PathBuf::from(args.next().ok_or(usage)?);
    let output = PathBuf::from(args.next().ok_or(usage)?);
    if !source.is_absolute() || args.next().is_some() {
        return Err(usage.into());
    }
    const SOURCE_CAP: u64 = 1024 * 1024;
    let metadata = std::fs::metadata(&source).map_err(|error| error.to_string())?;
    if !metadata.is_file() || metadata.len() > SOURCE_CAP {
        return Err("portable source is not a bounded regular file".into());
    }
    let file = std::fs::File::open(source).map_err(|error| error.to_string())?;
    let mut bytes = Vec::new();
    file.take(SOURCE_CAP + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() as u64 > SOURCE_CAP {
        return Err("portable source exceeds byte cap".into());
    }
    let source: ExpandedModelSourceV1 =
        serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
    let manifest = portable_chain_v1::write_v1(&source, &output)?;
    println!(
        "{}",
        serde_json::to_string(&manifest).map_err(|error| error.to_string())?
    );
    Ok(())
}

fn inspect_portable_chain() -> Result<(), String> {
    let usage = concat!(
        "usage: native_expanded_training_run_v1 ",
        "--inspect-portable-chain ABS_PACKAGE_DIRECTORY MANIFEST_SHA256"
    );
    let mut args = std::env::args_os().skip(2);
    let output = PathBuf::from(args.next().ok_or(usage)?);
    let expected = args.next().ok_or(usage)?;
    if args.next().is_some() {
        return Err(usage.into());
    }
    let identity = portable_chain_v1::inspect_v1(&output, expected.to_str().ok_or(usage)?)?;
    println!(
        "{}",
        serde_json::to_string(&identity).map_err(|error| error.to_string())?
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
