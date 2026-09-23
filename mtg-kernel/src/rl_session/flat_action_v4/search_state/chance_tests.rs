use super::*;
use crate::environment_randomization_v2::{GameEnvironmentRandomizationV2,PhysicalOwnerV2};

fn with_rng(state:&GameState,environment:bool,seed:u64)->GameState {
    let mut value=serde_json::to_value(state).unwrap();
    value.as_object_mut().unwrap().remove("rng");
    value.as_object_mut().unwrap().remove("environment_randomization_v2");
    if environment {
        let mut rng=GameEnvironmentRandomizationV2::new(seed);
        rng.set_live_shuffle_ordinal(PhysicalOwnerV2::P0,4);
        rng.set_live_shuffle_ordinal(PhysicalOwnerV2::P1,9);
        value["environment_randomization_v2"]=serde_json::to_value(rng).unwrap();
    } else {value["rng"]=serde_json::json!({"state":seed});}
    serde_json::from_value(value).unwrap()
}
fn decision(s:&FastActorSessionV1)->FastActorDecisionV1 {
    let FastActorResponseV1::Decision(d)=s.current_response() else{panic!("decision")};d
}

#[test]
fn v4_chance_v3_preserves_boundary_ignores_real_seed_and_diversifies_shuffles() {
    for actor in [PlayerId::P0,PlayerId::P1] {for environment in [false,true] {for form in 0..3 {
        let names=if form==1 {vec!["Squadron Hawk","Squadron Hawk","Forest","Mountain"]}else{vec!["Forest","Forest","Island","Mountain"]};
        let original=library_tests::fixture(actor,form,&names,false,true,false);
        let a=FastActorSessionV1::from_v3_fixture_state(with_rng(&original,environment,111));
        let b=FastActorSessionV1::from_v3_fixture_state(with_rng(&original,environment,999));
        let before=a.state.clone();let d=decision(&a);let known=a.state.players[actor.index()].library[0];
        assert_ne!(a.state,b.state);assert_eq!(a.kernel_search_visible_key_v4(8),b.kernel_search_visible_key_v4(8));
        let x=a.kernel_search_redeterminized_clone_mode_v4(81,V4SearchSampleMode::FutureChanceV3).unwrap();
        let y=b.kernel_search_redeterminized_clone_mode_v4(81,V4SearchSampleMode::FutureChanceV3).unwrap();
        assert_eq!(x.state,y.state);assert_eq!(x.current.as_ref().unwrap().candidates,y.current.as_ref().unwrap().candidates);
        assert_eq!(boundary(&a,d,V4SearchSampleMode::FutureChanceV3).unwrap(),boundary(&x,d,V4SearchSampleMode::FutureChanceV3).unwrap());
        assert_eq!(super::tests::tensor_and_output(&a),super::tests::tensor_and_output(&x));
        assert_eq!(a.kernel_search_action_token_v4(d),x.kernel_search_action_token_v4(d));
        assert_eq!(x.state.players[actor.index()].library[0],known);
        if environment {
            let rng=x.state.environment_randomization_v2().unwrap();
            assert_eq!(rng.next_live_shuffle_ordinal(PhysicalOwnerV2::P0),4);
            assert_eq!(rng.next_live_shuffle_ordinal(PhysicalOwnerV2::P1),9);
        } else {assert!(x.state.legacy_rng().is_some());}
        let positions:std::collections::BTreeSet<_>=(1..=32).map(|seed| {
            let mut sample=a.kernel_search_redeterminized_clone_mode_v4(seed,V4SearchSampleMode::FutureChanceV3).unwrap();
            sample.state.shuffle_library(actor).unwrap();
            assert!(sample.state.known_library_cards(actor,actor).is_empty());
            sample.state.players[actor.index()].library.iter().position(|id|*id==known).unwrap()
        }).collect();
        assert!(positions.len()>1,"future known-card slot remained fixed");
        assert_eq!(before,a.state);
        let old=a.kernel_search_redeterminized_clone_library_v2(81).unwrap();
        assert_eq!(a.state.legacy_rng(),old.state.legacy_rng());
        assert_eq!(a.state.environment_randomization_v2(),old.state.environment_randomization_v2());
    }}}
}

#[test]
fn v4_chance_v3_empty_partial_and_physical_step() {
    for actor in [PlayerId::P0,PlayerId::P1] {for (form,names,partial) in [
        (0,vec!["Lightning Bolt","Counterspell"],false),
        (1,vec!["Squadron Hawk","Squadron Hawk","Squadron Hawk","Forest"],true),
        (2,vec!["Forest","Mountain","Island"],false)] {
        let state=library_tests::fixture(actor,form,&names,false,false,partial);
        let s=FastActorSessionV1::from_v3_fixture_state(state);let before=s.state.clone();
        let sample=s.kernel_search_redeterminized_clone_mode_v4(81,V4SearchSampleMode::FutureChanceV3).unwrap();
        let d=decision(&sample);let token=sample.kernel_search_action_token_v4(d).unwrap();
        for i in 0..d.legal_action_count {
            let mut via=sample.clone();let mut direct=sample.clone();
            assert_eq!(via.kernel_search_consume_v4(d,token,i).unwrap(),direct.step(d.episode_id,d.step,i).unwrap());
            assert_eq!(via.state,direct.state);
        }
        assert_eq!(before,s.state);
    }}
}

#[test]
#[cfg(feature="experimental-burn-net8-packed-cuda-v1")]
fn v4_chance_v3_bound_report_invariant_to_real_seed_both_modes_and_seats() {
    use crate::sideboard_play_policy_v1::FrozenPlayPolicyV1;
    use crate::paired_bo1_harness_v1::PairedBo1PolicyInputV1;
    use crate::model_guided_search_core_v4::{Limits,Error as SearchError};
    let limits=Limits{simulations:16,transitions:64,depth:4,seed:29};
    for actor in [PlayerId::P0,PlayerId::P1] {for environment in [false,true] {
        let state=library_tests::fixture(actor,0,&["Forest","Forest","Island","Mountain"],false,true,false);
        let a=FastActorSessionV1::from_v3_fixture_state(with_rng(&state,environment,111));
        let b=FastActorSessionV1::from_v3_fixture_state(with_rng(&state,environment,999));
        let before=a.state.clone();let mut policy=FrozenPlayPolicyV1::training_fixture_v4();
        policy.score_fast_session_v1(&a).unwrap();
        let x=PairedBo1PolicyInputV1::new(&a,decision(&a)).report_search_future_v3(&policy,limits).unwrap();
        let y=PairedBo1PolicyInputV1::new(&b,decision(&b)).report_search_future_v3(&policy,limits).unwrap();
        assert_eq!(x,y);assert_eq!(before,a.state);
        let mut stale=decision(&a);stale.step+=1;
        assert_eq!(PairedBo1PolicyInputV1::new(&a,stale).report_search_future_v3(&policy,limits),Err(SearchError::InvalidAdapterBinding));
    }}
}
