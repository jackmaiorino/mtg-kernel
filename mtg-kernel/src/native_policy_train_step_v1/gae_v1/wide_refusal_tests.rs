//! The single wide refusal list (collab
//! LANES/throughput-architecture-20261020/WIDE-BACKWARD-DESIGN.md v2.1,
//! sections 2.1 and 5.5): every narrow-only learner path refuses net-8w128
//! with `UnsupportedForArchitecture` before any forward work, and the state
//! is unchanged.

use super::*;
use crate::line_b_teacher_target_v1::LineBDivergenceV1;
use crate::native_flat_tensorizer_v3::{encoded_decision_view_v3, NativeFlatDecisionTensorV3};
use crate::native_policy_train_step_v1::tests::real_map_training_tensor_v3;
use crate::sideboard_play_policy_v1::FreshLineageGenerationV1;

const VC: f32 = 0.75;
const LR: f32 = 0.00003;

fn wide_state_v1() -> NativePolicyValueTrainStateWideV1 {
    NativePolicyValueTrainStateWideV1::new_wide_v1(
        NativePolicyValueNetWideV1::runner_fixed_wide_v1(
            NativePolicyValueModelConfigV1::contract_wide_v1(),
        )
        .unwrap(),
    )
    .unwrap()
}

/// The real Map decision under the V3 contract, with the wide model's own
/// recorded outputs.
struct WideFixtureV1 {
    tensor: NativeFlatDecisionTensorV3,
    logits: Vec<u32>,
    value: u32,
}

impl WideFixtureV1 {
    fn of_v1(state: &NativePolicyValueTrainStateWideV1) -> Self {
        let tensor = real_map_training_tensor_v3();
        let output = state
            .model_v1()
            .forward_feature_transfer_v3_wide_v1(encoded_decision_view_v3(&tensor))
            .unwrap();
        Self {
            logits: output.logits.iter().map(|value| value.to_bits()).collect(),
            value: output.value.to_bits(),
            tensor,
        }
    }

    fn substeps_v1(&self) -> Vec<NativePolicySubstepV1<'_>> {
        vec![NativePolicySubstepV1 {
            forward: NativePolicyForwardInputV1::Encoded(Box::new(encoded_decision_view_v3(
                &self.tensor,
            ))),
            selected_action_index: 0,
            expected_raw_action_logit_bits: &self.logits,
            expected_value_bits: self.value,
        }]
    }
}

fn group_v1<'a>(
    substeps: &'a [NativePolicySubstepV1<'a>],
) -> [NativePolicyPhysicalDecisionV1<'a>; 1] {
    [NativePolicyPhysicalDecisionV1 {
        substeps,
        terminal_return: 1,
        baseline_bits: 0,
    }]
}

/// Runs `step` on `state` and requires the refusal of `path`, with no
/// forward run and no state change.
fn assert_refused_v1(
    mut state: NativePolicyValueTrainStateWideV1,
    path: &'static str,
    step: impl FnOnce(&mut NativePolicyValueTrainStateWideV1) -> Result<(), NativePolicyTrainErrorV1>,
) {
    let before = (
        state.state_sha256_v1().unwrap(),
        state.adam_step_v1(),
        state.training_math_dims_v1(),
    );
    let forwards = forward_with_tape_call_count_for_test_v1();
    assert_eq!(
        step(&mut state),
        Err(NativePolicyTrainErrorV1::UnsupportedForArchitecture {
            architecture: crate::native_policy_value_net_v1::W_MODEL_ARCHITECTURE_VERSION_V1,
            path,
        })
    );
    assert_eq!(
        forward_with_tape_call_count_for_test_v1(),
        forwards,
        "{path}"
    );
    assert_eq!(
        (
            state.state_sha256_v1().unwrap(),
            state.adam_step_v1(),
            state.training_math_dims_v1(),
        ),
        before,
        "{path}"
    );
}

/// The ordinary GAE core with one optional narrow-only term.
fn core_step_v1(
    state: &mut NativePolicyValueTrainStateWideV1,
    groups: &[NativePolicyPhysicalDecisionV1<'_>],
    line_b: Option<AuxiliaryTermsSourceV1<'_>>,
    exploration: Option<(f32, u64, bool)>,
) -> Result<(), NativePolicyTrainErrorV1> {
    let config = state.model_v1().feature_transfer_config_v3_wide_v1();
    state
        .train_step_gae_core_dims_v1(
            groups,
            &[0.3],
            &[0.15],
            VC,
            LR,
            BackwardExecutionV1::Sequential,
            config,
            false,
            line_b,
            exploration,
        )
        .map(|_| ())
}

#[test]
fn wide_refuses_in_batch_line_b() {
    let state = wide_state_v1();
    let fixture = WideFixtureV1::of_v1(&state);
    let substeps = fixture.substeps_v1();
    let groups = group_v1(&substeps);
    let input = LineBAuxiliaryInputV1 {
        scheduled_count: None,
        direction: LineBDivergenceV1::Forward,
        coefficient: 0.5,
        roots: vec![LineBAuxiliaryRootV1 {
            group_index: 0,
            substep_index: 0,
            target: None,
        }],
    };
    assert_refused_v1(state, "in-batch line (b)", |state| {
        core_step_v1(
            state,
            &groups,
            Some(AuxiliaryTermsSourceV1::InBatch(&input)),
            None,
        )
    });
}

#[test]
fn wide_refuses_external_auxiliary() {
    let state = wide_state_v1();
    let fixture = WideFixtureV1::of_v1(&state);
    let substeps = fixture.substeps_v1();
    let groups = group_v1(&substeps);
    let target = archive_distillation_v1::one_hot_target_v1(fixture.logits.len(), 0).unwrap();
    let external = archive_distillation_v1::ExternalAuxiliaryInputV1 {
        direction: LineBDivergenceV1::Forward,
        coefficient: 0.5,
        denominator: 1,
        cap: None,
        step_telemetry: false,
        rows: vec![archive_distillation_v1::ExternalAuxiliaryRowV1 {
            encoded: encoded_decision_view_v3(&fixture.tensor),
            target: &target,
        }],
    };
    assert_refused_v1(state, "external auxiliary", |state| {
        core_step_v1(
            state,
            &groups,
            Some(AuxiliaryTermsSourceV1::External(&external)),
            None,
        )
    });
}

#[test]
fn wide_refuses_exploration() {
    let state = wide_state_v1();
    let fixture = WideFixtureV1::of_v1(&state);
    let substeps = fixture.substeps_v1();
    let groups = group_v1(&substeps);
    assert_refused_v1(state, "exploration and recovery", |state| {
        core_step_v1(state, &groups, None, Some((0.01, 2, false)))
    });
}

#[test]
fn wide_refuses_recovery() {
    let state = wide_state_v1();
    let fixture = WideFixtureV1::of_v1(&state);
    let substeps = fixture.substeps_v1();
    let groups = group_v1(&substeps);
    // The recovery entry's shape: the first update, with diagnostics.
    assert_refused_v1(state, "exploration and recovery", |state| {
        core_step_v1(state, &groups, None, Some((0.01, 1, true)))
    });
}

#[test]
fn wide_refuses_packed_inputs_on_both_drivers() {
    // A packed tape can only come from the narrow builder; the wide drivers
    // refuse it before validating anything about it.
    let narrow =
        NativePolicyValueNetV1::runner_fixed_v1(NativePolicyValueModelConfigV1::contract_v1())
            .unwrap();
    let builder = NativePolicyPackedForwardBuilderV1::from_model_v1(&narrow).unwrap();
    let tensor = real_map_training_tensor_v3();
    // The builder forwards under the contract schema; the V3 tensor has its
    // dims.
    let encoded = NativeEncodedDecisionViewV1 {
        schema: crate::native_policy_value_net_v1::NativeEncodedDecisionSchemaV1::contract_v1(),
        ..encoded_decision_view_v3(&tensor)
    };
    let tape = builder.forward_v1(encoded).unwrap();
    let logits: Vec<u32> = tape
        .logits_v1()
        .iter()
        .map(|value| value.to_bits())
        .collect();
    let substeps = [NativePolicySubstepV1 {
        forward: NativePolicyForwardInputV1::Packed {
            encoded: Box::new(encoded),
            tape: &tape,
        },
        selected_action_index: 0,
        expected_raw_action_logit_bits: &logits,
        expected_value_bits: tape.value_v1().to_bits(),
    }];
    let groups = group_v1(&substeps);
    assert_refused_v1(wide_state_v1(), "packed forward input", |state| {
        state
            .train_step_with_recompute_workers_dims_v1(&groups, VC, LR, 4)
            .map(|_| ())
    });
    assert_refused_v1(wide_state_v1(), "packed forward input", |state| {
        state
            .train_step_gae_feature_transfer_v3_dims_v1(&groups, &[0.3], &[0.15], VC, LR)
            .map(|_| ())
    });
}

#[test]
fn wide_refuses_a_no_state_digest_install() {
    assert_refused_v1(wide_state_v1(), "no-state-digest install", |state| {
        state.install_no_state_digest_dims_v1(123..219)
    });
    let state = wide_state_v1();
    assert!(state.no_state_digest_columns.is_none());
}

#[test]
fn wide_refuses_portable_math_on_both_drivers() {
    let mut state = wide_state_v1();
    let fixture = WideFixtureV1::of_v1(&state);
    let substeps = fixture.substeps_v1();
    let groups = group_v1(&substeps);
    state.set_training_math_dims_v1(TrainingMathV1::PortableV1);
    assert_refused_v1(state.clone(), "portable math", |state| {
        state
            .train_step_gae_feature_transfer_profiled_dims_v1(
                FreshLineageGenerationV1::V3,
                &groups,
                &[0.3],
                &[0.15],
                VC,
                LR,
                None,
                1,
                &mut NativeTrainingPhaseRecorderV1::disabled_v1(),
            )
            .map(|_| ())
    });
    assert_refused_v1(state, "portable math", |state| {
        state
            .train_step_feature_transfer_v3_dims_v1(&groups, VC, LR)
            .map(|_| ())
    });
}
