//! Explicit census samples with independent future randomness.
//! Existing registered census callers continue to use their V1 sampler.
use super::FastActorSessionV1;
use sha2::{Digest, Sha256};

pub const CENSUS_FUTURE_SAMPLER_IDENTITY_V2: &str =
    "census-pinned-hidden-identities-independent-future-shuffle/v2";

/// A disposable sampled session together with its distinct sampler identity.
/// This is neither a training receipt nor an upgrade of a frozen census run.
pub struct CensusFutureSampleV2 {
    session: FastActorSessionV1,
}

impl CensusFutureSampleV2 {
    pub fn sampler_identity(&self) -> &'static str {
        CENSUS_FUTURE_SAMPLER_IDENTITY_V2
    }

    pub fn session(&self) -> &FastActorSessionV1 {
        &self.session
    }

    pub fn into_session(self) -> FastActorSessionV1 {
        self.session
    }
}

impl FastActorSessionV1 {
    /// Samples once using V1's identities, known-card pins and source-conflict
    /// rejection, then replaces only the disposable clone's future RNG.
    /// `seed` must come from the caller's external rollout schedule. The
    /// derivation reads no real RNG, private state hash or environment seed.
    /// Physical-owner shuffle counters and randomness mode are preserved.
    pub fn census_redeterminized_clone_future_v2(
        &self,
        seed: u64,
    ) -> Result<CensusFutureSampleV2, String> {
        let mut session = self.census_redeterminized_clone_v1(seed)?;
        let mut digest = Sha256::new();
        digest.update(b"mtg-kernel/census/future-randomness/v2\0");
        digest.update(seed.to_be_bytes());
        let bytes = digest.finalize();
        let future = u64::from_be_bytes(bytes[..8].try_into().expect("SHA256 prefix"));
        session
            .state
            .resample_future_randomness_for_search_v3(future);
        Ok(CensusFutureSampleV2 { session })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::environment_randomization_v2::{GameEnvironmentRandomizationV2, PhysicalOwnerV2};
    use crate::ids::PlayerId;
    use crate::rl::ActionSemanticV1;
    use crate::rl_session::{search_library_fixture_v3, FastActorResponseV1};
    use crate::state::{GameState, Zone};

    fn with_rng(state: &GameState, environment: bool, seed: u64) -> GameState {
        let mut value = serde_json::to_value(state).unwrap();
        let fields = value.as_object_mut().unwrap();
        fields.remove("rng");
        fields.remove("environment_randomization_v2");
        if environment {
            let mut rng = GameEnvironmentRandomizationV2::new(seed);
            rng.set_live_shuffle_ordinal(PhysicalOwnerV2::P0, 4);
            rng.set_live_shuffle_ordinal(PhysicalOwnerV2::P1, 9);
            value["environment_randomization_v2"] = serde_json::to_value(rng).unwrap();
        } else {
            value["rng"] = serde_json::json!({"state":seed});
        }
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn independent_future_ignores_real_rng_preserves_public_root_and_replays() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            for environment in [false, true] {
                for form in 0..3 {
                    let names = if form == 1 {
                        vec!["Squadron Hawk", "Squadron Hawk", "Forest", "Island"]
                    } else {
                        vec!["Forest", "Forest", "Island", "Mountain"]
                    };
                    let state = search_library_fixture_v3(actor, form, &names, false, true, false);
                    let a = FastActorSessionV1::from_v3_fixture_state(with_rng(
                        &state,
                        environment,
                        111,
                    ));
                    let b = FastActorSessionV1::from_v3_fixture_state(with_rng(
                        &state,
                        environment,
                        999,
                    ));
                    let original = a.state.clone();
                    let FastActorResponseV1::Decision(d) = a.current_response() else {
                        panic!("root")
                    };
                    let x = a.census_redeterminized_clone_future_v2(81).unwrap();
                    let y = b.census_redeterminized_clone_future_v2(81).unwrap();
                    assert_eq!(x.sampler_identity(), CENSUS_FUTURE_SAMPLER_IDENTITY_V2);
                    assert_eq!(x.session.state, y.session.state);
                    assert_eq!(
                        a.diagnostic_current_decision_input_v4(d).unwrap(),
                        x.session.diagnostic_current_decision_input_v4(d).unwrap()
                    );
                    assert_eq!(
                        a.kernel_search_action_token_v4(d),
                        x.session.kernel_search_action_token_v4(d)
                    );
                    if environment {
                        let rng = x.session.state.environment_randomization_v2().unwrap();
                        assert_eq!(rng.next_live_shuffle_ordinal(PhysicalOwnerV2::P0), 4);
                        assert_eq!(rng.next_live_shuffle_ordinal(PhysicalOwnerV2::P1), 9);
                    } else {
                        assert!(x.session.state.legacy_rng().is_some());
                    }
                    let old = a.census_redeterminized_clone_v1(81).unwrap();
                    assert_eq!(old.state.legacy_rng(), a.state.legacy_rng());
                    assert_eq!(
                        old.state.environment_randomization_v2(),
                        a.state.environment_randomization_v2()
                    );
                    for index in 0..d.legal_action_count {
                        let mut left = x.session.clone();
                        let mut right = y.session.clone();
                        assert_eq!(
                            left.step(d.episode_id, d.step, index).unwrap(),
                            right.step(d.episode_id, d.step, index).unwrap()
                        );
                        assert_eq!(left.state, right.state);
                        assert!(left.state.engine.halted.is_none());
                    }
                    assert_eq!(a.state, original);
                }
            }
        }
    }

    #[test]
    fn declined_search_closing_shuffle_diversifies_known_card_position() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            for environment in [false, true] {
                let state = search_library_fixture_v3(
                    actor,
                    0,
                    &["Forest", "Forest", "Island", "Mountain"],
                    false,
                    true,
                    false,
                );
                let session =
                    FastActorSessionV1::from_v3_fixture_state(with_rng(&state, environment, 111));
                let known = session.state.players[actor.index()].library[0];
                let original = session.state.clone();
                let positions: std::collections::BTreeSet<_> = (1..=16)
                    .map(|seed| {
                        let mut sample = session
                            .census_redeterminized_clone_future_v2(seed)
                            .unwrap()
                            .into_session();
                        let FastActorResponseV1::Decision(d) = sample.current_response() else {
                            panic!("root")
                        };
                        let finish = sample
                            .current
                            .as_ref()
                            .unwrap()
                            .candidates
                            .iter()
                            .position(|c| {
                                matches!(c.semantic, ActionSemanticV1::FinishEffectSelection { .. })
                            })
                            .unwrap();
                        sample.step(d.episode_id, d.step, finish as u32).unwrap();
                        assert!(sample.state.engine.halted.is_none());
                        assert_eq!(sample.state.objects.get(known).zone, Zone::Library);
                        assert!(sample.state.known_library_cards(actor, actor).is_empty());
                        sample.state.players[actor.index()]
                            .library
                            .iter()
                            .position(|id| *id == known)
                            .unwrap()
                    })
                    .collect();
                assert!(
                    positions.len() > 1,
                    "closing shuffle retained the real known-card slot"
                );
                assert_eq!(session.state, original);
            }
        }
    }

    #[test]
    fn source_conflicts_keep_the_single_v1_rejection_without_retries() {
        let (mut state, source, _, _) =
            crate::rl_session::avenging_hunter_undercity_arena_choose_targets_state_v1(false);
        crate::rl_session::shuffle_trigger_source_into_library_v1(&mut state, source, PlayerId::P0);
        for name in ["Forest", "Island", "Mountain"] {
            crate::policy_observation_v6::tests::put(&mut state, PlayerId::P0, name, Zone::Library);
        }
        let session = FastActorSessionV1::from_v3_fixture_state(state);
        let original = session.state.clone();
        let mut rejected = 0;
        for seed in 1..=32 {
            match session.census_redeterminized_clone_v1(seed) {
                Err(error) => {
                    assert_eq!(
                        session.census_redeterminized_clone_future_v2(seed).err(),
                        Some(error)
                    );
                    rejected += 1;
                }
                Ok(_) => {
                    assert!(session.census_redeterminized_clone_future_v2(seed).is_ok());
                }
            }
        }
        assert!(rejected > 0);
        assert_eq!(session.state, original);
    }
}
