//! Opt-in root allocation at the first search choice only.
//! - `Halving`: anytime successive halving. Quotas and means use actual
//!   natural terminal backups. Ties at the cut survive, including an all-loss
//!   prefix. No learned prior enters it.
//! - `Puct` (collab LANES/searchprobe-20261111, measurement only): a
//!   PUCT-style allocation with the searching policy as the prior at the root
//!   alone; every deeper node keeps E's UCB rule, and the root choice stays
//!   `frozen_choice` (greatest mean among edges with eight backups).

use super::tree::{live_index, Key, Node, MIN_EXEC_BACKUPS};
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum RootAllocation {
    #[default]
    Uct,
    Halving,
    /// The root prior in `SearchOptions::root_prior` (required).
    Puct,
}

/// A root prior over the live menu (live action order), already tempered and
/// normalized by the caller, and the exploration constant.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RootPrior {
    pub(crate) probs: Vec<f64>,
    pub(crate) c_puct: f64,
}

impl RootPrior {
    /// `softmax(logits / temperature)`; temperature 1 is the raw policy.
    /// Its caller (`search_probe.rs`) needs the evaluator's CUDA feature.
    #[cfg_attr(
        not(feature = "experimental-burn-net8-packed-cuda-v1"),
        allow(dead_code)
    )]
    pub(crate) fn tempered(logits: &[f32], temperature: f64, c_puct: f64) -> Self {
        let scaled: Vec<f64> = logits.iter().map(|&x| f64::from(x) / temperature).collect();
        let max = scaled.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let exp: Vec<f64> = scaled.iter().map(|x| (x - max).exp()).collect();
        let sum: f64 = exp.iter().sum();
        Self {
            probs: exp.iter().map(|x| x / sum).collect(),
            c_puct,
        }
    }

    /// PUCT at the root: the edge maximizing
    /// `Q(a) + c * P(a) * sqrt(max(N, 1)) / (1 + n(a))`, where an untried
    /// edge's Q is the node's mean over all its backups (0.5 before any), and
    /// ties go to the seeded order. Skipped (merged) edges are never chosen.
    pub(crate) fn select(&self, node: &Node) -> usize {
        let visits: u64 = node.n.iter().sum();
        let fpu = if visits == 0 {
            0.5
        } else {
            (0..node.n.len())
                .filter(|&a| node.n[a] > 0)
                .map(|a| node.mean(a) * node.n[a] as f64)
                .sum::<f64>()
                / visits as f64
        };
        let root_n = (visits.max(1) as f64).sqrt();
        let mut best = None;
        let mut best_v = f64::NEG_INFINITY;
        for &a in &node.perm {
            if node.skip.get(a).copied().unwrap_or(false) {
                continue;
            }
            let p = live_index(&node.canon, &node.canon.edges[a])
                .ok()
                .and_then(|live| self.probs.get(live as usize).copied())
                .unwrap_or(0.0);
            let q = if node.n[a] == 0 { fpu } else { node.mean(a) };
            let v = q + self.c_puct * p * root_n / (1 + node.n[a]) as f64;
            if v > best_v {
                best_v = v;
                best = Some(a);
            }
        }
        best.expect("legal root edge")
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct SearchOptions {
    pub(crate) allocation: RootAllocation,
    /// The root prior of `RootAllocation::Puct`; None otherwise.
    pub(crate) root_prior: Option<RootPrior>,
    pub(crate) sharing: bool,
    pub(crate) cpu_seconds: Option<f64>,
    /// Attempted simulations, including rejected samples, for exact replay.
    pub(crate) work_limit: Option<u64>,
    /// An explicit exact key memo for this search (W5's recorded setting):
    /// `Some(Some(bytes))` on with that limit and no counters, `Some(None)`
    /// off; `None` keeps the process default (`arms::key_memo_use`).
    pub(crate) key_memo: Option<Option<usize>>,
    /// Opt-in value-leaf search (`value_leaf.rs`); None keeps E unchanged.
    pub(crate) value_leaf: Option<super::value_leaf::ValueLeafV1>,
    /// SD1's world lifecycle for E's selection (`world::WorldMode`); `None`
    /// is the default `Fresh` (unchanged). Never read from the environment.
    pub(crate) world: Option<super::world::WorldMode>,
}

#[derive(Default)]
pub(crate) struct RootAllocator {
    roots: HashMap<Key, Round>,
    /// Set: PUCT at the root instead of successive halving.
    prior: Option<RootPrior>,
}

struct Round {
    active: Vec<usize>,
    quota: u64,
}

impl RootAllocator {
    pub(crate) fn puct(prior: RootPrior) -> Self {
        Self {
            roots: HashMap::new(),
            prior: Some(prior),
        }
    }

    pub(crate) fn is_puct(&self) -> bool {
        self.prior.is_some()
    }

    pub(crate) fn select(&mut self, key: Key, node: &Node) -> usize {
        if let Some(prior) = &self.prior {
            return prior.select(node);
        }
        let round = self.roots.entry(key).or_insert_with(|| Round {
            active: node
                .perm
                .iter()
                .copied()
                .filter(|&a| !node.skip.get(a).copied().unwrap_or(false))
                .collect(),
            quota: MIN_EXEC_BACKUPS,
        });
        if round.active.iter().all(|&a| node.n[a] >= round.quota) {
            if round.active.len() > 1 {
                let mut ranked = round.active.clone();
                ranked.sort_by(|&a, &b| mean(node, b).total_cmp(&mean(node, a)));
                let cut = mean(node, ranked[ranked.len().div_ceil(2) - 1]);
                // Preserve seeded order and every tie at the boundary.
                round.active.retain(|&a| mean(node, a) >= cut);
            }
            round.quota = round.quota.saturating_mul(2);
        }
        *round
            .active
            .iter()
            .min_by_key(|&&a| node.n[a])
            .expect("legal root edge")
    }

    pub(crate) fn choice(&self, key: &Key, node: &Node) -> Option<usize> {
        if self.prior.is_some() {
            return node.frozen_choice();
        }
        let round = self.roots.get(key)?;
        let mut best = None;
        for &a in &round.active {
            if node.n[a] >= MIN_EXEC_BACKUPS && best.is_none_or(|b| mean(node, a) > mean(node, b)) {
                best = Some(a);
            }
        }
        best
    }
}

fn mean(node: &Node, a: usize) -> f64 {
    node.wins[a] as f64 / node.n[a].max(1) as f64
}

#[cfg(test)]
mod tests {
    use super::super::tree::Canon;
    use super::*;

    fn node() -> Node {
        let canon = Canon {
            obs: vec![],
            menu: vec![vec![0], vec![1], vec![2], vec![3]],
            edges: vec![vec![0], vec![1], vec![2], vec![3]],
        };
        Node::with_perm([0; 32], vec![], canon, 1, vec![3, 2, 1, 0])
    }

    #[test]
    fn covers_all_edges_with_eight_real_backups_and_keeps_zero_ties() {
        let mut n = node();
        let mut a = RootAllocator::default();
        assert_eq!(a.choice(&[1; 32], &n), None);
        for _ in 0..32 {
            let e = a.select([1; 32], &n);
            n.n[e] += 1;
        }
        assert_eq!(n.n, vec![8; 4]);
        assert_eq!(a.select([1; 32], &n), 3);
        assert_eq!(a.roots[&[1; 32]].active.len(), 4);
        assert_eq!(a.choice(&[1; 32], &n), Some(3));
    }

    #[test]
    fn halves_only_strictly_worse_edges_and_excludes_stale_eliminated_means() {
        let mut n = node();
        n.n.fill(8);
        n.wins = vec![0, 2, 4, 8];
        let mut a = RootAllocator::default();
        assert_eq!(a.select([1; 32], &n), 3);
        assert_eq!(a.roots[&[1; 32]].active, vec![3, 2]);
        n.n[3] = 80;
        n.n[2] = 80;
        assert_eq!(a.choice(&[1; 32], &n), Some(3));
    }

    #[test]
    fn puct_follows_the_prior_then_the_values() {
        let mut n = node();
        // Edge e's live index is e (menu rows 0..3).
        let prior = RootPrior {
            probs: vec![0.7, 0.1, 0.1, 0.1],
            c_puct: 1.0,
        };
        let mut a = RootAllocator::puct(prior.clone());
        assert!(a.is_puct());
        assert_eq!(a.select([1; 32], &n), 0);
        // Edge 0 keeps losing; the untried edges' first-play value (the
        // node mean, 0) plus their prior term overtakes it.
        n.n[0] = 10;
        n.visits = 10;
        assert_ne!(a.select([1; 32], &n), 0);
        // With every edge tried, a winning edge beats the prior's favorite.
        n.n = vec![10, 10, 10, 20];
        n.wins = vec![0, 0, 0, 20];
        n.visits = 50;
        assert_eq!(a.select([1; 32], &n), 3);
        // Equal priors and no visits: the seeded order breaks the tie.
        let flat = RootPrior::tempered(&[0.0; 4], 1.0, 1.0);
        assert_eq!(flat.select(&node()), 3);
        // The root choice is frozen_choice.
        assert_eq!(a.choice(&[1; 32], &n), n.frozen_choice());
    }

    #[test]
    fn tempered_prior_flattens_a_near_certain_policy() {
        let raw = RootPrior::tempered(&[0.0, -4.6], 1.0, 1.0);
        let flat = RootPrior::tempered(&[0.0, -4.6], 4.0, 1.0);
        assert!(raw.probs[1] < 0.011);
        assert!(flat.probs[1] > 0.23 && flat.probs[1] < 0.25);
    }

    #[test]
    fn never_allocates_interchangeable_skipped_edges() {
        let mut n = node();
        n.skip = vec![false, true, false, true];
        let mut a = RootAllocator::default();
        for _ in 0..16 {
            let e = a.select([1; 32], &n);
            assert!(e == 0 || e == 2);
            n.n[e] += 1;
        }
        assert_eq!(n.n, vec![8, 0, 8, 0]);
    }
}
