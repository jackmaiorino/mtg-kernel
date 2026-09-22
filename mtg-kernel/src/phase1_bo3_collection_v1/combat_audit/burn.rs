//! One explicitly selected natural root, sharing only the confirmed-step sink.
use super::*;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Bo3BurnAuditOptionsV1 {
    pub actor: PlayerSeatV1,
    pub game_index: u8,
    pub decision_index: u64,
}
impl Bo3BurnAuditOptionsV1 {
    pub fn from_json_v1(text: &str) -> Result<Self, String> {
        ensure(text.len() <= 4096, "burn options exceed 4 KiB")?;
        crate::rl::parse_strict_json_value(text).map_err(|e| e.to_string())?;
        let value: Self = serde_json::from_str(text).map_err(|e| e.to_string())?;
        ensure(value.game_index < 3, "burn game index outside BO3")?;
        Ok(value)
    }
}
pub fn collect_bo3_with_burn_audit_v1(
    config: Bo3CollectionConfigV1,
    packages: [CompleteAgentPackageV1; 2],
    options: Bo3BurnAuditOptionsV1,
) -> Result<Value, String> {
    ensure(cfg!(feature = "experimental-burn-net8-packed-cuda-v1"), "burn diagnostic feature unavailable")?;
    ensure(options.game_index < 3, "burn game index outside BO3")?;
    let mut sink = CombatAuditSink::new(Bo3CombatAuditOptionsV1 {
        actor: options.actor, depth: 16, nodes_per_action: 16,
        max_roots: 1, max_transitions: 512, max_json_bytes: 4 * 1024 * 1024,
    })?;
    sink.burn = Some(options.clone());
    let collection = collect_public_inner(config, packages, None, Some(&mut sink))?;
    let complete = matches!(&collection.collected.trajectory.ending, Bo3TrajectoryEndingV1::Complete { .. });
    let mut audit = sink.finish(complete);
    audit["options"] = serde_json::to_value(options).map_err(|e| e.to_string())?;
    audit["complete"] = json!(complete && audit["committed_roots"] == 1 && audit["prepared_roots"] == 1);
    audit["non_claim"] = json!("Single selected public burn diagnostic; unresolved branches remain unknown. No training or strength estimate.");
    Ok(json!({"schema":"mtg-kernel-bo3-burn-audit/v1", "collection":collection, "burn_audit":audit}))
}
impl CombatAuditSink {
    #[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
    pub(super) fn prepare_burn(
        &mut self, input: &PairedBo1PolicyInputV1<'_>, record: &Bo3DecisionRecordV1,
        game_index: u8, policy: &FrozenPlayPolicyV1,
        scores: &crate::sideboard_play_policy_v1::FrozenPlayDecisionScoresV1,
    ) -> Result<Option<Value>, String> {
        let options = self.burn.as_ref().ok_or("missing burn options")?;
        if game_index != options.game_index || record.decision_index != options.decision_index {
            return Ok(None);
        }
        let d = input.decision();
        ensure(d.acting_player == options.actor, "burn selected actor differs")?;
        ensure(self.prepared_roots == 0, "duplicate burn root")?;
        ensure(policy.feature_generation_v1() == PlayPolicyGenerationV1::V4, "burn capture requires V4 scorer")?;
        let audit = input.diagnostic_terminal_targets_v1()?;
        // A rejected root is retained as an abstention, never silently skipped.
        if audit["status"] == "audited" {
            let visible = serde_json::to_value(&record.visible).map_err(|e| e.to_string())?;
            ensure(visible["observation"] == audit["visible"] && visible["ordered_actions"] == audit["actions"], "burn root differs from actual record")?;
            let outcomes = audit["outcomes"].as_array().ok_or("burn outcomes missing")?;
            ensure(outcomes.len() == d.legal_action_count as usize, "burn action count differs")?;
            self.transitions = outcomes.iter().try_fold(0u64, |sum, o| {
                let line = o["line"].as_array().ok_or("burn line missing")?;
                ensure(line.len() <= 16, "burn branch exceeds oracle bound")?;
                Ok::<_, String>(sum + line.len() as u64)
            })?;
            ensure(self.transitions <= 512, "burn total bound exceeded")?;
        }
        let status = audit["status"].as_str().ok_or("burn status missing")?;
        *self.counts.entry(status.into()).or_default() += 1;
        let tensor = crate::native_flat_tensorizer_v3::NativeFlatDecisionTensorV3 {
            common: policy.last_scored_training_tensor_v4()?.common.clone(),
        };
        let row = json!({"game_index":game_index, "record":record,
            "tensor_bits":crate::phase1_bo3_learning_v1::Bo3CapturedTensorBitsV1::from_tensor(&tensor),
            "logits_bits":scores.logits.iter().map(|x|x.to_bits()).collect::<Vec<_>>(),
            "value_bits":scores.value.to_bits(),"audit":audit});
        self.bytes = serde_json::to_vec(&row).map_err(|e| e.to_string())?.len() as u64;
        ensure(self.bytes <= 4 * 1024 * 1024, "burn root exceeds byte bound")?;
        self.prepared_roots = 1;
        Ok(Some(row))
    }
}
