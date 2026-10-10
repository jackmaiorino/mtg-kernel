//! Explicit public-feature policy adapter. No legacy loader constructs this.
//! The inherited generation describes the base actor tensor, not model identity.
use super::*;
#[cfg(test)]
mod state_only_tests;
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

/// Explicit evaluation-only counterpart of the previously qualified adapter.
/// The session method tries the original encoder first and never steps its
/// private repair copy. The installed policy and all training paths stay V3.
pub(crate) fn select_spell_adapter_v3_for_evaluation(
    policy: &mut FrozenPlayPolicyV1,
    input: &PairedBo1PolicyInputV1<'_>,
) -> Result<(u32, FrozenPlayDecisionScoresV1, bool), RlSessionError> {
    if policy.feature_generation_v1() != PlayPolicyGenerationV1::V3 {
        return Err(policy_error("spell-target evaluation requires V3".into()));
    }
    let decision = input.decision();
    let successor = policy
        .successor
        .as_mut()
        .ok_or_else(|| policy_error("missing V3 encoder".into()))?;
    let (encoded, repaired) = input
        .encode_scoring_v3_spell_target_adapter_v1(
            &mut successor.encoder,
            &mut policy.owned.buffers(),
        )
        .map_err(|e| {
            policy_error(format!(
                "V3 spell-target evaluation: {e:?}; decision={decision:?}"
            ))
        })?;
    policy.owned.globals = encoded.globals;
    successor.extensions = encoded.extensions;
    let scores = policy.score_owned().map_err(policy_error)?;
    let selected = policy
        .sample_scores(
            &scores.logits,
            decision.acting_player,
            decision.legal_action_count,
        )
        .map_err(policy_error)?;
    Ok((selected, scores, repaired))
}

pub(crate) struct PublicInputPlayPolicyV1 {
    base: FrozenPlayPolicyV1,
    model: NativePublicInputNetV1,
    inputs_enabled: bool,
    auxiliary: Option<crate::public_cost_features_v1::PublicFeatureRowsV1>,
}

impl PublicInputPlayPolicyV1 {
    /// Same immutable learned parameters, private encoders and per-game RNG.
    pub(crate) fn fork_for_collection(&self) -> Result<Self, String> {
        Ok(Self {
            base: self.base.fork_for_collection_v3()?,
            model: self.model.clone(),
            inputs_enabled: self.inputs_enabled,
            auxiliary: None,
        })
    }

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

    #[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
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
        let (observation, actions) = input.diagnostic_visible_v4().map_err(policy_error)?;
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
    fn v3_spell_target_evaluation_resolves_departed_linked_exile_source() {
        use crate::ids::PlayerId;
        use crate::policy_observation_v6::tests::put;
        use crate::state::Zone;
        let mut outputs = Vec::new();
        let mut tensors = Vec::new();
        for variant in 0..2 {
            let mut state = crate::rl_session::linked_exile_target_fixture_v1();
            put(
                &mut state,
                PlayerId::P1,
                if variant == 0 { "Island" } else { "Mountain" },
                Zone::Hand,
            );
            put(&mut state, PlayerId::P1, "Mountain", Zone::Library);
            if variant == 1 {
                state.players[1].library.reverse();
            }
            let session = FastActorSessionV1::from_v3_fixture_state(state);
            let response = session.current_response();
            let FastActorResponseV1::Decision(decision) = response else {
                panic!("expected a live priority choice: {response:?}");
            };
            assert_eq!(decision.acting_player, PlayerId::P0.into());
            assert_eq!(decision.legal_action_count, 2);
            let input = PairedBo1PolicyInputV1::new(&session, decision);
            let (observation, _) = input.diagnostic_visible_v1().unwrap();
            assert!(observation.extensions.historical_public_sources.is_empty());
            assert!(observation
                .projection
                .surface
                .object_relations
                .iter()
                .any(|relation| {
                    matches!(relation, crate::rl::ObjectRelationPublicV4::ExiledBy { .. })
                }));
            let mut policy = FrozenPlayPolicyV1::training_fixture_v3();
            policy.reset_sampling_v1([123, 456]);
            assert!(policy
                .select_paired_with_scores_v1(&input)
                .unwrap_err()
                .to_string()
                .contains("InvalidReference"));
            let expected = select_spell_adapter_v3_for_evaluation(&mut policy, &input).unwrap();
            assert!(expected.2);
            assert_eq!(expected.1.logits.len(), 2);
            assert!(expected.1.logits.iter().all(|x| x.is_finite()));
            assert!(expected.1.value.is_finite());
            tensors.push(policy.successor.as_ref().unwrap().tensor.clone());
            outputs.push((
                expected.0,
                expected.1.logits.clone(),
                expected.1.value.to_bits(),
            ));
            policy.reset_sampling_v1([123, 456]);
            let replay = select_spell_adapter_v3_for_evaluation(&mut policy, &input).unwrap();
            assert_eq!(replay.0, expected.0);
            assert_eq!(replay.1.logits, expected.1.logits);
            assert_eq!(replay.1.value.to_bits(), expected.1.value.to_bits());
            assert_eq!(
                policy.successor.as_ref().unwrap().tensor,
                *tensors.last().unwrap()
            );
            assert_eq!(session.current_response(), response);
            let mut stale = decision;
            stale.step += 1;
            assert!(select_spell_adapter_v3_for_evaluation(
                &mut policy,
                &PairedBo1PolicyInputV1::new(&session, stale)
            )
            .is_err());
        }
        assert_eq!(outputs[0], outputs[1]);
        assert_eq!(tensors[0], tensors[1]);
    }

    #[test]
    fn v3_spell_target_evaluation_rejects_unauthenticated_linked_exile_source() {
        let mut state = crate::rl_session::linked_exile_target_fixture_v1();
        state.engine.linked_exile_records[0].source.card_def = 0;
        let session = FastActorSessionV1::from_v3_fixture_state(state);
        let FastActorResponseV1::Decision(decision) = session.current_response() else {
            panic!("expected a live priority choice");
        };
        let mut policy = FrozenPlayPolicyV1::training_fixture_v3();
        assert!(select_spell_adapter_v3_for_evaluation(
            &mut policy,
            &PairedBo1PolicyInputV1::new(&session, decision)
        )
        .is_err());
    }

    #[test]
    fn public_v4_spell_target_features_preserve_zero_control_and_hidden_invariance() {
        use crate::ids::PlayerId;
        use crate::policy_observation_v6::tests::put;
        use crate::state::Zone;
        let (state, _, _) = crate::rl_session::pyroblast_target_fixture_v1();
        let mut outputs = Vec::new();
        for variant in 0..2 {
            let mut state = state.clone();
            put(
                &mut state,
                PlayerId::P1,
                if variant == 0 { "Island" } else { "Mountain" },
                Zone::Hand,
            );
            for actor in [PlayerId::P0, PlayerId::P1] {
                for name in ["Island", "Mountain"] {
                    put(&mut state, actor, name, Zone::Library);
                }
                if variant == 1 {
                    state.players[actor.index()].library.reverse();
                }
            }
            let session = FastActorSessionV1::from_v3_fixture_state(state);
            let response = session.current_response();
            let FastActorResponseV1::Decision(decision) = response else {
                panic!()
            };
            let input = PairedBo1PolicyInputV1::new(&session, decision);
            assert!(input.diagnostic_visible_v1().is_err());
            let mut reference = FrozenPlayPolicyV1::training_fixture_v4();
            let mut zero = PublicInputPlayPolicyV1::new(
                FrozenPlayPolicyV1::training_fixture_v4(),
                PublicInputWeightsV1::zero(),
            )
            .unwrap();
            reference.reset_sampling_v1([321, 654]);
            zero.reset_for_game_v1([321, 654]).unwrap();
            for _ in 0..8 {
                let expected = reference.select_paired_with_scores_v1(&input).unwrap();
                let actual = zero.select_with_scores(&input).unwrap();
                assert_eq!(actual.0, expected.0);
                assert_eq!(actual.1.logits, expected.1.logits);
                assert_eq!(actual.1.value.to_bits(), expected.1.value.to_bits());
            }
            let mut nonzero = PublicInputPlayPolicyV1::new(
                FrozenPlayPolicyV1::training_fixture_v4(),
                PublicInputWeightsV1::new(vec![0.01; 2048], vec![0.02; 384]).unwrap(),
            )
            .unwrap();
            nonzero.reset_for_game_v1([321, 654]).unwrap();
            let (action, scores) = nonzero.select_with_scores(&input).unwrap();
            assert!(
                action < 2
                    && scores.logits.len() == 2
                    && scores.logits.iter().all(|x| x.is_finite())
            );
            outputs.push((
                action,
                scores.logits,
                scores.value.to_bits(),
                serde_json::to_vec(nonzero.auxiliary.as_ref().unwrap()).unwrap(),
            ));
            assert_eq!(session.current_response(), response);
        }
        assert_eq!(outputs[0], outputs[1]);
    }

    #[test]
    fn v3_spell_target_evaluation_preserves_hidden_invariance_and_replay() {
        use crate::ids::PlayerId;
        use crate::policy_observation_v6::tests::put;
        use crate::state::Zone;
        let (state, _, _) = crate::rl_session::pyroblast_target_fixture_v1();
        let mut outputs = Vec::new();
        let mut tensors = Vec::new();
        let mut diagnostics = Vec::new();
        for variant in 0..2 {
            let mut state = state.clone();
            put(
                &mut state,
                PlayerId::P1,
                if variant == 0 { "Island" } else { "Mountain" },
                Zone::Hand,
            );
            for actor in [PlayerId::P0, PlayerId::P1] {
                for name in ["Island", "Mountain"] {
                    put(&mut state, actor, name, Zone::Library);
                }
                if variant == 1 {
                    state.players[actor.index()].library.reverse();
                }
            }
            let session = FastActorSessionV1::from_v3_fixture_state(state);
            let response = session.current_response();
            let FastActorResponseV1::Decision(decision) = response else {
                panic!("expected target choice")
            };
            let mut policy = FrozenPlayPolicyV1::training_fixture_v3();
            policy.reset_sampling_v1([123, 456]);
            let input = PairedBo1PolicyInputV1::new(&session, decision);
            assert!(input.diagnostic_visible_v1().is_err());
            let (observation, actions, diagnostic_repaired) =
                input.diagnostic_visible_spell_adapter_v1().unwrap();
            assert!(diagnostic_repaired);
            assert_eq!(actions.len(), 2);
            diagnostics.push(serde_json::to_vec(&(observation, actions)).unwrap());
            assert_eq!(
                *diagnostics.last().unwrap(),
                serde_json::to_vec(&input.diagnostic_visible_v4().unwrap()).unwrap()
            );
            let mut stale = decision;
            stale.step += 1;
            assert!(PairedBo1PolicyInputV1::new(&session, stale)
                .diagnostic_visible_spell_adapter_v1()
                .is_err());
            assert!(PairedBo1PolicyInputV1::new(&session, stale)
                .diagnostic_visible_v4()
                .is_err());
            assert!(policy.select_paired_with_scores_v1(&input).is_err());
            let (action, scores, repaired) =
                select_spell_adapter_v3_for_evaluation(&mut policy, &input).unwrap();
            assert!(repaired);
            assert_eq!(scores.logits.len(), 2);
            assert_eq!(session.current_response(), response);
            assert!(scores.logits.iter().all(|x| x.is_finite()) && scores.value.is_finite());
            tensors.push(policy.successor.as_ref().unwrap().tensor.clone());
            outputs.push((action, scores.logits, scores.value.to_bits()));
            policy.reset_sampling_v1([123, 456]);
            assert_eq!(
                select_spell_adapter_v3_for_evaluation(&mut policy, &input)
                    .unwrap()
                    .0,
                action
            );
            assert!(select_spell_adapter_v3_for_evaluation(
                &mut FrozenPlayPolicyV1::training_fixture_v4(),
                &input
            )
            .is_err());
        }
        assert_eq!(outputs[0], outputs[1]);
        assert_eq!(tensors[0], tensors[1]);
        assert_eq!(diagnostics[0], diagnostics[1]);
    }

    #[test]
    fn v3_spell_target_evaluation_preserves_valid_scores_tensors_and_rng() {
        let (mut state, _, _) = crate::rl_session::pyroblast_target_fixture_v1();
        state.stack.remove(1);
        let session = FastActorSessionV1::from_v3_fixture_state(state);
        let FastActorResponseV1::Decision(decision) = session.current_response() else {
            panic!()
        };
        let input = PairedBo1PolicyInputV1::new(&session, decision);
        let original_visible = input.diagnostic_visible_v1().unwrap();
        assert_eq!(
            serde_json::to_vec(&original_visible).unwrap(),
            serde_json::to_vec(&input.diagnostic_visible_v4().unwrap()).unwrap()
        );
        let (observation, actions, repaired) = input.diagnostic_visible_spell_adapter_v1().unwrap();
        assert!(!repaired);
        assert_eq!(
            serde_json::to_vec(&original_visible).unwrap(),
            serde_json::to_vec(&(observation, actions)).unwrap()
        );
        let mut original = FrozenPlayPolicyV1::training_fixture_v3();
        let mut adapted = FrozenPlayPolicyV1::training_fixture_v3();
        original.reset_sampling_v1([222, 444]);
        adapted.reset_sampling_v1([222, 444]);
        for _ in 0..8 {
            let a = original.select_paired_with_scores_v1(&input).unwrap();
            let b = select_spell_adapter_v3_for_evaluation(&mut adapted, &input).unwrap();
            assert!(!b.2);
            assert_eq!(a.0, b.0);
            assert_eq!(a.1.logits, b.1.logits);
            assert_eq!(a.1.value.to_bits(), b.1.value.to_bits());
            assert_eq!(
                original.successor.as_ref().unwrap().tensor,
                adapted.successor.as_ref().unwrap().tensor
            );
        }
    }

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
