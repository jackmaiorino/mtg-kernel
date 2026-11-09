//! EX1's search inside RT1 collection (collab
//! LANES/spy-primary-direction-20261028/EX1-DESIGN-DRAFT.md v3.3 sections 4
//! and 7): a search enters only through `guide_ctx` and `select_e`, so the
//! sampler supplies hidden cards and the true state is never read; both seats'
//! rollouts are the learner's own policy (SELF), forked so no live sampling
//! stream moves. The retained tree serves the rest of the engine turn by the
//! refresh guide's visible-history cursor (depth 32).
//!
//! A failed or incomplete search gives no tree; the caller continues the
//! episode (the guide's invalidate-on-failure is not inherited).

use super::arms::{Limits, Roles};
use super::bank::{fixed_work_roles, guide_ctx};
use super::canon2::KeyMode;
use super::play::decision;
use super::refresh::{Cursor, Lookup};
use super::root_allocation::SearchOptions;
use super::seeds::RootSeeds;
use super::world::DeckPrior;
use crate::ex1_v1::{Ex1EdgeCountsV1, EX1_SUPPORT_MIN_V1};
use crate::ids::PlayerId;
use crate::rl_session::{FastActorDecisionV1, FastActorSessionV1};
use crate::runtime_decks::RUNTIME_DECKS;
use crate::sideboard_play_policy_v1::FrozenPlayPolicyV1;
use serde_json::{json, Value};
use std::time::Instant;

/// The seed model string of every EX1 search (RootSeeds' namespace).
pub(crate) const EX1_SEED_MODEL_V1: &str = "ex1-search-v1";
/// Each search's thread stack (the census binary's).
const EX1_SEARCH_STACK_BYTES_V1: usize = 256 << 20;

/// A node of the retained tree at a live decision, in live menu order.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Ex1NodeV1 {
    pub(crate) counts: Vec<Ex1EdgeCountsV1>,
    /// Completed simulations through the node.
    pub(crate) visits: u64,
}

/// What the retained tree says at a live learner decision.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Ex1LookupV1 {
    /// No tree is retained.
    NoTree,
    /// The history left the tree, or passed its depth limit.
    Miss(&'static str),
    /// A matched node.
    Node(Ex1NodeV1),
}

/// One learner's searcher: SELF roles, the unconditioned registered-deck
/// prior, and at most one retained tree with its cursor.
pub(crate) struct Ex1SearcherV1 {
    roles: Roles,
    prior: DeckPrior,
    focal: PlayerId,
    cursor: Option<Cursor>,
    /// The cursor's pending hit at the current decision (advanced with the
    /// learner's sampled action).
    pending: Option<super::refresh::Hit>,
    /// An explicit exact key memo (W5); `None` keeps the process default.
    key_memo: Option<Option<usize>>,
}

impl Ex1SearcherV1 {
    pub(crate) fn new(learner: &FrozenPlayPolicyV1, focal: PlayerId) -> Result<Self, String> {
        let opponent = learner.fork_for_collection_v3()?;
        Ok(Self {
            roles: fixed_work_roles(learner, std::slice::from_ref(&opponent))?,
            prior: DeckPrior::new(&(0..RUNTIME_DECKS.len()).collect::<Vec<_>>()),
            focal,
            cursor: None,
            pending: None,
            key_memo: None,
        })
    }

    /// Every search runs with an exact key memo of `bytes` (W5's recorded
    /// setting; keys, trees, selection and statistics are unchanged).
    pub(crate) fn with_key_memo_v1(mut self, bytes: usize) -> Self {
        self.key_memo = Some(Some(bytes));
        self
    }

    /// Every search runs with the key memo off, whatever the process default.
    pub(crate) fn with_key_memo_off_v1(mut self) -> Self {
        self.key_memo = Some(None);
        self
    }

    /// Drops the retained tree (the burst's turn ended).
    pub(crate) fn clear(&mut self) {
        self.cursor = None;
        self.pending = None;
    }

    /// A new search at the live decision `s` with `budget` raw selection
    /// transitions and seeds rooted at `seed_root`. It replaces any retained
    /// tree; a failed search retains none. Returns the search record and
    /// whether it succeeded.
    pub(crate) fn search(
        &mut self,
        s: &FastActorSessionV1,
        budget: u64,
        seed_root: String,
    ) -> Result<(Value, bool), String> {
        self.clear();
        let seeds = RootSeeds {
            model: EX1_SEED_MODEL_V1.into(),
            root: seed_root,
        };
        let limits = Limits {
            select_cap: budget,
            ..Limits::FORMAL
        };
        let ctx = guide_ctx(s, self.focal, 0, &seeds, &self.prior, limits)?;
        let started = Instant::now();
        // The search recurses deeply: it runs on its own thread with the
        // census binary's stack, whatever thread collects the episode.
        let roles = &mut self.roles;
        let options = SearchOptions {
            key_memo: self.key_memo,
            ..SearchOptions::default()
        };
        let (sel, tree) = std::thread::scope(|scope| {
            std::thread::Builder::new()
                .name("ex1-search".into())
                .stack_size(EX1_SEARCH_STACK_BYTES_V1)
                .spawn_scoped(scope, || {
                    roles.select_e_options(&ctx, None, 1, None, None, None, &options)
                })
                .map_err(|e| format!("ex1 search thread: {e}"))?
                .join()
                .map_err(|_| "ex1 search thread panicked".to_string())
        })?;
        let wall = started.elapsed().as_secs_f64();
        let failed = sel.incomplete || !sel.faults.is_empty();
        let record = json!({"seed_root":seeds.root,"budget":budget,"transitions":sel.transitions,
            "completed_natural":sel.completed,"attempted":sel.attempted,"nonnatural":sel.nonnatural,
            "cap_reached":sel.cap_reached,"incomplete":sel.incomplete,"faults":sel.faults,
            "sampler":sel.sampler.json(),"boundary_encoding_failures":
                sel.sampler.kinds.get("SampleBoundaryEncodingFailed").copied().unwrap_or(0),
            "nodes":tree.nodes.len(),"wall":wall,"failed":failed});
        let mut record = record;
        if let Some(memo) = self.key_memo {
            record["key_memo_bytes"] = json!(memo);
        }
        if !failed {
            self.cursor = Some(Cursor::new(tree, 0, KeyMode::Legacy, false));
        }
        Ok((record, !failed))
    }

    /// One value-leaf search (`s4a-vl-v1`, W5 Stage 2a capture) at the live
    /// decision `s`: `sims` attempted simulations at `horizon` under a
    /// transition guard of `sims` x 512, with the same seeds model, sampler,
    /// key mode and key memo as `search`. It retains no tree. Returns the
    /// search record (with `primary_sha256` over its deterministic fields) and,
    /// for a completed search, the root's (visits, value units, draws) per live
    /// action.
    #[allow(clippy::type_complexity, reason = "one per-action triple")]
    pub(crate) fn value_leaf_search(
        &mut self,
        s: &FastActorSessionV1,
        horizon: super::value_leaf::Horizon,
        sims: u64,
        seed_root: String,
    ) -> Result<(Value, Option<Vec<(u64, u64, u64)>>), String> {
        self.clear();
        let seeds = RootSeeds {
            model: EX1_SEED_MODEL_V1.into(),
            root: seed_root,
        };
        let limits = Limits {
            select_cap: sims.saturating_mul(512),
            ..Limits::FORMAL
        };
        let ctx = guide_ctx(s, self.focal, 0, &seeds, &self.prior, limits)?;
        let started = Instant::now();
        let roles = &mut self.roles;
        let options = SearchOptions {
            key_memo: self.key_memo,
            work_limit: Some(sims),
            value_leaf: Some(super::value_leaf::ValueLeafV1 { horizon }),
            ..SearchOptions::default()
        };
        let (sel, tree) = std::thread::scope(|scope| {
            std::thread::Builder::new()
                .name("ex1-vl-search".into())
                .stack_size(EX1_SEARCH_STACK_BYTES_V1)
                .spawn_scoped(scope, || {
                    roles.select_e_options(&ctx, None, 1, None, None, None, &options)
                })
                .map_err(|e| format!("value-leaf search thread: {e}"))?
                .join()
                .map_err(|_| "value-leaf search thread panicked".to_string())
        })?;
        let wall = started.elapsed().as_secs_f64();
        let failed = sel.incomplete || !sel.faults.is_empty();
        let primary = json!({"attempted":sel.attempted,"completed":sel.completed,
            "nonnatural":sel.nonnatural,"transitions":sel.transitions,"inference":sel.inference,
            "cap_reached":sel.cap_reached,"incomplete":sel.incomplete,"faults":sel.faults,
            "tree":sel.extra["tree"],"value_leaf":sel.extra["value_leaf"]});
        let primary_sha256: String = {
            use sha2::{Digest, Sha256};
            Sha256::digest(primary.to_string().as_bytes())
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect()
        };
        let mut record = json!({"seed_root":seeds.root,"horizon":horizon.label(),"sims":sims,
            "primary":primary,"primary_sha256":primary_sha256,"nodes":tree.nodes.len(),
            "wall":wall,"failed":failed});
        if let Some(memo) = self.key_memo {
            record["key_memo_bytes"] = json!(memo);
        }
        let root = (!failed)
            .then(|| super::value_leaf::root_edges(&tree))
            .flatten()
            .map(|edges| {
                let mut out = vec![(0, 0, 0); edges.len()];
                for e in &edges {
                    out[e.live as usize] = (e.n, e.units, e.draws);
                }
                out
            });
        Ok((record, root))
    }

    /// SD1 (collab LANES/search-diagnostic-20261108/DESIGN.md v4 E1, E2): one
    /// search at the live decision `s` under an explicit world lifecycle,
    /// with the same seeds model, sampler, key mode and key memo as `search`.
    /// `value_leaf` = (horizon, simulations) runs `s4a-vl-v1` as
    /// `value_leaf_search` does; `None` runs the rollout search at `budget`
    /// raw selection transitions. It retains no tree. Returns the search
    /// record (with the world receipt) and, for a completed search, the root
    /// in live menu order.
    pub(crate) fn sd1_search_v1(
        &mut self,
        s: &FastActorSessionV1,
        budget: u64,
        value_leaf: Option<(super::value_leaf::Horizon, u64)>,
        world: super::world::WorldMode,
        seed_root: String,
    ) -> Result<(Value, Option<Sd1RootV1>), String> {
        self.clear();
        let seeds = RootSeeds {
            model: EX1_SEED_MODEL_V1.into(),
            root: seed_root,
        };
        let limits = Limits {
            select_cap: match value_leaf {
                Some((_, sims)) => sims.saturating_mul(512),
                None => budget,
            },
            ..Limits::FORMAL
        };
        let ctx = guide_ctx(s, self.focal, 0, &seeds, &self.prior, limits)?;
        let started = Instant::now();
        let roles = &mut self.roles;
        let options = SearchOptions {
            key_memo: self.key_memo,
            work_limit: value_leaf.map(|(_, sims)| sims),
            value_leaf: value_leaf.map(|(horizon, _)| super::value_leaf::ValueLeafV1 { horizon }),
            world: Some(world),
            ..SearchOptions::default()
        };
        let (sel, tree) = std::thread::scope(|scope| {
            std::thread::Builder::new()
                .name("sd1-search".into())
                .stack_size(EX1_SEARCH_STACK_BYTES_V1)
                .spawn_scoped(scope, || {
                    roles.select_e_options(&ctx, None, 1, None, None, None, &options)
                })
                .map_err(|e| format!("sd1 search thread: {e}"))?
                .join()
                .map_err(|_| "sd1 search thread panicked".to_string())
        })?;
        let wall = started.elapsed().as_secs_f64();
        let failed = sel.incomplete || !sel.faults.is_empty();
        let mut record = json!({"seed_root":seeds.root,"budget":limits.select_cap,
            "world":world.label(),"transitions":sel.transitions,"inference":sel.inference,
            "completed":sel.completed,"attempted":sel.attempted,"nonnatural":sel.nonnatural,
            "cap_reached":sel.cap_reached,"incomplete":sel.incomplete,"faults":sel.faults,
            "sampler":sel.sampler.json(),"world_receipt":sel.extra.get("world").cloned(),
            "nodes":tree.nodes.len(),"wall":wall,"failed":failed});
        if let Some((horizon, sims)) = value_leaf {
            record["value_leaf"] = json!({"horizon":horizon.label(),"sims":sims});
        }
        if let Some(memo) = self.key_memo {
            record["key_memo_bytes"] = json!(memo);
        }
        let root = if failed {
            None
        } else {
            sd1_root_v1(&tree, decision(s).map(|d| d.legal_action_count as usize))?
        };
        Ok((record, root))
    }

    /// Looks up the current learner decision (two or more legal actions) in
    /// the retained tree. Every such decision along the burst must be looked
    /// up and then advanced with the learner's sampled action.
    pub(crate) fn lookup(
        &mut self,
        s: &FastActorSessionV1,
        d: FastActorDecisionV1,
    ) -> Result<Ex1LookupV1, String> {
        self.pending = None;
        let Some(cursor) = self.cursor.as_mut() else {
            return Ok(Ex1LookupV1::NoTree);
        };
        let (lookup, hit) = cursor.lookup(s, d)?;
        let node = match (&lookup, &hit) {
            (Lookup::Miss | Lookup::DepthLimit, _) => None,
            (_, Some(h)) => h.key.and_then(|k| cursor.tree.nodes.get(&k)),
            _ => None,
        };
        let result = match node {
            None => Ex1LookupV1::Miss(lookup.reason()),
            Some(node) => {
                let h = hit.as_ref().expect("a matched node has a hit");
                let width = node.n.len();
                if width != d.legal_action_count as usize {
                    return Err(
                        "ex1: the matched node's menu differs from the live substep menu".into(),
                    );
                }
                let mut counts = vec![None; width];
                for e in 0..width {
                    let live = cursor.live(h, e)? as usize;
                    let slot = counts
                        .get_mut(live)
                        .ok_or("ex1: a node edge maps outside the live menu")?;
                    if slot.is_some() {
                        return Err("ex1: two node edges map to one live action".into());
                    }
                    *slot = Some(Ex1EdgeCountsV1 {
                        n: node.n[e],
                        wins: node.wins[e],
                        draws: node.draws[e],
                    });
                }
                Ex1LookupV1::Node(Ex1NodeV1 {
                    counts: counts.into_iter().map(|c| c.expect("bijection")).collect(),
                    visits: node.visits,
                })
            }
        };
        self.pending = hit;
        Ok(result)
    }

    /// Advances the cursor along the learner's sampled live action.
    pub(crate) fn advance(&mut self, live: u32) {
        if let (Some(cursor), Some(hit)) = (self.cursor.as_mut(), self.pending.take()) {
            if cursor.matching {
                cursor.advance(hit, live);
            }
        }
    }
}

/// The search's fixed settings, bound into an EX1 run's continuation
/// identity: seeds, keys, the coverage rule and the formal limits it extends.
pub(crate) fn ex1_search_identity_v1() -> Value {
    json!({"seed_model":EX1_SEED_MODEL_V1,"entry":"guide_ctx+select_e","key_mode":"legacy",
        "interchange":false,"coverage_min_visits":EX1_SUPPORT_MIN_V1,
        "coverage_min_supported_edges":2,"limits":Limits::FORMAL.json()})
}

/// Section 4's burst coverage rule for a matched node: at least 8 backups and
/// at least two supported edges.
pub(crate) fn ex1_node_covers_v1(node: &Ex1NodeV1) -> bool {
    node.visits >= EX1_SUPPORT_MIN_V1
        && node
            .counts
            .iter()
            .filter(|c| c.n >= EX1_SUPPORT_MIN_V1)
            .count()
            >= 2
}

/// The Spy self-target offer at a live learner decision: the live index of
/// the Balustrade Spy target choice naming the learner, if offered.
pub(crate) fn ex1_self_target_v1(s: &FastActorSessionV1, focal: PlayerId) -> Option<usize> {
    let defs = super::labels::spy_defs();
    decision(s)?;
    s.diagnostic_current_action_semantics()?
        .iter()
        .position(|x| super::labels::classify(x, s.game_state(), focal, &defs).1)
}

/// A searched root in live menu order (SD1's capture): per live action the
/// natural backups, wins and draws (and value units in a value-leaf tree),
/// the node's seeded edge order as live indices (the frozen choice's tie
/// order), and the engine's own `frozen_choice` as a live index.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Sd1RootV1 {
    pub(crate) visits: u64,
    pub(crate) n: Vec<u64>,
    pub(crate) wins: Vec<u64>,
    pub(crate) draws: Vec<u64>,
    pub(crate) units: Option<Vec<u64>>,
    pub(crate) perm_live: Vec<u32>,
    pub(crate) frozen_live: Option<u32>,
}

/// The root of `tree` mapped to live indices; an edge that does not bind to
/// exactly one live candidate, two edges on one live action, or a width other
/// than the live menu's is an error. `None` when the tree has no root.
pub(crate) fn sd1_root_v1(
    tree: &super::tree::Tree,
    width: Option<usize>,
) -> Result<Option<Sd1RootV1>, String> {
    let Some(r) = super::value_leaf::root(tree) else {
        return Ok(None);
    };
    let k = r.n.len();
    if width.is_some_and(|w| w != k) {
        return Err("sd1: the root's menu differs from the live menu".into());
    }
    let mut live = Vec::with_capacity(k);
    for e in 0..k {
        live.push(
            super::tree::live_index(&r.canon, &r.canon.edges[e])
                .map_err(|_| "sd1: a root edge does not bind to one live candidate".to_string())?,
        );
    }
    let mut seen = vec![false; k];
    for &l in &live {
        let slot = seen
            .get_mut(l as usize)
            .ok_or("sd1: a root edge maps outside the live menu")?;
        if *slot {
            return Err("sd1: two root edges map to one live action".into());
        }
        *slot = true;
    }
    let place = |v: &[u64]| {
        let mut out = vec![0; k];
        for e in 0..k {
            out[live[e] as usize] = v[e];
        }
        out
    };
    Ok(Some(Sd1RootV1 {
        visits: r.visits,
        n: place(&r.n),
        wins: place(&r.wins),
        draws: place(&r.draws),
        units: (r.vsum.len() == k).then(|| place(&r.vsum)),
        perm_live: r.perm.iter().map(|&e| live[e]).collect(),
        frozen_live: r.frozen_choice().map(|e| live[e]),
    }))
}
