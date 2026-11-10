use super::super::parallel::{replay_verified_test_terms_v1, SGradientTermsV1};
use super::super::tools::{synthetic_decision_v1, ShapeV1};
use super::super::{
    encode_checkpoint_v1, SGroupV1, SInputV1, SLossV1, STrainStateV1, SWorkV1, SplitMixV1,
};
use super::*;

const TINY: SConfigV1 = SConfigV1 {
    d_model: 8,
    layers: 2,
    heads: 2,
    ffn: 16,
    card_dim: 4,
    card_vocab: 200,
};

fn shape(objects: usize, edges: usize, actions: usize, refs: usize) -> ShapeV1 {
    ShapeV1 {
        objects: (objects, objects),
        edges: (edges, edges),
        actions: (actions, actions),
        refs: (refs, refs),
    }
}

fn forwards() -> usize {
    TEST_FORWARDS.with(std::cell::Cell::get)
}

fn recorded(model: &SModelV1, step: SSubstepV1<'_>) -> (Vec<u32>, u32) {
    let output = model.forward_v1(step.input, &mut STapeV1::default()).unwrap();
    (
        output.logits.iter().map(|value| value.to_bits()).collect(),
        output.value.to_bits(),
    )
}

fn loss_bits(loss: SLossV1) -> [u32; 5] {
    [
        loss.policy_sum.to_bits(),
        loss.value_sum.to_bits(),
        loss.policy_term.to_bits(),
        loss.value_term.to_bits(),
        loss.loss.to_bits(),
    ]
}

#[test]
fn verified_rows_match_loss_and_gradients_for_both_sinks() {
    let model = SModelV1::init_parity_v1(TINY, 61).unwrap();
    let generation = ModelScopeV1::bind_v1(&model, model.cache_namespace_v1()).unwrap();
    let mut rng = SplitMixV1::new(29);
    let rows = [shape(1, 0, 1, 0), shape(16, 4, 7, 5), shape(3, 0, 3, 2)]
        .map(|shape| synthetic_decision_v1(&mut rng, shape));
    let steps = rows.each_ref().map(|row| SSubstepV1 {
        input: SInputV1::of_tensor_v1(row),
        selected: 0,
    });
    let groups = [
        SGroupV1 {
            substeps: vec![steps[0]],
            advantage: -0.75,
            value_target: 0.2,
        },
        SGroupV1 {
            substeps: vec![steps[1], SSubstepV1 { selected: 2, ..steps[2] }],
            advantage: 1.5,
            value_target: -0.6,
        },
    ];
    let coefficient = 0.7;
    let mut expected_gradient = vec![0.0; model.layout.total];
    let expected_loss = model
        .gae_loss_gradient_v1(
            &groups,
            coefficient,
            &mut expected_gradient,
            &mut SWorkV1::default(),
        )
        .unwrap();
    let expected_outputs = steps.map(|step| recorded(&model, step));
    let mut expected_state = STrainStateV1::fresh_optimizer_v1(model.clone());
    expected_state.adam_step_with_gradient_v1(&expected_gradient, 0.001).unwrap();

    for record in [false, true] {
        let mut scratch = SBackScratchV1::default();
        let mut tape = TapeSlotV1::new(1024 * 1024);
        let mut gradient = vec![0.0; model.layout.total];
        let mut policy_sum = 0.0;
        let mut value_sum = 0.0;
        let mut ordinal = 0;
        let before = forwards();
        for (group_index, group) in groups.iter().enumerate() {
            let mut joint = 0.0;
            let mut value_error = 0.0;
            let mut group_terms = Vec::new();
            for (substep_index, &step) in group.substeps.iter().enumerate() {
                let (logits, value) = &expected_outputs[ordinal];
                let row = tape.authenticate_v1(
                    &generation, step, (group_index, substep_index), logits, *value,
                ).unwrap();
                assert_eq!(forwards(), before + ordinal + 1);
                let mut terms = SGradientTermsV1::default();
                let mut sink = if record {
                    SGradientSinkV1::Record(&mut terms)
                } else {
                    SGradientSinkV1::Direct(&mut gradient)
                };
                let (selected, error) = row.backward_v1(GaeTargetV1 {
                    group_count: groups.len(),
                    advantage: group.advantage,
                    value_target: group.value_target,
                    value_coefficient: coefficient,
                }, &mut sink, &mut scratch).unwrap();
                assert_eq!(forwards(), before + ordinal + 1);
                if record {
                    group_terms.push(terms);
                }
                if substep_index == 0 {
                    joint = selected;
                    value_error = error;
                } else {
                    joint += selected;
                }
                ordinal += 1;
            }
            if record {
                replay_verified_test_terms_v1(&model.layout, &mut gradient, group_terms).unwrap();
            }
            policy_sum += -joint * group.advantage;
            value_sum += value_error * value_error;
        }
        for range in model.layout.gauge_ranges_v1() {
            gradient[range].fill(0.0);
        }
        let count = groups.len() as f32;
        assert_eq!(loss_bits(SLossV1 {
            group_count: groups.len(),
            policy_sum,
            value_sum,
            policy_term: policy_sum / count,
            value_term: value_sum / count,
            loss: (policy_sum + coefficient * value_sum) / count,
        }), loss_bits(expected_loss));
        assert!(gradient.iter().zip(&expected_gradient).all(|(a, b)| a.to_bits() == b.to_bits()));
        let mut state = STrainStateV1::fresh_optimizer_v1(model.clone());
        state.adam_step_with_gradient_v1(&gradient, 0.001).unwrap();
        assert_eq!(encode_checkpoint_v1(&state), encode_checkpoint_v1(&expected_state));
    }
}

#[test]
fn verified_rows_refuse_stale_model_input_and_output_bits() {
    let model = SModelV1::init_parity_v1(TINY, 61).unwrap();
    let generation = ModelScopeV1::bind_v1(&model, model.cache_namespace_v1()).unwrap();
    let mut rng = SplitMixV1::new(29);
    let first = synthetic_decision_v1(&mut rng, shape(6, 2, 5, 3));
    let second = synthetic_decision_v1(&mut rng, shape(6, 2, 5, 3));
    let step = SSubstepV1 { input: SInputV1::of_tensor_v1(&first), selected: 2 };
    let other = SSubstepV1 { input: SInputV1::of_tensor_v1(&second), ..step };
    let (logits, value) = recorded(&model, step);
    // The synthetic row never uses the last embedding. Its scores cannot
    // detect this different model generation, but the update scope can.
    let mut stale_values = model.values_v1().to_vec();
    stale_values[model.layout.card + (TINY.card_vocab - 1) * TINY.card_dim] += 1.0;
    let stale = SModelV1::from_values_v1(TINY, stale_values).unwrap();
    assert_eq!(recorded(&stale, step), (logits.clone(), value));
    let before = forwards();
    assert!(matches!(
        ModelScopeV1::bind_v1(&stale, model.cache_namespace_v1()),
        Err(error) if error == "S verified model generation differs"
    ));
    assert_eq!(forwards(), before);
    let mut changed = logits.clone();
    changed[1] ^= 1;
    let mut tape = TapeSlotV1::new(1024 * 1024);
    for (used_step, logits, value) in [
        (other, logits.as_slice(), value),
        (step, changed.as_slice(), value),
        (step, logits.as_slice(), value ^ 1),
        (step, &logits[..logits.len() - 1], value),
    ] {
        let before = forwards();
        let result = tape.authenticate_v1(&generation, used_step, (0, 0), logits, value);
        assert!(matches!(result, Err(error) if error == "stored tensor does not reproduce rollout outputs"));
        assert_eq!(forwards(), before + 1);
    }
    assert!(tape.authenticate_v1(&generation, step, (0, 0), &logits, value).is_ok());
}

#[test]
fn verified_tape_checks_reserved_capacity_before_forward() {
    let model = SModelV1::init_parity_v1(TINY, 61).unwrap();
    let generation = ModelScopeV1::bind_v1(&model, model.cache_namespace_v1()).unwrap();
    let mut rng = SplitMixV1::new(29);
    let rows = [shape(48, 1, 11, 1), shape(3, 64, 3, 128)]
        .map(|shape| synthetic_decision_v1(&mut rng, shape));
    let steps = rows.each_ref().map(|row| SSubstepV1 {
        input: SInputV1::of_tensor_v1(row),
        selected: 0,
    });
    let outputs = steps.map(|step| recorded(&model, step));
    let counts = steps.map(|step| step.input.validate_v1(&TINY).unwrap());
    let sizes = counts.map(|counts| {
        TapeSlotV1::new(0).bytes_v1(&TINY, counts, true).unwrap()
    });
    assert_eq!(
        sizes[0],
        STapeV1::bytes_for_counts_v1(&TINY, counts[0])
            + size_of::<TapeSlotV1>()
            + TINY.layers * size_of::<LayerTapeV1>()
    );
    let mut too_small = TapeSlotV1::new(sizes[0] - 1);
    let before = forwards();
    assert!(matches!(
        too_small.authenticate_v1(&generation, steps[0], (0, 0), &outputs[0].0, outputs[0].1),
        Err(error) if error == CAP_ERROR
    ));
    assert_eq!(forwards(), before);
    assert_eq!(too_small.bytes_v1(&TINY, counts[0], false), Some(size_of::<TapeSlotV1>()));

    let mut exact = TapeSlotV1::new(sizes[0]);
    exact.authenticate_v1(&generation, steps[0], (0, 0), &outputs[0].0, outputs[0].1).unwrap();
    assert_eq!(exact.bytes_v1(&TINY, counts[0], false), Some(sizes[0]));
    let cap = sizes[0].max(sizes[1]);
    let mut reused = TapeSlotV1::new(cap);
    reused.authenticate_v1(&generation, steps[0], (0, 0), &outputs[0].0, outputs[0].1).unwrap();
    assert!(reused.bytes_v1(&TINY, counts[1], true).unwrap() > cap);
    let before = forwards();
    assert!(matches!(
        reused.authenticate_v1(&generation, steps[1], (0, 1), &outputs[1].0, outputs[1].1),
        Err(error) if error == CAP_ERROR
    ));
    assert_eq!(forwards(), before);
    assert_eq!(reused.bytes_v1(&TINY, counts[1], false), Some(sizes[0]));
    TapeSlotV1::new(cap)
        .authenticate_v1(&generation, steps[1], (0, 1), &outputs[1].0, outputs[1].1)
        .unwrap();
}
