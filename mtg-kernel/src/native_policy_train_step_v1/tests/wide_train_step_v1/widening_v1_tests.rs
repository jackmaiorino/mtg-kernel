//! Function-preserving widening and the w32 control (design section 5.6 and
//! section 3 PR 4a), plus the `#[ignore]`d Adam step-age probe of section
//! 7.6. The collected-b512 preservation check is the `#[ignore]`d
//! `expanded_deck_training_v1` test `widening_b512_tests`.

use super::*;
use crate::native_policy_train_step_v1::widening_v1::{
    hex_sha256_v1, w32_control_from_net8_snapshot_v1, w32_zeroed_units_v1,
    wide_coordinate_classes_v1, widen_net8_snapshot_v1, HiddenSpaceV1, WideCoordinateClassV1,
    WideningErrorV1, WideningInitLawV1, HIDDEN_SPACES_V1, TORCH_DEFAULT_UNIFORM_V1,
    W32_CONTROL_CONSTRUCTION_VERSION_V1, W32_CONTROL_RULE_V1, WIDENING_CONSTRUCTION_VERSION_V1,
};

const WIDENING_SEED_V1: u64 = 0x4A57_1DE0_0000_0001;

/// A trained net-8 parent: the perturbed runner-fixed model after two GAE
/// updates, so its moments and Adam step are live.
fn trained_parent_v1() -> (
    NativePolicyValueTrainStateV1,
    NativePolicyValueTrainSnapshotV1,
) {
    let mut state = NativePolicyValueTrainStateV1::new_v1(perturbed_dims_v1::<Net8DimsV1>(
        narrow_model_v1(),
        0x4A_0001,
    ))
    .unwrap();
    for _ in 0..2 {
        route_update_dims_v1(&mut state, WideRouteV1::GaeSequential);
    }
    let snapshot = state.snapshot_v1().unwrap();
    assert_eq!(snapshot.adam_step, 2);
    (state, snapshot)
}

fn widen_v1(
    parent: &NativePolicyValueTrainSnapshotV1,
    seed: u64,
) -> NativePolicyValueTrainStateWideV1 {
    widen_net8_snapshot_v1(
        parent,
        W_MODEL_ARCHITECTURE_VERSION_V1,
        TORCH_DEFAULT_UNIFORM_V1,
        seed,
    )
    .unwrap()
    .state
}

fn assert_same_outputs_v1(
    label: &str,
    narrow: &NativePolicyValueOutputV1,
    wide: &NativePolicyValueOutputV1,
) {
    assert_eq!(narrow.logits.len(), wide.logits.len(), "{label}");
    for (index, (narrow_logit, wide_logit)) in narrow.logits.iter().zip(&wide.logits).enumerate() {
        assert!(
            narrow_logit == wide_logit,
            "{label} logit {index}: narrow {narrow_logit} wide {wide_logit}"
        );
    }
    assert!(
        narrow.value == wide.value,
        "{label} value: narrow {} wide {}",
        narrow.value,
        wide.value
    );
}

fn wide_segment_column_v1(tensor: usize, segment: usize, unit: usize) -> usize {
    column_segments_v1(tensor)[..segment]
        .iter()
        .map(|&segment| segment_width_v1(segment, W_HIDDEN_DIM_V1, W_CARD_EMBEDDING_DIM_V1))
        .sum::<usize>()
        + unit
}

// ---------------------------------------------------------------------------
// Design section 5.6 (a): exact preservation.
// ---------------------------------------------------------------------------

/// The widened net's logits and value equal the parent's under IEEE `==`
/// (a -0 may become +0) on every fixture decision: the synthetic gradient
/// decisions under the V3 and contract inputs and the real Map V3 decisions,
/// through the inference forwards and the train step's tape forward.
#[test]
fn widening_preserves_logits_and_value_exactly_on_the_fixtures() {
    let (parent_state, parent) = trained_parent_v1();
    let narrow = parent_state.model_v1();
    let wide_state = widen_v1(&parent, WIDENING_SEED_V1);
    let wide = wide_state.model_v1();
    let decisions = gradient_decisions_v1();
    let many = ManyFixtureV1::of_v1(narrow);
    let mut views: Vec<_> = decisions.iter().map(|d| d.view_v1(false)).collect();
    views.extend(many.tensors.iter().map(encoded_decision_view_v3));
    let narrow_parameters = narrow.parameter_snapshot_v1();
    let wide_parameters = wide.parameter_snapshot_wide_v1();
    for (index, view) in views.into_iter().enumerate() {
        assert_same_outputs_v1(
            &format!("v3 inference {index}"),
            &narrow.forward_v3_for_test_v1(view),
            &wide.forward_v3_for_test_v1(view),
        );
        let narrow_tape = forward_with_tape_dims_v1::<Net8DimsV1>(
            &narrow_parameters,
            narrow.feature_transfer_config_v3(),
            view,
            TrainingMathV1::PlatformLibm,
        )
        .unwrap();
        let wide_tape = forward_with_tape_dims_v1::<Net8W128DimsV1>(
            &wide_parameters,
            TrainableNetV1::feature_transfer_config_v3(wide),
            view,
            TrainingMathV1::PlatformLibm,
        )
        .unwrap();
        assert_eq!(narrow_tape.logits_v1().len(), wide_tape.logits_v1().len());
        for (narrow_logit, wide_logit) in narrow_tape.logits_v1().iter().zip(wide_tape.logits_v1())
        {
            assert!(narrow_logit == wide_logit, "v3 tape {index}");
        }
        assert!(
            narrow_tape.value_v1() == wide_tape.value_v1(),
            "v3 tape {index}"
        );
    }
    for (index, decision) in decisions.iter().enumerate() {
        assert_same_outputs_v1(
            &format!("contract {index}"),
            &narrow.forward_contract_for_test_v1(decision.view_v1(true)),
            &wide.forward_contract_for_test_v1(decision.view_v1(true)),
        );
    }
}

// ---------------------------------------------------------------------------
// Design section 5.6 (b) and (c): invariants and the Adam mapping.
// ---------------------------------------------------------------------------

/// Every coordinate class is where the layout says: the old coordinates are
/// exactly the layout map's image, with parameters and both moments equal to
/// the parent's by bits; every new fan-out weight, new bias, new embedding
/// column (padding row included) and every new moment is +0 by bits; drawn
/// coordinates lie inside the law's bound; `adam_step` and the gauge anchor
/// are kept.
#[test]
fn widening_keeps_old_coordinates_and_zeroes_every_new_fan_out_and_moment() {
    use WideCoordinateClassV1::*;
    let (_, parent) = trained_parent_v1();
    let widened = widen_net8_snapshot_v1(
        &parent,
        W_MODEL_ARCHITECTURE_VERSION_V1,
        TORCH_DEFAULT_UNIFORM_V1,
        WIDENING_SEED_V1,
    )
    .unwrap();
    let wide = widened.state.snapshot_v1().unwrap();
    assert_eq!(wide.adam_step, parent.adam_step);
    assert_eq!(wide.scorer_bias_anchor_bits, parent.scorer_bias_anchor_bits);
    assert_eq!(
        wide.parameters[SCORER_SECOND_BIAS].values[0].to_bits(),
        parent.parameters[SCORER_SECOND_BIAS].values[0].to_bits()
    );
    assert!(
        wide.parameters[CARD_EMBEDDING].values[..W_CARD_EMBEDDING_DIM_V1]
            .iter()
            .all(|value| value.to_bits() == 0)
    );
    let mut counts = BTreeMap::<WideCoordinateClassV1, u64>::new();
    for tensor in 0..PARAMETER_TENSOR_COUNT {
        let classes = wide_coordinate_classes_v1(tensor);
        assert_eq!(classes.len(), wide.parameters[tensor].values.len());
        let mut image = vec![false; classes.len()];
        for index in 0..parent.parameters[tensor].values.len() {
            let wide_index = narrow_to_wide_index_v1(tensor, index);
            image[wide_index] = true;
            assert_eq!(classes[wide_index], Old, "tensor {tensor} index {index}");
            for (narrow, wide) in [
                (&parent.parameters, &wide.parameters),
                (&parent.first_moments, &wide.first_moments),
                (&parent.second_moments, &wide.second_moments),
            ] {
                assert_eq!(
                    narrow[tensor].values[index].to_bits(),
                    wide[tensor].values[wide_index].to_bits(),
                    "tensor {tensor} index {index}"
                );
            }
        }
        let shape = W_EXPECTED_PARAMETER_SHAPES[tensor];
        for (index, &class) in classes.iter().enumerate() {
            *counts.entry(class).or_default() += 1;
            assert_eq!(image[index], class == Old, "tensor {tensor} index {index}");
            if class == Old {
                continue;
            }
            let value = wide.parameters[tensor].values[index];
            assert_eq!(wide.first_moments[tensor].values[index].to_bits(), 0);
            assert_eq!(wide.second_moments[tensor].values[index].to_bits(), 0);
            if class.is_drawn_v1() {
                let bound = (1.0 / (shape[1] as f64).sqrt()) as f32;
                assert!(
                    (-bound..=bound).contains(&value),
                    "tensor {tensor} index {index}: {value}"
                );
            } else {
                assert_eq!(value.to_bits(), 0, "tensor {tensor} {class:?} {index}");
            }
        }
    }
    let drawn = counts[&NewFanIn] + counts[&NewEmbeddingFanOut];
    assert_eq!(drawn, widened.provenance.drawn_coordinate_count);
    assert_eq!(counts[&Old], PARAMETER_COUNT_V1 as u64);
    assert_eq!(counts.values().sum::<u64>(), W_PARAMETER_COUNT_V1 as u64);
    // Every embedding row of the new columns, and 16 new embedding columns
    // feeding the 64 old object-encoder rows.
    assert_eq!(
        counts[&NewEmbeddingColumn],
        (CARD_VOCAB_SIZE_V1 * (W_CARD_EMBEDDING_DIM_V1 - CARD_EMBEDDING_DIM_V1)) as u64
    );
    assert_eq!(
        counts[&NewEmbeddingFanOut],
        (HIDDEN_DIM_V1 * (W_CARD_EMBEDDING_DIM_V1 - CARD_EMBEDDING_DIM_V1)) as u64
    );
    assert!(counts[&NewFanOut] > 0 && counts[&NewBias] > 0);
}

/// The draws follow the documented law and order, recomputed here without
/// the class function: object_encoder.0.weight is the first drawn tensor
/// (the embedding draws nothing); in each of its 64 old rows the 16 new
/// embedding columns 114..130 are drawn first, and the first new-unit row
/// (row 64) starts at draw 64 * 16.
#[test]
fn widening_draws_follow_the_documented_law_and_order() {
    let (_, parent) = trained_parent_v1();
    let wide = widen_v1(&parent, WIDENING_SEED_V1).snapshot_v1().unwrap();
    let fan_in = W_OBJECT_ENCODER_INPUT;
    let mut rng = SplitMix64::seed(WIDENING_SEED_V1);
    let mut draw = || {
        let unit = (rng.next_u64() >> 40) as f64 / 16_777_216.0;
        ((2.0 * unit - 1.0) / (fan_in as f64).sqrt()) as f32
    };
    let weight = &wide.parameters[OBJECT_FIRST_WEIGHT].values;
    let first_new_column = OBJECT_FEATURE_DIM_V1 + CARD_EMBEDDING_DIM_V1;
    for row in 0..HIDDEN_DIM_V1 {
        for column in first_new_column..fan_in {
            assert_eq!(
                weight[row * fan_in + column].to_bits(),
                draw().to_bits(),
                "row {row} column {column}"
            );
        }
    }
    for column in 0..fan_in {
        assert_eq!(
            weight[HIDDEN_DIM_V1 * fan_in + column].to_bits(),
            draw().to_bits(),
            "row 64 column {column}"
        );
    }
    assert_eq!(
        WideningInitLawV1::from_name_v1(TORCH_DEFAULT_UNIFORM_V1).unwrap(),
        WideningInitLawV1::TorchDefaultUniformV1
    );
}

/// The same seed gives the same bits; a different seed changes only the
/// drawn coordinates (new fan-in and new embedding fan-out), and changes
/// both kinds.
#[test]
fn widening_is_deterministic_and_the_seed_moves_only_drawn_coordinates() {
    let (_, parent) = trained_parent_v1();
    let first = widen_net8_snapshot_v1(
        &parent,
        W_MODEL_ARCHITECTURE_VERSION_V1,
        TORCH_DEFAULT_UNIFORM_V1,
        WIDENING_SEED_V1,
    )
    .unwrap();
    let again = widen_net8_snapshot_v1(
        &parent,
        W_MODEL_ARCHITECTURE_VERSION_V1,
        TORCH_DEFAULT_UNIFORM_V1,
        WIDENING_SEED_V1,
    )
    .unwrap();
    assert_eq!(first.provenance, again.provenance);
    let (first, again) = (
        first.state.snapshot_v1().unwrap(),
        again.state.snapshot_v1().unwrap(),
    );
    assert_eq!(first, again);
    let other = widen_v1(&parent, WIDENING_SEED_V1 + 1)
        .snapshot_v1()
        .unwrap();
    let mut changed = BTreeMap::<WideCoordinateClassV1, u64>::new();
    for tensor in 0..PARAMETER_TENSOR_COUNT {
        for (index, class) in wide_coordinate_classes_v1(tensor).into_iter().enumerate() {
            for (left, right) in [
                (&first.parameters, &other.parameters),
                (&first.first_moments, &other.first_moments),
                (&first.second_moments, &other.second_moments),
            ] {
                let differs =
                    left[tensor].values[index].to_bits() != right[tensor].values[index].to_bits();
                if differs {
                    assert!(class.is_drawn_v1(), "tensor {tensor} {class:?} {index}");
                    *changed.entry(class).or_default() += 1;
                }
            }
        }
    }
    assert_eq!(changed.len(), 2, "{changed:?}");
    assert_eq!(first.adam_step, other.adam_step);
}

#[test]
fn widening_refuses_other_targets_laws_and_non_net8_parents() {
    let (_, parent) = trained_parent_v1();
    for target in [
        MODEL_ARCHITECTURE_VERSION_V1,
        "kernel-policy-value-net-8w256",
        "",
    ] {
        assert_eq!(
            widen_net8_snapshot_v1(&parent, target, TORCH_DEFAULT_UNIFORM_V1, 1).unwrap_err(),
            WideningErrorV1::UnsupportedTargetArchitecture {
                requested: target.into()
            }
        );
    }
    for law in ["xavier_uniform_v1", "zeros", "TORCH_DEFAULT_UNIFORM_V1", ""] {
        assert_eq!(
            widen_net8_snapshot_v1(&parent, W_MODEL_ARCHITECTURE_VERSION_V1, law, 1).unwrap_err(),
            WideningErrorV1::UnsupportedInitLaw {
                requested: law.into()
            }
        );
    }
    let refused = |snapshot: &NativePolicyValueTrainSnapshotV1| {
        matches!(
            widen_net8_snapshot_v1(
                snapshot,
                W_MODEL_ARCHITECTURE_VERSION_V1,
                TORCH_DEFAULT_UNIFORM_V1,
                1
            ),
            Err(WideningErrorV1::ParentNotNet8(_))
        ) && matches!(
            w32_control_from_net8_snapshot_v1(snapshot),
            Err(WideningErrorV1::ParentNotNet8(_))
        )
    };
    // A wide parent (already widened).
    let wide = widen_v1(&parent, 1).snapshot_v1().unwrap();
    assert!(refused(&wide));
    // A nonzero padding row.
    let mut padding = parent.clone();
    padding.parameters[CARD_EMBEDDING].values[3] = 0.5;
    assert!(refused(&padding));
    // A gauge anchor that disagrees with the scorer bias.
    let mut anchor = parent.clone();
    anchor.scorer_bias_anchor_bits ^= 1;
    assert!(refused(&anchor));
    // A truncated tensor.
    let mut truncated = parent.clone();
    truncated.parameters[VALUE_FIRST_BIAS].values.pop();
    assert!(refused(&truncated));
}

#[test]
fn widening_provenance_records_parent_hash_seed_law_and_version() {
    let (_, parent) = trained_parent_v1();
    let widened = widen_net8_snapshot_v1(
        &parent,
        W_MODEL_ARCHITECTURE_VERSION_V1,
        TORCH_DEFAULT_UNIFORM_V1,
        WIDENING_SEED_V1,
    )
    .unwrap();
    let provenance = &widened.provenance;
    assert_eq!(
        provenance.construction_version,
        WIDENING_CONSTRUCTION_VERSION_V1
    );
    assert_eq!(
        provenance.parent_architecture,
        MODEL_ARCHITECTURE_VERSION_V1
    );
    assert_eq!(
        provenance.child_architecture,
        W_MODEL_ARCHITECTURE_VERSION_V1
    );
    assert_eq!(
        provenance.parent_state_sha256,
        hex_sha256_v1(parent.state_sha256_v1().unwrap())
    );
    let snapshot = widened.state.snapshot_v1().unwrap();
    assert_eq!(
        provenance.child_state_sha256,
        hex_sha256_v1(wide_owned_train_snapshot_state_sha256_v1(&snapshot).unwrap())
    );
    assert_eq!(
        provenance.child_state_sha256,
        hex_sha256_v1(widened.state.state_sha256_v1().unwrap())
    );
    assert_eq!(provenance.init_law, TORCH_DEFAULT_UNIFORM_V1);
    assert_eq!(provenance.seed, WIDENING_SEED_V1);
    assert_eq!(provenance.adam_step, parent.adam_step);
    let json = serde_json::to_value(provenance).unwrap();
    for field in [
        "construction_version",
        "parent_state_sha256",
        "child_state_sha256",
        "init_law",
        "seed",
    ] {
        assert!(json.get(field).is_some(), "{field}");
    }
}

// ---------------------------------------------------------------------------
// Design section 5.6 (d): the first update.
// ---------------------------------------------------------------------------

/// On the first update of the widened state, every new fan-in weight and new
/// bias gets an exactly zero gradient (no new unit reaches an output yet),
/// as does every new embedding fan-out weight (its column is +0); every
/// hidden space's new fan-out weights and every present card's new
/// embedding columns get nonzero gradients: the zero-fan-out weights move
/// first. Checked on the terminal and GAE routes; the padding row and gauge
/// anchor survive the update.
#[test]
fn widened_first_update_moves_new_fan_out_and_embedding_columns_first() {
    use WideCoordinateClassV1::*;
    let (_, parent) = trained_parent_v1();
    let start = widen_v1(&parent, WIDENING_SEED_V1);
    let anchor = start.scorer_bias_anchor_f32_bits_v1();
    let present_cards: Vec<usize> = GRADIENT_CARD_IDS_V1
        .iter()
        .filter(|&&id| id != 0)
        .map(|&id| id as usize)
        .collect();
    for route in [WideRouteV1::TerminalSequential, WideRouteV1::GaeSequential] {
        let mut state = start.clone();
        let result = route_update_dims_v1(&mut state, route);
        for (tensor, gradient) in result.gradients.iter().enumerate() {
            for (index, class) in wide_coordinate_classes_v1(tensor).into_iter().enumerate() {
                if matches!(class, NewFanIn | NewBias | NewEmbeddingFanOut) {
                    assert!(
                        gradient.values[index] == 0.0,
                        "{route:?} tensor {tensor} {class:?} {index}: {}",
                        gradient.values[index]
                    );
                }
            }
        }
        for space in &HIDDEN_SPACES_V1 {
            let moved = space.fan_out.iter().any(|&(tensor, segment)| {
                let columns = W_EXPECTED_PARAMETER_SHAPES[tensor][1];
                let old_rows = EXPECTED_PARAMETER_SHAPES[tensor][0];
                (HIDDEN_DIM_V1..W_HIDDEN_DIM_V1).any(|unit| {
                    let column = wide_segment_column_v1(tensor, segment, unit);
                    (0..old_rows)
                        .any(|row| result.gradients[tensor].values[row * columns + column] != 0.0)
                })
            });
            assert!(
                moved,
                "{route:?} {} new fan-out has no gradient",
                space.name
            );
        }
        let embedding = &result.gradients[CARD_EMBEDDING].values;
        for &card in &present_cards {
            let row =
                &embedding[card * W_CARD_EMBEDDING_DIM_V1..(card + 1) * W_CARD_EMBEDDING_DIM_V1];
            assert!(
                row[CARD_EMBEDDING_DIM_V1..]
                    .iter()
                    .any(|value| *value != 0.0),
                "{route:?} card {card} new embedding columns have no gradient"
            );
        }
        assert!(embedding[..W_CARD_EMBEDDING_DIM_V1]
            .iter()
            .all(|value| *value == 0.0));
        let after = state.snapshot_v1().unwrap();
        assert!(
            after.parameters[CARD_EMBEDDING].values[..W_CARD_EMBEDDING_DIM_V1]
                .iter()
                .all(|value| value.to_bits() == 0)
        );
        assert_eq!(after.scorer_bias_anchor_bits, anchor);
        assert_eq!(after.adam_step, parent.adam_step + 1);
    }
}

// ---------------------------------------------------------------------------
// The w32 control.
// ---------------------------------------------------------------------------

/// Every hidden segment of every net-8 layer input belongs to exactly one
/// hidden space, and the widening's new fan-out coordinates are exactly the
/// old rows of the new units of those segments.
#[test]
fn hidden_spaces_cover_every_hidden_segment_once() {
    let mut owners = BTreeMap::<(usize, usize), &str>::new();
    for HiddenSpaceV1 { name, fan_out } in &HIDDEN_SPACES_V1 {
        for &(tensor, segment) in *fan_out {
            assert_eq!(
                column_segments_v1(tensor)[segment],
                SegmentV1::Hidden,
                "{name}"
            );
            assert!(owners.insert((tensor, segment), *name).is_none(), "{name}");
        }
    }
    for tensor in 0..PARAMETER_TENSOR_COUNT {
        let segments = column_segments_v1(tensor);
        for (segment, kind) in segments.iter().enumerate() {
            assert_eq!(
                *kind == SegmentV1::Hidden,
                owners.contains_key(&(tensor, segment)),
                "tensor {tensor} segment {segment}"
            );
        }
        let classes = wide_coordinate_classes_v1(tensor);
        let mut expected = vec![false; classes.len()];
        for (segment, kind) in segments.iter().enumerate() {
            if *kind != SegmentV1::Hidden {
                continue;
            }
            let columns = W_EXPECTED_PARAMETER_SHAPES[tensor][1];
            for row in 0..EXPECTED_PARAMETER_SHAPES[tensor][0] {
                for unit in HIDDEN_DIM_V1..W_HIDDEN_DIM_V1 {
                    expected[row * columns + wide_segment_column_v1(tensor, segment, unit)] = true;
                }
            }
        }
        for (index, class) in classes.iter().enumerate() {
            assert_eq!(
                *class == WideCoordinateClassV1::NewFanOut,
                expected[index],
                "tensor {tensor} index {index}"
            );
        }
    }
}

/// The w32 control zeroes, in every hidden space, the fan-out weights of
/// units 32 to 63 and both of their moments (+0 by bits), keeps every other
/// coordinate, `adam_step` and the anchor by bits, records the zeroed units,
/// is deterministic, and changes the function.
#[test]
fn w32_control_zeroes_the_upper_half_fan_out_and_records_it() {
    let (parent_state, parent) = trained_parent_v1();
    let control = w32_control_from_net8_snapshot_v1(&parent).unwrap();
    let again = w32_control_from_net8_snapshot_v1(&parent).unwrap();
    assert_eq!(control.provenance, again.provenance);
    let snapshot = control.state.snapshot_v1().unwrap();
    assert_eq!(snapshot, again.state.snapshot_v1().unwrap());
    let provenance = &control.provenance;
    assert_eq!(
        provenance.construction_version,
        W32_CONTROL_CONSTRUCTION_VERSION_V1
    );
    assert_eq!(provenance.rule, W32_CONTROL_RULE_V1);
    assert_eq!(provenance.architecture, MODEL_ARCHITECTURE_VERSION_V1);
    assert_eq!(
        provenance.parent_state_sha256,
        hex_sha256_v1(parent.state_sha256_v1().unwrap())
    );
    assert_eq!(
        provenance.child_state_sha256,
        hex_sha256_v1(snapshot.state_sha256_v1().unwrap())
    );
    assert_eq!(provenance.adam_step, parent.adam_step);
    assert_eq!(snapshot.adam_step, parent.adam_step);
    assert_eq!(
        snapshot.scorer_bias_anchor_bits,
        parent.scorer_bias_anchor_bits
    );
    assert_eq!(provenance.zeroed.len(), HIDDEN_SPACES_V1.len());
    let units: Vec<usize> = (32..64).collect();
    let mut zeroed = vec![Vec::<bool>::new(); PARAMETER_TENSOR_COUNT];
    for (tensor, mask) in zeroed.iter_mut().enumerate() {
        *mask = vec![false; parent.parameters[tensor].values.len()];
    }
    for (record, space) in provenance.zeroed.iter().zip(&HIDDEN_SPACES_V1) {
        assert_eq!(record.space, space.name);
        assert_eq!(record.units, units);
        assert_eq!(w32_zeroed_units_v1().collect::<Vec<_>>(), units);
        let mut count = 0u64;
        for &(tensor, segment) in space.fan_out {
            let columns = EXPECTED_PARAMETER_SHAPES[tensor][1];
            let offset: usize = column_segments_v1(tensor)[..segment]
                .iter()
                .map(|&s| segment_width_v1(s, HIDDEN_DIM_V1, CARD_EMBEDDING_DIM_V1))
                .sum();
            for row in 0..EXPECTED_PARAMETER_SHAPES[tensor][0] {
                for &unit in &units {
                    zeroed[tensor][row * columns + offset + unit] = true;
                    count += 1;
                }
            }
        }
        assert_eq!(record.zeroed_weights, count, "{}", space.name);
    }
    let total: usize = zeroed.iter().flatten().filter(|&&z| z).count();
    assert_eq!(provenance.zeroed_weight_count, total as u64);
    for tensor in 0..PARAMETER_TENSOR_COUNT {
        for (child, parent) in [
            (&snapshot.parameters, &parent.parameters),
            (&snapshot.first_moments, &parent.first_moments),
            (&snapshot.second_moments, &parent.second_moments),
        ] {
            for (index, &is_zeroed) in zeroed[tensor].iter().enumerate() {
                let bits = child[tensor].values[index].to_bits();
                if is_zeroed {
                    assert_eq!(bits, 0, "tensor {tensor} index {index}");
                } else {
                    assert_eq!(bits, parent[tensor].values[index].to_bits());
                }
            }
        }
    }
    // A function-changing control: some fixture output differs.
    let decisions = gradient_decisions_v1();
    let changed = decisions.iter().any(|decision| {
        let before = parent_state
            .model_v1()
            .forward_v3_for_test_v1(decision.view_v1(false));
        let after = control
            .state
            .model_v1()
            .forward_v3_for_test_v1(decision.view_v1(false));
        before.logits != after.logits || before.value != after.value
    });
    assert!(changed);
}

// ---------------------------------------------------------------------------
// Design section 7.6: the measured Adam step-age trajectory (probe).
// ---------------------------------------------------------------------------

/// The b512 dose checkpoint's inherited Adam step (OH2 chain r2-s1,
/// doses/b512.json, `adam_step`).
const B512_ADAM_STEP_V1: u64 = 118_584;
const STEP_AGE_UPDATES_V1: usize = 300;

fn quantile_v1(values: &mut [f64], quantile: f64) -> f64 {
    let index = ((values.len() - 1) as f64 * quantile).round() as usize;
    *values
        .select_nth_unstable_by(index, |a, b| a.total_cmp(b))
        .1
}

/// The constant-gradient multiplier of design section 7.6 for a coordinate
/// whose moments start at zero, `k` updates in, with `bias_age` the global
/// Adam step after the update (`None`: bias correction taken as 1).
fn constant_gradient_multiplier_v1(k: u64, bias_age: Option<u64>) -> f64 {
    let k = k as i32;
    let raw = (1.0 - 0.9f64.powi(k)) / (1.0 - 0.999f64.powi(k)).sqrt();
    match bias_age {
        None => raw,
        Some(age) => {
            let age = age as i32;
            raw * (1.0 - 0.999f64.powi(age)).sqrt() / (1.0 - 0.9f64.powi(age))
        }
    }
}

/// Widens the trained parent with the b512 Adam step and runs 300 real GAE
/// updates (exact parallel backward, 4 workers, 100 groups of real Map V3
/// decisions) on one fixed batch. For each update it records, per
/// coordinate class, the effective step `|delta parameter| / lr` over the
/// active coordinates (nonzero first moment): count, median, p90 and max,
/// beside the constant-gradient reference of design section 7.6 (3.16 at
/// k = 1, peak 6.57 at k = 12) and its about 7.27 bound. Measurement only:
/// the optimizer is the unchanged train step. Writes JSON to
/// `MTG_KERNEL_PR4A_STEP_AGE_OUT` when set, else prints it.
#[test]
#[ignore = "measurement probe (design section 7.6): run explicitly; writes the step-age JSON"]
fn adam_step_age_trajectory_probe_v1() {
    let (_, mut parent) = trained_parent_v1();
    parent.adam_step = B512_ADAM_STEP_V1;
    let widened = widen_net8_snapshot_v1(
        &parent,
        W_MODEL_ARCHITECTURE_VERSION_V1,
        TORCH_DEFAULT_UNIFORM_V1,
        WIDENING_SEED_V1,
    )
    .unwrap();
    let mut state = widened.state.clone();
    let classes: Vec<Vec<WideCoordinateClassV1>> = (0..PARAMETER_TENSOR_COUNT)
        .map(wide_coordinate_classes_v1)
        .collect();
    let mut first_active = BTreeMap::<&str, usize>::new();
    let mut rows = Vec::with_capacity(STEP_AGE_UPDATES_V1);
    let started = std::time::Instant::now();
    for update in 1..=STEP_AGE_UPDATES_V1 {
        let before = state.snapshot_v1().unwrap();
        let result = route_update_dims_v1(&mut state, WideRouteV1::GaeExactParallel);
        let after = state.snapshot_v1().unwrap();
        let mut per_class = BTreeMap::<WideCoordinateClassV1, Vec<f64>>::new();
        let mut moved = BTreeMap::<WideCoordinateClassV1, u64>::new();
        for tensor in 0..PARAMETER_TENSOR_COUNT {
            for (index, &class) in classes[tensor].iter().enumerate() {
                let delta = f64::from(after.parameters[tensor].values[index])
                    - f64::from(before.parameters[tensor].values[index]);
                if delta != 0.0 {
                    *moved.entry(class).or_default() += 1;
                }
                if after.first_moments[tensor].values[index].to_bits() != 0 {
                    per_class
                        .entry(class)
                        .or_default()
                        .push(delta.abs() / f64::from(LR));
                }
            }
        }
        let mut class_rows = serde_json::Map::new();
        for class in WideCoordinateClassV1::ALL {
            let mut values = per_class.remove(&class).unwrap_or_default();
            if !values.is_empty() {
                first_active.entry(class.name_v1()).or_insert(update);
            }
            let row = if values.is_empty() {
                serde_json::json!({"active": 0, "moved": moved.get(&class).copied().unwrap_or(0)})
            } else {
                let max = values.iter().copied().fold(0.0f64, f64::max);
                serde_json::json!({
                    "active": values.len(),
                    "moved": moved.get(&class).copied().unwrap_or(0),
                    "median": quantile_v1(&mut values, 0.5),
                    "p90": quantile_v1(&mut values, 0.9),
                    "max": max,
                })
            };
            class_rows.insert(class.name_v1().into(), row);
        }
        let k = update as u64;
        rows.push(serde_json::json!({
            "update": update,
            "adam_step": result.adam_step,
            "loss": result.loss,
            "classes": class_rows,
            "reference_constant_gradient_k_equals_update": constant_gradient_multiplier_v1(k, None),
            "reference_constant_gradient_k_equals_update_minus_1":
                if k > 1 { Some(constant_gradient_multiplier_v1(k - 1, None)) } else { None },
            "reference_with_global_bias_correction":
                constant_gradient_multiplier_v1(k, Some(B512_ADAM_STEP_V1 + k)),
        }));
    }
    let peak = (1..=STEP_AGE_UPDATES_V1 as u64)
        .map(|k| (k, constant_gradient_multiplier_v1(k, None)))
        .fold(
            (0, 0.0),
            |best, item| if item.1 > best.1 { item } else { best },
        );
    let report = serde_json::json!({
        "schema": "mtg-kernel-pr4a-adam-step-age-trajectory-v1",
        "design": "WIDE-BACKWARD-DESIGN.md v2.1 section 7.6 (measurement only, optimizer unchanged)",
        "definition": "per update and coordinate class: |parameter after - parameter before| / learning_rate over active coordinates (first moment nonzero after the update); moved counts every coordinate whose parameter changed",
        "parent": "perturbed runner-fixed net-8 after two GAE updates on the gradient fixture, then adam_step set to the b512 dose checkpoint's 118584 (OH2 r2-s1 doses/b512.json)",
        "inherited_adam_step": B512_ADAM_STEP_V1,
        "widening": widened.provenance,
        "route": "generic CPU GAE train step, exact parallel backward, 4 forward workers, 100 groups (199 substeps) of the real Map V3 decisions, one fixed batch every update (wide_train_step_v1 GaeExactParallel)",
        "learning_rate": LR,
        "value_coefficient": VC,
        "updates": STEP_AGE_UPDATES_V1,
        "reference_constant_gradient": "(1 - 0.9^k) / sqrt(1 - 0.999^k): 3.16 at k = 1, peak 6.57 at k = 12",
        "reference_peak": {"k": peak.0, "multiplier": peak.1},
        "reference_bound_changing_gradients": 7.27,
        "first_active_update": first_active,
        "elapsed_seconds": started.elapsed().as_secs_f64(),
        "trajectory": rows,
    });
    let text = serde_json::to_string_pretty(&report).unwrap();
    match std::env::var_os("MTG_KERNEL_PR4A_STEP_AGE_OUT") {
        Some(path) => std::fs::write(path, text).unwrap(),
        None => println!("{text}"),
    }
}
