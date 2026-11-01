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
pub(crate) mod net8_learner;
pub(crate) mod service;
#[cfg(test)]
mod tests;

pub(crate) use device::{DeviceIdentityV1, ExactDeviceV1};

/// The fault-injection trigger for `kind` (`inference` or `learner`):
/// `MTG_KERNEL_GPU_EXACT_FAULT=inference:N` faults the Nth inference batch,
/// `learner:U` the Uth learner update (1-based).
#[cfg(feature = "gpu-exact-fault-injection-v1")]
pub(crate) fn fault_trigger_v1(kind: &str) -> Option<u64> {
    let value = std::env::var("MTG_KERNEL_GPU_EXACT_FAULT").ok()?;
    let (which, at) = value.split_once(':')?;
    (which == kind).then(|| at.parse().ok()).flatten()
}

/// Sets the process's forward kernels (`"v1"` or `"v2"`, the default) and how
/// a host thread waits for the GPU (`"spin"`, the default, or `"block"`).
/// Call before installing a service or learner. Neither changes a result bit.
pub fn configure_v1(kernels: &str, wait: &str) -> Result<(), String> {
    let kernels = match kernels {
        "v1" => device::KernelsV1::V1,
        "v2" => device::KernelsV1::V2,
        other => return Err(format!("unknown GPU exact kernels {other:?} (v1 or v2)")),
    };
    let blocking_wait = match wait {
        "spin" => false,
        "block" => true,
        other => return Err(format!("unknown GPU wait mode {other:?} (spin or block)")),
    };
    device::configure_v1(device::ExactOptionsV1 {
        kernels,
        blocking_wait,
    })
}

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

/// The process's exact learner device, installed by a run that declares it.
struct InstalledLearnerV1 {
    device: ExactDeviceV1,
    buffers: net8_learner::LearnerBuffersV1,
    updates: u64,
    substeps: u64,
    busy_seconds: f64,
    /// Updates that did not fit the device and ran the CPU backward.
    cpu_fallbacks: u64,
}

static LEARNER: std::sync::OnceLock<std::sync::Mutex<InstalledLearnerV1>> =
    std::sync::OnceLock::new();

/// Opens device `ordinal` for the learner backward. The GAE train step then
/// takes its gradients from it (`native_policy_train_step_v1::
/// gpu_exact_backward_v1`): the same bits, so this changes timing only.
pub fn install_learner_v1(ordinal: usize) -> Result<(), String> {
    let device = ExactDeviceV1::open_v1(ordinal)?;
    LEARNER
        .set(std::sync::Mutex::new(InstalledLearnerV1 {
            device,
            buffers: Default::default(),
            updates: 0,
            substeps: 0,
            busy_seconds: 0.0,
            cpu_fallbacks: 0,
        }))
        .map_err(|_| "a GPU learner is already installed".to_owned())
}

pub(crate) fn learner_installed_v1() -> bool {
    LEARNER.get().is_some()
}

pub(crate) fn learner_gradients_v1(
    config: crate::native_policy_value_net_v1::NativePolicyValueModelConfigV1,
    tensors: &[&[f32]],
    substeps: &[net8_learner::LearnerSubstepV1<'_>],
) -> Result<Option<Vec<Vec<f32>>>, String> {
    let learner = LEARNER.get().ok_or("no GPU learner installed")?;
    let mut learner = learner.lock().unwrap_or_else(|p| p.into_inner());
    let started = std::time::Instant::now();
    #[cfg(feature = "gpu-exact-fault-injection-v1")]
    if fault_trigger_v1("learner") == Some(learner.updates + learner.cpu_fallbacks + 1) {
        learner.device.inject_fault_v1();
    }
    let InstalledLearnerV1 {
        device, buffers, ..
    } = &mut *learner;
    let gradients =
        match net8_learner::net8_gradients_v1(device, buffers, config, tensors, substeps) {
            Ok(gradients) => gradients,
            // Too big for the device: the caller's CPU backward gives the same bits.
            Err(error) if net8_learner::is_resource_error_v1(&error) => {
                learner.cpu_fallbacks += 1;
                return Ok(None);
            }
            Err(error) => return Err(error),
        };
    learner.updates += 1;
    learner.substeps += substeps.len() as u64;
    learner.busy_seconds += started.elapsed().as_secs_f64();
    Ok(Some(gradients))
}

/// The installed learner's device identity and counters, if one is installed.
pub fn learner_receipt_v1() -> Option<serde_json::Value> {
    LEARNER.get().map(|learner| {
        let learner = learner.lock().unwrap_or_else(|p| p.into_inner());
        let id = &learner.device.identity;
        serde_json::json!({
            "schema": "mtg-kernel-gpu-exact-learner-receipt/v1",
            "device": {"ordinal": id.ordinal, "name": id.name,
                "compute_capability": [id.compute_capability.0, id.compute_capability.1],
                "driver_version": id.driver_version,
                "nvrtc_version": [id.nvrtc_version.0, id.nvrtc_version.1],
                "program_sha256": id.program_sha256,
                "host_tanhf_identity": id.host_tanhf_identity, "tanh_proof_key": id.tanh_proof_key},
            "wait": if learner.device.options.blocking_wait { "block" } else { "spin" },
            "updates": learner.updates, "substeps": learner.substeps,
            "cpu_fallbacks": learner.cpu_fallbacks,
            "busy_seconds": learner.busy_seconds,
        })
    })
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
        "learner": learner_receipt_v1(),
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
