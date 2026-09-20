//! Explicit public-feature policy adapter. No legacy loader constructs this.
//! The inherited generation describes the base actor tensor, not model identity.
use super::*;
use crate::native_policy_value_net_v1::public_inputs_v1::{NativePublicInputNetV1, PublicInputWeightsV1};

pub(crate) struct PublicInputPlayPolicyV1 {
    base: FrozenPlayPolicyV1,
    model: NativePublicInputNetV1,
}

impl PublicInputPlayPolicyV1 {
    pub(crate) fn new(base: FrozenPlayPolicyV1, weights: PublicInputWeightsV1) -> Result<Self,String> {
        require(base.fresh_successor.is_some() && base.successor.is_none(),"public inputs require the exact V4 actor encoder")?;
        let model = NativePublicInputNetV1::new(base.model.clone(),weights).map_err(|e|e.to_string())?;
        Ok(Self { base, model })
    }

    pub(crate) fn select_with_scores(
        &mut self, input: &PairedBo1PolicyInputV1<'_>,
    ) -> Result<(u32,FrozenPlayDecisionScoresV1), RlSessionError> {
        let decision = input.decision();
        // Both representations come from this same bound decision. No live
        // session, registered opponent deck or hidden state enters the scorer.
        let (observation, actions) = input.diagnostic_visible_v1().map_err(policy_error)?;
        let fresh = self.base.fresh_successor.as_mut().ok_or_else(||policy_error("missing V4 encoder".into()))?;
        let encoded = input.encode_scoring_owned_v4(&mut fresh.encoder,&mut self.base.owned.buffers())
            .map_err(|e|policy_error(format!("public-input V4 encoding: {e:?}")))?;
        self.base.owned.globals = encoded.globals;
        fresh.extensions = encoded.extensions;
        fresh.tensorizer.fill(FlatScoringDecisionViewV4::new(self.base.owned.view(),&fresh.extensions),&mut fresh.tensor)
            .map_err(|e|policy_error(format!("public-input V4 tensorization: {e:?}")))?;
        let output = self.model.forward(encoded_decision_view_v4(&fresh.tensor),&observation)
            .map_err(|e|policy_error(e.to_string()))?;
        if output.logits.len() != actions.len() || actions.len() != decision.legal_action_count as usize {
            return Err(policy_error("public-input scorer/menu count mismatch".into()));
        }
        let selected = self.base.sample_scores(&output.logits,decision.acting_player,decision.legal_action_count)
            .map_err(policy_error)?;
        Ok((selected,FrozenPlayDecisionScoresV1 { logits:output.logits,value:output.value }))
    }
}

impl PairedBo1PolicyV1 for PublicInputPlayPolicyV1 {
    fn uses_observation_successor_v3(&self) -> bool { true }
    fn feature_generation_v1(&self) -> PlayPolicyGenerationV1 { PlayPolicyGenerationV1::V4 }
    fn reset_for_game_v1(&mut self,seeds:[u64;2]) -> Result<(),RlSessionError> {
        self.base.reset_for_game_v1(seeds)
    }
    fn select_action_v1(&mut self,input:PairedBo1PolicyInputV1<'_>) -> Result<u32,RlSessionError> {
        self.select_with_scores(&input).map(|(selected,_)|selected)
    }
}
