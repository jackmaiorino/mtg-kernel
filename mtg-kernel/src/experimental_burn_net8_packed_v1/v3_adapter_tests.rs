//! Host-only adapter checks and an explicitly ignored device arithmetic test.

use super::*;
use crate::native_flat_tensorizer_v3::{encoded_decision_view_v3, NativeFlatDecisionTensorV3};
use crate::native_policy_train_step_v1::{
    NativePolicyForwardInputV1, NativePolicyPhysicalDecisionV1, NativePolicySubstepV1,
    NativePolicyValueTrainSnapshotV1,
};

fn tensor_fixture(empty_edges: bool, empty_refs: bool) -> NativeFlatDecisionTensorV3 {
    let mut tensor = NativeFlatDecisionTensorV3::default();
    let t = &mut tensor.common;
    t.state = vec![0.125; STATE_DIM_V1];
    t.object_features = vec![0.25; 2 * OBJECT_FEATURE_DIM_V1];
    t.object_card_ids = vec![1, 2];
    t.object_groups = vec![0, 1];
    t.object_node_ids = vec![0, 1];
    t.action_features = vec![0.375; 2 * ACTION_FEATURE_DIM_V1];
    if !empty_edges {
        t.edge_features = vec![0.5; EDGE_FEATURE_DIM_V1];
        t.edge_source_indices = vec![0];
        t.edge_target_indices = vec![1];
    }
    if !empty_refs {
        t.action_ref_features = vec![0.625; ACTION_REF_FEATURE_DIM_V1];
        t.action_ref_card_ids = vec![1];
        t.action_ref_action_indices = vec![0];
        t.action_ref_node_indices = vec![0];
    }
    tensor
}

fn state_fixture() -> NativePolicyValueTrainStateV1 {
    NativePolicyValueTrainStateV1::new_v1(
        NativePolicyValueNetV1::runner_fixed_v1(NativePolicyValueModelConfigV1::contract_v1())
            .unwrap(),
    )
    .unwrap()
}

#[test]
fn v3_cuda_resident_reuse_requires_device_and_all_snapshot_bits() {
    let snapshot = state_fixture().snapshot_v1().unwrap();
    assert!(bridge::resident_snapshot_matches_v1(
        1, 1, &snapshot, &snapshot
    ));
    assert!(!bridge::resident_snapshot_matches_v1(
        0, 1, &snapshot, &snapshot
    ));
    let mut changed = snapshot.clone();
    changed.first_moments[1].values[0] = 0.25;
    assert!(!bridge::resident_snapshot_matches_v1(
        1, 1, &snapshot, &changed
    ));
    changed = snapshot.clone();
    changed.parameters[1].values[0] = -0.0;
    let mut positive_zero = changed.clone();
    positive_zero.parameters[1].values[0] = 0.0;
    assert!(!bridge::resident_snapshot_matches_v1(
        1,
        1,
        &positive_zero,
        &changed
    ));
}

#[test]
fn v3_cuda_direct_bridge_rejects_before_device_creation() {
    let mut state = state_fixture();
    let tensor = tensor_fixture(true, true);
    let output = state
        .model_v1()
        .forward_feature_transfer_v3(encoded_decision_view_v3(&tensor))
        .unwrap();
    let bits: Vec<_> = output.logits.iter().map(|v| v.to_bits()).collect();
    let before = state.state_sha256_v1().unwrap();
    for invalid_schema in [false, true] {
        let mut view = encoded_decision_view_v3(&tensor);
        if invalid_schema {
            view.schema = NativeEncodedDecisionSchemaV1::contract_v1();
        }
        let steps = [NativePolicySubstepV1 {
            forward: NativePolicyForwardInputV1::Encoded(Box::new(view)),
            selected_action_index: 0,
            expected_raw_action_logit_bits: &bits,
            expected_value_bits: output.value.to_bits(),
        }];
        let groups = [NativePolicyPhysicalDecisionV1 {
            substeps: &steps,
            terminal_return: 1,
            baseline_bits: 0,
        }];
        // A valid ordinal with a rejected schema, or a valid schema with an
        // unrepresentable ordinal. Neither call can reach CudaDevice::new.
        let ordinal = if invalid_schema { 1 } else { usize::MAX };
        assert!(bridge::train_step_cuda_burn_dense_feature_transfer_v3(
            &mut state, &groups, 0.5, 0.0001, ordinal,
        )
        .is_err());
        assert_eq!(state.state_sha256_v1().unwrap(), before);
        assert!(
            bridge::train_step_cuda_burn_dense_feature_transfer_capture_numerics_v3(
                &mut state, &groups, 0.5, 0.0001, ordinal,
            )
            .is_err()
        );
        assert_eq!(state.state_sha256_v1().unwrap(), before);
    }
}

fn numerical_evidence_fixture() -> bridge::V3CudaNumericalEvidenceV1 {
    bridge::V3CudaNumericalEvidenceV1 {
        device_ordinal: 1,
        normalization_group_count: 2,
        value_coefficient: 0.5,
        logit_outputs: vec![0.5, -0.5, 0.25, 1.0, 0.0, -1.0],
        value_outputs: vec![0.25, -0.5, 0.125],
        action_offsets: vec![0, 2, 3, 6],
        selected_action_indices: vec![1, 0, 2],
        substep_group_indices: vec![0, 0, 1],
        group_first_substeps: vec![0, 2],
        terminal_returns: vec![1, -1],
        chunks: vec![
            bridge::V3CudaChunkObjectiveV1 {
                group_begin: 0,
                group_end: 1,
                substep_begin: 0,
                substep_end: 2,
                device_objective: 0.5,
            },
            bridge::V3CudaChunkObjectiveV1 {
                group_begin: 1,
                group_end: 2,
                substep_begin: 2,
                substep_end: 3,
                device_objective: -0.25,
            },
        ],
        device_objective_host_sum: 0.25,
    }
}

#[test]
fn v3_cuda_numerical_capture_validates_partitions_and_finite_device_data() {
    let evidence = numerical_evidence_fixture();
    bridge::validate_v3_cuda_numerical_evidence_v1(&evidence).unwrap();
    let corruptions: &[fn(&mut bridge::V3CudaNumericalEvidenceV1)] = &[
        |e| e.device_ordinal = usize::MAX,
        |e| e.normalization_group_count = 0,
        |e| e.value_coefficient = f32::NAN,
        |e| e.logit_outputs[0] = f32::INFINITY,
        |e| e.value_outputs[1] = f32::NAN,
        |e| e.action_offsets[0] = 1,
        |e| e.action_offsets[2] = 2,
        |e| e.action_offsets[3] = 7,
        |e| e.selected_action_indices[1] = 1,
        |e| e.substep_group_indices[1] = 1,
        |e| e.group_first_substeps[1] = 1,
        |e| e.group_first_substeps[1] = usize::MAX,
        |e| e.terminal_returns[0] = 2,
        |e| e.chunks.clear(),
        |e| e.chunks[1].group_begin = 0,
        |e| e.chunks[1].group_end = 3,
        |e| e.chunks[0].substep_end = 1,
        |e| e.chunks[1].substep_begin = 1,
        |e| e.chunks[1].substep_end = 2,
        |e| e.chunks[0].device_objective = f32::NAN,
        |e| {
            e.chunks.pop();
        },
        |e| e.device_objective_host_sum = f64::INFINITY,
        |e| e.device_objective_host_sum = 0.0,
    ];
    for (case, corrupt) in corruptions.iter().enumerate() {
        let mut changed = evidence.clone();
        corrupt(&mut changed);
        assert!(
            bridge::validate_v3_cuda_numerical_evidence_v1(&changed).is_err(),
            "corruption {case}"
        );
    }
}

#[test]
fn v3_cuda_numerical_capture_preserves_actual_chunk_scalars_in_host_sum() {
    let mut evidence = numerical_evidence_fixture();
    evidence.chunks[0].device_objective = 100_000_000.0;
    evidence.chunks[1].device_objective = 1.0;
    evidence.device_objective_host_sum = 100_000_001.0;
    bridge::validate_v3_cuda_numerical_evidence_v1(&evidence).unwrap();
    // A separately rounded f32 sum is not the declared f64 host sum.
    evidence.device_objective_host_sum =
        f64::from(evidence.chunks[0].device_objective + evidence.chunks[1].device_objective);
    assert!(bridge::validate_v3_cuda_numerical_evidence_v1(&evidence).is_err());
}

fn assert_named_close(
    cpu: &[NativeNamedParameterV1],
    gpu: &[NativeNamedParameterV1],
    tolerance: f32,
) {
    assert_eq!(cpu.len(), gpu.len());
    for (expected, actual) in cpu.iter().zip(gpu) {
        assert_eq!(expected.name, actual.name);
        assert_eq!(expected.shape, actual.shape);
        assert_eq!(expected.values.len(), actual.values.len());
        for (index, (&left, &right)) in expected.values.iter().zip(&actual.values).enumerate() {
            assert!(left.is_finite() && right.is_finite());
            assert!(
                (left - right).abs() <= tolerance.max(tolerance * left.abs()),
                "{}[{index}]: CPU {left}, CUDA {right}",
                expected.name
            );
        }
    }
}

#[test]
#[ignore = "requires separately authorized GPU execution and MTG_KERNEL_V3_CUDA_TEST_ORDINAL; not an automatic or full qualification test"]
fn v3_cuda_empty_relation_adam_continuation_requires_explicit_device() {
    let ordinal: usize = std::env::var("MTG_KERNEL_V3_CUDA_TEST_ORDINAL")
        .expect("explicit authorized test device required")
        .parse()
        .unwrap();
    assert!(i32::try_from(ordinal).is_ok());
    for (empty_edges, empty_refs) in [(false, false), (true, false), (false, true), (true, true)] {
        let initial = state_fixture();
        let mut snapshot: NativePolicyValueTrainSnapshotV1 = initial.snapshot_v1().unwrap();
        snapshot.adam_step = 7;
        for moments in &mut snapshot.first_moments {
            if moments.name.starts_with("edge_encoder.")
                || moments.name.starts_with("action_ref_encoder.")
            {
                moments.values.fill(0.03125);
            }
        }
        for moments in &mut snapshot.second_moments {
            if moments.name.starts_with("edge_encoder.")
                || moments.name.starts_with("action_ref_encoder.")
            {
                moments.values.fill(0.0625);
            }
        }
        let mut cpu =
            NativePolicyValueTrainStateV1::from_snapshot_v1(initial.model_v1().clone(), &snapshot)
                .unwrap();
        let mut gpu =
            NativePolicyValueTrainStateV1::from_snapshot_v1(initial.model_v1().clone(), &snapshot)
                .unwrap();
        let mut tensors = [
            tensor_fixture(empty_edges, empty_refs),
            tensor_fixture(empty_edges, empty_refs),
            tensor_fixture(empty_edges, empty_refs),
        ];
        tensors[0].common.action_features[0] = -0.125;
        tensors[1].common.state[0] = -0.25;
        tensors[2]
            .common
            .action_features
            .truncate(ACTION_FEATURE_DIM_V1);
        let outputs: Vec<_> = tensors
            .iter()
            .map(|tensor| {
                cpu.model_v1()
                    .forward_feature_transfer_v3(encoded_decision_view_v3(tensor))
                    .unwrap()
            })
            .collect();
        let bits: Vec<Vec<_>> = outputs
            .iter()
            .map(|output| output.logits.iter().map(|v| v.to_bits()).collect())
            .collect();
        let selected = [1, 0, 0];
        let steps: Vec<_> = tensors
            .iter()
            .enumerate()
            .map(|(index, tensor)| NativePolicySubstepV1 {
                forward: NativePolicyForwardInputV1::Encoded(Box::new(encoded_decision_view_v3(
                    tensor,
                ))),
                selected_action_index: selected[index],
                expected_raw_action_logit_bits: &bits[index],
                expected_value_bits: outputs[index].value.to_bits(),
            })
            .collect();
        let groups = [
            NativePolicyPhysicalDecisionV1 {
                substeps: &steps[..2],
                terminal_return: -1,
                baseline_bits: 0,
            },
            NativePolicyPhysicalDecisionV1 {
                substeps: &steps[2..],
                terminal_return: 1,
                baseline_bits: 0,
            },
        ];
        let cpu_result = cpu
            .train_step_feature_transfer_v3(&groups, 0.5, 0.0001)
            .unwrap();
        let captured = bridge::train_step_cuda_burn_dense_feature_transfer_capture_numerics_v3(
            &mut gpu, &groups, 0.5, 0.0001, ordinal,
        )
        .unwrap();
        // These are the existing diagnostic tolerances, not a V3 campaign
        // acceptance envelope. Full qualification still needs declared
        // per-layer delta bounds and real multi-deck trajectory rows.
        let gpu_result = captured.result;
        let device = captured.device;
        bridge::validate_v3_cuda_numerical_evidence_v1(&device).unwrap();
        assert_eq!(device.device_ordinal, ordinal);
        assert_eq!(device.normalization_group_count, 2);
        assert_eq!(device.action_offsets, [0, 2, 4, 5]);
        assert_eq!(device.selected_action_indices, selected);
        assert_eq!(device.substep_group_indices, [0, 0, 1]);
        assert_eq!(device.group_first_substeps, [0, 2]);
        assert_eq!(device.terminal_returns, [-1, 1]);
        assert_eq!(device.chunks.len(), 1);
        assert_eq!(device.chunks[0].group_begin, 0);
        assert_eq!(device.chunks[0].group_end, 2);
        assert_eq!(device.chunks[0].substep_begin, 0);
        assert_eq!(device.chunks[0].substep_end, 3);
        let cpu_logits = outputs.iter().flat_map(|output| &output.logits);
        for (expected, actual) in cpu_logits.zip(&device.logit_outputs) {
            assert!((expected - actual).abs() <= 0.005_f32.max(0.005 * expected.abs()));
        }
        for (expected, actual) in outputs
            .iter()
            .map(|output| output.value)
            .zip(&device.value_outputs)
        {
            assert!((expected - actual).abs() <= 0.005_f32.max(0.005 * expected.abs()));
        }
        assert!(
            (f64::from(cpu_result.loss) - device.device_objective_host_sum).abs()
                <= 0.005_f64.max(0.005 * f64::from(cpu_result.loss).abs())
        );
        assert_named_close(&cpu_result.gradients, &gpu_result.gradients, 0.005);
        let cpu_after = cpu.snapshot_v1().unwrap();
        let gpu_after = gpu.snapshot_v1().unwrap();
        assert_eq!(cpu_after.adam_step, 8);
        assert_eq!(gpu_after.adam_step, 8);
        assert_named_close(&cpu_after.parameters, &gpu_after.parameters, 0.002);
        assert_named_close(&cpu_after.first_moments, &gpu_after.first_moments, 0.002);
        assert_named_close(&cpu_after.second_moments, &gpu_after.second_moments, 0.002);
        for (index, gradient) in gpu_result.gradients.iter().enumerate() {
            let skipped = (empty_edges && gradient.name.starts_with("edge_encoder."))
                || (empty_refs && gradient.name.starts_with("action_ref_encoder."));
            if skipped {
                assert!(
                    gradient.values.iter().all(|v| v.to_bits() == 0),
                    "{}",
                    gradient.name
                );
                assert!(gpu_after.first_moments[index]
                    .values
                    .iter()
                    .all(|v| *v < 0.03125));
                assert!(gpu_after.second_moments[index]
                    .values
                    .iter()
                    .all(|v| *v < 0.0625));
                assert_ne!(
                    gpu_after.parameters[index].values, snapshot.parameters[index].values,
                    "unused layer must still apply its existing Adam moments"
                );
            }
        }
        gpu.validate_state_v1().unwrap();
    }
}
