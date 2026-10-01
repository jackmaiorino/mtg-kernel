//! V4-only search leaf scoring with fresh local scratch. No playing route.
use super::*;
use crate::model_guided_search_core_v1::{
    ModelGuidedSearchCoreErrorV1, ModelGuidedSearchLeafEvaluatorV1, ModelGuidedSearchLeafForwardV1,
    ModelGuidedSearchLeafSiteV1,
};

pub(crate) struct V4SearchLeafEvaluatorV1<'a> {
    policy: &'a FrozenPlayPolicyV1,
}
impl<'a> V4SearchLeafEvaluatorV1<'a> {
    pub(crate) fn new(policy: &'a FrozenPlayPolicyV1) -> Result<Self, String> {
        require(
            policy.feature_generation_v1() == PlayPolicyGenerationV1::V4,
            "search leaf requires explicit V4 policy",
        )?;
        crate::deterministic_math_v1::ensure_thread_mxcsr_normalized_v1()
            .map_err(|e| format!("V4 search leaf floating-point state: {e:?}"))?;
        crate::deterministic_math_v1::verify_pinned_mxcsr_state_v1()
            .map_err(|e| format!("V4 search leaf floating-point verification: {e:?}"))?;
        Ok(Self { policy })
    }
    pub(crate) fn tensor_digest(
        &self,
        session: &FastActorSessionV1,
        count: u32,
    ) -> Result<[u8; 32], String> {
        use sha2::{Digest, Sha256};
        let t = self.tensor(session, count)?;
        let bits = crate::phase1_bo3_learning_v1::Bo3CapturedTensorBitsV1::from_tensor(
            &NativeFlatDecisionTensorV3 { common: t.common },
        );
        let bytes = serde_json::to_vec(&bits).map_err(|e| e.to_string())?;
        let mut h = Sha256::new();
        h.update(b"mtg-kernel/v4-search-tensor-witness/v1\0");
        h.update(bytes);
        Ok(h.finalize().into())
    }
    fn tensor(
        &self,
        session: &FastActorSessionV1,
        legal_count: u32,
    ) -> Result<NativeFlatDecisionTensorV4, String> {
        let FastActorResponseV1::Decision(decision) = session.current_response() else {
            return Err("search leaf requires live decision".into());
        };
        require(
            decision.legal_action_count == legal_count,
            "search leaf legal count differs",
        )?;
        let mut encoder = FlatDecisionEncoderV4::default();
        let mut owned = OwnedScoringV1::default();
        let encoded = session
            .encode_current_flat_scoring_decision_owned_v4(
                decision,
                &mut encoder,
                &mut owned.buffers(),
            )
            .map_err(|e| format!("V4 search leaf encoding: {e:?}"))?;
        owned.globals = encoded.globals;
        let mut tensor = NativeFlatDecisionTensorV4::default();
        NativeFlatTensorizerV4::default()
            .fill(
                FlatScoringDecisionViewV4::new(owned.view(), &encoded.extensions),
                &mut tensor,
            )
            .map_err(|e| format!("V4 search leaf tensorization: {e:?}"))?;
        Ok(tensor)
    }
}
impl ModelGuidedSearchLeafEvaluatorV1 for V4SearchLeafEvaluatorV1<'_> {
    fn evaluate_leaf_v1(
        &self,
        session: &FastActorSessionV1,
        _leaf_key: [u8; 32],
        legal_action_count: u32,
        _site: ModelGuidedSearchLeafSiteV1,
    ) -> Result<ModelGuidedSearchLeafForwardV1, ModelGuidedSearchCoreErrorV1> {
        if !matches!(session.current_response(), FastActorResponseV1::Decision(_)) {
            return Err(ModelGuidedSearchCoreErrorV1::NoLiveDecisionToEncode);
        }
        let tensor = self
            .tensor(session, legal_action_count)
            .map_err(ModelGuidedSearchCoreErrorV1::Tensorize)?;
        let output = self
            .policy
            .model
            .forward_search_feature_transfer_v4(encoded_decision_view_v4(&tensor))?;
        if output.logits.len() != legal_action_count as usize
            || output.logits.is_empty()
            || !output.value.is_finite()
            || output.logits.iter().any(|x| !x.is_finite())
        {
            return Err(ModelGuidedSearchCoreErrorV1::EvaluatorContract);
        }
        Ok(ModelGuidedSearchLeafForwardV1 {
            legal_action_weights: crate::deterministic_math_v1::softmax_legal_action_weights_v1(
                &output.logits,
            ),
            v_raw: output.value,
        })
    }
}
/// Opt-in coordinator report on a sampled, live root. Does not select an action.
#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
pub(crate) fn diagnostic_report(
    policy: &FrozenPlayPolicyV1,
    session: &FastActorSessionV1,
    scores: &FrozenPlayDecisionScoresV1,
) -> Result<serde_json::Value, String> {
    use serde_json::json;
    // An observer must not repair the collector's arithmetic environment.
    // Search-owned threads may normalize in the constructor; this hook may not.
    crate::deterministic_math_v1::verify_pinned_mxcsr_state_v1().map_err(|e| {
        format!("leaf diagnostic requires already pinned floating-point state: {e:?}")
    })?;
    let FastActorResponseV1::Decision(d) = session.current_response() else {
        return Err("diagnostic requires live root".into());
    };
    let before = session.diagnostic_state_hash();
    let rng = policy.seat_rng;
    let retained = policy.last_scored_training_tensor_v4()?.clone();
    let e = V4SearchLeafEvaluatorV1::new(policy)?;
    let tensor = e.tensor(session, d.legal_action_count)?;
    let capture = |t: &NativeFlatDecisionTensorV4| {
        crate::phase1_bo3_learning_v1::Bo3CapturedTensorBitsV1::from_tensor(
            &NativeFlatDecisionTensorV3 {
                common: t.common.clone(),
            },
        )
    };
    require(
        capture(&tensor) == capture(&retained),
        "diagnostic tensor differs from ordinary scoring",
    )?;
    let ordinary = policy
        .model
        .forward_feature_transfer_v4(encoded_decision_view_v4(&tensor))
        .map_err(|e| format!("{e:?}"))?;
    let raw = policy
        .model
        .forward_search_feature_transfer_v4(encoded_decision_view_v4(&tensor))
        .map_err(|e| format!("{e:?}"))?;
    let logits_bits = |v: &[f32]| v.iter().map(|x| x.to_bits()).collect::<Vec<_>>();
    require(
        logits_bits(&ordinary.logits) == logits_bits(&scores.logits)
            && ordinary.value.to_bits() == scores.value.to_bits(),
        "diagnostic ordinary output differs",
    )?;
    require(
        raw.value.is_finite()
            && (-1.0..=1.0).contains(&raw.value)
            && raw.logits.len() == ordinary.logits.len()
            && !raw.logits.is_empty()
            && raw.logits.iter().all(|x| x.is_finite()),
        "diagnostic raw search output invalid",
    )?;
    let mut variants = Vec::new();
    for (name, library, rng_change) in [
        ("own_library", Some(d.acting_player as usize), false),
        (
            "opponent_library",
            Some(1 - d.acting_player as usize),
            false,
        ),
        ("randomness", None, true),
    ] {
        let changed = session.diagnostic_certificate_perturbed_clone_v1(library, rng_change)?;
        let t = e.tensor(&changed, d.legal_action_count)?;
        require(
            capture(&t) == capture(&tensor),
            "hidden perturbation changed leaf tensor",
        )?;
        let out = policy
            .model
            .forward_search_feature_transfer_v4(encoded_decision_view_v4(&t))
            .map_err(|e| format!("{e:?}"))?;
        require(
            logits_bits(&out.logits) == logits_bits(&raw.logits)
                && out.value.to_bits() == raw.value.to_bits(),
            "hidden perturbation changed leaf output",
        )?;
        variants.push(name);
    }
    let argmax = |xs: &[f32]| {
        xs.iter()
            .enumerate()
            .fold(0, |best, (i, x)| if *x > xs[best] { i } else { best })
    };
    require(
        before == session.diagnostic_state_hash(),
        "diagnostic changed original session",
    )?;
    require(
        rng == policy.seat_rng
            && capture(&retained) == capture(policy.last_scored_training_tensor_v4()?),
        "diagnostic changed policy scratch",
    )?;
    Ok(
        json!({"schema":"v4-search-leaf-diagnostic/v1","tensor_bits":capture(&tensor),
        "ordinary_logits_bits":logits_bits(&ordinary.logits),"ordinary_value_bits":ordinary.value.to_bits(),
        "search_logits_bits":logits_bits(&raw.logits),"search_value_bits":raw.value.to_bits(),
        "max_logit_absolute_delta":raw.logits.iter().zip(&ordinary.logits).map(|(a,b)|(f64::from(*a)-f64::from(*b)).abs()).fold(0.0_f64,f64::max),
        "value_absolute_delta":(f64::from(raw.value)-f64::from(ordinary.value)).abs(),
        "argmax_agreement":argmax(&raw.logits)==argmax(&ordinary.logits),"value_domain":[-1,1],
        "invariant_variants":variants,"original_unchanged":true,"policy_unchanged":true}),
    )
}
/// A Report-only observation of an already scored ordinary decision.
#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
pub(crate) fn report_search_observation(
    policy: &FrozenPlayPolicyV1,
    session: &FastActorSessionV1,
    limits: crate::model_guided_search_core_v4::Limits,
) -> Result<crate::model_guided_search_core_v4::Outcome, crate::model_guided_search_core_v4::Error>
{
    report_search_observation_mode(
        policy,
        session,
        limits,
        crate::rl_session::V4SearchSampleMode::Legacy,
    )
}
#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
pub(crate) fn report_search_observation_library_v2(
    policy: &FrozenPlayPolicyV1,
    session: &FastActorSessionV1,
    limits: crate::model_guided_search_core_v4::Limits,
) -> Result<crate::model_guided_search_core_v4::Outcome, crate::model_guided_search_core_v4::Error>
{
    report_search_observation_mode(
        policy,
        session,
        limits,
        crate::rl_session::V4SearchSampleMode::LibraryChoiceV2,
    )
}
#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
pub(crate) fn report_search_observation_future_v3(
    policy: &FrozenPlayPolicyV1,
    session: &FastActorSessionV1,
    limits: crate::model_guided_search_core_v4::Limits,
) -> Result<crate::model_guided_search_core_v4::Outcome, crate::model_guided_search_core_v4::Error>
{
    report_search_observation_mode(
        policy,
        session,
        limits,
        crate::rl_session::V4SearchSampleMode::FutureChanceV3,
    )
}
#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
fn report_search_observation_mode(
    policy: &FrozenPlayPolicyV1,
    session: &FastActorSessionV1,
    limits: crate::model_guided_search_core_v4::Limits,
    mode: crate::rl_session::V4SearchSampleMode,
) -> Result<crate::model_guided_search_core_v4::Outcome, crate::model_guided_search_core_v4::Error>
{
    use crate::model_guided_search_core_v4::{
        search_with_sample_mode, BackupMode, Error, InteriorBonus, RootAllocation,
    };
    crate::deterministic_math_v1::verify_pinned_mxcsr_state_v1()
        .map_err(|e| Error::ObserverEnvironment(format!("{e:?}")))?;
    let before = session.diagnostic_state_hash();
    let rng = policy.seat_rng;
    let capture = || {
        policy
            .last_scored_training_tensor_v4()
            .map(|t| {
                crate::phase1_bo3_learning_v1::Bo3CapturedTensorBitsV1::from_tensor(
                    &NativeFlatDecisionTensorV3 {
                        common: t.common.clone(),
                    },
                )
            })
            .map_err(Error::Evaluator)
    };
    let retained = capture()?;
    let evaluator = V4SearchLeafEvaluatorV1::new(policy).map_err(Error::Evaluator)?;
    let mut witness = |s: &FastActorSessionV1, n| evaluator.tensor_digest(s, n);
    let result = search_with_sample_mode(
        session,
        limits,
        &evaluator,
        RootAllocation::RoundRobin,
        InteriorBonus::PriorFree,
        BackupMode::Report,
        mode,
        Some(&mut witness),
    );
    // Check failures too. An invalid simulation is not permission to mutate
    // the ordinary policy or to return a partially trusted observation.
    let session_changed = before != session.diagnostic_state_hash();
    let policy_rng_changed = rng != policy.seat_rng;
    let tensor_changed = retained != capture()?;
    if session_changed || policy_rng_changed || tensor_changed {
        return Err(Error::ObserverInvariant {
            session_changed,
            policy_rng_changed,
            tensor_changed,
        });
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
    fn v4_search_bound_evaluation_matches_core_without_mutating_live_state() {
        use crate::model_guided_search_core_v4::{
            search_with_policies, Error, InteriorBonus, Limits, RootAllocation,
        };
        use crate::paired_bo1_harness_v1::PairedBo1PolicyInputV1;
        for actor in [crate::ids::PlayerId::P0, crate::ids::PlayerId::P1] {
            let session = public_session(actor, 20);
            let FastActorResponseV1::Decision(decision) = session.current_response() else {
                panic!("fixture missing")
            };
            let mut policy = FrozenPlayPolicyV1::training_fixture_v4();
            policy.reset_sampling_v1([11, 22]);
            let before = session.diagnostic_state_hash();
            let rng = policy.seat_rng;
            let capture = |p: &FrozenPlayPolicyV1| {
                crate::phase1_bo3_learning_v1::Bo3CapturedTensorBitsV1::from_tensor(
                    &NativeFlatDecisionTensorV3 {
                        common: p.last_scored_training_tensor_v4().unwrap().common.clone(),
                    },
                )
            };
            let retained = capture(&policy);
            let limits = Limits {
                simulations: 32,
                transitions: 128,
                depth: 4,
                seed: 29,
            };
            let evaluator = V4SearchLeafEvaluatorV1::new(&policy).unwrap();
            let expected = search_with_policies(
                &session,
                limits,
                &evaluator,
                RootAllocation::RoundRobin,
                InteriorBonus::PriorFree,
                None,
            )
            .unwrap();
            let actual = PairedBo1PolicyInputV1::new(&session, decision)
                .evaluation_search_v4(
                    &policy,
                    limits,
                    RootAllocation::RoundRobin,
                    InteriorBonus::PriorFree,
                )
                .unwrap();
            assert_eq!(actual, expected);
            let mut stale = decision;
            stale.step += 1;
            assert_eq!(
                PairedBo1PolicyInputV1::new(&session, stale).evaluation_search_v4(
                    &policy,
                    limits,
                    RootAllocation::RoundRobin,
                    InteriorBonus::PriorFree
                ),
                Err(Error::InvalidAdapterBinding)
            );
            assert_eq!(session.diagnostic_state_hash(), before);
            assert_eq!(policy.seat_rng, rng);
            assert_eq!(capture(&policy), retained);
        }
    }

    #[test]
    #[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
    fn v4_search_bound_report_matches_off_and_preserves_scored_policy() {
        use crate::model_guided_search_core_v4::{Error, InteriorBonus, Limits, RootAllocation};
        use crate::paired_bo1_harness_v1::PairedBo1PolicyInputV1;
        for actor in [crate::ids::PlayerId::P0, crate::ids::PlayerId::P1] {
            let session = public_session(actor, 20);
            let FastActorResponseV1::Decision(d) = session.current_response() else {
                panic!("fixture missing")
            };
            let mut policy = FrozenPlayPolicyV1::training_fixture_v4();
            policy.reset_sampling_v1([11, 22]);
            let input = PairedBo1PolicyInputV1::new(&session, d);
            let _ = policy.select_paired_with_scores_v1(&input).unwrap();
            let capture = |p: &FrozenPlayPolicyV1| {
                crate::phase1_bo3_learning_v1::Bo3CapturedTensorBitsV1::from_tensor(
                    &NativeFlatDecisionTensorV3 {
                        common: p.last_scored_training_tensor_v4().unwrap().common.clone(),
                    },
                )
            };
            let before = session.diagnostic_state_hash();
            let rng = policy.seat_rng;
            let retained = capture(&policy);
            let limits = Limits {
                simulations: 32,
                transitions: 128,
                depth: 4,
                seed: 29,
            };
            let off = input
                .evaluation_search_v4(
                    &policy,
                    limits,
                    RootAllocation::RoundRobin,
                    InteriorBonus::PriorFree,
                )
                .unwrap();
            assert_eq!(
                input.report_search_v4(
                    &policy,
                    Limits {
                        simulations: 1,
                        transitions: 8,
                        ..limits
                    }
                ),
                Err(Error::InvalidBudget)
            );
            assert_eq!(session.diagnostic_state_hash(), before);
            assert_eq!(policy.seat_rng, rng);
            assert_eq!(capture(&policy), retained);
            let mut report = input.report_search_v4(&policy, limits).unwrap();
            assert!(report.estimator.take().is_some());
            assert_eq!(report, off);
            let mut stale = d;
            stale.step += 1;
            assert_eq!(
                PairedBo1PolicyInputV1::new(&session, stale).report_search_v4(&policy, limits),
                Err(Error::InvalidAdapterBinding)
            );
            assert_eq!(session.diagnostic_state_hash(), before);
            assert_eq!(policy.seat_rng, rng);
            assert_eq!(capture(&policy), retained);
        }
    }

    #[test]
    #[cfg(all(
        target_arch = "x86_64",
        feature = "experimental-burn-net8-packed-cuda-v1"
    ))]
    fn v4_search_bound_report_rejects_dirty_float_state_without_repair() {
        std::thread::spawn(|| {
            use crate::deterministic_math_v1::*;
            use crate::model_guided_search_core_v4::{Error, Limits};
            let session = public_session(crate::ids::PlayerId::P0, 20);
            let FastActorResponseV1::Decision(d) = session.current_response() else {
                panic!("fixture missing")
            };
            let policy = FrozenPlayPolicyV1::training_fixture_v4();
            let original = read_mxcsr_v1();
            let dirty = original | (1 << 15);
            write_mxcsr_v1(dirty);
            let result = crate::paired_bo1_harness_v1::PairedBo1PolicyInputV1::new(&session, d)
                .report_search_v4(
                    &policy,
                    Limits {
                        simulations: 32,
                        transitions: 128,
                        depth: 4,
                        seed: 29,
                    },
                );
            let after = read_mxcsr_v1();
            write_mxcsr_v1(original);
            assert!(matches!(result, Err(Error::ObserverEnvironment(_))));
            assert_eq!(after, dirty);
        })
        .join()
        .unwrap();
    }
    #[test]
    #[cfg(all(
        target_arch = "x86_64",
        feature = "experimental-burn-net8-packed-cuda-v1"
    ))]
    fn v4_search_leaf_diagnostic_rejects_dirty_thread_without_repair() {
        std::thread::spawn(|| {
            use crate::deterministic_math_v1::*;
            let s = public_session(crate::ids::PlayerId::P0, 20);
            let mut policy = FrozenPlayPolicyV1::training_fixture_v4();
            let scores = policy.score_fast_session_v1(&s).unwrap();
            let original = read_mxcsr_v1();
            let dirty = original | (1 << 15);
            write_mxcsr_v1(dirty);
            let result = diagnostic_report(&policy, &s, &scores);
            let after = read_mxcsr_v1();
            write_mxcsr_v1(original);
            assert!(result.is_err());
            assert_eq!(after, dirty);
        })
        .join()
        .unwrap();
    }
    #[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
    fn public_session(actor: crate::ids::PlayerId, life: i32) -> FastActorSessionV1 {
        use crate::policy_observation_v6::tests::{put, ready_state};
        use crate::state::Zone;
        let mut state = ready_state();
        state.active_player = actor;
        state.priority_player = actor;
        state.players[actor.index()].life = life;
        put(&mut state, actor, "Lightning Bolt", Zone::Hand);
        state.players[actor.index()].mana_pool[crate::mana::ManaColor::R.pool_index()] = 3;
        put(&mut state, actor.opponent(), "Gut Shot", Zone::Hand);
        for owner in [actor, actor.opponent()] {
            for name in ["Forest", "Mountain", "Island"] {
                put(&mut state, owner, name, Zone::Library);
            }
        }
        FastActorSessionV1::from_v3_fixture_state(state)
    }
    #[test]
    #[cfg(target_arch = "x86_64")]
    fn v4_search_leaf_constructor_normalizes_then_rejects_dirty_thread() {
        std::thread::spawn(|| {
            use crate::deterministic_math_v1::*;
            let original = read_mxcsr_v1();
            let policy = FrozenPlayPolicyV1::training_fixture_v4();
            write_mxcsr_v1(original | (1 << 15) | (1 << 6));
            assert!(V4SearchLeafEvaluatorV1::new(&policy).is_ok());
            assert!(verify_pinned_mxcsr_state_v1().is_ok());
            write_mxcsr_v1(read_mxcsr_v1() | (1 << 15));
            let rejected = V4SearchLeafEvaluatorV1::new(&policy).is_err();
            write_mxcsr_v1(original);
            assert!(rejected);
        })
        .join()
        .unwrap();
    }
    #[test]
    fn v4_search_leaf_terminal_has_named_error() {
        let mut state = crate::policy_observation_v6::tests::ready_state();
        state.players[0].has_lost = true;
        let session = FastActorSessionV1::from_v3_fixture_state(state);
        assert!(matches!(
            session.current_response(),
            FastActorResponseV1::Terminal(_)
        ));
        let policy = FrozenPlayPolicyV1::training_fixture_v4();
        assert!(matches!(
            V4SearchLeafEvaluatorV1::new(&policy)
                .unwrap()
                .evaluate_leaf_v1(&session, [0; 32], 1, ModelGuidedSearchLeafSiteV1::RootPrior),
            Err(ModelGuidedSearchCoreErrorV1::NoLiveDecisionToEncode)
        ));
    }
    #[test]
    fn v4_search_leaf_rejects_cross_schema_forward_both_directions() {
        let policy = FrozenPlayPolicyV1::training_fixture_v4();
        let e = V4SearchLeafEvaluatorV1::new(&policy).unwrap();
        let s = session();
        let FastActorResponseV1::Decision(d) = s.current_response() else {
            panic!("fixture")
        };
        let v4 = e.tensor(&s, d.legal_action_count).unwrap();
        assert!(policy
            .model
            .forward_search_deterministic_v1(encoded_decision_view_v4(&v4))
            .is_err());
        assert!(policy
            .model
            .forward_search_feature_transfer_v4(encoded_decision_view_v1(&v4.common))
            .is_err());
    }
    #[test]
    #[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
    fn v4_search_leaf_hidden_invariance_both_seats_and_visible_positive_control() {
        for actor in [crate::ids::PlayerId::P0, crate::ids::PlayerId::P1] {
            let session = public_session(actor, 20);
            let before = session.diagnostic_state_hash();
            let FastActorResponseV1::Decision(d) = session.current_response() else {
                panic!("fixture")
            };
            assert_eq!(d.acting_player as usize, actor.index());
            let mut policy = FrozenPlayPolicyV1::training_fixture_v4();
            policy.score_fast_session_v1(&session).unwrap();
            let ordinary = policy.last_scored_training_tensor_v4().unwrap().clone();
            let e = V4SearchLeafEvaluatorV1::new(&policy).unwrap();
            let tensor = e.tensor(&session, d.legal_action_count).unwrap();
            assert_eq!(tensor_bits(&tensor), tensor_bits(&ordinary));
            let forward = |s: &FastActorSessionV1| {
                bits(
                    e.evaluate_leaf_v1(
                        s,
                        [0; 32],
                        d.legal_action_count,
                        ModelGuidedSearchLeafSiteV1::RootPrior,
                    )
                    .unwrap(),
                )
            };
            let output = forward(&session);
            let mut variants = Vec::new();
            for (library, rng) in [(Some(0), false), (Some(1), false), (None, true)] {
                variants.push(
                    session
                        .diagnostic_certificate_perturbed_clone_v1(library, rng)
                        .unwrap(),
                );
            }
            // Existing redetermination admits only the old V2 action contract.
            // Preserve its rejection; a V4-compatible route needs separate design.
            assert!(matches!(session.kernel_search_redeterminized_clone_v1(71823),
                Err(crate::kernel_native_search_opponent_v1::KernelNativeSearchErrorV1::UnsupportedFlatActionContract)));
            for changed in &variants {
                assert_eq!(
                    tensor_bits(&e.tensor(changed, d.legal_action_count).unwrap()),
                    tensor_bits(&tensor)
                );
                assert_eq!(forward(changed), output);
            }
            let visible = public_session(actor, 19);
            assert_ne!(
                tensor_bits(&e.tensor(&visible, d.legal_action_count).unwrap()),
                tensor_bits(&tensor)
            );
            assert_eq!(session.diagnostic_state_hash(), before);
        }
    }
    fn tensor_bits(
        t: &NativeFlatDecisionTensorV4,
    ) -> crate::phase1_bo3_learning_v1::Bo3CapturedTensorBitsV1 {
        crate::phase1_bo3_learning_v1::Bo3CapturedTensorBitsV1::from_tensor(
            &NativeFlatDecisionTensorV3 {
                common: t.common.clone(),
            },
        )
    }
    #[test]
    fn v4_search_leaf_extension_roots_match_warmed_and_fresh_tensors() {
        use crate::policy_observation_v6::tests::*;
        let mut ward = ward_multi_targeter_state().0;
        for player in [crate::ids::PlayerId::P0, crate::ids::PlayerId::P1] {
            put(
                &mut ward,
                player,
                "Lightning Bolt",
                crate::state::Zone::Hand,
            );
            ward.players[player.index()].mana_pool[crate::mana::ManaColor::R.pool_index()] += 1;
        }
        let mut payment = ward.clone();
        reach_ward_payment(&mut payment);
        let cases = [
            ("cost", escape_prefix_state().0),
            ("library", forest_search_state(false, "Lightning Bolt")),
            ("queued_ward", ward),
            ("ward", payment),
            ("historical", initiative_transfer_state()),
            (
                "chosen",
                crate::native_flat_tensorizer_v3::monstrous_emergence_cost_fixture_v3(true).0,
            ),
        ];
        let mut warmed = FrozenPlayPolicyV1::training_fixture_v4();
        for (name, state) in cases {
            let s = FastActorSessionV1::from_v3_fixture_state(state);
            let FastActorResponseV1::Decision(d) = s.current_response() else {
                panic!("{name} fixture missing")
            };
            let mut owned = OwnedScoringV1::default();
            let encoded = s
                .encode_current_flat_scoring_decision_owned_v4(
                    d,
                    &mut FlatDecisionEncoderV4::default(),
                    &mut owned.buffers(),
                )
                .unwrap();
            let ext = &encoded.extensions;
            match name {
                "cost" => assert!(ext.pending_cast_object_cost.is_some()),
                "library" => assert!(ext.decision_local_library.is_some()),
                "queued_ward" => assert!(!ext.queued_ward_payments.is_empty()),
                "ward" => assert!(ext.pending_ward_payment.is_some()),
                "historical" => assert!(!ext.historical_public_sources.is_empty()),
                "chosen" => assert!(ext.pending_chosen_creature_cost.is_some()),
                _ => unreachable!(),
            }
            warmed.score_fast_session_v1(&s).unwrap();
            let mut fresh = FrozenPlayPolicyV1::training_fixture_v4();
            fresh.score_fast_session_v1(&s).unwrap();
            let actual = V4SearchLeafEvaluatorV1::new(&warmed)
                .unwrap()
                .tensor(&s, d.legal_action_count)
                .unwrap();
            assert_eq!(
                tensor_bits(&actual),
                tensor_bits(warmed.last_scored_training_tensor_v4().unwrap()),
                "{name}"
            );
            assert_eq!(
                tensor_bits(&actual),
                tensor_bits(fresh.last_scored_training_tensor_v4().unwrap()),
                "{name}"
            );
        }
    }
    fn session() -> FastActorSessionV1 {
        let (mut state, hunter, _, _) =
            crate::rl_session::avenging_hunter_undercity_arena_choose_targets_state_v1(false);
        crate::rl_session::shuffle_trigger_source_into_library_v1(
            &mut state,
            hunter,
            crate::ids::PlayerId::P0,
        );
        FastActorSessionV1::from_v3_fixture_state(state)
    }
    fn bits(x: ModelGuidedSearchLeafForwardV1) -> (Vec<u32>, u32) {
        (
            x.legal_action_weights.iter().map(|x| x.to_bits()).collect(),
            x.v_raw.to_bits(),
        )
    }
    #[test]
    fn v4_search_leaf_matches_current_tensor_and_preserves_policy_state() {
        let session = session();
        let before = session.diagnostic_state_hash();
        let FastActorResponseV1::Decision(d) = session.current_response() else {
            panic!("fixture missing decision")
        };
        let mut policy = FrozenPlayPolicyV1::training_fixture_v4();
        policy.reset_sampling_v1([19, 37]);
        policy.score_fast_session_v1(&session).unwrap();
        let expected = policy.last_scored_training_tensor_v4().unwrap().clone();
        let rng = policy.seat_rng;
        let evaluator = V4SearchLeafEvaluatorV1::new(&policy).unwrap();
        let observed = evaluator.tensor(&session, d.legal_action_count).unwrap();
        let captured = |t: &NativeFlatDecisionTensorV4| {
            crate::phase1_bo3_learning_v1::Bo3CapturedTensorBitsV1::from_tensor(
                &NativeFlatDecisionTensorV3 {
                    common: t.common.clone(),
                },
            )
        };
        assert_eq!(captured(&expected), captured(&observed));
        let first = bits(
            evaluator
                .evaluate_leaf_v1(
                    &session,
                    [0; 32],
                    d.legal_action_count,
                    ModelGuidedSearchLeafSiteV1::RootPrior,
                )
                .unwrap(),
        );
        let repeated = bits(
            evaluator
                .evaluate_leaf_v1(
                    &session,
                    [0; 32],
                    d.legal_action_count,
                    ModelGuidedSearchLeafSiteV1::RootPrior,
                )
                .unwrap(),
        );
        assert_eq!(first, repeated);
        assert_eq!(policy.seat_rng, rng);
        assert_eq!(session.diagnostic_state_hash(), before);
        assert_eq!(expected, *policy.last_scored_training_tensor_v4().unwrap());
        assert!(evaluator
            .tensor(&session, d.legal_action_count + 1)
            .is_err());
    }
    #[test]
    fn v4_search_leaf_rejects_v3_and_does_not_depend_on_policy_scratch() {
        let v3 = FrozenPlayPolicyV1::training_fixture_v3();
        assert!(V4SearchLeafEvaluatorV1::new(&v3).is_err());
        let session = session();
        let FastActorResponseV1::Decision(d) = session.current_response() else {
            panic!("fixture missing")
        };
        let mut policy = FrozenPlayPolicyV1::training_fixture_v4();
        policy.reset_sampling_v1([11, 22]);
        let first = bits(
            V4SearchLeafEvaluatorV1::new(&policy)
                .unwrap()
                .evaluate_leaf_v1(
                    &session,
                    [1; 32],
                    d.legal_action_count,
                    ModelGuidedSearchLeafSiteV1::RootPrior,
                )
                .unwrap(),
        );
        let mut other = session.clone();
        other.step(d.episode_id, d.step, 0).unwrap();
        if matches!(other.current_response(), FastActorResponseV1::Decision(_)) {
            policy.score_fast_session_v1(&other).unwrap();
        }
        let second = bits(
            V4SearchLeafEvaluatorV1::new(&policy)
                .unwrap()
                .evaluate_leaf_v1(
                    &session,
                    [1; 32],
                    d.legal_action_count,
                    ModelGuidedSearchLeafSiteV1::RootPrior,
                )
                .unwrap(),
        );
        assert_eq!(first, second);
    }
}

/// Report-only fixed work on a consumed archive root. No action override.
#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
pub(crate) fn core_diagnostic_report(
    policy: &FrozenPlayPolicyV1,
    session: &FastActorSessionV1,
) -> Result<serde_json::Value, String> {
    use crate::model_guided_search_core_v4::{search_inner, Limits};
    use serde_json::json;
    crate::deterministic_math_v1::verify_pinned_mxcsr_state_v1().map_err(|e| format!("{e:?}"))?;
    let FastActorResponseV1::Decision(d) = session.current_response() else {
        return Err("core diagnostic requires live root".into());
    };
    require(
        d.legal_action_count <= 64,
        "fixed diagnostic supports at most64rootactions",
    )?;
    let before = session.diagnostic_state_hash();
    let rng = policy.seat_rng;
    let capture = |t: &NativeFlatDecisionTensorV4| {
        crate::phase1_bo3_learning_v1::Bo3CapturedTensorBitsV1::from_tensor(
            &NativeFlatDecisionTensorV3 {
                common: t.common.clone(),
            },
        )
    };
    let retained = capture(policy.last_scored_training_tensor_v4()?);
    let e = V4SearchLeafEvaluatorV1::new(policy)?;
    let limits = Limits {
        simulations: 64,
        transitions: 512,
        depth: 8,
        seed: 20260922,
    };
    let mut witness = |s: &FastActorSessionV1, n| e.tensor_digest(s, n);
    let first = search_inner(session, limits, &e, Some(&mut witness));
    let second = search_inner(session, limits, &e, Some(&mut witness));
    require(first == second, "core diagnostic repeat differs")?;
    require(
        before == session.diagnostic_state_hash(),
        "core diagnostic mutated original",
    )?;
    require(
        rng == policy.seat_rng && retained == capture(policy.last_scored_training_tensor_v4()?),
        "core diagnostic mutated policy scratch",
    )?;
    let result = match first {
        Ok(o) => json!({"status":"available","outcome":o}),
        Err(err) => json!({"status":"unavailable","error":format!("{err:?}")}),
    };
    Ok(
        json!({"schema":"v4-search-core-diagnostic/v1","result":result,"limits":{"simulations":64,"transitions":512,"depth":8,"seed":20260922},
        "tensor_witness":true,"repeat_exact":true,"original_unchanged":true,"policy_unchanged":true,"non_claim":"Consumed-root correctness only; no action override or strength estimate."}),
    )
}

/// Follow one already-certified strategy path; opponent responses are passes.
#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
pub(crate) fn certificate_prior_report(
    policy: &FrozenPlayPolicyV1,
    session: &FastActorSessionV1,
    hand: bool,
) -> Result<serde_json::Value, String> {
    use crate::expanded_deck_training_v1::stack_features::terminal_tactics::{
        public_burn_tree, public_hand_burn_tree,
    };
    use crate::model_guided_search_prior_quantization_v1::{
        prior_expansion_order_v1, quantize_prior_v1,
    };
    use serde_json::json;
    crate::deterministic_math_v1::verify_pinned_mxcsr_state_v1().map_err(|e| format!("{e:?}"))?;
    let FastActorResponseV1::Decision(root) = session.current_response() else {
        return Err("certificate probe root not live".into());
    };
    let before = session.diagnostic_state_hash();
    let rng = policy.seat_rng;
    let capture = |t: &NativeFlatDecisionTensorV4| {
        crate::phase1_bo3_learning_v1::Bo3CapturedTensorBitsV1::from_tensor(
            &NativeFlatDecisionTensorV3 {
                common: t.common.clone(),
            },
        )
    };
    let retained = capture(policy.last_scored_training_tensor_v4()?);
    let audit = if hand {
        public_hand_burn_tree::audit(session, root)?
    } else {
        public_burn_tree::audit(session, root)?
    };
    let winning = audit["outcomes"]
        .as_array()
        .ok_or("certificate probe audit unavailable")?
        .iter()
        .find(|x| x["tree"][0].as_i64() == Some(1))
        .ok_or("certificate probe has no winning action")?;
    let root_action = winning["index"].as_u64().ok_or("root index missing")? as u32;
    let mut tree = &winning["tree"];
    let mut sample = session.clone();
    let e = V4SearchLeafEvaluatorV1::new(policy)?;
    let mut rows = Vec::new();
    let mut terminal = false;
    for depth in 0..=32u32 {
        let d = match sample.current_response() {
            FastActorResponseV1::Terminal(t) => {
                require(
                    tree[2].as_u64() == Some(2)
                        && tree[0].as_i64() == Some(1)
                        && t.terminal_classification
                            == crate::rl::TerminalClassificationV1::Natural
                        && t.winner == Some(root.acting_player),
                    "certificate path did not reach natural root win",
                )?;
                terminal = true;
                break;
            }
            FastActorResponseV1::Decision(d) => d,
        };
        require(depth < 32, "certificate path exceeds declared depth")?;
        let (_, actions) = crate::paired_bo1_harness_v1::PairedBo1PolicyInputV1::new(&sample, d)
            .diagnostic_visible_v4()?;
        let (action, next) = if depth == 0 {
            (root_action, tree)
        } else {
            let branches = tree[3].as_array().ok_or("certificate branches missing")?;
            require(
                branches.len() == actions.len()
                    && branches
                        .iter()
                        .enumerate()
                        .all(|(i, x)| x[0].as_u64() == Some(i as u64)),
                "certificate menu shape differs",
            )?;
            let own = d.acting_player == root.acting_player;
            require(
                tree[2].as_u64() == Some(if own { 7 } else { 8 }),
                "certificate actor reason differs",
            )?;
            let chosen = if own {
                branches.iter().find(|x| x[1][0].as_i64() == Some(1))
            } else {
                branches.iter().find(|x| {
                    x[0].as_u64().is_some_and(|i| {
                        matches!(
                            actions.get(i as usize),
                            Some(crate::rl::ActionSemanticV1::Pass { .. })
                        )
                    })
                })
            };
            let chosen = chosen.ok_or("certificate own winning branch or opponent pass missing")?;
            (
                chosen[0].as_u64().ok_or("certificate action missing")? as u32,
                &chosen[1],
            )
        };
        require(
            (action as usize) < actions.len(),
            "certificate index outside current menu",
        )?;
        let key = sample
            .kernel_search_visible_key_v4(32 - depth)
            .map_err(|e| format!("{e:?}"))?;
        let f = e
            .evaluate_leaf_v1(
                &sample,
                key,
                d.legal_action_count,
                ModelGuidedSearchLeafSiteV1::NewlyExpandedNode,
            )
            .map_err(|e| format!("{e:?}"))?;
        let priors = quantize_prior_v1(&f.legal_action_weights).map_err(|e| format!("{e:?}"))?;
        let rank = prior_expansion_order_v1(&priors)
            .iter()
            .position(|i| *i == action as usize)
            .ok_or("prior rank missing")?;
        rows.push(json!({"depth":depth,"actor":d.acting_player,"own":d.acting_player==root.acting_player,
            "selected":action,"selected_action":actions[action as usize],"prior_rank_zero_based":rank,
            "priors":priors,"raw_value":f.v_raw,"tensor_sha256":e.tensor_digest(&sample,d.legal_action_count)?.iter().map(|b|format!("{b:02x}")).collect::<String>()}));
        let token = sample
            .kernel_search_action_token_v4(d)
            .map_err(|e| format!("{e:?}"))?;
        sample
            .kernel_search_consume_v4(d, token, action)
            .map_err(|e| format!("{e:?}"))?;
        tree = next;
    }
    require(terminal, "certificate path missing terminal")?;
    require(
        before == session.diagnostic_state_hash()
            && rng == policy.seat_rng
            && retained == capture(policy.last_scored_training_tensor_v4()?),
        "certificate probe changed original or policy",
    )?;
    Ok(
        json!({"schema":"v4-certificate-prior-path/v1","root_action":root_action,"decision_depth":rows.len(),"rows":rows,
        "natural_root_win":true,"original_unchanged":true,"policy_unchanged":true,"scope":"One certified path with opponent passes; not worst-case depth or strength."}),
    )
}

#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
pub(crate) fn allocation_diagnostic_report(
    policy: &FrozenPlayPolicyV1,
    session: &FastActorSessionV1,
) -> Result<serde_json::Value, String> {
    use crate::model_guided_search_core_v4::{
        search_with_policies, InteriorBonus, Limits, RootAllocation,
    };
    use serde_json::json;
    crate::deterministic_math_v1::verify_pinned_mxcsr_state_v1().map_err(|e| format!("{e:?}"))?;
    let FastActorResponseV1::Decision(d) = session.current_response() else {
        return Err("allocation probe root not live".into());
    };
    require(
        [4, 8].contains(&d.legal_action_count),
        "allocation diagnostic requires the declared4/8actionroots",
    )?;
    let before = session.diagnostic_state_hash();
    let rng = policy.seat_rng;
    let capture = |t: &NativeFlatDecisionTensorV4| {
        crate::phase1_bo3_learning_v1::Bo3CapturedTensorBitsV1::from_tensor(
            &NativeFlatDecisionTensorV3 {
                common: t.common.clone(),
            },
        )
    };
    let retained = capture(policy.last_scored_training_tensor_v4()?);
    let e = V4SearchLeafEvaluatorV1::new(policy)?;
    let mut witness = |s: &FastActorSessionV1, n| e.tensor_digest(s, n);
    let simulations = 16 * d.legal_action_count;
    let transitions = 8 * simulations;
    let limits = Limits {
        simulations,
        transitions,
        depth: 8,
        seed: 20260922,
    };
    let mut arms = Vec::new();
    for (allocation, interior_bonus) in [
        (RootAllocation::Puct, InteriorBonus::PriorWeighted),
        (RootAllocation::RoundRobin, InteriorBonus::PriorWeighted),
        (RootAllocation::RoundRobin, InteriorBonus::PriorFree),
    ] {
        let first = search_with_policies(
            session,
            limits,
            &e,
            allocation,
            interior_bonus,
            Some(&mut witness),
        );
        let repeat = search_with_policies(
            session,
            limits,
            &e,
            allocation,
            interior_bonus,
            Some(&mut witness),
        );
        require(first == repeat, "allocation diagnostic repeat differs")?;
        arms.push(match first {Ok(outcome)=>json!({"allocation":allocation,"interior_bonus":interior_bonus,"status":"available","outcome":outcome}),
            Err(e)=>json!({"allocation":allocation,"interior_bonus":interior_bonus,"status":"unavailable","error":format!("{e:?}")})});
    }
    require(
        before == session.diagnostic_state_hash()
            && rng == policy.seat_rng
            && retained == capture(policy.last_scored_training_tensor_v4()?),
        "allocation diagnostic mutated original or policy",
    )?;
    Ok(
        json!({"schema":"v4-root-interior-allocation-diagnostic/v2","limits":{"simulations":simulations,"transitions":transitions,"depth":8,"seed":20260922},
        "arms":arms,"repeat_exact":true,"tensor_witness":true,"original_unchanged":true,"policy_unchanged":true,"non_claim":"Two consumed roots only; no playing override or strength estimate."}),
    )
}

#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
pub(crate) fn backup_diagnostic_report(
    policy: &FrozenPlayPolicyV1,
    session: &FastActorSessionV1,
) -> Result<serde_json::Value, String> {
    use crate::model_guided_search_core_v4::{
        search_with_backup, BackupMode, InteriorBonus, Limits, RootAllocation,
    };
    use serde_json::json;
    crate::deterministic_math_v1::verify_pinned_mxcsr_state_v1().map_err(|e| format!("{e:?}"))?;
    let FastActorResponseV1::Decision(d) = session.current_response() else {
        return Err("allocation probe root not live".into());
    };
    require(
        [4, 8].contains(&d.legal_action_count),
        "allocation diagnostic requires the declared4/8actionroots",
    )?;
    let before = session.diagnostic_state_hash();
    let rng = policy.seat_rng;
    let capture = |t: &NativeFlatDecisionTensorV4| {
        crate::phase1_bo3_learning_v1::Bo3CapturedTensorBitsV1::from_tensor(
            &NativeFlatDecisionTensorV3 {
                common: t.common.clone(),
            },
        )
    };
    let retained = capture(policy.last_scored_training_tensor_v4()?);
    let e = V4SearchLeafEvaluatorV1::new(policy)?;
    let mut witness = |s: &FastActorSessionV1, n| e.tensor_digest(s, n);
    let simulations = 16 * d.legal_action_count;
    let transitions = 8 * simulations;
    let limits = Limits {
        simulations,
        transitions,
        depth: 8,
        seed: 20260922,
    };
    let mut arms = Vec::new();
    for mode in [BackupMode::Off, BackupMode::Report, BackupMode::Blend] {
        let start = std::time::Instant::now();
        let first = search_with_backup(
            session,
            limits,
            &e,
            RootAllocation::RoundRobin,
            InteriorBonus::PriorFree,
            mode,
            Some(&mut witness),
        );
        let first_ns = start.elapsed().as_nanos();
        let start = std::time::Instant::now();
        let repeat = search_with_backup(
            session,
            limits,
            &e,
            RootAllocation::RoundRobin,
            InteriorBonus::PriorFree,
            mode,
            Some(&mut witness),
        );
        eprintln!(
            "V4_BACKUP_TIMING {}",
            json!({"mode":mode,"first_ns":first_ns,"repeat_ns":start.elapsed().as_nanos()})
        );
        require(first == repeat, "allocation diagnostic repeat differs")?;
        arms.push(match first {
            Ok(outcome) => json!({"mode":mode,"status":"available","outcome":outcome}),
            Err(e) => json!({"mode":mode,"status":"unavailable","error":format!("{e:?}")}),
        });
    }
    require(
        before == session.diagnostic_state_hash()
            && rng == policy.seat_rng
            && retained == capture(policy.last_scored_training_tensor_v4()?),
        "allocation diagnostic mutated original or policy",
    )?;
    Ok(
        json!({"schema":"v4-backup-diagnostic/v1","limits":{"simulations":simulations,"transitions":transitions,"depth":8,"seed":20260922},
        "arms":arms,"repeat_exact":true,"tensor_witness":true,"original_unchanged":true,"policy_unchanged":true,"non_claim":"Two consumed roots only; no playing override or strength estimate."}),
    )
}
