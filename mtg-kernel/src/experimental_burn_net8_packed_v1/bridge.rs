//! Production trainer bridge for the CudaBurnDense backend.
//!
//! One update: snapshot the CPU train state, reuse the resident device state
//! when the snapshot is bit-identical to the one the device already holds
//! (otherwise import fresh), pack the update's encoded decision views, run
//! the dense group loss step, read back logits/values, tolerance-check the
//! CUDA outputs against the transported scorer bits (the CUDA identity's
//! semantic difference from the CPU backends' bit-exact revalidation), then
//! build every evidence field from the transported bits in the exact CPU f32
//! fold order so the trainer's bit-exact evidence revalidation holds
//! unchanged. The gauge record observes the transported rows and coefficients
//! in the CPU backward traversal order, with the device's raw scorer-bias
//! gradient as the residual witness. The device state is exported and
//! replaces the CPU state; transactional semantics stay CPU-owned, and the
//! exported device state is parked for the next update.

use super::training::{
    build_dense_group_loss_plan_v1, DenseGroupLossPlanV1, ExperimentalDeviceTrainStateV1,
};
use super::{DevicePackedBatch, HostPackingWorkspace};
use crate::native_policy_train_step_v1::{
    selected_log_softmax, NativePhysicalLossTermV1, NativePolicyForwardInputV1,
    NativePolicyPhysicalDecisionV1, NativePolicyTrainErrorV1, NativePolicyTrainStepResultV1,
    NativePolicyValueTrainSnapshotV1, NativePolicyValueTrainStateV1,
    NativePolicyValueTrainStateWideV1, NativeSelectedOutputV1, ScorerBiasGaugeAccumulatorV1,
};
// Only the #[cfg(test)] measurement-mode diagnostic below needs the wide
// snapshot hash; gated the same way to stay warning-free in normal builds.
#[cfg(test)]
use crate::native_policy_train_step_v1::wide_owned_train_snapshot_state_sha256_v1;
use crate::native_policy_value_net_v1::NativeNamedParameterV1;
use std::error::Error;
use std::sync::{Mutex, MutexGuard, PoisonError};

const TRANSPORTED_OUTPUT_ABSOLUTE_TOLERANCE_V1: f32 = 5.0e-3;
const TRANSPORTED_OUTPUT_RELATIVE_TOLERANCE_V1: f32 = 5.0e-3;
const SCORER_SECOND_BIAS_ORDINAL_V1: usize = 28;
/// Group-aligned training chunk size in substeps. Bounds peak backward
/// activation memory; the value keeps a chunk comfortably inside the device
/// while staying large enough that dense-kernel launch overhead is amortized.
const BRIDGE_CHUNK_SUBSTEP_TARGET_V1: usize = 8_192;

#[cfg(test)]
fn validate_policy_anchor_rows_v1<'a>(
    groups: &[NativePolicyPhysicalDecisionV1<'_>],
    anchor_target_probabilities: &'a [Vec<Vec<f32>>],
    policy_anchor_coefficient: f32,
) -> Result<Vec<&'a [f32]>, NativePolicyTrainErrorV1> {
    if !policy_anchor_coefficient.is_finite() || policy_anchor_coefficient <= 0.0 {
        return Err(NativePolicyTrainErrorV1::CudaBackend {
            code: "cuda-burn-dense-bridge-policy-anchor-coefficient",
        });
    }
    if anchor_target_probabilities.len() != groups.len() {
        return Err(NativePolicyTrainErrorV1::CudaBackend {
            code: "cuda-burn-dense-bridge-policy-anchor-group-cardinality",
        });
    }
    let mut flat_rows = Vec::new();
    for (group, anchor_group) in groups.iter().zip(anchor_target_probabilities) {
        if anchor_group.len() != group.substeps.len() {
            return Err(NativePolicyTrainErrorV1::CudaBackend {
                code: "cuda-burn-dense-bridge-policy-anchor-substep-cardinality",
            });
        }
        for (substep, row) in group.substeps.iter().zip(anchor_group) {
            let action_count = substep.expected_raw_action_logit_bits.len();
            if row.len() != action_count {
                return Err(NativePolicyTrainErrorV1::CudaBackend {
                    code: "cuda-burn-dense-bridge-policy-anchor-action-cardinality",
                });
            }
            if row
                .iter()
                .any(|probability| !probability.is_finite() || *probability < 0.0)
            {
                return Err(NativePolicyTrainErrorV1::CudaBackend {
                    code: "cuda-burn-dense-bridge-policy-anchor-probability",
                });
            }
            let sum = row.iter().copied().map(f64::from).sum::<f64>();
            if !sum.is_finite() || (sum - 1.0).abs() > 1.0e-6_f64 {
                return Err(NativePolicyTrainErrorV1::CudaBackend {
                    code: "cuda-burn-dense-bridge-policy-anchor-normalization",
                });
            }
            if row.iter().any(|probability| {
                *probability != 0.0 && !(*probability * probability.ln()).is_finite()
            }) {
                return Err(NativePolicyTrainErrorV1::CudaBackend {
                    code: "cuda-burn-dense-bridge-policy-anchor-p-log-p",
                });
            }
            flat_rows.push(row.as_slice());
        }
    }
    Ok(flat_rows)
}

#[cfg(test)]
fn stable_forward_kl_sum_f64_v1(
    logits: &[f32],
    action_offsets: &[usize],
    anchor_rows: &[&[f32]],
) -> Result<f64, NativePolicyTrainErrorV1> {
    if action_offsets.len() != anchor_rows.len() + 1
        || action_offsets.last().copied() != Some(logits.len())
    {
        return Err(NativePolicyTrainErrorV1::CudaBackend {
            code: "cuda-burn-dense-bridge-policy-anchor-row-slicing",
        });
    }
    let mut total = 0.0_f64;
    for (bounds, parent) in action_offsets.windows(2).zip(anchor_rows) {
        let row = &logits[bounds[0]..bounds[1]];
        if row.len() != parent.len() || row.iter().any(|value| !value.is_finite()) {
            return Err(NativePolicyTrainErrorV1::CudaBackend {
                code: "cuda-burn-dense-bridge-policy-anchor-row-slicing",
            });
        }
        let maximum = row
            .iter()
            .copied()
            .map(f64::from)
            .fold(f64::NEG_INFINITY, f64::max);
        let log_normalizer = row
            .iter()
            .copied()
            .map(|value| (f64::from(value) - maximum).exp())
            .sum::<f64>()
            .ln()
            + maximum;
        let row_kl =
            parent
                .iter()
                .copied()
                .zip(row)
                .try_fold(0.0_f64, |sum, (probability, logit)| {
                    let probability = f64::from(probability);
                    let term = if probability == 0.0 {
                        0.0
                    } else {
                        probability * (probability.ln() - (f64::from(*logit) - log_normalizer))
                    };
                    if term.is_finite() {
                        Ok(sum + term)
                    } else {
                        Err(NativePolicyTrainErrorV1::CudaBackend {
                            code: "cuda-burn-dense-bridge-policy-anchor-objective-nonfinite",
                        })
                    }
                })?;
        if !row_kl.is_finite() {
            return Err(NativePolicyTrainErrorV1::CudaBackend {
                code: "cuda-burn-dense-bridge-policy-anchor-objective-nonfinite",
            });
        }
        total += row_kl;
    }
    if !total.is_finite() {
        return Err(NativePolicyTrainErrorV1::CudaBackend {
            code: "cuda-burn-dense-bridge-policy-anchor-objective-nonfinite",
        });
    }
    Ok(total)
}

fn bridge_error_v1(error: Box<dyn Error>) -> NativePolicyTrainErrorV1 {
    #[cfg(test)]
    eprintln!("cuda-burn-dense bridge failure: {error:?}");
    #[cfg(not(test))]
    let _ = error;
    NativePolicyTrainErrorV1::CudaBackend {
        code: "cuda-burn-dense-bridge-device-failure",
    }
}

/// Device-resident trainer state parked between updates.
///
/// `exported` is the exact snapshot the resident device tensors hold: the
/// snapshot exported by the update that parked them. Reuse requires the next
/// update's candidate snapshot to be bit-identical to it, so the resident
/// path is numerically indistinguishable from a fresh import, and a resumed,
/// rolled-back, or foreign candidate always falls back to importing.
struct ResidentDeviceStateV1 {
    exported: NativePolicyValueTrainSnapshotV1,
    device_state: ExperimentalDeviceTrainStateV1,
}

static RESIDENT_DEVICE_STATE_V1: Mutex<Option<ResidentDeviceStateV1>> = Mutex::new(None);

fn resident_device_state_slot_v1() -> MutexGuard<'static, Option<ResidentDeviceStateV1>> {
    // A poisoned lock only means a panicking thread once held it; any
    // surviving entry stays safe to consider because reuse is gated on exact
    // content equality, never on how the entry got there.
    RESIDENT_DEVICE_STATE_V1
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}

/// Test-only qualification-measurement mode: when set, the transported-logit
/// gate records rejections and per-row metrics instead of failing, so
/// measurement probes can traverse training depths past the gate's current
/// (unratified) bound. Production builds compile without this flag and stay
/// fail-closed unconditionally.
#[cfg(test)]
pub(crate) static TOLERANCE_MEASUREMENT_MODE_V1: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// f64 log-softmax of one row, for measurement only: the metric itself must
/// not carry f32 evaluation noise.
#[cfg(test)]
fn log_softmax_f64_for_measurement_v1(row: &[f32], index: usize) -> f64 {
    let maximum = row
        .iter()
        .fold(f64::NEG_INFINITY, |m, v| m.max(f64::from(*v)));
    let log_sum = row
        .iter()
        .map(|v| (f64::from(*v) - maximum).exp())
        .sum::<f64>()
        .ln();
    (f64::from(row[index]) - maximum) - log_sum
}

#[cfg(test)]
pub(super) static RESIDENT_REUSE_COUNT_V1: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);
#[cfg(test)]
pub(super) static RESIDENT_IMPORT_COUNT_V1: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);

#[cfg(test)]
pub(super) fn clear_resident_device_state_for_test_v1() {
    *resident_device_state_slot_v1() = None;
}

/// Stronger dedicated-process gate: no resident entry and no prior import or
/// reuse event may have occurred. Counters are monotonic for the process, so
/// clearing the slot cannot manufacture freshness.
#[cfg(test)]
pub(crate) fn resident_device_process_is_fresh_for_test_v1() -> bool {
    resident_device_state_slot_v1().is_none()
        && RESIDENT_IMPORT_COUNT_V1.load(std::sync::atomic::Ordering::Relaxed) == 0
        && RESIDENT_REUSE_COUNT_V1.load(std::sync::atomic::Ordering::Relaxed) == 0
}

fn named_tensors_bit_identical_v1(
    left: &[NativeNamedParameterV1],
    right: &[NativeNamedParameterV1],
) -> bool {
    left.len() == right.len()
        && left.iter().zip(right).all(|(a, b)| {
            a.name == b.name
                && a.shape == b.shape
                && a.values.len() == b.values.len()
                && a.values
                    .iter()
                    .zip(&b.values)
                    .all(|(x, y)| x.to_bits() == y.to_bits())
        })
}

/// Bit-level snapshot identity. The state hash covers f32 bit patterns, so
/// the reuse gate must too: derived float equality would conflate -0.0 with
/// +0.0 and admit a resident state whose exported hash differs.
pub(super) fn snapshots_bit_identical_v1(
    left: &NativePolicyValueTrainSnapshotV1,
    right: &NativePolicyValueTrainSnapshotV1,
) -> bool {
    left.adam_step == right.adam_step
        && left.scorer_bias_anchor_bits == right.scorer_bias_anchor_bits
        && named_tensors_bit_identical_v1(&left.parameters, &right.parameters)
        && named_tensors_bit_identical_v1(&left.first_moments, &right.first_moments)
        && named_tensors_bit_identical_v1(&left.second_moments, &right.second_moments)
}

pub(super) fn tolerance_ok_v1(actual: f32, expected: f32) -> bool {
    let difference = (actual - expected).abs();
    difference <= TRANSPORTED_OUTPUT_ABSOLUTE_TOLERANCE_V1
        || difference
            <= TRANSPORTED_OUTPUT_RELATIVE_TOLERANCE_V1 * expected.abs().max(f32::MIN_POSITIVE)
}

const TRANSPORTED_LOGIT_RANGE_ABSOLUTE_TOLERANCE_V2: f64 = 2.0e-3;
const TRANSPORTED_LOGIT_RANGE_RELATIVE_TOLERANCE_V2: f64 = 3.0e-3;
const TRANSPORTED_LOGIT_COMMON_SHIFT_CAP_V2: f64 = 5.0e-2;

/// Scale-aware transported-logit gate. The retired raw per-element 5e-3
/// rule tripped at training depth on proportionate f32 forward drift (the
/// measured 128-update trajectory holds a ~1e-3 ratio of discrepancy to row
/// magnitude at every depth while row magnitude grows with the sharpening
/// policy). What a softmax can observe is the per-row RANGE of
/// discrepancies, equal to the maximum log-odds error, so the gate bounds
/// that range in f64 against the row's transported magnitude, caps the
/// common shift absolutely so f32 resolution loss cannot hide inside a
/// softmax-invariant offset, and rejects nonfinite device or transported
/// values outright.
pub(super) fn validate_transported_logit_row_v2(
    actual_row: &[f32],
    expected_bits: &[u32],
) -> Result<(), &'static str> {
    let mut min_delta = f64::INFINITY;
    let mut max_delta = f64::NEG_INFINITY;
    let mut row_magnitude = 0.0_f64;
    for (actual, bits) in actual_row.iter().zip(expected_bits) {
        if !actual.is_finite() {
            return Err("cuda-burn-dense-bridge-nonfinite-device-output");
        }
        let expected = f64::from(f32::from_bits(*bits));
        if !expected.is_finite() {
            return Err("cuda-burn-dense-bridge-nonfinite-transported-logit");
        }
        let delta = f64::from(*actual) - expected;
        min_delta = min_delta.min(delta);
        max_delta = max_delta.max(delta);
        row_magnitude = row_magnitude.max(expected.abs());
    }
    if max_delta.abs().max(min_delta.abs()) > TRANSPORTED_LOGIT_COMMON_SHIFT_CAP_V2 {
        return Err("cuda-burn-dense-bridge-transported-logit-shift-cap");
    }
    let range = max_delta - min_delta;
    let bound = TRANSPORTED_LOGIT_RANGE_ABSOLUTE_TOLERANCE_V2
        + TRANSPORTED_LOGIT_RANGE_RELATIVE_TOLERANCE_V2 * row_magnitude;
    if range > bound {
        return Err("cuda-burn-dense-bridge-transported-logit-range-tolerance");
    }
    Ok(())
}

/// Shared body of [`train_step_cuda_burn_dense_v1`]/[`train_step_cuda_burn_dense_wide_v1`]:
/// one production training update on the CudaBurnDense backend, generic over
/// which architecture width `snapshot` and the resident device state are.
/// `NativePolicyValueTrainSnapshotV1` and `ExperimentalDeviceTrainStateV1` are
/// both already dimension-erased (the snapshot is plain named-parameter
/// vectors; the device model is the same Burn struct for either width, see
/// its `wide` field), so everything below this point is dims-oblivious except
/// the one `wide`-branched import call. The resident-slot content-keying
/// (`snapshots_bit_identical_v1`) compares tensor names/shapes/bits, so a
/// wide snapshot can never spuriously match a parked frozen one (or vice
/// versa) and the two widths safely share the single
/// `RESIDENT_DEVICE_STATE_V1` slot: a width switch is just a cache miss that
/// re-imports, never silent cross-contamination.
/// The `capture_named_gradients` selector exists only under `cfg(test)`
/// (authority #136). In a non-test build the parameter is absent from the
/// signature, the capture branch is absent from the compiled body, and the
/// result's `gradients` field stays the same empty vector it has always
/// been -- production carries no selector, template, branch, or extra
/// return.
fn train_step_cuda_burn_dense_inner_v1(
    snapshot: NativePolicyValueTrainSnapshotV1,
    wide: bool,
    groups: &[NativePolicyPhysicalDecisionV1<'_>],
    value_coefficient: f32,
    learning_rate: f32,
    #[cfg(test)] anchor_target_probability_rows: Option<&[&[f32]]>,
    #[cfg(test)] policy_anchor_coefficient: Option<f32>,
    #[cfg(test)] capture_named_gradients: bool,
) -> Result<
    (
        NativePolicyTrainStepResultV1,
        NativePolicyValueTrainSnapshotV1,
    ),
    NativePolicyTrainErrorV1,
> {
    if groups.is_empty() {
        return Err(NativePolicyTrainErrorV1::EmptyBatch);
    }
    #[cfg(test)]
    if anchor_target_probability_rows.is_some() != policy_anchor_coefficient.is_some() {
        return Err(NativePolicyTrainErrorV1::CudaBackend {
            code: "cuda-burn-dense-bridge-policy-anchor-contract",
        });
    }
    // The capture template is the input snapshot's OWN 33 named parameters.
    // The device import borrows the snapshot, so the capture branch reads
    // `snapshot.parameters` in place: no clone is performed for any
    // capture-false caller, and none at all.
    // Flatten every substep in order, retaining group structure.
    let mut views = Vec::new();
    let mut selected_action_indices = Vec::new();
    let mut substep_group_indices = Vec::new();
    let mut group_first_substeps = Vec::with_capacity(groups.len());
    let mut terminal_returns = Vec::with_capacity(groups.len());
    for (group_index, group) in groups.iter().enumerate() {
        if group.substeps.is_empty() {
            return Err(NativePolicyTrainErrorV1::EmptyPhysicalDecision { group_index });
        }
        if !matches!(group.terminal_return, -1..=1) {
            return Err(NativePolicyTrainErrorV1::InvalidTerminalReturn {
                group_index,
                value: group.terminal_return,
            });
        }
        group_first_substeps.push(views.len());
        terminal_returns.push(group.terminal_return);
        for substep in group.substeps.iter() {
            let encoded = match &substep.forward {
                NativePolicyForwardInputV1::Encoded(encoded) => **encoded,
                NativePolicyForwardInputV1::Packed { encoded, .. } => **encoded,
            };
            views.push(encoded);
            selected_action_indices.push(substep.selected_action_index);
            substep_group_indices.push(group_index);
        }
    }
    #[cfg(test)]
    if let Some(rows) = anchor_target_probability_rows {
        if rows.len() != views.len() || rows.iter().zip(&views).any(|(row, _)| row.is_empty()) {
            return Err(NativePolicyTrainErrorV1::CudaBackend {
                code: "cuda-burn-dense-bridge-policy-anchor-row-cardinality",
            });
        }
    }

    // Reuse the resident device state or import fresh, pack, plan, step.
    let parameter_before_bits =
        snapshot.parameters[SCORER_SECOND_BIAS_ORDINAL_V1].values[0].to_bits();
    // Diagnostic-only device ordinal override for multi-GPU economics probes
    // (process-wide; per-run placement uses one process per device). Absent or
    // unparsable means ordinal 0, the qualified default. Non-authorizing: any
    // evidence path must still capture and pin the actual device identity.
    let device_ordinal = std::env::var("MTG_KERNEL_PILOT_CUDA_ORDINAL")
        .ok()
        .and_then(|raw| raw.parse::<usize>().ok())
        .unwrap_or(0);
    let device = burn_cuda::CudaDevice::new(device_ordinal);
    // Take (not borrow) the resident entry for the whole update: every
    // failure path below leaves the slot empty, so a partially stepped
    // device state can never become eligible for reuse; the slot is refilled
    // only after the update commits host-side. On the reuse path the
    // incoming snapshot needs no fresh validation: it is bit-identical to
    // the parked exported snapshot, which `export_snapshot_v1` validated.
    let resident = resident_device_state_slot_v1().take();
    let mut device_state = match resident {
        Some(resident) if snapshots_bit_identical_v1(&resident.exported, &snapshot) => {
            #[cfg(test)]
            RESIDENT_REUSE_COUNT_V1.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            resident.device_state
        }
        stale => {
            // Free any stale resident tensors before importing the new set.
            drop(stale);
            #[cfg(test)]
            RESIDENT_IMPORT_COUNT_V1.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            if wide {
                ExperimentalDeviceTrainStateV1::import_snapshot_wide_v1(&snapshot, &device)
                    .map_err(bridge_error_v1)?
            } else {
                ExperimentalDeviceTrainStateV1::import_snapshot_v1(&snapshot, &device)
                    .map_err(bridge_error_v1)?
            }
        }
    };
    // The training step runs in group-aligned chunks with device-side
    // gradient accumulation and a single Adam application: peak activation
    // memory stays bounded by one chunk regardless of the update's substep
    // count, and each chunk's loss divides by the whole update's group count
    // so the accumulated gradient is the full-batch gradient. Each chunk
    // also reads back detached outputs from that same forward, removing the
    // former whole-update upload and duplicate forward.
    let mut chunk_group_starts = vec![0_usize];
    for (group_index, group_first) in group_first_substeps.iter().enumerate().skip(1) {
        let chunk_start_group = *chunk_group_starts.last().expect("nonempty starts");
        let chunk_first = group_first_substeps[chunk_start_group];
        if group_first - chunk_first >= BRIDGE_CHUNK_SUBSTEP_TARGET_V1 {
            chunk_group_starts.push(group_index);
        }
    }
    let mut accumulator = burn::optim::GradientsAccumulator::new();
    let mut raw_residual = 0.0_f32;
    let mut logit_outputs = Vec::new();
    let mut value_outputs = Vec::with_capacity(views.len());
    let mut global_action_offsets = Vec::with_capacity(views.len() + 1);
    global_action_offsets.push(0_usize);
    let total_group_count = groups.len() as f32;
    for (ordinal, chunk_start_group) in chunk_group_starts.iter().copied().enumerate() {
        let chunk_end_group = chunk_group_starts
            .get(ordinal + 1)
            .copied()
            .unwrap_or(groups.len());
        let substep_begin = group_first_substeps[chunk_start_group];
        let substep_end = group_first_substeps
            .get(chunk_end_group)
            .copied()
            .unwrap_or(views.len());
        let chunk_group_first_substeps = group_first_substeps[chunk_start_group..chunk_end_group]
            .iter()
            .map(|first| first - substep_begin)
            .collect::<Vec<_>>();
        let chunk_substep_group_indices = substep_group_indices[substep_begin..substep_end]
            .iter()
            .map(|group| group - chunk_start_group)
            .collect::<Vec<_>>();
        #[cfg(test)]
        let chunk_anchor_rows = anchor_target_probability_rows.map(|rows| {
            rows.get(substep_begin..substep_end)
                .ok_or(NativePolicyTrainErrorV1::CudaBackend {
                    code: "cuda-burn-dense-bridge-policy-anchor-chunk-slicing",
                })
        });
        #[cfg(test)]
        let chunk_anchor_rows = match chunk_anchor_rows {
            Some(Ok(rows)) => Some(rows),
            Some(Err(error)) => return Err(error),
            None => None,
        };
        let mut chunk_workspace = HostPackingWorkspace::default();
        chunk_workspace
            .pack_views(&views[substep_begin..substep_end])
            .map_err(bridge_error_v1)?;
        let chunk_plan: DenseGroupLossPlanV1 = build_dense_group_loss_plan_v1(
            &chunk_workspace,
            &selected_action_indices[substep_begin..substep_end],
            &chunk_substep_group_indices,
            &chunk_group_first_substeps,
            &terminal_returns[chunk_start_group..chunk_end_group],
            &device,
            #[cfg(test)]
            chunk_anchor_rows,
            #[cfg(test)]
            policy_anchor_coefficient,
        )
        .map_err(bridge_error_v1)?;
        let chunk_batch = DevicePackedBatch::upload(&device, &chunk_workspace);
        let chunk_outputs = device_state
            .chunk_backward_v1(
                &mut accumulator,
                &chunk_batch,
                &chunk_plan,
                value_coefficient,
                total_group_count,
            )
            .map_err(bridge_error_v1)?;
        let chunk_substep_count = substep_end - substep_begin;
        if chunk_workspace.action_offsets.len() != chunk_substep_count + 1 {
            return Err(NativePolicyTrainErrorV1::CudaBackend {
                code: "cuda-burn-dense-bridge-chunk-action-offset-cardinality",
            });
        }
        let expected_chunk_logits = chunk_workspace.action_offsets.last().copied().ok_or(
            NativePolicyTrainErrorV1::CudaBackend {
                code: "cuda-burn-dense-bridge-chunk-action-offset-empty",
            },
        )?;
        if chunk_outputs.logit_outputs.len() != expected_chunk_logits {
            return Err(NativePolicyTrainErrorV1::CudaBackend {
                code: "cuda-burn-dense-bridge-chunk-logit-cardinality",
            });
        }
        if chunk_outputs.value_outputs.len() != chunk_substep_count {
            return Err(NativePolicyTrainErrorV1::CudaBackend {
                code: "cuda-burn-dense-bridge-chunk-value-cardinality",
            });
        }
        let global_action_base = *global_action_offsets
            .last()
            .expect("global action offsets are initialized");
        for local_end in chunk_workspace.action_offsets.iter().copied().skip(1) {
            global_action_offsets.push(global_action_base.checked_add(local_end).ok_or(
                NativePolicyTrainErrorV1::CudaBackend {
                    code: "cuda-burn-dense-bridge-action-offset-overflow",
                },
            )?);
        }
        raw_residual += chunk_outputs.raw_gauge_residual;
        logit_outputs.extend(chunk_outputs.logit_outputs);
        value_outputs.extend(chunk_outputs.value_outputs);
    }
    if global_action_offsets.len() != views.len() + 1 {
        return Err(NativePolicyTrainErrorV1::CudaBackend {
            code: "cuda-burn-dense-bridge-action-offset-cardinality",
        });
    }
    if global_action_offsets.last().copied() != Some(logit_outputs.len()) {
        return Err(NativePolicyTrainErrorV1::CudaBackend {
            code: "cuda-burn-dense-bridge-logit-cardinality",
        });
    }
    if value_outputs.len() != views.len() {
        return Err(NativePolicyTrainErrorV1::CudaBackend {
            code: "cuda-burn-dense-bridge-value-cardinality",
        });
    }
    // The sole accumulated-gradient apply point.
    #[cfg(not(test))]
    device_state
        .apply_accumulated_v1(accumulator, learning_rate)
        .map_err(bridge_error_v1)?;
    #[cfg(test)]
    let captured_named_gradients: Vec<NativeNamedParameterV1> = if capture_named_gradients {
        let capture_template = &snapshot.parameters;
        // Fail closed on the literal frozen cardinality before the export.
        if capture_template.len() != 33 {
            return Err(NativePolicyTrainErrorV1::CudaBackend {
                code: "cuda-burn-dense-bridge-capture-cardinality",
            });
        }
        let captured = device_state
            .apply_accumulated_with_named_gradient_export_v1(
                accumulator,
                learning_rate,
                capture_template,
            )
            .map_err(bridge_error_v1)?;
        // Fail closed on exact count, names, order, shapes, and finite
        // values before anything downstream may read the stream.
        if captured.len() != 33 || captured.len() != capture_template.len() {
            return Err(NativePolicyTrainErrorV1::CudaBackend {
                code: "cuda-burn-dense-bridge-capture-cardinality",
            });
        }
        for (produced, expected) in captured.iter().zip(capture_template) {
            if produced.name != expected.name
                || produced.shape != expected.shape
                || produced.values.len() != expected.values.len()
                || produced.values.iter().any(|value| !value.is_finite())
            {
                return Err(NativePolicyTrainErrorV1::CudaBackend {
                    code: "cuda-burn-dense-bridge-capture-manifest",
                });
            }
        }
        captured
    } else {
        device_state
            .apply_accumulated_v1(accumulator, learning_rate)
            .map_err(bridge_error_v1)?;
        Vec::new()
    };

    // Tolerance-gate the CUDA outputs against the transported scorer bits,
    // then build every evidence field from the transported bits in the exact
    // CPU f32 fold order: the trainer revalidates evidence bit-exactly against
    // the rollout transport, so evidence stays CPU-canonical while the device
    // update itself uses the CUDA outputs.
    let mut selected_outputs = Vec::with_capacity(selected_action_indices.len());
    let mut physical_terms = Vec::with_capacity(groups.len());
    let mut transported_advantages = Vec::with_capacity(groups.len());
    let mut policy_sum = 0.0_f32;
    let mut value_sum = 0.0_f32;
    let group_count = groups.len() as f32;
    let mut flat_substep = 0_usize;
    #[cfg(test)]
    let measurement_mode = TOLERANCE_MEASUREMENT_MODE_V1.load(std::sync::atomic::Ordering::Relaxed);
    #[cfg(test)]
    let mut measurement_deferred_error: Option<&'static str> = None;
    #[cfg(test)]
    #[derive(Clone, serde::Serialize)]
    struct QualificationWorstRowV1 {
        group: usize,
        substep: usize,
        action_count: usize,
        min_delta_index: usize,
        max_delta_index: usize,
        min_delta: f64,
        max_delta: f64,
        min_endpoint_expected_bits: u32,
        min_endpoint_actual_bits: u32,
        max_endpoint_expected_bits: u32,
        max_endpoint_actual_bits: u32,
        expected_span: f64,
        expected_magnitude: f64,
        selected_index: usize,
    }
    #[cfg(test)]
    #[derive(Clone, serde::Serialize)]
    struct QualificationWorstValueV1 {
        group: usize,
        substep: usize,
        actual_bits: u32,
        expected_bits: u32,
        abs_error: f64,
    }
    #[cfg(test)]
    #[derive(Default)]
    struct QualificationRowStatsV1 {
        max_range: f64,
        worst: Option<QualificationWorstRowV1>,
        max_selected_logprob_delta: f64,
        max_joint_logprob_delta: f64,
        max_sum_abs_logprob_delta: f64,
        max_decision_d_sum: f64,
        decisions_over_b_cap: u64,
        rows_over_row_candidate: u64,
        rows_over_5e3: u64,
        rows_over_ln101: u64,
        max_value_abs_error: f64,
        worst_value: Option<QualificationWorstValueV1>,
        values_over_value_cap: u64,
    }
    #[cfg(test)]
    let mut qualification_stats = QualificationRowStatsV1::default();
    // Predeclared characterization thresholds (joint ruling): row candidate
    // 1.8e-3, per-decision B cap 1e-2, value cap 2e-3.
    #[cfg(test)]
    const MEASUREMENT_ROW_CANDIDATE_V1: f64 = 1.8e-3;
    #[cfg(test)]
    const MEASUREMENT_DECISION_B_CAP_V1: f64 = 1.0e-2;
    #[cfg(test)]
    const MEASUREMENT_VALUE_CAP_V1: f64 = 2.0e-3;
    #[cfg(test)]
    let measurement_scale_proxies = if measurement_mode {
        let norm = |name: &str| {
            snapshot
                .parameters
                .iter()
                .find(|parameter| parameter.name == name)
                .map(|parameter| {
                    parameter
                        .values
                        .iter()
                        .map(|v| f64::from(*v) * f64::from(*v))
                        .sum::<f64>()
                        .sqrt()
                })
                .unwrap_or_else(|| panic!("measurement mode requires snapshot parameter {name}"))
        };
        Some((norm("scorer.2.weight"), norm("value_head.2.weight")))
    } else {
        None
    };
    for (group_index, group) in groups.iter().enumerate() {
        let mut joint_log_probability: Option<f32> = None;
        #[cfg(test)]
        let mut measurement_joint_delta = (0.0_f64, 0.0_f64);
        #[cfg(test)]
        let mut measurement_decision_d_sum = 0.0_f64;
        value_outputs.get(group_first_substeps[group_index]).ok_or(
            NativePolicyTrainErrorV1::CudaBackend {
                code: "cuda-burn-dense-bridge-value-cardinality",
            },
        )?;
        let transported_first_value = f32::from_bits(group.substeps[0].expected_value_bits);
        let target = f32::from(group.terminal_return);
        let advantage = target - transported_first_value;
        for (substep_index, substep) in group.substeps.iter().enumerate() {
            let begin = global_action_offsets[flat_substep];
            let end = global_action_offsets[flat_substep + 1];
            let row = &logit_outputs[begin..end];
            if substep.selected_action_index >= row.len() {
                return Err(NativePolicyTrainErrorV1::SelectedActionOutOfRange {
                    group_index,
                    substep_index,
                    selected: substep.selected_action_index,
                    action_count: row.len(),
                });
            }
            if substep.expected_raw_action_logit_bits.len() != row.len() {
                return Err(NativePolicyTrainErrorV1::ExpectedLogitCountMismatch {
                    group_index,
                    substep_index,
                    expected: substep.expected_raw_action_logit_bits.len(),
                    actual: row.len(),
                });
            }
            if let Err(code) =
                validate_transported_logit_row_v2(row, substep.expected_raw_action_logit_bits)
            {
                eprintln!(
                    "cuda bridge transported-logit rejection: group={group_index} \
                     substep={substep_index} code={code}"
                );
                // Measurement mode bypasses ONLY the two tolerance codes
                // under joint review; structural and finiteness rejections
                // stay fail-closed in every mode.
                let bypassable = code == "cuda-burn-dense-bridge-transported-logit-range-tolerance"
                    || code == "cuda-burn-dense-bridge-transported-logit-shift-cap";
                #[cfg(test)]
                let fail_closed = !(measurement_mode && bypassable);
                #[cfg(not(test))]
                let fail_closed = {
                    let _ = bypassable;
                    true
                };
                if fail_closed {
                    return Err(NativePolicyTrainErrorV1::CudaBackend { code });
                }
            }
            #[cfg(test)]
            if measurement_mode {
                let mut min_delta = f64::INFINITY;
                let mut max_delta = f64::NEG_INFINITY;
                let mut min_delta_index = 0_usize;
                let mut max_delta_index = 0_usize;
                let mut min_expected = f64::INFINITY;
                let mut max_expected = f64::NEG_INFINITY;
                for (action_index, (actual, bits)) in row
                    .iter()
                    .zip(substep.expected_raw_action_logit_bits)
                    .enumerate()
                {
                    let expected = f64::from(f32::from_bits(*bits));
                    let delta = f64::from(*actual) - expected;
                    if delta < min_delta {
                        min_delta = delta;
                        min_delta_index = action_index;
                    }
                    if delta > max_delta {
                        max_delta = delta;
                        max_delta_index = action_index;
                    }
                    min_expected = min_expected.min(expected);
                    max_expected = max_expected.max(expected);
                }
                let range = max_delta - min_delta;
                measurement_decision_d_sum += range;
                // Option-initialized from the first observed row so a
                // perfect-parity update still emits a genuine identity.
                if qualification_stats.worst.is_none() || range > qualification_stats.max_range {
                    qualification_stats.max_range = range;
                    qualification_stats.worst = Some(QualificationWorstRowV1 {
                        group: group_index,
                        substep: substep_index,
                        action_count: row.len(),
                        min_delta_index,
                        max_delta_index,
                        min_delta,
                        max_delta,
                        min_endpoint_expected_bits: substep.expected_raw_action_logit_bits
                            [min_delta_index],
                        min_endpoint_actual_bits: row[min_delta_index].to_bits(),
                        max_endpoint_expected_bits: substep.expected_raw_action_logit_bits
                            [max_delta_index],
                        max_endpoint_actual_bits: row[max_delta_index].to_bits(),
                        expected_span: max_expected - min_expected,
                        expected_magnitude: max_expected.abs().max(min_expected.abs()),
                        selected_index: substep.selected_action_index,
                    });
                }
                if range > MEASUREMENT_ROW_CANDIDATE_V1 {
                    qualification_stats.rows_over_row_candidate += 1;
                }
                if range > 5.0e-3 {
                    qualification_stats.rows_over_5e3 += 1;
                }
                if range > 1.01_f64.ln() {
                    qualification_stats.rows_over_ln101 += 1;
                }
                let transported_row_f32: Vec<f32> = substep
                    .expected_raw_action_logit_bits
                    .iter()
                    .map(|bits| f32::from_bits(*bits))
                    .collect();
                let lp_delta =
                    log_softmax_f64_for_measurement_v1(row, substep.selected_action_index)
                        - log_softmax_f64_for_measurement_v1(
                            &transported_row_f32,
                            substep.selected_action_index,
                        );
                qualification_stats.max_selected_logprob_delta = qualification_stats
                    .max_selected_logprob_delta
                    .max(lp_delta.abs());
                measurement_joint_delta.0 += lp_delta;
                measurement_joint_delta.1 += lp_delta.abs();
            }
            let substep_value = value_outputs[flat_substep];
            if !substep_value.is_finite() {
                return Err(NativePolicyTrainErrorV1::CudaBackend {
                    code: "cuda-burn-dense-bridge-nonfinite-device-output",
                });
            }
            if !f32::from_bits(substep.expected_value_bits).is_finite() {
                return Err(NativePolicyTrainErrorV1::CudaBackend {
                    code: "cuda-burn-dense-bridge-nonfinite-transported-value",
                });
            }
            #[cfg(test)]
            if measurement_mode {
                let value_error = (f64::from(substep_value)
                    - f64::from(f32::from_bits(substep.expected_value_bits)))
                .abs();
                if qualification_stats.worst_value.is_none()
                    || value_error > qualification_stats.max_value_abs_error
                {
                    qualification_stats.max_value_abs_error = value_error;
                    qualification_stats.worst_value = Some(QualificationWorstValueV1 {
                        group: group_index,
                        substep: substep_index,
                        actual_bits: substep_value.to_bits(),
                        expected_bits: substep.expected_value_bits,
                        abs_error: value_error,
                    });
                }
                if value_error > MEASUREMENT_VALUE_CAP_V1 {
                    qualification_stats.values_over_value_cap += 1;
                }
            }
            if !tolerance_ok_v1(substep_value, f32::from_bits(substep.expected_value_bits)) {
                // In measurement mode the rejection is DEFERRED past record
                // emission so a value failure cannot lose the worst-identity
                // evidence; the error still returns fail-closed afterward.
                #[cfg(test)]
                let defer = measurement_mode;
                #[cfg(not(test))]
                let defer = false;
                if defer {
                    #[cfg(test)]
                    if measurement_deferred_error.is_none() {
                        measurement_deferred_error =
                            Some("cuda-burn-dense-bridge-transported-value-tolerance");
                    }
                } else {
                    return Err(NativePolicyTrainErrorV1::CudaBackend {
                        code: "cuda-burn-dense-bridge-transported-value-tolerance",
                    });
                }
            }
            let transported_row = substep
                .expected_raw_action_logit_bits
                .iter()
                .map(|bits| f32::from_bits(*bits))
                .collect::<Vec<f32>>();
            let (selected_log_probability, _log_probabilities) =
                selected_log_softmax(&transported_row, substep.selected_action_index)?;
            joint_log_probability = Some(match joint_log_probability {
                None => selected_log_probability,
                Some(active) => active + selected_log_probability,
            });
            selected_outputs.push(NativeSelectedOutputV1 {
                group_index,
                substep_index,
                selected_action_index: substep.selected_action_index,
                selected_logit: transported_row[substep.selected_action_index],
                value: f32::from_bits(substep.expected_value_bits),
                selected_log_probability,
            });
            flat_substep += 1;
        }
        #[cfg(test)]
        if measurement_mode {
            qualification_stats.max_joint_logprob_delta = qualification_stats
                .max_joint_logprob_delta
                .max(measurement_joint_delta.0.abs());
            qualification_stats.max_sum_abs_logprob_delta = qualification_stats
                .max_sum_abs_logprob_delta
                .max(measurement_joint_delta.1);
            qualification_stats.max_decision_d_sum = qualification_stats
                .max_decision_d_sum
                .max(measurement_decision_d_sum);
            if measurement_decision_d_sum > MEASUREMENT_DECISION_B_CAP_V1 {
                qualification_stats.decisions_over_b_cap += 1;
            }
        }
        transported_advantages.push(advantage);
        let joint_log_probability = joint_log_probability.expect("nonempty group checked above");
        let substep_count = u32::try_from(group.substeps.len()).map_err(|_| {
            NativePolicyTrainErrorV1::PhysicalSubstepCountOverflow {
                group_index,
                substep_count: group.substeps.len(),
            }
        })?;
        let policy_term = -joint_log_probability * advantage;
        let value_error = transported_first_value - target;
        let value_term = value_error * value_error;
        policy_sum += policy_term;
        value_sum += value_term;
        physical_terms.push(NativePhysicalLossTermV1 {
            joint_log_probability,
            value: transported_first_value,
            terminal_return: group.terminal_return,
            substep_count,
        });
    }
    // Store V2's frozen `loss` evidence is the terminal policy-plus-value
    // objective rederived from `physical_terms`. The experiment-only anchor
    // changes the differentiated CUDA objective above, but it is not a reward
    // term and has no field in that frozen evidence schema. Keep the persisted
    // scalar on its legacy formula while still validating the regularized
    // objective as finite here.
    let loss = (policy_sum + value_coefficient * value_sum) / group_count;
    #[cfg(test)]
    if let Some(coefficient) = policy_anchor_coefficient {
        let rows = anchor_target_probability_rows.ok_or(NativePolicyTrainErrorV1::CudaBackend {
            code: "cuda-burn-dense-bridge-policy-anchor-contract",
        })?;
        let anchor_kl = stable_forward_kl_sum_f64_v1(&logit_outputs, &global_action_offsets, rows)?;
        let objective = f64::from(policy_sum)
            + f64::from(value_coefficient) * f64::from(value_sum)
            + f64::from(coefficient) * anchor_kl;
        let objective = (objective / f64::from(group_count)) as f32;
        if !objective.is_finite() {
            return Err(NativePolicyTrainErrorV1::CudaBackend {
                code: "cuda-burn-dense-bridge-policy-anchor-objective-nonfinite",
            });
        }
    }
    #[cfg(test)]
    if measurement_mode {
        #[derive(serde::Serialize)]
        struct QualificationUpdateRecordV1 {
            adam_step_before: u64,
            snapshot_state_sha256: String,
            max_d: f64,
            worst_row: QualificationWorstRowV1,
            max_lp_delta: f64,
            max_joint_lp_delta: f64,
            max_sum_abs_lp_delta: f64,
            max_decision_d_sum: f64,
            decisions_over_b_cap: u64,
            rows_over_row_candidate: u64,
            rows_over_5e3: u64,
            rows_over_ln101: u64,
            max_value_abs_error: f64,
            worst_value: QualificationWorstValueV1,
            values_over_value_cap: u64,
            scorer2_weight_l2: f64,
            value2_weight_l2: f64,
        }
        // Evidence identity must be complete and valid: hash failure or a
        // missing worst record fails the probe rather than serializing a
        // sentinel.
        let (scorer_weight_norm, value_weight_norm) =
            measurement_scale_proxies.expect("measurement mode set");
        let snapshot_digest = if wide {
            wide_owned_train_snapshot_state_sha256_v1(&snapshot)
        } else {
            snapshot.state_sha256_v1()
        }
        .expect("measurement snapshot hash")
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
        let record = QualificationUpdateRecordV1 {
            adam_step_before: snapshot.adam_step,
            snapshot_state_sha256: snapshot_digest,
            max_d: qualification_stats.max_range,
            worst_row: qualification_stats
                .worst
                .clone()
                .expect("measurement update observed at least one row"),
            max_lp_delta: qualification_stats.max_selected_logprob_delta,
            max_joint_lp_delta: qualification_stats.max_joint_logprob_delta,
            max_sum_abs_lp_delta: qualification_stats.max_sum_abs_logprob_delta,
            max_decision_d_sum: qualification_stats.max_decision_d_sum,
            decisions_over_b_cap: qualification_stats.decisions_over_b_cap,
            rows_over_row_candidate: qualification_stats.rows_over_row_candidate,
            rows_over_5e3: qualification_stats.rows_over_5e3,
            rows_over_ln101: qualification_stats.rows_over_ln101,
            max_value_abs_error: qualification_stats.max_value_abs_error,
            worst_value: qualification_stats
                .worst_value
                .clone()
                .expect("measurement update observed at least one value"),
            values_over_value_cap: qualification_stats.values_over_value_cap,
            scorer2_weight_l2: scorer_weight_norm,
            value2_weight_l2: value_weight_norm,
        };
        eprintln!(
            "QUALIFICATION_JSONL {}",
            serde_json::to_string(&record).expect("measurement record serializes")
        );
    }
    // A rejection deferred for record completeness still fails closed.
    #[cfg(test)]
    if let Some(code) = measurement_deferred_error {
        return Err(NativePolicyTrainErrorV1::CudaBackend { code });
    }

    // The gauge accumulator observes substeps in the CPU backward traversal
    // order (groups reversed, substeps reversed within each group) with the
    // transported rows and coefficients: the store lattice validation binds
    // the recorded bounds to that order and rederives every coefficient
    // bit-exactly from the evidence terms. The device's raw scorer-bias
    // gradient stays the residual witness of the softmax gauge identity.
    let mut gauge_accumulator = ScorerBiasGaugeAccumulatorV1::default();
    for group_index in (0..groups.len()).rev() {
        let group = &groups[group_index];
        let coefficient = -transported_advantages[group_index] / group_count;
        for substep_index in (0..group.substeps.len()).rev() {
            let substep = &group.substeps[substep_index];
            let transported_row = substep
                .expected_raw_action_logit_bits
                .iter()
                .map(|bits| f32::from_bits(*bits))
                .collect::<Vec<f32>>();
            gauge_accumulator.observe(
                &transported_row,
                substep.selected_action_index,
                coefficient,
            )?;
        }
    }
    let scorer_bias_gauge = gauge_accumulator.finish(raw_residual, parameter_before_bits)?;

    // Export the device state. The CPU-side state replacement (through the
    // validating snapshot constructor for whichever width `state` is) happens
    // in the caller; this shared body only owns the device-side update.
    let updated_snapshot = device_state.export_snapshot_v1().map_err(bridge_error_v1)?;
    let adam_step = updated_snapshot.adam_step;
    // The update is committed device-side here: park the device state for the
    // next update, keyed by the exact snapshot its tensors now hold. The
    // caller still must commit the CPU-side state before this update is
    // fully durable; a failure there leaves the resident slot correctly
    // describing the device's actual (already-updated) tensors, which the
    // next call's bit-identity check will simply not match against a
    // not-yet-reimported CPU snapshot, falling back to a fresh (correct)
    // import rather than silently diverging.
    *resident_device_state_slot_v1() = Some(ResidentDeviceStateV1 {
        exported: updated_snapshot.clone(),
        device_state,
    });

    Ok((
        NativePolicyTrainStepResultV1 {
            policy_sum,
            value_sum,
            loss,
            adam_step,
            selected_outputs,
            physical_terms,
            #[cfg(not(test))]
            gradients: Vec::new(),
            #[cfg(test)]
            gradients: captured_named_gradients,
            scorer_bias_gauge,
        },
        updated_snapshot,
    ))
}

/// Run one production training update on the CudaBurnDense backend.
pub(crate) fn train_step_cuda_burn_dense_v1(
    state: &mut NativePolicyValueTrainStateV1,
    groups: &[NativePolicyPhysicalDecisionV1<'_>],
    value_coefficient: f32,
    learning_rate: f32,
) -> Result<NativePolicyTrainStepResultV1, NativePolicyTrainErrorV1> {
    let snapshot = state
        .snapshot_v1()
        .map_err(|_| NativePolicyTrainErrorV1::CudaBackend {
            code: "cuda-burn-dense-bridge-snapshot-failure",
        })?;
    let (result, updated_snapshot) = train_step_cuda_burn_dense_inner_v1(
        snapshot,
        false,
        groups,
        value_coefficient,
        learning_rate,
        #[cfg(test)]
        None,
        #[cfg(test)]
        None,
        #[cfg(test)]
        false,
    )?;
    *state = NativePolicyValueTrainStateV1::from_snapshot_v1(
        state.model_v1().clone(),
        &updated_snapshot,
    )
    .map_err(|_| NativePolicyTrainErrorV1::CudaBackend {
        code: "cuda-burn-dense-bridge-state-reimport-failure",
    })?;
    Ok(result)
}

/// Test-only forward-KL policy-anchor sibling of
/// [`train_step_cuda_burn_dense_v1`].  The parent rows are aligned as
/// `anchor_target_probabilities[group][substep][action]` with
/// `NativePolicyPhysicalDecisionV1`.  A literal positive f32 coefficient is
/// retained without quantization.  A literal positive zero routes directly
/// through the original wrapper so the beta-zero result remains bit-identical.
#[cfg(test)]
pub(crate) fn train_step_cuda_burn_dense_policy_anchor_v1(
    state: &mut NativePolicyValueTrainStateV1,
    groups: &[NativePolicyPhysicalDecisionV1<'_>],
    anchor_target_probabilities: &[Vec<Vec<f32>>],
    policy_anchor_coefficient: f32,
    value_coefficient: f32,
    learning_rate: f32,
) -> Result<NativePolicyTrainStepResultV1, NativePolicyTrainErrorV1> {
    if policy_anchor_coefficient.to_bits() == 0 {
        return train_step_cuda_burn_dense_v1(state, groups, value_coefficient, learning_rate);
    }
    let flat_anchor_rows = validate_policy_anchor_rows_v1(
        groups,
        anchor_target_probabilities,
        policy_anchor_coefficient,
    )?;
    let snapshot = state
        .snapshot_v1()
        .map_err(|_| NativePolicyTrainErrorV1::CudaBackend {
            code: "cuda-burn-dense-bridge-snapshot-failure",
        })?;
    let (result, updated_snapshot) = train_step_cuda_burn_dense_inner_v1(
        snapshot,
        false,
        groups,
        value_coefficient,
        learning_rate,
        Some(flat_anchor_rows.as_slice()),
        Some(policy_anchor_coefficient),
        false,
    )?;
    *state = NativePolicyValueTrainStateV1::from_snapshot_v1(
        state.model_v1().clone(),
        &updated_snapshot,
    )
    .map_err(|_| NativePolicyTrainErrorV1::CudaBackend {
        code: "cuda-burn-dense-bridge-state-reimport-failure",
    })?;
    Ok(result)
}

/// Diagnostic-only frozen-width sibling of
/// [`train_step_cuda_burn_dense_v1`] (authority #136).
///
/// Same inner update and same validated-snapshot commit as the frozen
/// wrapper above; the only difference is that it requests named-gradient
/// capture and requires all 33 tensors to come back. It exists only under
/// `cfg(test)`, so no production caller can reach it and the frozen wrapper
/// is byte-semantics-equivalent to its pre-#136 form.
#[cfg(test)]
pub(crate) fn train_step_cuda_burn_dense_capture_named_gradients_v1(
    state: &mut NativePolicyValueTrainStateV1,
    groups: &[NativePolicyPhysicalDecisionV1<'_>],
    value_coefficient: f32,
    learning_rate: f32,
) -> Result<NativePolicyTrainStepResultV1, NativePolicyTrainErrorV1> {
    let snapshot = state
        .snapshot_v1()
        .map_err(|_| NativePolicyTrainErrorV1::CudaBackend {
            code: "cuda-burn-dense-bridge-snapshot-failure",
        })?;
    let expected_tensor_count = snapshot.parameters.len();
    if expected_tensor_count != 33 {
        return Err(NativePolicyTrainErrorV1::CudaBackend {
            code: "cuda-burn-dense-bridge-capture-cardinality",
        });
    }
    let (result, updated_snapshot) = train_step_cuda_burn_dense_inner_v1(
        snapshot,
        false,
        groups,
        value_coefficient,
        learning_rate,
        #[cfg(test)]
        None,
        #[cfg(test)]
        None,
        true,
    )?;
    if result.gradients.len() != 33 || result.gradients.len() != expected_tensor_count {
        return Err(NativePolicyTrainErrorV1::CudaBackend {
            code: "cuda-burn-dense-bridge-capture-cardinality",
        });
    }
    *state = NativePolicyValueTrainStateV1::from_snapshot_v1(
        state.model_v1().clone(),
        &updated_snapshot,
    )
    .map_err(|_| NativePolicyTrainErrorV1::CudaBackend {
        code: "cuda-burn-dense-bridge-state-reimport-failure",
    })?;
    Ok(result)
}

/// Capacity-experiment wide-net (kernel-policy-value-net-8w128) sibling of
/// [`train_step_cuda_burn_dense_v1`]. Identical update body
/// (`train_step_cuda_burn_dense_inner_v1`, `wide = true`); only the CPU-side
/// state type, and hence the model construction/export dims used to commit
/// the update back to CPU-owned state, differs. The frozen function above is
/// untouched.
///
/// Wired into `native_trainer_v1`'s `NativeTrainerStateV2` dispatch via
/// `train_grouped_candidate_wide_v1` (the wide sibling of the self-play
/// rollout/training pipeline that closes the wall this module's original
/// doc comment named): a genuine self-play pilot run with `MULTIRUN_WIDE=1`
/// now reaches this function on every update.
pub(crate) fn train_step_cuda_burn_dense_wide_v1(
    state: &mut NativePolicyValueTrainStateWideV1,
    groups: &[NativePolicyPhysicalDecisionV1<'_>],
    value_coefficient: f32,
    learning_rate: f32,
) -> Result<NativePolicyTrainStepResultV1, NativePolicyTrainErrorV1> {
    let snapshot = state
        .snapshot_v1()
        .map_err(|_| NativePolicyTrainErrorV1::CudaBackend {
            code: "cuda-burn-dense-bridge-snapshot-failure",
        })?;
    let (result, updated_snapshot) = train_step_cuda_burn_dense_inner_v1(
        snapshot,
        true,
        groups,
        value_coefficient,
        learning_rate,
        #[cfg(test)]
        None,
        #[cfg(test)]
        None,
        #[cfg(test)]
        false,
    )?;
    *state = NativePolicyValueTrainStateWideV1::from_snapshot_wide_v1(
        state.model_v1().clone(),
        &updated_snapshot,
    )
    .map_err(|_| NativePolicyTrainErrorV1::CudaBackend {
        code: "cuda-burn-dense-bridge-state-reimport-failure",
    })?;
    Ok(result)
}
