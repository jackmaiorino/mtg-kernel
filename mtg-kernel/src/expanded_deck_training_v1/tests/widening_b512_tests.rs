//! Design section 5.6 (a) on collected b512 decisions (PR 4a): the widened
//! b512 gives b512's logits and value under IEEE `==` on decisions b512
//! itself faced in real games, and the pre-screen fit starts from the same
//! held-out losses at both widths. The b512 checkpoint is not in the
//! repository, so the test is `#[ignore]`d and reads its path from
//! `MTG_KERNEL_PR4A_B512_CHECKPOINT` (an OH2 `doses/b512.json`).

use super::*;
use crate::native_policy_train_step_v1::screen_fit_v1::{
    fit_screen_value_dims_v1, ScreenDecisionGroupV1, ScreenFitConfigV1, ScreenGameOutcomeV1,
    ScreenGameRecordV1, ScreenSubstepV1, TERMINAL_RETURN_TARGET_SOURCE_V1,
};
use crate::native_policy_train_step_v1::widening_v1::{
    widen_net8_snapshot_v1, TORCH_DEFAULT_UNIFORM_V1,
};
use crate::native_policy_value_net_v1::W_MODEL_ARCHITECTURE_VERSION_V1;

const CHECKPOINT_ENV_V1: &str = "MTG_KERNEL_PR4A_B512_CHECKPOINT";
const WIDENING_SEED_V1: u64 = 0x0B51_2000_0000_0001;
const GAMES_V1: [(u64, u8, &str, &str); 2] = [
    (2_026_102_701, 0, "Burn", "Spy"),
    (2_026_102_702, 1, "Spy", "Burn"),
];

fn deck_v1(id: &str) -> ExpandedDeckListV1 {
    let registered = crate::sideboard::checked_in_pauper_registered_deck_by_id_v1(id).unwrap();
    ExpandedDeckListV1 {
        label: id.into(),
        mainboard: registered.registered_configuration().mainboard().to_vec(),
        sideboard: registered.registered_configuration().sideboard().to_vec(),
    }
}

/// One collected game as a screen record: every physical decision of both
/// seats (b512 plays both), with its recorded terminal outcome.
fn screen_record_v1(game_id: String, t: &ExpandedTrajectoryV1) -> ScreenGameRecordV1 {
    let mut groups: Vec<ScreenDecisionGroupV1> = Vec::new();
    for row in &t.decisions {
        if row.substep_index == 0 {
            groups.push(ScreenDecisionGroupV1 {
                actor: row.actor,
                substeps: Vec::new(),
            });
        }
        groups
            .last_mut()
            .expect("a physical decision starts at substep 0")
            .substeps
            .push(ScreenSubstepV1 {
                tensor: row.tensor.tensor(),
                selected_action_index: row.selected as usize,
            });
    }
    ScreenGameRecordV1 {
        game_id,
        outcome: ScreenGameOutcomeV1::from(&t.terminal),
        groups,
    }
}

#[test]
#[ignore = "needs MTG_KERNEL_PR4A_B512_CHECKPOINT (an OH2 b512 dose checkpoint); plays two real games"]
fn widened_b512_reproduces_b512_on_its_collected_decisions() {
    let path = std::env::var_os(CHECKPOINT_ENV_V1)
        .expect("set MTG_KERNEL_PR4A_B512_CHECKPOINT to a b512 checkpoint");
    let saved: ExpandedCheckpointV1 =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    let mut model =
        NativePolicyValueNetV1::runner_fixed_v1(NativePolicyValueModelConfigV1::contract_v1())
            .unwrap();
    let template = model.parameter_snapshot_v1();
    let snapshot = NativePolicyValueTrainSnapshotV1 {
        adam_step: saved.adam_step,
        scorer_bias_anchor_bits: saved.scorer_bias_anchor_bits,
        parameters: restore_parameters(&saved.parameters, &template).unwrap(),
        first_moments: restore_parameters(&saved.first_moments, &template).unwrap(),
        second_moments: restore_parameters(&saved.second_moments, &template).unwrap(),
    };
    assert_eq!(
        hex(&snapshot.state_sha256_v1().unwrap()),
        saved.state_sha256,
        "checkpoint state hash"
    );
    model
        .replace_parameter_snapshot_v1(&snapshot.parameters)
        .unwrap();
    let narrow_state =
        NativePolicyValueTrainStateV1::from_snapshot_v1(model.clone(), &snapshot).unwrap();
    let widened = widen_net8_snapshot_v1(
        &snapshot,
        W_MODEL_ARCHITECTURE_VERSION_V1,
        TORCH_DEFAULT_UNIFORM_V1,
        WIDENING_SEED_V1,
    )
    .unwrap();
    let wide = widened.state.model_v1();
    let v4 = saved.feature_contract_digest == FEATURE_CONTRACT_DIGEST_V4;
    let generation = if v4 {
        FreshLineageGenerationV1::V4
    } else {
        FreshLineageGenerationV1::V3
    };
    let mut policy = if v4 {
        FrozenPlayPolicyV1::training_fixture_v4()
    } else {
        FrozenPlayPolicyV1::training_fixture_v3()
    };
    policy
        .replace_training_parameters_v3(&snapshot.parameters)
        .unwrap();
    let learner = test_behavior(&policy, false);
    let mut records = Vec::new();
    let mut decisions = 0usize;
    for (seed, learner_seat, first, second) in GAMES_V1 {
        let episode = ExpandedEpisodeV1 {
            id: format!("pr4a-b512-widening-{seed}"),
            seed,
            starting_player: learner_seat,
            learner_seat,
            opponent: None,
            opponent_kind: None,
            opponent_search: None,
            registered: [deck_v1(first), deck_v1(second)],
            selected: [deck_v1(first), deck_v1(second)],
            postboard: false,
            max_physical_decisions: 100_000,
            max_policy_steps: 1_000_000,
            runtime_rules: None,
            reset: None,
            opening: None,
            mulligan_modes: None,
        };
        let trajectory = collect_episode(&mut policy, &learner, None, &episode).unwrap();
        for row in &trajectory.decisions {
            let tensor = row.tensor.tensor();
            let view = encoded_decision_view_generic_v1(&tensor, generation);
            let (narrow_output, wide_output) = if v4 {
                (
                    model.forward_feature_transfer_v4(view).unwrap(),
                    wide.forward_feature_transfer_v4_wide_v1(view).unwrap(),
                )
            } else {
                (
                    model.forward_feature_transfer_v3(view).unwrap(),
                    wide.forward_feature_transfer_v3_wide_v1(view).unwrap(),
                )
            };
            // The checkpoint's net is the collecting net.
            assert_eq!(bits(&narrow_output.logits), row.logits);
            assert_eq!(narrow_output.value.to_bits(), row.value);
            assert_eq!(narrow_output.logits.len(), wide_output.logits.len());
            for (narrow_logit, wide_logit) in narrow_output.logits.iter().zip(&wide_output.logits) {
                assert!(narrow_logit == wide_logit, "step {}", row.step);
            }
            assert!(
                narrow_output.value == wide_output.value,
                "step {}",
                row.step
            );
            decisions += 1;
        }
        records.push(screen_record_v1(episode.id.clone(), &trajectory));
    }
    assert!(decisions > 0);
    eprintln!(
        "widened b512 matched b512 on {decisions} collected decisions; provenance {}",
        serde_json::to_string(&widened.provenance).unwrap()
    );
    // The screen fit at both widths starts from the same held-out losses.
    let config = ScreenFitConfigV1 {
        value_target_source: TERMINAL_RETURN_TARGET_SOURCE_V1.into(),
        steps: 1,
        batch_groups: 8,
        learning_rate: 1.0e-4,
        value_coefficient: 0.5,
        minibatch_seed: 1,
        heldout_every_steps: 1,
        forward_workers: 4,
    };
    let (fit, heldout) = records.split_at(1);
    let mut narrow_fit = narrow_state;
    let narrow_report =
        fit_screen_value_dims_v1(&mut narrow_fit, generation, fit, heldout, &config).unwrap();
    let mut wide_fit = widened.state;
    let wide_report =
        fit_screen_value_dims_v1(&mut wide_fit, generation, fit, heldout, &config).unwrap();
    assert_eq!(narrow_report.heldout[0], wide_report.heldout[0]);
}
