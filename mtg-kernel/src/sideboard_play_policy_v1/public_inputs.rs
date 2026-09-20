//! Explicit public-feature policy adapter. No legacy loader constructs this.
//! The inherited generation describes the base actor tensor, not model identity.
use super::*;
use crate::native_policy_value_net_v1::public_inputs_v1::{
    NativePublicInputNetV1, PublicInputWeightsV1,
};

/// Evaluation-only forced V3 action. Matches the qualified unscored adapter:
/// the singleton still consumes the physical seat's usual sampler draw.
pub(crate) fn select_forced_v3_for_evaluation(
    policy: &mut FrozenPlayPolicyV1,
    input: &PairedBo1PolicyInputV1<'_>,
) -> Result<u32, RlSessionError> {
    let decision = input.decision();
    if policy.feature_generation_v1() != PlayPolicyGenerationV1::V3
        || decision.legal_action_count != 1
    {
        return Err(policy_error(
            "forced evaluation requires V3 singleton".into(),
        ));
    }
    policy
        .sample_scores(&[0.0], decision.acting_player, 1)
        .map_err(policy_error)
}

pub(crate) struct PublicInputPlayPolicyV1 {
    base: FrozenPlayPolicyV1,
    model: NativePublicInputNetV1,
    inputs_enabled: bool,
    auxiliary: Option<crate::public_cost_features_v1::PublicFeatureRowsV1>,
}

impl PublicInputPlayPolicyV1 {
    pub(crate) fn new(
        base: FrozenPlayPolicyV1,
        weights: PublicInputWeightsV1,
    ) -> Result<Self, String> {
        require(
            base.fresh_successor.is_some() && base.successor.is_none(),
            "public inputs require the exact V4 actor encoder",
        )?;
        let model =
            NativePublicInputNetV1::new(base.model.clone(), weights).map_err(|e| e.to_string())?;
        Ok(Self {
            base,
            model,
            inputs_enabled: true,
            auxiliary: None,
        })
    }

    pub(crate) fn with_inputs_enabled(mut self, enabled: bool) -> Self {
        self.inputs_enabled = enabled;
        self.model = self.model.with_inputs_enabled(enabled);
        self
    }

    pub(crate) fn install(
        &mut self,
        parameters: &[NativeNamedParameterV1],
        weights: PublicInputWeightsV1,
    ) -> Result<(), String> {
        let mut candidate = self.base.model.clone();
        candidate
            .replace_parameter_snapshot_v1(parameters)
            .map_err(|e| e.to_string())?;
        let model = NativePublicInputNetV1::new(candidate, weights)
            .map_err(|e| e.to_string())?
            .with_inputs_enabled(self.inputs_enabled);
        self.base.replace_training_parameters_v3(parameters)?;
        self.model = model;
        self.auxiliary = None;
        Ok(())
    }

    pub(crate) fn captured(
        &self,
    ) -> Result<
        (
            &NativeFlatDecisionTensorV4,
            &crate::public_cost_features_v1::PublicFeatureRowsV1,
        ),
        String,
    > {
        Ok((
            self.base.last_scored_training_tensor_v4()?,
            self.auxiliary
                .as_ref()
                .ok_or("no captured public decision")?,
        ))
    }

    pub(crate) fn replay(
        &self,
        tensor: &NativeFlatDecisionTensorV4,
        rows: &crate::public_cost_features_v1::PublicFeatureRowsV1,
    ) -> Result<FrozenPlayDecisionScoresV1, String> {
        let output = self
            .model
            .forward_rows(encoded_decision_view_v4(tensor), rows)
            .map_err(|e| e.to_string())?;
        Ok(FrozenPlayDecisionScoresV1 {
            logits: output.logits,
            value: output.value,
        })
    }

    pub(crate) fn select_with_scores(
        &mut self,
        input: &PairedBo1PolicyInputV1<'_>,
    ) -> Result<(u32, FrozenPlayDecisionScoresV1), RlSessionError> {
        let decision = input.decision();
        // Both representations come from this same bound decision. No live
        // session, registered opponent deck or hidden state enters the scorer.
        let (observation, actions) = input.diagnostic_visible_v1().map_err(policy_error)?;
        let fresh = self
            .base
            .fresh_successor
            .as_mut()
            .ok_or_else(|| policy_error("missing V4 encoder".into()))?;
        let encoded = input
            .encode_scoring_owned_v4(&mut fresh.encoder, &mut self.base.owned.buffers())
            .map_err(|e| policy_error(format!("public-input V4 encoding: {e:?}")))?;
        self.base.owned.globals = encoded.globals;
        fresh.extensions = encoded.extensions;
        fresh
            .tensorizer
            .fill(
                FlatScoringDecisionViewV4::new(self.base.owned.view(), &fresh.extensions),
                &mut fresh.tensor,
            )
            .map_err(|e| policy_error(format!("public-input V4 tensorization: {e:?}")))?;
        let output = self
            .model
            .forward(encoded_decision_view_v4(&fresh.tensor), &observation)
            .map_err(|e| policy_error(e.to_string()))?;
        self.auxiliary = Some(
            crate::public_cost_features_v1::from_actor_v4_v1(
                &observation,
                &fresh.tensor.common.object_card_ids,
            )
            .map_err(policy_error)?,
        );
        if output.logits.len() != actions.len()
            || actions.len() != decision.legal_action_count as usize
        {
            return Err(policy_error(
                "public-input scorer/menu count mismatch".into(),
            ));
        }
        let selected = self
            .base
            .sample_scores(
                &output.logits,
                decision.acting_player,
                decision.legal_action_count,
            )
            .map_err(policy_error)?;
        Ok((
            selected,
            FrozenPlayDecisionScoresV1 {
                logits: output.logits,
                value: output.value,
            },
        ))
    }
}

impl PairedBo1PolicyV1 for PublicInputPlayPolicyV1 {
    fn uses_observation_successor_v3(&self) -> bool {
        true
    }
    fn feature_generation_v1(&self) -> PlayPolicyGenerationV1 {
        PlayPolicyGenerationV1::V4
    }
    fn reset_for_game_v1(&mut self, seeds: [u64; 2]) -> Result<(), RlSessionError> {
        self.auxiliary = None;
        self.base.reset_for_game_v1(seeds)
    }
    fn select_action_v1(
        &mut self,
        input: PairedBo1PolicyInputV1<'_>,
    ) -> Result<u32, RlSessionError> {
        self.select_with_scores(&input)
            .map(|(selected, _)| selected)
    }
}

#[cfg(test)]
mod evaluation_tests {
    use super::*;

    #[test]
    fn forced_v3_evaluation_preserves_both_rng_streams_and_rejects_v4() {
        for seeds in [[341, 982], [982, 341]] {
            let (state, _, _) = crate::rl_session::goaded_attacker_fixture_state_v3(true);
            let session = FastActorSessionV1::from_v3_fixture_state(state);
            let FastActorResponseV1::Decision(decision) = session.current_response() else {
                panic!("expected live decision")
            };
            assert_eq!(decision.legal_action_count, 1);
            let mut reference = FrozenPlayPolicyV1::training_fixture_v3();
            let mut adapted = FrozenPlayPolicyV1::training_fixture_v3();
            reference.reset_sampling_v1(seeds);
            adapted.reset_sampling_v1(seeds);
            assert_eq!(
                reference
                    .select_action_v1(PairedBo1PolicyInputV1::new(&session, decision))
                    .unwrap(),
                0
            );
            assert_eq!(
                select_forced_v3_for_evaluation(
                    &mut adapted,
                    &PairedBo1PolicyInputV1::new(&session, decision)
                )
                .unwrap(),
                0
            );
            for seat in [PlayerSeatV1::P0, PlayerSeatV1::P1] {
                for _ in 0..16 {
                    let logits = [-0.7, 0.0, 0.1, 0.8, 1.5];
                    assert_eq!(
                        reference.sample_scores(&logits, seat, 5).unwrap(),
                        adapted.sample_scores(&logits, seat, 5).unwrap()
                    );
                }
            }
            assert!(select_forced_v3_for_evaluation(
                &mut FrozenPlayPolicyV1::training_fixture_v4(),
                &PairedBo1PolicyInputV1::new(&session, decision)
            )
            .is_err());
        }
    }
}
