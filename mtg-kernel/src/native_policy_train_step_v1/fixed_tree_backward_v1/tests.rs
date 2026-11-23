use super::*;
use crate::native_flat_tensorizer_v3::{encoded_decision_view_v3, NativeFlatDecisionTensorV3};
use crate::native_policy_train_step_v1::tests::real_map_training_tensor_v3;

const VC: f32 = 0.75;
const MATH: TrainingMathV1 = TrainingMathV1::PlatformLibm;

/// The worker counts every invariance test runs at.
const WORKER_COUNTS: [usize; 5] = [1, 2, 3, 7, 16];

/// Which of the fixture's tapes a substep replays. Tapes 2 and 3 name cards
/// no other tape names; `tape_of_v1` puts tape 3 in one group only.
const RARE_CARDS_TAPE: usize = 2;
const SINGLE_GROUP_TAPE: usize = 3;
const SINGLE_GROUP: usize = 11;

fn tape_of_v1(group: usize, substep: usize) -> usize {
    if group == SINGLE_GROUP {
        SINGLE_GROUP_TAPE
    } else if group % 37 == 5 {
        RARE_CARDS_TAPE
    } else {
        (group + substep) % 2
    }
}

/// The net-8 fixed model's parameters and four real forward tapes: the real
/// Map decision, the same with one state coordinate moved, and each of those
/// with its objects renamed to cards nothing else in the batch names.
struct TapesV1 {
    parameters: Vec<NativeNamedParameterV1>,
    tapes: Vec<DecisionTapeV1>,
}

fn tapes_v1() -> TapesV1 {
    let model =
        NativePolicyValueNetV1::runner_fixed_v1(NativePolicyValueModelConfigV1::contract_v1())
            .unwrap();
    let parameters = model.parameter_snapshot_v1();
    let config = model.feature_transfer_config_v3();
    let first = real_map_training_tensor_v3();
    let mut second = first.clone();
    second.common.state[0] += 0.125;
    // Every object's card renamed to a token nothing named so far.
    let mut named = first
        .common
        .object_card_ids
        .iter()
        .copied()
        .collect::<std::collections::BTreeSet<i64>>();
    let mut renamed = |tensor: &NativeFlatDecisionTensorV3| {
        let mut tensor = tensor.clone();
        for token in tensor.common.object_card_ids.iter_mut() {
            if *token != 0 {
                let fresh = (1_000..CARD_VOCAB_SIZE_V1 as i64)
                    .find(|candidate| !named.contains(candidate))
                    .unwrap();
                named.insert(fresh);
                *token = fresh;
            }
        }
        tensor
    };
    let third = renamed(&first);
    let fourth = renamed(&second);
    let tapes = [first, second, third, fourth]
        .iter()
        .map(|tensor| {
            forward_with_tape_dims_v1::<Net8DimsV1>(
                &parameters,
                config,
                encoded_decision_view_v3(tensor),
                MATH,
            )
            .unwrap()
        })
        .collect::<Vec<_>>();
    // The renamed tapes really name cards of their own.
    let cards = |tape: &DecisionTapeV1| {
        tape.object_card_ids_v1()
            .iter()
            .copied()
            .filter(|token| *token != 0)
            .collect::<std::collections::BTreeSet<_>>()
    };
    assert!(!cards(&tapes[0]).is_empty());
    assert!(cards(&tapes[RARE_CARDS_TAPE]).is_disjoint(&cards(&tapes[0])));
    assert!(cards(&tapes[SINGLE_GROUP_TAPE]).is_disjoint(&cards(&tapes[RARE_CARDS_TAPE])));
    TapesV1 { parameters, tapes }
}

/// `group_count` groups of one to three substeps over the fixture's tapes,
/// each with its own selected action, advantage and value error, so every
/// row's terms differ and most sums depend on the order of their additions.
fn groups_v1(fixture: &TapesV1, group_count: usize) -> Vec<GroupTapeV1<'_>> {
    (0..group_count)
        .map(|group| {
            let tapes = (0..1 + group % 3)
                .map(|substep| {
                    let tape = &fixture.tapes[tape_of_v1(group, substep)];
                    let selected = (group * 7 + substep) % tape.logits_v1().len();
                    let (_, log_probabilities) =
                        selected_log_softmax_math_v1(tape.logits_v1(), selected, MATH).unwrap();
                    SelectedDecisionTapeV1 {
                        tape: DecisionTapeSourceV1::Packed(tape),
                        selected_action_index: selected,
                        log_probabilities,
                    }
                })
                .collect();
            GroupTapeV1 {
                tapes,
                advantage: if group.is_multiple_of(2) { 0.5 } else { -0.375 }
                    / (1.0 + group as f32),
                value_error: 0.25 - 0.01 * group as f32,
                policy_mask: None,
            }
        })
        .collect()
}

fn row_count_v1(groups: &[GroupTapeV1<'_>]) -> usize {
    groups.iter().map(|group| group.tapes.len()).sum()
}

fn zeros_v1(parameters: &[NativeNamedParameterV1]) -> Vec<Vec<f32>> {
    parameters
        .iter()
        .map(|parameter| vec![0.0f32; parameter.values.len()])
        .collect()
}

fn bits_v1(tensors: &[Vec<f32>]) -> Vec<Vec<u32>> {
    tensors
        .iter()
        .map(|values| values.iter().map(|value| value.to_bits()).collect())
        .collect()
}

/// One row of the contract's row order, with its two loss-root coefficients.
type ReferenceRowV1<'g, 'a> = (&'g SelectedDecisionTapeV1<'a>, f32, f32);

/// The rows of `part` in the sequential pass's visiting order, written out
/// again here: groups from the last to the first, substeps from the last to
/// the first.
fn reference_rows_v1<'g, 'a>(
    groups: &'g [GroupTapeV1<'a>],
    part: BackwardPartV1<'_>,
    group_count: f32,
) -> Vec<ReferenceRowV1<'g, 'a>> {
    let mut rows = Vec::new();
    for (group_index, group) in groups.iter().enumerate().rev() {
        if !part.visits_v1(group_index) {
            continue;
        }
        let d_value = part.d_value_v1(group, VC, group_count);
        for (substep_index, selected) in group.tapes.iter().enumerate().rev() {
            rows.push((
                selected,
                part.d_joint_log_probability_v1(group_index, group, substep_index, group_count),
                if substep_index == 0 { d_value } else { 0.0 },
            ));
        }
    }
    rows
}

/// The sum of `rows` in order, from +0.0, on the reference sink: every
/// tensor dense and in its own shape, the card table included.
fn reference_sum_v1(
    parameters: &[NativeNamedParameterV1],
    rows: &[ReferenceRowV1<'_, '_>],
) -> Vec<Vec<f32>> {
    let mut gradients = zeros_v1(parameters);
    let mut workspace = ReverseWorkspaceV1::default();
    for (selected, d_joint_log_probability, d_value) in rows {
        prepare_reverse_logits_v1(selected, *d_joint_log_probability, &mut workspace, MATH);
        reverse_decision_dims_v1::<Net8DimsV1>(
            parameters,
            &mut gradients,
            &selected.tape,
            &workspace.d_logits,
            *d_value,
            &mut workspace.decision,
        )
        .unwrap();
    }
    gradients
}

/// The contract, written as plainly as it reads: chunks of `chunk_rows` rows,
/// each summed alone on the reference sink, then combined level by level,
/// left plus right, a last node without a partner carried up.
fn reference_tree_v1(
    parameters: &[NativeNamedParameterV1],
    groups: &[GroupTapeV1<'_>],
    part: BackwardPartV1<'_>,
    group_count: f32,
    chunk_rows: usize,
) -> Vec<Vec<f32>> {
    let rows = reference_rows_v1(groups, part, group_count);
    let mut level = rows
        .chunks(chunk_rows)
        .map(|chunk| reference_sum_v1(parameters, chunk))
        .collect::<Vec<_>>();
    while level.len() > 1 {
        let mut next = Vec::new();
        let mut nodes = level.into_iter();
        while let Some(mut left) = nodes.next() {
            if let Some(right) = nodes.next() {
                for (left, right) in left.iter_mut().zip(&right) {
                    for (left, right) in left.iter_mut().zip(right) {
                        *left += *right;
                    }
                }
            }
            next.push(left);
        }
        level = next;
    }
    level.pop().unwrap_or_else(|| zeros_v1(parameters))
}

fn fixed_tree_v1(
    parameters: &[NativeNamedParameterV1],
    groups: &[GroupTapeV1<'_>],
    part: BackwardPartV1<'_>,
    workers: usize,
) -> Vec<Vec<f32>> {
    let (result, runs) = with_fixed_tree_hooks_for_test_v1(FixedTreeHooksForTestV1::NONE, || {
        fixed_tree_backward_dims_v1::<Net8DimsV1>(
            parameters,
            groups,
            part,
            groups.len() as f32,
            VC,
            workers,
            MATH,
            ThreadMxcsrCheckV1::Off,
        )
    });
    assert_eq!(runs, 1);
    result.unwrap().0
}

/// The constant is the contract's, and the chunk count follows from the rows
/// alone: nothing about threads enters either.
#[test]
fn the_chunk_size_is_a_constant_of_the_contract() {
    // The identity names the size (the module asserts the two agree).
    assert!(FIXED_TREE_BACKWARD_NUMERICAL_BACKEND_IDENTITY_V1
        .ends_with(&format!("fixed-tree-rows{FIXED_TREE_CHUNK_ROWS_V1}-v1")));
    for (rows, chunks) in [
        (0, 0),
        (1, 1),
        (63, 1),
        (64, 1),
        (65, 2),
        (128, 2),
        (129, 3),
        (6_000, 94),
    ] {
        assert_eq!(fixed_tree_chunk_count_v1(rows), chunks, "{rows}");
    }
    // Widths of the levels over seven and over five chunks.
    assert_eq!(
        (0..4)
            .map(|level| level_width_v1(7, level))
            .collect::<Vec<_>>(),
        [7, 4, 2, 1]
    );
    assert_eq!(
        (0..4)
            .map(|level| level_width_v1(5, level))
            .collect::<Vec<_>>(),
        [5, 3, 2, 1]
    );
}

/// The gradients are the contract's reference, bit for bit, at every worker
/// count, for the whole loss and for each part of it: seven chunks (the
/// last of fifteen rows), with card rows only some chunks hold.
#[test]
fn the_fixed_tree_is_its_definition_bit_for_bit_at_every_worker_count() {
    let fixture = tapes_v1();
    let groups = groups_v1(&fixture, 200);
    let group_count = groups.len() as f32;
    assert_eq!(row_count_v1(&groups), 399);
    assert_eq!(fixed_tree_chunk_count_v1(399), 7);
    let marks = (0..groups.len())
        .map(|group| group % 5 == 1)
        .collect::<Vec<_>>();
    for part in [
        BackwardPartV1::All,
        BackwardPartV1::Rest(&marks),
        BackwardPartV1::Announcement(&marks),
    ] {
        let expected = reference_tree_v1(
            &fixture.parameters,
            &groups,
            part,
            group_count,
            FIXED_TREE_CHUNK_ROWS_V1,
        );
        for workers in WORKER_COUNTS {
            let actual = fixed_tree_v1(&fixture.parameters, &groups, part, workers);
            assert_eq!(bits_v1(&actual), bits_v1(&expected), "workers {workers}");
        }
    }
    // The whole loss reaches every kind of tensor, and the tree is not the
    // sequential order: its bits differ from one serial sum of every row.
    let tree = fixed_tree_v1(&fixture.parameters, &groups, BackwardPartV1::All, 4);
    for index in [
        CARD_EMBEDDING,
        EDGE_FIRST_WEIGHT,
        ACTION_REF_FIRST_WEIGHT,
        SCORER_SECOND_WEIGHT,
        VALUE_SECOND_BIAS,
    ] {
        assert!(tree[index].iter().any(|value| *value != 0.0), "{index}");
    }
    let serial = reference_sum_v1(
        &fixture.parameters,
        &reference_rows_v1(&groups, BackwardPartV1::All, group_count),
    );
    assert_ne!(bits_v1(&tree), bits_v1(&serial));
}

/// The card rows of the fixture exercise both cases of a combination: rows
/// both nodes hold and rows only one holds. One group alone names the cards
/// of tape 3, so exactly one chunk holds them and they travel up the tree
/// without an addition; their bits are that chunk's serial sum.
#[test]
fn a_card_row_one_chunk_holds_moves_up_the_tree_unchanged() {
    let fixture = tapes_v1();
    let groups = groups_v1(&fixture, 200);
    let group_count = groups.len() as f32;
    let rows = reference_rows_v1(&groups, BackwardPartV1::All, group_count);
    let single_tape = &fixture.tapes[SINGLE_GROUP_TAPE];
    let holding = rows
        .chunks(FIXED_TREE_CHUNK_ROWS_V1)
        .enumerate()
        .filter(|(_, chunk)| {
            chunk.iter().any(|(selected, _, _)| {
                std::ptr::eq::<DecisionTapeV1>(&*selected.tape, single_tape)
            })
        })
        .map(|(chunk, _)| chunk)
        .collect::<Vec<_>>();
    assert_eq!(holding.len(), 1, "{holding:?}");
    let chunk_sum = reference_sum_v1(
        &fixture.parameters,
        rows.chunks(FIXED_TREE_CHUNK_ROWS_V1)
            .nth(holding[0])
            .unwrap(),
    );
    let tree = fixed_tree_v1(&fixture.parameters, &groups, BackwardPartV1::All, 3);
    let width = CARD_EMBEDDING_DIM_V1;
    let mut checked = 0;
    for token in single_tape.object_card_ids_v1().iter().copied() {
        if token == 0 {
            continue;
        }
        let row = token * width..(token + 1) * width;
        assert_eq!(
            bits_v1(&[tree[CARD_EMBEDDING][row.clone()].to_vec()]),
            bits_v1(&[chunk_sum[CARD_EMBEDDING][row].to_vec()]),
            "card {token}"
        );
        checked += 1;
    }
    assert!(checked > 0);
    // A row no chunk named is +0.0, as the sequential pass leaves it.
    let unnamed = (1..CARD_VOCAB_SIZE_V1)
        .find(|token| {
            fixture
                .tapes
                .iter()
                .all(|tape| !tape.object_card_ids_v1().contains(token))
        })
        .unwrap();
    assert!(tree[CARD_EMBEDDING][unnamed * width..(unnamed + 1) * width]
        .iter()
        .all(|value| value.to_bits() == 0));
}

/// A batch of at most one chunk is the sequential backward bit for bit: the
/// chunk's sum is every row in order through the sequential sink, and the
/// tree has nothing to add.
#[test]
fn a_batch_of_one_chunk_is_the_sequential_sum_bit_for_bit() {
    let fixture = tapes_v1();
    for group_count in [1, 7, 32] {
        let groups = groups_v1(&fixture, group_count);
        assert!(row_count_v1(&groups) <= FIXED_TREE_CHUNK_ROWS_V1);
        let serial = reference_sum_v1(
            &fixture.parameters,
            &reference_rows_v1(&groups, BackwardPartV1::All, groups.len() as f32),
        );
        for workers in [1, 4] {
            let tree = fixed_tree_v1(&fixture.parameters, &groups, BackwardPartV1::All, workers);
            assert_eq!(bits_v1(&tree), bits_v1(&serial), "{group_count} {workers}");
        }
    }
}

/// The chunk size is part of the result. With the size a thread count would
/// give (the rows shared evenly among the workers) the gradients are the
/// reference's for that size and differ from the contract's, for every
/// worker count but the one whose share happens to be the constant. So the
/// equality of all worker counts above cannot hold for an implementation
/// whose chunks follow its threads.
#[test]
fn a_chunk_size_that_followed_the_worker_count_would_change_the_bits() {
    let fixture = tapes_v1();
    let groups = groups_v1(&fixture, 200);
    let group_count = groups.len() as f32;
    let rows = row_count_v1(&groups);
    let contract = fixed_tree_v1(&fixture.parameters, &groups, BackwardPartV1::All, 4);
    let mut seen = vec![bits_v1(&contract)];
    for workers in WORKER_COUNTS {
        let share = rows.div_ceil(workers);
        assert_ne!(share, FIXED_TREE_CHUNK_ROWS_V1);
        let hooks = FixedTreeHooksForTestV1 {
            chunk_rows: Some(share),
            ..FixedTreeHooksForTestV1::NONE
        };
        let (result, runs) = with_fixed_tree_hooks_for_test_v1(hooks, || {
            fixed_tree_backward_dims_v1::<Net8DimsV1>(
                &fixture.parameters,
                &groups,
                BackwardPartV1::All,
                group_count,
                VC,
                workers,
                MATH,
                ThreadMxcsrCheckV1::Off,
            )
        });
        assert_eq!(runs, 1);
        let followed = bits_v1(&result.unwrap().0);
        let expected = reference_tree_v1(
            &fixture.parameters,
            &groups,
            BackwardPartV1::All,
            group_count,
            share,
        );
        assert_eq!(followed, bits_v1(&expected), "workers {workers}");
        // Five thread counts would have trained five different states, none
        // of them the contract's.
        assert!(!seen.contains(&followed), "workers {workers}");
        seen.push(followed);
    }
}

/// Terms the reference sink adds to each element, as magnitudes: what the
/// rounding bound of a sum is stated in. The gradients it also keeps are the
/// reference's own.
struct AbsoluteTermsV1 {
    gradients: Vec<Vec<f32>>,
    /// The sum of the magnitudes of each element's binary32 terms.
    magnitude: Vec<Vec<f64>>,
    /// How many terms each element receives.
    terms: Vec<Vec<u32>>,
}

impl<'t> ReverseGradientSinkV1<'t> for AbsoluteTermsV1 {
    fn linear_v1(
        &mut self,
        parameters: &[NativeNamedParameterV1],
        weight_index: usize,
        bias_index: usize,
        input: &'t [f32],
        rows: usize,
        input_dim: usize,
        output_dim: usize,
        d_output: &[f32],
        d_input: &mut Vec<f32>,
    ) -> Result<(), NativePolicyTrainErrorV1> {
        AccumulateGradientsV1(&mut self.gradients).linear_v1(
            parameters,
            weight_index,
            bias_index,
            input,
            rows,
            input_dim,
            output_dim,
            d_output,
            d_input,
        )?;
        for row in 0..rows {
            for output in 0..output_dim {
                let d_value = d_output[row * output_dim + output];
                self.magnitude[bias_index][output] += f64::from(d_value.abs());
                self.terms[bias_index][output] += 1;
                for column in 0..input_dim {
                    // The binary32 product the kernels add.
                    let term = d_value * input[row * input_dim + column];
                    let element = output * input_dim + column;
                    self.magnitude[weight_index][element] += f64::from(term.abs());
                    self.terms[weight_index][element] += 1;
                }
            }
        }
        Ok(())
    }

    fn card_embedding_dims_v1<D: NetDimsV1>(
        &mut self,
        object_card_ids: &'t [usize],
        d_object_input: &[f32],
    ) {
        AccumulateGradientsV1(&mut self.gradients)
            .card_embedding_dims_v1::<D>(object_card_ids, d_object_input);
        for (object, token) in object_card_ids.iter().copied().enumerate() {
            if token == 0 {
                continue;
            }
            let source = object * D::OBJECT_ENCODER_INPUT + OBJECT_FEATURE_DIM_V1;
            for embedding in 0..D::CARD_EMBEDDING {
                let element = token * D::CARD_EMBEDDING + embedding;
                self.magnitude[CARD_EMBEDDING][element] +=
                    f64::from(d_object_input[source + embedding].abs());
                self.terms[CARD_EMBEDDING][element] += 1;
            }
        }
    }
}

/// `gamma(k) = k u / (1 - k u)` with the binary32 unit roundoff `u = 2^-24`.
fn gamma_v1(operations: u32) -> f64 {
    let scaled = f64::from(operations) * (f64::from(f32::EPSILON) / 2.0);
    assert!(scaled < 1.0);
    scaled / (1.0 - scaled)
}

/// The stated rounding bound, on a small batch. The fixed tree and the
/// sequential pass add the same binary32 terms to an element, in different
/// orders. A sum of `n` terms in any order, by `n - 1` rounded additions, is
/// within `gamma(n - 1) * sum(|term|)` of the exact sum, so the two results
/// differ by at most `2 * gamma(n - 1) * sum(|term|)`, element by element.
/// (An element of one term, or of terms that are all zero, is the same bits.)
#[test]
fn the_fixed_tree_is_within_the_rounding_bound_of_the_sequential_sum() {
    let fixture = tapes_v1();
    // 120 rows: one full chunk and a second of 56.
    let groups = groups_v1(&fixture, 60);
    let group_count = groups.len() as f32;
    assert_eq!(row_count_v1(&groups), 120);
    let mut sink = AbsoluteTermsV1 {
        gradients: zeros_v1(&fixture.parameters),
        magnitude: fixture
            .parameters
            .iter()
            .map(|parameter| vec![0.0f64; parameter.values.len()])
            .collect(),
        terms: fixture
            .parameters
            .iter()
            .map(|parameter| vec![0u32; parameter.values.len()])
            .collect(),
    };
    let mut workspace = ReverseWorkspaceV1::default();
    for (selected, d_joint_log_probability, d_value) in
        reference_rows_v1(&groups, BackwardPartV1::All, group_count)
    {
        prepare_reverse_logits_v1(selected, d_joint_log_probability, &mut workspace, MATH);
        reverse_decision_into_dims_v1::<Net8DimsV1>(
            &fixture.parameters,
            &mut sink,
            &selected.tape,
            &workspace.d_logits,
            d_value,
            &mut workspace.decision,
        )
        .unwrap();
    }
    let serial = &sink.gradients;
    let tree = fixed_tree_v1(&fixture.parameters, &groups, BackwardPartV1::All, 3);
    let (mut differing, mut compared, mut largest_share) = (0usize, 0usize, 0.0f64);
    for (index, (tree, serial)) in tree.iter().zip(serial).enumerate() {
        assert_eq!(tree.len(), serial.len());
        for (element, (tree, serial)) in tree.iter().zip(serial).enumerate() {
            let terms = sink.terms[index][element];
            let difference = (f64::from(*tree) - f64::from(*serial)).abs();
            // The magnitudes were summed in binary64; allow for that.
            let bound = 2.0
                * gamma_v1(terms.saturating_sub(1))
                * sink.magnitude[index][element]
                * (1.0 + 1.0e-9);
            assert!(
                difference <= bound,
                "tensor {index} element {element}: {tree} against {serial}, {terms} terms, \
                 bound {bound}"
            );
            compared += 1;
            if tree.to_bits() != serial.to_bits() {
                differing += 1;
                largest_share = largest_share.max(difference / bound);
            }
        }
    }
    // The comparison is not empty: the orders do differ in some last bits,
    // and by less than the bound allows.
    assert!(compared > 1_000_000, "{compared}");
    assert!(differing > 100, "{differing}");
    assert!(largest_share <= 1.0, "{largest_share}");
}

/// A pass that visits no group (an announcement pass without a marked
/// group) has no chunk and gives +0.0 everywhere.
#[test]
fn a_pass_without_rows_gives_positive_zeros() {
    let fixture = tapes_v1();
    let groups = groups_v1(&fixture, 9);
    let marks = vec![false; groups.len()];
    let gradients = fixed_tree_v1(
        &fixture.parameters,
        &groups,
        BackwardPartV1::Announcement(&marks),
        4,
    );
    assert_eq!(gradients.len(), fixture.parameters.len());
    for (gradient, parameter) in gradients.iter().zip(&fixture.parameters) {
        assert_eq!(gradient.len(), parameter.values.len());
        assert!(gradient.iter().all(|value| value.to_bits() == 0));
    }
}

/// The first failure reported is the one the sequential loop would reach
/// first: an earlier row ahead of a later one whatever chunks they are in,
/// and a row's gauge observation ahead of its backward.
#[test]
fn the_first_failure_is_reported_in_visiting_order() {
    let fixture = tapes_v1();
    let groups = groups_v1(&fixture, 200);
    let group_count = groups.len() as f32;
    let failure = |gauge_error_at, backward_error_at, backward_panic_at| {
        let hooks = FixedTreeHooksForTestV1 {
            chunk_rows: None,
            gauge_error_at,
            backward_error_at,
            backward_panic_at,
        };
        let (result, runs) = with_fixed_tree_hooks_for_test_v1(hooks, || {
            fixed_tree_backward_dims_v1::<Net8DimsV1>(
                &fixture.parameters,
                &groups,
                BackwardPartV1::All,
                group_count,
                VC,
                4,
                MATH,
                ThreadMxcsrCheckV1::Off,
            )
        });
        assert_eq!(runs, 1);
        match result {
            Ok(_) => panic!("the injected failure was not reported"),
            Err(error) => format!("{error:?}"),
        }
    };
    let backward = |index: usize| {
        format!(
            "{:?}",
            NativePolicyTrainErrorV1::NonFinite {
                stage: "injected_fixed_tree_backward",
                index,
            }
        )
    };
    let gauge = format!("{:?}", NativePolicyTrainErrorV1::GaugeBoundOverflow);
    // The earlier row wins, in its own chunk or an earlier one.
    assert_eq!(failure(None, Some(300), None), backward(300));
    assert_eq!(failure(Some(301), Some(300), None), backward(300));
    assert_eq!(failure(Some(299), Some(300), None), gauge);
    assert_eq!(failure(Some(40), None, None), gauge);
    // A row's gauge observation comes before its backward.
    assert_eq!(failure(Some(300), Some(300), None), gauge);
    // A panic resumes on the caller when it comes first, and not otherwise.
    let panic = catch_unwind(AssertUnwindSafe(|| failure(None, Some(300), Some(100)))).unwrap_err();
    assert_eq!(
        panic.downcast_ref::<String>().map(String::as_str),
        Some("injected fixed-tree backward panic at row 100")
    );
    assert_eq!(failure(None, Some(100), Some(300)), backward(100));
}

/// xorshift64*: a fixed, dependency-free source of test values.
struct TestBitsV1(u64);

impl TestBitsV1 {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    fn below(&mut self, bound: usize) -> usize {
        (self.next() % bound as u64) as usize
    }

    /// Values over six orders of magnitude, so most sums depend on the order
    /// of their additions.
    fn values(&mut self, len: usize) -> Vec<f32> {
        (0..len)
            .map(|_| {
                let unit = (self.next() >> 40) as f32 / (1u64 << 24) as f32 - 0.5;
                unit * [1.0e-3, 1.0, 1.0e3][self.below(3)]
            })
            .collect()
    }
}

/// One synthetic chunk sum: a tensor in its own shape, a transposed one,
/// and the card rows the chunk holds.
struct SyntheticChunkV1 {
    own_shape: Vec<f32>,
    transposed: Vec<f32>,
    cards: Vec<(usize, Vec<f32>)>,
}

/// The tree alone, on synthetic nodes: whatever order the chunks finish in,
/// the root is the level-by-level reduction of the chunks in index order,
/// left plus right, for every chunk count from one to forty. Card rows are
/// added where both nodes hold them and carried over where one does.
#[test]
fn the_tree_gives_one_result_for_every_arrival_order() {
    const OWN_SHAPE: usize = 1;
    const TRANSPOSED: usize = 2;
    let shape = NodeShapeV1 {
        // No card tensor, a tensor in its own shape, and a transposed one
        // whose own-shape twin stays zero.
        gradient_lens: vec![0, 5, 6],
        input_major_lens: vec![0, 0, 6],
        hidden: 3,
        card_width: 2,
    };
    let mut bits = TestBitsV1(0x6a09_e667_f3bc_c908);
    let mut order_dependent = 0;
    for chunks in 1..=40usize {
        // Each chunk's sums, and the card rows it holds (of six cards).
        let mut sums = Vec::new();
        for _ in 0..chunks {
            let own_shape = bits.values(5);
            let transposed = bits.values(6);
            let mut cards = Vec::new();
            for token in 0..6usize {
                if bits.below(3) == 0 {
                    cards.push((token, bits.values(2)));
                }
            }
            sums.push(SyntheticChunkV1 {
                own_shape,
                transposed,
                cards,
            });
        }
        // Dense: five values, six values, then six card rows of two (a row
        // a chunk does not hold is +0.0).
        let dense = |sum: &SyntheticChunkV1| {
            let mut values = [
                sum.own_shape.clone(),
                sum.transposed.clone(),
                vec![0.0f32; 12],
            ]
            .concat();
            for (token, row) in &sum.cards {
                values[11 + token * 2..11 + token * 2 + 2].copy_from_slice(row);
            }
            values
        };
        let mut level = sums.iter().map(dense).collect::<Vec<_>>();
        let serial = level
            .iter()
            .skip(1)
            .fold(level[0].clone(), |mut sum, next| {
                for (sum, next) in sum.iter_mut().zip(next) {
                    *sum += *next;
                }
                sum
            });
        while level.len() > 1 {
            let mut next = Vec::new();
            let mut nodes = level.into_iter();
            while let Some(mut left) = nodes.next() {
                if let Some(right) = nodes.next() {
                    for (left, right) in left.iter_mut().zip(&right) {
                        *left += *right;
                    }
                }
                next.push(left);
            }
            level = next;
        }
        let expected = level.pop().unwrap();
        order_dependent += usize::from(
            expected
                .iter()
                .map(|value| value.to_bits())
                .collect::<Vec<_>>()
                != serial
                    .iter()
                    .map(|value| value.to_bits())
                    .collect::<Vec<_>>(),
        );

        for _ in 0..6 {
            let mut order = (0..chunks).collect::<Vec<_>>();
            for index in (1..order.len()).rev() {
                order.swap(index, bits.below(index + 1));
            }
            let tree = TreeV1::new_v1(&shape, chunks);
            for chunk in order {
                let mut node = tree.take_v1();
                assert!(node.card_slots.is_empty() && node.card_rows.is_empty());
                assert!(node
                    .tensors
                    .gradients
                    .iter()
                    .chain(&node.tensors.input_major)
                    .flatten()
                    .all(|value| value.to_bits() == 0));
                let sum = &sums[chunk];
                node.tensors.gradients[OWN_SHAPE].copy_from_slice(&sum.own_shape);
                node.tensors.input_major[TRANSPOSED].copy_from_slice(&sum.transposed);
                for (token, row) in &sum.cards {
                    node.card_slots.insert(*token, node.card_rows.len() / 2);
                    node.card_rows.extend_from_slice(row);
                }
                tree.complete_v1(0, chunk, node);
            }
            assert!(tree.waiting.lock().unwrap().is_empty());
            let root = tree.root.lock().unwrap().take().unwrap();
            let mut actual = [
                root.tensors.gradients[OWN_SHAPE].clone(),
                root.tensors.input_major[TRANSPOSED].clone(),
                vec![0.0f32; 12],
            ]
            .concat();
            for (token, slot) in &root.card_slots {
                actual[11 + token * 2..11 + token * 2 + 2]
                    .copy_from_slice(&root.card_rows[slot * 2..slot * 2 + 2]);
            }
            // The transposed tensor's own-shape twin was never written.
            assert!(root.tensors.gradients[TRANSPOSED]
                .iter()
                .all(|value| value.to_bits() == 0));
            assert_eq!(
                actual
                    .iter()
                    .map(|value| value.to_bits())
                    .collect::<Vec<_>>(),
                expected
                    .iter()
                    .map(|value| value.to_bits())
                    .collect::<Vec<_>>(),
                "{chunks} chunks"
            );
        }
    }
    // The values make the association visible: most trees are not the
    // serial sum of their chunks.
    assert!(order_dependent >= 30, "{order_dependent}");
}

/// Five chunks combine as `((c0 + c1) + (c2 + c3)) + c4`, shown with values
/// whose binary32 sum depends on the association.
#[test]
fn five_chunks_combine_as_the_documented_tree() {
    let shape = NodeShapeV1 {
        gradient_lens: vec![0, 1],
        input_major_lens: vec![0, 0],
        hidden: 1,
        card_width: 1,
    };
    let chunks: [f32; 5] = [1.0e8, 1.0, -1.0e8, 1.0, 1.0];
    let documented = ((chunks[0] + chunks[1]) + (chunks[2] + chunks[3])) + chunks[4];
    let serial = chunks.iter().fold(0.0f32, |sum, chunk| sum + chunk);
    assert_ne!(documented.to_bits(), serial.to_bits());
    for order in [[0, 1, 2, 3, 4], [4, 3, 2, 1, 0], [2, 4, 0, 3, 1]] {
        let tree = TreeV1::new_v1(&shape, chunks.len());
        for chunk in order {
            let mut node = tree.take_v1();
            node.tensors.gradients[1][0] = chunks[chunk];
            tree.complete_v1(0, chunk, node);
        }
        let root = tree.root.lock().unwrap().take().unwrap();
        assert_eq!(root.tensors.gradients[1][0].to_bits(), documented.to_bits());
    }
}
