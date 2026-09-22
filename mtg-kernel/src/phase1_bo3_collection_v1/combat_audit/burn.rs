//! One explicitly selected natural root, sharing only the confirmed-step sink.
use super::*;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Bo3BurnAuditOptionsV1 {
    pub actor: PlayerSeatV1,
    pub game_index: u8,
    pub decision_index: u64,
    #[serde(default)]
    pub response_tree: bool,
    #[serde(default)]
    pub hand_response_tree: bool,
    #[serde(default)]
    pub execute_certificate: bool,
    #[serde(default)]
    pub search_leaf_diagnostic: bool,
    #[serde(default)]
    pub search_core_diagnostic: bool,
}
impl Bo3BurnAuditOptionsV1 {
    pub fn from_json_v1(text: &str) -> Result<Self, String> {
        ensure(text.len() <= 4096, "burn options exceed 4 KiB")?;
        crate::rl::parse_strict_json_value(text).map_err(|e| e.to_string())?;
        let value: Self = serde_json::from_str(text).map_err(|e| e.to_string())?;
        ensure((1..=3).contains(&value.game_index), "burn game index outside BO3")?;
        ensure(!(value.response_tree && value.hand_response_tree), "select only one burn tree")?;
        ensure(!value.execute_certificate || value.response_tree || value.hand_response_tree,"certificate execution requires tree diagnostic")?;
        Ok(value)
    }
}
#[derive(Debug, Serialize)]
pub struct Bo3BurnAuditResultV1 {
    pub schema: &'static str,
    pub collection: Bo3CollectionResultV1,
    pub burn_audit: Value,
}
pub fn collect_bo3_with_burn_audit_v1(
    config: Bo3CollectionConfigV1,
    packages: [CompleteAgentPackageV1; 2],
    options: Bo3BurnAuditOptionsV1,
) -> Result<Bo3BurnAuditResultV1, String> {
    ensure(cfg!(feature = "experimental-burn-net8-packed-cuda-v1"), "burn diagnostic feature unavailable")?;
    ensure((1..=3).contains(&options.game_index), "burn game index outside BO3")?;
    ensure(!(options.response_tree && options.hand_response_tree), "select only one burn tree")?;
    ensure(!options.execute_certificate || options.response_tree || options.hand_response_tree,"certificate execution requires tree diagnostic")?;
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
    Ok(Bo3BurnAuditResultV1 { schema: "mtg-kernel-bo3-burn-audit/v1", collection, burn_audit: audit })
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
        let audit = if options.execute_certificate { input.diagnostic_public_certificate_execution_v1(options.hand_response_tree)? }
            else if options.hand_response_tree { input.diagnostic_public_hand_burn_tree_v1()? }
            else if options.response_tree { input.diagnostic_public_burn_tree_v1()? }
            else { input.diagnostic_terminal_targets_v1()? };
        // A rejected root is retained as an abstention, never silently skipped.
        if audit["status"] == "audited" {
            let visible = serde_json::to_value(&record.visible).map_err(|e| e.to_string())?;
            ensure(visible["observation"] == audit["visible"] && visible["ordered_actions"] == audit["actions"], "burn root differs from actual record")?;
            let outcomes = audit["outcomes"].as_array().ok_or("burn outcomes missing")?;
            ensure(outcomes.len() == d.legal_action_count as usize, "burn action count differs")?;
            self.transitions = outcomes.iter().try_fold(0u64, |sum, o| {
                if options.response_tree || options.hand_response_tree {
                    let n=o["transitions"].as_u64().ok_or("burn tree transitions missing")?;
                    ensure(n<=8192,"burn tree per-action bound exceeded")?;
                    return Ok::<_,String>(sum+n);
                }
                let line = o["line"].as_array().ok_or("burn line missing")?;
                ensure(line.len() <= 16, "burn branch exceeds oracle bound")?;
                Ok::<_, String>(sum + line.len() as u64)
            })?;
            ensure(self.transitions <= if options.response_tree || options.hand_response_tree {32*8192} else {512}, "burn total bound exceeded")?;
        }
        let status = audit["status"].as_str().ok_or("burn status missing")?;
        *self.counts.entry(status.into()).or_default() += 1;
        let tensor = crate::native_flat_tensorizer_v3::NativeFlatDecisionTensorV3 {
            common: policy.last_scored_training_tensor_v4()?.common.clone(),
        };
        let mut row = json!({"game_index":game_index, "record":record,
            "tensor_bits":crate::phase1_bo3_learning_v1::Bo3CapturedTensorBitsV1::from_tensor(&tensor),
            "logits_bits":scores.logits.iter().map(|x|x.to_bits()).collect::<Vec<_>>(),
            "value_bits":scores.value.to_bits(),"audit":audit});
        if options.search_leaf_diagnostic {
            row["search_leaf"]=input.diagnostic_v4_search_leaf_v1(policy,scores)?;
        }
        if options.search_core_diagnostic { row["search_core"]=input.diagnostic_v4_search_core_v1(policy)?; }
        self.bytes = serde_json::to_vec(&row).map_err(|e| e.to_string())?.len() as u64;
        ensure(self.bytes <= 4 * 1024 * 1024, "burn root exceeds byte bound")?;
        self.prepared_roots = 1;
        Ok(Some(row))
    }
}
