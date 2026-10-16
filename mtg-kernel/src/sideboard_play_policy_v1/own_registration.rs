//! V4 policy requiring explicit own-mainboard context at every ingress.
//! Deliberately does not implement the context-free PairedBo1PolicyV1 trait.
use super::*;
use crate::native_policy_value_net_v1::own_registration::{
    NativeRegistrationNetV1, RegistrationInputWeightsV1,
};
use crate::own_registration_v1::{
    KnownOwnMainboardV1, RegistrationExpectationV1, RegistrationFeatureRowsV1, RegistrationModeV1,
};

pub(crate) struct OwnRegistrationPlayPolicyV1 {
    base: FrozenPlayPolicyV1,
    model: NativeRegistrationNetV1,
    mode: RegistrationModeV1,
    projection_enabled: bool,
    auxiliary: Option<RegistrationFeatureRowsV1>,
}
impl OwnRegistrationPlayPolicyV1 {
    pub(crate) fn mode(&self) -> RegistrationModeV1 {
        self.mode
    }
    pub(crate) fn projection_enabled(&self) -> bool {
        self.projection_enabled
    }
    pub(crate) fn new(
        mut base: FrozenPlayPolicyV1,
        weights: RegistrationInputWeightsV1,
        mode: RegistrationModeV1,
    ) -> Result<Self, String> {
        require(
            base.fresh_successor.is_some() && base.successor.is_none(),
            "own registration requires exact V4 actor encoder",
        )?;
        require(
            base.feature_identity_v1() == FRESH_FEATURE_IDENTITY_V4,
            "own registration base feature identity differs",
        )?;
        // Prediction caches have no registration context key and cannot be used.
        base.inference_cache = None;
        base.inference_key.clear();
        base.inference_probe = None;
        base.fast_search_forward = false;
        let model = NativeRegistrationNetV1::new(base.model.clone(), weights, mode)
            .map_err(|e| e.to_string())?;
        Ok(Self {
            base,
            model,
            mode,
            projection_enabled: true,
            auxiliary: None,
        })
    }
    /// Ordinary continuation B. Own rows only supply a common stored format;
    /// their features never enter the inherited scorer and W stays zero.
    pub(crate) fn new_legacy(base: FrozenPlayPolicyV1) -> Result<Self, String> {
        let mut policy = Self::new(
            base,
            RegistrationInputWeightsV1::zero(),
            RegistrationModeV1::Own,
        )?;
        policy.model = NativeRegistrationNetV1::new_legacy(policy.base.model.clone())
            .map_err(|e| e.to_string())?;
        policy.projection_enabled = false;
        Ok(policy)
    }
    pub(crate) fn fork_for_collection(&self) -> Result<Self, String> {
        let mut base = self.base.fork_for_collection_v3()?;
        base.inference_cache = None;
        base.inference_key.clear();
        base.inference_probe = None;
        Ok(Self {
            base,
            model: self.model.clone(),
            mode: self.mode,
            projection_enabled: self.projection_enabled,
            auxiliary: None,
        })
    }
    pub(crate) fn reset_for_game(&mut self, seeds: [u64; 2]) -> Result<(), RlSessionError> {
        self.auxiliary = None;
        self.base.reset_for_game_v1(seeds)
    }
    pub(crate) fn enable_unclamped_collection_sampler_v1(&mut self) {
        self.base.enable_unclamped_collection_sampler_v1();
    }
    pub(crate) fn collection_sampler_identity_v1(&self) -> Option<&'static str> {
        self.base.collection_sampler_identity_v1()
    }
    pub(crate) fn runtime_sampler_identity_v1(&self) -> &'static str {
        self.base.runtime_sampler_identity_v1()
    }
    pub(crate) fn runtime_sampler_max_actions_v1(&self) -> usize {
        self.base.runtime_sampler_max_actions_v1()
    }
    pub(crate) fn set_training_math_v1(&mut self, math: TrainingMathV1) {
        self.base.set_training_math_v1(math);
    }
    pub(crate) fn training_math_v1(&self) -> TrainingMathV1 {
        self.base.training_math_v1()
    }
    pub(crate) fn feature_identity_v1(&self) -> FreshFeatureIdentityV1 {
        self.base.feature_identity_v1()
    }
    pub(crate) fn training_model_v3(&self) -> NativePolicyValueNetV1 {
        self.base.training_model_v3()
    }
    pub(crate) fn training_parameters_v3(&self) -> Vec<NativeNamedParameterV1> {
        self.base.training_parameters_v3()
    }
    pub(crate) fn install(
        &mut self,
        parameters: &[NativeNamedParameterV1],
        weights: RegistrationInputWeightsV1,
    ) -> Result<(), String> {
        if !self.projection_enabled && !weights.is_zero() {
            return Err("baseline registration projection must stay zero".into());
        }
        let mut candidate = self.base.model.clone();
        candidate
            .replace_parameter_snapshot_v1(parameters)
            .map_err(|e| e.to_string())?;
        let model = if self.projection_enabled {
            NativeRegistrationNetV1::new(candidate, weights, self.mode)
        } else {
            NativeRegistrationNetV1::new_legacy(candidate)
        }
        .map_err(|e| e.to_string())?;
        self.base.replace_training_parameters_v3(parameters)?;
        self.model = model;
        self.auxiliary = None;
        Ok(())
    }
    pub(crate) fn captured(
        &self,
    ) -> Result<(&NativeFlatDecisionTensorV4, &RegistrationFeatureRowsV1), String> {
        let rows = self
            .auxiliary
            .as_ref()
            .ok_or("no captured registration decision")?;
        Ok((self.base.last_scored_training_tensor_v4()?, rows))
    }
    pub(crate) fn replay(
        &self,
        tensor: &NativeFlatDecisionTensorV4,
        rows: &RegistrationFeatureRowsV1,
        expected: RegistrationExpectationV1<'_>,
    ) -> Result<FrozenPlayDecisionScoresV1, String> {
        let output = self
            .model
            .forward_rows_mode(
                encoded_decision_view_v4(tensor),
                rows,
                expected,
                self.base.forward_mode_v1(),
            )
            .map_err(|e| e.to_string())?;
        Ok(FrozenPlayDecisionScoresV1 {
            logits: output.logits,
            value: output.value,
        })
    }
    pub(crate) fn score(
        &mut self,
        input: &PairedBo1PolicyInputV1<'_>,
        context: &KnownOwnMainboardV1,
        expected: RegistrationExpectationV1<'_>,
    ) -> Result<FrozenPlayDecisionScoresV1, RlSessionError> {
        self.auxiliary = None;
        let decision = input.decision();
        if decision.acting_player != expected.actor {
            return Err(policy_error(
                "registration expected actor differs from current decision".into(),
            ));
        }
        context.validate_expected(expected).map_err(policy_error)?;
        let rows = context.rows(self.mode).map_err(policy_error)?;
        let activation = self.base.forward_mode_v1();
        let fresh = self
            .base
            .fresh_successor
            .as_mut()
            .ok_or_else(|| policy_error("missing V4 encoder".into()))?;
        let encoded = input
            .encode_scoring_owned_v4(&mut fresh.encoder, &mut self.base.owned.buffers())
            .map_err(|e| policy_error(format!("registration V4 encoding: {e:?}")))?;
        self.base.owned.globals = encoded.globals;
        fresh.extensions = encoded.extensions;
        fresh
            .tensorizer
            .fill(
                FlatScoringDecisionViewV4::new(self.base.owned.view(), &fresh.extensions),
                &mut fresh.tensor,
            )
            .map_err(|e| policy_error(format!("registration V4 tensorization: {e:?}")))?;
        let output = self
            .model
            .forward_rows_mode(
                encoded_decision_view_v4(&fresh.tensor),
                &rows,
                expected,
                activation,
            )
            .map_err(|e| policy_error(e.to_string()))?;
        if output.logits.len() != decision.legal_action_count as usize {
            return Err(policy_error(
                "registration scorer/menu count mismatch".into(),
            ));
        }
        self.auxiliary = Some(rows);
        Ok(FrozenPlayDecisionScoresV1 {
            logits: output.logits,
            value: output.value,
        })
    }
    pub(crate) fn select_with_scores(
        &mut self,
        input: &PairedBo1PolicyInputV1<'_>,
        context: &KnownOwnMainboardV1,
        expected: RegistrationExpectationV1<'_>,
    ) -> Result<(u32, FrozenPlayDecisionScoresV1), RlSessionError> {
        let scores = self.score(input, context, expected)?;
        let decision = input.decision();
        let selected = self
            .base
            .sample_scores(
                &scores.logits,
                decision.acting_player,
                decision.legal_action_count,
            )
            .map_err(policy_error)?;
        Ok((selected, scores))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn context<'a>(
        episode: &'a str,
        actor: PlayerSeatV1,
        name: &str,
    ) -> (KnownOwnMainboardV1, RegistrationExpectationV1<'a>) {
        let selected = crate::sideboard::DeckConfigurationV1::new_exact_v1(
            crate::runtime_decks::runtime_deck_by_id(name)
                .unwrap()
                .card_ids
                .to_vec(),
            vec![0; 15],
        )
        .unwrap();
        (
            KnownOwnMainboardV1::new(episode, actor, &selected).unwrap(),
            RegistrationExpectationV1 {
                external_episode_id: episode,
                actor,
                selected_mainboard_sha256: selected.mainboard_sha256_v1(),
            },
        )
    }
    #[test]
    fn zero_parity_nonzero_effect_replay_binding_and_fork_capture() {
        let (state, _, _) = crate::rl_session::pyroblast_target_fixture_v1();
        let session = FastActorSessionV1::from_v3_fixture_state(state);
        let FastActorResponseV1::Decision(decision) = session.current_response() else {
            panic!()
        };
        let input = PairedBo1PolicyInputV1::new(&session, decision);
        let (context, expected) = context("external-episode", decision.acting_player, "Wildfire");
        let mut base = FrozenPlayPolicyV1::training_fixture_v4();
        base.reset_sampling_v1([123, 456]);
        let reference = base.select_paired_with_scores_v1(&input).unwrap();
        for mode in [RegistrationModeV1::Own, RegistrationModeV1::Mean] {
            let mut zero = OwnRegistrationPlayPolicyV1::new(
                FrozenPlayPolicyV1::training_fixture_v4(),
                RegistrationInputWeightsV1::zero(),
                mode,
            )
            .unwrap();
            zero.reset_for_game([123, 456]).unwrap();
            let actual = zero.select_with_scores(&input, &context, expected).unwrap();
            assert_eq!(actual.0, reference.0);
            assert_eq!(actual.1.logits, reference.1.logits);
            assert_eq!(actual.1.value.to_bits(), reference.1.value.to_bits());
            let (tensor, rows) = zero.captured().unwrap();
            let replay = zero.replay(tensor, rows, expected).unwrap();
            assert_eq!(replay.logits, actual.1.logits);
            assert_eq!(replay.value.to_bits(), actual.1.value.to_bits());
            assert!(zero
                .replay(
                    tensor,
                    rows,
                    RegistrationExpectationV1 {
                        external_episode_id: "reused-numeric-session-id-is-not-enough",
                        ..expected
                    }
                )
                .is_err());
            assert!(zero
                .replay(
                    tensor,
                    rows,
                    RegistrationExpectationV1 {
                        actor: if expected.actor == PlayerSeatV1::P0 {
                            PlayerSeatV1::P1
                        } else {
                            PlayerSeatV1::P0
                        },
                        ..expected
                    }
                )
                .is_err());
            assert!(zero.fork_for_collection().unwrap().captured().is_err());
            let mut nonzero = OwnRegistrationPlayPolicyV1::new(
                FrozenPlayPolicyV1::training_fixture_v4(),
                RegistrationInputWeightsV1::new(vec![0.1; 64 * crate::own_registration_v1::WIDTH])
                    .unwrap(),
                mode,
            )
            .unwrap();
            let changed = nonzero.score(&input, &context, expected).unwrap();
            assert!(
                changed.logits != actual.1.logits
                    || changed.value.to_bits() != actual.1.value.to_bits()
            );
            assert!(zero
                .score(
                    &input,
                    &context,
                    RegistrationExpectationV1 {
                        external_episode_id: "wrong",
                        ..expected
                    }
                )
                .is_err());
            assert!(zero.captured().is_err());
        }
        let mut legacy =
            OwnRegistrationPlayPolicyV1::new_legacy(FrozenPlayPolicyV1::training_fixture_v4())
                .unwrap();
        legacy.reset_for_game([123, 456]).unwrap();
        let actual = legacy
            .select_with_scores(&input, &context, expected)
            .unwrap();
        assert_eq!(actual.0, reference.0);
        assert_eq!(actual.1.logits, reference.1.logits);
        assert_eq!(actual.1.value.to_bits(), reference.1.value.to_bits());
        assert!(legacy
            .install(
                &legacy.training_parameters_v3(),
                RegistrationInputWeightsV1::new(vec![0.1; 64 * crate::own_registration_v1::WIDTH])
                    .unwrap()
            )
            .is_err());
        assert!(OwnRegistrationPlayPolicyV1::new(
            FrozenPlayPolicyV1::training_fixture_v3(),
            RegistrationInputWeightsV1::zero(),
            RegistrationModeV1::Own
        )
        .is_err());
    }
    #[test]
    fn shared_policy_requires_current_context_and_mean_does_not_vary() {
        let (state, _, _) = crate::rl_session::pyroblast_target_fixture_v1();
        let session = FastActorSessionV1::from_v3_fixture_state(state);
        let FastActorResponseV1::Decision(decision) = session.current_response() else {
            panic!()
        };
        let input = PairedBo1PolicyInputV1::new(&session, decision);
        let (first, first_expected) = context("game-a", decision.acting_player, "Wildfire");
        let (second, second_expected) = context("game-b", decision.acting_player, "Burn");
        let first_features = first.rows(RegistrationModeV1::Own).unwrap().features;
        let second_features = second.rows(RegistrationModeV1::Own).unwrap().features;
        let col = first_features
            .iter()
            .zip(&second_features)
            .position(|(a, b)| a != b)
            .unwrap();
        let mut weights = vec![0.0; 64 * crate::own_registration_v1::WIDTH];
        for row in 0..64 {
            weights[row * crate::own_registration_v1::WIDTH + col] = 0.3;
        }
        for mode in [RegistrationModeV1::Own, RegistrationModeV1::Mean] {
            let mut policy = OwnRegistrationPlayPolicyV1::new(
                FrozenPlayPolicyV1::training_fixture_v4(),
                RegistrationInputWeightsV1::new(weights.clone()).unwrap(),
                mode,
            )
            .unwrap();
            let a = policy.score(&input, &first, first_expected).unwrap();
            assert!(policy.score(&input, &first, second_expected).is_err());
            assert!(policy.captured().is_err());
            let b = policy.score(&input, &second, second_expected).unwrap();
            if mode == RegistrationModeV1::Own {
                assert!(a.logits != b.logits || a.value.to_bits() != b.value.to_bits());
            } else {
                assert_eq!(a.logits, b.logits);
                assert_eq!(a.value.to_bits(), b.value.to_bits());
            }
            assert_eq!(policy.captured().unwrap().1.external_episode_id, "game-b");
            let wrong_actor = if decision.acting_player == PlayerSeatV1::P0 {
                PlayerSeatV1::P1
            } else {
                PlayerSeatV1::P0
            };
            let (opposite, opposite_expected) = context("game-b", wrong_actor, "Burn");
            assert!(policy.score(&input, &opposite, opposite_expected).is_err());
        }
    }
}
