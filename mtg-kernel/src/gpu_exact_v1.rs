//! Exact-order GPU kernels: GPU results bit-identical to the CPU code that
//! defines them (design: `docs/design/gpu_batched_inference_v1.md`).
//!
//! Every kernel computes each output with the CPU's IEEE operations in the
//! CPU's order (correctly rounded intrinsics, NVRTC `--fmad=false`, no fast
//! math, no flush-to-zero, no atomics). So a GPU forward equals the CPU
//! forward bit for bit, and a request's outputs never depend on the rest of
//! its batch. The device tests are `#[ignore]`d (CI has no GPU) and run with
//! `MTG_KERNEL_GPU_EXACT_TEST_ORDINAL` set.

mod device;
pub(crate) mod net8;
pub(crate) mod service;
#[cfg(test)]
mod tests;

pub(crate) use device::{DeviceIdentityV1, ExactDeviceV1};

/// Starts the exact inference service on device `ordinal` and installs it
/// for this process's collector threads (`service::enter_collector_v1`).
/// Replies equal the CPU forward's bits, so this changes timing only.
pub fn install_inference_service_v1(ordinal: usize, max_latency_micros: u64) -> Result<(), String> {
    let policy = service::BatchPolicyV1 {
        max_latency: std::time::Duration::from_micros(max_latency_micros),
        ..service::BatchPolicyV1::default()
    };
    service::install_global_v1(service::InferenceServiceV1::start_v1(ordinal, policy)?)
}

/// The installed service's device identity and counters, if one is installed.
pub fn inference_receipt_v1() -> Option<serde_json::Value> {
    service::global_receipt_v1()
}

/// The receipt of one RT1 invocation that used an exact GPU backend, written
/// to `output/gpu-exact-receipts/invocation-NNNNNN.json` (the next free
/// number) and returned. Outside every iteration directory and run identity:
/// the outputs are the CPU path's bits, and this records which backend
/// produced them.
pub fn invocation_receipt_v1(
    output_directory: &std::path::Path,
    result: Result<&serde_json::Value, &str>,
) -> Result<serde_json::Value, String> {
    let inference = inference_receipt_v1();
    let receipt = serde_json::json!({
        "schema": "mtg-kernel-gpu-exact-invocation-receipt/v1",
        "inference": inference,
        "completed_iterations": result.ok().and_then(|r| r.get("completed_iterations").cloned()),
        "error": result.err(),
    });
    let bytes = serde_json::to_vec_pretty(&receipt).map_err(|e| e.to_string())?;
    let directory = output_directory.join("gpu-exact-receipts");
    std::fs::create_dir_all(&directory).map_err(|e| format!("{}: {e}", directory.display()))?;
    // `create_new` reserves the name atomically, so concurrent invocations
    // never overwrite each other's receipts.
    for index in 0usize.. {
        let path = directory.join(format!("invocation-{index:06}.json"));
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(mut file) => {
                use std::io::Write;
                file.write_all(&bytes)
                    .map_err(|e| format!("{}: {e}", path.display()))?;
                return Ok(receipt);
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(format!("{}: {error}", path.display())),
        }
    }
    unreachable!("receipt names are unbounded")
}
