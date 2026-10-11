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
use super::labels::{classify, spy_defs};
use super::play::{acting, apply, decision, Counters, Meter};
use super::seeds::RootSeeds;
use super::tree::{canon, child_key, live_index, Found, Key, Tree, MAX_DEPTH};
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

/// The current tree and where the actual history stands in it.
struct Cursor {
    tree: Tree,
    parent: (Key, Vec<u8>),
    matching: bool,
    depth: u32,
    last_phys: Option<u64>,
    search: usize,
}

impl Cursor {
    fn new(tree: Tree, search: usize) -> Self {
        Self {
            tree,
            parent: ([0u8; 32], b"root".to_vec()),
            matching: true,
            depth: 0,
            last_phys: None,
            search,
        }
    }

    /// Looks up the current decision; on a hit returns the live action of
    /// the supported edge (and advances on it once executed).
    fn lookup(
        &mut self,
        s: &FastActorSessionV1,
        d: crate::rl_session::FastActorDecisionV1,
    ) -> Result<(Lookup, Option<(Key, super::tree::Canon)>), String> {
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
        let cn = canon(s, d).map_err(err)?;
        let key = child_key(&self.parent.0, &self.parent.1, &cn);
        let found = match self
            .tree
            .find(&key, &self.parent.0, &self.parent.1, &cn)
            .map_err(err)?
        {
            Found::Hit(node) => match node.frozen_choice() {
                Some(e) => Lookup::Supported(e),
                None => Lookup::Unsupported,
            },
            Found::Miss => Lookup::Miss,
        };
        if found == Lookup::Miss {
            self.matching = false;
        }
        Ok((found, Some((key, cn))))
    }

    /// Advances along the chosen live action (a matched node, any chooser).
    fn advance(&mut self, key: Key, cn: &super::tree::Canon, live: u32) {
        self.parent = (key, cn.menu[live as usize].clone());
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
        let ctx = guide_ctx(s, self.focal, self.opp, &seeds, &self.sh.prior, limits)?;
        let started = Instant::now();
        let (sel, tree) = self.roles.select_e(&ctx);
        self.used += sel.transitions;
        let failed = sel.incomplete || !sel.faults.is_empty();
        self.records.push(json!({"search":k,"step":at_step,"reason":reason,"budget":budget,
            "transitions":sel.transitions,"completed_natural":sel.completed,"attempted":sel.attempted,
            "nonnatural":sel.nonnatural,"incomplete":sel.incomplete,"faults":sel.faults,
            "tree":sel.extra["tree"].clone(),"wall":started.elapsed().as_secs_f64()}));
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
    };
    let anchor_step = decision(anchor).ok_or("anchor is terminal")?.step;
    let (first, kept) = match anchor_tree {
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
    };
    let mut cursor = Cursor::new(first, 0);
    live_focal.reset_sampling_v1(src.guide_policy_seeds);
    live_opp.reset_sampling_v1(src.guide_policy_seeds);
    let mut s = anchor.clone();
    let mut meter = Meter::new(u64::MAX);
    let mut c = Counters::default();
    let (mut actions, mut decisions) = (Vec::new(), Vec::new());
    let mut invalid: Option<String> = None;
    while let Some(d) = decision(&s) {
        let a = if acting(&d) == focal && d.legal_action_count >= 2 {
            let (mut lookup, mut hit) = cursor.lookup(&s, d)?;
            let mut refreshed = None;
            let step = rule(lookup, allocation, searches.used, false);
            if let Step::Refresh(budget) = step {
                match searches.run(&s, budget, d.step, lookup.reason())? {
                    Some(tree) => {
                        cursor = Cursor::new(tree, searches.records.len() - 1);
                        refreshed = Some(lookup.reason());
                        (lookup, hit) = cursor.lookup(&s, d)?;
                    }
                    None => {
                        invalid = Some(format!("failed search at step {}", d.step));
                        break;
                    }
                }
            }
            let new_root = cursor.depth == 1 && cursor.parent.0 == [0u8; 32];
            let (a, chooser) = match rule(lookup, allocation, searches.used, true) {
                Step::Execute(e) => {
                    let (_, cn) = hit.as_ref().expect("a supported lookup has a node");
                    (
                        live_index(cn, &cn.edges[e]).map_err(err)?,
                        if new_root { "new_root" } else { "reused" },
                    )
                }
                _ => (act_plain(live_focal, &s, &d, &mut c)?, "plain"),
            };
            if let Some((key, cn)) = &hit {
                if cursor.matching {
                    cursor.advance(*key, cn, a);
                }
            }
            let sem = s.diagnostic_current_action_semantics().unwrap_or_default();
            let (mut own_offered, mut dr_offered) = (false, false);
            for x in &sem {
                let (_, own, dr) = classify(x, s.game_state(), focal, &defs);
                own_offered |= own;
                dr_offered |= dr;
            }
            let (_, own_chosen, dr_chosen) =
                sem.get(a as usize).map_or((false, false, false), |x| {
                    classify(x, s.game_state(), focal, &defs)
                });
            decisions.push(
                json!({"step":d.step,"physical_decision_id":d.physical_decision_id,
                "substep":d.substep_index,"k":d.legal_action_count,"chooser":chooser,
                "search":cursor.search,"lookup":lookup.reason(),"refreshed":refreshed,
                "spy_self_offered":own_offered,"spy_self_chosen":own_chosen,
                "dr_giant_offered":dr_offered,"dr_giant_chosen":dr_chosen}),
            );
            a
        } else if acting(&d) == focal {
            0
        } else {
            act_plain(live_opp, &s, &d, &mut c)?
        };
        actions.push(a);
        apply(&mut s, &d, a, &mut meter, focal, None, &defs).map_err(err)?;
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
    Ok((
        json!({"valid":valid,"invalid_reason":invalid.or_else(|| (!valid).then(|| "non-natural guide".to_owned())),
            "end":end,"actions":actions,"decisions":decisions,"learner_starts":starts.len(),
            "searches":searches.records,"transitions_used":searches.used,
            "wall":started.elapsed().as_secs_f64()}),
        kept,
    ))
}

/// Rebuilds the bank's anchor for `src` and plays the requested arms.
pub(super) fn source_arms(
    sh: &BankShared,
    policy: &FrozenPlayPolicyV1,
    src: &SourceSpec,
    parity: bool,
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
                )?
                .0
            }
            None => json!({"valid":false,"invalid_reason":"failed anchor search"}),
        };
        arms.insert("S128".into(), rec);
    }
    Ok(
        json!({"kind":"refresh_source","id":src.id,"lineage":spec.lineage,"own":src.own,
        "other":src.other,"seat":src.seat,"opp":src.opp,"anchor_step":eligible[index],
        "allocation":{"total":TOTAL_BUDGET,"anchor":ANCHOR_BUDGET,"refresh":REFRESH_BUDGET},
        "arms":arms,"wall":started.elapsed().as_secs_f64()}),
    )
}

#[cfg(test)]
mod tests {
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
            loop {
                let Some(d) = decision(&s) else { break };
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
