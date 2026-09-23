//! Evaluation-only playing route. The training/package search guards are unchanged.
use super::*;
use crate::model_guided_search_core_v4::{Error as SearchError, Limits};
use crate::phase1_agent_v1::{V4SearchInteriorBonusV1, V4SearchRootAllocationV1};
use crate::rl_session::RlSessionErrorCode;

const SCHEMA: &str = "mtg-kernel-v4-information-set-estimate-search/v3";
const ALGORITHM: &str = "v4-depth-keyed-estimate-library-independent-chance/v3";

pub(super) struct SearchPlayV3 {
    policy: FrozenPlayPolicyV1,
    descriptor: V4InformationSetSearchDescriptorV1,
    game_index: u32,
    rows: Vec<Value>,
    failure: Option<Value>,
    pub(super) timings: Vec<Value>,
}

impl SearchPlayV3 {
    pub(super) fn new(policy: FrozenPlayPolicyV1, descriptor: V4InformationSetSearchDescriptorV1) -> Result<Self, String> {
        let d = &descriptor;
        if policy.feature_generation_v1() != PlayPolicyGenerationV1::V4
            || d.schema != SCHEMA || d.algorithm != ALGORITHM
            || d.root_allocation != V4SearchRootAllocationV1::RoundRobin
            || d.interior_bonus != V4SearchInteriorBonusV1::PriorFree {
            return Err("unknown V4 future-chance evaluation contract".into());
        }
        // This is the single D3 candidate, not a tuning surface.
        if (d.simulations, d.transitions, d.depth, d.experiment_seed) != (128, 1024, 8, 20260922) {
            return Err("V4 future-chance evaluation requires the frozen D3 budget".into());
        }
        let model = policy.actual_model_identity_v1();
        if d.weights_sha256 != model.weights_sha256
            || d.model_parameter_sha256 != model.model_parameter_sha256
            || d.embedding_table_sha256 != model.embedding_table_sha256
            || d.feature_contract_digest != model.feature_contract_digest
            || d.feature_encoding_digest != model.feature_encoding_digest {
            return Err("V4 future-chance descriptor differs from loaded model/features".into());
        }
        Ok(Self { policy, descriptor, game_index: 0, rows: vec![], failure: None, timings: vec![] })
    }
    fn limits(&self) -> Limits {
        Limits { simulations: self.descriptor.simulations, transitions: self.descriptor.transitions,
            depth: self.descriptor.depth, seed: self.descriptor.experiment_seed }
    }
    pub(super) fn begin_match(&mut self) {
        self.game_index = 0;
        self.rows.clear();
        self.timings.clear();
        self.failure = None;
    }
    pub(super) fn records(&self) -> Value {
        json!({"schema":"v4-information-set-evaluation-decisions/v3",
            "descriptor":self.descriptor,"decisions":self.rows,"failure":self.failure})
    }
}

impl PairedBo1PolicyV1 for SearchPlayV3 {
    fn uses_observation_successor_v3(&self) -> bool { true }
    fn feature_generation_v1(&self) -> PlayPolicyGenerationV1 { PlayPolicyGenerationV1::V4 }
    fn reset_for_game_v1(&mut self, seeds: [u64; 2]) -> Result<(), RlSessionError> {
        self.policy.reset_for_game_v1(seeds)?;
        self.game_index += 1;
        Ok(())
    }
    fn select_action_v1(&mut self, input: PairedBo1PolicyInputV1<'_>) -> Result<u32, RlSessionError> {
        let decision = input.decision();
        let binding = json!({"episode_id":decision.episode_id,"step":decision.step,
            "environment_revision":decision.environment_revision,"physical_decision_id":decision.physical_decision_id,
            "substep_index":decision.substep_index,"substep_count":decision.substep_count,
            "actor":decision.acting_player,"kind":format!("{:?}",decision.decision_kind),
            "legal_action_count":decision.legal_action_count});
        let started = std::time::Instant::now();
        let result = (|| {
            let outcome = input.report_search_future_v3(&self.policy, self.limits())?;
            let estimate = outcome.estimator.as_ref().ok_or(SearchError::CorruptTree)?;
            let root = estimate.nodes.first().ok_or(SearchError::CorruptTree)?;
            let selected = estimate.selected_by_estimate;
            if selected >= decision.legal_action_count { return Err(SearchError::CorruptTree); }
            let root_key = outcome.tree.first().ok_or(SearchError::CorruptTree)?.key;
            let bytes = serde_json::to_vec(&outcome).map_err(|e| SearchError::Evaluator(e.to_string()))?;
            let row = json!({"game_index":self.game_index,"decision":binding,
                "root_key":root_key,"selected":selected,"selected_by_core":outcome.selected,
                "selected_by_mean":outcome.selected_by_mean,
                "root_estimates":root.edges.iter().map(|e|e.value).collect::<Vec<_>>(),
                "simulations":outcome.simulations,"transitions":outcome.transitions,
                "nodes":outcome.nodes,"headroom":outcome.headroom,"census":outcome.census,
                "root_visits":outcome.root_visits,"root_value_sums":outcome.root_value_sums,
                "root_priors":outcome.root_priors,"root_work":outcome.root_work,
                "outcome_sha256":hash(&bytes)});
            Ok((selected, row))
        })();
        self.timings.push(json!({"game_index":self.game_index,"actor":decision.acting_player,
            "step":decision.step,"elapsed_ns":u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX)}));
        match result {
            Ok((selected, row)) => { self.rows.push(row); Ok(selected) }
            Err(error) => {
                self.failure = Some(json!({"game_index":self.game_index,"decision":binding,"error":error}));
                Err(RlSessionError { code: RlSessionErrorCode::StaleEnvironmentBinding,
                    message: format!("V4 future-chance search failed: {error:?}") })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rl_session::{FastActorResponseV1, FastActorSessionV1};

    fn descriptor(policy: &FrozenPlayPolicyV1) -> V4InformationSetSearchDescriptorV1 {
        let m = policy.actual_model_identity_v1();
        V4InformationSetSearchDescriptorV1 { schema:SCHEMA.into(), algorithm:ALGORITHM.into(),
            root_allocation:V4SearchRootAllocationV1::RoundRobin,
            interior_bonus:V4SearchInteriorBonusV1::PriorFree,
            simulations:128, transitions:1024, depth:8, experiment_seed:20260922,
            weights_sha256:m.weights_sha256, model_parameter_sha256:m.model_parameter_sha256,
            embedding_table_sha256:m.embedding_table_sha256, feature_contract_digest:m.feature_contract_digest,
            feature_encoding_digest:m.feature_encoding_digest }
    }

    #[test]
    fn search_v3_rejects_old_identity_changed_budget_and_unknown_fields() {
        let p = FrozenPlayPolicyV1::training_fixture_v4();
        let d = descriptor(&p);
        for field in ["schema", "algorithm", "weights_sha256", "model_parameter_sha256",
            "embedding_table_sha256", "feature_contract_digest", "feature_encoding_digest",
            "simulations", "transitions", "depth", "experiment_seed", "root_allocation", "interior_bonus"] {
            let mut value = serde_json::to_value(&d).unwrap();
            value[field] = match field {
                "simulations" | "transitions" | "depth" | "experiment_seed" => json!(1),
                "root_allocation" => json!("puct"),
                "interior_bonus" => json!("prior_weighted"),
                _ => json!("wrong"),
            };
            assert!(SearchPlayV3::new(FrozenPlayPolicyV1::training_fixture_v4(),
                serde_json::from_value(value).unwrap()).is_err(), "accepted {field}");
        }
        let mut value = serde_json::to_value(&d).unwrap();
        value["fallback"] = json!(true);
        assert!(serde_json::from_value::<V4InformationSetSearchDescriptorV1>(value).is_err());
        assert!(SearchPlayV3::new(FrozenPlayPolicyV1::training_fixture_v3(), d.clone()).is_err());
        SearchPlayV3::new(p, d).unwrap();
    }

    fn public_session(actor: PlayerId) -> FastActorSessionV1 {
        use crate::policy_observation_v6::tests::{put, ready_state};
        use crate::state::Zone;
        let mut s = ready_state();
        s.active_player = actor; s.priority_player = actor;
        put(&mut s, actor, "Lightning Bolt", Zone::Hand);
        s.players[actor.index()].mana_pool[crate::mana::ManaColor::R.pool_index()] = 3;
        put(&mut s, actor.opponent(), "Gut Shot", Zone::Hand);
        for owner in [actor, actor.opponent()] {
            for name in ["Forest", "Mountain", "Island"] { put(&mut s, owner, name, Zone::Library); }
        }
        FastActorSessionV1::from_v3_fixture_state(s)
    }

    #[test]
    fn search_v3_plays_exact_estimate_both_seats_and_library_without_changing_sampler() {
        let library = FastActorSessionV1::from_v3_fixture_state(
            crate::policy_observation_v6::tests::forest_search_state(false, "Lightning Bolt"));
        for session in [public_session(PlayerId::P0), public_session(PlayerId::P1), library] {
            let FastActorResponseV1::Decision(d) = session.current_response() else { panic!("fixture"); };
            let policy = FrozenPlayPolicyV1::training_fixture_v4();
            let desc = descriptor(&policy);
            let mut wrapper = SearchPlayV3::new(policy, desc).unwrap();
            let mut ordinary = FrozenPlayPolicyV1::training_fixture_v4();
            let before = session.diagnostic_state_hash();
            let mut first = None;
            for _ in 0..2 {
                wrapper.begin_match();
                wrapper.reset_for_game_v1([123, 456]).unwrap();
                ordinary.reset_for_game_v1([123, 456]).unwrap();
                let input = PairedBo1PolicyInputV1::new(&session, d);
                let expected = input.report_search_future_v3(&wrapper.policy, wrapper.limits()).unwrap();
                let selected = wrapper.select_action_v1(input).unwrap();
                assert_eq!(selected, expected.estimator.unwrap().selected_by_estimate);
                assert_eq!(wrapper.rows.len(), 1);
                assert_eq!(wrapper.timings.len(), 1);
                assert!(wrapper.failure.is_none());
                assert_eq!(session.diagnostic_state_hash(), before);
                let records = wrapper.records();
                if let Some(ref first) = first { assert_eq!(&records, first); }
                first = Some(records);
                // A subsequent ordinary sample must see the same RNG and policy state.
                assert_eq!(wrapper.policy.select_action_v1(PairedBo1PolicyInputV1::new(&session, d)).unwrap(),
                    ordinary.select_action_v1(PairedBo1PolicyInputV1::new(&session, d)).unwrap());
            }
        }
    }

    #[test]
    fn search_v3_typed_failure_is_retained_without_fallback() {
        let session = public_session(PlayerId::P0);
        let FastActorResponseV1::Decision(mut d) = session.current_response() else { panic!("fixture"); };
        let policy = FrozenPlayPolicyV1::training_fixture_v4();
        let desc = descriptor(&policy);
        let mut wrapper = SearchPlayV3::new(policy, desc).unwrap();
        wrapper.reset_for_game_v1([1, 2]).unwrap();
        let before = session.diagnostic_state_hash();
        d.step += 1;
        assert!(wrapper.select_action_v1(PairedBo1PolicyInputV1::new(&session, d)).is_err());
        assert_eq!(wrapper.failure.as_ref().unwrap()["error"], json!("InvalidAdapterBinding"));
        assert!(wrapper.rows.is_empty());
        assert_eq!(wrapper.timings.len(), 1);
        assert_eq!(session.diagnostic_state_hash(), before);
    }
}
