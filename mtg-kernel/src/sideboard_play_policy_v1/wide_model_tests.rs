//! The wide net (`kernel-policy-value-net-8w128`) on the play policy
//! (collab LANES/w128-20261113, PR A): installed on a plain FlatV4 shell as
//! native S is, every V4 forward of the policy is the wide net's, its bits
//! equal the wide train step's tape forward, and net-8-only operations refuse.

use super::*;
use crate::native_policy_train_step_v1::widening_v1::{
    widen_net8_snapshot_v1, TORCH_DEFAULT_UNIFORM_V1,
};
use crate::native_policy_train_step_v1::{
    wide_tape_scores_for_test_v1, NativePolicyValueTrainStateV1,
};
use crate::native_policy_value_net_v1::W_MODEL_ARCHITECTURE_VERSION_V1;
use std::sync::Arc;

const SEED_V1: u64 = 0x5128_0000_0000_0001;

/// The fixture policy's net-8 widened by the function-preserving
/// construction.
fn widened_fixture_model_v1() -> Arc<NativePolicyValueNetWideV1> {
    let narrow = FrozenPlayPolicyV1::training_fixture_v4().training_model_v3();
    let snapshot = NativePolicyValueTrainStateV1::new_v1(narrow)
        .unwrap()
        .snapshot_v1()
        .unwrap();
    let widened = widen_net8_snapshot_v1(
        &snapshot,
        W_MODEL_ARCHITECTURE_VERSION_V1,
        TORCH_DEFAULT_UNIFORM_V1,
        SEED_V1,
    )
    .unwrap();
    Arc::new(widened.state.model_dims_v1().clone())
}

/// An independent wide net whose function is not net-8's.
fn runner_fixed_wide_model_v1() -> Arc<NativePolicyValueNetWideV1> {
    Arc::new(
        NativePolicyValueNetWideV1::runner_fixed_wide_v1(
            NativePolicyValueModelConfigV1::contract_wide_v1(),
        )
        .unwrap(),
    )
}

fn wide_policy_v1(model: Arc<NativePolicyValueNetWideV1>) -> FrozenPlayPolicyV1 {
    let mut policy = FrozenPlayPolicyV1::training_fixture_v4();
    policy.install_wide_model_v1(model).unwrap();
    policy
}

/// A real V4 decision: the fixture policy selects on an Avenging Hunter
/// targets state, as the V4 replay test does.
fn session_v1() -> FastActorSessionV1 {
    let (mut state, hunter, _goaded, _ordinary) =
        crate::rl_session::avenging_hunter_undercity_arena_choose_targets_state_v1(false);
    crate::rl_session::shuffle_trigger_source_into_library_v1(
        &mut state,
        hunter,
        crate::ids::PlayerId::P0,
    );
    FastActorSessionV1::from_v3_fixture_state(state)
}

fn assert_bits_v1(label: &str, a: &FrozenPlayDecisionScoresV1, logits: &[f32], value: f32) {
    assert_eq!(a.logits.len(), logits.len(), "{label}");
    for (x, y) in a.logits.iter().zip(logits) {
        assert_eq!(x.to_bits(), y.to_bits(), "{label}");
    }
    assert_eq!(a.value.to_bits(), value.to_bits(), "{label}");
}

#[test]
fn wide_policy_collects_and_replays_with_the_wide_tape_bits() {
    let model = runner_fixed_wide_model_v1();
    let mut policy = wide_policy_v1(Arc::clone(&model));
    policy.reset_sampling_v1([55, 66]);
    let (selected, scores, tensor) = policy
        .select_with_training_tensor_v4(&session_v1())
        .unwrap();
    assert!((selected as usize) < scores.logits.len());
    let view = encoded_decision_view_v4(&tensor);
    let (logits, value) =
        wide_tape_scores_for_test_v1(&model, model.feature_transfer_config_v4_wide_v1(), view);
    assert_bits_v1("collection against the tape", &scores, &logits, value);
    let replayed = policy.score_training_tensor_v4(&tensor).unwrap();
    assert_bits_v1("replay against the tape", &replayed, &logits, value);
    let diagnostic = policy.diagnostic_forward_v4(&tensor).unwrap();
    assert_bits_v1("diagnostic against the tape", &diagnostic, &logits, value);
    // The wide net, not net-8, scored it.
    let narrow = FrozenPlayPolicyV1::training_fixture_v4();
    let narrow_scores = narrow.score_training_tensor_v4(&tensor).unwrap();
    assert_ne!(narrow_scores.logits, scores.logits);
}

#[test]
fn widened_policy_plays_exactly_as_its_net8_parent() {
    let mut narrow = FrozenPlayPolicyV1::training_fixture_v4();
    let mut wide = wide_policy_v1(widened_fixture_model_v1());
    narrow.reset_sampling_v1([7, 8]);
    wide.reset_sampling_v1([7, 8]);
    let (narrow_selected, narrow_scores, tensor) = narrow
        .select_with_training_tensor_v4(&session_v1())
        .unwrap();
    let (wide_selected, wide_scores, wide_tensor) =
        wide.select_with_training_tensor_v4(&session_v1()).unwrap();
    assert_eq!(tensor, wide_tensor);
    assert_eq!(narrow_selected, wide_selected);
    // IEEE `==`: the construction may turn a -0 into +0, nothing else.
    assert_eq!(narrow_scores.logits.len(), wide_scores.logits.len());
    for (x, y) in narrow_scores.logits.iter().zip(&wide_scores.logits) {
        assert!(x == y);
    }
    assert!(narrow_scores.value == wide_scores.value);
}

#[test]
fn wide_policy_identity_and_forks_carry_the_wide_net() {
    let model = runner_fixed_wide_model_v1();
    let wide = wide_policy_v1(Arc::clone(&model));
    let narrow = FrozenPlayPolicyV1::training_fixture_v4();
    let identity = wide.actual_model_identity_v1();
    assert_ne!(
        identity.weights_sha256,
        narrow.actual_model_identity_v1().weights_sha256
    );
    assert_eq!(
        identity.model_parameter_sha256,
        model.parameter_manifest_sha256_wide_v1()
    );
    assert_eq!(identity, wide_policy_v1(model).actual_model_identity_v1());
    let mut fork = wide.fork_for_collection_v3().unwrap();
    assert!(fork.wide_model_v1().is_some());
    fork.reset_sampling_v1([1, 2]);
    let mut original = wide;
    original.reset_sampling_v1([1, 2]);
    let (a, a_scores, _) = fork.select_with_training_tensor_v4(&session_v1()).unwrap();
    let (b, b_scores, _) = original
        .select_with_training_tensor_v4(&session_v1())
        .unwrap();
    assert_eq!(a, b);
    assert_bits_v1("fork", &a_scores, &b_scores.logits, b_scores.value);
}

#[test]
fn wide_install_and_net8_operations_refuse() {
    // Twice.
    let mut policy = wide_policy_v1(runner_fixed_wide_model_v1());
    assert!(policy
        .install_wide_model_v1(runner_fixed_wide_model_v1())
        .is_err());
    // Net-8's operations.
    let parameters = FrozenPlayPolicyV1::training_fixture_v4().training_parameters_v3();
    assert!(policy.replace_training_parameters_v3(&parameters).is_err());
    let policy = wide_policy_v1(runner_fixed_wide_model_v1());
    assert!(policy.into_flat_v7_transfer_v1().is_err());
    let policy = wide_policy_v1(runner_fixed_wide_model_v1());
    assert!(policy.into_flat_v8_v1().is_err());
    // A V3 shell has no FlatV4 scorer.
    let mut v3 = FrozenPlayPolicyV1::training_fixture_v3();
    assert!(v3
        .install_wide_model_v1(runner_fixed_wide_model_v1())
        .is_err());
    // Portable math and the search-only fast forward.
    let mut portable = FrozenPlayPolicyV1::training_fixture_v4();
    portable.set_training_math_v1(TrainingMathV1::PortableV1);
    assert!(portable
        .install_wide_model_v1(runner_fixed_wide_model_v1())
        .is_err());
    let mut fast = FrozenPlayPolicyV1::training_fixture_v4();
    fast.enable_fast_search_forward_v1();
    assert!(fast
        .install_wide_model_v1(runner_fixed_wide_model_v1())
        .is_err());
    // Set after install, the wide forward refuses at scoring time.
    let mut later = wide_policy_v1(runner_fixed_wide_model_v1());
    later.set_training_math_v1(TrainingMathV1::PortableV1);
    later.reset_sampling_v1([3, 4]);
    assert!(later.select_with_training_tensor_v4(&session_v1()).is_err());
}
