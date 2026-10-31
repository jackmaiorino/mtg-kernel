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
pub fn inference_receipt_v1() -> Option<Result<serde_json::Value, String>> {
    service::global_receipt_v1()
}

/// The process's exact learner device, installed by a run that declares it.
struct InstalledLearnerV1 {
    device: ExactDeviceV1,
    buffers: net8_learner::LearnerBuffersV1,
    updates: u64,
    substeps: u64,
    busy_seconds: f64,
}

static LEARNER: std::sync::OnceLock<std::sync::Mutex<InstalledLearnerV1>> = std::sync::OnceLock::new();

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
) -> Result<Vec<Vec<f32>>, String> {
    let learner = LEARNER.get().ok_or("no GPU learner installed")?;
    let mut learner = learner.lock().unwrap_or_else(|p| p.into_inner());
    let started = std::time::Instant::now();
    let InstalledLearnerV1 { device, buffers, .. } = &mut *learner;
    let gradients = net8_learner::net8_gradients_v1(device, buffers, config, tensors, substeps)?;
    learner.updates += 1;
    learner.substeps += substeps.len() as u64;
    learner.busy_seconds += started.elapsed().as_secs_f64();
    Ok(gradients)
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
                "program_sha256": id.program_sha256},
            "updates": learner.updates, "substeps": learner.substeps,
            "busy_seconds": learner.busy_seconds,
        })
    })
}
