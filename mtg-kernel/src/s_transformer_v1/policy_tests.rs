//! Native S inside `FrozenPlayPolicyV1`: every V4 forward of the policy is
//! S's, with the same bits as the model's own forward.

use super::*;
use crate::rl_session::FastActorSessionV1;
use crate::sideboard_play_policy_v1::{FreshLineageGenerationV1, FrozenPlayPolicyV1};
use std::sync::Arc;

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

fn s_policy_v1(seed: u64) -> (FrozenPlayPolicyV1, Arc<SModelV1>) {
    let model = Arc::new(SModelV1::init_parity_v1(SConfigV1::s_v1(), seed).unwrap());
    let mut policy = FrozenPlayPolicyV1::training_fixture_v4();
    policy.install_s_model_v1(model.clone()).unwrap();
    policy.reset_sampling_v1([3, 4]);
    (policy, model)
}

fn bits(values: &[f32]) -> Vec<u32> {
    values.iter().map(|v| v.to_bits()).collect()
}

#[test]
fn collection_replay_and_identity_use_native_s() {
    let session = session_v1();
    let (mut policy, model) = s_policy_v1(1);
    let scores = policy.score_fast_session_v1(&session).unwrap();
    let tensor = policy.last_scored_training_tensor_v4().unwrap().clone();
    let direct = model
        .forward_v1(
            SInputV1::of_tensor_v1(&tensor.common),
            &mut STapeV1::default(),
        )
        .unwrap();
    assert_eq!(bits(&scores.logits), bits(&direct.logits));
    assert_eq!(scores.value.to_bits(), direct.value.to_bits());
    let replay = policy
        .score_replay_tensor_v1(FreshLineageGenerationV1::V4, tensor.common.clone(), &[])
        .unwrap();
    assert_eq!(bits(&replay.logits), bits(&direct.logits));
    assert_eq!(replay.value.to_bits(), direct.value.to_bits());
    assert_eq!(
        policy.actual_model_identity_v1().weights_sha256,
        model.parameter_sha256_v1()
    );
    let (selected, sampled, _) = policy.select_with_training_tensor_v4(&session).unwrap();
    assert!((selected as usize) < sampled.logits.len());
    assert_eq!(bits(&sampled.logits), bits(&direct.logits));
}

#[test]
fn forks_and_prediction_caches_keep_native_s_bits() {
    let session = session_v1();
    let (mut policy, _) = s_policy_v1(2);
    let plain = policy.score_fast_session_v1(&session).unwrap();
    let mut fork = policy.fork_for_collection_v3().unwrap();
    fork.reset_sampling_v1([3, 4]);
    assert_eq!(
        bits(&fork.score_fast_session_v1(&session).unwrap().logits),
        bits(&plain.logits)
    );
    let cache = crate::inference_cache_v1::InferenceCacheV1::shared(64 << 20);
    policy.attach_inference_cache_v1(&cache).unwrap();
    let miss = policy.score_fast_session_v1(&session).unwrap();
    let hit = policy.score_fast_session_v1(&session).unwrap();
    assert_eq!(bits(&miss.logits), bits(&plain.logits));
    assert_eq!(bits(&hit.logits), bits(&plain.logits));
    assert_eq!(hit.value.to_bits(), plain.value.to_bits());
}

#[test]
fn native_s_refuses_shells_and_net8_only_operations() {
    let model = Arc::new(SModelV1::init_pilot_scheme_v1(SConfigV1::s_v1(), 1).unwrap());
    let mut v3 = FrozenPlayPolicyV1::training_fixture_v3();
    assert!(v3.install_s_model_v1(model.clone()).is_err());
    let mut v4 = FrozenPlayPolicyV1::training_fixture_v4();
    let parameters = v4.training_parameters_v3();
    v4.install_s_model_v1(model.clone()).unwrap();
    assert!(v4.install_s_model_v1(model).is_err());
    assert!(
        crate::sideboard_play_policy_v1::search_leaf_v4::V4SearchLeafEvaluatorV1::new(&v4).is_err()
    );
    assert!(v4.replace_training_parameters_v3(&parameters).is_err());
    assert!(v4.into_flat_v7_transfer_v1().is_err());
}
