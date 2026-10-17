//! Opt-in bounded acquisition, not a completed-game or action-quality result.
use super::*;
use crate::rl_session::{FastActorDecisionV1, FastActorResponseV1};

const SCHEMA: &str = "s4a-reused-prefix/v1";
const EXECUTOR_TOTAL: u64 = 2_048_000;
const MAX_ACQUISITION_SEARCH: u64 = 320_000;

fn fits(spent: u64, requested: u64, cap: u64) -> bool {
    requested > 0 && spent.checked_add(requested).is_some_and(|next| next <= cap)
}

fn later_reuse(
    width: u32,
    physical: u64,
    origin_physical: u64,
    step: u64,
    origin_step: u64,
) -> bool {
    width >= 2 && physical > origin_physical && step > origin_step
}

fn accounting(searches: &[Value], used: u64, cap: u64) -> Result<(), String> {
    let mut total = 0u64;
    for (index, search) in searches.iter().enumerate() {
        let requested = search["budget"]
            .as_u64()
            .ok_or("prefix search budget missing")?;
        let actual = search["transitions"]
            .as_u64()
            .ok_or("prefix search spending missing")?;
        if search["search"].as_u64() != Some(index as u64)
            || !fits(total, requested, cap)
            || actual > requested
        {
            return Err("prefix search admission/accounting differs".into());
        }
        total = total
            .checked_add(actual)
            .ok_or("prefix search accounting overflow")?;
    }
    if total != used || used > cap {
        return Err("prefix total search accounting differs".into());
    }
    Ok(())
}

#[derive(Default)]
pub(super) struct Acquisition {
    cap: u64,
    pub(super) applied_actions: usize,
    selected: Option<Value>,
    blocked: Option<Value>,
    admissions: Vec<Value>,
}

impl Acquisition {
    fn new(cap: u64) -> Self {
        Self {
            cap,
            ..Self::default()
        }
    }

    pub(super) fn admit(
        &mut self,
        session: &FastActorSessionV1,
        decision: FastActorDecisionV1,
        requested: u64,
        spent: u64,
        index: usize,
        reason: &str,
    ) -> Result<bool, String> {
        if spent > self.cap || requested == 0 {
            return Err("prefix acquisition search accounting invalid".into());
        }
        let admitted = fits(spent, requested, self.cap);
        let record = json!({"search":index,"step":decision.step,
            "physical_decision_id":decision.physical_decision_id,"reason":reason,
            "requested_budget":requested,"actual_spent":spent,
            "remaining":self.cap-spent,"admitted":admitted});
        if admitted {
            self.admissions.push(record);
        } else {
            let mut blocked = record;
            blocked["decision"] = super::super::bank::decision_identity(session)?;
            self.blocked = Some(blocked);
        }
        Ok(admitted)
    }

    pub(super) fn candidate(
        &self,
        session: &FastActorSessionV1,
        d: FastActorDecisionV1,
        action: u32,
        witness: &Value,
        searches: &[Value],
        action_index: usize,
    ) -> Result<Option<Value>, String> {
        let index = witness["search"]
            .as_u64()
            .and_then(|x| usize::try_from(x).ok())
            .ok_or("prefix witness search index missing")?;
        let origin = searches
            .get(index)
            .ok_or("prefix originating search missing")?;
        let origin_step = origin["root"]["step"]
            .as_u64()
            .ok_or("prefix search root step missing")?;
        let origin_physical = origin["root"]["physical_decision_id"]
            .as_u64()
            .ok_or("prefix search root physical decision missing")?;
        if !later_reuse(
            d.legal_action_count,
            d.physical_decision_id,
            origin_physical,
            d.step,
            origin_step,
        ) {
            return Ok(None);
        }
        let identity = super::super::bank::decision_identity(session)?;
        if witness["schema"] != super::super::reuse_witness::SCHEMA
            || witness["decision"] != identity
            || witness["applied_live_index"].as_u64() != Some(u64::from(action))
            || origin["step"].as_u64() != Some(origin_step)
            || witness["search_tree_hash"] != origin["tree"]["hash"]
            || action_index as u64 != d.step
        {
            return Err("prefix selected reused witness differs from live execution".into());
        }
        Ok(Some(json!({"action_index":action_index,"action":action,
            "originating_search":index,"originating_search_step":origin_step,
            "originating_search_physical_decision_id":origin_physical,
            "decision":identity,"witness":witness})))
    }

    pub(super) fn acquired(
        &mut self,
        mut target: Value,
        session: &FastActorSessionV1,
    ) -> Result<(), String> {
        if target["action_index"]
            .as_u64()
            .and_then(|x| x.checked_add(1))
            != Some(self.applied_actions as u64)
        {
            return Err("prefix target was not successfully applied at the prefix boundary".into());
        }
        target["post_action_identity"] = match decision(session) {
            Some(_) => super::super::bank::decision_identity(session)?,
            None => Value::Null,
        };
        target["post_action_terminal"] = terminal(session);
        target["post_action_diagnostic_state_hash"] =
            json!(format!("{:016x}", session.diagnostic_state_hash()));
        target["post_action_core_environment_hash"] = json!(format!(
            "{:016x}",
            session.privileged_core_environment_hash()
        ));
        target["post_action_policy_steps"] = json!(session.policy_step_count());
        target["post_action_physical_decisions"] = json!(session.physical_decision_count());
        self.selected = Some(target);
        Ok(())
    }

    #[allow(
        clippy::too_many_arguments,
        reason = "complete successful-prefix and partial-failure accounting"
    )]
    pub(super) fn finish(
        &self,
        session: &FastActorSessionV1,
        focal: PlayerId,
        mut actions: Vec<u32>,
        mut decisions: Vec<Value>,
        searches: &[Value],
        used: u64,
        charged: u64,
        calls: u64,
        error: Option<String>,
        wall: f64,
    ) -> Value {
        let unapplied_action = actions.get(self.applied_actions).copied();
        actions.truncate(self.applied_actions);
        decisions.retain(|row| {
            row["step"]
                .as_u64()
                .is_some_and(|step| step < self.applied_actions as u64)
        });
        let error = error.or_else(|| accounting(searches, used, self.cap).err());
        let end = outcome(session, focal);
        let error = error.or_else(|| {
            if self.selected.is_none()
                && self.blocked.is_none()
                && !matches!(end, Some("win" | "loss" | "draw"))
            {
                Some("prefix guide ended without an acquired root or natural terminal".into())
            } else {
                None
            }
        });
        let status = if error.is_some() {
            "failure"
        } else if self.selected.is_some() {
            "acquired"
        } else if self.blocked.is_some() {
            "acquisition_cap"
        } else {
            "natural_terminal_without_reuse"
        };
        json!({"status":status,"complete":error.is_none(),"selected":self.selected,
            "blocked_search":self.blocked,"error":error,"search_admissions":self.admissions,
            "game":{"actions":actions,"decisions":decisions,"searches":searches,
                "key_mode":"repaired-v2","transitions_used":used,
                "real_transitions":self.applied_actions,"charged_real_transitions":charged,
                "live_inference_calls":calls,"unapplied_action":unapplied_action,
                "end":end.filter(|x| *x != "nonnatural"),"terminal":terminal(session),
                "final_diagnostic_state_hash":format!("{:016x}",session.diagnostic_state_hash()),
                "final_core_environment_hash":format!("{:016x}",session.privileged_core_environment_hash()),
                "final_policy_steps":session.policy_step_count(),
                "final_physical_decisions":session.physical_decision_count()},"wall":wall})
    }
}

fn terminal(session: &FastActorSessionV1) -> Value {
    match session.current_response() {
        FastActorResponseV1::Terminal(value) => json!(value),
        FastActorResponseV1::Decision(_) => Value::Null,
    }
}

fn setup_failure(error: String) -> Value {
    json!({"status":"failure","complete":false,"selected":null,"blocked_search":null,
        "error":error,"search_admissions":[],"game":{"actions":[],"decisions":[],
        "searches":[],"key_mode":"repaired-v2","transitions_used":0,"real_transitions":0,
        "charged_real_transitions":0,"live_inference_calls":0,"unapplied_action":null,
        "end":null,"terminal":null},"wall":0.0})
}

pub(super) fn run(
    sh: &BankShared,
    policy: &FrozenPlayPolicyV1,
    src: &SourceSpec,
    cap: u64,
    binding: &str,
) -> Result<Value, String> {
    if !(1..=MAX_ACQUISITION_SEARCH).contains(&cap) {
        return Err("reuse prefix acquisition search cap must be 1..=320000".into());
    }
    let allocation = Allocation {
        anchor: ANCHOR_BUDGET,
        refresh: REFRESH_BUDGET,
        total: EXECUTOR_TOTAL,
    };
    let mut row = match execute(sh, policy, src, allocation, cap) {
        Ok(value) => value,
        Err(error) => setup_failure(error),
    };
    Recording::Enabled.annotate(&mut row["game"]);
    let object = row.as_object_mut().expect("prefix receipt object");
    object.extend(json!({"kind":"reused_prefix","schema":SCHEMA,"case":src,
        "lineage":sh.spec.lineage,"bank_binding_sha256":binding,
        "git_head":env!("MTG_KERNEL_BUILD_GIT_HEAD"),
        "tracked_tree_sha256":env!("MTG_KERNEL_BUILD_TRACKED_TREE_SHA256"),
        "allocation":{"initial":allocation.anchor,"refresh":allocation.refresh,"total":allocation.total},
        "acquisition_search_cap":cap,"interchange":false,"backup":"mean",
        "live_policy_seeds":src.guide_policy_seeds,
        "inherited_episode_limits":{"max_physical_decisions":sh.spec.max_physical_decisions,
            "max_policy_steps":sh.spec.max_policy_steps},
        "interpretation":"bounded prefix acquisition; not a completed whole-game result",
        "search_transition_convention":"sum actual search transitions; full unchanged budget checked before each search",
        "prefix_action_convention":"successfully applied executable actions including selected target"})
        .as_object().expect("metadata object").clone());
    Ok(row)
}

// The production wrapper above fixes its recipe. Parent-module engineering
// fixtures can lower these limits without exposing that seam to the CLI.
pub(super) fn execute(
    sh: &BankShared,
    policy: &FrozenPlayPolicyV1,
    src: &SourceSpec,
    allocation: Allocation,
    cap: u64,
) -> Result<Value, String> {
    sh.spec.validate()?;
    if src.seat > 1
        || src.starting_player > 1
        || src.own >= sh.spec.decks.len()
        || src.other >= sh.spec.decks.len()
    {
        return Err("reuse prefix case coordinates differ".into());
    }
    let opp = sh
        .labels
        .iter()
        .position(|label| *label == src.opp)
        .ok_or("unknown prefix opponent")?;
    let mut roles = bank_roles(policy, &sh.opponents)?;
    let mut live_focal = policy.fork_for_collection_v3()?;
    let mut live_opp = sh.opponents[opp].fork_for_collection_v3()?;
    let opening = source_session(&sh.spec, src)?;
    let mut control = Acquisition::new(cap);
    guide_inner(
        sh,
        &mut roles,
        &mut live_focal,
        &mut live_opp,
        &opening,
        src,
        opp,
        allocation,
        None,
        GuideRun {
            opening: true,
            mode: KeyMode::Repaired,
            interchange: false,
            bestline: None,
            reuse_witnesses: Recording::Enabled,
        },
        Some(&mut control),
        #[cfg(feature = "viewer-recording-v1")]
        None,
        #[cfg(feature = "viewer-recording-v1")]
        None,
        #[cfg(feature = "viewer-recording-v1")]
        None,
    )
    .map(|(row, _)| row)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acquisition_admits_whole_searches_and_never_clamps_the_next_budget() {
        let mut used = 0;
        for requested in [128_000, 64_000, 64_000, 64_000] {
            assert!(fits(used, requested, 320_000));
            used += requested;
        }
        assert_eq!(used, 320_000);
        assert!(!fits(used, 64_000, 320_000));
        assert!(!fits(300_000, 64_000, 320_000));
        assert!(!fits(u64::MAX, 1, u64::MAX));
        assert!(!fits(0, 0, 320_000));
        // Admission is consulted only for a search, not existing-tree traversal.
        assert!(later_reuse(2, 8, 7, 20, 16));
    }

    #[test]
    fn acquisition_requires_a_later_physical_decision_not_just_a_later_substep() {
        assert!(!later_reuse(2, 7, 7, 17, 16));
        assert!(!later_reuse(1, 8, 7, 20, 16));
        assert!(!later_reuse(2, 8, 7, 16, 16));
        assert!(later_reuse(2, 8, 7, 20, 16));
    }

    #[test]
    fn acquisition_accounts_actual_spending_without_funding_a_partial_search() {
        let records = vec![
            json!({"search":0,"budget":128000,"transitions":100000}),
            json!({"search":1,"budget":64000,"transitions":64000}),
        ];
        assert!(accounting(&records, 164000, 320000).is_ok());
        assert!(accounting(&records, 192000, 320000).is_err());
        assert!(accounting(&records, 164000, 150000).is_err());
        let over = vec![json!({"search":0,"budget":128000,"transitions":128001})];
        assert!(accounting(&over, 128001, 320000).is_err());
    }
}
