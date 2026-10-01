//! Heuristic graph estimate. Never a terminal proof or calibrated bound.
use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum BackupMode {
    Off,
    Report,
    Blend,
}
#[derive(Debug, Default, Clone, PartialEq, Serialize)]
pub(crate) struct EdgeEstimate {
    pub terminal_count: u32,
    pub terminal_sum: i64,
    pub cutoff_count: u32,
    pub cutoff_sum: i64,
    pub children: Vec<(usize, u32)>,
    pub value: Option<i64>,
}

#[cfg(test)]
mod tests {
    use super::*;
    fn n(actor: PlayerId, visits: u32, actions: usize) -> Node {
        Node {
            key: [0; 32],
            actor,
            visits,
            prior: vec![1_000_000 / actions as u32; actions],
            actions: (0..actions)
                .map(|_| SearchActionStatV1 {
                    visits: 0,
                    value_sum: 0,
                    child_nodes: vec![],
                })
                .collect(),
            witness: None,
        }
    }
    fn add(e: &mut Estimator, t: &mut [Node], i: usize, a: usize, s: Sample) {
        if let Sample::Child(c) = &s {
            if !t[i].actions[a].child_nodes.contains(c) {
                t[i].actions[a].child_nodes.push(*c);
            }
        }
        t[i].actions[a].visits += 1;
        e.record(i, a, s).unwrap();
    }
    #[test]
    fn v4_search_estimate_sparse_optimism_then_pooled_worlds() {
        for root in [PlayerId::P0, PlayerId::P1] {
            let mut t = vec![n(root, 4, 2)];
            let mut e = Estimator::new(BackupMode::Report);
            e.push(1, 0, 2);
            add(&mut e, &mut t, 0, 0, Sample::Terminal(10000));
            add(&mut e, &mut t, 0, 1, Sample::Terminal(10000));
            e.recompute(&t, root).unwrap();
            assert_eq!(e.nodes[0].h, 10000);
            add(&mut e, &mut t, 0, 0, Sample::Terminal(-10000));
            add(&mut e, &mut t, 0, 1, Sample::Terminal(-10000));
            e.recompute(&t, root).unwrap();
            assert_eq!(e.nodes[0].h, 0);
            assert_eq!(e.selected_by_estimate, 0);
        }
    }
    #[test]
    fn v4_search_estimate_terminal_cutoff_live_mixture_and_accounting() {
        let root = PlayerId::P0;
        let mut t = vec![n(root, 4, 1), n(root, 2, 1)];
        let mut e = Estimator::new(BackupMode::Blend);
        e.push(2, 0, 1);
        e.push(1, -3000, 1);
        add(&mut e, &mut t, 0, 0, Sample::Terminal(10000));
        add(&mut e, &mut t, 0, 0, Sample::Terminal(-10000));
        for _ in 0..2 {
            add(&mut e, &mut t, 0, 0, Sample::Child(1));
        }
        e.recompute(&t, root).unwrap();
        assert_eq!(e.nodes[0].h, -1500);
        assert_eq!(
            choose_estimated(&t[0], root, InteriorBonus::PriorFree, Some(&e.nodes[0])).unwrap(),
            0
        );
        add(&mut e, &mut t, 0, 0, Sample::Cutoff(4000));
        t[0].visits += 1;
        e.recompute(&t, root).unwrap();
        assert_eq!(e.nodes[0].h, -400);
        t[1].visits += 1;
        assert_eq!(e.recompute(&t, root), Err(Error::CorruptTree));
    }
    #[test]
    fn v4_search_estimate_diamond_refreshes_all_parents_and_is_idempotent() {
        let root = PlayerId::P0;
        let mut t = vec![
            n(root, 4, 2),
            n(root, 2, 1),
            n(root.opponent(), 2, 1),
            n(root, 4, 1),
        ];
        let mut e = Estimator::new(BackupMode::Report);
        for (r, a) in [(3, 2), (2, 1), (2, 1), (1, 1)] {
            e.push(r, -3000, a);
        }
        for _ in 0..2 {
            add(&mut e, &mut t, 0, 0, Sample::Child(1));
            add(&mut e, &mut t, 0, 1, Sample::Child(2));
            add(&mut e, &mut t, 1, 0, Sample::Child(3));
            add(&mut e, &mut t, 2, 0, Sample::Child(3));
        }
        e.recompute(&t, root).unwrap();
        assert_eq!(e.nodes[0].h, -3000);
        for _ in 0..4 {
            add(&mut e, &mut t, 3, 0, Sample::Terminal(10000));
        }
        e.recompute(&t, root).unwrap();
        assert!(e.nodes.iter().all(|n| n.h == 10000));
        let before = e.nodes.clone();
        e.recompute(&t, root).unwrap();
        assert_eq!(before, e.nodes);
        e.nodes[3].remaining = 2;
        assert_eq!(e.recompute(&t, root), Err(Error::CorruptTree));
    }
    #[test]
    fn v4_search_estimate_actor_signs_do_not_assume_alternation() {
        for root in [PlayerId::P0, PlayerId::P1] {
            for last in [root, root.opponent()] {
                let actors = [root, root, root.opponent(), last];
                let mut t = actors
                    .iter()
                    .enumerate()
                    .map(|(i, a)| n(*a, 2, if i == 3 { 2 } else { 1 }))
                    .collect::<Vec<_>>();
                let mut e = Estimator::new(BackupMode::Blend);
                for i in 0..4 {
                    e.push(4 - i as u16, 0, if i == 3 { 2 } else { 1 });
                }
                for i in 0..3 {
                    for _ in 0..2 {
                        add(&mut e, &mut t, i, 0, Sample::Child(i + 1));
                    }
                }
                add(&mut e, &mut t, 3, 0, Sample::Terminal(10000));
                add(&mut e, &mut t, 3, 1, Sample::Terminal(-10000));
                e.recompute(&t, root).unwrap();
                assert_eq!(e.nodes[0].h, if last == root { 10000 } else { -10000 });
                assert_eq!(
                    choose_estimated(&t[3], root, InteriorBonus::PriorFree, Some(&e.nodes[3]))
                        .unwrap(),
                    if last == root { 0 } else { 1 }
                );
            }
        }
    }
}
#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct NodeEstimate {
    pub remaining: u16,
    pub leaf: i64,
    pub h: i64,
    pub edges: Vec<EdgeEstimate>,
}
#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct Estimator {
    pub mode: BackupMode,
    pub nodes: Vec<NodeEstimate>,
    pub recompute_passes: u32,
    pub selected_by_estimate: u32,
}
pub(super) enum Sample {
    Terminal(i64),
    Cutoff(i64),
    Child(usize),
}
impl Estimator {
    pub(super) fn new(mode: BackupMode) -> Self {
        Self {
            mode,
            nodes: vec![],
            recompute_passes: 0,
            selected_by_estimate: 0,
        }
    }
    pub(super) fn push(&mut self, remaining: u16, leaf: i64, actions: usize) {
        self.nodes.push(NodeEstimate {
            remaining,
            leaf,
            h: leaf,
            edges: vec![EdgeEstimate::default(); actions],
        });
    }
    pub(super) fn record(&mut self, node: usize, action: usize, sample: Sample) -> Result<()> {
        let e = self
            .nodes
            .get_mut(node)
            .and_then(|n| n.edges.get_mut(action))
            .ok_or(Error::CorruptTree)?;
        match sample {
            Sample::Terminal(v) => {
                e.terminal_count += 1;
                e.terminal_sum += v;
            }
            Sample::Cutoff(v) => {
                e.cutoff_count += 1;
                e.cutoff_sum += v;
            }
            Sample::Child(child) => {
                if let Some((_, n)) = e.children.iter_mut().find(|(i, _)| *i == child) {
                    *n += 1
                } else {
                    e.children.push((child, 1))
                }
            }
        }
        Ok(())
    }
    pub(super) fn recompute(&mut self, tree: &[Node], root: PlayerId) -> Result<()> {
        if self.nodes.len() != tree.len() || tree.is_empty() {
            return Err(Error::CorruptTree);
        }
        let mut order = (0..tree.len()).collect::<Vec<_>>();
        order.sort_by_key(|i| self.nodes[*i].remaining);
        let mut incoming = vec![0u64; tree.len()];
        for i in order {
            if self.nodes[i].edges.len() != tree[i].actions.len() {
                return Err(Error::CorruptTree);
            }
            let mut best = None;
            for (a, stat) in tree[i].actions.iter().enumerate() {
                let edge = &self.nodes[i].edges[a];
                if edge.children.iter().map(|(c, _)| *c).collect::<Vec<_>>() != stat.child_nodes {
                    return Err(Error::CorruptTree);
                }
                let mut count = u64::from(edge.terminal_count) + u64::from(edge.cutoff_count);
                let mut sum = i128::from(edge.terminal_sum) + i128::from(edge.cutoff_sum);
                for &(child, n) in &edge.children {
                    let c = self.nodes.get(child).ok_or(Error::CorruptTree)?;
                    if n == 0 || c.remaining.checked_add(1) != Some(self.nodes[i].remaining) {
                        return Err(Error::CorruptTree);
                    }
                    count += u64::from(n);
                    incoming[child] += u64::from(n);
                    sum += i128::from(n) * i128::from(c.h);
                }
                if count != u64::from(stat.visits) {
                    return Err(Error::CorruptTree);
                }
                let v = if count == 0 {
                    None
                } else {
                    Some(i64::try_from(sum / i128::from(count)).map_err(|_| Error::CorruptTree)?)
                };
                self.nodes[i].edges[a].value = v;
                if let Some(v) = v {
                    best = Some(best.map_or(v, |old: i64| {
                        if tree[i].actor == root {
                            old.max(v)
                        } else {
                            old.min(v)
                        }
                    }));
                }
            }
            self.nodes[i].h = best.unwrap_or(self.nodes[i].leaf);
        }
        if incoming[0] != 0
            || tree
                .iter()
                .enumerate()
                .skip(1)
                .any(|(i, n)| incoming[i] != u64::from(n.visits))
        {
            return Err(Error::CorruptTree);
        }
        self.selected_by_estimate = self.nodes[0]
            .edges
            .iter()
            .enumerate()
            .filter_map(|(i, e)| e.value.map(|v| (i, v)))
            .fold(None, |best: Option<(usize, i64)>, x| {
                if best.is_none_or(|(_, v)| x.1 > v) {
                    Some(x)
                } else {
                    best
                }
            })
            .map_or(0, |(i, _)| i as u32);
        self.recompute_passes += 1;
        Ok(())
    }
}
