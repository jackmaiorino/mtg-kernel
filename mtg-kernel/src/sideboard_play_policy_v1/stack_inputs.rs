//! Actor-bound stack policy. Legacy loaders never construct this architecture.
use super::*;
use crate::native_policy_value_net_v1::stack_inputs_v1::{
    NativeStackInputNetV1, StackInputWeightsV1,
};
use crate::public_stack_features_v1::{
    encode_stack_decision_v1, StackColumnPermutationV1, StackEncodedDecisionV1, StackInputModeV1,
};

pub(crate) struct StackInputPlayPolicyV1 {
    base: FrozenPlayPolicyV1,
    model: NativeStackInputNetV1,
    inputs_enabled: bool,
    mode: StackInputModeV1,
    permutation_rng: [SplitMix64; 2],
    captured: Option<StackEncodedDecisionV1>,
}
impl StackInputPlayPolicyV1 {
    pub(crate) fn new(
        base: FrozenPlayPolicyV1,
        weights: StackInputWeightsV1,
    ) -> Result<Self, String> {
        require(
            base.fresh_successor.is_some() && base.successor.is_none(),
            "stack inputs require the exact V4 actor encoder",
        )?;
        let model =
            NativeStackInputNetV1::new(base.model.clone(), weights).map_err(|e| e.to_string())?;
        Ok(Self {
            base,
            model,
            inputs_enabled: true,
            mode: StackInputModeV1::Structured,
            permutation_rng: [SplitMix64::seed(0), SplitMix64::seed(0)],
            captured: None,
        })
    }
    pub(crate) fn with_inputs_enabled(mut self, enabled: bool) -> Self {
        self.inputs_enabled = enabled;
        self.mode = if enabled {
            StackInputModeV1::Structured
        } else {
            StackInputModeV1::Disabled
        };
        self.model = self.model.with_inputs_enabled(enabled);
        self
    }
    pub(crate) fn with_mode(mut self, mode: StackInputModeV1) -> Self {
        self.inputs_enabled = mode != StackInputModeV1::Disabled;
        self.mode = mode;
        self.model = self.model.with_inputs_enabled(self.inputs_enabled);
        self
    }
    pub(crate) fn fork_for_collection(&self) -> Result<Self, String> {
        Ok(Self {
            base: self.base.fork_for_collection_v3()?,
            model: self.model.clone(),
            inputs_enabled: self.inputs_enabled,
            mode: self.mode,
            permutation_rng: [SplitMix64::seed(0), SplitMix64::seed(0)],
            captured: None,
        })
    }
    pub(crate) fn install(
        &mut self,
        parameters: &[NativeNamedParameterV1],
        weights: StackInputWeightsV1,
    ) -> Result<(), String> {
        let mut candidate = self.base.model.clone();
        candidate
            .replace_parameter_snapshot_v1(parameters)
            .map_err(|e| e.to_string())?;
        let model = NativeStackInputNetV1::new(candidate, weights)
            .map_err(|e| e.to_string())?
            .with_inputs_enabled(self.inputs_enabled);
        self.base.replace_training_parameters_v3(parameters)?;
        self.model = model;
        self.captured = None;
        Ok(())
    }
    pub(crate) fn captured(&self) -> Result<&StackEncodedDecisionV1, String> {
        self.captured
            .as_ref()
            .ok_or_else(|| "no captured stack decision".into())
    }
    pub(crate) fn replay(
        &self,
        decision: &StackEncodedDecisionV1,
    ) -> Result<FrozenPlayDecisionScoresV1, String> {
        require(
            decision.stack.permutation.is_some() == (self.mode == StackInputModeV1::Permuted),
            "captured stack transform differs from policy mode",
        )?;
        let output = self.model.forward(decision).map_err(|e| e.to_string())?;
        Ok(FrozenPlayDecisionScoresV1 {
            logits: output.logits,
            value: output.value,
        })
    }
    pub(crate) fn select_with_scores(
        &mut self,
        input: &PairedBo1PolicyInputV1<'_>,
    ) -> Result<(u32, FrozenPlayDecisionScoresV1), RlSessionError> {
        // Clear before any fallible binding operation; an earlier decision is
        // never exposed as the capture of a rejected/stale input.
        self.captured = None;
        let decision = input.decision();
        let fresh = self
            .base
            .fresh_successor
            .as_mut()
            .ok_or_else(|| policy_error("missing V4 encoder".into()))?;
        let encoded = input
            .encode_scoring_owned_v4(&mut fresh.encoder, &mut self.base.owned.buffers())
            .map_err(|e| policy_error(format!("stack-input V4 encoding: {e:?}")))?;
        self.base.owned.globals = encoded.globals;
        fresh.extensions = encoded.extensions;
        let mut paired = encode_stack_decision_v1(FlatScoringDecisionViewV4::new(
            self.base.owned.view(),
            &fresh.extensions,
        ))
        .map_err(|e| policy_error(format!("stack-input V4 tensorization: {e:?}")))?;
        let actor = decision.acting_player as usize;
        let mut permutation_rng = self.permutation_rng[actor];
        if self.mode == StackInputModeV1::Permuted {
            paired.stack.permutation = Some(StackColumnPermutationV1::sample(&mut permutation_rng));
        }
        let output = self
            .model
            .forward(&paired)
            .map_err(|e| policy_error(e.to_string()))?;
        if output.logits.len() != decision.legal_action_count as usize {
            return Err(policy_error("stack-input scorer/menu count differs".into()));
        }
        let selected = self
            .base
            .sample_scores(
                &output.logits,
                decision.acting_player,
                decision.legal_action_count,
            )
            .map_err(policy_error)?;
        self.captured = Some(paired);
        self.permutation_rng[actor] = permutation_rng;
        Ok((
            selected,
            FrozenPlayDecisionScoresV1 {
                logits: output.logits,
                value: output.value,
            },
        ))
    }
}
impl PairedBo1PolicyV1 for StackInputPlayPolicyV1 {
    fn uses_observation_successor_v3(&self) -> bool {
        true
    }
    fn feature_generation_v1(&self) -> PlayPolicyGenerationV1 {
        PlayPolicyGenerationV1::V4
    }
    fn reset_for_game_v1(&mut self, seeds: [u64; 2]) -> Result<(), RlSessionError> {
        self.captured = None;
        self.base.reset_for_game_v1(seeds)?;
        self.permutation_rng = seeds.map(|s| SplitMix64::seed(s ^ 0x5374_6163_6b50_6572));
        Ok(())
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
mod tests {
    use super::*;
    #[test]
    fn stack_policy_preserves_rng_hidden_invariance_replay_and_rejects_stale_capture() {
        use crate::ids::PlayerId;
        use crate::policy_observation_v6::tests::put;
        use crate::state::Zone;
        let (state, _, _) = crate::rl_session::pyroblast_target_fixture_v1();
        let mut outputs = Vec::new();
        let mut permuted_outputs = Vec::new();
        for hidden in [false, true] {
            let mut state = state.clone();
            put(
                &mut state,
                PlayerId::P1,
                if hidden { "Island" } else { "Mountain" },
                Zone::Hand,
            );
            for actor in [PlayerId::P0, PlayerId::P1] {
                for name in ["Island", "Mountain"] {
                    put(&mut state, actor, name, Zone::Library);
                }
                if hidden {
                    state.players[actor.index()].library.reverse();
                }
            }
            let session = FastActorSessionV1::from_v3_fixture_state(state);
            let response = session.current_response();
            let FastActorResponseV1::Decision(decision) = response else {
                panic!()
            };
            let input = PairedBo1PolicyInputV1::new(&session, decision);
            let mut reference = FrozenPlayPolicyV1::training_fixture_v4();
            reference.reset_sampling_v1([321, 654]);
            let mut zero = StackInputPlayPolicyV1::new(
                FrozenPlayPolicyV1::training_fixture_v4(),
                StackInputWeightsV1::zero(),
            )
            .unwrap();
            zero.reset_for_game_v1([321, 654]).unwrap();
            for _ in 0..8 {
                let expected = reference.select_paired_with_scores_v1(&input).unwrap();
                let actual = zero.select_with_scores(&input).unwrap();
                assert_eq!(actual.0, expected.0);
                assert_eq!(actual.1.logits, expected.1.logits);
                assert_eq!(actual.1.value.to_bits(), expected.1.value.to_bits());
                let replay = zero.replay(zero.captured().unwrap()).unwrap();
                assert_eq!(replay.logits, actual.1.logits);
                assert_eq!(replay.value.to_bits(), actual.1.value.to_bits());
            }
            let weights = StackInputWeightsV1::new(
                (0..64 * 440)
                    .map(|i| ((i * 17 % 23) as f32 - 11.0) * 0.003)
                    .collect(),
            )
            .unwrap();
            let mut nonzero = StackInputPlayPolicyV1::new(
                FrozenPlayPolicyV1::training_fixture_v4(),
                weights.clone(),
            )
            .unwrap();
            nonzero.reset_for_game_v1([321, 654]).unwrap();
            let actual = nonzero.select_with_scores(&input).unwrap();
            assert!(actual.0 < decision.legal_action_count);
            assert_ne!(
                actual.1.logits,
                zero.replay(zero.captured().unwrap()).unwrap().logits
            );
            let mut fork = nonzero.fork_for_collection().unwrap();
            fork.reset_for_game_v1([321, 654]).unwrap();
            let forked = fork.select_with_scores(&input).unwrap();
            assert_eq!(actual.0, forked.0);
            assert_eq!(actual.1.logits, forked.1.logits);
            assert_eq!(actual.1.value.to_bits(), forked.1.value.to_bits());
            outputs.push((
                actual.0,
                actual.1.logits,
                actual.1.value.to_bits(),
                nonzero.captured().unwrap().clone(),
            ));
            let mut control = StackInputPlayPolicyV1::new(
                FrozenPlayPolicyV1::training_fixture_v4(),
                weights.clone(),
            )
            .unwrap()
            .with_inputs_enabled(false);
            control.reset_for_game_v1([321, 654]).unwrap();
            reference.reset_sampling_v1([321, 654]);
            let c = control.select_with_scores(&input).unwrap();
            let r = reference.select_paired_with_scores_v1(&input).unwrap();
            assert_eq!(c.0, r.0);
            assert_eq!(c.1.logits, r.1.logits);
            let mut permuted_zero = StackInputPlayPolicyV1::new(
                FrozenPlayPolicyV1::training_fixture_v4(),
                StackInputWeightsV1::zero(),
            )
            .unwrap()
            .with_mode(StackInputModeV1::Permuted);
            permuted_zero.reset_for_game_v1([321, 654]).unwrap();
            reference.reset_sampling_v1([321, 654]);
            let mut previous_columns = None;
            for _ in 0..8 {
                let p = permuted_zero.select_with_scores(&input).unwrap();
                let r = reference.select_paired_with_scores_v1(&input).unwrap();
                assert_eq!(p.0, r.0);
                assert_eq!(p.1.logits, r.1.logits);
                assert_eq!(p.1.value.to_bits(), r.1.value.to_bits());
                let captured = permuted_zero.captured().unwrap();
                let columns = &captured.stack.permutation.as_ref().unwrap().columns;
                if let Some(old) = previous_columns {
                    assert_ne!(&old, columns);
                }
                previous_columns = Some(columns.clone());
                for row in &captured.stack.rows {
                    let mut raw = row.features.clone();
                    let mut transformed = Vec::new();
                    captured.stack.append_model_features(row, &mut transformed);
                    raw.sort_by(f32::total_cmp);
                    transformed.sort_by(f32::total_cmp);
                    assert_eq!(raw, transformed);
                }
            }
            let mut permuted = StackInputPlayPolicyV1::new(
                FrozenPlayPolicyV1::training_fixture_v4(),
                weights.clone(),
            )
            .unwrap()
            .with_mode(StackInputModeV1::Permuted);
            permuted.reset_for_game_v1([321, 654]).unwrap();
            let first = permuted.select_with_scores(&input).unwrap();
            let captured = permuted.captured().unwrap().clone();
            let replay = permuted.replay(&captured).unwrap();
            assert_eq!(first.1.logits, replay.logits);
            assert!(
                nonzero.replay(&captured).is_err(),
                "structured policy rejects a permuted capture"
            );
            let mut fork = permuted.fork_for_collection().unwrap();
            fork.reset_for_game_v1([321, 654]).unwrap();
            let from_fork = fork.select_with_scores(&input).unwrap();
            assert_eq!(first.0, from_fork.0);
            assert_eq!(first.1.logits, from_fork.1.logits);
            assert_eq!(&captured, fork.captured().unwrap());
            permuted_outputs.push((first.0, first.1.logits, first.1.value.to_bits(), captured));
            let mut stale = decision;
            stale.step += 1;
            assert!(nonzero
                .select_with_scores(&PairedBo1PolicyInputV1::new(&session, stale))
                .is_err());
            assert!(nonzero.captured().is_err());
            let parameters = nonzero.base.model.parameter_snapshot_v1();
            nonzero.install(&parameters, weights).unwrap();
            assert!(nonzero.captured().is_err());
            assert_eq!(session.current_response(), response);
        }
        assert_eq!(outputs[0], outputs[1]);
        assert_eq!(permuted_outputs[0], permuted_outputs[1]);
    }
}
