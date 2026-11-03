//! Value-leaf E search, `s4a-vl-v1` (collab LANES/lead/w5/VALUE-LEAF-DESIGN.md
//! and VALUE-LEAF-RULINGS.md, the staged prototype). Opt-in through
//! `SearchOptions::value_leaf`; with it unset nothing here runs and every
//! existing search is unchanged.
//!
//! A simulation descends the information-set tree exactly as E does. After
//! the end of its tree prefix (the expansion, or the first focal choice past
//! the depth limit: the anchor), both seats play their policies until the
//! first focal decision with two or more legal actions that satisfies the
//! horizon (H0: at or after the anchor; T0: in a later engine turn than the
//! anchor's). There the searching network scores the focal player's
//! actor-visible view once, and its value v becomes the backup
//! u = clamp((v + 1) / 2, 0, 1), stored as an integer number of 2^-20 units.
//! A natural terminal reached first backs up its exact outcome in the same
//! units (win 2^20, draw 2^19, loss 0). Edge means, selection and the root
//! choice read the integer sums, so trees merge by exact integer addition.
//!
//! The value head is trained on RT1's GAE (lambda 0.9) returns, which bottom
//! out in terminal win/loss outcomes: the leaf value is a model estimate of
//! terminal outcomes, never a shaped reward.

use super::tree::{Node, Tree};
use serde_json::{json, Value};

pub(crate) const SCHEMA_V1: &str = "s4a-vl-v1";
/// One full win, in value units.
pub(crate) const UNIT: u64 = 1 << 20;

/// Where a simulation stops for its value leaf.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Horizon {
    /// The first focal choice at or after the anchor.
    H0,
    /// The first focal choice in a later engine turn than the anchor's.
    T0,
}

impl Horizon {
    pub(crate) fn parse(label: &str) -> Result<Self, String> {
        match label {
            "h0" => Ok(Self::H0),
            "t0" => Ok(Self::T0),
            other => Err(format!("value-leaf horizon must be h0 or t0, got {other:?}")),
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::H0 => "h0",
            Self::T0 => "t0",
        }
    }

    /// Whether a focal choice in engine turn `turn` is the leaf for an
    /// anchor in turn `anchor`.
    pub(crate) fn reached(self, anchor: u32, turn: u32) -> bool {
        match self {
            Self::H0 => true,
            Self::T0 => turn > anchor,
        }
    }
}

/// The opt-in value-leaf configuration of one search.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ValueLeafV1 {
    pub(crate) horizon: Horizon,
}

impl ValueLeafV1 {
    pub(crate) fn json(&self) -> Value {
        json!({"schema":SCHEMA_V1,"horizon":self.horizon.label(),"unit":UNIT,
            "mapping":"clamp((v+1)/2,0,1), round half to even, x 2^20","natural":{"win":UNIT,"draw":UNIT / 2,"loss":0}})
    }
}

/// A raw value-head output in value units; a non-finite value is a fault.
pub(crate) fn units(v: f32) -> Result<u32, String> {
    if !v.is_finite() {
        return Err(format!("value leaf: non-finite value {v}"));
    }
    let u = ((f64::from(v) + 1.0) / 2.0).clamp(0.0, 1.0);
    // u x 2^20 is exact in f64; the rounding is the only inexact step.
    Ok((u * UNIT as f64).round_ties_even() as u32)
}

/// A natural ending in value units.
pub(crate) fn natural_units(win: bool, draw: bool) -> u32 {
    if win {
        UNIT as u32
    } else if draw {
        (UNIT / 2) as u32
    } else {
        0
    }
}

/// Per-search value-leaf census (receipts; never read by the search).
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Census {
    pub(crate) leaves: u64,
    pub(crate) natural: u64,
    pub(crate) clamped_low: u64,
    pub(crate) clamped_high: u64,
    pub(crate) raw_min: Option<f32>,
    pub(crate) raw_max: Option<f32>,
    /// Sum of leaf units (exact), for a mean leaf value.
    pub(crate) leaf_units: u64,
}

impl Census {
    pub(crate) fn leaf(&mut self, raw: f32, u: u32) {
        self.leaves += 1;
        self.clamped_low += u64::from(raw < -1.0);
        self.clamped_high += u64::from(raw > 1.0);
        self.raw_min = Some(self.raw_min.map_or(raw, |m| m.min(raw)));
        self.raw_max = Some(self.raw_max.map_or(raw, |m| m.max(raw)));
        self.leaf_units += u64::from(u);
    }

    pub(crate) fn json(&self) -> Value {
        json!({"leaves":self.leaves,"natural":self.natural,"clamped_low":self.clamped_low,
            "clamped_high":self.clamped_high,"raw_min":self.raw_min,"raw_max":self.raw_max,
            "leaf_units":self.leaf_units})
    }
}

/// The root's edge statistics in canonical edge order: (canonical edge,
/// visits, value units, natural draws).
pub(crate) type RootEdges = Vec<(Vec<u8>, u64, u64, u64)>;

/// The root node of a value-leaf tree (the depth-1 node with the zero parent).
pub(crate) fn root(tree: &Tree) -> Option<&Node> {
    tree.nodes
        .values()
        .find(|n| n.depth == 1 && n.parent == [0; 32])
}

/// The root's edges, or None when the tree has no root or is not a
/// value-leaf tree.
pub(crate) fn root_edges(tree: &Tree) -> Option<RootEdges> {
    let r = root(tree)?;
    if r.vsum.len() != r.n.len() {
        return None;
    }
    Some(
        (0..r.n.len())
            .map(|a| (r.canon.edges[a].clone(), r.n[a], r.vsum[a], r.draws[a]))
            .collect(),
    )
}

/// Root-parallel merge: integer sums in tree order. Every tree must show the
/// same canonical root edges; a tree that differs is refused, never merged.
pub(crate) fn merge(trees: &[RootEdges]) -> Result<RootEdges, String> {
    let first = trees.first().ok_or("merge of no trees")?;
    let mut out: RootEdges = first.iter().map(|(e, _, _, _)| (e.clone(), 0, 0, 0)).collect();
    for (k, t) in trees.iter().enumerate() {
        if t.len() != out.len() || t.iter().zip(&out).any(|(a, b)| a.0 != b.0) {
            return Err(format!("tree {k}: root edges differ"));
        }
        for (o, x) in out.iter_mut().zip(t) {
            o.1 += x.1;
            o.2 += x.2;
            o.3 += x.3;
        }
    }
    Ok(out)
}

/// `pick-v2` on merged root edges: most visits, then the higher mean value
/// (exact: S1 x n2 against S2 x n1 in u128), then the lowest canonical edge
/// index. None when no edge was visited.
pub(crate) fn pick_v2(edges: &RootEdges) -> Option<usize> {
    let mut best: Option<usize> = None;
    for (i, e) in edges.iter().enumerate() {
        if e.1 == 0 {
            continue;
        }
        best = Some(match best {
            None => i,
            Some(b) => {
                let f = &edges[b];
                let better = e.1 > f.1
                    || (e.1 == f.1 && u128::from(e.2) * u128::from(f.1) > u128::from(f.2) * u128::from(e.1));
                if better {
                    i
                } else {
                    b
                }
            }
        });
    }
    best
}

/// Q-hat per root edge (value units over visits); None for unvisited edges.
pub(crate) fn q_hat(edges: &RootEdges) -> Vec<Option<f64>> {
    edges
        .iter()
        .map(|e| (e.1 > 0).then(|| e.2 as f64 / (e.1 as f64 * UNIT as f64)))
        .collect()
}

/// Receipt of a root's merged edges.
pub(crate) fn edges_json(edges: &RootEdges) -> Value {
    let hex = |b: &[u8]| -> String {
        use sha2::{Digest, Sha256};
        Sha256::digest(b).iter().map(|x| format!("{x:02x}")).collect()
    };
    Value::Array(
        edges
            .iter()
            .map(|(e, n, s, d)| json!({"edge_sha256":hex(e),"n":n,"value_units":s,"draws":d}))
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn units_map_clamp_and_round_half_even() {
        assert_eq!(units(1.0).unwrap(), UNIT as u32);
        assert_eq!(units(-1.0).unwrap(), 0);
        assert_eq!(units(0.0).unwrap(), (UNIT / 2) as u32);
        assert_eq!(units(3.5).unwrap(), UNIT as u32);
        assert_eq!(units(-7.0).unwrap(), 0);
        assert!(units(f32::NAN).is_err());
        assert!(units(f32::INFINITY).is_err());
        // v = (2k + 1) / 2^20 - 1 is exact in f32 and lands on k + 0.5 units:
        // ties round to even.
        assert_eq!(units((1.0f64 / UNIT as f64 - 1.0) as f32).unwrap(), 0);
        assert_eq!(units((3.0f64 / UNIT as f64 - 1.0) as f32).unwrap(), 2);
    }

    #[test]
    fn natural_units_are_exact() {
        assert_eq!(natural_units(true, false), UNIT as u32);
        assert_eq!(natural_units(false, true), (UNIT / 2) as u32);
        assert_eq!(natural_units(false, false), 0);
    }

    #[test]
    fn horizon_rules() {
        assert!(Horizon::H0.reached(5, 5));
        assert!(!Horizon::T0.reached(5, 5));
        assert!(Horizon::T0.reached(5, 6));
        assert_eq!(Horizon::parse("t0").unwrap(), Horizon::T0);
        assert!(Horizon::parse("t1").is_err());
    }

    fn edges(rows: &[(u8, u64, u64)]) -> RootEdges {
        rows.iter().map(|&(e, n, s)| (vec![e], n, s, 0)).collect()
    }

    #[test]
    fn merge_sums_in_order_and_refuses_different_roots() {
        let a = edges(&[(0, 3, 10), (1, 5, 20)]);
        let b = edges(&[(0, 4, 1), (1, 0, 0)]);
        assert_eq!(merge(&[a.clone(), b.clone()]).unwrap(), edges(&[(0, 7, 11), (1, 5, 20)]));
        // Order of summation cannot change an integer sum.
        assert_eq!(merge(&[a.clone(), b.clone()]), merge(&[b, a.clone()]));
        assert!(merge(&[a, edges(&[(0, 1, 1), (2, 1, 1)])]).is_err());
    }

    #[test]
    fn pick_v2_visits_then_exact_mean_then_index() {
        // Most visits wins.
        assert_eq!(pick_v2(&edges(&[(0, 3, 3 * UNIT), (1, 5, 0)])), Some(1));
        // Equal visits: higher mean.
        assert_eq!(pick_v2(&edges(&[(0, 4, 1), (1, 4, 2)])), Some(1));
        // Full tie: lowest index.
        assert_eq!(pick_v2(&edges(&[(0, 4, 2), (1, 4, 2)])), Some(0));
        // Unvisited edges never win.
        assert_eq!(pick_v2(&edges(&[(0, 0, 0), (1, 0, 0)])), None);
        assert_eq!(q_hat(&edges(&[(0, 2, UNIT), (1, 0, 0)])), vec![Some(0.5), None]);
    }
}
