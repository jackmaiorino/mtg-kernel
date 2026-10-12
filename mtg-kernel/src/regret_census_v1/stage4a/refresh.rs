//! Generic search refresh (collab LANES/spy-search-refresh-plan-20261011,
//! PLAN.md revision 2), mode `s4a-refresh`.
//!
//! A guide plays from a stopped-bank anchor on the actual world. At each real
//! decision of the guide's player (two or more legal actions):
//! 1. reuse: follow the current tree from its root along the player-visible
//!    history (unchanged keys, no merging); a matched node within depth 32
//!    with an action of at least 8 natural terminal backups executes its
//!    greatest-mean such action;
//! 2. refresh: otherwise (history miss, matched node without support, depth
//!    limit, or no tree) and while budget remains, search again rooted at
//!    this decision with min(64,000, remaining) raw selection transitions,
//!    replace the tree and apply rule 1 at its root;
//! 3. plain: otherwise the live plain policy plays. A decision refreshes at
//!    most once.
//!
//! Live policies (plain focal and opponent, seeded once from the guide seeds)
//! are separate instances from the search roles, which every simulation
//! reseeds; a search never touches live sampling state.
//!
//! Arms per source: `R` (128,000 at the anchor, refreshes, cap 512,000) and
//! `S128` (the same anchor tree, never refreshed). `S512` (512,000 at the
//! anchor, never refreshed) is the parity arm that must reproduce the bank's
//! S guide.

use super::arms::{Limits, Roles};
use super::bank::{
    act_plain, bank_roles, err, guide_ctx, guide_record, outcome, plain_logged, source_session,
    walk, BankShared, SourceSpec,
};
use super::canon2::{KeyMode, KeyStats, Keyer, Step as KeyStep};
use super::labels::{classify, spy_defs};
use super::play::{acting, apply, decision, Counters, Meter};
use super::seeds::RootSeeds;
use super::tree::{live_index, Found, Tree, MAX_DEPTH};
use crate::ids::PlayerId;
use crate::rl_session::FastActorSessionV1;
use crate::sideboard_play_policy_v1::FrozenPlayPolicyV1;
use serde_json::{json, Value};
use std::time::Instant;

pub(super) const TOTAL_BUDGET: u64 = 512_000;
pub(super) const ANCHOR_BUDGET: u64 = 128_000;
pub(super) const REFRESH_BUDGET: u64 = 64_000;

/// One arm's search allocation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Allocation {
    pub(super) anchor: u64,
    pub(super) refresh: u64,
    pub(super) total: u64,
}

impl Allocation {
    pub(super) const R: Self = Self {
        anchor: ANCHOR_BUDGET,
        refresh: REFRESH_BUDGET,
        total: TOTAL_BUDGET,
    };
    pub(super) const S128: Self = Self {
        anchor: ANCHOR_BUDGET,
        refresh: 0,
        total: ANCHOR_BUDGET,
    };
    pub(super) const S512: Self = Self {
        anchor: TOTAL_BUDGET,
        refresh: 0,
        total: TOTAL_BUDGET,
    };

    /// Transitions the next refresh may spend after `used`, or 0 (none).
    pub(super) fn next_refresh(&self, used: u64) -> u64 {
        if self.refresh == 0 {
            return 0;
        }
        self.refresh.min(self.total.saturating_sub(used))
    }
}

/// What the current tree says at a real decision.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Lookup {
    /// A matched node with a supported action (its edge index).
    Supported(usize),
    /// A matched node with no action of 8 natural backups.
    Unsupported,
    /// The history left the tree (now or earlier).
    Miss,
    /// Past the depth limit along a matching history.
    DepthLimit,
}

impl Lookup {
    fn reason(self) -> &'static str {
        match self {
            Self::Supported(_) => "supported",
            Self::Unsupported => "unsupported",
            Self::Miss => "miss",
            Self::DepthLimit => "depth_limit",
        }
    }
}

/// The rule at one real decision, before any refresh here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Step {
    Execute(usize),
    Refresh(u64),
    Plain,
}

/// How a guide starts and which keys it uses.
#[derive(Clone, Copy, Debug)]
pub(super) struct GuideRun {
    /// From an ordinary opening (initial search at the first eligible focal
    /// choice) rather than from a bank anchor.
    pub(super) opening: bool,
    pub(super) mode: KeyMode,
}

impl GuideRun {
    pub(super) const ANCHOR_LEGACY: Self = Self {
        opening: false,
        mode: KeyMode::Legacy,
    };
}

pub(super) fn rule(
    lookup: Lookup,
    allocation: Allocation,
    used: u64,
    refreshed_here: bool,
) -> Step {
    match lookup {
        Lookup::Supported(e) => Step::Execute(e),
        _ if refreshed_here => Step::Plain,
        _ => match allocation.next_refresh(used) {
            0 => Step::Plain,
            budget => Step::Refresh(budget),
        },
    }
}

/// The current tree and where the actual history stands in it; keys use the
/// same version and mapping as the search that built the tree.
struct Cursor {
    tree: Tree,
    keyer: Keyer<KeyStats>,
    advanced: bool,
    matching: bool,
    depth: u32,
    last_phys: Option<u64>,
    search: usize,
}

/// A looked-up decision: the keyed step and, on a hit, the supported edge's
/// prior natural backups.
struct Hit {
    step: KeyStep,
    backups: Option<u64>,
}

impl Cursor {
    fn new(tree: Tree, search: usize, mode: KeyMode) -> Self {
        Self {
            tree,
            keyer: Keyer::new(mode, KeyStats::default()),
            advanced: false,
            matching: true,
            depth: 0,
            last_phys: None,
            search,
        }
    }

    /// Looks up the current decision.
    fn lookup(
        &mut self,
        s: &FastActorSessionV1,
        d: crate::rl_session::FastActorDecisionV1,
    ) -> Result<(Lookup, Option<Hit>), String> {
        if self.last_phys != Some(d.physical_decision_id) {
            self.depth += 1;
            self.last_phys = Some(d.physical_decision_id);
        }
        if !self.matching {
            return Ok((Lookup::Miss, None));
        }
        if self.depth > MAX_DEPTH {
            return Ok((Lookup::DepthLimit, None));
        }
        let st = self.keyer.step(s, d).map_err(err)?;
        let (key, pk, pe, cn) = self.keyer.storage(&st);
        let (found, backups) = match self.tree.find(&key, &pk, &pe, cn).map_err(err)? {
            Found::Hit(node) => match node.frozen_choice() {
                Some(e) => (Lookup::Supported(e), Some(node.n[e])),
                None => (Lookup::Unsupported, None),
            },
            Found::Miss => (Lookup::Miss, None),
        };
        if found == Lookup::Miss {
            self.matching = false;
        }
        Ok((found, Some(Hit { step: st, backups })))
    }

    /// The live candidate of storage edge `e` at a looked-up decision.
    fn live(&self, hit: &Hit, e: usize) -> Result<u32, String> {
        let (_, _, _, cn) = self.keyer.storage(&hit.step);
        live_index(cn, &cn.edges[e]).map_err(err)
    }

    /// Advances along the chosen live action (a matched node, any chooser).
    fn advance(&mut self, hit: Hit, live: u32) {
        self.keyer.advance(hit.step, live);
        self.advanced = true;
    }
}

struct Searches<'a> {
    roles: &'a mut Roles,
    sh: &'a BankShared,
    focal: PlayerId,
    opp: usize,
    root: String,
    lineage: String,
    used: u64,
    records: Vec<Value>,
    mode: KeyMode,
    keys: KeyStats,
}

impl Searches<'_> {
    /// One persistent search rooted at `s`; None when it failed (incomplete
    /// or faulted), which invalidates the guide.
    fn run(
        &mut self,
        s: &FastActorSessionV1,
        budget: u64,
        at_step: u64,
        reason: &str,
    ) -> Result<Option<Tree>, String> {
        let k = self.records.len();
        let seeds = RootSeeds {
            model: self.lineage.clone(),
            root: if k == 0 {
                self.root.clone()
            } else {
                format!("{}/refresh{k}", self.root)
            },
        };
        let limits = Limits {
            select_cap: budget,
            ..self.sh.limits
        };
        let mut ctx = guide_ctx(s, self.focal, self.opp, &seeds, &self.sh.prior, limits)?;
        ctx.key_mode = self.mode;
        let started = Instant::now();
        let mut diag = super::arms::SearchDiag::default();
        let (sel, tree) = self
            .roles
            .select_e_diagnosed(&ctx, None, 1, None, None, Some(&mut diag));
        self.keys.merge(&diag.keys);
        self.used += sel.transitions;
        let failed = sel.incomplete || !sel.faults.is_empty();
        self.records.push(json!({"search":k,"step":at_step,"reason":reason,"budget":budget,
            "root":super::bank::decision_identity(s)?,
            "transitions":sel.transitions,"completed_natural":sel.completed,"attempted":sel.attempted,
            "nonnatural":sel.nonnatural,"incomplete":sel.incomplete,"faults":sel.faults,
            "tree":sel.extra["tree"].clone(),"keys":diag.keys.json(),"wall":started.elapsed().as_secs_f64()}));
        Ok((!failed).then_some(tree))
    }
}

/// One arm's guide from the anchor. Returns the arm record. With a
/// `scheduler` (track A of collab LANES/discovery-efficiency-plan-20261012),
/// searches happen only when it chooses a positive amount at an unsupported
/// focal decision; the support and fallback rules are unchanged.
#[allow(clippy::too_many_arguments, reason = "explicit roles and identities")]
pub(super) fn guide(
    sh: &BankShared,
    roles: &mut Roles,
    live_focal: &mut FrozenPlayPolicyV1,
    live_opp: &mut FrozenPlayPolicyV1,
    anchor: &FastActorSessionV1,
    src: &SourceSpec,
    opp: usize,
    allocation: Allocation,
    anchor_tree: Option<(Tree, Value)>,
    run: GuideRun,
    mut scheduler: Option<&mut super::allocator::Scheduler>,
) -> Result<(Value, Option<Tree>), String> {
    let started = Instant::now();
    let focal = PlayerId(src.seat);
    let defs = spy_defs();
    let mut searches = Searches {
        roles,
        sh,
        focal,
        opp,
        root: src.search_root.clone(),
        lineage: sh.spec.lineage.clone(),
        used: 0,
        records: Vec::new(),
        mode: run.mode,
        keys: KeyStats::default(),
    };
    let (first, kept) = if run.opening {
        // From an ordinary opening: the initial search waits for the first
        // focal decision with two or more legal actions.
        (Tree::default(), None)
    } else {
        let anchor_step = decision(anchor).ok_or("anchor is terminal")?.step;
        match anchor_tree {
            Some((tree, record)) => {
                searches.used = allocation.anchor;
                searches.records.push(record);
                (tree.clone(), None)
            }
            None => match searches.run(anchor, allocation.anchor, anchor_step, "anchor")? {
                Some(tree) => {
                    let kept = (allocation == Allocation::R).then(|| tree.clone());
                    (tree, kept)
                }
                None => {
                    return Ok((
                        json!({"valid":false,"invalid_reason":"failed search","searches":searches.records}),
                        None,
                    ))
                }
            },
        }
    };
    let mut cursor = Cursor::new(first, 0, run.mode);
    let mut cursor_keys = KeyStats::default();
    let mut suffix = run.opening.then(|| super::labels::Suffix::new(true));
    live_focal.reset_sampling_v1(src.guide_policy_seeds);
    live_opp.reset_sampling_v1(src.guide_policy_seeds);
    let mut s = anchor.clone();
    let mut meter = Meter::new(u64::MAX);
    let mut c = Counters::default();
    let (mut actions, mut decisions) = (Vec::new(), Vec::new());
    let mut invalid: Option<String> = None;
    // Scheduler counters: focal nonforced substeps so far and the one of the
    // last search.
    let (mut nonforced, mut last_search_at) = (0u64, None::<u64>);
    while let Some(d) = decision(&s) {
        let a = if acting(&d) == focal && d.legal_action_count >= 2 {
            let remaining_before = allocation.total.saturating_sub(searches.used);
            let (mut lookup, mut hit) = cursor.lookup(&s, d)?;
            let mut refreshed = None;
            let mut scheduled = None;
            let step = if let Some(sc) = scheduler.as_deref_mut() {
                use super::allocator::{sched_rule, Counts, SchedStep};
                match sched_rule(lookup, remaining_before, false) {
                    SchedStep::Execute(e) => Step::Execute(e),
                    SchedStep::Plain => Step::Plain,
                    SchedStep::Consult => {
                        let counts = Counts {
                            remaining: remaining_before,
                            cap: sc.cap,
                            nonforced,
                            since_search: last_search_at.map(|i| nonforced - i),
                            searches: searches.records.len() as u64,
                        };
                        let (amount, record) = sc.consult(&s, focal, lookup, counts)?;
                        scheduled = Some(record);
                        if amount > 0 {
                            Step::Refresh(amount)
                        } else {
                            Step::Plain
                        }
                    }
                }
            } else if run.opening && searches.records.is_empty() {
                // The initial search: min(initial allocation, ceiling).
                Step::Refresh(allocation.anchor.min(allocation.total))
            } else {
                rule(lookup, allocation, searches.used, false)
            };
            if let Step::Refresh(budget) = step {
                let reason = if scheduled.is_some() {
                    match lookup {
                        Lookup::Unsupported => "scheduled_unsupported",
                        Lookup::DepthLimit => "scheduled_depth_limit",
                        _ => "scheduled_miss",
                    }
                } else if searches.records.is_empty() {
                    "initial"
                } else {
                    lookup.reason()
                };
                last_search_at = Some(nonforced);
                match searches.run(&s, budget, d.step, reason)? {
                    Some(tree) => {
                        cursor_keys.merge(&cursor.keyer.stats);
                        cursor = Cursor::new(tree, searches.records.len() - 1, run.mode);
                        refreshed = Some(reason);
                        (lookup, hit) = cursor.lookup(&s, d)?;
                    }
                    None => {
                        invalid = Some(format!("failed search at step {}", d.step));
                        break;
                    }
                }
            }
            let new_root = cursor.depth == 1 && !cursor.advanced;
            let mut edge_backups = None;
            let (a, chooser) = match rule(lookup, allocation, searches.used, true) {
                Step::Execute(e) => {
                    let h = hit.as_ref().expect("a supported lookup has a node");
                    edge_backups = h.backups;
                    (
                        cursor.live(h, e)?,
                        if new_root { "new_root" } else { "reused" },
                    )
                }
                _ => (act_plain(live_focal, &s, &d, &mut c)?, "plain"),
            };
            if let Some(h) = hit {
                if cursor.matching {
                    cursor.advance(h, a);
                }
            }
            let sem = s.diagnostic_current_action_semantics().unwrap_or_default();
            let (mut cast_offered, mut own_offered, mut dr_offered) = (false, false, false);
            for x in &sem {
                let (cast, own, dr) = classify(x, s.game_state(), focal, &defs);
                cast_offered |= cast;
                own_offered |= own;
                dr_offered |= dr;
            }
            let (cast_chosen, own_chosen, dr_chosen) =
                sem.get(a as usize).map_or((false, false, false), |x| {
                    classify(x, s.game_state(), focal, &defs)
                });
            // The passive sequence tracker watches a chosen Spy cast or
            // self-target through the next transition (the same rule as
            // selection, frozen execution and label replay).
            super::play::observe(suffix.as_mut(), &s, a, focal, &defs);
            let mut row = json!({"step":d.step,"physical_decision_id":d.physical_decision_id,
                "substep":d.substep_index,"k":d.legal_action_count,"chooser":chooser,
                "search":cursor.search,"lookup":lookup.reason(),"refreshed":refreshed,
                "edge_backups":edge_backups,"remaining_before":remaining_before,
                "remaining_after":allocation.total.saturating_sub(searches.used),
                "spy_cast_offered":cast_offered,"spy_cast_chosen":cast_chosen,
                "spy_self_offered":own_offered,"spy_self_chosen":own_chosen,
                "dr_giant_offered":dr_offered,"dr_giant_chosen":dr_chosen});
            if scheduler.is_some() {
                row["scheduler"] = scheduled.unwrap_or(Value::Null);
            }
            decisions.push(row);
            nonforced += 1;
            a
        } else if acting(&d) == focal {
            0
        } else {
            act_plain(live_opp, &s, &d, &mut c)?
        };
        actions.push(a);
        apply(&mut s, &d, a, &mut meter, focal, suffix.as_mut(), &defs).map_err(err)?;
    }
    cursor_keys.merge(&cursor.keyer.stats);
    let end = if invalid.is_some() {
        None
    } else {
        outcome(&s, focal)
    };
    let (starts, _) = if invalid.is_none() {
        guide_record(anchor, focal, &actions)?
    } else {
        (Vec::new(), Vec::new())
    };
    let valid = invalid.is_none() && end.is_some_and(|e| e != "nonnatural");
    Ok((
        json!({"valid":valid,"invalid_reason":invalid.or_else(|| (!valid).then(|| "non-natural guide".to_owned())),
            "end":end,"actions":actions,"decisions":decisions,"learner_starts":starts.len(),
            "searches":searches.records,"transitions_used":searches.used,
            "key_mode":run.mode.label(),"search_keys":searches.keys.json(),"execution_keys":cursor_keys.json(),
            "sequence":suffix.as_ref().map(|x| x.json()),"real_transitions":meter.spent,
            "wall":started.elapsed().as_secs_f64()}),
        kept,
    ))
}

/// One total-budget arm-game from an ordinary opening (collab
/// LANES/search-budget-discovery-plan-20261011): 128,000 initially at the
/// first focal choice with two or more legal actions, then 64,000 per
/// support-triggered refresh, within `total`. Seeds come from the case, never
/// from the ceiling.
pub(super) fn budget_case(
    sh: &BankShared,
    policy: &FrozenPlayPolicyV1,
    src: &SourceSpec,
    total: u64,
    mode: KeyMode,
    binding: &str,
) -> Result<Value, String> {
    let started = Instant::now();
    let opp = sh
        .labels
        .iter()
        .position(|l| *l == src.opp)
        .ok_or("unknown guide opponent")?;
    let mut roles = bank_roles(policy, &sh.opponents)?;
    let mut live_focal = policy.fork_for_collection_v3()?;
    let mut live_opp = sh.opponents[opp].fork_for_collection_v3()?;
    let opening = source_session(&sh.spec, src)?;
    let allocation = Allocation {
        anchor: ANCHOR_BUDGET,
        refresh: REFRESH_BUDGET,
        total,
    };
    let (rec, _) = guide(
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
            mode,
        },
        None,
    )?;
    Ok(
        json!({"kind":"budget_game","schema":"search-total-budget-v1/game","id":src.id,
        "lineage":sh.spec.lineage,"own":src.own,"other":src.other,"seat":src.seat,"opp":src.opp,
        "seed":src.seed,"starting_player":src.starting_player,"live_policy_seeds":src.guide_policy_seeds,
        "search_root":src.search_root,"bank_binding_sha256":binding,
        "allocation":{"initial":allocation.anchor,"refresh":allocation.refresh,"total":allocation.total},
        "game":rec,"wall":started.elapsed().as_secs_f64()}),
    )
}

/// Replay check of one recorded budget game (analysis only): rebuilds the
/// case's opening, applies the recorded actions for both seats with the
/// passive sequence tracker (the same observation rule as live play), and
/// returns the replayed sequence labels and outcome. Nothing is searched.
pub(super) fn budget_replay(
    sh: &BankShared,
    src: &SourceSpec,
    actions: &[u32],
) -> Result<Value, String> {
    let focal = PlayerId(src.seat);
    let defs = spy_defs();
    let mut s = source_session(&sh.spec, src)?;
    let mut suffix = super::labels::Suffix::new(true);
    let mut meter = Meter::new(u64::MAX);
    for &a in actions {
        let d = decision(&s).ok_or("replay reached a terminal early")?;
        if acting(&d) == focal && d.legal_action_count >= 2 {
            super::play::observe(Some(&mut suffix), &s, a, focal, &defs);
        }
        apply(&mut s, &d, a, &mut meter, focal, Some(&mut suffix), &defs).map_err(err)?;
    }
    // Focal physical decision starts (with replay identities) for reset cuts.
    let (starts, witnesses) = guide_record(&source_session(&sh.spec, src)?, focal, actions)?;
    Ok(
        json!({"id":src.id,"end":outcome(&s, focal),"sequence":suffix.json(),"actions":actions.len(),
        "starts":starts,"witnesses":witnesses}),
    )
}

/// Rebuilds the bank's anchor for `src` and plays the requested arms.
pub(super) fn source_arms(
    sh: &BankShared,
    policy: &FrozenPlayPolicyV1,
    src: &SourceSpec,
    parity: bool,
    binding: &str,
) -> Result<Value, String> {
    let started = Instant::now();
    let spec = &sh.spec;
    let focal = PlayerId(src.seat);
    let opp = sh
        .labels
        .iter()
        .position(|l| *l == src.opp)
        .ok_or("unknown guide opponent")?;
    let mut roles = bank_roles(policy, &sh.opponents)?;
    let mut live_focal = policy.fork_for_collection_v3()?;
    let mut live_opp = sh.opponents[opp].fork_for_collection_v3()?;
    // Source game and anchor, exactly as the bank built them.
    let mut s = source_session(spec, src)?;
    live_focal.reset_sampling_v1(src.source_policy_seeds);
    live_opp.reset_sampling_v1(src.source_policy_seeds);
    let mut source_actions = Vec::new();
    plain_logged(
        &mut s,
        focal,
        &mut live_focal,
        &mut live_opp,
        &mut source_actions,
    )?;
    let mut eligible: Vec<u64> = Vec::new();
    walk(source_session(spec, src)?, &source_actions, |_, d, _| {
        if acting(d) == focal && d.substep_index == 0 && d.legal_action_count >= 2 {
            eligible.push(d.step);
        }
        Ok(())
    })?;
    if eligible.is_empty() {
        return Err("source has no anchor".into());
    }
    let index = ((u128::from(src.anchor_draw) * eligible.len() as u128) >> 64) as usize;
    let anchor = walk(
        source_session(spec, src)?,
        &source_actions[..eligible[index] as usize],
        |_, _, _| Ok(()),
    )?;
    let mut row = super::bank::source_identity(
        sh,
        src,
        &s,
        &source_actions,
        &anchor,
        index,
        eligible.len(),
        binding,
    )?;
    let mut arms = serde_json::Map::new();
    if parity {
        let (rec, _) = guide(
            sh,
            &mut roles,
            &mut live_focal,
            &mut live_opp,
            &anchor,
            src,
            opp,
            Allocation::S512,
            None,
            GuideRun::ANCHOR_LEGACY,
            None,
        )?;
        arms.insert("S512".into(), rec);
    } else {
        let (rec, kept) = guide(
            sh,
            &mut roles,
            &mut live_focal,
            &mut live_opp,
            &anchor,
            src,
            opp,
            Allocation::R,
            None,
            GuideRun::ANCHOR_LEGACY,
            None,
        )?;
        let anchor_record = rec["searches"][0].clone();
        arms.insert("R".into(), rec);
        // S128 executes R's own anchor tree (same completed backups), never refreshed.
        let rec = match kept {
            Some(tree) => {
                guide(
                    sh,
                    &mut roles,
                    &mut live_focal,
                    &mut live_opp,
                    &anchor,
                    src,
                    opp,
                    Allocation::S128,
                    Some((tree, anchor_record)),
                    GuideRun::ANCHOR_LEGACY,
                    None,
                )?
                .0
            }
            None => json!({"valid":false,"invalid_reason":"failed anchor search"}),
        };
        arms.insert("S128".into(), rec);
    }
    row["kind"] = json!("refresh_source");
    row["anchor_step"] = json!(eligible[index]);
    row["allocation"] =
        json!({"total":TOTAL_BUDGET,"anchor":ANCHOR_BUDGET,"refresh":REFRESH_BUDGET});
    row["arms"] = json!(arms);
    row["wall"] = json!(started.elapsed().as_secs_f64());
    Ok(row)
}

#[cfg(test)]
mod tests {
    use super::super::tree::canon;
    use super::*;

    #[test]
    fn supported_actions_execute_without_refresh() {
        assert_eq!(
            rule(Lookup::Supported(3), Allocation::R, 0, false),
            Step::Execute(3)
        );
        assert_eq!(
            rule(Lookup::Supported(1), Allocation::S128, 128_000, true),
            Step::Execute(1)
        );
    }

    #[test]
    fn each_support_end_refreshes_once_then_falls_back() {
        for l in [Lookup::Miss, Lookup::Unsupported, Lookup::DepthLimit] {
            assert_eq!(
                rule(l, Allocation::R, 128_000, false),
                Step::Refresh(64_000)
            );
            assert_eq!(
                rule(l, Allocation::R, 128_000, true),
                Step::Plain,
                "no double refresh"
            );
            assert_eq!(rule(l, Allocation::S128, 128_000, false), Step::Plain);
            assert_eq!(rule(l, Allocation::S512, 512_000, false), Step::Plain);
        }
    }

    #[test]
    fn budget_is_capped_and_exhaustion_is_plain() {
        let mut used = ANCHOR_BUDGET;
        let mut refreshes = 0;
        while let Step::Refresh(b) = rule(Lookup::Miss, Allocation::R, used, false) {
            used += b;
            refreshes += 1;
            assert!(used <= TOTAL_BUDGET);
        }
        assert_eq!((refreshes, used), (6, TOTAL_BUDGET));
        assert_eq!(
            rule(Lookup::Unsupported, Allocation::R, used, false),
            Step::Plain
        );
        // A partial remainder is spent, never exceeded.
        assert_eq!(Allocation::R.next_refresh(500_000), 12_000);
    }

    /// A ceiling that is not a multiple of the refresh size ends with one
    /// partial refresh that spends the remainder exactly.
    #[test]
    fn budget_ceiling_ends_with_a_partial_refresh() {
        let a = Allocation {
            anchor: ANCHOR_BUDGET,
            refresh: REFRESH_BUDGET,
            total: 300_000,
        };
        let mut used = ANCHOR_BUDGET;
        let mut sizes = vec![];
        while let Step::Refresh(b) = rule(Lookup::Miss, a, used, false) {
            used += b;
            sizes.push(b);
        }
        assert_eq!(sizes, [64_000, 64_000, 44_000]);
        assert_eq!(used, 300_000);
        assert_eq!(rule(Lookup::Miss, a, used, false), Step::Plain);
    }

    use super::super::world::{sample, DeckPrior};
    use crate::runtime_decks::RUNTIME_DECKS;

    fn game(seed: u64) -> FastActorSessionV1 {
        let decks = [&RUNTIME_DECKS[4], &RUNTIME_DECKS[5]];
        let mut s = FastActorSessionV1::reset_with_explicit_decks_and_limits_flat_action_v3_environment_v2_with_starting_player_v1(
            1, seed, 100_000, 200_000, decks.map(|d| d.id.to_owned()), decks.map(|d| d.card_ids.to_vec()), PlayerId(0),
        )
        .unwrap();
        s.set_runtime_rules_v1(crate::engine::RuntimeRulesV1::RESOLUTION_BOUNDARY_V1)
            .unwrap();
        s
    }

    /// Plays first legal actions until `stop` accepts a decision.
    fn advance(
        s: &mut FastActorSessionV1,
        stop: impl Fn(&crate::rl_session::FastActorDecisionV1) -> bool,
    ) -> crate::rl_session::FastActorDecisionV1 {
        loop {
            let d = decision(s).expect("game ended before the wanted decision");
            if stop(&d) {
                return d;
            }
            s.step(d.episode_id, d.step, 0).unwrap();
        }
    }

    fn roles(policy: &FrozenPlayPolicyV1) -> Roles {
        bank_roles(policy, std::slice::from_ref(policy)).unwrap()
    }

    /// Live plain and opponent policies are separate from the search roles:
    /// a search between two live decisions leaves the live draws exactly as
    /// a reference with no search.
    #[test]
    #[ignore = "native qualification: fixture games and a small search"]
    fn search_leaves_live_policy_sampling_unchanged() {
        let policy = FrozenPlayPolicyV1::training_fixture_v3();
        let mut s = game(11);
        let d = advance(&mut s, |d| {
            d.legal_action_count >= 2 && d.acting_player == crate::rl::PlayerSeatV1::P0
        });
        let (mut live, mut reference) = (
            policy.fork_for_collection_v3().unwrap(),
            policy.fork_for_collection_v3().unwrap(),
        );
        live.reset_sampling_v1([7, 9]);
        reference.reset_sampling_v1([7, 9]);
        let first = (
            live.select_fast_session_v1(&s).unwrap(),
            reference.select_fast_session_v1(&s).unwrap(),
        );
        assert_eq!(first.0, first.1);
        let mut roles = roles(&policy);
        let prior = DeckPrior::new(&(0..RUNTIME_DECKS.len()).collect::<Vec<_>>());
        let seeds = RootSeeds {
            model: "test".into(),
            root: "isolation".into(),
        };
        let limits = Limits {
            select_cap: 3_000,
            ..Limits::FORMAL
        };
        let ctx = guide_ctx(&s, PlayerId(0), 0, &seeds, &prior, limits).unwrap();
        let (sel, _) = roles.select_e(&ctx);
        assert!(sel.transitions > 0);
        let _ = d;
        // Twenty further live draws equal the no-search reference.
        let mut a = s.clone();
        for _ in 0..20 {
            let Some(d) = decision(&a) else { break };
            let x = live.select_fast_session_v1(&a).unwrap();
            let y = reference.select_fast_session_v1(&a).unwrap();
            assert_eq!(x, y, "search changed a live sampling stream");
            a.step(d.episode_id, d.step, x).unwrap();
        }
    }

    /// A refresh root at a nonzero substep samples only the actor's
    /// information set: the sampled world shows the actor the same
    /// canonical observation and menu, and a search runs from it.
    #[test]
    #[ignore = "native qualification: fixture games and a small search"]
    fn nonzero_substep_root_samples_lawfully() {
        let policy = FrozenPlayPolicyV1::training_fixture_v3();
        let mut found = None;
        for seed in 1..40u64 {
            let mut s = game(seed);
            let mut steps = 0;
            while let Some(d) = decision(&s) {
                if d.substep_index > 0 && d.legal_action_count >= 2 {
                    found = Some((s.clone(), d));
                    break;
                }
                steps += 1;
                if steps > 4_000 {
                    break;
                }
                s.step(d.episode_id, d.step, 0).unwrap();
            }
            if found.is_some() {
                break;
            }
        }
        let (s, d) = found.expect("a nonzero-substep decision in a fixture game");
        let prior = DeckPrior::new(&(0..RUNTIME_DECKS.len()).collect::<Vec<_>>());
        let actual = canon(&s, d).unwrap();
        let mut accepted = 0;
        for seed in 0..16u64 {
            if let Ok(w) = sample(&s, seed, &prior) {
                let wd = decision(&w.world).unwrap();
                let c = canon(&w.world, wd).unwrap();
                assert_eq!(
                    (wd.substep_index, &c.obs, &c.menu),
                    (d.substep_index, &actual.obs, &actual.menu)
                );
                accepted += 1;
            }
        }
        assert!(accepted > 0, "no lawful sample accepted at the root");
        let mut roles = roles(&policy);
        let seeds = RootSeeds {
            model: "test".into(),
            root: "substep".into(),
        };
        let limits = Limits {
            select_cap: 3_000,
            ..Limits::FORMAL
        };
        let focal = acting(&d);
        let ctx = guide_ctx(&s, focal, 0, &seeds, &prior, limits).unwrap();
        let (sel, tree) = roles.select_e(&ctx);
        assert!(!sel.incomplete && sel.faults.is_empty() && !tree.nodes.is_empty());
    }
}
