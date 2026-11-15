//! The pregame announcement advantage scale (`pregame_advantage_scale`): the
//! normalized advantage of the pregame announcement rows, and only those,
//! times `s` in the policy loss; put-back rows, in-game rows, every value
//! target and the receipt statistics are exactly as without it; absent is
//! bit-identical; the key is part of the loss selection.

use super::*;

struct Batch {
    values: Vec<f32>,
    returns: Vec<i8>,
    pregame: Vec<bool>,
    announcement: Vec<bool>,
    ordinals: Vec<usize>,
}

/// Episode 0: two announcements and a put-back decision (pregame), then four
/// in-game decisions, learner won. Episode 1: one announcement, three in-game
/// decisions, learner lost.
fn batch() -> Batch {
    let values = vec![
        -0.3125, -0.25, -0.1875, 0.0625, 0.1875, -0.125, 0.5, // episode 0
        -0.3125, 0.25, -0.0625, -0.375, // episode 1
    ];
    let pregame = vec![
        true, true, true, false, false, false, false, //
        true, false, false, false,
    ];
    let announcement = vec![
        true, true, false, false, false, false, false, //
        true, false, false, false,
    ];
    let mut returns = vec![1i8; 7];
    returns.extend([-1i8; 4]);
    let mut ordinals = vec![0usize; 7];
    ordinals.extend([1usize; 4]);
    Batch {
        values,
        returns,
        pregame,
        announcement,
        ordinals,
    }
}

fn unscaled(batch: &Batch) -> GaeTargetsV1 {
    terminal_pregame_targets_of_values_v1(
        &batch.values,
        &batch.returns,
        &batch.pregame,
        &batch.ordinals,
        1.0,
        0.9,
    )
    .unwrap()
}

fn scaled(batch: &Batch, scale: Option<f32>) -> GaeTargetsV1 {
    terminal_pregame_targets_scaled_of_values_v1(
        &batch.values,
        &batch.returns,
        &batch.pregame,
        &batch.announcement,
        &batch.ordinals,
        1.0,
        0.9,
        scale,
    )
    .unwrap()
}

fn bits(values: &[f32]) -> Vec<u32> {
    values.iter().map(|value| value.to_bits()).collect()
}

#[test]
fn an_absent_scale_is_the_unscaled_function_bit_for_bit_and_writes_no_key() {
    let batch = batch();
    let plain = unscaled(&batch);
    let absent = scaled(&batch, None);
    assert_eq!(bits(&absent.advantages), bits(&plain.advantages));
    assert_eq!(bits(&absent.value_targets), bits(&plain.value_targets));
    assert_eq!(absent.statistics, plain.statistics);
    assert_eq!(absent.pregame, plain.pregame);
    let receipt = serde_json::to_value(absent.pregame.unwrap()).unwrap();
    for key in [
        "scale",
        "scaled_groups",
        "announcement_advantage_mean_unscaled",
        "announcement_advantage_mean_scaled",
    ] {
        assert!(receipt.get(key).is_none(), "{key}");
    }
}

#[test]
fn a_scale_of_one_changes_no_advantage_or_target() {
    let batch = batch();
    let plain = unscaled(&batch);
    let one = scaled(&batch, Some(1.0));
    assert_eq!(bits(&one.advantages), bits(&plain.advantages));
    assert_eq!(bits(&one.value_targets), bits(&plain.value_targets));
    assert_eq!(one.statistics, plain.statistics);
}

#[test]
fn the_scale_multiplies_the_announcement_rows_only() {
    let batch = batch();
    let plain = unscaled(&batch);
    for scale in [0.5f32, 0.1, 0.003, 0.9999] {
        let result = scaled(&batch, Some(scale));
        for index in 0..batch.values.len() {
            let expected = if batch.announcement[index] {
                plain.advantages[index] * scale
            } else {
                plain.advantages[index]
            };
            assert_eq!(
                result.advantages[index].to_bits(),
                expected.to_bits(),
                "row {index}, scale {scale}"
            );
            // The value loss is untouched for every row, announcements too.
            assert_eq!(
                result.value_targets[index].to_bits(),
                plain.value_targets[index].to_bits(),
                "target of row {index}, scale {scale}"
            );
        }
        // Put-back rows are pregame rows that are not scaled.
        assert!(batch.pregame[2] && !batch.announcement[2]);
        assert_eq!(
            result.advantages[2].to_bits(),
            plain.advantages[2].to_bits()
        );
        assert_eq!(result.statistics, plain.statistics);
    }
}

#[test]
fn the_receipt_names_the_scale_the_rows_and_both_means() {
    let batch = batch();
    let plain = unscaled(&batch);
    let result = scaled(&batch, Some(0.25));
    let receipt = result.pregame.unwrap();
    assert_eq!(receipt.scale, Some(0.25));
    assert_eq!(receipt.scaled_groups, Some(3));
    let unscaled_mean = (plain.advantages[0] + plain.advantages[1] + plain.advantages[7]) / 3.0;
    let written = serde_json::to_value(receipt).unwrap();
    assert_eq!(written["scale"], 0.25);
    assert_eq!(written["scaled_groups"], 3);
    let got = written["announcement_advantage_mean_unscaled"]
        .as_f64()
        .unwrap() as f32;
    assert!(
        (got - unscaled_mean).abs() <= 1.0e-6,
        "{got} {unscaled_mean}"
    );
    let got_scaled = written["announcement_advantage_mean_scaled"]
        .as_f64()
        .unwrap() as f32;
    assert!((got_scaled - 0.25 * unscaled_mean).abs() <= 1.0e-6);
    // No announcement row: the means are null and the count is zero.
    let mut none = batch;
    none.announcement = vec![false; none.values.len()];
    let written = serde_json::to_value(scaled(&none, Some(0.25)).pregame.unwrap()).unwrap();
    assert_eq!(written["scaled_groups"], 0);
    assert!(written["announcement_advantage_mean_unscaled"].is_null());
    assert!(written["announcement_advantage_mean_scaled"].is_null());
}

#[test]
fn announcement_marks_of_a_different_length_are_refused() {
    let batch = batch();
    assert!(terminal_pregame_targets_scaled_of_values_v1(
        &batch.values,
        &batch.returns,
        &batch.pregame,
        &batch.announcement[..5],
        &batch.ordinals,
        1.0,
        0.9,
        Some(0.5),
    )
    .is_err());
}

fn selection(
    pregame_advantage: PregameAdvantageV1,
    pregame_advantage_scale: Option<f32>,
) -> ExpandedLossSelectionV1 {
    ExpandedLossSelectionV1::GaeAdvantageValueV1 {
        gamma: 1.0,
        lambda: 0.9,
        entropy_coefficient: 0.0,
        pregame_advantage,
        pregame_advantage_scale,
    }
}

#[test]
fn the_scale_is_validated_and_recorded_only_when_set() {
    let terminal = PregameAdvantageV1::TerminalV1;
    for good in [1.0f32, 0.5, 0.003, f32::MIN_POSITIVE] {
        selection(terminal, Some(good)).validate_v1().unwrap();
    }
    for bad in [0.0f32, -0.5, 1.0000001, 2.0, f32::NAN, f32::INFINITY] {
        assert!(
            selection(terminal, Some(bad)).validate_v1().is_err(),
            "scale {bad}"
        );
    }
    // Only with the terminal pregame advantage.
    assert!(selection(PregameAdvantageV1::Gae, Some(0.5))
        .validate_v1()
        .is_err());
    // Absent: the selection and its bytes are the unscaled terminal selection.
    let absent = selection(terminal, None);
    let written = serde_json::to_value(absent).unwrap();
    assert!(written.get("pregame_advantage_scale").is_none());
    assert_eq!(absent.pregame_advantage_scale_v1(), None);
    // Set: written, read back, and a different selection.
    let set = selection(terminal, Some(0.25));
    let written = serde_json::to_value(set).unwrap();
    assert_eq!(written["pregame_advantage_scale"], 0.25);
    let read: ExpandedLossSelectionV1 = serde_json::from_value(written).unwrap();
    assert_eq!(read, set);
    assert_eq!(read.pregame_advantage_scale_v1(), Some(0.25));
    assert_ne!(set, absent);
    assert_ne!(set, selection(terminal, Some(0.5)));
    assert_ne!(set, selection(terminal, Some(1.0)));
    // The loss identity string is the GAE one throughout.
    assert_eq!(set.loss_identity_v1(), absent.loss_identity_v1());
    // The key on a struct loss other than the plain GAE one is refused at
    // parse time (a unit loss ignores unknown keys, as before).
    assert!(
        serde_json::from_value::<ExpandedLossSelectionV1>(serde_json::json!({
            "kind": "gae_uniform_kl_recovery_v1", "gamma": 1.0, "lambda": 0.9,
            "initial_adam_step": 32400, "recovery": true, "diagnostics": false,
            "pregame_advantage_scale": 0.5,
        }))
        .is_err()
    );
    assert_eq!(
        ExpandedLossSelectionV1::TerminalReinforceValueV3.pregame_advantage_scale_v1(),
        None
    );
}

/// A learner decision with real announcement geometry: two action rows of the
/// FlatV7 width, the London opening kind on both, the VALUE flag on the
/// mulligan row only. `kind` 0: in-game, 1: announcement, 2: put-back.
fn decision(kind: u8) -> NativeFlatDecisionTensorV2 {
    let mut state = vec![0.0; 219];
    if kind != 0 {
        state[london_keep_offset_v1::PREGAME_STATE_COLUMN_V1] = 1.0;
    }
    let width = 195;
    let mut action_features = vec![0.0; 2 * width];
    for row in 0..2 {
        action_features[row * width + london_keep_offset_v1::LONDON_OPENING_ACTION_COLUMN_V1] =
            f32::from(u8::from(kind != 0));
    }
    if kind == 1 {
        action_features[width + london_keep_offset_v1::MULLIGAN_FLAG_ACTION_COLUMN_V1] = 1.0;
    }
    NativeFlatDecisionTensorV2 {
        state,
        action_features,
        ..NativeFlatDecisionTensorV2::default()
    }
}

#[test]
fn announcement_rows_are_found_from_the_tensors_and_put_backs_are_not() {
    let batch = batch();
    let kinds: Vec<u8> = (0..batch.values.len())
        .map(|index| {
            if batch.announcement[index] {
                1
            } else if batch.pregame[index] {
                2
            } else {
                0
            }
        })
        .collect();
    let tensors: Vec<NativeFlatDecisionTensorV2> =
        kinds.iter().map(|kind| decision(*kind)).collect();
    let zero_logits = [0u32; 2];
    let substeps: Vec<Vec<NativePolicySubstepV1<'_>>> = tensors
        .iter()
        .zip(&batch.values)
        .map(|(tensor, value)| {
            vec![NativePolicySubstepV1 {
                forward: NativePolicyForwardInputV1::Encoded(Box::new(
                    encoded_decision_view_generic_v1(tensor, FreshLineageGenerationV1::V4),
                )),
                selected_action_index: 0,
                expected_raw_action_logit_bits: &zero_logits,
                expected_value_bits: value.to_bits(),
            }]
        })
        .collect();
    let groups: Vec<NativePolicyPhysicalDecisionV1<'_>> = substeps
        .iter()
        .zip(&batch.returns)
        .map(
            |(substeps, terminal_return)| NativePolicyPhysicalDecisionV1 {
                substeps,
                terminal_return: *terminal_return,
                baseline_bits: 0,
            },
        )
        .collect();
    let plain = compute_gae_targets_with_pregame_v1(
        &groups,
        &batch.ordinals,
        1.0,
        0.9,
        PregameAdvantageV1::TerminalV1,
        None,
    )
    .unwrap();
    let marked = compute_gae_targets_with_pregame_v1(
        &groups,
        &batch.ordinals,
        1.0,
        0.9,
        PregameAdvantageV1::TerminalV1,
        Some(0.2),
    )
    .unwrap();
    let direct = scaled(&batch, Some(0.2));
    assert_eq!(bits(&marked.advantages), bits(&direct.advantages));
    assert_eq!(bits(&marked.value_targets), bits(&plain.value_targets));
    assert_eq!(marked.pregame.unwrap().scaled_groups, Some(3));
    // A scale does nothing under the default mode (it is refused earlier by
    // the selection; the function ignores it).
    let off = compute_gae_targets_with_pregame_v1(
        &groups,
        &batch.ordinals,
        1.0,
        0.9,
        PregameAdvantageV1::Gae,
        Some(0.2),
    )
    .unwrap();
    let default = compute_gae_targets_v1(&groups, &batch.ordinals, 1.0, 0.9).unwrap();
    assert_eq!(bits(&off.advantages), bits(&default.advantages));
}
