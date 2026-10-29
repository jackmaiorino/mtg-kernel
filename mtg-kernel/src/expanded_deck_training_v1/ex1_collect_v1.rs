//! EX1's collection hook in RT1 (collab
//! LANES/spy-primary-direction-20261028/EX1-DESIGN-DRAFT.md v3.3 sections 3
//! to 5 and 7 items 3 to 6). At every learner decision with two or more
//! legal actions:
//!
//! - **Placement:** the decision triggers when `ex1/placement`'s uniform is
//!   below its menu stratum's p(S). A trigger searches the live state through
//!   `guide_ctx` and `select_e` with SELF rollouts at B transitions, seeded
//!   from the placement hash, and its tree replaces any retained tree.
//! - **Burst:** the tree is retained until the engine turn of its trigger
//!   ends. Every later learner decision of that turn is looked up along the
//!   visible history; a matched node with at least 8 backups and two
//!   supported edges gives that row its target.
//! - **One target per row**, computed at the live row from the deployed
//!   logits (`ex1-target-v1`). The learner always plays its own sampled
//!   action; nothing here touches a live sampling stream.
//! - **Failures:** a failed search gives no target and no tree, is counted,
//!   and the episode continues.
//!
//! The record is a sidecar beside the trajectory, whose bytes the hook never
//! changes.

use super::*;
use crate::ex1_v1::{
    ex1_hash_u64_v1, ex1_placement_v1, ex1_rowcap_key_v1, ex1_stratum_v1, ex1_target_v1,
    ex1_uniform_v1, Ex1StratumV1, Ex1TargetV1,
};
use crate::line_b_teacher_target_v1::line_b_softmax_v1;
use crate::regret_census_v1::stage4a::{
    ex1_node_covers_v1, ex1_self_target_v1, Ex1LookupV1, Ex1NodeV1, Ex1SearcherV1,
};
use crate::rl_session::FastActorDecisionV1;
use std::collections::BTreeMap;

pub const EX1_COLLECT_SCHEMA_V1: &str = "ex1-collect-episode/v1";
/// The frozen per-search budget B, in raw selection transitions.
pub const EX1_BUDGET_V1: u64 = 64_000;
/// A useful self-target row lifts q(self) over p(self) by at least this.
pub const EX1_USEFUL_LIFT_V1: f64 = 0.05;

/// One update's collection options.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ex1CollectOptionsV1 {
    pub run: String,
    /// The global update index of this collection.
    pub update: u64,
    pub budget: u64,
}

impl Ex1CollectOptionsV1 {
    pub fn validate_v1(&self) -> Result<(), String> {
        ensure(
            !self.run.is_empty() && self.run.len() <= 256,
            "ex1 run id empty or too long",
        )?;
        ensure(
            self.budget == EX1_BUDGET_V1,
            "ex1 search budget differs from the frozen B = 64k",
        )
    }
}

/// What one learner decision brought to the select step.
pub(super) struct Ex1PendingV1 {
    stratum: Ex1StratumV1,
    kind: Option<&'static str>,
    node: Option<Ex1NodeV1>,
    looked_up: bool,
    self_target: Option<usize>,
    turn: u32,
}

#[derive(Default, Clone, Copy)]
struct StratumCountsV1 {
    decisions: u64,
    triggers: u64,
    retriggers: u64,
    searches_failed: u64,
    root_rows: u64,
    burst_rows: u64,
    burst_lookups: u64,
    burst_misses: u64,
    burst_uncovered: u64,
    no_target_support: u64,
    no_target_mass: u64,
}

impl StratumCountsV1 {
    fn json(&self) -> Value {
        json!({"decisions":self.decisions,"triggers":self.triggers,"retriggers":self.retriggers,
            "searches_failed":self.searches_failed,"root_rows":self.root_rows,
            "burst_rows":self.burst_rows,"burst_lookups":self.burst_lookups,
            "burst_misses":self.burst_misses,"burst_uncovered":self.burst_uncovered,
            "no_target_support":self.no_target_support,"no_target_mass":self.no_target_mass})
    }
}

/// One episode attempt's hook state and record.
pub(super) struct Ex1EpisodeHookV1 {
    options: Ex1CollectOptionsV1,
    seed: u64,
    learner_seat: u8,
    searcher: Ex1SearcherV1,
    burst_turn: Option<u32>,
    counts: BTreeMap<Ex1StratumV1, StratumCountsV1>,
    searches: Vec<Value>,
    rows: Vec<Value>,
    offers: Vec<Value>,
}

impl Ex1EpisodeHookV1 {
    pub(super) fn new(
        options: &Ex1CollectOptionsV1,
        learner: &FrozenPlayPolicyV1,
        episode: &ExpandedEpisodeV1,
    ) -> Result<Self, String> {
        options.validate_v1()?;
        Self::with_any_budget_v1(options, learner, episode)
    }

    /// Tests only: a hook at a budget other than the frozen B.
    #[cfg(test)]
    pub(super) fn with_test_budget_v1(
        options: &Ex1CollectOptionsV1,
        learner: &FrozenPlayPolicyV1,
        episode: &ExpandedEpisodeV1,
    ) -> Result<Self, String> {
        Self::with_any_budget_v1(options, learner, episode)
    }

    fn with_any_budget_v1(
        options: &Ex1CollectOptionsV1,
        learner: &FrozenPlayPolicyV1,
        episode: &ExpandedEpisodeV1,
    ) -> Result<Self, String> {
        Ok(Self {
            options: options.clone(),
            seed: episode.seed,
            learner_seat: episode.learner_seat,
            searcher: Ex1SearcherV1::new(learner, PlayerId(episode.learner_seat))?,
            burst_turn: None,
            counts: Ex1StratumV1::ALL
                .iter()
                .map(|&s| (s, StratumCountsV1::default()))
                .collect(),
            searches: Vec::new(),
            rows: Vec::new(),
            offers: Vec::new(),
        })
    }

    /// Whether `d` is a hooked decision: the learner's, with two or more
    /// legal actions.
    pub(super) fn hooks_v1(&self, d: &FastActorDecisionV1) -> bool {
        seat(d.acting_player) == self.learner_seat && d.legal_action_count >= 2
    }

    /// Placement, search or burst lookup at the live decision, before the
    /// learner samples.
    pub(super) fn before_select_v1(
        &mut self,
        session: &FastActorSessionV1,
        d: FastActorDecisionV1,
    ) -> Result<Ex1PendingV1, String> {
        let stratum = ex1_stratum_v1(
            &session
                .diagnostic_current_action_semantics()
                .ok_or("ex1: no live menu at a decision")?,
        );
        let turn = session.game_state().turn;
        if self.burst_turn.is_some_and(|t| t != turn) {
            self.searcher.clear();
            self.burst_turn = None;
        }
        let counts = self.counts.get_mut(&stratum).expect("every stratum");
        counts.decisions += 1;
        let self_target = ex1_self_target_v1(session, PlayerId(self.learner_seat));
        let (run, update, seed, decision) = (
            self.options.run.as_str(),
            self.options.update,
            self.seed,
            d.physical_decision_id,
        );
        if ex1_placement_v1(run, update, seed, decision, stratum) {
            counts.triggers += 1;
            counts.retriggers += u64::from(self.burst_turn.is_some());
            let hash = ex1_hash_u64_v1("ex1/placement", run, update, seed, decision);
            let (mut record, ok) = self.searcher.search(
                session,
                self.options.budget,
                format!("{hash:016x}/{}", d.step),
            )?;
            record["step"] = json!(d.step);
            record["physical_decision_id"] = json!(d.physical_decision_id);
            record["substep"] = json!(d.substep_index);
            record["stratum"] = json!(stratum.as_str());
            record["turn"] = json!(turn);
            record["uniform"] = json!(ex1_uniform_v1(hash));
            self.searches.push(record);
            if !ok {
                counts.searches_failed += 1;
                self.burst_turn = None;
                return Ok(Ex1PendingV1 {
                    stratum,
                    kind: None,
                    node: None,
                    looked_up: false,
                    self_target,
                    turn,
                });
            }
            self.burst_turn = Some(turn);
            let node = match self.searcher.lookup(session, d)? {
                Ex1LookupV1::Node(node) => node,
                other => {
                    return Err(format!(
                        "ex1: a new search's root is not its tree's: {other:?}"
                    ))
                }
            };
            return Ok(Ex1PendingV1 {
                stratum,
                kind: Some("root"),
                node: Some(node),
                looked_up: true,
                self_target,
                turn,
            });
        }
        if self.burst_turn.is_none() {
            return Ok(Ex1PendingV1 {
                stratum,
                kind: None,
                node: None,
                looked_up: false,
                self_target,
                turn,
            });
        }
        counts.burst_lookups += 1;
        let node = match self.searcher.lookup(session, d)? {
            Ex1LookupV1::Node(node) if ex1_node_covers_v1(&node) => Some(node),
            Ex1LookupV1::Node(_) => {
                counts.burst_uncovered += 1;
                None
            }
            Ex1LookupV1::Miss(_) | Ex1LookupV1::NoTree => {
                counts.burst_misses += 1;
                None
            }
        };
        Ok(Ex1PendingV1 {
            stratum,
            kind: node.is_some().then_some("burst"),
            node,
            looked_up: true,
            self_target,
            turn,
        })
    }

    /// The target at the live row from the deployed logits, then the cursor
    /// follows the learner's own sampled action.
    pub(super) fn after_select_v1(
        &mut self,
        pending: Ex1PendingV1,
        d: FastActorDecisionV1,
        logit_bits: &[u32],
        selected: u32,
    ) -> Result<(), String> {
        let counts = self
            .counts
            .get_mut(&pending.stratum)
            .expect("every stratum");
        let mut row_index = None;
        if let (Some(kind), Some(node)) = (pending.kind, &pending.node) {
            match ex1_target_v1(logit_bits, &node.counts)? {
                Err(reason) => match reason {
                    crate::ex1_v1::Ex1NoTargetV1::Support => counts.no_target_support += 1,
                    crate::ex1_v1::Ex1NoTargetV1::Mass => counts.no_target_mass += 1,
                },
                Ok(target) => {
                    if kind == "root" {
                        counts.root_rows += 1;
                    } else {
                        counts.burst_rows += 1;
                    }
                    row_index = Some(self.rows.len());
                    let row = row_json_v1(
                        kind,
                        pending.stratum,
                        &self.options,
                        self.seed,
                        d,
                        pending.turn,
                        node,
                        &target,
                        selected,
                        pending.self_target,
                    );
                    self.rows.push(row);
                }
            }
        }
        if let Some(index) = pending.self_target {
            let z: Vec<f64> = logit_bits
                .iter()
                .map(|&b| f64::from(f32::from_bits(b)))
                .collect();
            let p = line_b_softmax_v1(&z).map_err(|e| format!("ex1: {e}"))?;
            self.offers.push(
                json!({"step":d.step,"physical_decision_id":d.physical_decision_id,
                "substep":d.substep_index,"turn":pending.turn,"self_index":index,
                "p_self":p.probabilities[index],"chosen":selected as usize == index,
                "searched":pending.kind,"row":row_index,
                "tree":pending.node.as_ref().map(|node| offer_tree_json_v1(node, index))}),
            );
        }
        if pending.looked_up {
            self.searcher.advance(selected);
        }
        Ok(())
    }

    /// The attempt's record.
    pub(super) fn finish_v1(self, episode: &ExpandedEpisodeV1, natural: bool) -> Value {
        let strata: serde_json::Map<String, Value> = self
            .counts
            .iter()
            .map(|(s, c)| (s.as_str().to_string(), c.json()))
            .collect();
        json!({"schema":EX1_COLLECT_SCHEMA_V1,"options":self.options,"episode_id":episode.id,
            "seed":self.seed,"learner_seat":self.learner_seat,
            "learner_deck":episode.selected[usize::from(self.learner_seat)].label,
            "natural":natural,"strata":strata,"searches":self.searches,"rows":self.rows,
            "offers":self.offers})
    }
}

#[allow(clippy::too_many_arguments)]
fn row_json_v1(
    kind: &str,
    stratum: Ex1StratumV1,
    options: &Ex1CollectOptionsV1,
    seed: u64,
    d: FastActorDecisionV1,
    turn: u32,
    node: &Ex1NodeV1,
    target: &Ex1TargetV1,
    selected: u32,
    self_target: Option<usize>,
) -> Value {
    let played = selected as usize;
    let argmax = target.target_argmax_v1();
    // Teacher advantage (telemetry): Q-hat at the argmax of q over S minus
    // Q-hat at the played edge, only when the played edge is supported.
    let advantage = target.qhat[played].map(|q| target.qhat[argmax].expect("in S") - q);
    let mut row = json!({"kind":kind,"stratum":stratum.as_str(),"step":d.step,
        "physical_decision_id":d.physical_decision_id,"substep":d.substep_index,"turn":turn,
        "rowcap_key":ex1_rowcap_key_v1(&options.run, options.update, seed, d.physical_decision_id),
        "n":node.counts.iter().map(|c| c.n).collect::<Vec<_>>(),
        "wins":node.counts.iter().map(|c| c.wins).collect::<Vec<_>>(),
        "draws":node.counts.iter().map(|c| c.draws).collect::<Vec<_>>(),
        "visits":node.visits,"selected":selected,"support":target.support,
        "mass":target.mass,"tv":target.total_variation_v1(),
        "q_bits":target.target.probabilities.iter().map(|x| x.to_bits()).collect::<Vec<_>>(),
        "q":target.target.probabilities,"p":target.student.probabilities,
        "qhat":target.qhat,"target_argmax":argmax,"played_supported":target.qhat[played].is_some(),
        "teacher_advantage":advantage});
    if let Some(index) = self_target {
        row["self_target"] = self_target_json_v1(target, index, node.counts[index].wins);
    }
    row
}

/// The tree's view of a searched self-target offer, target or not: whether
/// its Q-hat favors the self-target over the best other supported edge, or
/// "unsupported" when the self edge or every other edge has under 8 backups.
fn offer_tree_json_v1(node: &Ex1NodeV1, index: usize) -> Value {
    let supported = |c: &crate::ex1_v1::Ex1EdgeCountsV1| c.n >= crate::ex1_v1::EX1_SUPPORT_MIN_V1;
    let own = &node.counts[index];
    let best_other = node
        .counts
        .iter()
        .enumerate()
        .filter(|&(a, c)| a != index && supported(c))
        .map(|(_, c)| crate::ex1_v1::ex1_qhat_v1(c))
        .fold(None, |best: Option<f64>, q| {
            Some(best.map_or(q, |b| b.max(q)))
        });
    match (supported(own), best_other) {
        (true, Some(other)) => {
            let margin = crate::ex1_v1::ex1_qhat_v1(own) - other;
            json!({"unsupported":false,"margin":margin,"favors":margin > 0.0,"self_n":own.n,
                "self_wins":own.wins})
        }
        _ => json!({"unsupported":true,"margin":null,"favors":null,"self_n":own.n,
            "self_wins":own.wins}),
    }
}

/// Section 5's self-target fields at a taught row: the margin (Q-hat(self)
/// minus the best other supported edge), "unsupported" when the self edge
/// or every other edge is unsupported, and the useful-row test (lift at
/// least 0.05, the self edge supported with a terminal win, margin above 0).
fn self_target_json_v1(target: &Ex1TargetV1, index: usize, self_wins: u64) -> Value {
    let q_self = target.target.probabilities[index];
    let p_self = target.student.probabilities[index];
    let lift = q_self - p_self;
    let best_other = target
        .support
        .iter()
        .filter(|&&a| a != index)
        .filter_map(|&a| target.qhat[a])
        .fold(None, |best: Option<f64>, q| {
            Some(best.map_or(q, |b| b.max(q)))
        });
    match (target.qhat[index], best_other) {
        (Some(q), Some(other)) => {
            let margin = q - other;
            json!({"index":index,"unsupported":false,"margin":margin,"favors":margin > 0.0,
                "lift":lift,"q_self":q_self,"p_self":p_self,"self_wins":self_wins,
                "useful":lift >= EX1_USEFUL_LIFT_V1 && self_wins >= 1 && margin > 0.0})
        }
        _ => json!({"index":index,"unsupported":true,"margin":null,"favors":null,
            "lift":lift,"q_self":q_self,"p_self":p_self,"self_wins":self_wins,"useful":false}),
    }
}
