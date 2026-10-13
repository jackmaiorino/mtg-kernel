//! Opt-in, anytime successive halving at the first search choice only.
//! Quotas and means use actual natural terminal backups. Ties at the cut
//! survive, including an all-loss prefix. No learned prior enters allocation.

use super::tree::{Key, MIN_EXEC_BACKUPS, Node};
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum RootAllocation {
    #[default]
    Uct,
    Halving,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct SearchOptions {
    pub(crate) allocation: RootAllocation,
    pub(crate) sharing: bool,
    pub(crate) cpu_seconds: Option<f64>,
    /// Attempted simulations, including rejected samples, for exact replay.
    pub(crate) work_limit: Option<u64>,
}

#[derive(Default)]
pub(crate) struct RootAllocator {
    roots: HashMap<Key, Round>,
}

struct Round {
    active: Vec<usize>,
    quota: u64,
}

impl RootAllocator {
    pub(crate) fn select(&mut self, key: Key, node: &Node) -> usize {
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
