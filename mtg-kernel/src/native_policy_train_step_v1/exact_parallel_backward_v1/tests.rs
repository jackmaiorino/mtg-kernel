use super::*;

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

/// Every tensor in the real order, with small random shapes.
fn synthetic_parameters_v1(bits: &mut TestBitsV1) -> Vec<NativeNamedParameterV1> {
    let tensor = |shape: Vec<usize>, bits: &mut TestBitsV1| NativeNamedParameterV1 {
        name: "synthetic",
        values: bits.values(shape.iter().product()),
        shape,
    };
    let mut parameters = vec![tensor(
        vec![2 + bits.below(40), CARD_EMBEDDING_DIM_V1],
        bits,
    )];
    while parameters.len() < PARAMETER_TENSOR_COUNT {
        let (output_dim, input_dim) = (1 + bits.below(6), 1 + bits.below(6));
        parameters.push(tensor(vec![output_dim, input_dim], bits));
        parameters.push(tensor(vec![output_dim], bits));
    }
    parameters
}

enum SinkCallV1 {
    Linear {
        weight_index: usize,
        rows: usize,
        input: Vec<f32>,
        d_output: Vec<f32>,
    },
    Cards {
        object_card_ids: Vec<usize>,
        d_object_input: Vec<f32>,
    },
}

/// Substeps of random layer calls (repeated layers, zero rows, the scorer's
/// reversed bias) and card calls (padding and repeated tokens).
fn synthetic_substeps_v1(
    parameters: &[NativeNamedParameterV1],
    bits: &mut TestBitsV1,
) -> Vec<Vec<SinkCallV1>> {
    let vocabulary = parameters[CARD_EMBEDDING].shape[0];
    (0..1 + bits.below(12))
        .map(|_| {
            (0..1 + bits.below(10))
                .map(|_| {
                    if bits.below(4) == 0 {
                        let objects = bits.below(6);
                        SinkCallV1::Cards {
                            object_card_ids: (0..objects).map(|_| bits.below(vocabulary)).collect(),
                            d_object_input: bits.values(objects * OBJECT_ENCODER_INPUT),
                        }
                    } else {
                        let weight_index = 1 + 2 * bits.below(PARAMETER_TENSOR_COUNT / 2);
                        let shape = &parameters[weight_index].shape;
                        let rows = bits.below(5);
                        SinkCallV1::Linear {
                            weight_index,
                            rows,
                            input: bits.values(rows * shape[1]),
                            d_output: bits.values(rows * shape[0]),
                        }
                    }
                })
                .collect()
        })
        .collect()
}

/// Runs `calls` on `sink`, returning every layer's input gradient.
fn run_calls_v1<'t>(
    parameters: &[NativeNamedParameterV1],
    calls: &'t [SinkCallV1],
    sink: &mut impl ReverseGradientSinkV1<'t>,
) -> Vec<Vec<f32>> {
    let mut d_inputs = Vec::new();
    for call in calls {
        match call {
            SinkCallV1::Linear {
                weight_index,
                rows,
                input,
                d_output,
            } => {
                let shape = &parameters[*weight_index].shape;
                let mut d_input = Vec::new();
                sink.linear_v1(
                    parameters,
                    *weight_index,
                    weight_index + 1,
                    input,
                    *rows,
                    shape[1],
                    shape[0],
                    d_output,
                    &mut d_input,
                )
                .unwrap();
                d_inputs.push(d_input);
            }
            SinkCallV1::Cards {
                object_card_ids,
                d_object_input,
            } => sink.card_embedding_v1(object_card_ids, d_object_input),
        }
    }
    d_inputs
}

fn bits_v1(tensors: &[Vec<f32>]) -> Vec<Vec<u32>> {
    tensors
        .iter()
        .map(|values| values.iter().map(|value| value.to_bits()).collect())
        .collect()
}

#[test]
fn row_blocks_replay_the_sequential_additions_bit_for_bit() {
    let mut bits = TestBitsV1(0x9e37_79b9_7f4a_7c15);
    let (mut scorer_bias_split, mut card_rows_split) = (false, false);
    for _ in 0..300 {
        let parameters = synthetic_parameters_v1(&mut bits);
        let substeps = synthetic_substeps_v1(&parameters, &mut bits);
        let zeros = || {
            parameters
                .iter()
                .map(|parameter| vec![0.0f32; parameter.values.len()])
                .collect::<Vec<_>>()
        };

        let mut expected = zeros();
        let mut expected_d_inputs = Vec::new();
        for calls in &substeps {
            expected_d_inputs.extend(run_calls_v1(
                &parameters,
                calls,
                &mut AccumulateGradientsV1(&mut expected),
            ));
        }

        let mut terms = Vec::new();
        terms.resize_with(substeps.len(), SubstepTermsV1::default);
        let mut recorded_d_inputs = Vec::new();
        for (calls, terms) in substeps.iter().zip(&mut terms) {
            let mut sink = RecordTermsV1 {
                terms,
                card_embedding_len: parameters[CARD_EMBEDDING].values.len(),
            };
            recorded_d_inputs.extend(run_calls_v1(&parameters, calls, &mut sink));
        }
        assert_eq!(bits_v1(&recorded_d_inputs), bits_v1(&expected_d_inputs));

        for workers in [2, 3, 7, 1_000] {
            let mut gradients = GradientTensorsV1::zeroed_dims_v1::<Net8DimsV1>(&parameters);
            let mut blocks = gradient_blocks_v1(&parameters, &mut gradients, &terms, workers);
            for block in &blocks {
                scorer_bias_split |= block.tensor == SCORER_SECOND_BIAS && block.rows.start > 0;
                card_rows_split |= block.tensor == CARD_EMBEDDING && block.rows.start > 0;
            }
            // Threads take blocks in any order.
            for index in (1..blocks.len()).rev() {
                blocks.swap(index, bits.below(index + 1));
            }
            for block in blocks {
                replay_block_v1(block, &terms);
            }
            assert_eq!(bits_v1(&gradients.into_gradients_v1()), bits_v1(&expected));
        }
    }
    assert!(scorer_bias_split && card_rows_split);
}

/// How a hidden-width test draws its inputs and output gradients.
#[derive(Clone, Copy, PartialEq, Eq)]
enum HiddenWidthValuesV1 {
    /// Finite values; most inputs are zeros of either sign or denormal.
    Sparse,
    /// `Sparse` with infinities and NaNs among the inputs and the output
    /// gradients, and values large enough to overflow.
    NotFinite,
}

/// Every tensor in the real order. Half of the weights have the hidden
/// width of outputs, as the encoders' layers do, so phase B accumulates them
/// transposed; the rest stay small.
fn hidden_width_parameters_v1(bits: &mut TestBitsV1, hidden: usize) -> Vec<NativeNamedParameterV1> {
    let tensor = |shape: Vec<usize>, bits: &mut TestBitsV1| NativeNamedParameterV1 {
        name: "synthetic",
        values: bits.values(shape.iter().product()),
        shape,
    };
    let mut parameters = vec![tensor(
        vec![2 + bits.below(40), CARD_EMBEDDING_DIM_V1],
        bits,
    )];
    while parameters.len() < PARAMETER_TENSOR_COUNT {
        let (output_dim, input_dim) = if bits.below(2) == 0 {
            (hidden, 1 + bits.below(40))
        } else {
            (1 + bits.below(6), 1 + bits.below(6))
        };
        parameters.push(tensor(vec![output_dim, input_dim], bits));
        parameters.push(tensor(vec![output_dim], bits));
    }
    parameters
}

fn hidden_width_substeps_v1(
    parameters: &[NativeNamedParameterV1],
    bits: &mut TestBitsV1,
    values: HiddenWidthValuesV1,
) -> Vec<Vec<SinkCallV1>> {
    let not_finite = values == HiddenWidthValuesV1::NotFinite;
    (0..1 + bits.below(8))
        .map(|_| {
            (0..1 + bits.below(10))
                .map(|_| {
                    let weight_index = 1 + 2 * bits.below(PARAMETER_TENSOR_COUNT / 2);
                    let shape = &parameters[weight_index].shape;
                    let rows = bits.below(5);
                    let mut input = bits.values(rows * shape[1]);
                    for value in &mut input {
                        *value = match bits.below(16) {
                            0..=4 => 0.0,
                            5..=8 => -0.0,
                            9 => f32::from_bits(1 + bits.below(1 << 20) as u32),
                            10 if not_finite => {
                                [f32::INFINITY, f32::NEG_INFINITY, f32::NAN][bits.below(3)]
                            }
                            11 if not_finite => *value * 1.0e35,
                            _ => *value,
                        };
                    }
                    let mut d_output = bits.values(rows * shape[0]);
                    // One layer call in four has an output gradient that is
                    // not finite, which keeps every term of that call.
                    if not_finite && !d_output.is_empty() && bits.below(4) == 0 {
                        let at = bits.below(d_output.len());
                        d_output[at] =
                            [f32::INFINITY, f32::NEG_INFINITY, f32::NAN, 3.0e38][bits.below(4)];
                    }
                    SinkCallV1::Linear {
                        weight_index,
                        rows,
                        input,
                        d_output,
                    }
                })
                .collect()
        })
        .collect()
}

/// A NaN's sign and payload depend on the order of an instruction's
/// operands when two NaNs meet, so NaNs compare as one value.
fn bits_with_one_nan_v1(tensors: &[Vec<f32>]) -> Vec<Vec<u32>> {
    tensors
        .iter()
        .map(|values| {
            values
                .iter()
                .map(|value| {
                    if value.is_nan() {
                        f32::NAN.to_bits()
                    } else {
                        value.to_bits()
                    }
                })
                .collect()
        })
        .collect()
}

/// The sequential sink's gradients and the transposed replay's for the same
/// calls, with the count of layer rows whose zero inputs were passed over.
fn sequential_and_replayed_v1(
    bits: &mut TestBitsV1,
    hidden: usize,
    values: HiddenWidthValuesV1,
    workers: usize,
) -> (Vec<Vec<f32>>, Vec<Vec<f32>>, u64) {
    let parameters = hidden_width_parameters_v1(bits, hidden);
    let substeps = hidden_width_substeps_v1(&parameters, bits, values);
    let mut expected = parameters
        .iter()
        .map(|parameter| vec![0.0f32; parameter.values.len()])
        .collect::<Vec<_>>();
    for calls in &substeps {
        run_calls_v1(
            &parameters,
            calls,
            &mut AccumulateGradientsV1(&mut expected),
        );
    }
    let mut terms = Vec::new();
    terms.resize_with(substeps.len(), SubstepTermsV1::default);
    for (calls, terms) in substeps.iter().zip(&mut terms) {
        let mut sink = RecordTermsV1 {
            terms,
            card_embedding_len: parameters[CARD_EMBEDDING].values.len(),
        };
        run_calls_v1(&parameters, calls, &mut sink);
    }
    let passed_before = ZERO_INPUT_ROWS_PASSED_FOR_TEST_V1.with(Cell::get);
    let mut gradients = match hidden {
        HIDDEN_DIM_V1 => GradientTensorsV1::zeroed_dims_v1::<Net8DimsV1>(&parameters),
        _ => GradientTensorsV1::zeroed_dims_v1::<Net8W128DimsV1>(&parameters),
    };
    assert!(gradients
        .input_major
        .iter()
        .any(|tensor| !tensor.is_empty()));
    // Two blocks of substeps, as a long update replays them.
    let (early, late) = terms.split_at(terms.len() / 2);
    for terms in [early, late] {
        let mut blocks = match hidden {
            HIDDEN_DIM_V1 => {
                gradient_blocks_dims_v1::<Net8DimsV1>(&parameters, &mut gradients, terms, workers)
            }
            _ => gradient_blocks_dims_v1::<Net8W128DimsV1>(
                &parameters,
                &mut gradients,
                terms,
                workers,
            ),
        };
        for index in (1..blocks.len()).rev() {
            blocks.swap(index, bits.below(index + 1));
        }
        for block in blocks {
            match hidden {
                HIDDEN_DIM_V1 => replay_block_dims_v1::<Net8DimsV1>(block, terms),
                _ => replay_block_dims_v1::<Net8W128DimsV1>(block, terms),
            }
        }
    }
    let passed = ZERO_INPUT_ROWS_PASSED_FOR_TEST_V1.with(Cell::get) - passed_before;
    (expected, gradients.into_gradients_v1(), passed)
}

/// Hidden-width weights accumulate transposed and pass over inputs that are
/// zero, and still give the sequential sink's gradients bit for bit: with
/// zeros of both signs and denormals among the inputs, at both hidden
/// widths, with the tensors split into any number of blocks.
#[test]
fn transposed_blocks_pass_over_zero_inputs_and_keep_every_bit() {
    let mut bits = TestBitsV1(0x243f_6a88_85a3_08d3);
    let mut passed_rows = 0;
    for round in 0..120 {
        let hidden = [HIDDEN_DIM_V1, W_HIDDEN_DIM_V1][round % 2];
        let workers = [1, 2, 3, 7, 1_000][round % 5];
        let (expected, replayed, passed) =
            sequential_and_replayed_v1(&mut bits, hidden, HiddenWidthValuesV1::Sparse, workers);
        assert_eq!(bits_v1(&replayed), bits_v1(&expected), "round {round}");
        passed_rows += passed;
    }
    // The guard held, so the zero inputs really were passed over.
    #[cfg(target_arch = "x86_64")]
    assert!(passed_rows > 1_000, "{passed_rows}");
    #[cfg(not(target_arch = "x86_64"))]
    assert_eq!(passed_rows, 0);
}

/// Infinities, NaNs and overflow among the inputs and the output gradients
/// leave every element what the sequential sink makes it: the same bits, or
/// a NaN where that is a NaN. A layer call whose output gradient is not
/// finite keeps every term, because zero times it is not a zero.
#[test]
fn transposed_blocks_keep_values_that_are_not_finite() {
    let mut bits = TestBitsV1(0x1319_8a2e_0370_7344);
    let (mut nans, mut infinities) = (0, 0);
    for round in 0..120 {
        let hidden = [HIDDEN_DIM_V1, W_HIDDEN_DIM_V1][round % 2];
        let workers = [1, 2, 3, 7, 1_000][round % 5];
        let (expected, replayed, _) =
            sequential_and_replayed_v1(&mut bits, hidden, HiddenWidthValuesV1::NotFinite, workers);
        assert_eq!(
            bits_with_one_nan_v1(&replayed),
            bits_with_one_nan_v1(&expected),
            "round {round}"
        );
        for value in expected.iter().flatten() {
            nans += usize::from(value.is_nan());
            infinities += usize::from(value.is_infinite());
        }
    }
    assert!(nans > 100 && infinities > 100, "{nans} {infinities}");
}

/// A thread that does not round to nearest adds every term: there a zero
/// added to a zero can change its sign, so the sequential sink's bits need
/// the zero terms too.
#[cfg(target_arch = "x86_64")]
#[test]
fn a_thread_that_does_not_round_to_nearest_adds_every_term() {
    use crate::deterministic_math_v1::{read_mxcsr_v1, write_mxcsr_v1};
    struct Restore(u32);
    impl Drop for Restore {
        fn drop(&mut self) {
            write_mxcsr_v1(self.0);
        }
    }
    let _restore = Restore(read_mxcsr_v1());
    let mut differing_from_nearest = 0;
    // Round down, round up, toward zero, and denormals read as zero.
    for dirty in [1 << 13, 2 << 13, 3 << 13, 1 << 6] {
        let seed = 0xa409_3822_299f_31d0;
        let (nearest, _, _) = sequential_and_replayed_v1(
            &mut TestBitsV1(seed),
            HIDDEN_DIM_V1,
            HiddenWidthValuesV1::Sparse,
            3,
        );
        write_mxcsr_v1(_restore.0 | dirty);
        let (expected, replayed, passed) = sequential_and_replayed_v1(
            &mut TestBitsV1(seed),
            HIDDEN_DIM_V1,
            HiddenWidthValuesV1::Sparse,
            3,
        );
        write_mxcsr_v1(_restore.0);
        assert_eq!(passed, 0, "{dirty:#x}");
        assert_eq!(bits_v1(&replayed), bits_v1(&expected), "{dirty:#x}");
        differing_from_nearest += usize::from(bits_v1(&expected) != bits_v1(&nearest));
    }
    // The states really change the sums this test compares.
    assert!(differing_from_nearest >= 3, "{differing_from_nearest}");
}

/// `linear_read_from_v1` leaves the columns from `first_read` on exactly as
/// the sequential sink leaves them, at widths that take the 64-column and
/// 16-column sums, the remainder loop, and each of them alone.
#[test]
fn the_input_gradient_from_a_first_read_column_matches_the_sequential_sink() {
    let mut bits = TestBitsV1(0x082e_fa98_ec4e_6c89);
    for (output_dim, input_dim) in [
        (64, 114),
        (64, 64),
        (64, 128),
        (64, 259),
        (64, 89),
        (64, 169),
        (64, 1499),
        (128, 130),
        (128, 2779),
        (1, 64),
        (5, 16),
        (5, 15),
        (3, 17),
        (2, 1),
    ] {
        let tensor = |shape: Vec<usize>, bits: &mut TestBitsV1| NativeNamedParameterV1 {
            name: "synthetic",
            values: bits.values(shape.iter().product()),
            shape,
        };
        let parameters = vec![
            tensor(vec![2, CARD_EMBEDDING_DIM_V1], &mut bits),
            tensor(vec![output_dim, input_dim], &mut bits),
            tensor(vec![output_dim], &mut bits),
        ];
        for rows in [0, 1, 3] {
            let input = bits.values(rows * input_dim);
            let d_output = bits.values(rows * output_dim);
            let mut gradients = parameters
                .iter()
                .map(|parameter| vec![0.0f32; parameter.values.len()])
                .collect::<Vec<_>>();
            let mut expected = Vec::new();
            AccumulateGradientsV1(&mut gradients)
                .linear_v1(
                    &parameters,
                    1,
                    2,
                    &input,
                    rows,
                    input_dim,
                    output_dim,
                    &d_output,
                    &mut expected,
                )
                .unwrap();
            let first_reads = [0, 1, 16, 25, 41, 98, 195, 219, input_dim / 2, input_dim];
            for first_read in first_reads.into_iter().filter(|first| *first <= input_dim) {
                let mut terms = SubstepTermsV1::default();
                // Stale values from an earlier, larger layer.
                let mut recorded = vec![7.0; 5];
                RecordTermsV1 {
                    terms: &mut terms,
                    card_embedding_len: parameters[CARD_EMBEDDING].values.len(),
                }
                .linear_read_from_v1(
                    &parameters,
                    1,
                    2,
                    &input,
                    rows,
                    input_dim,
                    output_dim,
                    &d_output,
                    first_read,
                    &mut recorded,
                )
                .unwrap();
                assert_eq!(recorded.len(), expected.len());
                for (recorded, expected) in recorded
                    .chunks_exact(input_dim)
                    .zip(expected.chunks_exact(input_dim))
                {
                    assert_eq!(
                        bits_v1(&[recorded[first_read..].to_vec()]),
                        bits_v1(&[expected[first_read..].to_vec()]),
                        "{output_dim}x{input_dim} rows={rows} first_read={first_read}"
                    );
                    // Test builds mark what was not computed.
                    assert!(recorded[..first_read].iter().all(|value| value.is_nan()));
                }
            }
        }
    }
}

/// The reference sink and the transposed sequential sink on the same calls.
struct ReferenceAndTransposedV1 {
    /// The gradients: the reference's, then the transposed sink's.
    gradients: [Vec<Vec<f32>>; 2],
    /// Every layer's input gradient, in the same order.
    d_inputs: [Vec<Vec<f32>>; 2],
    /// The layers accumulated transposed, and those of them whose zero
    /// inputs were passed over.
    layers: (u64, u64),
    /// Whether the scorer's second layer, whose bias takes its rows from the
    /// last one back, was accumulated transposed over two rows or more.
    reversed_bias: bool,
}

fn reference_and_transposed_v1(
    bits: &mut TestBitsV1,
    hidden: usize,
    values: HiddenWidthValuesV1,
) -> ReferenceAndTransposedV1 {
    use super::super::accumulate_transposed_v1::{
        transposed_layers_for_test_v1, AccumulateTransposedV1,
    };
    let parameters = hidden_width_parameters_v1(bits, hidden);
    let mut substeps = hidden_width_substeps_v1(&parameters, bits, values);
    // Cards as well: padding and repeated tokens.
    let objects = 1 + bits.below(6);
    let vocabulary = parameters[CARD_EMBEDDING].shape[0];
    substeps.push(vec![SinkCallV1::Cards {
        object_card_ids: (0..objects).map(|_| bits.below(vocabulary)).collect(),
        d_object_input: bits.values(objects * OBJECT_ENCODER_INPUT),
    }]);
    let reversed_bias = parameters[SCORER_SECOND_WEIGHT].shape[0] == hidden
        && substeps.iter().flatten().any(|call| {
            matches!(
                call,
                SinkCallV1::Linear { weight_index, rows, .. }
                    if *weight_index == SCORER_SECOND_WEIGHT && *rows >= 2
            )
        });

    let mut expected = parameters
        .iter()
        .map(|parameter| vec![0.0f32; parameter.values.len()])
        .collect::<Vec<_>>();
    let mut expected_d_inputs = Vec::new();
    for calls in &substeps {
        expected_d_inputs.extend(run_calls_v1(
            &parameters,
            calls,
            &mut AccumulateGradientsV1(&mut expected),
        ));
    }

    let before = transposed_layers_for_test_v1();
    let mut gradients = match hidden {
        HIDDEN_DIM_V1 => GradientTensorsV1::zeroed_dims_v1::<Net8DimsV1>(&parameters),
        _ => GradientTensorsV1::zeroed_dims_v1::<Net8W128DimsV1>(&parameters),
    };
    let mut sink = AccumulateTransposedV1::new_v1(&mut gradients);
    let mut d_inputs = Vec::new();
    for calls in &substeps {
        d_inputs.extend(run_calls_v1(&parameters, calls, &mut sink));
    }
    let after = transposed_layers_for_test_v1();
    ReferenceAndTransposedV1 {
        gradients: [expected, gradients.into_gradients_v1()],
        d_inputs: [expected_d_inputs, d_inputs],
        layers: (after.0 - before.0, after.1 - before.1),
        reversed_bias,
    }
}

/// The sequential sink that accumulates hidden-width gradients transposed
/// leaves every gradient and every input gradient exactly as the reference
/// sink leaves them, at both hidden widths, and passes over the zero inputs
/// of every layer while it does.
#[test]
fn the_transposed_sequential_sink_keeps_every_bit() {
    let mut bits = TestBitsV1(0x4528_21e6_38d0_1377);
    let (mut layers, mut passing, mut reversed_bias) = (0, 0, 0);
    for round in 0..120 {
        let hidden = [HIDDEN_DIM_V1, W_HIDDEN_DIM_V1][round % 2];
        let run = reference_and_transposed_v1(&mut bits, hidden, HiddenWidthValuesV1::Sparse);
        let [expected, actual] = &run.gradients;
        assert_eq!(bits_v1(actual), bits_v1(expected), "round {round}");
        let [expected, actual] = &run.d_inputs;
        assert_eq!(bits_v1(actual), bits_v1(expected), "round {round}");
        layers += run.layers.0;
        passing += run.layers.1;
        reversed_bias += usize::from(run.reversed_bias);
    }
    assert!(layers > 500, "{layers}");
    // The scorer's bias order was among the transposed layers.
    assert!(reversed_bias >= 5, "{reversed_bias}");
    #[cfg(target_arch = "x86_64")]
    assert_eq!(passing, layers);
    #[cfg(not(target_arch = "x86_64"))]
    assert_eq!(passing, 0);
}

/// With infinities, NaNs and overflow among the inputs and the output
/// gradients, every element is what the reference sink makes it: the same
/// bits, or a NaN where that is a NaN. A layer whose output gradient is not
/// finite keeps every term.
#[test]
fn the_transposed_sequential_sink_keeps_values_that_are_not_finite() {
    let mut bits = TestBitsV1(0xbe54_66cf_34e9_0c6c);
    let (mut nans, mut infinities) = (0, 0);
    let (mut layers, mut passing) = (0, 0);
    for round in 0..120 {
        let hidden = [HIDDEN_DIM_V1, W_HIDDEN_DIM_V1][round % 2];
        let run = reference_and_transposed_v1(&mut bits, hidden, HiddenWidthValuesV1::NotFinite);
        let [expected, actual] = &run.gradients;
        assert_eq!(
            bits_with_one_nan_v1(actual),
            bits_with_one_nan_v1(expected),
            "round {round}"
        );
        for value in expected.iter().flatten() {
            nans += usize::from(value.is_nan());
            infinities += usize::from(value.is_infinite());
        }
        let [expected, actual] = &run.d_inputs;
        assert_eq!(
            bits_with_one_nan_v1(actual),
            bits_with_one_nan_v1(expected),
            "round {round}"
        );
        layers += run.layers.0;
        passing += run.layers.1;
    }
    assert!(nans > 100 && infinities > 100, "{nans} {infinities}");
    // Some layers had an output gradient that is not finite.
    assert!(passing < layers, "{passing} {layers}");
}

/// A layer added outside the pinned state ends the passing over for the
/// rest of the backward. Rounding down, a cancellation and a negative zero
/// term both leave a negative zero, and back in the pinned state the next
/// zero term makes it positive: the reference's bits need that term.
#[cfg(target_arch = "x86_64")]
#[test]
fn a_layer_outside_the_pinned_state_ends_the_passing_over() {
    use super::super::accumulate_transposed_v1::{
        transposed_layers_for_test_v1, AccumulateTransposedV1,
    };
    use crate::deterministic_math_v1::{read_mxcsr_v1, write_mxcsr_v1};
    struct Restore(u32);
    impl Drop for Restore {
        fn drop(&mut self) {
            write_mxcsr_v1(self.0);
        }
    }
    let restore = Restore(read_mxcsr_v1());
    const ROUND_DOWN: u32 = 1 << 13;
    const HIDDEN: usize = HIDDEN_DIM_V1;
    let tensor = |shape: Vec<usize>| NativeNamedParameterV1 {
        name: "synthetic",
        values: vec![0.5; shape.iter().product()],
        shape,
    };
    let parameters = vec![
        tensor(vec![2, CARD_EMBEDDING_DIM_V1]),
        tensor(vec![HIDDEN, 2]),
        tensor(vec![HIDDEN]),
    ];
    let d_output = vec![1.0f32; HIDDEN];
    // In the pinned state, rounding down, and in the pinned state again.
    let layers: [(u32, [f32; 2]); 3] =
        [(0, [1.0, 0.0]), (ROUND_DOWN, [-1.0, -0.0]), (0, [0.0, 0.0])];
    let run = |sink: &mut dyn FnMut(&[f32], &mut Vec<f32>), count: usize| {
        let mut d_inputs = Vec::new();
        for (dirty, input) in &layers[..count] {
            let mut d_input = Vec::new();
            write_mxcsr_v1(restore.0 | dirty);
            sink(input, &mut d_input);
            write_mxcsr_v1(restore.0);
            d_inputs.push(d_input);
        }
        d_inputs
    };

    let mut expected = parameters
        .iter()
        .map(|parameter| vec![0.0f32; parameter.values.len()])
        .collect::<Vec<_>>();
    let expected_d_inputs = run(
        &mut |input, d_input| {
            AccumulateGradientsV1(&mut expected)
                .linear_v1(&parameters, 1, 2, input, 1, 2, HIDDEN, &d_output, d_input)
                .unwrap()
        },
        3,
    );
    // The reference ends on positive zeros.
    assert!(expected[1].iter().all(|value| value.to_bits() == 0));

    let before = transposed_layers_for_test_v1();
    let mut gradients = GradientTensorsV1::zeroed_dims_v1::<Net8DimsV1>(&parameters);
    let mut sink = AccumulateTransposedV1::new_v1(&mut gradients);
    let d_inputs = run(
        &mut |input, d_input| {
            sink.linear_v1(&parameters, 1, 2, input, 1, 2, HIDDEN, &d_output, d_input)
                .unwrap()
        },
        3,
    );
    let after = transposed_layers_for_test_v1();
    // Three layers, and only the first passed over its zero input.
    assert_eq!((after.0 - before.0, after.1 - before.1), (3, 1));
    assert_eq!(bits_v1(&gradients.into_gradients_v1()), bits_v1(&expected));
    assert_eq!(bits_v1(&d_inputs), bits_v1(&expected_d_inputs));

    // After the second layer every element is a negative zero, so passing
    // over the third layer's zero inputs would have kept the wrong sign.
    let mut gradients = GradientTensorsV1::zeroed_dims_v1::<Net8DimsV1>(&parameters);
    let mut sink = AccumulateTransposedV1::new_v1(&mut gradients);
    run(
        &mut |input, d_input| {
            sink.linear_v1(&parameters, 1, 2, input, 1, 2, HIDDEN, &d_output, d_input)
                .unwrap()
        },
        2,
    );
    let all_are =
        |values: &[f32], zero: f32| values.iter().all(|value| value.to_bits() == zero.to_bits());
    assert!(all_are(&gradients.input_major[1], -0.0));
    let third = &layers[2].1;
    add_input_major_layer_v1(
        HIDDEN,
        &mut gradients.input_major[1],
        third,
        2,
        &d_output,
        true,
    );
    assert!(all_are(&gradients.input_major[1], -0.0));
    // Every term added, they are the reference's positive zeros.
    add_input_major_layer_v1(
        HIDDEN,
        &mut gradients.input_major[1],
        third,
        2,
        &d_output,
        false,
    );
    assert!(all_are(&gradients.input_major[1], 0.0));
}

/// A layer call whose output width is not the one the transposed tensor
/// was laid out for is refused before any arithmetic.
#[test]
fn the_transposed_sequential_sink_refuses_another_width() {
    use super::super::accumulate_transposed_v1::AccumulateTransposedV1;
    let tensor = |shape: Vec<usize>| NativeNamedParameterV1 {
        name: "synthetic",
        values: vec![0.5; shape.iter().product()],
        shape,
    };
    let parameters = vec![
        tensor(vec![2, CARD_EMBEDDING_DIM_V1]),
        tensor(vec![HIDDEN_DIM_V1, 2 * HIDDEN_DIM_V1]),
        tensor(vec![2 * HIDDEN_DIM_V1]),
    ];
    let mut gradients = GradientTensorsV1::zeroed_dims_v1::<Net8DimsV1>(&parameters);
    let before = gradients.input_major[1].clone();
    assert!(!before.is_empty());
    let mut sink = AccumulateTransposedV1::new_v1(&mut gradients);
    // The same element count with the two dimensions exchanged, which the
    // reference's own checks let through.
    let input = vec![1.0f32; HIDDEN_DIM_V1];
    let d_output = vec![1.0f32; 2 * HIDDEN_DIM_V1];
    let mut d_input = Vec::new();
    let refused = sink.linear_v1(
        &parameters,
        1,
        2,
        &input,
        1,
        HIDDEN_DIM_V1,
        2 * HIDDEN_DIM_V1,
        &d_output,
        &mut d_input,
    );
    assert!(matches!(
        refused,
        Err(NativePolicyTrainErrorV1::ParameterManifest)
    ));
    assert!(d_input.is_empty());
    assert_eq!(
        bits_v1(&[gradients.input_major[1].clone()]),
        bits_v1(&[before])
    );
}

#[test]
fn a_card_token_outside_the_embedding_panics_like_the_sequential_sink() {
    let parameters = synthetic_parameters_v1(&mut TestBitsV1(7));
    let object_card_ids = vec![parameters[CARD_EMBEDDING].shape[0]];
    let d_object_input = vec![0.0; OBJECT_ENCODER_INPUT];
    let mut gradients = vec![Vec::new(); parameters.len()];
    gradients[CARD_EMBEDDING] = vec![0.0; parameters[CARD_EMBEDDING].values.len()];
    let sequential = catch_unwind(AssertUnwindSafe(|| {
        AccumulateGradientsV1(&mut gradients).card_embedding_v1(&object_card_ids, &d_object_input)
    }));
    let mut terms = SubstepTermsV1::default();
    let recorded = catch_unwind(AssertUnwindSafe(|| {
        RecordTermsV1 {
            terms: &mut terms,
            card_embedding_len: parameters[CARD_EMBEDDING].values.len(),
        }
        .card_embedding_v1(&object_card_ids, &d_object_input)
    }));
    assert!(sequential.is_err() && recorded.is_err());
}

/// Under the no-state-digest contract every helper verifies its own MXCSR
/// before taking an item: a dirty helper takes none, the remaining threads
/// drain every item, and its error is returned once all threads stop.
#[test]
fn a_dirty_helper_mxcsr_takes_no_item_and_fails_the_drain() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    for (check, dirty) in [
        (ThreadMxcsrCheckV1::Off, false),
        (ThreadMxcsrCheckV1::Verify, false),
        (ThreadMxcsrCheckV1::DirtyWorkersForTest, true),
    ] {
        let done = AtomicUsize::new(0);
        let result = drain_in_parallel_v1(
            (0..64).collect::<Vec<usize>>(),
            4,
            check,
            || (),
            |_, _| {
                done.fetch_add(1, Ordering::SeqCst);
            },
        );
        assert_eq!(done.load(Ordering::SeqCst), 64);
        if dirty {
            assert!(matches!(
                result,
                Err(NativePolicyTrainErrorV1::MxcsrPinnedState { observed })
                    if observed & (1 << 15) != 0
            ));
        } else {
            assert_eq!(result, Ok(()));
        }
    }
    assert!(crate::deterministic_math_v1::verify_pinned_mxcsr_state_v1().is_ok());
}
