//! Actor-visible V4 key and fresh action authority. No sampled playing caller.
use super::*;

/// Opaque authority over an ordered visible menu, never over a hidden world.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct V4SearchActionTokenV1(FlatActionDecisionSliceV3);

fn hash_visible(
    mut observation: crate::policy_observation_v6::ObservationV6,
    semantics: Vec<ActionSemanticV1>,
    history: Vec<crate::policy_observation_v7::HistoricalPublicSourceV7>,
    depth: u32,
) -> Result<[u8; 32], Error> {
    observation.step_index = 0;
    observation.physical_decision_id = 0;
    observation.visible_projection_hash = 0;
    let bytes = serde_json::to_vec(&(observation, semantics, history, depth))
        .map_err(|_| Error::InvalidVisibleBinding)?;
    let mut hash = Sha256::new();
    hash.update(b"mtg-kernel/v4-search-visible-key/v1\0");
    hash.update(bytes);
    Ok(hash.finalize().into())
}

impl FastActorSessionV1 {
    pub(crate) fn kernel_search_visible_key_v4(&self, depth: u32) -> Result<[u8; 32], Error> {
        if self.flat_action_contract_mode != FlatActionContractModeV1::V3 {
            return Err(Error::UnsupportedActionContract);
        }
        let FastActorResponseV1::Decision(d) = self.current_response() else {
            return Err(Error::NoLiveDecision);
        };
        let (observation, semantics) = self
            .diagnostic_current_decision_input_v4(d)
            .map_err(|_| Error::InvalidVisibleBinding)?;
        let actor = self.current.as_ref().ok_or(Error::NoLiveDecision)?.actor;
        let history =
            crate::policy_observation_v7::policy_observation_extensions_v7(&self.state, actor)
                .map_err(|_| Error::InvalidVisibleBinding)?
                .historical_public_sources;
        let semantics = semantics
            .into_iter()
            .map(|x| frozen_pending_trigger_semantic_v4(&self.state, x))
            .collect();
        hash_visible(observation, semantics, history, depth)
    }

    pub(crate) fn kernel_search_action_token_v4(
        &self,
        expected: FastActorDecisionV1,
    ) -> Result<V4SearchActionTokenV1, Error> {
        if self.flat_action_contract_mode != FlatActionContractModeV1::V3 {
            return Err(Error::UnsupportedActionContract);
        }
        if self.current_response() != FastActorResponseV1::Decision(expected) {
            return Err(Error::InvalidVisibleBinding);
        }
        let count = expected.legal_action_count as usize;
        let capacity = count
            .checked_mul(FLAT_ACTION_MAX_TRIGGER_ORDER_REFS_V1)
            .ok_or(Error::InvalidVisibleBinding)?
            .max(256);
        let mut actions = vec![FlatActionCoreV1::default(); count];
        let mut refs = vec![FlatActionRefV2::default(); capacity];
        let mut objects = vec![FlatActionObjectV2::default(); capacity];
        let slice = self
            .encode_current_flat_action_slice_v4(
                expected,
                &mut FlatActionDecisionSliceBuffersV2 {
                    actions: &mut actions,
                    refs: &mut refs,
                    objects: &mut objects,
                },
            )
            .map_err(|_| Error::InvalidVisibleBinding)?;
        Ok(V4SearchActionTokenV1(slice))
    }

    pub(crate) fn kernel_search_consume_v4(
        &mut self,
        expected: FastActorDecisionV1,
        token: V4SearchActionTokenV1,
        index: u32,
    ) -> Result<FastActorResponseV1, Error> {
        if self.kernel_search_action_token_v4(expected)? != token
            || index >= token.0.active_action_count
        {
            return Err(Error::InvalidVisibleBinding);
        }
        let response = self
            .step(expected.episode_id, expected.step, index)
            .map_err(|_| Error::StepFailed)?;
        if let FastActorResponseV1::Terminal(t) = &response {
            if t.terminal_classification == TerminalClassificationV1::Halted {
                return Err(Error::HaltedSimulation);
            }
        }
        Ok(response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn session() -> FastActorSessionV1 {
        FastActorSessionV1::from_v3_fixture_state(super::super::tests::state(PlayerId::P0))
    }
    fn decision(s: &FastActorSessionV1) -> FastActorDecisionV1 {
        let FastActorResponseV1::Decision(d) = s.current_response() else {
            panic!("fixture")
        };
        d
    }
    #[test]
    fn v4_search_key_depth_visible_and_hidden_controls() {
        let s = session();
        let key = s.kernel_search_visible_key_v4(3).unwrap();
        assert_ne!(key, s.kernel_search_visible_key_v4(2).unwrap());
        for seed in [8172, 9381] {
            assert_eq!(
                key,
                s.kernel_search_redeterminized_clone_v4(seed)
                    .unwrap()
                    .kernel_search_visible_key_v4(3)
                    .unwrap()
            );
        }
        let mut state = super::super::tests::state(PlayerId::P0);
        state.players[0].life -= 1;
        assert_ne!(
            key,
            FastActorSessionV1::from_v3_fixture_state(state)
                .kernel_search_visible_key_v4(3)
                .unwrap()
        );
        let (obs, semantics) = s
            .diagnostic_current_decision_input_v4(decision(&s))
            .unwrap();
        let base = hash_visible(obs.clone(), semantics.clone(), vec![], 3).unwrap();
        let mut transport = obs.clone();
        transport.step_index += 7;
        transport.physical_decision_id += 9;
        transport.visible_projection_hash ^= 1;
        assert_eq!(
            base,
            hash_visible(transport, semantics.clone(), vec![], 3).unwrap()
        );
        let mut substep = obs.clone();
        substep.substep_index += 1;
        assert_ne!(
            base,
            hash_visible(substep, semantics.clone(), vec![], 3).unwrap()
        );
        let mut reordered = semantics.clone();
        reordered.reverse();
        assert_ne!(reordered, semantics);
        assert_ne!(base, hash_visible(obs, reordered, vec![], 3).unwrap());
    }
    #[test]
    fn v4_search_consume_matches_direct_step_and_accepts_equal_clone_binding() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let s = FastActorSessionV1::from_v3_fixture_state(super::super::tests::state(actor));
            let d = decision(&s);
            let token = s.kernel_search_action_token_v4(d).unwrap();
            for seed in [8172, 9381] {
                let mut sampled = s.kernel_search_redeterminized_clone_v4(seed).unwrap();
                assert_eq!(
                    s.kernel_search_visible_key_v4(3),
                    sampled.kernel_search_visible_key_v4(3)
                );
                assert_eq!(token, sampled.kernel_search_action_token_v4(d).unwrap());
                let mut direct = sampled.clone();
                assert_eq!(
                    sampled.kernel_search_consume_v4(d, token, 0).unwrap(),
                    direct.step(d.episode_id, d.step, 0).unwrap()
                );
                assert_eq!(
                    sampled.diagnostic_state_hash(),
                    direct.diagnostic_state_hash()
                );
            }
        }
    }
    #[test]
    fn v4_search_key_hidden_trigger_contracts_and_natural_consume() {
        let (mut hunter, source, _, _) =
            crate::rl_session::avenging_hunter_undercity_arena_choose_targets_state_v1(false);
        crate::rl_session::shuffle_trigger_source_into_library_v1(
            &mut hunter,
            source,
            PlayerId::P0,
        );
        let (ordered, _) = crate::rl_session::hidden_order_triggers_state_v1(2);
        for (i, mut state) in [hunter, ordered].into_iter().enumerate() {
            for owner in [PlayerId::P0, PlayerId::P1] {
                if i == 0 && owner == PlayerId::P1 {
                    continue;
                }
                for name in ["Forest", "Island", "Mountain"] {
                    crate::policy_observation_v6::tests::put(
                        &mut state,
                        owner,
                        name,
                        Zone::Library,
                    );
                }
            }
            let mut s = FastActorSessionV1::from_v3_fixture_state(state);
            let d = decision(&s);
            let key = s.kernel_search_visible_key_v4(3).unwrap();
            let token = s.kernel_search_action_token_v4(d).unwrap();
            for seed in [1, 8172] {
                let sample = s.kernel_search_redeterminized_clone_v4(seed).unwrap();
                assert_eq!(key, sample.kernel_search_visible_key_v4(3).unwrap());
                assert_eq!(token, sample.kernel_search_action_token_v4(d).unwrap());
            }
            if i == 0 {
                // Only the original world is valid for stepping until the sampler repair.
                let response = s.kernel_search_consume_v4(d, token, 0).unwrap();
                assert!(matches!(response, FastActorResponseV1::Terminal(t)
                    if t.terminal_classification == TerminalClassificationV1::Natural));
            }
        }
    }
    #[test]
    fn v4_search_consume_rejects_invalid_authority_without_mutation() {
        let mut s = session();
        let d = decision(&s);
        let token = s.kernel_search_action_token_v4(d).unwrap();
        let hash = s.diagnostic_state_hash();
        assert_eq!(
            s.kernel_search_consume_v4(d, token, d.legal_action_count),
            Err(Error::InvalidVisibleBinding)
        );
        let mut wrong = d;
        wrong.episode_id += 1;
        assert_eq!(
            s.kernel_search_consume_v4(wrong, token, 0),
            Err(Error::InvalidVisibleBinding)
        );
        let mut forged = token;
        forged.0.binding.0.episode_id += 1;
        assert_eq!(
            s.kernel_search_consume_v4(d, forged, 0),
            Err(Error::InvalidVisibleBinding)
        );
        assert_eq!(hash, s.diagnostic_state_hash());
        s.kernel_search_consume_v4(d, token, 0).unwrap();
        let after = s.diagnostic_state_hash();
        assert_eq!(
            s.kernel_search_consume_v4(d, token, 0),
            Err(Error::InvalidVisibleBinding)
        );
        assert_eq!(after, s.diagnostic_state_hash());
        let mut old = FastActorSessionV1::reset_with_limits(23, 91, 1000, 1000);
        assert_eq!(
            old.kernel_search_consume_v4(d, token, 0),
            Err(Error::UnsupportedActionContract)
        );
        assert_eq!(
            old.kernel_search_visible_key_v4(3),
            Err(Error::UnsupportedActionContract)
        );
    }
    #[test]
    fn v4_search_key_allows_sampled_library_choice() {
        let s = FastActorSessionV1::from_v3_fixture_state(
            crate::policy_observation_v6::tests::forest_search_state(false, "Lightning Bolt"),
        );
        assert!(s.kernel_search_visible_key_v4(2).is_ok());
        assert!(s.kernel_search_action_token_v4(decision(&s)).is_ok());
    }
    #[test]
    fn v4_search_consume_multiple_live_nodes_match_direct_steps() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut s =
                FastActorSessionV1::from_v3_fixture_state(super::super::tests::state(actor));
            let mut direct = s.clone();
            let mut consumed = 0;
            for _ in 0..16 {
                let FastActorResponseV1::Decision(d) = s.current_response() else {
                    break;
                };
                let token = s.kernel_search_action_token_v4(d).unwrap();
                let response = s.kernel_search_consume_v4(d, token, 0).unwrap();
                assert_eq!(response, direct.step(d.episode_id, d.step, 0).unwrap());
                assert_eq!(s.diagnostic_state_hash(), direct.diagnostic_state_hash());
                consumed += 1;
            }
            assert!(
                consumed >= 2,
                "must exercise a freshly captured successor token"
            );
        }
    }
    #[test]
    fn v4_search_consume_reports_failed_sample_instead_of_value() {
        let (mut state, hunter, _, _) =
            crate::rl_session::avenging_hunter_undercity_arena_choose_targets_state_v1(false);
        crate::rl_session::shuffle_trigger_source_into_library_v1(&mut state, hunter, PlayerId::P0);
        for name in ["Forest", "Island", "Mountain"] {
            crate::policy_observation_v6::tests::put(&mut state, PlayerId::P0, name, Zone::Library);
        }
        let s = FastActorSessionV1::from_v3_fixture_state(state);
        let definition = s.state.objects.get(hunter).card_def;
        let mut witnessed = false;
        for seed in 1..=32 {
            // Reproduce the archived old-sampler defect only as fault injection.
            let mut sample = s
                .search_clone_v4_inner(8172, |state, actor| {
                    crate::kernel_native_search_opponent_v1::redeterminize_hidden_zones_v1(
                        state, actor, seed,
                    )
                    .unwrap();
                })
                .unwrap();
            if sample.state.objects.get(hunter).card_def == definition {
                continue;
            }
            let d = decision(&sample);
            let token = sample.kernel_search_action_token_v4(d).unwrap();
            assert_eq!(
                sample.kernel_search_consume_v4(d, token, 0),
                Err(Error::StepFailed)
            );
            assert!(
                matches!(sample.current_response(), FastActorResponseV1::Terminal(t)
                if t.terminal_classification == TerminalClassificationV1::Halted)
            );
            witnessed = true;
            break;
        }
        assert!(witnessed);
    }
}
