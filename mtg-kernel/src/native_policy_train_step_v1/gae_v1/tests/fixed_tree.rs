//! The fixed-tree backward (`fixed_tree_backward_v1`, the opt-in
//! `update_backward_execution` value `fixed_tree_v1`) through the whole plain
//! GAE step: forward tapes, loss, backward, gauge, Adam and commit.

use super::*;
use crate::native_policy_train_step_v1::fixed_tree_backward_v1::{
    with_fixed_tree_hooks_for_test_v1 as with_fixed_tree, FixedTreeHooksForTestV1,
};
use crate::native_policy_train_step_v1::FIXED_TREE_CHUNK_ROWS_V1;
use crate::sideboard_play_policy_v1::FreshLineageGenerationV1;

/// The worker counts every invariance test runs at.
const WORKER_COUNTS: [usize; 5] = [1, 2, 3, 7, 16];

/// One plain GAE step under the fixed tree on `workers` workers, with
/// `profiled_step`'s targets and advantages.
fn fixed_tree_step(
    state: &mut NativePolicyValueTrainStateV1,
    groups: &[NativePolicyPhysicalDecisionV1<'_>],
    workers: usize,
) -> Result<NativePolicyTrainStepResultV1, NativePolicyTrainErrorV1> {
    let targets = (0..groups.len())
        .map(|g| 0.25 - 0.01 * g as f32)
        .collect::<Vec<_>>();
    let advantages = (0..groups.len())
        .map(|g| if g.is_multiple_of(2) { 0.5 } else { -0.375 } / (1.0 + g as f32))
        .collect::<Vec<_>>();
    state.train_step_gae_feature_transfer_fixed_tree_dims_v1(
        FreshLineageGenerationV1::V3,
        groups,
        &targets,
        &advantages,
        VC,
        LR,
        workers,
        &mut NativeTrainingPhaseRecorderV1::disabled_v1(),
    )
}

fn substep_count(steps: &[Vec<NativePolicySubstepV1<'_>>]) -> usize {
    steps.iter().map(Vec::len).sum()
}

/// The whole update is the same bits at 1, 2, 3, 7 and 16 workers: every
/// reported number (Debug prints each float in shortest round-trip form, so
/// equal text is equal bits) and the complete committed state. Everything the
/// step computes before its backward is the sequential step's, and the
/// gradient is another reduction of the same terms, so its bits are not.
#[test]
fn the_fixed_tree_step_keeps_every_bit_across_worker_counts() {
    let base = model();
    let fixture = Fixture::capture(&base);
    // 399 substeps: seven chunks, the last of fifteen rows.
    let steps = many_substeps(&fixture, 200);
    assert_eq!(substep_count(&steps), 399);
    let batch = physical_groups(&steps);
    let start = NativePolicyValueTrainStateV1::new_v1(base).unwrap();
    let mut sequential = start.clone();
    let (sequential_result, runs) = with_fixed_tree(FixedTreeHooksForTestV1::NONE, || {
        profiled_step(&mut sequential, &batch, 1).unwrap()
    });
    // The sequential backward never enters the fixed tree.
    assert_eq!(runs, 0);

    let mut reference = None;
    for workers in WORKER_COUNTS {
        let mut state = start.clone();
        let (result, runs) = with_fixed_tree(FixedTreeHooksForTestV1::NONE, || {
            fixed_tree_step(&mut state, &batch, workers).unwrap()
        });
        assert_eq!(runs, 1, "workers {workers}");
        let text = format!("{result:?}");
        let bits = state_bits(&state);
        match &reference {
            None => reference = Some((text, bits, result)),
            Some((reference_text, reference_bits, reference_result)) => {
                assert_eq!(&text, reference_text, "workers {workers}");
                assert_eq!(
                    gradient_bits(&result),
                    gradient_bits(reference_result),
                    "workers {workers}"
                );
                assert_eq!(&bits, reference_bits, "workers {workers}");
            }
        }
    }
    let (_, fixed_tree_bits, fixed_tree) = reference.unwrap();

    // The loss scalars, the recomputed outputs, the physical terms and the
    // gauge's bound and high-precision residual are the sequential step's:
    // with the two gradient records exchanged, the results are equal.
    let mut exchanged = fixed_tree.clone();
    exchanged.gradients = sequential_result.gradients.clone();
    exchanged.scorer_bias_gauge.raw_gradient_residual =
        sequential_result.scorer_bias_gauge.raw_gradient_residual;
    assert_eq!(format!("{exchanged:?}"), format!("{sequential_result:?}"));
    assert_eq!(
        fixed_tree.loss.to_bits(),
        sequential_result.loss.to_bits(),
        "the loss is summed before any backward runs"
    );
    // Not the sequential order's bits: another lineage.
    assert_ne!(
        gradient_bits(&fixed_tree),
        gradient_bits(&sequential_result)
    );
    assert_ne!(fixed_tree_bits, state_bits(&sequential));
    // The canonical gauge still holds: the projected element is exactly zero.
    let scorer_bias = fixed_tree
        .gradients
        .iter()
        .find(|gradient| gradient.name == "scorer.2.bias")
        .unwrap();
    assert_eq!(scorer_bias.values[0].to_bits(), 0);
}

/// A chunk size that followed the worker count (the rows shared evenly among
/// the workers) would train other bits at every worker count, so the
/// equality of all worker counts above holds only because the size is a
/// constant. The hook stands in for such an implementation.
#[test]
fn a_chunk_size_that_followed_the_worker_count_would_change_the_step() {
    let base = model();
    let fixture = Fixture::capture(&base);
    let steps = many_substeps(&fixture, 200);
    let rows = substep_count(&steps);
    let batch = physical_groups(&steps);
    let start = NativePolicyValueTrainStateV1::new_v1(base).unwrap();
    let mut contract = start.clone();
    let contract_result = fixed_tree_step(&mut contract, &batch, 4).unwrap();
    // The hook at the contract's own size changes nothing.
    let mut same = start.clone();
    let (same_result, _) = with_fixed_tree(
        FixedTreeHooksForTestV1 {
            chunk_rows: Some(FIXED_TREE_CHUNK_ROWS_V1),
            ..FixedTreeHooksForTestV1::NONE
        },
        || fixed_tree_step(&mut same, &batch, 4).unwrap(),
    );
    assert_eq!(format!("{same_result:?}"), format!("{contract_result:?}"));
    assert_eq!(state_bits(&same), state_bits(&contract));

    let mut seen = vec![state_bits(&contract)];
    for workers in WORKER_COUNTS {
        let share = rows.div_ceil(workers);
        assert_ne!(share, FIXED_TREE_CHUNK_ROWS_V1);
        let mut state = start.clone();
        let (result, runs) = with_fixed_tree(
            FixedTreeHooksForTestV1 {
                chunk_rows: Some(share),
                ..FixedTreeHooksForTestV1::NONE
            },
            || fixed_tree_step(&mut state, &batch, workers).unwrap(),
        );
        assert_eq!(runs, 1);
        assert_ne!(
            gradient_bits(&result),
            gradient_bits(&contract_result),
            "workers {workers}"
        );
        let bits = state_bits(&state);
        assert!(!seen.contains(&bits), "workers {workers}");
        seen.push(bits);
    }
}

/// A batch of at most one chunk is the sequential step bit for bit, at any
/// worker count: every reported number and the committed state.
#[test]
fn a_batch_of_one_chunk_is_the_sequential_step_bit_for_bit() {
    let base = model();
    let fixture = Fixture::capture(&base);
    // 63 substeps.
    let steps = many_substeps(&fixture, 32);
    assert!(substep_count(&steps) <= FIXED_TREE_CHUNK_ROWS_V1);
    let batch = physical_groups(&steps);
    let start = NativePolicyValueTrainStateV1::new_v1(base).unwrap();
    let mut sequential = start.clone();
    let expected = profiled_step(&mut sequential, &batch, 1).unwrap();
    for workers in [1, 4] {
        let mut state = start.clone();
        let (actual, runs) = with_fixed_tree(FixedTreeHooksForTestV1::NONE, || {
            fixed_tree_step(&mut state, &batch, workers).unwrap()
        });
        assert_eq!(runs, 1);
        assert_eq!(gradient_bits(&actual), gradient_bits(&expected));
        assert_eq!(format!("{actual:?}"), format!("{expected:?}"));
        assert_eq!(state_bits(&state), state_bits(&sequential));
    }
}

/// Under the portable math the fixed tree keeps every bit across worker
/// counts too, and trains other bits than under the platform math.
#[test]
fn the_fixed_tree_step_keeps_worker_parity_under_the_portable_math() {
    let base = model();
    let fixture = Fixture::capture_mode(&base, ForwardActivationModeV1::KernelDeterministicTanh);
    let steps = many_substeps(&fixture, 200);
    let batch = physical_groups(&steps);
    let mut start = NativePolicyValueTrainStateV1::new_v1(base.clone()).unwrap();
    start.set_training_math_v1(TrainingMathV1::PortableV1);
    let mut first = start.clone();
    let expected = fixed_tree_step(&mut first, &batch, 1).unwrap();
    for workers in [2, 3, 7, 16] {
        let mut state = start.clone();
        let actual = fixed_tree_step(&mut state, &batch, workers).unwrap();
        assert_eq!(format!("{actual:?}"), format!("{expected:?}"), "{workers}");
        assert_eq!(state_bits(&state), state_bits(&first), "{workers}");
    }
    let platform = Fixture::capture(&base);
    let platform_steps = many_substeps(&platform, 200);
    let mut libm = NativePolicyValueTrainStateV1::new_v1(base).unwrap();
    fixed_tree_step(&mut libm, &physical_groups(&platform_steps), 4).unwrap();
    assert_ne!(state_bits(&libm), state_bits(&first));
}

/// The pregame announcement step scale under the fixed tree: both gradient
/// buffers are reduced by the tree, the step keeps every bit across worker
/// counts, and with at most one chunk in each pass it is the sequential
/// scaled step bit for bit.
#[test]
fn the_fixed_tree_routes_the_pregame_announcement_buffers() {
    let scaled = |state: &mut NativePolicyValueTrainStateV1,
                  batch: &[NativePolicyPhysicalDecisionV1<'_>],
                  targets: &[f32],
                  advantages: &[f32],
                  workers: usize| {
        state.set_pregame_announcement_step_scale_v1(Some(0.1));
        state
            .train_step_gae_feature_transfer_fixed_tree_dims_v1(
                FreshLineageGenerationV1::V3,
                batch,
                targets,
                advantages,
                VC,
                LR,
                workers,
                &mut NativeTrainingPhaseRecorderV1::disabled_v1(),
            )
            .unwrap()
    };
    let start = warmed_state();
    let fixture = announcement_fixture(start.model_v1());

    // 399 substeps: the rest pass has seven chunks and the announcement pass
    // (the marked groups' substeps) several.
    let steps = many_substeps(&fixture, 200);
    let batch = physical_groups(&steps);
    let (targets, advantages) = gae_inputs(&batch);
    let mut sequential = start.clone();
    let sequential_result =
        announcement_step(&mut sequential, &batch, &targets, &advantages, 1, Some(0.1)).unwrap();
    let sequential_record = sequential
        .take_pregame_announcement_step_record_v1()
        .unwrap();
    let mut reference = None;
    for workers in WORKER_COUNTS {
        let mut state = start.clone();
        let (result, runs) = with_fixed_tree(FixedTreeHooksForTestV1::NONE, || {
            scaled(&mut state, &batch, &targets, &advantages, workers)
        });
        // One fixed-tree backward for each buffer.
        assert_eq!(runs, 2, "workers {workers}");
        let record = state.take_pregame_announcement_step_record_v1().unwrap();
        assert_eq!(record.routed_groups, sequential_record.routed_groups);
        assert_eq!(state.pregame_announcement_step_scale_v1(), None);
        let observed = (format!("{result:?}"), state_bits(&state), record);
        match &reference {
            None => reference = Some(observed),
            Some(reference) => assert_eq!(&observed, reference, "workers {workers}"),
        }
    }
    let (text, bits, _) = reference.unwrap();
    assert_ne!(text, format!("{sequential_result:?}"));
    assert_ne!(bits, state_bits(&sequential));

    // 63 substeps: one chunk in each pass, so the sequential scaled step.
    let steps = many_substeps(&fixture, 32);
    assert!(substep_count(&steps) <= FIXED_TREE_CHUNK_ROWS_V1);
    let batch = physical_groups(&steps);
    let (targets, advantages) = gae_inputs(&batch);
    let mut sequential = start.clone();
    let expected =
        announcement_step(&mut sequential, &batch, &targets, &advantages, 1, Some(0.1)).unwrap();
    let expected_record = sequential
        .take_pregame_announcement_step_record_v1()
        .unwrap();
    assert!(expected_record.routed_groups > 0);
    let mut state = start.clone();
    let actual = scaled(&mut state, &batch, &targets, &advantages, 3);
    assert_eq!(format!("{actual:?}"), format!("{expected:?}"));
    assert_eq!(
        state.take_pregame_announcement_step_record_v1().unwrap(),
        expected_record
    );
    assert_eq!(state_bits(&state), state_bits(&sequential));
}

/// Only the plain GAE objective runs the fixed tree. The imitation seam, the
/// recovery loss, line (b) and the terminal loss refuse it before any change
/// to the state.
#[test]
fn the_fixed_tree_is_refused_outside_the_plain_gae_step() {
    let base = NativePolicyValueTrainStateV1::new_v1(model()).unwrap();
    let fixture = Fixture::capture(base.model_v1());
    let steps = fixture.substeps();
    let groups = groups(&steps);
    let config = base.model_v1().feature_transfer_config_v3();
    let expected = NativePolicyTrainErrorV1::FixedTreeBackward {
        code: "fixed-tree-backward-requires-plain-gae",
    };
    let refused =
        |run: &dyn Fn(&mut NativePolicyValueTrainStateV1) -> Option<NativePolicyTrainErrorV1>| {
            let mut state = base.clone();
            let (error, runs) = with_fixed_tree(FixedTreeHooksForTestV1::NONE, || run(&mut state));
            assert_eq!(runs, 0);
            assert_eq!(error.as_ref(), Some(&expected));
            assert_eq!(state_bits(&state), state_bits(&base));
        };
    // The imitation seam.
    refused(&|state| {
        state
            .train_step_gae_with_input_config_v1(
                &groups,
                &[0.0, 0.0],
                &[1.0, 1.0],
                0.0,
                LR,
                BackwardExecutionV1::FixedTree,
                config,
                true,
            )
            .err()
    });
    // The recovery loss.
    refused(&|state| {
        state
            .train_step_gae_core_v1(
                &groups,
                &[0.3, -0.2],
                &[0.15, -0.4],
                VC,
                LR,
                BackwardExecutionV1::FixedTree,
                config,
                false,
                None,
                Some((0.01, 1, false)),
            )
            .err()
    });
    // Line (b).
    let line_b = line_b_input(
        crate::line_b_teacher_target_v1::LineBDivergenceV1::Forward,
        0.1,
        [Some(line_b_target(&fixture)), None],
    );
    refused(&|state| {
        state
            .train_step_gae_core_v1(
                &groups,
                &LINE_B_TARGETS,
                &LINE_B_ADVANTAGES,
                VC,
                LR,
                BackwardExecutionV1::FixedTree,
                config,
                false,
                Some(AuxiliaryTermsSourceV1::InBatch(&line_b)),
                None,
            )
            .err()
    });
    // The terminal loss.
    refused(&|state| {
        state
            .train_step_with_input_config_v1(
                &groups,
                VC,
                LR,
                1,
                BackwardExecutionV1::FixedTree,
                &mut NativeTrainingPhaseRecorderV1::disabled_v1(),
                config,
            )
            .err()
    });
    // The plain objective itself trains.
    let mut state = base.clone();
    state
        .train_step_gae_with_input_config_v1(
            &groups,
            &[0.3, -0.2],
            &[0.15, -0.4],
            VC,
            LR,
            BackwardExecutionV1::FixedTree,
            config,
            false,
        )
        .unwrap();
    assert_ne!(state_bits(&state), state_bits(&base));
}
