//! The terminal pregame advantage (`PregameAdvantageV1::TerminalV1`): London
//! pregame rows get the advantage `R - V` and the value target `R` (terminal
//! result, own value estimate, no bootstrap), every in-game row keeps the
//! default GAE bit for bit (including its normalization), and the mode is part
//! of the loss selection every command and run manifest records.

use super::*;

/// The default normalization of `raw`, written out (mean, then the std of the
/// deviations, `(x - mean) / (std + 1e-6)`), as the test's own oracle.
fn mean_and_denominator(raw: &[f32]) -> (f32, f32) {
    let count = raw.len() as f32;
    let mean = raw.iter().copied().fold(0.0f32, |sum, value| sum + value) / count;
    let variance = raw.iter().copied().fold(0.0f32, |sum, value| {
        let deviation = value - mean;
        sum + deviation * deviation
    }) / count;
    (mean, variance.sqrt() + 1.0e-6)
}

/// Two episodes. Episode 0: an announcement, a second announcement, a
/// put-back decision (all pregame), then four in-game decisions; the learner
/// won. Episode 1: one announcement, then three in-game decisions; the learner
/// lost. Values are arbitrary but fixed.
struct Batch {
    values: Vec<f32>,
    returns: Vec<i8>,
    pregame: Vec<bool>,
    ordinals: Vec<usize>,
}

fn batch() -> Batch {
    let values = vec![
        -0.3125, -0.25, -0.1875, 0.0625, 0.1875, -0.125, 0.5, // episode 0
        -0.3125, 0.25, -0.0625, -0.375, // episode 1
    ];
    let pregame = vec![
        true, true, true, false, false, false, false, //
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
        ordinals,
    }
}

fn default_targets(batch: &Batch, lambda: f32) -> GaeTargetsV1 {
    compute_gae_targets_of_values_v1(&batch.values, &batch.returns, &batch.ordinals, 1.0, lambda)
        .unwrap()
}

fn terminal_targets(batch: &Batch, lambda: f32) -> GaeTargetsV1 {
    terminal_pregame_targets_of_values_v1(
        &batch.values,
        &batch.returns,
        &batch.pregame,
        &batch.ordinals,
        1.0,
        lambda,
    )
    .unwrap()
}

fn bits(values: &[f32]) -> Vec<u32> {
    values.iter().map(|value| value.to_bits()).collect()
}

#[test]
fn in_game_rows_are_bit_identical_to_the_default_mode() {
    let batch = batch();
    for lambda in [0.9f32, 1.0, 0.5, 0.0] {
        let default = default_targets(&batch, lambda);
        let terminal = terminal_targets(&batch, lambda);
        for index in 0..batch.values.len() {
            if !batch.pregame[index] {
                assert_eq!(
                    terminal.advantages[index].to_bits(),
                    default.advantages[index].to_bits(),
                    "normalized advantage of in-game row {index}, lambda {lambda}"
                );
                assert_eq!(
                    terminal.value_targets[index].to_bits(),
                    default.value_targets[index].to_bits(),
                    "value target of in-game row {index}, lambda {lambda}"
                );
            }
        }
    }
}

#[test]
fn pregame_rows_get_the_terminal_result_minus_their_own_value() {
    let batch = batch();
    let default = default_targets(&batch, 0.9);
    let terminal = terminal_targets(&batch, 0.9);
    // The default series' raw advantages: exactly what an in-game row
    // normalizes from, and what the pregame rows are normalized against.
    let pairs =
        gae_pairs_of_values_v1(&batch.values, &batch.returns, &batch.ordinals, 1.0, 0.9).unwrap();
    let default_raw: Vec<f32> = pairs.iter().map(|(advantage, _)| *advantage).collect();
    let (mean, denominator) = mean_and_denominator(&default_raw);
    #[allow(
        clippy::needless_range_loop,
        reason = "one index across six parallel series"
    )]
    for index in 0..batch.values.len() {
        let expected_default = (default_raw[index] - mean) / denominator;
        assert_eq!(
            default.advantages[index].to_bits(),
            expected_default.to_bits(),
            "the oracle reproduces the default normalization at {index}"
        );
        if batch.pregame[index] {
            let result = f32::from(batch.returns[index]);
            let raw = result - batch.values[index];
            assert_eq!(terminal.value_targets[index].to_bits(), result.to_bits());
            assert_eq!(
                terminal.advantages[index].to_bits(),
                ((raw - mean) / denominator).to_bits(),
                "pregame row {index}: (R - V) under the default statistics"
            );
            // The default mode would have bootstrapped it differently.
            assert_ne!(
                default.value_targets[index].to_bits(),
                terminal.value_targets[index].to_bits()
            );
        }
    }
}

#[test]
fn a_batch_without_pregame_rows_equals_the_default_mode() {
    let mut batch = batch();
    batch.pregame = vec![false; batch.values.len()];
    let default = default_targets(&batch, 0.9);
    let terminal = terminal_targets(&batch, 0.9);
    assert_eq!(bits(&terminal.advantages), bits(&default.advantages));
    assert_eq!(bits(&terminal.value_targets), bits(&default.value_targets));
    assert_eq!(terminal.statistics, default.statistics);
    let receipt = terminal.pregame.unwrap();
    assert_eq!(receipt.pregame_groups, 0);
    assert_eq!(receipt.raw_terminal_mean, None);
    assert_eq!(receipt.raw_default_mean, None);
    assert_eq!(default.pregame, None);
}

#[test]
fn the_receipt_names_the_mode_and_the_two_means() {
    let batch = batch();
    let terminal = terminal_targets(&batch, 0.9);
    let pairs =
        gae_pairs_of_values_v1(&batch.values, &batch.returns, &batch.ordinals, 1.0, 0.9).unwrap();
    let receipt = terminal.pregame.unwrap();
    assert_eq!(receipt.mode, "terminal_v1");
    assert_eq!(receipt.pregame_groups, 4);
    let marked: Vec<usize> = (0..batch.values.len())
        .filter(|index| batch.pregame[*index])
        .collect();
    let terminal_sum: f32 = marked
        .iter()
        .map(|index| f32::from(batch.returns[*index]) - batch.values[*index])
        .fold(0.0, |sum, value| sum + value);
    let default_sum: f32 = marked
        .iter()
        .map(|index| pairs[*index].0)
        .fold(0.0, |sum, value| sum + value);
    assert_eq!(receipt.raw_terminal_mean, Some(terminal_sum / 4.0));
    assert_eq!(receipt.raw_default_mean, Some(default_sum / 4.0));
    let json = serde_json::to_value(receipt).unwrap();
    assert_eq!(json["mode"], "terminal_v1");
    assert_eq!(json["pregame_groups"], 4);
    // The statistics describe the final (normalized) series, not the default.
    let default = default_targets(&batch, 0.9);
    assert_ne!(terminal.statistics, default.statistics);
    assert_eq!(
        terminal.statistics.max,
        terminal
            .advantages
            .iter()
            .copied()
            .fold(f32::NEG_INFINITY, f32::max)
    );
}

#[test]
fn the_mode_refuses_what_it_cannot_do_exactly() {
    let batch = batch();
    // gamma other than exactly 1.0.
    for gamma in [0.99f32, 0.5, 0.0, 1.0 - f32::EPSILON / 2.0] {
        assert!(terminal_pregame_targets_of_values_v1(
            &batch.values,
            &batch.returns,
            &batch.pregame,
            &batch.ordinals,
            gamma,
            0.9,
        )
        .is_err());
    }
    // A pregame decision after an in-game decision of its episode.
    let mut late = batch.pregame.clone();
    late[5] = true;
    assert!(terminal_pregame_targets_of_values_v1(
        &batch.values,
        &batch.returns,
        &late,
        &batch.ordinals,
        1.0,
        0.9,
    )
    .is_err());
    // A pregame row opening the second episode is fine (the check is within
    // an episode), and so is a batch whose first row is in-game.
    assert!(terminal_pregame_targets_of_values_v1(
        &batch.values,
        &batch.returns,
        &batch.pregame,
        &batch.ordinals,
        1.0,
        0.9,
    )
    .is_ok());
    // Counts must agree.
    assert!(terminal_pregame_targets_of_values_v1(
        &batch.values,
        &batch.returns,
        &batch.pregame[..5],
        &batch.ordinals,
        1.0,
        0.9,
    )
    .is_err());
}

#[test]
fn the_loss_selection_carries_the_mode_and_a_default_selection_keeps_its_bytes() {
    let default = ExpandedLossSelectionV1::GaeAdvantageValueV1 {
        gamma: 1.0,
        lambda: 0.9,
        entropy_coefficient: 0.0,
        pregame_advantage: PregameAdvantageV1::default(),
        pregame_announcement_step_scale: None,
    };
    // Written without the key: every earlier config, manifest and receipt
    // keeps its bytes, and an earlier file reads as the default mode.
    let written = serde_json::to_value(default).unwrap();
    assert_eq!(
        written,
        serde_json::json!({
            "kind": "gae_advantage_value_v1",
            "gamma": 1.0,
            "lambda": 0.9_f32 as f64,
            "entropy_coefficient": 0.0,
        })
    );
    let read: ExpandedLossSelectionV1 = serde_json::from_value(written).unwrap();
    assert_eq!(read, default);
    assert_eq!(read.pregame_advantage_v1(), PregameAdvantageV1::Gae);
    // An explicit "gae" is the same selection.
    let explicit: ExpandedLossSelectionV1 = serde_json::from_value(serde_json::json!({
        "kind": "gae_advantage_value_v1", "gamma": 1.0, "lambda": 0.9_f32 as f64,
        "entropy_coefficient": 0.0, "pregame_advantage": "gae",
    }))
    .unwrap();
    assert_eq!(explicit, default);
    // The terminal mode is written, read back, and a different selection (so
    // a run identity that records the selection cannot mix modes).
    let terminal = ExpandedLossSelectionV1::GaeAdvantageValueV1 {
        gamma: 1.0,
        lambda: 0.9,
        entropy_coefficient: 0.0,
        pregame_advantage: PregameAdvantageV1::TerminalV1,
        pregame_announcement_step_scale: None,
    };
    assert_ne!(terminal, default);
    let written = serde_json::to_value(terminal).unwrap();
    assert_eq!(written["pregame_advantage"], "terminal_v1");
    let read: ExpandedLossSelectionV1 = serde_json::from_value(written).unwrap();
    assert_eq!(read, terminal);
    assert_eq!(read.pregame_advantage_v1(), PregameAdvantageV1::TerminalV1);
    terminal.validate_v1().unwrap();
    // The loss identity string is the GAE one for both: checkpoints and
    // receipts of earlier runs read as before.
    assert_eq!(terminal.loss_identity_v1(), default.loss_identity_v1());
    // Unknown modes, and the key on another loss, are refused.
    assert!(
        serde_json::from_value::<ExpandedLossSelectionV1>(serde_json::json!({
            "kind": "gae_advantage_value_v1", "gamma": 1.0, "lambda": 0.9,
            "entropy_coefficient": 0.0, "pregame_advantage": "terminal_v2",
        }))
        .is_err()
    );
    assert!(
        serde_json::from_value::<ExpandedLossSelectionV1>(serde_json::json!({
            "kind": "gae_uniform_kl_recovery_v1", "gamma": 1.0, "lambda": 0.9,
            "initial_adam_step": 32400, "recovery": true, "diagnostics": false,
            "pregame_advantage": "terminal_v1",
        }))
        .is_err()
    );
    assert_eq!(
        ExpandedLossSelectionV1::TerminalReinforceValueV3.pregame_advantage_v1(),
        PregameAdvantageV1::Gae
    );
}

#[test]
fn the_terminal_mode_requires_gamma_exactly_one() {
    for gamma in [0.99f32, 0.0, 1.0 - f32::EPSILON / 2.0] {
        let selection = ExpandedLossSelectionV1::GaeAdvantageValueV1 {
            gamma,
            lambda: 0.9,
            entropy_coefficient: 0.0,
            pregame_advantage: PregameAdvantageV1::TerminalV1,
            pregame_announcement_step_scale: None,
        };
        assert!(selection.validate_v1().is_err(), "gamma {gamma}");
        // The same gamma is fine in the default mode.
        let default = ExpandedLossSelectionV1::GaeAdvantageValueV1 {
            gamma,
            lambda: 0.9,
            entropy_coefficient: 0.0,
            pregame_advantage: PregameAdvantageV1::Gae,
            pregame_announcement_step_scale: None,
        };
        default.validate_v1().unwrap();
    }
}

/// A learner physical decision whose first tensor has the pregame stage
/// column set (or clear).
fn tensor(pregame: bool) -> NativeFlatDecisionTensorV2 {
    let mut state = vec![0.0; 219];
    state[london_keep_offset_v1::PREGAME_STATE_COLUMN_V1] = f32::from(u8::from(pregame));
    NativeFlatDecisionTensorV2 {
        state,
        ..NativeFlatDecisionTensorV2::default()
    }
}

#[test]
fn groups_are_marked_pregame_from_the_first_tensor_stage_column() {
    let batch = batch();
    let tensors: Vec<NativeFlatDecisionTensorV2> = batch
        .pregame
        .iter()
        .map(|pregame| tensor(*pregame))
        .collect();
    let substeps: Vec<Vec<NativePolicySubstepV1<'_>>> = tensors
        .iter()
        .zip(&batch.values)
        .map(|(tensor, value)| {
            vec![NativePolicySubstepV1 {
                forward: NativePolicyForwardInputV1::Encoded(Box::new(
                    encoded_decision_view_generic_v1(tensor, FreshLineageGenerationV1::V4),
                )),
                selected_action_index: 0,
                expected_raw_action_logit_bits: &[],
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
    // The mode off is `compute_gae_targets_v1`, bit for bit.
    let off = compute_gae_targets_with_pregame_v1(
        &groups,
        &batch.ordinals,
        1.0,
        0.9,
        PregameAdvantageV1::Gae,
    )
    .unwrap();
    let plain = compute_gae_targets_v1(&groups, &batch.ordinals, 1.0, 0.9).unwrap();
    assert_eq!(bits(&off.advantages), bits(&plain.advantages));
    assert_eq!(bits(&off.value_targets), bits(&plain.value_targets));
    assert!(off.pregame.is_none());
    // The mode on marks exactly the rows whose tensor says pregame.
    let on = compute_gae_targets_with_pregame_v1(
        &groups,
        &batch.ordinals,
        1.0,
        0.9,
        PregameAdvantageV1::TerminalV1,
    )
    .unwrap();
    let direct = terminal_targets(&batch, 0.9);
    assert_eq!(bits(&on.advantages), bits(&direct.advantages));
    assert_eq!(bits(&on.value_targets), bits(&direct.value_targets));
    assert_eq!(on.pregame, direct.pregame);
    assert_eq!(on.pregame.unwrap().pregame_groups, 4);
    // The mode requires gamma exactly 1.0 here too.
    assert!(compute_gae_targets_with_pregame_v1(
        &groups,
        &batch.ordinals,
        0.99,
        0.9,
        PregameAdvantageV1::TerminalV1,
    )
    .is_err());
}

#[test]
fn normalizing_against_itself_is_the_default_normalization() {
    let raw = [0.5f32, -1.25, 0.125, 2.0, -0.75, 0.0];
    let (own, own_statistics) = normalize_advantages_v1(&raw, 1.0e-6).unwrap();
    let (against, against_statistics) =
        bootstrapped_advantage_v1::normalize_advantages_against_v1(&raw, &raw, 1.0e-6).unwrap();
    assert_eq!(bits(&own), bits(&against));
    assert_eq!(own_statistics, against_statistics);
    assert!(
        bootstrapped_advantage_v1::normalize_advantages_against_v1(&raw, &raw[..5], 1.0e-6)
            .is_err()
    );
}
