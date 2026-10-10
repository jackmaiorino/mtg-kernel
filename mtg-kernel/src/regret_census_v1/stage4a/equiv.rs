//! Decision-equivalence census (`s4a-equiv`): which exposed decisions are
//! not real choices.
//!
//! Plays plain nine-deck games exactly like `s4a-corpus`. At every decision
//! with at least two options (either seat), each option is applied to a
//! clone of the session and settled: the clone continues with the first
//! option at every decision until the original chooser faces a decision of
//! a different kind (not another step of the same selection), or the game
//! ends. The chooser's canonical view there (the stage4a node-key bytes:
//! observation and menu) is the option's signature. Options with equal
//! signatures lead to the same situation for the chooser, so a menu with a
//! single signature class is not a decision for that player. A second,
//! masked signature also drops engine counters (`timestamp`,
//! `zone_change_count`, object handles), which separates choices whose only
//! visible effect is the ordering of those counters.
//!
//! Diagnostic only: the clones never touch the played game, which follows
//! the plain policy exactly as the corpus does. Limits: the signature is the
//! chooser's own view, so a difference hidden from the chooser (for
//! example, a card order the chooser cannot see) counts as equivalent.

use super::play::{decision, terminal};
use super::tree::canon;
use crate::card_def::CARD_DEFS;
use crate::rl_session::{FastActorDecisionV1, FastActorSessionV1};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

/// Transitions a settle may take before the option is recorded unsettled.
pub(crate) const SETTLE_CAP: u64 = 400;

fn kinds_and_sources(sem: &[crate::rl::ActionSemanticV1]) -> (BTreeSet<String>, BTreeSet<String>) {
    let mut kinds = BTreeSet::new();
    let mut sources = BTreeSet::new();
    for x in sem {
        let v = serde_json::to_value(x).unwrap_or(Value::Null);
        if let Some(k) = v["action_kind"].as_str() {
            kinds.insert(k.to_owned());
        }
        if let Some(id) = v["source"]["card_db_id"].as_u64() {
            if let Some(c) = CARD_DEFS.get(id as usize) {
                sources.insert(c.name.to_owned());
            }
        }
    }
    (kinds, sources)
}

/// Every option selects one more object for the same effect and the effect
/// must take every remaining option: only the order varies.
fn forced_complete(sem: &[crate::rl::ActionSemanticV1]) -> bool {
    let mut need = BTreeSet::new();
    for x in sem {
        match x {
            crate::rl::ActionSemanticV1::ChooseEffectTarget {
                source,
                selected_count,
                min_targets,
                ..
            } => {
                need.insert((
                    source.card_db_id,
                    u32::from(*min_targets).saturating_sub(u32::from(*selected_count)),
                ));
            }
            _ => return false,
        }
    }
    need.len() == 1 && need.iter().next().is_some_and(|&(_, n)| n as usize == sem.len())
}

fn masked_bytes(v: &mut Value) {
    match v {
        Value::Object(m) => {
            for k in ["arena_id", "zone_change_count", "timestamp", "attachments"] {
                m.remove(k);
            }
            m.values_mut().for_each(masked_bytes);
        }
        Value::Array(a) => {
            a.iter_mut().for_each(masked_bytes);
            if a.iter().all(Value::is_object) {
                a.sort_by_cached_key(|x| x.to_string());
            }
        }
        _ => {}
    }
}

fn hash(b: &[u8]) -> [u8; 32] {
    Sha256::digest(b).into()
}

/// Plays option `a` on a clone and returns (exact, masked) signatures and
/// the transitions taken; `None` signatures when the settle hit its cap.
fn settle(
    s: &FastActorSessionV1,
    d: &FastActorDecisionV1,
    a: u32,
    kinds: &BTreeSet<String>,
    sources: &BTreeSet<String>,
) -> Result<(Option<([u8; 32], [u8; 32])>, u64), String> {
    let mut w = s.clone();
    let chooser = d.acting_player;
    w.step(d.episode_id, d.step, a)
        .map_err(|e| format!("settle step: {e:?}"))?;
    let mut n = 1u64;
    loop {
        if let Some(end) = terminal(&w, super::play::acting(d)) {
            let b = format!("terminal {end:?}");
            return Ok((Some((hash(b.as_bytes()), hash(b.as_bytes()))), n));
        }
        let e = decision(&w).ok_or("settle: no decision")?;
        if e.acting_player == chooser && e.legal_action_count >= 2 {
            let sem = w.diagnostic_current_action_semantics().unwrap_or_default();
            let (k2, s2) = kinds_and_sources(&sem);
            let same_selection = k2 == *kinds && s2 == *sources && kinds.contains("choose_effect_target");
            if !same_selection {
                let c = canon(&w, e).map_err(|x| format!("settle canon: {x:?}"))?;
                let mut exact = Sha256::new();
                exact.update(&c.obs);
                for m in &c.edges {
                    exact.update((m.len() as u64).to_le_bytes());
                    exact.update(m);
                }
                let (obs, menu) = w
                    .actor_visible_decision_v4(e)
                    .map_err(|x| format!("settle view: {x:?}"))?;
                let mut v = json!({"obs": serde_json::to_value(&obs).unwrap_or(Value::Null),
                    "menu": serde_json::to_value(&menu).unwrap_or(Value::Null)});
                for k in ["step_index", "physical_decision_id", "visible_projection_hash", "kernel_version"] {
                    if let Value::Object(m) = &mut v["obs"] {
                        m.remove(k);
                    }
                }
                masked_bytes(&mut v);
                return Ok((Some((exact.finalize().into(), hash(v.to_string().as_bytes()))), n));
            }
        }
        if n >= SETTLE_CAP {
            return Ok((None, n));
        }
        w.step(e.episode_id, e.step, 0)
            .map_err(|x| format!("settle continue: {x:?}"))?;
        n += 1;
    }
}

/// One census record for the decision about to be taken at `s`.
pub(crate) fn record(s: &FastActorSessionV1, d: &FastActorDecisionV1, deck: &str) -> Result<Value, String> {
    let sem = s.diagnostic_current_action_semantics().ok_or("missing semantics")?;
    let (kinds, sources) = kinds_and_sources(&sem);
    let mut exact = BTreeMap::new();
    let mut masked = BTreeMap::new();
    let (mut unsettled, mut longest) = (0u64, 0u64);
    for a in 0..d.legal_action_count {
        let (sig, n) = settle(s, d, a, &kinds, &sources)?;
        longest = longest.max(n);
        match sig {
            Some((e, m)) => {
                *exact.entry(e).or_insert(0u32) += 1;
                *masked.entry(m).or_insert(0u32) += 1;
            }
            None => unsettled += 1,
        }
    }
    Ok(json!({"step":d.step,"pd":d.physical_decision_id,"seat":format!("{:?}",d.acting_player),
        "deck":deck,"k":d.legal_action_count,"kinds":kinds,"sources":sources,
        "forced_complete":forced_complete(&sem),"classes":exact.len(),"classes_masked":masked.len(),
        "unsettled":unsettled,"settle_max":longest}))
}
