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
use super::reuse_witness::{Recording, Selected};
use super::seeds::RootSeeds;
use super::tree::{live_index, Found, Tree, MAX_DEPTH};
use crate::ids::PlayerId;
use crate::rl_session::FastActorSessionV1;
use crate::sideboard_play_policy_v1::FrozenPlayPolicyV1;
use serde_json::{json, Value};
use std::time::Instant;

mod execution;
pub(super) use execution::ExecutionMode;

pub(super) const TOTAL_BUDGET: u64 = 512_000;
pub(super) const ANCHOR_BUDGET: u64 = 128_000;
pub(super) const REFRESH_BUDGET: u64 = 64_000;
/// Row label of the opt-in interchangeable-choice merging (`interchange.rs`).
pub(super) const INTERCHANGE_RULE: &str = "interchange-orbit-v1";

/// Row label of the opt-in best-line backup with threshold `m` (`tree.rs`).
pub(super) fn bestline_rule(m: u64) -> String {
    format!("bestline-m{m}")
}

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
    /// Merge interchangeable choices (`interchange.rs`) in every search and
    /// in the cursor that walks its tree.
    pub(super) interchange: bool,
    /// Best-line backup threshold in every search (`tree.rs` `BestLine`);
    /// None is the formal mean backup.
    pub(super) bestline: Option<u64>,
    /// Passive actual reused-decision evidence; never changes execution.
    pub(super) reuse_witnesses: Recording,
    /// Opt-in root-only execution, independently of search allocation.
    pub(super) executor: ExecutionMode,
}

impl GuideRun {
    pub(super) const ANCHOR_LEGACY: Self = Self {
        opening: false,
        mode: KeyMode::Legacy,
        interchange: false,
        bestline: None,
        reuse_witnesses: Recording::Disabled,
        executor: ExecutionMode::Persistent,
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
    /// The matched node's key (Supported or Unsupported lookups).
    key: Option<super::tree::Key>,
}

impl Cursor {
    fn new(tree: Tree, search: usize, mode: KeyMode, interchange: bool) -> Self {
        Self {
            tree,
            keyer: Keyer::new(mode, KeyStats::default()).with_interchange(interchange),
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
        let key = (found != Lookup::Miss).then_some(key);
        Ok((
            found,
            Some(Hit {
                step: st,
                backups,
                key,
            }),
        ))
    }

    /// The live candidate of storage edge `e` at a looked-up decision.
    fn live(&self, hit: &Hit, e: usize) -> Result<u32, String> {
        let (_, _, _, cn) = self.keyer.storage(&hit.step);
        live_index(cn, &cn.edges[e]).map_err(err)
    }

    /// Read the already matched storage mapping before advance consumes it.
    fn reuse_witness(
        &self,
        recording: Recording,
        session: &FastActorSessionV1,
        hit: &Hit,
        edge: usize,
        applied: u32,
        searches: &[Value],
    ) -> Result<Option<Value>, String> {
        if recording == Recording::Disabled {
            return Ok(None);
        }
        let record = searches
            .get(self.search)
            .ok_or("reused-decision search record missing")?;
        let (key, parent, parent_edge, canon) = self.keyer.storage(&hit.step);
        if hit.key != Some(key) {
            return Err("reused-decision matched node key differs".into());
        }
        recording.capture(
            || super::bank::decision_identity(session),
            self.search,
            record,
            Selected {
                tree: &self.tree,
                key,
                parent,
                parent_edge,
                canon,
                edge,
                backups: hit.backups,
                applied,
            },
        )
    }

    /// Advances along the chosen live action (a matched node, any chooser).
    fn advance(&mut self, hit: Hit, live: u32) {
        self.keyer.advance(hit.step, live);
        self.advanced = true;
    }
}

struct Searches<'a> {
    #[cfg(feature = "viewer-recording-v1")]
    observer: Option<&'a mut super::viewer_uct::Recorder>,
    roles: &'a mut Roles,
    sh: &'a BankShared,
    focal: PlayerId,
    opp: usize,
    root: String,
    lineage: String,
    used: u64,
    records: Vec<Value>,
    mode: KeyMode,
    interchange: bool,
    bestline: Option<u64>,
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
        ctx.interchange = self.interchange;
        ctx.bestline = self.bestline;
        let started = Instant::now();
        let mut diag = super::arms::SearchDiag::default();
        #[cfg(feature = "viewer-recording-v1")]
        let mut trace = self
            .observer
            .as_ref()
            .filter(|r| r.error.is_none() && k < 3)
            .map(|_| {
                let mut trace = super::diag::Trace::default();
                trace.viewer = Some(super::viewer_uct::SearchCapture::default());
                trace
            });
        #[cfg(not(feature = "viewer-recording-v1"))]
        let mut trace = None;
        #[cfg(feature = "viewer-recording-v1")]
        self.roles.set_inference_probes(trace.is_some())?;
        let (sel, tree) =
            self.roles
                .select_e_diagnosed(&ctx, trace.as_mut(), 1, None, None, Some(&mut diag));
        #[cfg(feature = "viewer-recording-v1")]
        self.roles.set_inference_probes(false)?;
        self.keys.merge(&diag.keys);
        self.used += sel.transitions;
        let failed = sel.incomplete || !sel.faults.is_empty();
        self.records.push(json!({"search":k,"step":at_step,"reason":reason,"budget":budget,
            "root":super::bank::decision_identity(s)?,
            "transitions":sel.transitions,"completed_natural":sel.completed,"attempted":sel.attempted,
            "nonnatural":sel.nonnatural,"incomplete":sel.incomplete,"faults":sel.faults,
            "tree":sel.extra["tree"].clone(),"keys":diag.keys.json(),"wall":started.elapsed().as_secs_f64()}));
        if !diag.key_memo.is_null() {
            if let Some(r) = self.records.last_mut() {
                r["key_memo"] = diag.key_memo;
            }
        }
        if !diag.inference_cache.is_null() {
            if let Some(r) = self.records.last_mut() {
                r["inference_cache"] = diag.inference_cache;
            }
        }
        #[cfg(feature = "viewer-recording-v1")]
        if let Some(observer) = self.observer.as_deref_mut() {
            let mut record = self.records.last().unwrap().clone();
            record["key"] =
                serde_json::to_value(crate::rl::PlayerSeatV1::from(self.focal)).unwrap();
            let d = decision(s).unwrap();
            record["decision"] = json!({"episode_id":d.episode_id,"step":d.step,"physical_decision_id":d.physical_decision_id,"substep_index":d.substep_index,"acting_player":d.acting_player});
            observer.search(k, &record, trace.and_then(|mut t| t.viewer.take()));
        }
        Ok((!failed).then_some(tree))
    }
}

/// One arm's guide from the anchor. Returns the arm record.
#[allow(clippy::too_many_arguments, reason = "explicit roles and identities")]
fn guide(
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
) -> Result<(Value, Option<Tree>), String> {
    guide_inner(
        sh,
        roles,
        live_focal,
        live_opp,
        anchor,
        src,
        opp,
        allocation,
        anchor_tree,
        run,
        #[cfg(feature = "viewer-recording-v1")]
        None,
        #[cfg(feature = "viewer-recording-v1")]
        None,
        #[cfg(feature = "viewer-recording-v1")]
        None,
    )
}

#[cfg(feature = "viewer-recording-v1")]
#[allow(
    clippy::too_many_arguments,
    reason = "same guide with passive viewer observer"
)]
pub(super) fn guide_viewer(
    sh: &BankShared,
    roles: &mut Roles,
    live_focal: &mut FrozenPlayPolicyV1,
    live_opp: &mut FrozenPlayPolicyV1,
    anchor: &FastActorSessionV1,
    src: &SourceSpec,
    observer: Option<&mut super::viewer_uct::Recorder>,
    max_step: u64,
) -> Result<Value, String> {
    let mut actual_core_hash = 0u64;
    guide_inner(
        sh,
        roles,
        live_focal,
        live_opp,
        anchor,
        src,
        0,
        Allocation::R,
        None,
        GuideRun::ANCHOR_LEGACY,
        observer,
        Some(max_step),
        Some(&mut actual_core_hash),
    )
    .map(|(mut v, _)| {
        v["actual_core_hash"] = json!(actual_core_hash);
        v
    })
}

#[allow(
    clippy::too_many_arguments,
    reason = "existing guide roles and optional passive viewer sink"
)]
fn guide_inner(
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
    #[cfg(feature = "viewer-recording-v1")] observer: Option<&mut super::viewer_uct::Recorder>,
    #[cfg(feature = "viewer-recording-v1")] max_step: Option<u64>,
    #[cfg(feature = "viewer-recording-v1")] actual_core_hash: Option<&mut u64>,
) -> Result<(Value, Option<Tree>), String> {
    run.executor.validate_budget(
        false,
        run.mode,
        run.interchange,
        run.bestline,
        run.reuse_witnesses,
    )?;
    if run.executor == ExecutionMode::RootOnly && !run.opening {
        return Err("root-only-v1 runs from ordinary openings only".into());
    }
    let started = Instant::now();
    let focal = PlayerId(src.seat);
    let defs = spy_defs();
    let mut searches = Searches {
        #[cfg(feature = "viewer-recording-v1")]
        observer,
        roles,
        sh,
        focal,
        opp,
        root: src.search_root.clone(),
        lineage: sh.spec.lineage.clone(),
        used: 0,
        records: Vec::new(),
        mode: run.mode,
        interchange: run.interchange,
        bestline: run.bestline,
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
    if run.interchange && !run.opening {
        // A bank anchor tree was built without merging.
        return Err("interchangeable-choice merging runs from openings only".into());
    }
    let mut cursor = Cursor::new(first, 0, run.mode, run.interchange);
    let mut cursor_keys = KeyStats::default();
    let mut suffix = run.opening.then(|| super::labels::Suffix::new(true));
    live_focal.reset_sampling_v1(src.guide_policy_seeds);
    live_opp.reset_sampling_v1(src.guide_policy_seeds);
    let mut s = anchor.clone();
    let mut meter = Meter::new(u64::MAX);
    let mut c = Counters::default();
    let (mut actions, mut decisions) = (Vec::new(), Vec::new());
    let mut invalid: Option<String> = None;
    // Passive (`pathlog.rs`): the actual history's nodes in each tree.
    let mut actual = super::pathlog::dir().map(|_| super::pathlog::ActualLog::default());
    while let Some(d) = decision(&s) {
        #[cfg(feature = "viewer-recording-v1")]
        if max_step.is_some_and(|limit| d.step >= limit) {
            invalid = Some("viewer development step bound".into());
            break;
        }
        #[cfg(feature = "viewer-recording-v1")]
        if let Some(observer) = searches.observer.as_deref_mut() {
            observer.before(&s, d);
        }
        let a = if acting(&d) == focal && d.legal_action_count >= 2 {
            let remaining_before = allocation.total.saturating_sub(searches.used);
            let (mut lookup, mut hit) = cursor.lookup(&s, d)?;
            let mut refreshed = None;
            let step = if run.opening && searches.records.is_empty() {
                // The initial search: min(initial allocation, ceiling).
                Step::Refresh(allocation.anchor.min(allocation.total))
            } else {
                rule(lookup, allocation, searches.used, false)
            };
            if let Step::Refresh(budget) = step {
                let reason = if searches.records.is_empty() {
                    "initial"
                } else {
                    lookup.reason()
                };
                match searches.run(&s, budget, d.step, reason)? {
                    Some(tree) => {
                        cursor_keys.merge(&cursor.keyer.stats);
                        cursor = Cursor::new(
                            tree,
                            searches.records.len() - 1,
                            run.mode,
                            run.interchange,
                        );
                        refreshed = Some(reason);
                        (lookup, hit) = cursor.lookup(&s, d)?;
                    }
                    None => {
                        invalid = Some(format!("failed search at step {}", d.step));
                        break;
                    }
                }
            }
            if let (Some(log), Some(Hit { key: Some(k), .. })) = (actual.as_mut(), hit.as_ref()) {
                log.hit(cursor.search, d.step, k);
            }
            let new_root = cursor.depth == 1 && !cursor.advanced;
            let route = execution::route(
                run.executor,
                new_root,
                rule(lookup, allocation, searches.used, true),
            );
            let mut edge_backups = None;
            let suppressed_reuse = match route {
                execution::Route::RawInsteadOfReuse(edge) => {
                    let h = hit.as_ref().expect("a supported lookup has a node");
                    Some(
                        json!({"storage_edge_index":edge,"live_action":cursor.live(h, edge)?,
                        "natural_terminal_backups":h.backups}),
                    )
                }
                _ => None,
            };
            let a = execution::choose(
                route,
                |edge| {
                    let h = hit.as_ref().expect("a supported lookup has a node");
                    edge_backups = h.backups;
                    cursor.live(h, edge)
                },
                || act_plain(live_focal, &s, &d, &mut c),
            )?;
            let chooser = route.chooser(new_root);
            // Along a matching history an interchangeable copy plays as its
            // representative, so the cursor stays on the stored edge.
            let a = match &hit {
                Some(h) if cursor.matching => cursor.keyer.representative(&h.step, a),
                _ => a,
            };
            let reuse_witness = if chooser == "reused" {
                let h = hit.as_ref().expect("a reused execution has a matched node");
                let Lookup::Supported(edge) = lookup else {
                    return Err("reused execution lacks a supported storage edge".into());
                };
                cursor.reuse_witness(run.reuse_witnesses, &s, h, edge, a, &searches.records)?
            } else {
                None
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
            decisions.push(
                json!({"step":d.step,"physical_decision_id":d.physical_decision_id,
                "substep":d.substep_index,"k":d.legal_action_count,"chooser":chooser,
                "search":cursor.search,"lookup":lookup.reason(),"refreshed":refreshed,
                "edge_backups":edge_backups,"remaining_before":remaining_before,
                "remaining_after":allocation.total.saturating_sub(searches.used),
                "spy_cast_offered":cast_offered,"spy_cast_chosen":cast_chosen,
                "spy_self_offered":own_offered,"spy_self_chosen":own_chosen,
                "dr_giant_offered":dr_offered,"dr_giant_chosen":dr_chosen}),
            );
            if run.executor == ExecutionMode::RootOnly {
                let record = decisions.last_mut().expect("just recorded decision");
                record["executor"] = json!(execution::ROOT_ONLY_IDENTITY);
                if let Some(candidate) = suppressed_reuse {
                    record["suppressed_reuse"] = candidate;
                }
            }
            if let Some(witness) = reuse_witness {
                decisions.last_mut().expect("just recorded decision")["reuse_witness"] = witness;
            }
            a
        } else if acting(&d) == focal {
            0
        } else {
            act_plain(live_opp, &s, &d, &mut c)?
        };
        actions.push(a);
        apply(&mut s, &d, a, &mut meter, focal, suffix.as_mut(), &defs).map_err(err)?;
        #[cfg(feature = "viewer-recording-v1")]
        if let Some(observer) = searches.observer.as_deref_mut() {
            let evidence = decisions
                .last()
                .filter(|r| r["step"] == d.step)
                .cloned()
                .unwrap_or(Value::Null);
            observer.after(&s, a, evidence);
        }
    }
    cursor_keys.merge(&cursor.keyer.stats);
    #[cfg(feature = "viewer-recording-v1")]
    if let Some(hash) = actual_core_hash {
        *hash = s.privileged_core_environment_hash();
    }
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
    if let (Some(log), Some(dir)) = (actual, super::pathlog::dir()) {
        let roots: Vec<Value> = searches
            .records
            .iter()
            .enumerate()
            .map(|(k, r)| {
                json!({"search":k,"step":r["step"],"reason":r["reason"],
                    "tree_hash":r["tree"]["hash"],"budget":r["budget"]})
            })
            .collect();
        let header = json!({"id":src.id,"lineage":sh.spec.lineage,"search_root":src.search_root,
            "seat":src.seat,"opp":src.opp,"end":end,"valid":valid,"searches":roots});
        if let Err(e) = log.write(&dir, header) {
            eprintln!("S4A_PATH_LOG: guide log not written: {e}");
        }
    }
    let mut game = json!({"valid":valid,"invalid_reason":invalid.or_else(|| (!valid).then(|| "non-natural guide".to_owned())),
            "end":end,"actions":actions,"decisions":decisions,"learner_starts":starts.len(),
            "searches":searches.records,"transitions_used":searches.used,
            "key_mode":run.mode.label(),"search_keys":searches.keys.json(),"execution_keys":cursor_keys.json(),
            "sequence":suffix.as_ref().map(|x| x.json()),"real_transitions":meter.spent,
            "wall":started.elapsed().as_secs_f64()});
    run.reuse_witnesses.annotate(&mut game);
    Ok((game, kept))
}

/// One total-budget arm-game from an ordinary opening (collab
/// LANES/search-budget-discovery-plan-20261011): 128,000 initially at the
/// first focal choice with two or more legal actions, then 64,000 per
/// support-triggered refresh, within `total`. Seeds come from the case, never
/// from the ceiling.
#[allow(
    clippy::too_many_arguments,
    reason = "explicit case and opt-in search controls"
)]
pub(super) fn budget_case(
    sh: &BankShared,
    policy: &FrozenPlayPolicyV1,
    src: &SourceSpec,
    total: u64,
    mode: KeyMode,
    interchange: bool,
    bestline: Option<u64>,
    reuse_witnesses: Recording,
    binding: &str,
) -> Result<Value, String> {
    budget_case_with_executor(
        sh,
        policy,
        src,
        total,
        mode,
        interchange,
        bestline,
        reuse_witnesses,
        binding,
        ExecutionMode::Persistent,
    )
}

/// Explicit sibling; legacy callers never select an alternate executor.
#[allow(
    clippy::too_many_arguments,
    reason = "versioned budget player contract"
)]
pub(super) fn budget_case_with_executor(
    sh: &BankShared,
    policy: &FrozenPlayPolicyV1,
    src: &SourceSpec,
    total: u64,
    mode: KeyMode,
    interchange: bool,
    bestline: Option<u64>,
    reuse_witnesses: Recording,
    binding: &str,
    executor: ExecutionMode,
) -> Result<Value, String> {
    executor.validate_budget(false, mode, interchange, bestline, reuse_witnesses)?;
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
            interchange,
            bestline,
            reuse_witnesses,
            executor,
        },
    )?;
    let mut row = json!({"kind":"budget_game","schema":"search-total-budget-v1/game","id":src.id,
        "lineage":sh.spec.lineage,"own":src.own,"other":src.other,"seat":src.seat,"opp":src.opp,
        "seed":src.seed,"starting_player":src.starting_player,"live_policy_seeds":src.guide_policy_seeds,
        "search_root":src.search_root,"bank_binding_sha256":binding,
        "allocation":{"initial":allocation.anchor,"refresh":allocation.refresh,"total":allocation.total},
        "game":rec,"wall":started.elapsed().as_secs_f64()});
    executor.annotate_row(&mut row);
    // Present only when merging was on, so default rows are unchanged.
    if interchange {
        row["interchange"] = json!(INTERCHANGE_RULE);
    }
    if let Some(m) = bestline {
        row["backup"] = json!(bestline_rule(m));
    }
    Ok(row)
}

/// Versioned plain control for the ordinary-opening budget player. Both live
/// roles use exactly the search arm's opening and guide sampling streams.
/// No search roles, tree, initial search or refresh are constructed here.
/// The terminal replay still constructs the existing diagnostic replay keys.
pub(super) fn budget_plain_case(
    sh: &BankShared,
    policy: &FrozenPlayPolicyV1,
    src: &SourceSpec,
    binding: &str,
) -> Result<Value, String> {
    let started = Instant::now();
    let opp = sh
        .labels
        .iter()
        .position(|l| *l == src.opp)
        .ok_or("unknown guide opponent")?;
    let mut live_focal = policy.fork_for_collection_v3()?;
    let mut live_opp = sh.opponents[opp].fork_for_collection_v3()?;
    live_focal.reset_sampling_v1(src.guide_policy_seeds);
    live_opp.reset_sampling_v1(src.guide_policy_seeds);
    let focal = PlayerId(src.seat);
    let opening = source_session(&sh.spec, src)?;
    let mut s = opening.clone();
    let defs = spy_defs();
    let mut suffix = super::labels::Suffix::new(true);
    let mut meter = Meter::new(u64::MAX);
    let mut counters = Counters::default();
    let (mut actions, mut decisions) = (Vec::new(), Vec::new());
    while let Some(d) = decision(&s) {
        let a = if acting(&d) == focal {
            act_plain(&mut live_focal, &s, &d, &mut counters)?
        } else {
            act_plain(&mut live_opp, &s, &d, &mut counters)?
        };
        if acting(&d) == focal && d.legal_action_count >= 2 {
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
            super::play::observe(Some(&mut suffix), &s, a, focal, &defs);
            decisions.push(
                json!({"step":d.step,"physical_decision_id":d.physical_decision_id,
                "substep":d.substep_index,"k":d.legal_action_count,"chooser":"plain",
                "search":null,"lookup":null,"refreshed":null,"edge_backups":null,
                "remaining_before":0,"remaining_after":0,
                "spy_cast_offered":cast_offered,"spy_cast_chosen":cast_chosen,
                "spy_self_offered":own_offered,"spy_self_chosen":own_chosen,
                "dr_giant_offered":dr_offered,"dr_giant_chosen":dr_chosen}),
            );
        }
        actions.push(a);
        apply(&mut s, &d, a, &mut meter, focal, Some(&mut suffix), &defs).map_err(err)?;
    }
    let end = outcome(&s, focal);
    let valid = end.is_some_and(|e| e != "nonnatural");
    let (starts, _) = guide_record(&opening, focal, &actions)?;
    Ok(
        json!({"kind":"budget_game","schema":"search-total-budget-v1/game","arm":"plain-v1",
        "id":src.id,"lineage":sh.spec.lineage,"own":src.own,"other":src.other,
        "seat":src.seat,"opp":src.opp,"seed":src.seed,"starting_player":src.starting_player,
        "live_policy_seeds":src.guide_policy_seeds,"search_root":src.search_root,
        "bank_binding_sha256":binding,"allocation":{"initial":0,"refresh":0,"total":0},
        "game":{"valid":valid,"invalid_reason":(!valid).then_some("non-natural guide"),
            "end":end,"actions":actions,"decisions":decisions,"learner_starts":starts.len(),
            "searches":[],"transitions_used":0,"key_mode":null,
            "search_keys":KeyStats::default().json(),"execution_keys":KeyStats::default().json(),
            "sequence":suffix.json(),"real_transitions":meter.spent,"wall":started.elapsed().as_secs_f64()},
        "wall":started.elapsed().as_secs_f64()}),
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
    use std::collections::HashMap;

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

    /// A real V4 decision plus a deterministic stored-node fixture exercises
    /// the actual cursor mapping/capture/advance boundary. No search runs.
    #[test]
    #[ignore = "native qualification: at most 64 prefix steps and two fixture policy calls"]
    fn reused_decision_witness_is_passive_on_cloned_live_state() {
        let mut state = game(11);
        let mut current = None;
        for _ in 0..64 {
            let d = decision(&state).expect("fixture ended before a focal decision");
            if acting(&d) == PlayerId(0) && d.legal_action_count >= 2 {
                current = Some(d);
                break;
            }
            state.step(d.episode_id, d.step, 0).unwrap();
        }
        let d = current.expect("fixture must expose a focal choice within 64 transitions");
        let build_cursor = || {
            let mut cursor = Cursor::new(Tree::default(), 0, KeyMode::Repaired, true);
            let st = cursor.keyer.step(&state, d).unwrap();
            let (key, parent, parent_edge, cn) = cursor.keyer.storage(&st);
            let mut node = super::super::tree::Node::with_perm(
                parent,
                parent_edge,
                cn.clone(),
                2,
                (0..cn.edges.len()).collect(),
            );
            node.skip = cursor.keyer.skipped(&st);
            let edge = (0..node.n.len())
                .find(|&e| !node.skip.get(e).copied().unwrap_or(false))
                .unwrap();
            node.n.fill(8);
            node.wins[edge] = 8;
            node.visits = 8 * node.n.len() as u64;
            cursor.tree.nodes.insert(key, node);
            cursor.advanced = true;
            cursor
        };
        let mut observed_cursor = build_cursor();
        let mut reference_cursor = build_cursor();
        let records =
            vec![json!({"search":0,"step":d.step,"tree":{"hash":observed_cursor.tree.hash()}})];
        let (lookup, hit) = observed_cursor.lookup(&state, d).unwrap();
        let (reference_lookup, reference_hit) = reference_cursor.lookup(&state, d).unwrap();
        assert_eq!(lookup, reference_lookup);
        let Lookup::Supported(edge) = lookup else {
            panic!("fixture needs a supported stored node")
        };
        let hit = hit.unwrap();
        let reference_hit = reference_hit.unwrap();
        let applied = observed_cursor
            .keyer
            .representative(&hit.step, observed_cursor.live(&hit, edge).unwrap());
        let reference_applied = reference_cursor.keyer.representative(
            &reference_hit.step,
            reference_cursor.live(&reference_hit, edge).unwrap(),
        );
        assert_eq!(applied, reference_applied);
        let before = (
            state.diagnostic_state_hash(),
            state.privileged_core_environment_hash(),
            state.policy_step_count(),
            state.physical_decision_count(),
            observed_cursor.keyer.stats.json(),
        );
        let witness = observed_cursor
            .reuse_witness(Recording::Enabled, &state, &hit, edge, applied, &records)
            .unwrap()
            .unwrap();
        assert!(reference_cursor
            .reuse_witness(
                Recording::Disabled,
                &state,
                &reference_hit,
                edge,
                reference_applied,
                &[],
            )
            .unwrap()
            .is_none());
        assert_eq!(
            witness["decision"],
            super::super::bank::decision_identity(&state).unwrap()
        );
        assert_eq!(witness["applied_live_index"], applied);
        assert_eq!(witness["natural_terminal_backups"], 8);
        assert_eq!(
            before,
            (
                state.diagnostic_state_hash(),
                state.privileged_core_environment_hash(),
                state.policy_step_count(),
                state.physical_decision_count(),
                observed_cursor.keyer.stats.json()
            )
        );
        observed_cursor.advance(hit, applied);
        reference_cursor.advance(reference_hit, reference_applied);
        let mut observed = state.clone();
        let mut reference = state.clone();
        observed.step(d.episode_id, d.step, applied).unwrap();
        reference
            .step(d.episode_id, d.step, reference_applied)
            .unwrap();
        assert_eq!(
            observed.diagnostic_state_hash(),
            reference.diagnostic_state_hash()
        );
        assert_eq!(
            observed.privileged_core_environment_hash(),
            reference.privileged_core_environment_hash()
        );
        let policy = FrozenPlayPolicyV1::training_fixture_v3();
        let mut a = policy.fork_for_collection_v3().unwrap();
        let mut b = policy.fork_for_collection_v3().unwrap();
        a.reset_sampling_v1([7, 9]);
        b.reset_sampling_v1([7, 9]);
        assert_eq!(
            a.select_fast_session_v1(&observed).unwrap(),
            b.select_fast_session_v1(&reference).unwrap()
        );
    }

    fn plain_control_fixture(
        seat: u8,
        starting_player: u8,
        max_physical_decisions: u64,
        max_policy_steps: u64,
    ) -> (FrozenPlayPolicyV1, BankShared, SourceSpec) {
        use super::super::bank::{BankSpec, DeckSpec, SPEC_SCHEMA};
        let policy = FrozenPlayPolicyV1::training_fixture_v3();
        let src = SourceSpec {
            id: format!("plain-control-s{seat}-p{starting_player}"),
            own: 5,
            other: 1,
            seat,
            opp: "fixture".into(),
            seed: 101,
            starting_player,
            source_policy_seeds: [101, 103],
            anchor_draw: 0,
            guide_policy_seeds: [7, 9],
            search_root: "unused-plain-control".into(),
        };
        let sh = BankShared {
            spec: BankSpec {
                schema: SPEC_SCHEMA.into(),
                namespace: "spy-reset-pilot-v1".into(),
                lineage: "fixture".into(),
                runtime_rules: "resolution-boundary-v1".into(),
                select_rule: "fpu-1.5".into(),
                max_physical_decisions,
                max_policy_steps,
                decks: RUNTIME_DECKS
                    .iter()
                    .map(|d| DeckSpec {
                        label: d.id.into(),
                        mainboard: d.card_ids.to_vec(),
                    })
                    .collect(),
                sources: vec![src.clone()],
            },
            labels: vec!["fixture".into()],
            opponents: vec![policy.fork_for_collection_v3().unwrap()],
            prior: DeckPrior::new(&(0..RUNTIME_DECKS.len()).collect::<Vec<_>>()),
            limits: Limits::FORMAL,
        };
        (policy, sh, src)
    }

    /// Independent plain collection with the same opening and guide seeds
    /// must reproduce every selected action for either focal/starting seat.
    #[test]
    #[ignore = "native qualification: bounded fixture policy inference"]
    fn plain_control_matches_guide_seeded_plain_collection_for_both_seats() {
        for seat in 0..2 {
            for starting_player in 0..2 {
                let (policy, sh, src) = plain_control_fixture(seat, starting_player, 12, 40);
                let row = budget_plain_case(&sh, &policy, &src, "binding").unwrap();
                let mut reference = source_session(&sh.spec, &src).unwrap();
                let mut focal = policy.fork_for_collection_v3().unwrap();
                let mut opponent = sh.opponents[0].fork_for_collection_v3().unwrap();
                focal.reset_sampling_v1(src.guide_policy_seeds);
                opponent.reset_sampling_v1(src.guide_policy_seeds);
                let mut actions = Vec::new();
                plain_logged(
                    &mut reference,
                    PlayerId(seat),
                    &mut focal,
                    &mut opponent,
                    &mut actions,
                )
                .unwrap();
                assert_eq!(row["game"]["actions"], json!(actions));
                assert_eq!(
                    row["game"]["end"],
                    json!(outcome(&reference, PlayerId(seat)))
                );
                assert_eq!(row["live_policy_seeds"], json!(src.guide_policy_seeds));
                assert_eq!(row["arm"], "plain-v1");
                assert_eq!(
                    row["allocation"],
                    json!({"initial":0,"refresh":0,"total":0})
                );
                assert_eq!(row["game"]["searches"], json!([]));
                assert_eq!(row["game"]["transitions_used"], 0);
                assert_eq!(row["game"]["real_transitions"], actions.len());
                assert!(row["game"]["decisions"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .all(|d| {
                        d["chooser"] == "plain" && d["search"].is_null() && d["refreshed"].is_null()
                    }));
                // Source/anchor streams belong to the former bank procedure,
                // and must never seed the ordinary-opening plain control.
                let mut changed = src.clone();
                changed.source_policy_seeds = [997, 991];
                changed.search_root = "another-unused-search-root".into();
                changed.anchor_draw = u64::MAX;
                let again = budget_plain_case(&sh, &policy, &changed, "binding").unwrap();
                assert_eq!(row["game"]["actions"], again["game"]["actions"]);
                assert_eq!(row["game"]["end"], again["game"]["end"]);
            }
        }
    }

    /// A technical decision limit is an invalid non-natural result, never
    /// a loss or draw. Natural results retain focal perspective on replay.
    #[test]
    #[ignore = "native qualification: two complete fixture games and two limited games"]
    fn plain_control_preserves_natural_and_limited_terminal_results() {
        for seat in 0..2 {
            for limited in [true, false] {
                let (physical, policy_steps) = if limited { (1, 1) } else { (100_000, 200_000) };
                let (policy, sh, src) = plain_control_fixture(seat, 0, physical, policy_steps);
                let row = budget_plain_case(&sh, &policy, &src, "binding").unwrap();
                let actions: Vec<u32> =
                    serde_json::from_value(row["game"]["actions"].clone()).unwrap();
                let replay = budget_replay(&sh, &src, &actions).unwrap();
                assert_eq!(row["game"]["end"], replay["end"]);
                assert_eq!(row["game"]["sequence"], replay["sequence"]);
                if limited {
                    assert_eq!(row["game"]["end"], "nonnatural");
                    assert_eq!(row["game"]["valid"], false);
                    assert_eq!(row["game"]["invalid_reason"], "non-natural guide");
                } else {
                    assert_eq!(row["game"]["valid"], true);
                    assert!(row["game"]["invalid_reason"].is_null());
                    let end = walk(
                        source_session(&sh.spec, &src).unwrap(),
                        &actions,
                        |_, _, _| Ok(()),
                    )
                    .unwrap();
                    let other = outcome(&end, PlayerId(1 - seat)).unwrap();
                    let expected = match row["game"]["end"].as_str().unwrap() {
                        "win" => "loss",
                        "loss" => "win",
                        "draw" => "draw",
                        e => panic!("expected natural result, got {e}"),
                    };
                    assert_eq!(other, expected);
                }
            }
        }
    }

    /// SD1's root map (`root_rebuild.rs`) is the executor's own mapping at a
    /// new root: at fixture decisions, every storage edge maps to the live
    /// candidate a fresh repaired-key cursor executes for it (`Cursor::live`),
    /// and some live menus are not in storage order.
    #[test]
    fn root_rebuild_map_is_the_new_root_cursors_mapping() {
        let mut s = game(11);
        let seeds = RootSeeds {
            model: "test".into(),
            root: "root-map".into(),
        };
        let (mut checked, mut reordered) = (0, 0);
        while let Some(d) = decision(&s) {
            if checked == 40 {
                break;
            }
            if d.legal_action_count >= 2 && d.acting_player == crate::rl::PlayerSeatV1::P0 {
                let rm = super::super::root_rebuild::root_map(&s, d).unwrap();
                let mut keyer = Keyer::new(KeyMode::Repaired, KeyStats::default());
                let st = keyer.step(&s, d).unwrap();
                let (key, pk, pe, cn) = keyer.storage(&st);
                let mut tree = Tree::default();
                let node = super::super::tree::Node::new(&key, pk, pe, cn.clone(), 1, &seeds);
                tree.nodes.insert(key, node);
                let mut cursor = Cursor::new(tree, 0, KeyMode::Repaired, false);
                let (lookup, hit) = cursor.lookup(&s, d).unwrap();
                assert_eq!(lookup, Lookup::Unsupported);
                let hit = hit.unwrap();
                assert_eq!(rm.map.len(), d.legal_action_count as usize);
                for (e, &l) in rm.map.iter().enumerate() {
                    assert_eq!(l, cursor.live(&hit, e).unwrap());
                }
                let mut sorted = rm.map.clone();
                sorted.sort_unstable();
                assert!(sorted.into_iter().eq(0..d.legal_action_count));
                reordered += usize::from(rm.map.iter().enumerate().any(|(e, &l)| l != e as u32));
                checked += 1;
            }
            s.step(d.episode_id, d.step, 0).unwrap();
        }
        assert!(checked >= 20, "too few focal decisions");
        assert!(reordered > 0, "every live menu was already in edge order");
    }

    /// SD1's root rebuild (`root_rebuild.rs`) keeps a record the executor
    /// wrote: `budget_case` plays the fixture game from its opening with
    /// repaired keys, the 128,000-transition initial search and a ceiling one
    /// full refresh above it (the formal ceiling has no partial refresh), so
    /// a refresh runs; the rebuild keeps the game, and every new root's
    /// frozen choice maps through its root map to the action played there.
    #[test]
    #[ignore = "native qualification: a complete fixture game with 192,000 search transitions; minutes in release, hours in debug: run with cargo test --release"]
    fn root_rebuild_keeps_an_executor_written_search_game() {
        let (policy, sh, mut src) = plain_control_fixture(0, 0, 100_000, 200_000);
        src.id = "root-rebuild-s0-p0".into();
        src.search_root = "root-rebuild".into();
        let total = ANCHOR_BUDGET + REFRESH_BUDGET;
        let (row, rebuilt) = std::thread::scope(|scope| {
            std::thread::Builder::new()
                .stack_size(256 << 20)
                .spawn_scoped(scope, || {
                    let row = budget_case(
                        &sh,
                        &policy,
                        &src,
                        total,
                        KeyMode::Repaired,
                        false,
                        None,
                        Recording::Disabled,
                        "fixture",
                    )
                    .unwrap();
                    let rebuilt = super::super::root_rebuild::game(&sh.spec, &row, None);
                    (row, rebuilt)
                })
                .unwrap()
                .join()
                .unwrap()
        });
        let game = &row["game"];
        assert_eq!(game["valid"], true, "{}", game["invalid_reason"]);
        let searches = game["searches"].as_array().unwrap();
        assert!(searches.len() >= 2, "no refresh ran");
        assert!(rebuilt["void"].is_null(), "{}", rebuilt["void"]);
        let new_roots: Vec<&Value> = game["decisions"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|d| d["chooser"] == "new_root")
            .collect();
        let roots = rebuilt["roots"].as_array().unwrap();
        assert_eq!(roots.len(), new_roots.len());
        for (r, d) in roots.iter().zip(&new_roots) {
            assert_eq!((&r["step"], &r["search"]), (&d["step"], &d["search"]));
            let x = r["frozen_choice"].as_u64().unwrap() as usize;
            assert_eq!(r["map"][x], r["action"]);
            assert_eq!(
                game["actions"][r["step"].as_u64().unwrap() as usize],
                r["action"]
            );
        }
        let refreshed = roots.iter().filter(|r| r["search"] != 0).count();
        assert!(refreshed > 0, "no refresh root executed an edge");
        eprintln!(
            "root rebuild kept {}: {} searches, {} new roots ({refreshed} after a refresh), choosers {}, views {:?}",
            row["id"],
            searches.len(),
            roots.len(),
            rebuilt["choosers"],
            roots.iter().map(|r| r["view"].as_str().unwrap()).collect::<Vec<_>>()
        );
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

    /// The path log (`pathlog.rs`) is passive: the same search with it
    /// leaves the tree and its transitions unchanged, and the logged attempts
    /// rebuild every stored node's counts.
    #[test]
    #[ignore = "native qualification: fixture games and a small search"]
    fn path_log_is_passive_and_rebuilds_the_tree() {
        let policy = FrozenPlayPolicyV1::training_fixture_v3();
        let mut s = game(11);
        advance(&mut s, |d| {
            d.legal_action_count >= 2 && d.acting_player == crate::rl::PlayerSeatV1::P0
        });
        let prior = DeckPrior::new(&(0..RUNTIME_DECKS.len()).collect::<Vec<_>>());
        let seeds = RootSeeds {
            model: "test".into(),
            root: "path-log".into(),
        };
        let limits = Limits {
            select_cap: 6_000,
            ..Limits::FORMAL
        };
        let ctx = guide_ctx(&s, PlayerId(0), 0, &seeds, &prior, limits).unwrap();
        let (plain, plain_tree) = roles(&policy).select_e(&ctx);
        let dir = std::env::temp_dir().join(format!("s4a-path-log-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut diag = super::super::arms::SearchDiag::default();
        diag.path_log = Some(super::super::pathlog::PathLog::new(dir.clone()));
        let (sel, tree) =
            roles(&policy).select_e_diagnosed(&ctx, None, 1, None, None, Some(&mut diag));
        assert_eq!(tree.hash(), plain_tree.hash());
        assert_eq!(
            (sel.transitions, sel.completed),
            (plain.transitions, plain.completed)
        );
        let files: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().path())
            .collect();
        assert_eq!(files.len(), 1, "{files:?}");
        let v: Value = serde_json::from_slice(&std::fs::read(&files[0]).unwrap()).unwrap();
        assert_eq!(v["tree_hash"], tree.hash());
        let nodes = v["nodes"].as_array().unwrap();
        let sims = v["sims"].as_array().unwrap();
        assert_eq!(nodes.len(), tree.nodes.len());
        assert_eq!(sims.len() as u64, sel.completed);
        let mut n: Vec<Vec<u64>> = nodes
            .iter()
            .map(|x| vec![0; x["k"].as_u64().unwrap() as usize])
            .collect();
        let mut w = n.clone();
        for sim in sims {
            let win = sim[1].as_u64().unwrap();
            let flat = sim[4].as_array().unwrap();
            for pair in flat.chunks(2) {
                let (id, a) = (pair[0].as_i64().unwrap(), pair[1].as_u64().unwrap());
                assert!(id >= 0, "broken path");
                n[id as usize][a as usize] += 1;
                w[id as usize][a as usize] += win;
            }
        }
        for (i, x) in nodes.iter().enumerate() {
            assert_eq!(x["n"], json!(n[i]), "node {i}");
            assert_eq!(x["wins"], json!(w[i]), "node {i}");
            let p = x["parent"].as_i64().unwrap();
            assert!(p < i as i64, "parents precede children");
        }
        assert_eq!(nodes.iter().filter(|x| x["parent"] == -1).count(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The best-line test's formal untried-first search (`urgency` None),
    /// pinned from the code before Step 1 (main at 7c700c61).
    const UNTRIED_FIRST_PIN: &str =
        "a12b94f260a43dbbe8ed3d93d4ba5b3c3bb329ab9757d328ae81009ea57b120e";
    const UNTRIED_FIRST_COUNTS: (u64, u64) = (6_000, 23);

    /// Step 1 of the best-line proposal is exact when off: the default search,
    /// with and without finite urgency, builds the tree pinned from the code
    /// before the rule existed. On, every stored node carries best-line values
    /// that equal a from-scratch recomputation, each attempt's result is
    /// tallied clean or rollout by its logged rollout choices, and the root
    /// has a frozen pick.
    #[test]
    #[ignore = "native qualification: fixture games and a small search"]
    fn best_line_rule_is_exact_when_off_and_consistent_when_on() {
        let policy = FrozenPlayPolicyV1::training_fixture_v3();
        let mut s = game(11);
        advance(&mut s, |d| {
            d.legal_action_count >= 2 && d.acting_player == crate::rl::PlayerSeatV1::P0
        });
        let prior = DeckPrior::new(&(0..RUNTIME_DECKS.len()).collect::<Vec<_>>());
        let seeds = RootSeeds {
            model: "test".into(),
            root: "path-log".into(),
        };
        let limits = Limits {
            select_cap: 6_000,
            ..Limits::FORMAL
        };
        let mut ctx = guide_ctx(&s, PlayerId(0), 0, &seeds, &prior, limits).unwrap();
        let (off, off_tree) = roles(&policy).select_e(&ctx);
        // Pinned from the same search on the code before Step 1 (main at
        // ecf5282c, `path_log_is_passive_and_rebuilds_the_tree`'s plain search).
        assert_eq!(
            off_tree.hash(),
            "63957afec560de632059cc0f5ca25b18fa6def55951a7013c328d69452a286e7"
        );
        assert_eq!((off.transitions, off.completed), (6_000, 20));
        assert!(off_tree.nodes.values().all(|n| n.bl.is_none()));
        // The formal untried-first rule (no finite urgency), pinned the same way.
        let urgency = ctx.urgency.take();
        let (plain, plain_tree) = roles(&policy).select_e(&ctx);
        assert_eq!(plain_tree.hash(), UNTRIED_FIRST_PIN);
        assert_eq!((plain.transitions, plain.completed), UNTRIED_FIRST_COUNTS);
        ctx.urgency = urgency;
        ctx.limits.select_cap = 40_000;
        for m in [3, 8] {
            ctx.bestline = Some(m);
            let dir = std::env::temp_dir().join(format!("s4a-bestline-{}-{m}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            let mut diag = super::super::arms::SearchDiag::default();
            diag.path_log = Some(super::super::pathlog::PathLog::new(dir.clone()));
            let (on, tree) =
                roles(&policy).select_e_diagnosed(&ctx, None, 1, None, None, Some(&mut diag));
            assert!(on.completed > 100, "m={m}: {}", on.completed);
            let bad = tree.best_line_mismatches();
            assert!(
                bad.is_empty(),
                "m={m}: {} of {}: {:?}",
                bad.len(),
                tree.nodes.len(),
                &bad[..bad.len().min(3)]
            );
            let root = tree
                .nodes
                .values()
                .find(|n| n.parent == [0; 32])
                .expect("root");
            assert_eq!(root.visits, on.completed, "m={m}");
            assert!(root.frozen_choice().is_some(), "m={m}: no frozen pick");
            // Each attempt's result is a clean or rollout result of the last
            // edge on its path, by the logged rollout-choice count.
            let files: Vec<_> = std::fs::read_dir(&dir)
                .unwrap()
                .map(|e| e.unwrap().path())
                .collect();
            assert_eq!(files.len(), 1, "{files:?}");
            let v: Value = serde_json::from_slice(&std::fs::read(&files[0]).unwrap()).unwrap();
            assert_eq!(v["backup"], json!(bestline_rule(m)));
            let by_hex: HashMap<String, super::super::tree::Key> = tree
                .nodes
                .keys()
                .map(|k| (super::super::pathlog::key_hex(k), *k))
                .collect();
            let keys: Vec<_> = v["nodes"]
                .as_array()
                .unwrap()
                .iter()
                .map(|x| by_hex[x["key"].as_str().unwrap()])
                .collect();
            let mut want: HashMap<(usize, usize), super::super::tree::DirectResults> =
                HashMap::new();
            let mut rollout_results = 0;
            for sim in v["sims"].as_array().unwrap() {
                let win = sim[1].as_u64().unwrap();
                let flat = sim[4].as_array().unwrap();
                let (id, a) = (
                    flat[flat.len() - 2].as_u64().unwrap() as usize,
                    flat[flat.len() - 1].as_u64().unwrap() as usize,
                );
                let x = want.entry((id, a)).or_default();
                let t = if sim[3].as_u64().unwrap() == 0 {
                    &mut x.0
                } else {
                    rollout_results += 1;
                    &mut x.1
                };
                t.0 += 1;
                t.1 += win;
            }
            assert!(
                rollout_results > 0,
                "m={m}: the fixture has rollout choices"
            );
            for (id, k) in keys.iter().enumerate() {
                let n = &tree.nodes[k];
                for a in 0..n.n.len() {
                    let got = n.best_line_direct(a).expect("best-line node");
                    let exp = want.get(&(id, a)).copied().unwrap_or_default();
                    assert_eq!(got, exp, "m={m}: node {id} edge {a}");
                }
            }
            let _ = std::fs::remove_dir_all(&dir);
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

    /// Real sessions/cursor history with synthetic eight-backup stored nodes.
    /// No search or strength claim. The selected child edge deliberately differs
    /// from the frozen sampler action, exposing advancement along the wrong edge.
    #[test]
    #[ignore = "bounded native routing qualification: at most 514 prefix steps and 20 fixture policy calls"]
    fn root_only_executor_preserves_new_roots_and_uses_one_live_raw_draw_on_reuse() {
        let policy = FrozenPlayPolicyV1::training_fixture_v3();
        let mut live = policy.fork_for_collection_v3().unwrap();
        let mut reference = policy.fork_for_collection_v3().unwrap();
        live.reset_sampling_v1([7, 9]);
        reference.reset_sampling_v1([7, 9]);
        let original_streams = live.sampling_stream_snapshot_for_test_v1();
        assert_eq!(
            original_streams,
            reference.sampling_stream_snapshot_for_test_v1()
        );
        // Exercise the existing forced-decision helper on an actual forced
        // response. Both physical seats must retain their sampling state.
        let mut forced_state = game(11);
        let mut forced = None;
        for _ in 0..128 {
            let d = decision(&forced_state).expect("forced fixture ended early");
            if d.legal_action_count == 1 {
                forced = Some(d);
                break;
            }
            forced_state.step(d.episode_id, d.step, 0).unwrap();
        }
        let forced = forced.expect("fixture needs one actual forced decision");
        let mut forced_calls = Counters::default();
        assert_eq!(
            act_plain(&mut live, &forced_state, &forced, &mut forced_calls).unwrap(),
            0
        );
        assert_eq!(forced_calls.inference, 0);
        assert_eq!(
            live.sampling_stream_snapshot_for_test_v1(),
            original_streams
        );
        let mut state = game(11);
        let eligible = |d: &crate::rl_session::FastActorDecisionV1| {
            acting(d) == PlayerId(0) && d.legal_action_count >= 2
        };
        let bounded_next = |state: &mut FastActorSessionV1| {
            for _ in 0..128 {
                let d = decision(state).expect("fixture ended before next focal decision");
                if eligible(&d) {
                    return d;
                }
                state.step(d.episode_id, d.step, 0).unwrap();
            }
            panic!("fixture exceeded bounded prefix");
        };
        let first = bounded_next(&mut state);
        let mut cursor = Cursor::new(Tree::default(), 0, KeyMode::Repaired, false);
        let mut reference_keyer = Keyer::new(KeyMode::Repaired, KeyStats::default());
        let reference_first = reference_keyer.step(&state, first).unwrap();
        let st = cursor.keyer.step(&state, first).unwrap();
        let (key, parent, edge, canon) = cursor.keyer.storage(&st);
        let mut node = super::super::tree::Node::with_perm(
            parent,
            edge,
            canon.clone(),
            2,
            (0..canon.edges.len()).collect(),
        );
        node.n.fill(8);
        node.visits = 8 * node.n.len() as u64;
        cursor.tree.nodes.insert(key, node);
        let (lookup, hit) = cursor.lookup(&state, first).unwrap();
        let hit = hit.unwrap();
        let Lookup::Supported(edge) = lookup else {
            panic!("synthetic root lacks support")
        };
        assert!(cursor.depth == 1 && !cursor.advanced);
        let route = execution::route(ExecutionMode::RootOnly, true, Step::Execute(edge));
        assert_eq!(route.chooser(true), "new_root");
        let first_action = execution::choose(
            route,
            |e| cursor.live(&hit, e),
            || panic!("new root drew raw policy"),
        )
        .unwrap();
        assert_eq!(
            live.sampling_stream_snapshot_for_test_v1(),
            original_streams
        );
        // Persistent root execution must also leave both live streams intact.
        let persistent = execution::route(ExecutionMode::Persistent, true, Step::Execute(edge));
        assert_eq!(
            execution::choose(
                persistent,
                |e| cursor.live(&hit, e),
                || panic!("persistent new root drew raw policy")
            )
            .unwrap(),
            first_action
        );
        assert_eq!(
            live.sampling_stream_snapshot_for_test_v1(),
            original_streams
        );
        cursor.advance(hit, first_action);
        reference_keyer.advance(reference_first, first_action);
        state
            .step(first.episode_id, first.step, first_action)
            .unwrap();

        let child = bounded_next(&mut state);
        let expected = reference.select_fast_session_v1(&state).unwrap();
        let reference_child = reference_keyer.step(&state, child).unwrap();
        let st = cursor.keyer.step(&state, child).unwrap();
        let (key, parent, parent_edge, canon) = cursor.keyer.storage(&st);
        let chosen_edge = canon
            .edges
            .iter()
            .position(|bytes| live_index(canon, bytes).unwrap() != expected)
            .expect("fixture needs a different supported candidate");
        let candidate = live_index(canon, &canon.edges[chosen_edge]).unwrap();
        let mut node = super::super::tree::Node::with_perm(
            parent,
            parent_edge,
            canon.clone(),
            2,
            (0..canon.edges.len()).collect(),
        );
        node.n.fill(8);
        node.wins[chosen_edge] = 8;
        node.visits = 8 * node.n.len() as u64;
        cursor.tree.nodes.insert(key, node);
        let (lookup, hit) = cursor.lookup(&state, child).unwrap();
        let hit = hit.unwrap();
        assert_eq!(lookup, Lookup::Supported(chosen_edge));
        assert!(cursor.advanced);
        let route = execution::route(ExecutionMode::RootOnly, false, Step::Execute(chosen_edge));
        assert_eq!(route.chooser(false), "raw_instead_of_reuse");
        let mut calls = Counters::default();
        let actual = execution::choose(
            route,
            |_| panic!("reused edge was selected"),
            || act_plain(&mut live, &state, &child, &mut calls),
        )
        .unwrap();
        assert_eq!(actual, expected);
        assert_ne!(actual, candidate);
        assert_eq!(calls.inference, 1);
        assert_eq!(
            live.sampling_stream_snapshot_for_test_v1(),
            reference.sampling_stream_snapshot_for_test_v1()
        );
        // Persistent reuse still selects the tree edge without an extra raw
        // draw, retaining its pre-existing continuation stream position.
        let mut persistent_live = policy.fork_for_collection_v3().unwrap();
        let mut persistent_reference = policy.fork_for_collection_v3().unwrap();
        persistent_live.reset_sampling_v1([7, 9]);
        persistent_reference.reset_sampling_v1([7, 9]);
        let persistent =
            execution::route(ExecutionMode::Persistent, false, Step::Execute(chosen_edge));
        assert_eq!(
            execution::choose(
                persistent,
                |e| cursor.live(&hit, e),
                || panic!("persistent reuse drew raw policy")
            )
            .unwrap(),
            candidate
        );
        assert_eq!(
            persistent_live.sampling_stream_snapshot_for_test_v1(),
            persistent_reference.sampling_stream_snapshot_for_test_v1()
        );
        cursor.advance(hit, actual);
        reference_keyer.advance(reference_child, actual);
        state.step(child.episode_id, child.step, actual).unwrap();

        let next = bounded_next(&mut state);
        let (lookup, hit) = cursor.lookup(&state, next).unwrap();
        assert_eq!(lookup, Lookup::Miss);
        let st = reference_keyer.step(&state, next).unwrap();
        let hit = hit.unwrap();
        let actual_key = cursor.keyer.storage(&hit.step);
        let expected_key = reference_keyer.storage(&st);
        assert_eq!(
            (actual_key.0, actual_key.1, actual_key.2),
            (expected_key.0, expected_key.1, expected_key.2)
        );
        let allocation = Allocation {
            anchor: ANCHOR_BUDGET,
            refresh: REFRESH_BUDGET,
            total: 2_048_000,
        };
        assert_eq!(
            rule(lookup, allocation, ANCHOR_BUDGET, false),
            Step::Refresh(REFRESH_BUDGET)
        );
        // Check the actual legacy stream continuation as well as draw states.
        assert_eq!(
            persistent_live.select_fast_session_v1(&state).unwrap(),
            persistent_reference.select_fast_session_v1(&state).unwrap()
        );
        assert_eq!(
            persistent_live.sampling_stream_snapshot_for_test_v1(),
            persistent_reference.sampling_stream_snapshot_for_test_v1()
        );
        for _ in 0..8 {
            assert_eq!(
                live.select_fast_session_v1(&state).unwrap(),
                reference.select_fast_session_v1(&state).unwrap()
            );
        }
    }
}
