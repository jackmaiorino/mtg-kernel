//! EX1's collection hook in RT1 (collab
//! LANES/spy-primary-direction-20261028/EX1-DESIGN-DRAFT.md v3.3 sections 3
//! to 5 and 7 items 3 to 6). At every learner decision with two or more
//! legal actions:
//!
//! - **Placement:** the decision triggers when `ex1/placement`'s uniform is
//!   below its menu stratum's p(S). A trigger searches the live state through
//!   `guide_ctx` and `select_e` with SELF rollouts at B transitions, seeded
//!   from the placement hash, and its tree replaces any retained tree.
//! - **Burst:** the tree is retained until the game turn of its trigger
//!   ends: one player's turn, identified by the round, the active player and
//!   (when extra turns exist) each player's count of turns started, since
//!   the engine's `turn` counts rounds. Every later learner decision of that turn is looked up along the
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
//!
//! **W5 (`w5-xact-v1`, collab LANES/lead/W5-DESIGN.md v1.3 section 3):** with
//! `w5` set, placement is one SHA-256 drawn index per learner seat-episode
//! over W1-eligible decisions (`w5_xact_v1`), at most one search per episode;
//! at a root with a target the played action is drawn from q, which the
//! trajectory records as `selected` (the learner's own draw is kept in the
//! sidecar). Burst rows are EX1's: the student acts. Without `w5` nothing
//! here changes.

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
use crate::w5_xact_v1::{
    w5_eligible_v1, w5_placement_hash_v1, w5_placement_index_v1, w5_played_action_v1,
    W5_BUDGET_MAX_V1, W5_KEY_MEMO_MIB_MAX_V1, W5_PLACEMENT_MAX_V1, W5_XACT_VERSION_V1,
};
use sha2::Digest;
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
    /// W5's recipe; omitted for EX1, so every EX1 command keeps its bytes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub w5: Option<W5CollectOptionsV1>,
}

/// W5's collection options: the recipe identifier, `w5-xact-v1`, and the
/// search's exact key memo limit in MiB (0: off). The memo is exact (keys,
/// trees, selection and statistics are unchanged; only time is), so it is a
/// recorded cost setting, not a recipe constant.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct W5CollectOptionsV1 {
    pub recipe: String,
    pub key_memo_mib: u64,
    /// Stage 0b capture mode (W5 design section 6.1): at the placed decision
    /// the hook records the candidate searches and the Q_ref rollouts instead
    /// of searching once and acting; the learner plays its own draw
    /// throughout. Omitted in training.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capture_0b: Option<W5CaptureOptionsV1>,
}

/// Stage 0b's capture: every (budget, search seed) candidate search and
/// `rollouts` SELF continuations per Q_ref action (the union of the
/// candidates' supported sets and the policy's argmax).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct W5CaptureOptionsV1 {
    pub budgets: Vec<u64>,
    pub search_seeds: Vec<u64>,
    pub rollouts: u64,
    /// W5 Stage 2a (VALUE-LEAF-DESIGN.md section 12): the value-leaf searches
    /// at the same placed decision, whose supported actions join Q_ref.
    /// Omitted: the Stage 0b record is unchanged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value_leaf: Option<W5ValueLeafCaptureV1>,
}

/// Stage 2a's value-leaf candidates: every horizon x simulation count x
/// search seed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct W5ValueLeafCaptureV1 {
    pub horizons: Vec<String>,
    pub sims: Vec<u64>,
    pub search_seeds: Vec<u64>,
}

impl Ex1CollectOptionsV1 {
    pub fn validate_v1(&self) -> Result<(), String> {
        ensure(
            !self.run.is_empty() && self.run.len() <= 256,
            "ex1 run id empty or too long",
        )?;
        match &self.w5 {
            None => ensure(
                self.budget == EX1_BUDGET_V1,
                "ex1 search budget differs from the frozen B = 64k",
            ),
            Some(w5) => {
                ensure(
                    w5.recipe == W5_XACT_VERSION_V1,
                    "w5 collection names an unknown recipe",
                )?;
                ensure(
                    w5.key_memo_mib <= W5_KEY_MEMO_MIB_MAX_V1,
                    "w5 key memo limit above 4096 MiB",
                )?;
                if let Some(capture) = &w5.capture_0b {
                    ensure(
                        !capture.budgets.is_empty()
                            && capture.budgets.len() <= 8
                            && capture
                                .budgets
                                .iter()
                                .all(|&b| b > 0 && b <= W5_BUDGET_MAX_V1)
                            && !capture.search_seeds.is_empty()
                            && capture.search_seeds.len() <= 8
                            && capture.rollouts > 0
                            && capture.rollouts <= 4096,
                        "w5 0b capture: 1 to 8 budgets and seeds, 1 to 4096 rollouts",
                    )?;
                    if let Some(vl) = &capture.value_leaf {
                        ensure(
                            !vl.horizons.is_empty()
                                && vl.horizons.len() <= 2
                                && vl.horizons.iter().all(|h| {
                                    crate::regret_census_v1::stage4a::value_leaf::Horizon::parse(h)
                                        .is_ok()
                                })
                                && !vl.sims.is_empty()
                                && vl.sims.len() <= 4
                                && vl.sims.iter().all(|&n| n > 0 && n <= 65_536)
                                && !vl.search_seeds.is_empty()
                                && vl.search_seeds.len() <= 4,
                            "w5 2a capture: 1 or 2 horizons (h0, t0), 1 to 4 simulation counts \
                             in 1..=65536, 1 to 4 seeds",
                        )?;
                    }
                }
                ensure(
                    self.budget > 0 && self.budget <= W5_BUDGET_MAX_V1,
                    "w5 search budget outside 1..=2^20",
                )
            }
        }
    }
}

/// One player's game turn: the round, the active player and, once extra
/// turns exist, each player's count of turns started.
type TurnKeyV1 = (u32, u8, Option<[u32; 2]>);

fn turn_key_v1(session: &FastActorSessionV1) -> TurnKeyV1 {
    let state = session.game_state();
    (
        state.turn,
        state.active_player.0,
        state.extra_turn_v1.as_ref().map(|e| e.own_turns_started),
    )
}

/// W5's per seat-episode placement state.
struct W5PlacementV1 {
    slot: u64,
    hash: u64,
    /// The 1-based eligible index to search.
    index: u64,
    eligible: u64,
    searched: bool,
    /// Searches attempted, roots with a target (the search played), roots
    /// without one.
    attempted: u64,
    acted: u64,
    root_no_target: u64,
}

impl W5PlacementV1 {
    fn json(&self) -> Value {
        json!({"slot":self.slot,"placement_hash":format!("{:016x}",self.hash),
            "placement_index":self.index,"placement_max":W5_PLACEMENT_MAX_V1,
            "eligible_decisions":self.eligible,"reached":self.eligible >= self.index,
            "attempted":self.attempted,"acted":self.acted,"root_no_target":self.root_no_target})
    }
}

/// What one learner decision brought to the select step.
pub(super) struct Ex1PendingV1 {
    stratum: Ex1StratumV1,
    kind: Option<&'static str>,
    /// The node that gives this row its target (a root, or a covered burst
    /// node).
    node: Option<Ex1NodeV1>,
    /// Any matched node, covered or not (the offer's tree classification).
    matched: Option<Ex1NodeV1>,
    looked_up: bool,
    self_target: Option<usize>,
    turn: TurnKeyV1,
    /// Stage 0b: the placement hash of the decision to capture.
    capture_hash: Option<u64>,
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
    burst_turn: Option<TurnKeyV1>,
    counts: BTreeMap<Ex1StratumV1, StratumCountsV1>,
    searches: Vec<Value>,
    rows: Vec<Value>,
    offers: Vec<Value>,
    w5: Option<W5PlacementV1>,
    /// Stage 0b capture mode: its options, the rollout policy and the record.
    capture: Option<(W5CaptureOptionsV1, FrozenPlayPolicyV1)>,
    captured: Option<Value>,
}

impl Ex1EpisodeHookV1 {
    pub(super) fn new(
        options: &Ex1CollectOptionsV1,
        learner: &FrozenPlayPolicyV1,
        episode: &ExpandedEpisodeV1,
        slot: usize,
    ) -> Result<Self, String> {
        options.validate_v1()?;
        Self::with_any_budget_v1(options, learner, episode, slot)
    }

    /// Tests only: a hook at a budget other than the frozen B.
    #[cfg(test)]
    pub(super) fn with_test_budget_v1(
        options: &Ex1CollectOptionsV1,
        learner: &FrozenPlayPolicyV1,
        episode: &ExpandedEpisodeV1,
        slot: usize,
    ) -> Result<Self, String> {
        Self::with_any_budget_v1(options, learner, episode, slot)
    }

    fn with_any_budget_v1(
        options: &Ex1CollectOptionsV1,
        learner: &FrozenPlayPolicyV1,
        episode: &ExpandedEpisodeV1,
        slot: usize,
    ) -> Result<Self, String> {
        let w5 = options.w5.as_ref().map(|_| {
            let hash =
                w5_placement_hash_v1(&options.run, options.update, episode.seed, slot as u64);
            W5PlacementV1 {
                slot: slot as u64,
                hash,
                index: w5_placement_index_v1(hash),
                eligible: 0,
                searched: false,
                attempted: 0,
                acted: 0,
                root_no_target: 0,
            }
        });
        Ok(Self {
            options: options.clone(),
            seed: episode.seed,
            learner_seat: episode.learner_seat,
            searcher: match options.w5.as_ref().map(|w5| w5.key_memo_mib) {
                // W5 runs the search with its recorded exact key memo; EX1's
                // searches keep the process default, unchanged.
                Some(mib) if mib > 0 => {
                    Ex1SearcherV1::new(learner, PlayerId(episode.learner_seat))?
                        .with_key_memo_v1((mib as usize) << 20)
                }
                Some(_) => Ex1SearcherV1::new(learner, PlayerId(episode.learner_seat))?
                    .with_key_memo_off_v1(),
                None => Ex1SearcherV1::new(learner, PlayerId(episode.learner_seat))?,
            },
            burst_turn: None,
            counts: Ex1StratumV1::ALL
                .iter()
                .map(|&s| (s, StratumCountsV1::default()))
                .collect(),
            searches: Vec::new(),
            rows: Vec::new(),
            offers: Vec::new(),
            w5,
            capture: options
                .w5
                .as_ref()
                .and_then(|w5| w5.capture_0b.clone())
                .map(|capture| Ok::<_, String>((capture, learner.fork_for_collection_v3()?)))
                .transpose()?,
            captured: None,
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
        let menu = session
            .diagnostic_current_action_semantics()
            .ok_or("ex1: no live menu at a decision")?;
        let stratum = ex1_stratum_v1(&menu);
        let turn = turn_key_v1(session);
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
        // EX1: a per-decision stratum draw. W5: the episode's k-th
        // W1-eligible decision, once.
        let trigger = match self.w5.as_mut() {
            None => ex1_placement_v1(run, update, seed, decision, stratum)
                .then(|| ex1_hash_u64_v1("ex1/placement", run, update, seed, decision)),
            Some(w5) => {
                let eligible = w5_eligible_v1(&menu);
                w5.eligible += u64::from(eligible);
                let fire = eligible && !w5.searched && w5.eligible == w5.index;
                if fire {
                    w5.searched = true;
                    w5.attempted += 1;
                }
                fire.then_some(w5.hash)
            }
        };
        if let (Some(hash), true) = (trigger, self.capture.is_some()) {
            // Stage 0b: the captures run at the live row, after the learner's
            // logits are known (`after_select_v1`).
            counts.triggers += 1;
            return Ok(Ex1PendingV1 {
                stratum,
                kind: Some("capture"),
                node: None,
                matched: None,
                looked_up: false,
                self_target,
                turn,
                capture_hash: Some(hash),
            });
        }
        if self.capture.is_some() {
            return Ok(Ex1PendingV1 {
                stratum,
                kind: None,
                node: None,
                matched: None,
                looked_up: false,
                self_target,
                turn,
                capture_hash: None,
            });
        }
        if let Some(hash) = trigger {
            counts.triggers += 1;
            counts.retriggers += u64::from(self.burst_turn.is_some());
            let (mut record, ok) = self.searcher.search(
                session,
                self.options.budget,
                format!("{hash:016x}/{}", d.step),
            )?;
            record["step"] = json!(d.step);
            record["physical_decision_id"] = json!(d.physical_decision_id);
            record["substep"] = json!(d.substep_index);
            record["stratum"] = json!(stratum.as_str());
            record["turn"] = json!(turn.0);
            record["active_player"] = json!(turn.1);
            record["uniform"] = json!(ex1_uniform_v1(hash));
            if let Some(w5) = &self.w5 {
                record["eligible_index"] = json!(w5.index);
            }
            // A search whose tree lacks its own root counts as failed too.
            let root = if ok {
                match self.searcher.lookup(session, d)? {
                    Ex1LookupV1::Node(node) => Some(node),
                    _ => None,
                }
            } else {
                None
            };
            record["root_miss"] = json!(ok && root.is_none());
            record["failed"] = json!(root.is_none());
            self.searches.push(record);
            let Some(node) = root else {
                counts.searches_failed += 1;
                self.searcher.clear();
                self.burst_turn = None;
                return Ok(Ex1PendingV1 {
                    stratum,
                    kind: None,
                    node: None,
                    matched: None,
                    looked_up: false,
                    self_target,
                    turn,
                    capture_hash: None,
                });
            };
            self.burst_turn = Some(turn);
            return Ok(Ex1PendingV1 {
                stratum,
                kind: Some("root"),
                node: Some(node.clone()),
                matched: Some(node),
                looked_up: true,
                self_target,
                turn,
                capture_hash: None,
            });
        }
        if self.burst_turn.is_none() {
            return Ok(Ex1PendingV1 {
                stratum,
                kind: None,
                node: None,
                matched: None,
                looked_up: false,
                self_target,
                turn,
                capture_hash: None,
            });
        }
        counts.burst_lookups += 1;
        let matched = match self.searcher.lookup(session, d)? {
            Ex1LookupV1::Node(node) => Some(node),
            Ex1LookupV1::Miss(_) | Ex1LookupV1::NoTree => {
                counts.burst_misses += 1;
                None
            }
        };
        let node = matched.clone().filter(ex1_node_covers_v1);
        counts.burst_uncovered += u64::from(matched.is_some() && node.is_none());
        Ok(Ex1PendingV1 {
            stratum,
            kind: node.is_some().then_some("burst"),
            node,
            matched,
            looked_up: true,
            self_target,
            turn,
            capture_hash: None,
        })
    }

    /// The target at the live row from the deployed logits, then the cursor
    /// follows the played action. Returns `Some(action)` when the search
    /// plays (a W5 root with a target, q's draw); otherwise the learner's own
    /// sampled action plays and this returns `None`.
    pub(super) fn after_select_v1(
        &mut self,
        pending: Ex1PendingV1,
        session: &FastActorSessionV1,
        d: FastActorDecisionV1,
        logit_bits: &[u32],
        selected: u32,
    ) -> Result<Option<u32>, String> {
        if let Some(hash) = pending.capture_hash {
            self.captured = Some(self.capture_0b_v1(session, d, logit_bits, hash)?);
            return Ok(None);
        }
        let counts = self
            .counts
            .get_mut(&pending.stratum)
            .expect("every stratum");
        let mut row_index = None;
        let mut played = selected;
        let mut search_played = false;
        if let (Some(kind), Some(node)) = (pending.kind, &pending.node) {
            let target = ex1_target_v1(logit_bits, &node.counts)?;
            if let (Some(w5), "root", Err(_)) = (self.w5.as_mut(), kind, &target) {
                w5.root_no_target += 1;
            }
            match target {
                Err(reason) => match reason {
                    crate::ex1_v1::Ex1NoTargetV1::Support => counts.no_target_support += 1,
                    crate::ex1_v1::Ex1NoTargetV1::Mass => counts.no_target_mass += 1,
                },
                Ok(target) => {
                    let acted = match self.w5.as_mut() {
                        Some(w5) if kind == "root" => {
                            w5.acted += 1;
                            played = u32::try_from(w5_played_action_v1(
                                &target.target.probabilities,
                                &self.options.run,
                                self.options.update,
                                self.seed,
                                w5.slot,
                            )?)
                            .map_err(|e| e.to_string())?;
                            true
                        }
                        _ => false,
                    };
                    search_played = acted;
                    if kind == "root" {
                        counts.root_rows += 1;
                    } else {
                        counts.burst_rows += 1;
                    }
                    row_index = Some(self.rows.len());
                    let mut row = row_json_v1(
                        kind,
                        pending.stratum,
                        &self.options,
                        self.seed,
                        d,
                        pending.turn,
                        node,
                        &target,
                        played,
                        pending.self_target,
                    );
                    if self.w5.is_some() {
                        row["acted"] = json!(acted);
                        row["policy_selected"] = json!(selected);
                    }
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
                "substep":d.substep_index,"turn":pending.turn.0,"active_player":pending.turn.1,
                "self_index":index,"p_self":p.probabilities[index],
                "chosen":played as usize == index,"searched":pending.kind,"row":row_index,
                "tree":pending.matched.as_ref().map(|node| offer_tree_json_v1(node, index))}),
            );
        }
        if pending.looked_up {
            self.searcher.advance(played);
        }
        Ok(search_played.then_some(played))
    }

    /// The attempt's record.
    pub(super) fn finish_v1(self, episode: &ExpandedEpisodeV1, natural: bool) -> Value {
        let strata: serde_json::Map<String, Value> = self
            .counts
            .iter()
            .map(|(s, c)| (s.as_str().to_string(), c.json()))
            .collect();
        let mut record = json!({"schema":EX1_COLLECT_SCHEMA_V1,"options":self.options,"episode_id":episode.id,
            "seed":self.seed,"learner_seat":self.learner_seat,
            "learner_deck":episode.selected[usize::from(self.learner_seat)].label,
            "natural":natural,"strata":strata,"searches":self.searches,"rows":self.rows,
            "offers":self.offers});
        if let Some(w5) = &self.w5 {
            record["w5"] = w5.json();
        }
        if self.capture.is_some() {
            record["capture_0b"] = self.captured.unwrap_or(json!({"reached":false}));
        }
        record
    }

    /// Stage 0b at the placed decision: each candidate search's root counts
    /// and CPU, then `rollouts` SELF continuations of every Q_ref action from
    /// the live state (forked policy streams, so the episode's own sampling
    /// never moves). Each continuation's seeds are drawn from
    /// `SHA256(json(["w5/0b/qref", chain, update, seed, slot]))`, the action
    /// and the rollout index, independent of every search seed. A
    /// continuation that ends non-naturally scores as a draw (counted).
    fn capture_0b_v1(
        &mut self,
        session: &FastActorSessionV1,
        d: FastActorDecisionV1,
        logit_bits: &[u32],
        hash: u64,
    ) -> Result<Value, String> {
        let (capture, rollout_policy) = self.capture.as_mut().expect("capture mode");
        let slot = self.w5.as_ref().map_or(0, |w5| w5.slot);
        let mut candidates = Vec::new();
        let mut actions = std::collections::BTreeSet::new();
        for &budget in &capture.budgets {
            for &search_seed in &capture.search_seeds {
                let seed_root = format!("w5/0b/{hash:016x}/{}/b{budget}/s{search_seed}", d.step);
                let started = crate::regret_census_v1::stage4a::process_cpu::seconds()?;
                let (record, ok) = self.searcher.search(session, budget, seed_root.clone())?;
                let cpu_s = crate::regret_census_v1::stage4a::process_cpu::seconds()? - started;
                let node = if ok {
                    match self.searcher.lookup(session, d)? {
                        Ex1LookupV1::Node(node) => Some(node),
                        _ => None,
                    }
                } else {
                    None
                };
                self.searcher.clear();
                let mut candidate = json!({"budget":budget,"search_seed":search_seed,
                    "seed_root":seed_root,"status":if node.is_some() {"ok"} else {"failed"},
                    "cpu_s":cpu_s,"search":record});
                if let Some(node) = node {
                    let n: Vec<u64> = node.counts.iter().map(|c| c.n).collect();
                    for (a, &count) in n.iter().enumerate() {
                        if count >= crate::ex1_v1::EX1_SUPPORT_MIN_V1 {
                            actions.insert(a);
                        }
                    }
                    candidate["n"] = json!(n);
                    candidate["wins"] =
                        json!(node.counts.iter().map(|c| c.wins).collect::<Vec<_>>());
                    candidate["draws"] =
                        json!(node.counts.iter().map(|c| c.draws).collect::<Vec<_>>());
                    if let Ok(target) = ex1_target_v1(logit_bits, &node.counts)? {
                        candidate["q_bits"] = json!(target
                            .target
                            .probabilities
                            .iter()
                            .map(|x| x.to_bits())
                            .collect::<Vec<_>>());
                    }
                }
                candidates.push(candidate);
            }
        }
        // Stage 2a: the value-leaf searches at the same decision. Their
        // supported actions outside the 0b Q_ref set get their own
        // continuations (`qref_extra`, tag `w5/2a/qref`), so the 0b record is
        // unchanged.
        let mut value_leaf = Vec::new();
        let mut vl_actions = std::collections::BTreeSet::new();
        if let Some(vl) = capture.value_leaf.clone() {
            for label in &vl.horizons {
                let horizon = crate::regret_census_v1::stage4a::value_leaf::Horizon::parse(label)?;
                for &sims in &vl.sims {
                    for &search_seed in &vl.search_seeds {
                        let step = d.step;
                        let seed_root =
                            format!("w5/2a/{hash:016x}/{step}/{label}/n{sims}/s{search_seed}");
                        let started = crate::regret_census_v1::stage4a::process_cpu::seconds()?;
                        let (record, root) = self.searcher.value_leaf_search(
                            session,
                            horizon,
                            sims,
                            seed_root.clone(),
                        )?;
                        let cpu_s =
                            crate::regret_census_v1::stage4a::process_cpu::seconds()? - started;
                        let mut row = json!({"horizon":label,"n_sims":sims,"search_seed":search_seed,
                            "seed_root":seed_root,"status":if root.is_some() {"ok"} else {"failed"},
                            "cpu_s":cpu_s,"primary_sha256":record["primary_sha256"],"search":record});
                        if let Some(root) = root {
                            for (a, &(n, _, _)) in root.iter().enumerate() {
                                if n >= crate::ex1_v1::EX1_SUPPORT_MIN_V1 {
                                    vl_actions.insert(a);
                                }
                            }
                            row["n"] = json!(root.iter().map(|x| x.0).collect::<Vec<_>>());
                            row["value_units"] =
                                json!(root.iter().map(|x| x.1).collect::<Vec<_>>());
                            row["draws"] = json!(root.iter().map(|x| x.2).collect::<Vec<_>>());
                        }
                        value_leaf.push(row);
                    }
                }
            }
        }
        // The policy's argmax on its binary64 softmax (the consumer's
        // `ex1-target-v1` mirror), ties to the lowest index.
        let z: Vec<f64> = logit_bits
            .iter()
            .map(|&b| f64::from(f32::from_bits(b)))
            .collect();
        let p = line_b_softmax_v1(&z).map_err(|e| format!("w5 capture: {e}"))?;
        let argmax = (0..p.probabilities.len()).fold(0, |best, a| {
            if p.probabilities[a] > p.probabilities[best] {
                a
            } else {
                best
            }
        });
        actions.insert(argmax);
        let extra: Vec<usize> = vl_actions.difference(&actions).copied().collect();
        let actions: Vec<usize> = actions.into_iter().collect();
        let focal = self.learner_seat;
        let (run, update, seed) = (self.options.run.clone(), self.options.update, self.seed);
        let rollouts = capture.rollouts;
        let qref_root = format!("w5/0b/qref/{hash:016x}/{}", d.step);
        let mut continuations = |tag: &str, actions: &[usize]| -> Result<[Vec<u64>; 4], String> {
            let (mut wins, mut draws, mut n, mut nonnatural) = (vec![], vec![], vec![], vec![]);
            for &action in actions {
                let (mut w, mut dr, mut nn) = (0_u64, 0_u64, 0_u64);
                for r in 0..rollouts {
                    let text = json!([tag, run, update, seed, slot, action, r]).to_string();
                    let digest = sha2::Sha256::digest(text.as_bytes());
                    let mut first = [0_u8; 8];
                    first.copy_from_slice(&digest[..8]);
                    let mut s = session.clone();
                    s.step(d.episode_id, d.step, action as u32).map_err(err)?;
                    rollout_policy.reset_sampling_v1(
                        crate::paired_bo1_harness_v1::paired_policy_seeds_v1(u64::from_le_bytes(
                            first,
                        )),
                    );
                    loop {
                        match s.current_response() {
                            FastActorResponseV1::Terminal(t) => {
                                if t.terminal_classification != TerminalClassificationV1::Natural {
                                    nn += 1;
                                    dr += 1;
                                } else {
                                    match t.winner {
                                        Some(winner) if seat(winner) == focal => w += 1,
                                        Some(_) => {}
                                        None => dr += 1,
                                    }
                                }
                                break;
                            }
                            FastActorResponseV1::Decision(next) => {
                                let a = rollout_policy.select_fast_session_v1(&s)?;
                                s.step(next.episode_id, next.step, a).map_err(err)?;
                            }
                        }
                    }
                }
                wins.push(w);
                draws.push(dr);
                n.push(rollouts);
                nonnatural.push(nn);
            }
            Ok([wins, draws, n, nonnatural])
        };
        let [wins, draws, n, nonnatural] = continuations("w5/0b/qref", &actions)?;
        let mut out = json!({"reached":true,"step":d.step,"physical_decision_id":d.physical_decision_id,
            "policy":{"logit_bits":logit_bits},"candidates":candidates,
            "qref":{"rollouts":rollouts,"seed_root":qref_root,"actions":actions,
                "wins":wins,"draws":draws,"n":n},"qref_nonnatural":nonnatural});
        if capture.value_leaf.is_some() {
            let [wins, draws, n, nonnatural] = continuations("w5/2a/qref", &extra)?;
            out["value_leaf"] = json!(value_leaf);
            out["qref_extra"] = json!({"rollouts":rollouts,
                "seed_root":format!("w5/2a/qref/{hash:016x}/{}", d.step),"actions":extra,
                "wins":wins,"draws":draws,"n":n,"nonnatural":nonnatural});
        }
        Ok(out)
    }
}

#[allow(clippy::too_many_arguments)]
fn row_json_v1(
    kind: &str,
    stratum: Ex1StratumV1,
    options: &Ex1CollectOptionsV1,
    seed: u64,
    d: FastActorDecisionV1,
    turn: TurnKeyV1,
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
        "physical_decision_id":d.physical_decision_id,"substep":d.substep_index,"turn":turn.0,
        "active_player":turn.1,
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
