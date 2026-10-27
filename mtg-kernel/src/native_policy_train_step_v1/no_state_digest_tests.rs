//! The no-state-digest training contract at the train state
//! (`no_state_digest_v1`; lane `no-digest-training-20261023`, DESIGN.md v2
//! section 5): the production forward, backward and Adam keep the digest
//! columns inert and at +0.0, invariant violations are refused before a step
//! commits, and every worker verifies the pinned MXCSR.

use super::*;
use crate::native_flat_tensorizer_v3::{encoded_decision_view_v3, NativeFlatDecisionTensorV3};
use crate::native_policy_train_step_v1::tests::real_map_training_tensor_v3;
use crate::native_policy_value_net_v1::ForwardActivationModeV1;
use crate::sideboard_play_policy_v1::FreshLineageGenerationV1;

const DIGEST: std::ops::Range<usize> = 123..219;

/// One invariant violation written into a snapshot.
type SnapshotEditV1 = fn(&mut NativePolicyValueTrainSnapshotV1);
const VC: f32 = 0.75;
const LR: f32 = 0.0003;

fn model() -> NativePolicyValueNetV1 {
    NativePolicyValueNetV1::runner_fixed_v1(NativePolicyValueModelConfigV1::contract_v1()).unwrap()
}

/// Two decision tensors with +0.0 digest inputs and kept-input witnesses:
/// -0.0 and subnormals of both signs.
fn tensors() -> [NativeFlatDecisionTensorV3; 2] {
    let mut first = real_map_training_tensor_v3();
    first.common.state[DIGEST].fill(0.0);
    first.common.state[0] = -0.0;
    first.common.state[1] = f32::from_bits(1);
    first.common.state[122] = -f32::from_bits(0x0000_0400);
    let mut second = first.clone();
    second.common.state[2] += 0.125;
    second.common.state[3] = -0.0;
    [first, second]
}

/// Every element of the state encoder weight's columns, row by row.
fn columns_of(values: &[f32], columns: std::ops::Range<usize>) -> Vec<u32> {
    values
        .chunks_exact(STATE_ENCODER_INPUT)
        .flat_map(|row| row[columns.clone()].iter().map(|v| v.to_bits()))
        .collect()
}

fn kept_of(values: &[f32]) -> Vec<u32> {
    values
        .chunks_exact(STATE_ENCODER_INPUT)
        .flat_map(|row| {
            row[..DIGEST.start]
                .iter()
                .chain(&row[DIGEST.end..])
                .map(|v| v.to_bits())
        })
        .collect()
}

/// Every bit of a snapshot outside the digest columns of the state encoder
/// weight and its moments.
fn kept_bits(snapshot: &NativePolicyValueTrainSnapshotV1) -> Vec<u32> {
    let mut bits = vec![snapshot.adam_step as u32, snapshot.scorer_bias_anchor_bits];
    for list in [
        &snapshot.parameters,
        &snapshot.first_moments,
        &snapshot.second_moments,
    ] {
        for (index, tensor) in list.iter().enumerate() {
            if index == STATE_FIRST_WEIGHT {
                bits.extend(kept_of(&tensor.values));
            } else {
                bits.extend(tensor.values.iter().map(|v| v.to_bits()));
            }
        }
    }
    bits
}

/// A state whose moments are nonzero everywhere they may be, with the
/// digest columns of W, m and v set by `digest` (None: +0.0).
fn state_with(digest: Option<(f32, f32, f32)>) -> NativePolicyValueTrainStateV1 {
    let base = NativePolicyValueTrainStateV1::new_v1(model()).unwrap();
    let mut snapshot = base.snapshot_v1().unwrap();
    snapshot.adam_step = 7;
    for (index, (first, second)) in snapshot
        .first_moments
        .iter_mut()
        .zip(&mut snapshot.second_moments)
        .enumerate()
    {
        for (offset, (m, v)) in first.values.iter_mut().zip(&mut second.values).enumerate() {
            let witness = match offset % 5 {
                0 => f32::from_bits(3),
                1 => -0.0,
                2 => -1.5e-3,
                3 => 2.5e-4,
                _ => 0.0,
            };
            *m = witness;
            *v = if offset.is_multiple_of(3) {
                f32::from_bits(7)
            } else {
                1.0e-6 * (offset % 11) as f32
            };
            if index == CARD_EMBEDDING && offset < CARD_EMBEDDING_DIM_V1 {
                *m = 0.0;
                *v = 0.0;
            }
        }
    }
    snapshot.first_moments[SCORER_SECOND_BIAS].values[0] = 0.0;
    snapshot.second_moments[SCORER_SECOND_BIAS].values[0] = 0.0;
    // No -0.0 bias, and a +0.0 witness in it.
    for value in &mut snapshot.parameters[STATE_FIRST_BIAS].values {
        if value.to_bits() == 0x8000_0000 {
            *value = 0.0;
        }
    }
    snapshot.parameters[STATE_FIRST_BIAS].values[0] = 0.0;
    for (tensor, fill) in [
        (
            &mut snapshot.parameters[STATE_FIRST_WEIGHT],
            digest.map(|d| d.0),
        ),
        (
            &mut snapshot.first_moments[STATE_FIRST_WEIGHT],
            digest.map(|d| d.1),
        ),
        (
            &mut snapshot.second_moments[STATE_FIRST_WEIGHT],
            digest.map(|d| d.2),
        ),
    ] {
        for (row_index, row) in tensor
            .values
            .chunks_exact_mut(STATE_ENCODER_INPUT)
            .enumerate()
        {
            for (offset, value) in row[DIGEST].iter_mut().enumerate() {
                *value = fill.map_or(0.0, |fill| {
                    if (row_index + offset).is_multiple_of(2) {
                        fill
                    } else {
                        fill.abs() * 0.5
                    }
                });
            }
        }
    }
    let state = NativePolicyValueTrainStateV1::from_snapshot_v1(model(), &snapshot).unwrap();
    assert_eq!(state.adam_step_v1(), 7);
    state
}

struct OutputsV1 {
    logits: [Vec<u32>; 2],
    values: [u32; 2],
}

fn outputs(
    state: &NativePolicyValueTrainStateV1,
    tensors: &[NativeFlatDecisionTensorV3; 2],
) -> OutputsV1 {
    let outputs = tensors.each_ref().map(|tensor| {
        state
            .model_v1()
            .forward_feature_transfer_v3_mode(
                encoded_decision_view_v3(tensor),
                ForwardActivationModeV1::LibmTanh,
            )
            .unwrap()
    });
    OutputsV1 {
        logits: outputs
            .each_ref()
            .map(|output| output.logits.iter().map(|v| v.to_bits()).collect()),
        values: outputs.each_ref().map(|output| output.value.to_bits()),
    }
}

fn substeps<'a>(
    tensors: &'a [NativeFlatDecisionTensorV3; 2],
    outputs: &'a OutputsV1,
    copies: usize,
) -> Vec<Vec<NativePolicySubstepV1<'a>>> {
    let step = |tensor: usize, selected: usize| NativePolicySubstepV1 {
        forward: NativePolicyForwardInputV1::Encoded(Box::new(encoded_decision_view_v3(
            &tensors[tensor],
        ))),
        selected_action_index: selected,
        expected_raw_action_logit_bits: outputs.logits[tensor].as_slice(),
        expected_value_bits: outputs.values[tensor],
    };
    (0..copies)
        .flat_map(|_| {
            [
                vec![step(0, 0), step(1, 1)],
                vec![step(1, outputs.logits[1].len() - 1)],
            ]
        })
        .collect()
}

fn groups<'a>(
    steps: &'a [Vec<NativePolicySubstepV1<'a>>],
) -> Vec<NativePolicyPhysicalDecisionV1<'a>> {
    steps
        .iter()
        .enumerate()
        .map(|(index, substeps)| NativePolicyPhysicalDecisionV1 {
            substeps,
            terminal_return: if index.is_multiple_of(2) { 1 } else { -1 },
            baseline_bits: 0,
        })
        .collect()
}

/// One production update of `state`, alternating the terminal and the GAE
/// objectives by `step`.
fn update(
    state: &mut NativePolicyValueTrainStateV1,
    tensors: &[NativeFlatDecisionTensorV3; 2],
    step: usize,
) -> Result<NativePolicyTrainStepResultV1, NativePolicyTrainErrorV1> {
    let recorded = outputs(state, tensors);
    let steps = substeps(tensors, &recorded, 1);
    let groups = groups(&steps);
    if step.is_multiple_of(2) {
        state.train_step_feature_transfer_v3(&groups, VC, LR)
    } else {
        state.train_step_gae_feature_transfer_v3(&groups, &[0.4, -0.3], &[0.6, -0.6], VC, LR)
    }
}

/// The compact-equivalence check through production code (design section
/// 5). The model's layout is fixed at 1,499 inputs, so the compact model is
/// represented by the padded one with provably inert digest columns: the
/// converted state (+0.0 W, m and v columns, under the contract) and a
/// padded state whose digest columns of W, m and v hold arbitrary nonzero
/// values train on the same +0.0 digest inputs for several steps of both
/// objectives. Their forwards, every kept gradient (the kept input
/// gradients feed the upstream encoders' gradients) and every kept
/// parameter and moment bit agree, with nonzero kept moments and signed-zero
/// and subnormal witnesses; the converted columns stay +0.0.
#[test]
fn converted_kept_parameters_follow_the_inert_padded_arithmetic() {
    let tensors = tensors();
    let mut converted = state_with(None);
    converted.install_no_state_digest_v1(DIGEST).unwrap();
    let mut padded = state_with(Some((0.375, -2.0e-3, 4.0e-4)));
    assert_eq!(
        kept_bits(&converted.snapshot_v1().unwrap()),
        kept_bits(&padded.snapshot_v1().unwrap())
    );
    for step in 0..4 {
        let converted_outputs = outputs(&converted, &tensors);
        let padded_outputs = outputs(&padded, &tensors);
        assert_eq!(
            converted_outputs.logits, padded_outputs.logits,
            "step {step}"
        );
        assert_eq!(
            converted_outputs.values, padded_outputs.values,
            "step {step}"
        );
        let a = update(&mut converted, &tensors, step).unwrap();
        let b = update(&mut padded, &tensors, step).unwrap();
        assert_eq!(a.loss.to_bits(), b.loss.to_bits(), "step {step}");
        for (index, (x, y)) in a.gradients.iter().zip(&b.gradients).enumerate() {
            if index == STATE_FIRST_WEIGHT {
                assert_eq!(kept_of(&x.values), kept_of(&y.values), "step {step}");
                // Signed zeros only: the digest columns carry no gradient.
                assert!(columns_of(&x.values, DIGEST).iter().all(|bits| *bits == 0));
                assert!(columns_of(&y.values, DIGEST)
                    .iter()
                    .all(|bits| bits & 0x7fff_ffff == 0));
            } else {
                let bits = |v: &[f32]| v.iter().map(|v| v.to_bits()).collect::<Vec<_>>();
                assert_eq!(
                    bits(&x.values),
                    bits(&y.values),
                    "step {step} tensor {index}"
                );
            }
        }
        let after = converted.snapshot_v1().unwrap();
        assert_eq!(kept_bits(&after), kept_bits(&padded.snapshot_v1().unwrap()));
        for list in [
            &after.parameters,
            &after.first_moments,
            &after.second_moments,
        ] {
            assert!(columns_of(&list[STATE_FIRST_WEIGHT].values, DIGEST)
                .iter()
                .all(|bits| *bits == 0));
        }
    }
    // The kept parameters did move.
    assert_ne!(
        kept_bits(&converted.snapshot_v1().unwrap()),
        kept_bits(&state_with(None).snapshot_v1().unwrap())
    );
}

#[test]
fn installing_the_contract_refuses_every_invariant_violation_without_repair() {
    let mut clean = state_with(None);
    clean.install_no_state_digest_v1(DIGEST.clone()).unwrap();
    assert_eq!(clean.no_state_digest_columns_v1(), Some(DIGEST));
    assert_eq!(
        clean.install_no_state_digest_v1(DIGEST),
        Err(NativePolicyTrainErrorV1::NoStateDigestInvariant {
            code: "no-state-digest-contract-already-installed"
        })
    );
    let cases: [(SnapshotEditV1, &str); 7] = [
        (
            |s| s.parameters[STATE_FIRST_WEIGHT].values[150] = 0.5,
            "no-state-digest-weight-column-not-positive-zero",
        ),
        (
            |s| s.parameters[STATE_FIRST_WEIGHT].values[STATE_ENCODER_INPUT + 123] = -0.0,
            "no-state-digest-weight-column-not-positive-zero",
        ),
        (
            |s| s.first_moments[STATE_FIRST_WEIGHT].values[218] = 1.0e-3,
            "no-state-digest-first-moment-column-not-positive-zero",
        ),
        (
            |s| s.first_moments[STATE_FIRST_WEIGHT].values[200] = -0.0,
            "no-state-digest-first-moment-column-not-positive-zero",
        ),
        (
            |s| s.second_moments[STATE_FIRST_WEIGHT].values[130] = f32::from_bits(1),
            "no-state-digest-second-moment-column-not-positive-zero",
        ),
        (
            |s| s.parameters[STATE_FIRST_BIAS].values[3] = -0.0,
            "no-state-digest-negative-zero-state-encoder-bias",
        ),
        (
            |s| s.parameters[STATE_FIRST_BIAS].values[0] = -0.0,
            "no-state-digest-negative-zero-state-encoder-bias",
        ),
    ];
    for (mutate, code) in cases {
        let mut snapshot = state_with(None).snapshot_v1().unwrap();
        mutate(&mut snapshot);
        let mut state =
            NativePolicyValueTrainStateV1::from_snapshot_v1(model(), &snapshot).unwrap();
        let before = state.state_sha256_v1().unwrap();
        assert_eq!(
            state.install_no_state_digest_v1(DIGEST),
            Err(NativePolicyTrainErrorV1::NoStateDigestInvariant { code })
        );
        // Nothing was zeroed or installed.
        assert_eq!(state.state_sha256_v1().unwrap(), before);
        assert_eq!(state.no_state_digest_columns_v1(), None);
    }
}

/// A nonzero digest input gives the digest weight columns a gradient; the
/// candidate's W, m and v columns are refused before the step commits, so
/// the live state is unchanged. The same batch trains an ordinary state.
#[test]
fn a_candidate_step_that_breaks_the_columns_is_refused_with_the_live_state_unchanged() {
    let mut tensors = tensors();
    tensors[0].common.state[150] = 0.5;
    let mut converted = state_with(None);
    converted.install_no_state_digest_v1(DIGEST).unwrap();
    let before = converted.state_sha256_v1().unwrap();
    for step in 0..2 {
        assert_eq!(
            update(&mut converted, &tensors, step).unwrap_err(),
            NativePolicyTrainErrorV1::NoStateDigestInvariant {
                code: "no-state-digest-weight-column-not-positive-zero"
            }
        );
        assert_eq!(converted.state_sha256_v1().unwrap(), before);
        assert_eq!(converted.adam_step_v1(), 7);
    }
    let mut ordinary = state_with(None);
    update(&mut ordinary, &tensors, 0).unwrap();
    assert_eq!(ordinary.adam_step_v1(), 8);
}

#[test]
fn the_contract_refuses_learner_paths_it_does_not_cover() {
    let tensors = tensors();
    let mut converted = state_with(None);
    converted.install_no_state_digest_v1(DIGEST).unwrap();
    let recorded = outputs(&converted, &tensors);
    let steps = substeps(&tensors, &recorded, 1);
    let groups = groups(&steps);
    assert_eq!(
        converted
            .train_step_gae_recovery_v1(
                FreshLineageGenerationV1::V3,
                &groups,
                &[0.4, -0.3],
                &[0.6, -0.6],
                VC,
                0.0001,
                0.01,
                1,
                false,
            )
            .unwrap_err(),
        NativePolicyTrainErrorV1::NoStateDigestUnsupported {
            path: "recovery loss"
        }
    );
    assert_eq!(
        converted
            .train_step_terminal_winner_imitation_v4(&groups, LR, 2)
            .unwrap_err(),
        NativePolicyTrainErrorV1::NoStateDigestUnsupported { path: "imitation" }
    );
    assert_eq!(
        converted.require_no_input_contract_v1("cuda"),
        Err(NativePolicyTrainErrorV1::NoStateDigestUnsupported { path: "cuda" })
    );
}

/// Every spawned worker of a contract step verifies its own MXCSR: a worker
/// whose flush-to-zero bit is set fails the step (fixed-partition backward
/// and the GAE forward workers), and the live state is unchanged.
#[test]
fn a_dirty_worker_mxcsr_fails_the_step_before_it_commits() {
    let tensors = tensors();
    let mut converted = state_with(None);
    converted.install_no_state_digest_v1(DIGEST).unwrap();
    converted.dirty_worker_mxcsr_for_test_v1();
    let before = converted.state_sha256_v1().unwrap();
    let recorded = outputs(&converted, &tensors);
    let dirty = |error: NativePolicyTrainErrorV1| {
        matches!(error, NativePolicyTrainErrorV1::MxcsrPinnedState { observed }
            if observed & (1 << 15) != 0)
    };
    // Fixed-partition backward workers.
    let steps = substeps(&tensors, &recorded, 1);
    let batch = groups(&steps);
    let error = converted
        .train_step_gae_feature_transfer_profiled_v1(
            FreshLineageGenerationV1::V3,
            &batch,
            &[0.4, -0.3],
            &[0.6, -0.6],
            VC,
            LR,
            Some(4),
            1,
            &mut NativeTrainingPhaseRecorderV1::disabled_v1(),
        )
        .unwrap_err();
    assert!(dirty(error.clone()), "{error:?}");
    // GAE forward workers: at least 32 substeps per worker.
    let steps = substeps(&tensors, &recorded, 32);
    let batch = groups(&steps);
    let targets = vec![0.25; batch.len()];
    let advantages: Vec<f32> = (0..batch.len())
        .map(|index| if index.is_multiple_of(2) { 0.5 } else { -0.5 })
        .collect();
    let error = converted
        .train_step_gae_feature_transfer_profiled_v1(
            FreshLineageGenerationV1::V3,
            &batch,
            &targets,
            &advantages,
            VC,
            LR,
            None,
            2,
            &mut NativeTrainingPhaseRecorderV1::disabled_v1(),
        )
        .unwrap_err();
    assert!(dirty(error.clone()), "{error:?}");
    assert_eq!(converted.state_sha256_v1().unwrap(), before);
    // The calling thread was never dirtied.
    assert!(crate::deterministic_math_v1::verify_pinned_mxcsr_state_v1().is_ok());
    // A clean converted state trains the same batch on the same workers.
    let mut clean = state_with(None);
    clean.install_no_state_digest_v1(DIGEST).unwrap();
    clean
        .train_step_gae_feature_transfer_profiled_v1(
            FreshLineageGenerationV1::V3,
            &batch,
            &targets,
            &advantages,
            VC,
            LR,
            None,
            2,
            &mut NativeTrainingPhaseRecorderV1::disabled_v1(),
        )
        .unwrap();
    assert_eq!(clean.adam_step_v1(), 8);
}
