//! Target-identity probe (collab LANES/search-identity-audit-plan-20261011):
//! short paired engine checks after selection, never read by search.
//!
//! For two captured sibling nodes whose canonical views differ only in stack
//! target handles, the differing raw targets X (in A) and Y (in B) are found
//! through each capture's raw-to-canonical mapping. In one private snapshot
//! of A's simulation, the opponent decision whose recorded action has a legal
//! counterpart with X and Y exchanged is cloned twice; the recorded action and
//! its counterpart are applied with the same RNG state, and both clones then
//! advance in lockstep: the acting policy's highest-scored action on the
//! first clone and its exchanged counterpart on the second, stopping at a
//! terminal, at a choice with no exchanged counterpart, or at the shared
//! action-pair and transition limits. At each decision the probe records
//! whether the raw acting views are equal after exchanging X and Y (unordered
//! lists compared as multisets), whether the canonical views are equal, and
//! the largest difference between corresponding policy scores. Matching
//! successors support, but do not prove, interchangeability.

use super::arms::{Capture, Roles};
use super::play::{acting, decision};
use super::tree::{canon, canon_values, raw_decision, Key};
use crate::ids::PlayerId;
use serde_json::{json, Value};

/// Lists the canonicalizer treats as unordered (`tree::UNORDERED_LISTS`).
const UNORDERED: [&str; 10] = [
    "own_hand",
    "known_hand_cards",
    "battlefield",
    "graveyards",
    "exile",
    "continuous_effects",
    "object_relations",
    "exile_play_permissions",
    "historical_public_sources",
    "cards",
];

/// Exchanges object handles `x` and `y` everywhere (`arena_id` and
/// `attachments`).
pub(crate) fn swap_handles(v: &mut Value, x: u64, y: u64) {
    let sw = |id: u64| {
        if id == x {
            y
        } else if id == y {
            x
        } else {
            id
        }
    };
    match v {
        Value::Object(m) => {
            if let Some(id) = m.get("arena_id").and_then(Value::as_u64) {
                m.insert("arena_id".into(), Value::from(sw(id)));
            }
            if let Some(Value::Array(a)) = m.get_mut("attachments") {
                for e in a.iter_mut() {
                    if let Some(id) = e.as_u64() {
                        *e = Value::from(sw(id));
                    }
                }
            }
            for (k, e) in m.iter_mut() {
                if k != "attachments" {
                    swap_handles(e, x, y);
                }
            }
        }
        Value::Array(a) => a.iter_mut().for_each(|e| swap_handles(e, x, y)),
        _ => {}
    }
}

/// Unordered lists sorted by their full JSON (multiset comparison); every
/// other list keeps its order. The projection hash is dropped (it is a hash
/// over handles and cannot be exchanged).
pub(crate) fn as_multisets(v: &Value) -> Value {
    fn walk(v: &mut Value) {
        match v {
            Value::Object(m) => {
                m.remove("visible_projection_hash");
                for (k, e) in m.iter_mut() {
                    walk(e);
                    if UNORDERED.contains(&k.as_str()) {
                        if let Value::Array(a) = e {
                            if a.iter().all(Value::is_array) {
                                for inner in a.iter_mut() {
                                    if let Value::Array(i) = inner {
                                        i.sort_by_cached_key(|x| x.to_string());
                                    }
                                }
                            } else {
                                a.sort_by_cached_key(|x| x.to_string());
                            }
                        }
                    }
                }
            }
            Value::Array(a) => a.iter_mut().for_each(walk),
            _ => {}
        }
    }
    let mut x = v.clone();
    walk(&mut x);
    x
}

fn diff_paths(a: &Value, b: &Value, path: &str, out: &mut Vec<String>) {
    if a == b || out.len() >= 64 {
        return;
    }
    match (a, b) {
        (Value::Object(x), Value::Object(y)) => {
            let mut keys: Vec<&String> = x.keys().chain(y.keys()).collect();
            keys.sort();
            keys.dedup();
            for k in keys {
                diff_paths(
                    x.get(k).unwrap_or(&Value::Null),
                    y.get(k).unwrap_or(&Value::Null),
                    &format!("{path}/{k}"),
                    out,
                );
            }
        }
        (Value::Array(x), Value::Array(y)) if x.len() == y.len() => {
            for (i, (p, q)) in x.iter().zip(y).enumerate() {
                diff_paths(p, q, &format!("{path}/{i}"), out);
            }
        }
        _ => out.push(path.to_owned()),
    }
}

fn get_path<'a>(v: &'a Value, path: &str) -> Option<&'a Value> {
    path.split('/').skip(1).try_fold(v, |v, p| match v {
        Value::Object(m) => m.get(p),
        Value::Array(a) => a.get(p.parse::<usize>().ok()?),
        _ => None,
    })
}

/// Shared limits of all probes in one run.
pub(crate) struct Budget {
    pub(crate) pairs: u32,
    pub(crate) transitions: u32,
}

/// One pair probe; `Err` explains why no probe could run.
pub(crate) fn probe_pair(
    roles: &Roles,
    opp: usize,
    focal: PlayerId,
    cap: &Capture,
    a_key: &Key,
    b_key: &Key,
    budget: &mut Budget,
) -> Result<Value, String> {
    let a = cap.committed.get(a_key).ok_or("A not captured")?;
    let b = cap.committed.get(b_key).ok_or("B not captured")?;
    let ca: Value = serde_json::from_slice(
        &canon_values(a.raw_obs.clone(), a.raw_menu.clone())
            .map_err(|e| format!("{e:?}"))?
            .0
            .obs,
    )
    .map_err(|e| e.to_string())?;
    let cb: Value = serde_json::from_slice(
        &canon_values(b.raw_obs.clone(), b.raw_menu.clone())
            .map_err(|e| format!("{e:?}"))?
            .0
            .obs,
    )
    .map_err(|e| e.to_string())?;
    let mut paths = Vec::new();
    diff_paths(&ca, &cb, "", &mut paths);
    if paths.is_empty() || !paths.iter().all(|p| p.ends_with("/arena_id")) {
        return Err(format!("canonical views differ beyond handles: {paths:?}"));
    }
    // Raw handles behind each differing canonical handle.
    let mut pairs = Vec::new();
    for p in &paths {
        let ia = get_path(&ca, p).and_then(Value::as_u64).ok_or("bad path")? as usize;
        let ib = get_path(&cb, p).and_then(Value::as_u64).ok_or("bad path")? as usize;
        let ra = *a.mapping.order.get(ia).ok_or("unmapped handle")?;
        let rb = *b.mapping.order.get(ib).ok_or("unmapped handle")?;
        if !pairs.contains(&(ra, rb)) {
            pairs.push((ra, rb));
        }
    }
    let [(x, y)] = pairs[..] else {
        return Err(format!("not a single exchanged pair: {pairs:?}"));
    };
    // Both raw handles name objects in both captured views.
    let seen = |v: &Value, id: u64| {
        let mut out = Vec::new();
        let mut s = v.clone();
        swap_handles(&mut s, id, u64::MAX);
        diff_paths(v, &s, "", &mut out);
        !out.is_empty()
    };
    let present = [
        seen(&a.raw_obs, x),
        seen(&a.raw_obs, y),
        seen(&b.raw_obs, x),
        seen(&b.raw_obs, y),
    ];
    // Exchange test on the captured raw views: B's view with X and Y
    // exchanged equals A's as multisets.
    let mut bs = b.raw_obs.clone();
    swap_handles(&mut bs, x, y);
    let mut raw_diff = Vec::new();
    diff_paths(
        &as_multisets(&a.raw_obs),
        &as_multisets(&bs),
        "",
        &mut raw_diff,
    );
    let raw_diff: Vec<String> = raw_diff
        .into_iter()
        .filter(|p| !p.starts_with("/step_index") && !p.starts_with("/physical_decision_id"))
        .collect();
    // The opponent decision in A's simulation with an exchanged counterpart.
    let mut found = None;
    for (i, (s, step, act)) in a.opp_since.iter().enumerate().rev() {
        let d = decision(s).ok_or("terminal snapshot")?;
        let (_, menu) = raw_decision(s, d).map_err(|e| format!("{e:?}"))?;
        let mut alt = menu[*act as usize].clone();
        swap_handles(&mut alt, x, y);
        if alt != menu[*act as usize] {
            if let Some(j) = menu.iter().position(|m| *m == alt) {
                found = Some((i, s.clone(), *step, *act, j as u32));
                break;
            }
        }
    }
    let base = json!({"x":x,"y":y,"canonical_diff_paths":paths,"handles_present_in_both_views":present,
        "raw_views_equal_after_exchange_as_multisets":raw_diff.is_empty(),"raw_exchange_diff_paths":raw_diff,
        "snapshots":a.opp_since.len()});
    let Some((i, s, step, a1, a2)) = found else {
        let mut v = base;
        v["probe"] = json!("no snapshot decision with an exchanged counterpart");
        return Ok(v);
    };
    let mut p1 = roles.focal.fork_for_collection_v3()?;
    let mut p2 = roles.focal.fork_for_collection_v3()?;
    let mut q1 = roles.opps[opp].fork_for_collection_v3()?;
    let mut q2 = roles.opps[opp].fork_for_collection_v3()?;
    let (mut s1, mut s2) = (s.clone(), s);
    let mut steps = Vec::new();
    let (mut n1, mut n2) = (a1, a2);
    let mut stop = "limit";
    // The exchanged opponent choice first.
    {
        let d = decision(&s1).ok_or("terminal")?;
        let (l1, l2) = (
            q1.score_fast_session_v1(&s1)?,
            q2.score_fast_session_v1(&s2)?,
        );
        steps.push(json!({"actor":"opponent","legal":d.legal_action_count,"exchanged_choice":true,
            "actions":[a1,a2],
            "logits_recorded_and_counterpart_same_clone":[l1.logits[a1 as usize],l1.logits[a2 as usize]],
            "corresponding_logit_gap":(l1.logits[a1 as usize] - l2.logits[a2 as usize]).abs(),
            "value_gap":(l1.value - l2.value).abs()}));
    }
    loop {
        if budget.pairs == 0 || budget.transitions < 2 {
            break;
        }
        let (d1, d2) = (
            decision(&s1).ok_or("terminal")?,
            decision(&s2).ok_or("terminal")?,
        );
        s1.step(d1.episode_id, d1.step, n1)
            .map_err(|e| format!("{e:?}"))?;
        s2.step(d2.episode_id, d2.step, n2)
            .map_err(|e| format!("{e:?}"))?;
        budget.pairs -= 1;
        budget.transitions -= 2;
        let (Some(d1), Some(d2)) = (decision(&s1), decision(&s2)) else {
            stop = if decision(&s1).is_none() && decision(&s2).is_none() {
                "both terminal"
            } else {
                "one terminal"
            };
            break;
        };
        let focal_turn = acting(&d1) == focal;
        let (o1, m1) = raw_decision(&s1, d1).map_err(|e| format!("{e:?}"))?;
        let (mut o2, m2) = raw_decision(&s2, d2).map_err(|e| format!("{e:?}"))?;
        swap_handles(&mut o2, x, y);
        let mut od = Vec::new();
        diff_paths(&as_multisets(&o1), &as_multisets(&o2), "", &mut od);
        // Corresponding actions: s2's candidate equal to s1's after exchange.
        let map: Vec<Option<usize>> = m1
            .iter()
            .map(|m| {
                let mut e = m.clone();
                swap_handles(&mut e, x, y);
                m2.iter().position(|n| *n == e)
            })
            .collect();
        let canon_equal = if focal_turn && d1.legal_action_count >= 2 {
            Some(
                canon(&s1, d1).map_err(|e| format!("{e:?}"))?
                    == canon(&s2, d2).map_err(|e| format!("{e:?}"))?,
            )
        } else {
            None
        };
        let (sc1, sc2) = if focal_turn {
            (
                p1.score_fast_session_v1(&s1)?,
                p2.score_fast_session_v1(&s2)?,
            )
        } else {
            (
                q1.score_fast_session_v1(&s1)?,
                q2.score_fast_session_v1(&s2)?,
            )
        };
        let logit_gap = map
            .iter()
            .enumerate()
            .filter_map(|(i, j)| j.map(|j| (sc1.logits[i] - sc2.logits[j]).abs()))
            .fold(0f32, f32::max);
        let best = (0..sc1.logits.len())
            .max_by(|&i, &j| sc1.logits[i].total_cmp(&sc1.logits[j]).then(j.cmp(&i)))
            .unwrap_or(0);
        steps.push(json!({"actor":if focal_turn { "focal" } else { "opponent" },"legal":d1.legal_action_count,
            "legal_counterpart":d2.legal_action_count,"raw_views_equal_after_exchange":od.is_empty(),
            "raw_diff_paths":od.iter().take(8).collect::<Vec<_>>(),"canonical_views_equal":canon_equal,
            "actions_with_counterpart":map.iter().filter(|j| j.is_some()).count(),
            "max_corresponding_logit_gap":logit_gap,"value_gap":(sc1.value - sc2.value).abs()}));
        if map.iter().any(Option::is_none) || d1.legal_action_count != d2.legal_action_count {
            stop = "unmatched choice";
            break;
        }
        n1 = best as u32;
        n2 = map[best].expect("checked") as u32;
    }
    let mut v = base;
    v["probe"] = json!({"snapshot":i,"opponent_step":step,"stop":stop,"steps":steps});
    Ok(v)
}
