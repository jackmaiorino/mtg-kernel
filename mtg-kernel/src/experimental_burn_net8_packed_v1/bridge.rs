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
    NativePolicyValueTrainSnapshotV1, NativePolicyValueTrainStateV1, NativeSelectedOutputV1,
    ScorerBiasGaugeAccumulatorV1,
};
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

fn bridge_error_v1(_error: Box<dyn Error>) -> NativePolicyTrainErrorV1 {
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

fn tolerance_ok_v1(actual: f32, expected: f32) -> bool {
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

/// Run one production training update on the CudaBurnDense backend.
pub(crate) fn train_step_cuda_burn_dense_v1(
    state: &mut NativePolicyValueTrainStateV1,
    groups: &[NativePolicyPhysicalDecisionV1<'_>],
    value_coefficient: f32,
    learning_rate: f32,
) -> Result<NativePolicyTrainStepResultV1, NativePolicyTrainErrorV1> {
    if groups.is_empty() {
        return Err(NativePolicyTrainErrorV1::EmptyBatch);
    }
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

    // Snapshot, then reuse the resident device state or import fresh, pack,
    // plan, step.
    let snapshot = state
        .snapshot_v1()
        .map_err(|_| NativePolicyTrainErrorV1::CudaBackend {
            code: "cuda-burn-dense-bridge-snapshot-failure",
        })?;
    let parameter_before_bits =
        snapshot.parameters[SCORER_SECOND_BIAS_ORDINAL_V1].values[0].to_bits();
    let device = burn_cuda::CudaDevice::new(0);
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
            ExperimentalDeviceTrainStateV1::import_snapshot_v1(&snapshot, &device)
                .map_err(bridge_error_v1)?
        }
    };
    let mut workspace = HostPackingWorkspace::default();
    workspace.pack_views(&views).map_err(bridge_error_v1)?;
    let batch = DevicePackedBatch::upload(&device, &workspace);
    let (logit_outputs, value_outputs) = device_state
        .forward_outputs_v1(&batch)
        .map_err(bridge_error_v1)?;
    drop(batch);

    // The training step runs in group-aligned chunks with device-side
    // gradient accumulation and a single Adam application: peak activation
    // memory stays bounded by one chunk regardless of the update's substep
    // count, and each chunk's loss divides by the whole update's group count
    // so the accumulated gradient is the full-batch gradient.
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
        )
        .map_err(bridge_error_v1)?;
        let chunk_batch = DevicePackedBatch::upload(&device, &chunk_workspace);
        raw_residual += device_state
            .chunk_backward_v1(
                &mut accumulator,
                &chunk_batch,
                &chunk_plan,
                value_coefficient,
                total_group_count,
            )
            .map_err(bridge_error_v1)?;
    }
    device_state
        .apply_accumulated_v1(accumulator, learning_rate)
        .map_err(bridge_error_v1)?;

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
    #[derive(Default)]
    struct QualificationWorstRowV1 {
        group: usize,
        substep: usize,
        action_count: usize,
        min_delta_index: usize,
        max_delta_index: usize,
        min_delta: f64,
        max_delta: f64,
        expected_span: f64,
        expected_magnitude: f64,
        selected_index: usize,
    }
    #[cfg(test)]
    #[derive(Default)]
    struct QualificationRowStatsV1 {
        max_range: f64,
        worst: QualificationWorstRowV1,
        max_selected_logprob_delta: f64,
        max_joint_logprob_delta: f64,
        max_sum_abs_logprob_delta: f64,
        rows_over_5e3: u64,
        rows_over_ln101: u64,
    }
    #[cfg(test)]
    let mut qualification_stats = QualificationRowStatsV1::default();
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
                .unwrap_or(f64::NAN)
        };
        Some((norm("scorer.2.weight"), norm("value_head.2.weight")))
    } else {
        None
    };
    for (group_index, group) in groups.iter().enumerate() {
        let mut joint_log_probability: Option<f32> = None;
        #[cfg(test)]
        let mut measurement_joint_delta = (0.0_f64, 0.0_f64);
        value_outputs.get(group_first_substeps[group_index]).ok_or(
            NativePolicyTrainErrorV1::CudaBackend {
                code: "cuda-burn-dense-bridge-value-cardinality",
            },
        )?;
        let transported_first_value = f32::from_bits(group.substeps[0].expected_value_bits);
        let target = f32::from(group.terminal_return);
        let advantage = target - transported_first_value;
        for (substep_index, substep) in group.substeps.iter().enumerate() {
            let begin = workspace.action_offsets[flat_substep];
            let end = workspace.action_offsets[flat_substep + 1];
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
                if range > qualification_stats.max_range {
                    qualification_stats.max_range = range;
                    qualification_stats.worst = QualificationWorstRowV1 {
                        group: group_index,
                        substep: substep_index,
                        action_count: row.len(),
                        min_delta_index,
                        max_delta_index,
                        min_delta,
                        max_delta,
                        expected_span: max_expected - min_expected,
                        expected_magnitude: max_expected.abs().max(min_expected.abs()),
                        selected_index: substep.selected_action_index,
                    };
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
            if !tolerance_ok_v1(substep_value, f32::from_bits(substep.expected_value_bits)) {
                return Err(NativePolicyTrainErrorV1::CudaBackend {
                    code: "cuda-burn-dense-bridge-transported-value-tolerance",
                });
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
    let loss = (policy_sum + value_coefficient * value_sum) / group_count;
    #[cfg(test)]
    if measurement_mode {
        let worst = &qualification_stats.worst;
        let (scorer_weight_norm, value_weight_norm) =
            measurement_scale_proxies.unwrap_or((f64::NAN, f64::NAN));
        let snapshot_digest = snapshot
            .state_sha256_v1()
            .map(|digest| {
                digest[..8]
                    .iter()
                    .map(|byte| format!("{byte:02x}"))
                    .collect::<String>()
            })
            .unwrap_or_else(|_| "invalid".to_owned());
        eprintln!(
            "QUALIFICATION_JSONL {{\"adam_step_before\":{},\"snapshot_sha_prefix\":\"{}\",\
             \"max_D\":{:e},\"worst\":{{\"group\":{},\"substep\":{},\"action_count\":{},\
             \"min_delta_index\":{},\"max_delta_index\":{},\"min_delta\":{:e},\
             \"max_delta\":{:e},\"expected_span\":{:e},\"expected_magnitude\":{:e},\
             \"selected_index\":{}}},\"max_lp_delta\":{:e},\"max_joint_lp_delta\":{:e},\
             \"max_sum_abs_lp_delta\":{:e},\"rows_over_5e3\":{},\"rows_over_ln101\":{},\
             \"scorer2_weight_l2\":{:e},\"value2_weight_l2\":{:e}}}",
            snapshot.adam_step,
            snapshot_digest,
            qualification_stats.max_range,
            worst.group,
            worst.substep,
            worst.action_count,
            worst.min_delta_index,
            worst.max_delta_index,
            worst.min_delta,
            worst.max_delta,
            worst.expected_span,
            worst.expected_magnitude,
            worst.selected_index,
            qualification_stats.max_selected_logprob_delta,
            qualification_stats.max_joint_logprob_delta,
            qualification_stats.max_sum_abs_logprob_delta,
            qualification_stats.rows_over_5e3,
            qualification_stats.rows_over_ln101,
            scorer_weight_norm,
            value_weight_norm,
        );
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

    // Export the device state and replace the CPU state through the
    // validating snapshot constructor.
    let updated_snapshot = device_state.export_snapshot_v1().map_err(bridge_error_v1)?;
    let adam_step = updated_snapshot.adam_step;
    *state = NativePolicyValueTrainStateV1::from_snapshot_v1(
        state.model_v1().clone(),
        &updated_snapshot,
    )
    .map_err(|_| NativePolicyTrainErrorV1::CudaBackend {
        code: "cuda-burn-dense-bridge-state-reimport-failure",
    })?;
    // The update is committed host-side: park the device state for the next
    // update, keyed by the exact snapshot its tensors now hold.
    *resident_device_state_slot_v1() = Some(ResidentDeviceStateV1 {
        exported: updated_snapshot,
        device_state,
    });

    Ok(NativePolicyTrainStepResultV1 {
        policy_sum,
        value_sum,
        loss,
        adam_step,
        selected_outputs,
        physical_terms,
        gradients: Vec::new(),
        scorer_bias_gauge,
    })
}
