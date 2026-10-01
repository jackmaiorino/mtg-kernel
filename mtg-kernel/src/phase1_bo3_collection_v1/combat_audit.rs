//! Optional coordinator-only diagnostic. Never a model input or learning capture.
use super::*;
use serde_json::{json, Value};
use std::collections::BTreeMap;
mod burn;
pub use burn::{collect_bo3_with_burn_audit_v1, Bo3BurnAuditOptionsV1};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Bo3CombatAuditOptionsV1 {
    pub actor: PlayerSeatV1,
    pub depth: u32,
    pub nodes_per_action: u32,
    pub max_roots: u32,
    pub max_transitions: u64,
    pub max_json_bytes: u64,
}

#[cfg(all(test, feature = "experimental-burn-net8-packed-cuda-v1"))]
mod tests {
    use super::*;
    use crate::expanded_deck_training_v1::stack_features::terminal_tactics::public_combat::tests::position;
    use crate::phase1_bo3_collection_v1::tests::{config, fixtures_v4};

    fn options(actor: PlayerSeatV1) -> Bo3CombatAuditOptionsV1 {
        Bo3CombatAuditOptionsV1 {
            actor,
            depth: 32,
            nodes_per_action: 128,
            max_roots: 2048,
            max_transitions: 2_000_000,
            max_json_bytes: 64 * 1024 * 1024,
        }
    }

    #[test]
    fn bo3_public_combat_record_requires_real_step_confirmation_and_preserves_scored_input() {
        for actor in 0..2 {
            for commit in [false, true] {
                let mut session = position(actor, false, false, false);
                let FastActorResponseV1::Decision(d) = session.current_response() else {
                    panic!("combat root missing")
                };
                let (mut policies, packages) = fixtures_v4([PlayDrawChoiceV1::Play; 2]);
                let hashes = packages.each_ref().map(|p| p.package_sha256_v1().unwrap());
                let mut sink = CombatAuditSink::new(options(d.acting_player)).unwrap();
                let mut game = Bo3TrainingGameV1 {
                    game_index: 1,
                    start: None,
                    decisions: vec![],
                    terminal: None,
                };
                let mut budget = RecordBudget {
                    count: 0,
                    bytes: 0,
                    max_count: 1000,
                    max_bytes: MAX_RECORD_BYTES,
                };
                let mut recorder = RecordingPolicy {
                    policies: &mut policies,
                    hashes: &hashes,
                    game: &mut game,
                    budget: &mut budget,
                    pending: None,
                    recording_cap: false,
                    rejected_selections: 0,
                    capture: None,
                    combat: Some(&mut sink),
                    continuation: None,
                };
                recorder.reset_for_game_v1([7, 11]).unwrap();
                let selected = recorder
                    .select_action_v1(PairedBo1PolicyInputV1::new(&session, d))
                    .unwrap();
                let pending = recorder.pending.as_ref().unwrap();
                let audit = pending.combat.as_ref().expect("positive combat audit");
                assert_eq!(
                    audit["record"],
                    serde_json::to_value(&pending.record).unwrap()
                );
                let tensor = crate::native_flat_tensorizer_v3::NativeFlatDecisionTensorV3 {
                    common: recorder.policies[actor as usize]
                        .last_scored_training_tensor_v4()
                        .unwrap()
                        .common
                        .clone(),
                };
                assert_eq!(
                    audit["tensor_bits"],
                    serde_json::to_value(
                        crate::phase1_bo3_learning_v1::Bo3CapturedTensorBitsV1::from_tensor(
                            &tensor
                        )
                    )
                    .unwrap()
                );
                assert_eq!(recorder.combat.as_ref().unwrap().roots.len(), 0);
                if commit {
                    session.step(d.episode_id, d.step, selected).unwrap();
                    let next = match session.current_response() {
                        FastActorResponseV1::Decision(next) => next.step,
                        FastActorResponseV1::Terminal(t) => t.policy_step_count,
                    };
                    recorder.commit_pending(next).unwrap();
                    assert_eq!(recorder.game.decisions.len(), 1);
                } else {
                    let error = session
                        .step(d.episode_id, d.step + 1, selected)
                        .unwrap_err();
                    let mut diagnostic = Bo3CollectionGameDiagnosticsV1 {
                        game_index: 1,
                        environment_seed: None,
                        observed_terminal: None,
                        discarded_pending_selections: 0,
                        error: None,
                    };
                    assert!(recorder
                        .finish_game(Err(error.to_string()), &mut diagnostic)
                        .is_err());
                    assert_eq!(diagnostic.discarded_pending_selections, 1);
                }
                drop(recorder);
                let receipt = sink.finish(false);
                assert_eq!(receipt["prepared_roots"], 1);
                assert_eq!(receipt["committed_roots"], u64::from(commit));
                assert_eq!(receipt["complete"], false);
            }
        }
    }

    #[test]
    fn bo3_public_combat_enabled_and_exhausted_audits_preserve_complete_match_bytes() {
        let cfg = config("combat-audit-natural-no-combat");
        let (mut policies, packages) = fixtures_v4([PlayDrawChoiceV1::Play; 2]);
        let original =
            collect_loaded_inner(&cfg, packages.each_ref(), &mut policies, [None, None], None)
                .unwrap();
        assert!(matches!(
            original.trajectory.ending,
            Bo3TrajectoryEndingV1::Complete { .. }
        ));
        for exhaust in [false, true] {
            let mut opts = options(PlayerSeatV1::P0);
            if exhaust {
                opts.max_transitions = 1;
            }
            let mut sink = CombatAuditSink::new(opts).unwrap();
            let observed = collect_loaded_observed(
                &cfg,
                packages.each_ref(),
                &mut policies,
                [None, None],
                None,
                Some(&mut sink),
            )
            .unwrap();
            assert_eq!(
                serde_json::to_vec(&original).unwrap(),
                serde_json::to_vec(&observed).unwrap()
            );
            let receipt = sink.finish(true);
            assert_eq!(receipt["complete"], !exhaust);
            assert_eq!(receipt["budget_exhausted"], exhaust);
        }
    }

    #[test]
    fn bo3_public_combat_options_reject_duplicate_unknown_and_unbounded_fields() {
        let good = serde_json::to_string(&options(PlayerSeatV1::P0)).unwrap();
        assert!(Bo3CombatAuditOptionsV1::from_json_v1(&good).is_ok());
        for bad in [
            good.replacen("{", "{\"depth\":32,", 1),
            good.replacen("{", "{\"train\":true,", 1),
            good.replace("\"depth\":32", "\"depth\":65"),
            good.replace("\"nodes_per_action\":128", "\"nodes_per_action\":0"),
        ] {
            assert!(Bo3CombatAuditOptionsV1::from_json_v1(&bad).is_err());
        }
    }
}
impl Bo3CombatAuditOptionsV1 {
    pub fn from_json_v1(text: &str) -> Result<Self, String> {
        ensure(text.len() <= 4096, "combat options exceed 4 KiB")?;
        crate::rl::parse_strict_json_value(text).map_err(|e| e.to_string())?;
        let value: Self = serde_json::from_str(text).map_err(|e| e.to_string())?;
        value.validate()?;
        Ok(value)
    }
    fn validate(&self) -> Result<(), String> {
        ensure(
            (1..=64).contains(&self.depth)
                && (1..=2048).contains(&self.nodes_per_action)
                && (1..=2048).contains(&self.max_roots)
                && (1..=2_000_000).contains(&self.max_transitions)
                && (1..=64 * 1024 * 1024).contains(&self.max_json_bytes),
            "combat audit budgets outside bounds",
        )
    }
}

#[derive(Debug, Serialize)]
pub struct Bo3CombatAuditResultV1 {
    pub schema: &'static str,
    pub collection: Bo3CollectionResultV1,
    pub combat_audit: Value,
}

/// Separate result envelope preserves every old collection/training schema.
pub fn collect_bo3_with_combat_audit_v1(
    config: Bo3CollectionConfigV1,
    packages: [CompleteAgentPackageV1; 2],
    options: Bo3CombatAuditOptionsV1,
) -> Result<Bo3CombatAuditResultV1, String> {
    ensure(
        cfg!(feature = "experimental-burn-net8-packed-cuda-v1"),
        "combat diagnostic feature unavailable",
    )?;
    let mut sink = CombatAuditSink::new(options)?;
    let collection = collect_public_inner(config, packages, None, Some(&mut sink))?;
    let match_complete = matches!(
        &collection.collected.trajectory.ending,
        Bo3TrajectoryEndingV1::Complete { .. }
    );
    Ok(Bo3CombatAuditResultV1 {
        schema: "mtg-kernel-bo3-combat-audit/v1",
        collection,
        combat_audit: sink.finish(match_complete),
    })
}

pub(super) struct CombatAuditSink {
    burn: Option<Bo3BurnAuditOptionsV1>,
    options: Bo3CombatAuditOptionsV1,
    counts: BTreeMap<String, u64>,
    roots: Vec<Value>,
    prepared_roots: u32,
    transitions: u64,
    bytes: u64,
    exhausted: bool,
}
impl CombatAuditSink {
    pub(super) fn new(options: Bo3CombatAuditOptionsV1) -> Result<Self, String> {
        options.validate()?;
        Ok(Self {
            burn: None,
            options,
            counts: BTreeMap::new(),
            roots: Vec::new(),
            prepared_roots: 0,
            transitions: 0,
            bytes: 0,
            exhausted: false,
        })
    }
    pub(super) fn commit(&mut self, row: Value) {
        self.roots.push(row);
    }
    pub(super) fn finish(self, match_complete: bool) -> Value {
        let committed = self.roots.len();
        json!({"options":self.options,"complete":match_complete && !self.exhausted && committed==self.prepared_roots as usize,
            "match_complete":match_complete,"budget_exhausted":self.exhausted,"counts":self.counts,
            "prepared_roots":self.prepared_roots,"committed_roots":committed,"transitions":self.transitions,
            "root_json_bytes":self.bytes,"roots":self.roots,
            "non_claim":"Offline public combat diagnostic; no model update, action intervention or validated training labels. Incomplete audit is not a completed census."})
    }

    #[cfg(not(feature = "experimental-burn-net8-packed-cuda-v1"))]
    pub(super) fn prepare(
        &mut self,
        _: &PairedBo1PolicyInputV1<'_>,
        _: &Bo3DecisionRecordV1,
        _: u8,
        _: &FrozenPlayPolicyV1,
        _: &crate::sideboard_play_policy_v1::FrozenPlayDecisionScoresV1,
    ) -> Result<Option<Value>, String> {
        Err("combat diagnostic feature unavailable".into())
    }

    #[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
    pub(super) fn prepare(
        &mut self,
        input: &PairedBo1PolicyInputV1<'_>,
        record: &Bo3DecisionRecordV1,
        game_index: u8,
        policy: &FrozenPlayPolicyV1,
        scores: &crate::sideboard_play_policy_v1::FrozenPlayDecisionScoresV1,
    ) -> Result<Option<Value>, String> {
        if self.burn.is_some() {
            return self.prepare_burn(input, record, game_index, policy, scores);
        }
        let d = input.decision();
        if d.acting_player != self.options.actor {
            return Ok(None);
        }
        ensure(
            policy.feature_generation_v1() == PlayPolicyGenerationV1::V4,
            "combat capture requires actual V4 scorer",
        )?;
        let reserve = u64::from(d.legal_action_count) * u64::from(self.options.nodes_per_action);
        if self.exhausted
            || self.prepared_roots >= self.options.max_roots
            || self.transitions.saturating_add(reserve) > self.options.max_transitions
        {
            self.exhausted = true;
            *self.counts.entry("budget_exhausted".into()).or_default() += 1;
            return Ok(None);
        }
        let audit =
            input.diagnostic_public_combat_v1(self.options.depth, self.options.nodes_per_action)?;
        let status = audit["status"]
            .as_str()
            .ok_or("combat audit lacks status")?;
        *self.counts.entry(status.into()).or_default() += 1;
        if status != "audited" {
            return Ok(None);
        }
        let visible = serde_json::to_value(&record.visible).map_err(|e| e.to_string())?;
        ensure(
            visible["observation"] == audit["visible"]
                && visible["ordered_actions"] == audit["actions"],
            "combat root differs from the actual bound gameplay record",
        )?;
        let outcomes = audit["outcomes"]
            .as_array()
            .ok_or("combat audit lacks outcomes")?;
        ensure(
            outcomes.len() == d.legal_action_count as usize,
            "combat action count differs",
        )?;
        let transitions = outcomes.iter().try_fold(0u64, |sum, o| {
            o["transitions"]
                .as_u64()
                .and_then(|n| sum.checked_add(n))
                .ok_or("combat transition count invalid")
        })?;
        ensure(transitions <= reserve, "combat transition budget exceeded")?;
        self.transitions += transitions;
        let tensor = crate::native_flat_tensorizer_v3::NativeFlatDecisionTensorV3 {
            common: policy.last_scored_training_tensor_v4()?.common.clone(),
        };
        let tensor_bits =
            crate::phase1_bo3_learning_v1::Bo3CapturedTensorBitsV1::from_tensor(&tensor);
        let row = json!({"game_index":game_index,"step":d.step,"physical_decision_id":d.physical_decision_id,
            "substep_index":d.substep_index,"substep_count":d.substep_count,"record":record,
            "feature_generation":"v4","tensor_bits":tensor_bits,
            "logits_bits":scores.logits.iter().map(|x|x.to_bits()).collect::<Vec<_>>(),"value_bits":scores.value.to_bits(),"audit":audit});
        let size = serde_json::to_vec(&row).map_err(|e| e.to_string())?.len() as u64;
        if self.bytes.saturating_add(size) > self.options.max_json_bytes {
            self.exhausted = true;
            *self
                .counts
                .entry("root_bytes_exhausted".into())
                .or_default() += 1;
            return Ok(None);
        }
        self.bytes += size;
        self.prepared_roots += 1;
        Ok(Some(row))
    }
}
