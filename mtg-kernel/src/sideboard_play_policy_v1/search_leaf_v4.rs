//! V4-only search leaf scoring with fresh local scratch. No playing route.
use super::*;
use crate::model_guided_search_core_v1::{ModelGuidedSearchLeafEvaluatorV1,ModelGuidedSearchLeafForwardV1,ModelGuidedSearchLeafSiteV1,ModelGuidedSearchCoreErrorV1};

pub(crate) struct V4SearchLeafEvaluatorV1<'a> { policy:&'a FrozenPlayPolicyV1 }
impl<'a> V4SearchLeafEvaluatorV1<'a> {
    pub(crate) fn new(policy:&'a FrozenPlayPolicyV1)->Result<Self,String> {
        require(policy.feature_generation_v1()==PlayPolicyGenerationV1::V4,"search leaf requires explicit V4 policy")?;
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
#[cfg(test)]
mod tests {
    use super::*;
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
