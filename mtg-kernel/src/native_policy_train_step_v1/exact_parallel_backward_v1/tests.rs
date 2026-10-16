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
            let mut gradients = zeros();
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
            assert_eq!(bits_v1(&gradients), bits_v1(&expected));
        }
    }
    assert!(scorer_bias_split && card_rows_split);
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
