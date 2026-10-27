//! The pre-screen fitting entry point (design section 3, PR 4a row, v2.1):
//! terminal-return targets only, provenance, the policy term exactly off,
//! held-out reporting through the tape forward, and a small end-to-end fit
//! at both widths.

use super::*;
use crate::expanded_deck_training_v1::encoded_decision_view_generic_v1;
use crate::native_flat_tensorizer_v2::NativeFlatDecisionTensorV2;
use crate::native_policy_train_step_v1::screen_fit_v1::{
    evaluate_screen_heldout_dims_v1, fit_screen_value_dims_v1, screen_minibatch_order_v1,
    ScreenDecisionGroupV1, ScreenFitConfigV1, ScreenFitErrorV1, ScreenFitReportV1,
    ScreenGameOutcomeV1, ScreenGameRecordV1, ScreenSubstepV1, TerminalReturnTargetV1,
    SCREEN_POLICY_TERM_V1, SCREEN_VALUE_FIT_ENTRY_VERSION_V1, TERMINAL_RETURN_TARGET_SOURCE_V1,
};
use crate::native_policy_train_step_v1::widening_v1::{
    hex_sha256_v1, widen_net8_snapshot_v1, TORCH_DEFAULT_UNIFORM_V1,
};
use crate::rl::{PlayerSeatV1, TerminalClassificationV1, TerminalOutcomeV1, TerminalSafeCodeV2};
use crate::sideboard_play_policy_v1::FreshLineageGenerationV1 as Generation;

fn tensor_v1(decision: &GradientDecisionV1) -> NativeFlatDecisionTensorV2 {
    NativeFlatDecisionTensorV2 {
        state: decision.state.clone(),
        object_features: decision.object_features.clone(),
        object_card_ids: decision.object_card_ids.clone(),
        object_groups: decision.object_groups.clone(),
        object_node_ids: decision.object_node_ids.clone(),
        edge_features: decision.edge_features.clone(),
        edge_source_indices: decision.edge_source_indices.clone(),
        edge_target_indices: decision.edge_target_indices.clone(),
        action_features: decision.action_features.clone(),
        action_ref_features: decision.action_ref_features.clone(),
        action_ref_card_ids: decision.action_ref_card_ids.clone(),
        action_ref_action_indices: decision.action_ref_action_indices.clone(),
        action_ref_node_indices: decision.action_ref_node_indices.clone(),
    }
}

fn natural_v1(winner: Option<PlayerSeatV1>) -> ScreenGameOutcomeV1 {
    let (terminal_outcome, terminal_reward) = match winner {
        Some(PlayerSeatV1::P0) => (TerminalOutcomeV1::P0Win, [1, -1]),
        Some(PlayerSeatV1::P1) => (TerminalOutcomeV1::P1Win, [-1, 1]),
        None => (TerminalOutcomeV1::Draw, [0, 0]),
    };
    ScreenGameOutcomeV1 {
        terminal_outcome,
        terminal_classification: TerminalClassificationV1::Natural,
        terminal_code: TerminalSafeCodeV2::NaturalGameOver,
        winner,
        terminal_reward,
    }
}

/// A synthetic recorded game: `groups` decision groups, alternating seats,
/// one or two substeps each, over seeded synthetic V3 decisions.
fn game_v1(id: &str, seed: u64, winner: Option<PlayerSeatV1>, groups: usize) -> ScreenGameRecordV1 {
    ScreenGameRecordV1 {
        game_id: id.into(),
        outcome: natural_v1(winner),
        groups: (0..groups)
            .map(|group| ScreenDecisionGroupV1 {
                actor: (group % 2) as u8,
                substeps: (0..1 + group % 2)
                    .map(|substep| ScreenSubstepV1 {
                        tensor: tensor_v1(&GradientDecisionV1::new_v1(
                            seed * 100 + (group * 2 + substep) as u64,
                        )),
                        selected_action_index: (group + 2 * substep) % GRADIENT_ACTIONS_V1,
                    })
                    .collect(),
            })
            .collect(),
    }
}

fn fit_games_v1() -> Vec<ScreenGameRecordV1> {
    vec![
        game_v1("fit-a", 1, Some(PlayerSeatV1::P0), 3),
        game_v1("fit-b", 2, Some(PlayerSeatV1::P1), 3),
        game_v1("fit-c", 3, None, 2),
    ]
}

fn heldout_games_v1() -> Vec<ScreenGameRecordV1> {
    vec![
        game_v1("held-a", 11, Some(PlayerSeatV1::P1), 2),
        game_v1("held-b", 12, Some(PlayerSeatV1::P0), 3),
    ]
}

fn config_v1(steps: u64, batch_groups: usize) -> ScreenFitConfigV1 {
    ScreenFitConfigV1 {
        value_target_source: TERMINAL_RETURN_TARGET_SOURCE_V1.into(),
        steps,
        batch_groups,
        learning_rate: 1.0e-3,
        value_coefficient: 0.5,
        minibatch_seed: 0x5C2E_E000,
        heldout_every_steps: 2,
        forward_workers: 1,
    }
}

fn narrow_state_v1() -> NativePolicyValueTrainStateV1 {
    NativePolicyValueTrainStateV1::new_v1(perturbed_dims_v1::<Net8DimsV1>(
        narrow_model_v1(),
        0x5C_0001,
    ))
    .unwrap()
}

fn widened_state_v1(narrow: &NativePolicyValueTrainStateV1) -> NativePolicyValueTrainStateWideV1 {
    widen_net8_snapshot_v1(
        &narrow.snapshot_v1().unwrap(),
        W_MODEL_ARCHITECTURE_VERSION_V1,
        TORCH_DEFAULT_UNIFORM_V1,
        7,
    )
    .unwrap()
    .state
}

fn fit_v1<D: NetDimsV1>(
    state: &mut NativePolicyValueTrainStateDimsV1<D>,
    fit: &[ScreenGameRecordV1],
    heldout: &[ScreenGameRecordV1],
    config: &ScreenFitConfigV1,
) -> Result<ScreenFitReportV1, ScreenFitErrorV1> {
    fit_screen_value_dims_v1(state, Generation::V3, fit, heldout, config)
}

/// Value targets come only from naturally finished games: a truncated,
/// halted or decision-capped game, or one whose outcome, winner and rewards
/// disagree, is refused before any work, as is any value target source
/// other than `terminal_return_v1`; the screen note's JSON must name every
/// field and nothing else.
#[test]
fn screen_fit_refuses_non_terminal_targets_and_other_target_sources() {
    let mut state = narrow_state_v1();
    let before = state.state_sha256_v1().unwrap();
    let config = config_v1(2, 2);
    let mut non_terminal = Vec::new();
    for (outcome, classification, code, reward) in [
        (
            TerminalOutcomeV1::Truncated,
            TerminalClassificationV1::Truncated,
            TerminalSafeCodeV2::DecisionCap,
            [0, 0],
        ),
        (
            TerminalOutcomeV1::Halted,
            TerminalClassificationV1::Halted,
            TerminalSafeCodeV2::FailClosed,
            [0, 0],
        ),
        // A natural classification under a decision-cap code.
        (
            TerminalOutcomeV1::Draw,
            TerminalClassificationV1::Natural,
            TerminalSafeCodeV2::DecisionCap,
            [0, 0],
        ),
        // Rewards that contradict the winner.
        (
            TerminalOutcomeV1::P0Win,
            TerminalClassificationV1::Natural,
            TerminalSafeCodeV2::NaturalGameOver,
            [-1, 1],
        ),
        // A shaped (non-unit) return.
        (
            TerminalOutcomeV1::P0Win,
            TerminalClassificationV1::Natural,
            TerminalSafeCodeV2::NaturalGameOver,
            [2, -2],
        ),
    ] {
        let mut game = game_v1("bad", 9, Some(PlayerSeatV1::P0), 2);
        game.outcome = ScreenGameOutcomeV1 {
            terminal_outcome: outcome,
            terminal_classification: classification,
            terminal_code: code,
            winner: matches!(outcome, TerminalOutcomeV1::P0Win).then_some(PlayerSeatV1::P0),
            terminal_reward: reward,
        };
        non_terminal.push(game);
    }
    for bad in non_terminal {
        assert_eq!(
            TerminalReturnTargetV1::of_game_v1(&bad, 0).unwrap_err(),
            ScreenFitErrorV1::NonTerminalOutcome {
                game_id: "bad".into()
            }
        );
        for (fit, heldout) in [
            (
                [fit_games_v1(), vec![bad.clone()]].concat(),
                heldout_games_v1(),
            ),
            (
                fit_games_v1(),
                [heldout_games_v1(), vec![bad.clone()]].concat(),
            ),
        ] {
            assert_eq!(
                fit_v1(&mut state, &fit, &heldout, &config).unwrap_err(),
                ScreenFitErrorV1::NonTerminalOutcome {
                    game_id: "bad".into()
                }
            );
        }
    }
    for source in [
        "gae_advantage_value/v1",
        "bootstrapped_value_v1",
        "search_value_v1",
        "teacher_value_v1",
        "",
    ] {
        let mut other = config.clone();
        other.value_target_source = source.into();
        assert_eq!(
            fit_v1(&mut state, &fit_games_v1(), &heldout_games_v1(), &other).unwrap_err(),
            ScreenFitErrorV1::UnsupportedValueTargetSource {
                requested: source.into()
            }
        );
    }
    // The screen note's JSON: every field required, unknown fields refused.
    let json = serde_json::to_value(&config).unwrap();
    assert_eq!(
        serde_json::from_value::<ScreenFitConfigV1>(json.clone()).unwrap(),
        config
    );
    for field in json.as_object().unwrap().keys() {
        let mut missing = json.clone();
        missing.as_object_mut().unwrap().remove(field);
        assert!(
            serde_json::from_value::<ScreenFitConfigV1>(missing).is_err(),
            "{field}"
        );
    }
    let mut extra = json.clone();
    extra
        .as_object_mut()
        .unwrap()
        .insert("value_targets".into(), serde_json::json!([0.5, -0.5]));
    assert!(serde_json::from_value::<ScreenFitConfigV1>(extra).is_err());
    assert_eq!(
        state.state_sha256_v1().unwrap(),
        before,
        "refusals change nothing"
    );
}

#[test]
fn screen_fit_validates_every_caller_parameter_and_the_split() {
    let mut state = narrow_state_v1();
    let (fit, heldout) = (fit_games_v1(), heldout_games_v1());
    let invalid = |field: &'static str| ScreenFitErrorV1::InvalidConfig { field };
    let cases: [(fn(&mut ScreenFitConfigV1), ScreenFitErrorV1); 9] = [
        (|c| c.steps = 0, invalid("steps")),
        (|c| c.batch_groups = 0, invalid("batch_groups")),
        (|c| c.learning_rate = 0.0, invalid("learning_rate")),
        (|c| c.learning_rate = f32::NAN, invalid("learning_rate")),
        (|c| c.value_coefficient = -1.0, invalid("value_coefficient")),
        (
            |c| c.heldout_every_steps = 0,
            invalid("heldout_every_steps"),
        ),
        (
            |c| c.heldout_every_steps = 3,
            invalid("heldout_every_steps"),
        ),
        (|c| c.forward_workers = 0, invalid("forward_workers")),
        (|c| c.forward_workers = 65, invalid("forward_workers")),
    ];
    for (edit, expected) in cases {
        let mut config = config_v1(2, 2);
        edit(&mut config);
        assert_eq!(
            fit_v1(&mut state, &fit, &heldout, &config).unwrap_err(),
            expected
        );
    }
    assert_eq!(
        fit_v1(&mut state, &fit, &heldout, &config_v1(2, 9)).unwrap_err(),
        ScreenFitErrorV1::BatchLargerThanFitSet {
            batch_groups: 9,
            fit_groups: 8
        }
    );
    let config = config_v1(2, 2);
    assert_eq!(
        fit_v1(&mut state, &[], &heldout, &config).unwrap_err(),
        ScreenFitErrorV1::EmptyGameSet { split: "fit" }
    );
    assert_eq!(
        fit_v1(&mut state, &fit, &[], &config).unwrap_err(),
        ScreenFitErrorV1::EmptyGameSet { split: "heldout" }
    );
    let mut shared = heldout.clone();
    shared[1].game_id = "fit-b".into();
    assert_eq!(
        fit_v1(&mut state, &fit, &shared, &config).unwrap_err(),
        ScreenFitErrorV1::DuplicateGameId {
            game_id: "fit-b".into()
        }
    );
    let mut empty_game = fit.clone();
    empty_game[0].groups.clear();
    assert_eq!(
        fit_v1(&mut state, &empty_game, &heldout, &config).unwrap_err(),
        ScreenFitErrorV1::EmptyGame {
            game_id: "fit-a".into()
        }
    );
    let mut empty_group = fit.clone();
    empty_group[1].groups[2].substeps.clear();
    assert_eq!(
        fit_v1(&mut state, &empty_group, &heldout, &config).unwrap_err(),
        ScreenFitErrorV1::EmptyGroup {
            game_id: "fit-b".into(),
            group: 2
        }
    );
    let mut bad_actor = fit.clone();
    bad_actor[2].groups[0].actor = 2;
    assert_eq!(
        fit_v1(&mut state, &bad_actor, &heldout, &config).unwrap_err(),
        ScreenFitErrorV1::InvalidActor {
            game_id: "fit-c".into(),
            actor: 2
        }
    );
}

/// The targets are the acting seat's terminal return; the report records
/// every game id, the outcome source hash (which moves with an outcome but
/// not with a decision input), the decision source hash (the converse), the
/// minibatch order and the state hashes around the fit.
#[test]
fn screen_fit_records_game_ids_and_outcome_provenance() {
    let (fit, heldout) = (fit_games_v1(), heldout_games_v1());
    let target = |game: &ScreenGameRecordV1, actor| {
        TerminalReturnTargetV1::of_game_v1(game, actor)
            .unwrap()
            .terminal_return_v1()
    };
    assert_eq!((target(&fit[0], 0), target(&fit[0], 1)), (1, -1));
    assert_eq!((target(&fit[1], 0), target(&fit[1], 1)), (-1, 1));
    assert_eq!((target(&fit[2], 0), target(&fit[2], 1)), (0, 0));
    let config = config_v1(3, 3);
    let mut state = narrow_state_v1();
    let initial = hex_sha256_v1(state.state_sha256_v1().unwrap());
    let report = fit_v1(&mut state, &fit, &heldout, &config).unwrap();
    assert_eq!(report.entry_version, SCREEN_VALUE_FIT_ENTRY_VERSION_V1);
    assert_eq!(report.architecture, MODEL_ARCHITECTURE_VERSION_V1);
    assert_eq!(report.input_generation, "v3");
    assert_eq!(report.value_target_source, TERMINAL_RETURN_TARGET_SOURCE_V1);
    assert_eq!(report.policy_term, SCREEN_POLICY_TERM_V1);
    assert_eq!(report.config, config);
    assert_eq!(report.learning_rate_bits, config.learning_rate.to_bits());
    assert_eq!(report.fit_game_ids, ["fit-a", "fit-b", "fit-c"]);
    assert_eq!(report.heldout_game_ids, ["held-a", "held-b"]);
    assert_eq!((report.fit_group_count, report.heldout_group_count), (8, 5));
    assert_eq!(report.initial_state_sha256, initial);
    assert_eq!(
        report.final_state_sha256,
        hex_sha256_v1(state.state_sha256_v1().unwrap())
    );
    assert_eq!((report.initial_adam_step, report.final_adam_step), (0, 3));
    assert_eq!(report.steps.len(), 3);
    for hash in [
        &report.outcome_source_sha256,
        &report.decision_source_sha256,
        &report.minibatch_order_sha256,
    ] {
        assert_eq!(hash.len(), 64);
    }
    let rerun = |fit: &[ScreenGameRecordV1]| {
        fit_v1(&mut narrow_state_v1(), fit, &heldout, &config).unwrap()
    };
    // Flip one fit game's outcome: the outcome hash moves, the decision hash
    // does not.
    let mut flipped = fit.clone();
    flipped[2].outcome = natural_v1(Some(PlayerSeatV1::P1));
    let other = rerun(&flipped);
    assert_ne!(other.outcome_source_sha256, report.outcome_source_sha256);
    assert_eq!(other.decision_source_sha256, report.decision_source_sha256);
    // Change one decision input: the converse.
    let mut moved = fit.clone();
    moved[0].groups[0].substeps[0].tensor.state[0] += 0.25;
    let other = rerun(&moved);
    assert_eq!(other.outcome_source_sha256, report.outcome_source_sha256);
    assert_ne!(other.decision_source_sha256, report.decision_source_sha256);
    // The order hash is the documented minibatch order's.
    let order = screen_minibatch_order_v1(config.minibatch_seed, 8, 3, 3);
    assert!(order.iter().all(|batch| batch.len() == 3));
    let mut first_epoch: Vec<usize> = order.concat()[..8].to_vec();
    first_epoch.sort_unstable();
    assert_eq!(first_epoch, (0..8).collect::<Vec<_>>());
    let json = serde_json::to_value(&report).unwrap();
    assert_eq!(json["fit_game_ids"][1], "fit-b");
    assert!(json["outcome_source_sha256"].is_string());
}

/// The policy term is exactly off: every step's policy sum is zero, and
/// changing every recorded action leaves the fitted state bit-identical
/// (with the policy term on, the selected actions would move it).
#[test]
fn screen_fit_policy_term_is_exactly_off() {
    let (fit, heldout) = (fit_games_v1(), heldout_games_v1());
    let config = config_v1(4, 3);
    let mut state = narrow_state_v1();
    let report = fit_v1(&mut state, &fit, &heldout, &config).unwrap();
    assert!(report.steps.iter().all(|step| step.policy_sum == 0.0));
    let mut relabelled = fit.clone();
    for group in relabelled.iter_mut().flat_map(|game| &mut game.groups) {
        for substep in &mut group.substeps {
            substep.selected_action_index =
                (substep.selected_action_index + 1) % GRADIENT_ACTIONS_V1;
        }
    }
    let mut other_state = narrow_state_v1();
    let other = fit_v1(&mut other_state, &relabelled, &heldout, &config).unwrap();
    assert_ne!(other.decision_source_sha256, report.decision_source_sha256);
    assert_eq!(other.final_state_sha256, report.final_state_sha256);
    for (left, right) in report.steps.iter().zip(&other.steps) {
        assert_eq!(left.loss.to_bits(), right.loss.to_bits());
        assert_eq!(left.value_sum.to_bits(), right.value_sum.to_bits());
    }
    for (left, right) in report.heldout.iter().zip(&other.heldout) {
        assert_eq!(left.value_mse.to_bits(), right.value_mse.to_bits());
    }
}

/// Held-out evaluation before the first update, every `heldout_every_steps`
/// updates and after the last; its value MSE and descriptive policy NLL
/// equal an independent recomputation from the inference forward, and its
/// per-game sums add up.
#[test]
fn screen_fit_reports_heldout_value_and_policy_losses() {
    let (fit, heldout) = (fit_games_v1(), heldout_games_v1());
    let mut config = config_v1(5, 2);
    config.heldout_every_steps = 2;
    let mut state = narrow_state_v1();
    let model = state.model_v1().clone();
    let report = fit_v1(&mut state, &fit, &heldout, &config).unwrap();
    let after: Vec<u64> = report.heldout.iter().map(|eval| eval.after_step).collect();
    assert_eq!(after, [0, 2, 4, 5]);
    let (mut value_total, mut policy_total, mut groups) = (0.0f64, 0.0f64, 0usize);
    for (game, record) in heldout.iter().zip(&report.heldout[0].games) {
        assert_eq!(record.game_id, game.game_id);
        assert_eq!(record.groups, game.groups.len());
        let (mut value_sum, mut policy_sum) = (0.0f64, 0.0f64);
        for group in &game.groups {
            let target = TerminalReturnTargetV1::of_game_v1(game, group.actor)
                .unwrap()
                .value_target_v1();
            let mut joint = 0.0f64;
            let mut value = None;
            for substep in &group.substeps {
                let output = model
                    .forward_feature_transfer_v3(encoded_decision_view_generic_v1(
                        &substep.tensor,
                        Generation::V3,
                    ))
                    .unwrap();
                let (log_probability, _) = selected_log_softmax_math_v1(
                    &output.logits,
                    substep.selected_action_index,
                    TrainingMathV1::PlatformLibm,
                )
                .unwrap();
                joint += f64::from(log_probability);
                value.get_or_insert(output.value);
            }
            let error = f64::from(value.unwrap()) - f64::from(target);
            value_sum += error * error;
            policy_sum -= joint;
        }
        assert_eq!(
            record.value_squared_error_sum.to_bits(),
            value_sum.to_bits()
        );
        assert_eq!(record.policy_nll_sum.to_bits(), policy_sum.to_bits());
        value_total += value_sum;
        policy_total += policy_sum;
        groups += game.groups.len();
    }
    assert_eq!(report.heldout[0].value_mse, value_total / groups as f64);
    assert_eq!(
        report.heldout[0].policy_nll_mean,
        policy_total / groups as f64
    );
    // The final evaluation is of the fitted state.
    let final_eval = evaluate_screen_heldout_dims_v1(&state, Generation::V3, &heldout, 5).unwrap();
    assert_eq!(&final_eval, report.heldout.last().unwrap());
}

/// A small end-to-end fit at both widths through the generic train step:
/// the widened start reports the narrow start's held-out losses exactly
/// (function preserved), each fit lowers its full-batch value loss, the
/// report is reproducible, and forward workers do not change it.
#[test]
fn screen_fit_end_to_end_at_both_widths() {
    let (fit, heldout) = (fit_games_v1(), heldout_games_v1());
    let mut config = config_v1(12, 8);
    config.heldout_every_steps = 4;
    let narrow_start = narrow_state_v1();
    let wide_start = widened_state_v1(&narrow_start);
    let mut narrow = narrow_start.clone();
    let narrow_report = fit_v1(&mut narrow, &fit, &heldout, &config).unwrap();
    let mut wide = wide_start.clone();
    let wide_report = fit_v1(&mut wide, &fit, &heldout, &config).unwrap();
    assert_eq!(wide_report.architecture, W_MODEL_ARCHITECTURE_VERSION_V1);
    assert_eq!(
        narrow_report.heldout[0].value_mse,
        wide_report.heldout[0].value_mse
    );
    assert_eq!(
        narrow_report.heldout[0].policy_nll_mean,
        wide_report.heldout[0].policy_nll_mean
    );
    for report in [&narrow_report, &wide_report] {
        let (first, last) = (&report.steps[0], report.steps.last().unwrap());
        assert!(
            last.value_sum < first.value_sum,
            "{}: {} -> {}",
            report.architecture,
            first.value_sum,
            last.value_sum
        );
        assert_eq!(report.heldout.len(), 4);
    }
    let mut again = wide_start.clone();
    assert_eq!(
        fit_v1(&mut again, &fit, &heldout, &config).unwrap(),
        wide_report
    );
    let mut workers = config.clone();
    workers.forward_workers = 4;
    let mut parallel = wide_start;
    let parallel_report = fit_v1(&mut parallel, &fit, &heldout, &workers).unwrap();
    assert_eq!(
        parallel_report.final_state_sha256,
        wide_report.final_state_sha256
    );
    assert_eq!(parallel_report.heldout, wide_report.heldout);
}
