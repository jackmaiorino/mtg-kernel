//! The survivor tree F of the opponent's hand and its allocator (W1 design
//! v7 section 6.4, rulings 41 and 44).
//!
//! A node (K, m) means "at least m of the untracked cards now in the hand are
//! retained members of K", K a multiset of card names (card definitions).
//! Tracked cards, pins and later arrivals never count toward a node. The tree
//! is rebuilt from V's projected history at every export (a pure function of
//! that history), so R8b is never a latch kept outside it.

use std::collections::BTreeMap;

/// Multiset of card definitions.
pub(crate) type NamesV1 = BTreeMap<u16, u32>;

fn size(k: &NamesV1) -> u32 {
    k.values().sum()
}

/// abs(a minus b) for multisets.
fn difference_size(a: &NamesV1, b: &NamesV1) -> u32 {
    a.iter()
        .map(|(n, c)| c.saturating_sub(b.get(n).copied().unwrap_or(0)))
        .sum()
}

fn contains(outer: &NamesV1, inner: &NamesV1) -> bool {
    inner
        .iter()
        .all(|(n, c)| outer.get(n).copied().unwrap_or(0) >= *c)
}

fn add(a: &mut NamesV1, b: &NamesV1) {
    for (n, c) in b {
        *a.entry(*n).or_default() += c;
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct NodeV1 {
    pub k: NamesV1,
    pub m: i64,
    pub parent: Option<usize>,
    /// R8b: a later public event located a card of a name in K outside the
    /// hand while this node existed.
    pub tainted: bool,
    alive: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct SurvivorTreeV1 {
    nodes: Vec<NodeV1>,
    /// A laminarity assertion failed after some update (R8).
    pub laminarity_violation: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AllocationErrorV1 {
    /// More than 8 nodes or depth above 4 (R8).
    Unrepresentable,
    /// A laminarity assertion failed during the walk (R8).
    NotLaminar,
    /// A deficit the pool or the hand slots cannot fill, or a node fails the
    /// final verification (R8).
    Unfillable,
}

/// The order names are compared in: the card name, then the definition.
pub(crate) type NameKeyV1 = (&'static str, u16);

pub(crate) const MAX_NODES_V1: usize = 8;
pub(crate) const MAX_DEPTH_V1: usize = 4;

impl SurvivorTreeV1 {
    fn live(&self) -> impl Iterator<Item = usize> + '_ {
        (0..self.nodes.len()).filter(|&i| self.nodes[i].alive)
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.live().next().is_none()
    }

    pub(crate) fn len(&self) -> usize {
        self.live().count()
    }

    pub(crate) fn nodes(&self) -> Vec<NodeV1> {
        self.live().map(|i| self.nodes[i].clone()).collect()
    }

    pub(crate) fn tainted(&self) -> bool {
        self.live().any(|i| self.nodes[i].tainted)
    }

    pub(crate) fn clear(&mut self) {
        self.nodes.clear();
    }

    fn children(&self, i: usize) -> Vec<usize> {
        self.live()
            .filter(|&c| self.nodes[c].parent == Some(i))
            .collect()
    }

    fn tops(&self) -> Vec<usize> {
        self.live()
            .filter(|&c| self.nodes[c].parent.is_none())
            .collect()
    }

    pub(crate) fn depth(&self) -> usize {
        self.live()
            .map(|mut i| {
                let mut d = 1;
                while let Some(p) = self.nodes[i].parent {
                    d += 1;
                    i = p;
                }
                d
            })
            .max()
            .unwrap_or(0)
    }

    /// A card of the hand became tracked in place, or an untracked card of
    /// that name left identified: every node whose K holds the name loses one
    /// copy, and m decreases by 1.
    fn lose_name(&mut self, name: u16) {
        for i in self.live().collect::<Vec<_>>() {
            let node = &mut self.nodes[i];
            if let Some(c) = node.k.get_mut(&name) {
                *c -= 1;
                if *c == 0 {
                    node.k.remove(&name);
                }
                node.m -= 1;
            }
        }
        self.prune();
    }

    pub(crate) fn tracked_in_place(&mut self, name: u16) {
        self.lose_name(name);
    }

    pub(crate) fn identified_departure(&mut self, name: u16) {
        self.lose_name(name);
    }

    /// An unidentified departure from the hand; `tracked` holds the names of
    /// the physical facts just before it (T).
    pub(crate) fn unidentified_departure(&mut self, tracked: &[u16]) {
        if tracked.is_empty() {
            for i in self.live().collect::<Vec<_>>() {
                self.nodes[i].m -= 1;
            }
            self.prune();
            return;
        }
        let mut t = NamesV1::new();
        for n in tracked {
            *t.entry(*n).or_default() += 1;
        }
        let t_size = i64::from(size(&t));
        let tops = self.tops();
        let mut root_k = t.clone();
        let mut root_m = t_size - 1;
        for &i in &tops {
            add(&mut root_k, &self.nodes[i].k);
            root_m += self.nodes[i].m;
        }
        for i in self.live().collect::<Vec<_>>() {
            self.nodes[i].m -= 1;
        }
        let root = self.nodes.len();
        self.nodes.push(NodeV1 {
            k: root_k,
            m: root_m,
            parent: None,
            tainted: false,
            alive: true,
        });
        for i in tops {
            self.nodes[i].parent = Some(root);
        }
        self.nodes.push(NodeV1 {
            k: t,
            m: t_size - 1,
            parent: Some(root),
            tainted: false,
            alive: true,
        });
        self.prune();
    }

    /// R8b: every node whose K holds `name` is tainted.
    pub(crate) fn taint_name(&mut self, name: u16) {
        for i in self.live().collect::<Vec<_>>() {
            if self.nodes[i].k.contains_key(&name) {
                self.nodes[i].tainted = true;
            }
        }
    }

    fn remove(&mut self, i: usize) {
        let parent = self.nodes[i].parent;
        for c in self.children(i) {
            self.nodes[c].parent = parent;
        }
        self.nodes[i].alive = false;
    }

    /// Prunes nodes with m <= 0 and children implied by their parent
    /// (m_c <= m_p - abs(K_p minus K_c)), until none is left.
    fn prune(&mut self) {
        loop {
            let dead = self.live().find(|&i| self.nodes[i].m <= 0);
            if let Some(i) = dead {
                self.remove(i);
                continue;
            }
            let implied = self.live().find(|&c| {
                self.nodes[c].parent.is_some_and(|p| {
                    let (pk, pm) = (&self.nodes[p].k, self.nodes[p].m);
                    self.nodes[c].m <= pm - i64::from(difference_size(pk, &self.nodes[c].k))
                })
            });
            if let Some(c) = implied {
                self.remove(c);
                continue;
            }
            break;
        }
        // Compact dead nodes away so indices stay small.
        if self.nodes.iter().any(|n| !n.alive) {
            let mut map = vec![None; self.nodes.len()];
            let mut kept = Vec::new();
            for (i, n) in self.nodes.iter().enumerate() {
                if n.alive {
                    map[i] = Some(kept.len());
                    kept.push(n.clone());
                }
            }
            for n in &mut kept {
                n.parent = n.parent.and_then(|p| map[p]);
            }
            self.nodes = kept;
        }
    }

    /// Laminarity: the children's K multisets sum inside their parent's K.
    pub(crate) fn assert_laminar(&mut self) {
        for i in self.live().collect::<Vec<_>>() {
            let mut sum = NamesV1::new();
            for c in self.children(i) {
                add(&mut sum, &self.nodes[c].k);
            }
            if !contains(&self.nodes[i].k, &sum) {
                self.laminarity_violation = true;
            }
        }
    }

    /// Children before parents; siblings by smallest name, then size.
    fn post_order(&self, name: &dyn Fn(u16) -> NameKeyV1) -> Vec<usize> {
        let key = |i: usize| {
            let k = &self.nodes[i].k;
            (k.keys().map(|&n| name(n)).min(), size(k), i)
        };
        fn visit(
            t: &SurvivorTreeV1,
            i: usize,
            key: &dyn Fn(usize) -> (Option<NameKeyV1>, u32, usize),
            out: &mut Vec<usize>,
        ) {
            let mut ch = t.children(i);
            ch.sort_by_key(|&c| key(c));
            for c in ch {
                visit(t, c, key, out);
            }
            out.push(i);
        }
        let mut tops = self.tops();
        tops.sort_by_key(|&c| key(c));
        let mut out = Vec::new();
        for i in tops {
            visit(self, i, &key, &mut out);
        }
        out
    }

    /// Step 5's allocation with residual multiplicities. `pool` is Pool5
    /// (the owner's decklist minus public zones, tracked objects and pinned
    /// definitions); `slots` the unknown hand slots available. Returns the
    /// `known_name` occupants in placement order.
    pub(crate) fn allocate(
        &self,
        pool: &NamesV1,
        slots: usize,
        name: &dyn Fn(u16) -> NameKeyV1,
    ) -> Result<Vec<u16>, AllocationErrorV1> {
        if self.laminarity_violation {
            return Err(AllocationErrorV1::NotLaminar);
        }
        if self.len() > MAX_NODES_V1 || self.depth() > MAX_DEPTH_V1 {
            return Err(AllocationErrorV1::Unrepresentable);
        }
        let mut pool = pool.clone();
        let mut placed: Vec<u16> = Vec::new();
        // Occupants placed for each node and its descendants.
        let mut sub: BTreeMap<usize, NamesV1> = BTreeMap::new();
        for i in self.post_order(name) {
            let mut u = NamesV1::new();
            for c in self.children(i) {
                if let Some(cs) = sub.get(&c) {
                    add(&mut u, cs);
                }
            }
            let node = &self.nodes[i];
            let inter = |u: &NamesV1| -> i64 {
                node.k
                    .iter()
                    .map(|(n, c)| i64::from((*c).min(u.get(n).copied().unwrap_or(0))))
                    .sum()
            };
            while node.m - inter(&u) > 0 {
                let pick = node
                    .k
                    .iter()
                    .filter(|(n, c)| {
                        **c > u.get(n).copied().unwrap_or(0)
                            && pool.get(n).copied().unwrap_or(0) > 0
                    })
                    .map(|(&n, _)| n)
                    .min_by_key(|&n| name(n));
                let Some(n) = pick else {
                    return Err(AllocationErrorV1::Unfillable);
                };
                if placed.len() >= slots {
                    return Err(AllocationErrorV1::Unfillable);
                }
                *pool.get_mut(&n).expect("picked from the pool") -= 1;
                *u.entry(n).or_default() += 1;
                placed.push(n);
            }
            sub.insert(i, u);
        }
        // Final verification on the same survivor domain: every node's
        // subtree occupants, and the hand's survivor occupants as a whole.
        let mut hand = NamesV1::new();
        for n in &placed {
            *hand.entry(*n).or_default() += 1;
        }
        for i in self.live() {
            let node = &self.nodes[i];
            let inter = |u: &NamesV1| -> i64 {
                node.k
                    .iter()
                    .map(|(n, c)| i64::from((*c).min(u.get(n).copied().unwrap_or(0))))
                    .sum()
            };
            let own = sub.get(&i).map(inter).unwrap_or(0);
            if own < node.m || inter(&hand) < node.m {
                return Err(AllocationErrorV1::Unfillable);
            }
        }
        Ok(placed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn by_id(n: u16) -> NameKeyV1 {
        ("", n)
    }

    fn names(xs: &[u16]) -> NamesV1 {
        let mut k = NamesV1::new();
        for x in xs {
            *k.entry(*x).or_default() += 1;
        }
        k
    }

    // Names: 1 Brainstorm, 2 Mystic, 3 Ponder, 4 Terror (sorted ids stand
    // in for sorted names).
    #[test]
    fn spy_versus_terror_keeps_one_survivor_of_the_three() {
        let mut f = SurvivorTreeV1::default();
        // Fiend reveal of 5, Dispel exiled, Brainstorm cast: physical facts
        // Terror, Ponder, Mystic; draw 3 (no change); two unidentified returns.
        f.unidentified_departure(&[4, 3, 2]);
        assert_eq!(f.nodes(), vec![NodeV1 { k: names(&[2, 3, 4]), m: 2, parent: None, tainted: false, alive: true }]);
        f.unidentified_departure(&[]);
        let nodes = f.nodes();
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].m, 1);
        let pool = names(&[1, 1, 2, 3, 4, 5, 5]);
        assert_eq!(f.allocate(&pool, 4, &by_id), Ok(vec![2]));
    }

    #[test]
    fn residual_multiplicity_picks_only_copies_that_grow_the_intersection() {
        // K = {A, B}, m = 2 with one A already placed for a child.
        let mut f = SurvivorTreeV1::default();
        f.nodes.push(NodeV1 { k: names(&[1, 2]), m: 2, parent: None, tainted: false, alive: true });
        f.nodes.push(NodeV1 { k: names(&[1]), m: 1, parent: Some(0), tainted: false, alive: true });
        let pool = names(&[1, 1, 1, 2]);
        assert_eq!(f.allocate(&pool, 4, &by_id), Ok(vec![1, 2]));
        // Without a B left the deficit cannot be filled: R8.
        assert_eq!(f.allocate(&names(&[1, 1, 1]), 4, &by_id), Err(AllocationErrorV1::Unfillable));
        assert_eq!(f.allocate(&pool, 1, &by_id), Err(AllocationErrorV1::Unfillable));
    }

    #[test]
    fn empty_tracked_set_decrements_in_place_and_two_brainstorms_stay_shallow() {
        let mut f = SurvivorTreeV1::default();
        f.unidentified_departure(&[1, 2, 3]);
        f.unidentified_departure(&[]);
        f.unidentified_departure(&[]);
        assert!(f.is_empty(), "three departures exhaust a node of m = 2");
        let mut f = SurvivorTreeV1::default();
        f.unidentified_departure(&[1, 2, 3, 4]);
        f.unidentified_departure(&[]);
        assert_eq!(f.len(), 1);
        assert_eq!(f.depth(), 1);
    }

    #[test]
    fn tracked_in_place_and_identified_departure_shrink_nodes() {
        let mut f = SurvivorTreeV1::default();
        f.unidentified_departure(&[1, 2]);
        assert_eq!(f.nodes()[0].m, 1);
        f.tracked_in_place(3);
        assert_eq!(f.nodes()[0].m, 1, "a name outside K changes nothing");
        f.identified_departure(1);
        assert!(f.is_empty());
    }

    #[test]
    fn taint_marks_nodes_holding_the_name_until_they_are_pruned() {
        let mut f = SurvivorTreeV1::default();
        f.unidentified_departure(&[7, 7, 7, 9]);
        f.unidentified_departure(&[]);
        assert!(!f.tainted());
        f.taint_name(9);
        assert!(f.tainted());
        f.unidentified_departure(&[]);
        f.unidentified_departure(&[]);
        assert!(f.is_empty() && !f.tainted());
    }
}
