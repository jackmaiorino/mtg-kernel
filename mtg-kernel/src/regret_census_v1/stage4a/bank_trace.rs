//! Diagnostic reconstruction of one guide-bank source's S guide (mode
//! `s4a-bank-trace`, `BANK_TRACE_ID`): the same source game, anchor and
//! search as the bank row, then E's frozen execution on the actual world,
//! decision by decision, with the tree's view at each lookup and, at the
//! first history miss, how the actual observation differs from the tree's
//! sibling histories under the same parent edge. Read-only: it reproduces the
//! row's actions (checked by the caller against the bank) and changes nothing.

use super::arms::Roles;
use super::bank::{
    act_plain, err, guide_ctx, outcome, plain_logged, source_session, walk, BankShared, SourceSpec,
};
use super::labels::{classify, spy_defs};
use super::play::{acting, apply, decision, Counters, Meter};
use super::seeds::RootSeeds;
use super::tree::{canon, child_key, live_index, Found, Key, MAX_DEPTH};
use crate::ids::PlayerId;
use serde_json::{json, Value};

fn json_diff(a: &Value, b: &Value, path: &str, out: &mut Vec<Value>, limit: usize) {
    if out.len() >= limit || a == b {
        return;
    }
    match (a, b) {
        (Value::Object(x), Value::Object(y)) => {
            let mut keys: Vec<&String> = x.keys().chain(y.keys()).collect();
            keys.sort();
            keys.dedup();
            for k in keys {
                let va = x.get(k).unwrap_or(&Value::Null);
                let vb = y.get(k).unwrap_or(&Value::Null);
                json_diff(va, vb, &format!("{path}/{k}"), out, limit);
            }
        }
        (Value::Array(x), Value::Array(y)) if x.len() == y.len() => {
            for (i, (va, vb)) in x.iter().zip(y).enumerate() {
                json_diff(va, vb, &format!("{path}/{i}"), out, limit);
            }
        }
        _ => out.push(json!({"path":path,"actual":a,"tree":b})),
    }
}

fn canon_json(bytes: &[u8]) -> Value {
    serde_json::from_slice(bytes).unwrap_or(Value::Null)
}

pub(super) fn trace_source(
    sh: &BankShared,
    roles: &mut Roles,
    src: &SourceSpec,
) -> Result<Value, String> {
    let spec = &sh.spec;
    let focal = PlayerId(src.seat);
    let opp = sh
        .labels
        .iter()
        .position(|l| *l == src.opp)
        .ok_or("unknown guide opponent")?;
    // Source game and anchor, exactly as `source_row`.
    let mut s = source_session(spec, src)?;
    roles.focal.reset_sampling_v1(src.source_policy_seeds);
    roles.opps[opp].reset_sampling_v1(src.source_policy_seeds);
    let mut source_actions = Vec::new();
    let (f, o) = (&mut roles.focal, &mut roles.opps[opp]);
    plain_logged(&mut s, focal, f, o, &mut source_actions)?;
    let mut eligible: Vec<u64> = Vec::new();
    walk(source_session(spec, src)?, &source_actions, |_, d, _| {
        if acting(d) == focal && d.substep_index == 0 && d.legal_action_count >= 2 {
            eligible.push(d.step);
        }
        Ok(())
    })?;
    let index = ((u128::from(src.anchor_draw) * eligible.len() as u128) >> 64) as usize;
    let anchor = walk(
        source_session(spec, src)?,
        &source_actions[..eligible[index] as usize],
        |_, _, _| Ok(()),
    )?;
    let seeds = RootSeeds {
        model: spec.lineage.clone(),
        root: src.search_root.clone(),
    };
    let ctx = guide_ctx(&anchor, focal, opp, &seeds, &sh.prior, sh.limits)?;
    let (sel, tree) = roles.select_e(&ctx);
    // E's frozen execution on the actual world, as `Roles::frozen`.
    let mut s = anchor.clone();
    roles.focal.reset_sampling_v1(src.guide_policy_seeds);
    roles.opps[opp].reset_sampling_v1(src.guide_policy_seeds);
    let defs = spy_defs();
    let mut meter = Meter::new(u64::MAX);
    let mut c = Counters::default();
    let mut parent: (Key, Vec<u8>) = ([0u8; 32], b"root".to_vec());
    let (mut matching, mut depth, mut last_phys) = (true, 0u32, None);
    let (mut actions, mut decisions) = (Vec::new(), Vec::new());
    let mut first_miss = Value::Null;
    let mut after_miss = 0;
    while let Some(d) = decision(&s) {
        let a = if acting(&d) == focal && d.legal_action_count >= 2 {
            if last_phys != Some(d.physical_decision_id) {
                depth += 1;
                last_phys = Some(d.physical_decision_id);
            }
            let sem = s.diagnostic_current_action_semantics().unwrap_or_default();
            let mut rec = json!({"step":d.step,"physical_decision_id":d.physical_decision_id,
                "substep":d.substep_index,"depth":depth,"k":d.legal_action_count,
                "matching_before":matching,"turn":s.game_state().turn,
                "phase":format!("{:?}", s.game_state().step)});
            let a = if matching && depth <= MAX_DEPTH {
                let cn = canon(&s, d).map_err(err)?;
                let key = child_key(&parent.0, &parent.1, &cn);
                let live = match tree.find(&key, &parent.0, &parent.1, &cn).map_err(err)? {
                    Found::Hit(node) => {
                        rec["lookup"] = json!("hit");
                        rec["node"] = json!({"visits":node.visits,"n":node.n,"wins":node.wins});
                        match node.frozen_choice() {
                            Some(e) => {
                                rec["source"] = json!("tree edge");
                                live_index(&cn, &cn.edges[e]).map_err(err)?
                            }
                            None => {
                                rec["source"] =
                                    json!("plain: matched node, no edge with 8 backups");
                                act_plain(&mut roles.focal, &s, &d, &mut c)?
                            }
                        }
                    }
                    Found::Miss => {
                        matching = false;
                        rec["lookup"] = json!("miss");
                        rec["source"] = json!("plain: first history miss");
                        let actual = canon_json(&cn.obs);
                        let mut siblings = Vec::new();
                        for node in tree.nodes.values() {
                            if node.parent == parent.0 && node.edge == parent.1 {
                                let mut diff = Vec::new();
                                json_diff(&actual, &canon_json(&node.canon.obs), "", &mut diff, 12);
                                siblings
                                    .push(json!({"visits":node.visits,"n":node.n,"wins":node.wins,
                                    "same_menu":node.canon.edges == cn.edges,"obs_diff":diff}));
                            }
                        }
                        siblings
                            .sort_by_key(|v| std::cmp::Reverse(v["visits"].as_u64().unwrap_or(0)));
                        let parent_node = tree.nodes.get(&parent.0).map(
                            |n| json!({"visits":n.visits,"n":n.n,"wins":n.wins,"depth":n.depth}),
                        );
                        first_miss = json!({"step":d.step,"depth":depth,"parent":parent_node,
                            "parent_edge":String::from_utf8_lossy(&parent.1),
                            "sibling_histories":siblings.len(),
                            "siblings":siblings.into_iter().take(6).collect::<Vec<_>>()});
                        act_plain(&mut roles.focal, &s, &d, &mut c)?
                    }
                };
                let edge = cn.menu[live as usize].clone();
                // The chosen edge's position and support in this node (a
                // missing child below an edge with backups would be an
                // identity problem; below an untried edge it is not).
                let e = cn.edges.iter().position(|x| *x == edge);
                rec["edge_index"] = json!(e);
                if let (Some(e), Found::Hit(node)) =
                    (e, tree.find(&key, &parent.0, &parent.1, &cn).map_err(err)?)
                {
                    rec["edge_backups"] = json!(node.n[e]);
                }
                parent = (key, edge);
                live
            } else {
                rec["source"] = json!(if matching {
                    "plain: depth limit"
                } else {
                    "plain: after miss"
                });
                act_plain(&mut roles.focal, &s, &d, &mut c)?
            };
            let (_, own, dr) = sem.get(a as usize).map_or((false, false, false), |x| {
                classify(x, s.game_state(), focal, &defs)
            });
            rec["chosen"] = json!(sem.get(a as usize));
            rec["chosen_spy_self"] = json!(own);
            rec["chosen_dr_giant"] = json!(dr);
            rec["menu"] = json!(sem);
            if !matching {
                after_miss += 1;
            }
            if after_miss <= 12 {
                decisions.push(rec);
            }
            a
        } else if acting(&d) == focal {
            0
        } else {
            act_plain(&mut roles.opps[opp], &s, &d, &mut c)?
        };
        actions.push(a);
        apply(&mut s, &d, a, &mut meter, focal, None, &defs).map_err(err)?;
    }
    Ok(
        json!({"kind":"bank_trace","id":src.id,"anchor_step":eligible[index],
        "selection":sel.json(),"actions":actions,"end":outcome(&s, focal),
        "decisions":decisions,"first_miss":first_miss}),
    )
}
