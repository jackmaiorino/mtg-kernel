use super::*;
use crate::line_b_teacher_target_v1::line_b_softmax_v1;
use crate::native_flat_tensorizer_v3::{encoded_decision_view_v3, NativeFlatDecisionTensorV3};
use crate::native_policy_train_step_v1::tests::real_map_training_tensor_v3;

const LR: f32 = 1e-4;
const WIDTH: usize = 7;

fn model() -> NativePolicyValueNetV1 {
    NativePolicyValueNetV1::runner_fixed_v1(NativePolicyValueModelConfigV1::contract_v1()).unwrap()
}

fn tensor() -> NativeFlatDecisionTensorV3 {
    real_map_training_tensor_v3()
}

/// Rows covering every card token the fixture reads, some features zero.
fn pathway(tensor: &NativeFlatDecisionTensorV3) -> RulesPathwayV1 {
    let cards = *tensor.common.object_card_ids.iter().max().unwrap() as usize;
    assert!(cards > 0, "the fixture reads at least one card");
    let rows = (0..cards * WIDTH)
        .map(|i| ((i * 7 + 3) % 5) as f32 / 5.0)
        .collect();
    RulesPathwayV1::new_v1(rows, cards, WIDTH).unwrap()
}

fn nonzero_weight(pathway: &mut RulesPathwayV1) {
    let weight = (0..CARD_EMBEDDING_DIM_V1 * WIDTH)
        .map(|i| ((i * 13 % 11) as f32 - 5.0) * 0.05)
        .collect();
    pathway.set_weight_v1(weight);
}

fn config(model: &NativePolicyValueNetV1) -> NativePolicyValueModelConfigV1 {
    input_config_v1(model, FreshLineageGenerationV1::V3)
}

/// A target far from the model: one-hot at its lowest-probability action.
fn far_target(
    model: &NativePolicyValueNetV1,
    tensor: &NativeFlatDecisionTensorV3,
) -> LineBSoftmaxV1 {
    let logits = model
        .forward_feature_transfer_v3(encoded_decision_view_v3(tensor))
        .unwrap()
        .logits;
    let worst = logits
        .iter()
        .enumerate()
        .min_by(|a, b| a.1.total_cmp(b.1))
        .unwrap()
        .0;
    one_hot_target_v1(logits.len(), worst).unwrap()
}

fn input<'a>(
    tensor: &'a NativeFlatDecisionTensorV3,
    target: &'a LineBSoftmaxV1,
) -> ExternalAuxiliaryInputV1<'a> {
    ExternalAuxiliaryInputV1 {
        direction: LineBDivergenceV1::Forward,
        coefficient: 1.0,
        denominator: 1,
        cap: None,
        step_telemetry: false,
        rows: vec![ExternalAuxiliaryRowV1 {
            encoded: encoded_decision_view_v3(tensor),
            target,
        }],
    }
}

/// The term's loss and its full gradient at `parameters`.
fn loss_and_gradient(
    parameters: &[NativeNamedParameterV1],
    config: NativePolicyValueModelConfigV1,
    input: &ExternalAuxiliaryInputV1<'_>,
) -> (f64, Vec<Vec<f32>>) {
    let terms = external_auxiliary_terms_v1(input, parameters, None, config).unwrap();
    let mut gradients: Vec<Vec<f32>> = parameters
        .iter()
        .map(|p| vec![0.0; p.values.len()])
        .collect();
    let mut gauge = ScorerBiasGaugeAccumulatorV1::default();
    let result = terms
        .combine_v1(parameters, &mut gradients, &mut gauge)
        .unwrap();
    (result.auxiliary_loss, gradients)
}

#[test]
fn zero_weight_reads_the_table_bit_for_bit() {
    let model = model();
    let parameters = model.parameter_snapshot_v1();
    let pathway = pathway(&tensor());
    let effective = pathway.effective_parameters_v1(&parameters).unwrap();
    assert_eq!(effective, parameters);
}

#[test]
fn tokens_read_their_card_row_and_padding_reads_nothing() {
    let mut pathway = pathway(&tensor());
    nonzero_weight(&mut pathway);
    assert_eq!(pathway.delta_v1(0), [0.0; CARD_EMBEDDING_DIM_V1]);
    assert_eq!(
        pathway.delta_v1(pathway.cards_v1() + 1),
        [0.0; CARD_EMBEDDING_DIM_V1]
    );
    // Token 1 reads row 0 exactly.
    let x: Vec<f32> = (0..WIDTH).map(|i| ((i * 7 + 3) % 5) as f32 / 5.0).collect();
    let w = pathway.weight_v1();
    let expected: Vec<f32> = (0..CARD_EMBEDDING_DIM_V1)
        .map(|k| {
            let mut sum = 0.0f32;
            for f in 0..WIDTH {
                if x[f] != 0.0 {
                    sum += w[k * WIDTH + f] * x[f];
                }
            }
            sum
        })
        .collect();
    assert_eq!(pathway.delta_v1(1).to_vec(), expected);
    // Padding stays zero in the effective table.
    let effective = pathway
        .effective_parameters_v1(&model().parameter_snapshot_v1())
        .unwrap();
    assert!(effective[CARD_EMBEDDING].values[..CARD_EMBEDDING_DIM_V1]
        .iter()
        .all(|v| v.to_bits() == 0));
}

#[test]
fn nonzero_weight_training_forward_matches_the_model_forward() {
    let model = model();
    let tensor = tensor();
    let mut pathway = pathway(&tensor);
    nonzero_weight(&mut pathway);
    let effective = pathway
        .effective_parameters_v1(&model.parameter_snapshot_v1())
        .unwrap();
    assert_ne!(effective, model.parameter_snapshot_v1());
    let tape = forward_with_tape(
        &effective,
        config(&model),
        encoded_decision_view_v3(&tensor),
        TrainingMathV1::PlatformLibm,
    )
    .unwrap();
    let mut scored = model.clone();
    scored.replace_parameter_snapshot_v1(&effective).unwrap();
    let output = scored
        .forward_feature_transfer_v3(encoded_decision_view_v3(&tensor))
        .unwrap();
    let base = model
        .forward_feature_transfer_v3(encoded_decision_view_v3(&tensor))
        .unwrap();
    assert_eq!(bits_of(tape.logits_v1()), bits_of(&output.logits));
    assert_eq!(tape.value_v1().to_bits(), output.value.to_bits());
    assert_ne!(bits_of(&output.logits), bits_of(&base.logits));
}

#[test]
fn weight_gradient_matches_finite_differences() {
    let model = model();
    let tensor = tensor();
    let config = config(&model);
    let target = far_target(&model, &tensor);
    let input = input(&tensor, &target);
    let mut pathway = pathway(&tensor);
    nonzero_weight(&mut pathway);
    let parameters = model.parameter_snapshot_v1();
    let (_, gradients) = loss_and_gradient(
        &pathway.effective_parameters_v1(&parameters).unwrap(),
        config,
        &input,
    );
    let analytic = pathway
        .weight_gradient_v1(&gradients[CARD_EMBEDDING])
        .unwrap();
    let norm = analytic
        .iter()
        .map(|&g| f64::from(g) * f64::from(g))
        .sum::<f64>()
        .sqrt();
    assert!(norm > 0.0, "the pathway has no gradient");
    // Along the unit gradient direction d the directional derivative is |g|;
    // a central difference over the whole weight pools every entry's signal.
    let at = |h: f64| {
        let mut p = pathway.clone();
        let w: Vec<f32> = p
            .weight_v1()
            .iter()
            .zip(&analytic)
            .map(|(&w, &g)| (f64::from(w) + h * f64::from(g) / norm) as f32)
            .collect();
        p.set_weight_v1(w);
        loss_and_gradient(
            &p.effective_parameters_v1(&parameters).unwrap(),
            config,
            &input,
        )
        .0
    };
    for h in [1e-2, 3e-2] {
        let numeric = (at(h) - at(-h)) / (2.0 * h);
        assert!(
            (numeric - norm).abs() <= 0.05 * norm,
            "h {h}: analytic {norm}, numeric {numeric}"
        );
    }
}

#[test]
fn zero_weight_step_matches_the_plain_step_and_moves_the_weight() {
    let model = model();
    let tensor = tensor();
    let target = far_target(&model, &tensor);
    let input = input(&tensor, &target);
    let mut plain = NativePolicyValueTrainStateV1::new_v1(model.clone()).unwrap();
    let mut with = plain.clone();
    let mut pathway = pathway(&tensor);
    plain
        .train_step_auxiliary_only_v1(FreshLineageGenerationV1::V3, LR, &[&input])
        .unwrap();
    with.auxiliary_only_step_v1(
        FreshLineageGenerationV1::V3,
        LR,
        &[&input],
        ArchiveDistillationUpdateScopeV1::FullNetwork,
        Some(&mut pathway),
    )
    .unwrap();
    assert_eq!(
        plain.model_v1().parameter_snapshot_v1(),
        with.model_v1().parameter_snapshot_v1()
    );
    assert_eq!(plain.adam_step_v1(), with.adam_step_v1());
    assert!(pathway.weight_norm_v1() > 0.0);
    assert!(pathway.last_gradient_norm_v1() > 0.0);
    // A second step reads the moved weight, so the networks now differ.
    plain
        .train_step_auxiliary_only_v1(FreshLineageGenerationV1::V3, LR, &[&input])
        .unwrap();
    with.auxiliary_only_step_v1(
        FreshLineageGenerationV1::V3,
        LR,
        &[&input],
        ArchiveDistillationUpdateScopeV1::FullNetwork,
        Some(&mut pathway),
    )
    .unwrap();
    assert_ne!(
        plain.model_v1().parameter_snapshot_v1(),
        with.model_v1().parameter_snapshot_v1()
    );
}

#[test]
fn parent_targets_leave_a_zero_weight_at_zero() {
    let model = model();
    let tensor = tensor();
    let logits: Vec<f64> = model
        .forward_feature_transfer_v3(encoded_decision_view_v3(&tensor))
        .unwrap()
        .logits
        .iter()
        .map(|&z| f64::from(z))
        .collect();
    let target = line_b_softmax_v1(&logits).unwrap();
    let input = input(&tensor, &target);
    let mut state = NativePolicyValueTrainStateV1::new_v1(model.clone()).unwrap();
    let mut pathway = pathway(&tensor);
    let before = pathway.clone();
    state
        .auxiliary_only_step_v1(
            FreshLineageGenerationV1::V3,
            LR,
            &[&input],
            ArchiveDistillationUpdateScopeV1::FullNetwork,
            Some(&mut pathway),
        )
        .unwrap();
    assert_eq!(pathway.state_bits_v1(), before.state_bits_v1());
    assert_eq!(
        model.parameter_snapshot_v1(),
        state.model_v1().parameter_snapshot_v1()
    );
}

#[test]
fn the_pathway_trains_the_full_network_only() {
    let model = model();
    let tensor = tensor();
    let target = far_target(&model, &tensor);
    let input = input(&tensor, &target);
    let mut state = NativePolicyValueTrainStateV1::new_v1(model).unwrap();
    let mut pathway = pathway(&tensor);
    assert!(state
        .auxiliary_only_step_v1(
            FreshLineageGenerationV1::V3,
            LR,
            &[&input],
            ArchiveDistillationUpdateScopeV1::ScorerOnly,
            Some(&mut pathway),
        )
        .is_err());
}

#[test]
fn rows_must_be_finite_and_sized() {
    assert!(RulesPathwayV1::new_v1(vec![0.0; 6], 2, 3).is_ok());
    assert!(RulesPathwayV1::new_v1(vec![0.0; 5], 2, 3).is_err());
    assert!(RulesPathwayV1::new_v1(vec![f32::NAN; 6], 2, 3).is_err());
    assert!(RulesPathwayV1::new_v1(Vec::new(), 0, 3).is_err());
}
