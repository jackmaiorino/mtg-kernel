//! V4-only search leaf scoring with fresh local scratch. No playing route.
use super::*;
use crate::model_guided_search_core_v1::{ModelGuidedSearchLeafEvaluatorV1,ModelGuidedSearchLeafForwardV1,ModelGuidedSearchLeafSiteV1,ModelGuidedSearchCoreErrorV1};

pub(crate) struct V4SearchLeafEvaluatorV1<'a> { policy:&'a FrozenPlayPolicyV1 }
impl<'a> V4SearchLeafEvaluatorV1<'a> {
    pub(crate) fn new(policy:&'a FrozenPlayPolicyV1)->Result<Self,String> {
        require(policy.feature_generation_v1()==PlayPolicyGenerationV1::V4,"search leaf requires explicit V4 policy")?;
        crate::deterministic_math_v1::ensure_thread_mxcsr_normalized_v1()
            .map_err(|e|format!("V4 search leaf floating-point state: {e:?}"))?;
        crate::deterministic_math_v1::verify_pinned_mxcsr_state_v1()
            .map_err(|e|format!("V4 search leaf floating-point verification: {e:?}"))?;
        Ok(Self{policy})
    }
    fn tensor(&self,session:&FastActorSessionV1,legal_count:u32)->Result<NativeFlatDecisionTensorV4,String> {
        let FastActorResponseV1::Decision(decision)=session.current_response() else {return Err("search leaf requires live decision".into());};
        require(decision.legal_action_count==legal_count,"search leaf legal count differs")?;
        let mut encoder=FlatDecisionEncoderV4::default();
        let mut owned=OwnedScoringV1::default();
        let encoded=session.encode_current_flat_scoring_decision_owned_v4(decision,&mut encoder,&mut owned.buffers())
            .map_err(|e|format!("V4 search leaf encoding: {e:?}"))?;
        owned.globals=encoded.globals;
        let mut tensor=NativeFlatDecisionTensorV4::default();
        NativeFlatTensorizerV4::default().fill(FlatScoringDecisionViewV4::new(owned.view(),&encoded.extensions),&mut tensor)
            .map_err(|e|format!("V4 search leaf tensorization: {e:?}"))?;
        Ok(tensor)
    }
}
impl ModelGuidedSearchLeafEvaluatorV1 for V4SearchLeafEvaluatorV1<'_> {
    fn evaluate_leaf_v1(&self,session:&FastActorSessionV1,_leaf_key:[u8;32],legal_action_count:u32,
        _site:ModelGuidedSearchLeafSiteV1)->Result<ModelGuidedSearchLeafForwardV1,ModelGuidedSearchCoreErrorV1> {
        if !matches!(session.current_response(),FastActorResponseV1::Decision(_)) {
            return Err(ModelGuidedSearchCoreErrorV1::NoLiveDecisionToEncode);
        }
        let tensor=self.tensor(session,legal_action_count).map_err(ModelGuidedSearchCoreErrorV1::Tensorize)?;
        let output=self.policy.model.forward_search_feature_transfer_v4(encoded_decision_view_v4(&tensor))?;
        if output.logits.len()!=legal_action_count as usize || output.logits.is_empty()
            || !output.value.is_finite() || output.logits.iter().any(|x|!x.is_finite()) {
            return Err(ModelGuidedSearchCoreErrorV1::EvaluatorContract);
        }
        Ok(ModelGuidedSearchLeafForwardV1{
            legal_action_weights:crate::deterministic_math_v1::softmax_legal_action_weights_v1(&output.logits),
            v_raw:output.value,
        })
    }
}
/// Opt-in coordinator report on a sampled, live root. Does not select an action.
#[cfg(feature="experimental-burn-net8-packed-cuda-v1")]
pub(crate) fn diagnostic_report(policy:&FrozenPlayPolicyV1,session:&FastActorSessionV1,
    scores:&FrozenPlayDecisionScoresV1)->Result<serde_json::Value,String> {
    use serde_json::json;
    // An observer must not repair the collector's arithmetic environment.
    // Search-owned threads may normalize in the constructor; this hook may not.
    crate::deterministic_math_v1::verify_pinned_mxcsr_state_v1()
        .map_err(|e|format!("leaf diagnostic requires already pinned floating-point state: {e:?}"))?;
    let FastActorResponseV1::Decision(d)=session.current_response() else {return Err("diagnostic requires live root".into())};
    let before=session.diagnostic_state_hash();let rng=policy.seat_rng;
    let retained=policy.last_scored_training_tensor_v4()?.clone();
    let e=V4SearchLeafEvaluatorV1::new(policy)?;
    let tensor=e.tensor(session,d.legal_action_count)?;
    let capture=|t:&NativeFlatDecisionTensorV4|crate::phase1_bo3_learning_v1::Bo3CapturedTensorBitsV1::from_tensor(&NativeFlatDecisionTensorV3{common:t.common.clone()});
    require(capture(&tensor)==capture(&retained),"diagnostic tensor differs from ordinary scoring")?;
    let ordinary=policy.model.forward_feature_transfer_v4(encoded_decision_view_v4(&tensor)).map_err(|e|format!("{e:?}"))?;
    let raw=policy.model.forward_search_feature_transfer_v4(encoded_decision_view_v4(&tensor)).map_err(|e|format!("{e:?}"))?;
    let logits_bits=|v:&[f32]|v.iter().map(|x|x.to_bits()).collect::<Vec<_>>();
    require(logits_bits(&ordinary.logits)==logits_bits(&scores.logits) && ordinary.value.to_bits()==scores.value.to_bits(),"diagnostic ordinary output differs")?;
    require(raw.value.is_finite() && (-1.0..=1.0).contains(&raw.value)
        && raw.logits.len()==ordinary.logits.len() && !raw.logits.is_empty()
        && raw.logits.iter().all(|x|x.is_finite()),"diagnostic raw search output invalid")?;
    let mut variants=Vec::new();
    for (name,library,rng_change) in [("own_library",Some(d.acting_player as usize),false),
        ("opponent_library",Some(1-d.acting_player as usize),false),("randomness",None,true)] {
        let changed=session.diagnostic_certificate_perturbed_clone_v1(library,rng_change)?;
        let t=e.tensor(&changed,d.legal_action_count)?;
        require(capture(&t)==capture(&tensor),"hidden perturbation changed leaf tensor")?;
        let out=policy.model.forward_search_feature_transfer_v4(encoded_decision_view_v4(&t)).map_err(|e|format!("{e:?}"))?;
        require(logits_bits(&out.logits)==logits_bits(&raw.logits) && out.value.to_bits()==raw.value.to_bits(),"hidden perturbation changed leaf output")?;
        variants.push(name);
    }
    let argmax=|xs:&[f32]|xs.iter().enumerate().fold(0,|best,(i,x)|if *x>xs[best]{i}else{best});
    require(before==session.diagnostic_state_hash(),"diagnostic changed original session")?;
    require(rng==policy.seat_rng && capture(&retained)==capture(policy.last_scored_training_tensor_v4()?),"diagnostic changed policy scratch")?;
    Ok(json!({"schema":"v4-search-leaf-diagnostic/v1","tensor_bits":capture(&tensor),
        "ordinary_logits_bits":logits_bits(&ordinary.logits),"ordinary_value_bits":ordinary.value.to_bits(),
        "search_logits_bits":logits_bits(&raw.logits),"search_value_bits":raw.value.to_bits(),
        "max_logit_absolute_delta":raw.logits.iter().zip(&ordinary.logits).map(|(a,b)|(f64::from(*a)-f64::from(*b)).abs()).fold(0.0_f64,f64::max),
        "value_absolute_delta":(f64::from(raw.value)-f64::from(ordinary.value)).abs(),
        "argmax_agreement":argmax(&raw.logits)==argmax(&ordinary.logits),"value_domain":[-1,1],
        "invariant_variants":variants,"original_unchanged":true,"policy_unchanged":true}))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[cfg(all(target_arch="x86_64",feature="experimental-burn-net8-packed-cuda-v1"))]
    fn v4_search_leaf_diagnostic_rejects_dirty_thread_without_repair() {
        std::thread::spawn(|| {
            use crate::deterministic_math_v1::*;
            let s=public_session(crate::ids::PlayerId::P0,20);
            let mut policy=FrozenPlayPolicyV1::training_fixture_v4();
            let scores=policy.score_fast_session_v1(&s).unwrap();
            let original=read_mxcsr_v1();let dirty=original | (1<<15);
            write_mxcsr_v1(dirty);
            let result=diagnostic_report(&policy,&s,&scores);
            let after=read_mxcsr_v1();write_mxcsr_v1(original);
            assert!(result.is_err());assert_eq!(after,dirty);
        }).join().unwrap();
    }
    fn public_session(actor:crate::ids::PlayerId,life:i32)->FastActorSessionV1 {
        use crate::policy_observation_v6::tests::{put,ready_state};
        use crate::state::Zone;
        let mut state=ready_state();state.active_player=actor;state.priority_player=actor;
        state.players[actor.index()].life=life;
        put(&mut state,actor,"Lightning Bolt",Zone::Hand);
        state.players[actor.index()].mana_pool[crate::mana::ManaColor::R.pool_index()]=3;
        put(&mut state,actor.opponent(),"Gut Shot",Zone::Hand);
        for owner in [actor,actor.opponent()] {
            for name in ["Forest","Mountain","Island"] {put(&mut state,owner,name,Zone::Library);}
        }
        FastActorSessionV1::from_v3_fixture_state(state)
    }
    #[test]
    #[cfg(target_arch="x86_64")]
    fn v4_search_leaf_constructor_normalizes_then_rejects_dirty_thread() {
        std::thread::spawn(|| {
            use crate::deterministic_math_v1::*;
            let original=read_mxcsr_v1();
            let policy=FrozenPlayPolicyV1::training_fixture_v4();
            write_mxcsr_v1(original | (1<<15) | (1<<6));
            assert!(V4SearchLeafEvaluatorV1::new(&policy).is_ok());
            assert!(verify_pinned_mxcsr_state_v1().is_ok());
            write_mxcsr_v1(read_mxcsr_v1() | (1<<15));
            let rejected=V4SearchLeafEvaluatorV1::new(&policy).is_err();
            write_mxcsr_v1(original);
            assert!(rejected);
        }).join().unwrap();
    }
    #[test]
    fn v4_search_leaf_terminal_has_named_error() {
        let mut state=crate::policy_observation_v6::tests::ready_state();
        state.players[0].has_lost=true;
        let session=FastActorSessionV1::from_v3_fixture_state(state);
        assert!(matches!(session.current_response(),FastActorResponseV1::Terminal(_)));
        let policy=FrozenPlayPolicyV1::training_fixture_v4();
        assert!(matches!(V4SearchLeafEvaluatorV1::new(&policy).unwrap().evaluate_leaf_v1(
            &session,[0;32],1,ModelGuidedSearchLeafSiteV1::RootPrior),
            Err(ModelGuidedSearchCoreErrorV1::NoLiveDecisionToEncode)));
    }
    #[test]
    fn v4_search_leaf_rejects_cross_schema_forward_both_directions() {
        let policy=FrozenPlayPolicyV1::training_fixture_v4();
        let e=V4SearchLeafEvaluatorV1::new(&policy).unwrap();
        let s=session();let FastActorResponseV1::Decision(d)=s.current_response() else {panic!("fixture")};
        let v4=e.tensor(&s,d.legal_action_count).unwrap();
        assert!(policy.model.forward_search_deterministic_v1(encoded_decision_view_v4(&v4)).is_err());
        assert!(policy.model.forward_search_feature_transfer_v4(encoded_decision_view_v1(&v4.common)).is_err());
    }
    #[test]
    #[cfg(feature="experimental-burn-net8-packed-cuda-v1")]
    fn v4_search_leaf_hidden_invariance_both_seats_and_visible_positive_control() {
        for actor in [crate::ids::PlayerId::P0,crate::ids::PlayerId::P1] {
            let session=public_session(actor,20);let before=session.diagnostic_state_hash();
            let FastActorResponseV1::Decision(d)=session.current_response() else {panic!("fixture")};
            assert_eq!(d.acting_player as usize,actor.index());
            let mut policy=FrozenPlayPolicyV1::training_fixture_v4();
            policy.score_fast_session_v1(&session).unwrap();
            let ordinary=policy.last_scored_training_tensor_v4().unwrap().clone();
            let e=V4SearchLeafEvaluatorV1::new(&policy).unwrap();
            let tensor=e.tensor(&session,d.legal_action_count).unwrap();assert_eq!(tensor_bits(&tensor),tensor_bits(&ordinary));
            let forward=|s:&FastActorSessionV1|bits(e.evaluate_leaf_v1(s,[0;32],d.legal_action_count,ModelGuidedSearchLeafSiteV1::RootPrior).unwrap());
            let output=forward(&session);
            let mut variants=Vec::new();
            for (library,rng) in [(Some(0),false),(Some(1),false),(None,true)] {
                variants.push(session.diagnostic_certificate_perturbed_clone_v1(library,rng).unwrap());
            }
            // Existing redetermination admits only the old V2 action contract.
            // Preserve its rejection; a V4-compatible route needs separate design.
            assert!(matches!(session.kernel_search_redeterminized_clone_v1(71823),
                Err(crate::kernel_native_search_opponent_v1::KernelNativeSearchErrorV1::UnsupportedFlatActionContract)));
            for changed in &variants {
                assert_eq!(tensor_bits(&e.tensor(changed,d.legal_action_count).unwrap()),tensor_bits(&tensor));
                assert_eq!(forward(changed),output);
            }
            let visible=public_session(actor,19);
            assert_ne!(tensor_bits(&e.tensor(&visible,d.legal_action_count).unwrap()),tensor_bits(&tensor));
            assert_eq!(session.diagnostic_state_hash(),before);
        }
    }
    fn tensor_bits(t:&NativeFlatDecisionTensorV4)->crate::phase1_bo3_learning_v1::Bo3CapturedTensorBitsV1 {
        crate::phase1_bo3_learning_v1::Bo3CapturedTensorBitsV1::from_tensor(&NativeFlatDecisionTensorV3{common:t.common.clone()})
    }
    #[test]
    fn v4_search_leaf_extension_roots_match_warmed_and_fresh_tensors() {
        use crate::policy_observation_v6::tests::*;
        let mut ward=ward_multi_targeter_state().0;
        for player in [crate::ids::PlayerId::P0,crate::ids::PlayerId::P1] {
            put(&mut ward,player,"Lightning Bolt",crate::state::Zone::Hand);
            ward.players[player.index()].mana_pool[crate::mana::ManaColor::R.pool_index()]+=1;
        }
        let mut payment=ward.clone();reach_ward_payment(&mut payment);
        let cases=[("cost",escape_prefix_state().0),("library",forest_search_state(false,"Lightning Bolt")),
            ("queued_ward",ward),("ward",payment),
            ("historical",initiative_transfer_state()),
            ("chosen",crate::native_flat_tensorizer_v3::monstrous_emergence_cost_fixture_v3(true).0)];
        let mut warmed=FrozenPlayPolicyV1::training_fixture_v4();
        for (name,state) in cases {
            let s=FastActorSessionV1::from_v3_fixture_state(state);
            let FastActorResponseV1::Decision(d)=s.current_response() else {panic!("{name} fixture missing")};
            let mut owned=OwnedScoringV1::default();
            let encoded=s.encode_current_flat_scoring_decision_owned_v4(d,&mut FlatDecisionEncoderV4::default(),&mut owned.buffers()).unwrap();
            let ext=&encoded.extensions;
            match name {
                "cost"=>assert!(ext.pending_cast_object_cost.is_some()),
                "library"=>assert!(ext.decision_local_library.is_some()),
                "queued_ward"=>assert!(!ext.queued_ward_payments.is_empty()),
                "ward"=>assert!(ext.pending_ward_payment.is_some()),
                "historical"=>assert!(!ext.historical_public_sources.is_empty()),
                "chosen"=>assert!(ext.pending_chosen_creature_cost.is_some()),
                _=>unreachable!(),
            }
            warmed.score_fast_session_v1(&s).unwrap();
            let mut fresh=FrozenPlayPolicyV1::training_fixture_v4();fresh.score_fast_session_v1(&s).unwrap();
            let actual=V4SearchLeafEvaluatorV1::new(&warmed).unwrap().tensor(&s,d.legal_action_count).unwrap();
            assert_eq!(tensor_bits(&actual),tensor_bits(warmed.last_scored_training_tensor_v4().unwrap()),"{name}");
            assert_eq!(tensor_bits(&actual),tensor_bits(fresh.last_scored_training_tensor_v4().unwrap()),"{name}");
        }
    }
    fn session()->FastActorSessionV1 {
        let (mut state,hunter,_,_)=crate::rl_session::avenging_hunter_undercity_arena_choose_targets_state_v1(false);
        crate::rl_session::shuffle_trigger_source_into_library_v1(&mut state,hunter,crate::ids::PlayerId::P0);
        FastActorSessionV1::from_v3_fixture_state(state)
    }
    fn bits(x:ModelGuidedSearchLeafForwardV1)->(Vec<u32>,u32) {
        (x.legal_action_weights.iter().map(|x|x.to_bits()).collect(),x.v_raw.to_bits())
    }
    #[test]
    fn v4_search_leaf_matches_current_tensor_and_preserves_policy_state() {
        let session=session();let before=session.diagnostic_state_hash();
        let FastActorResponseV1::Decision(d)=session.current_response() else {panic!("fixture missing decision")};
        let mut policy=FrozenPlayPolicyV1::training_fixture_v4();policy.reset_sampling_v1([19,37]);
        policy.score_fast_session_v1(&session).unwrap();
        let expected=policy.last_scored_training_tensor_v4().unwrap().clone();let rng=policy.seat_rng;
        let evaluator=V4SearchLeafEvaluatorV1::new(&policy).unwrap();
        let observed=evaluator.tensor(&session,d.legal_action_count).unwrap();
        let captured=|t:&NativeFlatDecisionTensorV4|crate::phase1_bo3_learning_v1::Bo3CapturedTensorBitsV1::from_tensor(
            &NativeFlatDecisionTensorV3{common:t.common.clone()});
        assert_eq!(captured(&expected),captured(&observed));
        let first=bits(evaluator.evaluate_leaf_v1(&session,[0;32],d.legal_action_count,ModelGuidedSearchLeafSiteV1::RootPrior).unwrap());
        let repeated=bits(evaluator.evaluate_leaf_v1(&session,[0;32],d.legal_action_count,ModelGuidedSearchLeafSiteV1::RootPrior).unwrap());
        assert_eq!(first,repeated);assert_eq!(policy.seat_rng,rng);assert_eq!(session.diagnostic_state_hash(),before);
        assert_eq!(expected,*policy.last_scored_training_tensor_v4().unwrap());
        assert!(evaluator.tensor(&session,d.legal_action_count+1).is_err());
    }
    #[test]
    fn v4_search_leaf_rejects_v3_and_does_not_depend_on_policy_scratch() {
        let v3=FrozenPlayPolicyV1::training_fixture_v3();assert!(V4SearchLeafEvaluatorV1::new(&v3).is_err());
        let session=session();let FastActorResponseV1::Decision(d)=session.current_response() else {panic!("fixture missing")};
        let mut policy=FrozenPlayPolicyV1::training_fixture_v4();policy.reset_sampling_v1([11,22]);
        let first=bits(V4SearchLeafEvaluatorV1::new(&policy).unwrap().evaluate_leaf_v1(&session,[1;32],d.legal_action_count,ModelGuidedSearchLeafSiteV1::RootPrior).unwrap());
        let mut other=session.clone();other.step(d.episode_id,d.step,0).unwrap();
        if matches!(other.current_response(),FastActorResponseV1::Decision(_)) {policy.score_fast_session_v1(&other).unwrap();}
        let second=bits(V4SearchLeafEvaluatorV1::new(&policy).unwrap().evaluate_leaf_v1(&session,[1;32],d.legal_action_count,ModelGuidedSearchLeafSiteV1::RootPrior).unwrap());
        assert_eq!(first,second);
    }
}
