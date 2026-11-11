//! Search probe choosers (mode `s4a-blunder-price`, `PRICE_MODE=search`;
//! collab LANES/searchprobe-20261111/DEFINITIONS.md). Measurement only:
//! nothing that plays, trains, samples or selects reads these rows.
//!
//! At each state (a recorded census decision, restored and record-checked as
//! the pricing mode does) this records the focal policy's logits, its most
//! probable action, its sampled action, its value head, and the root pick of
//! every chooser in `PROBE_CHOOSERS`: E's search (`select_e_options`) at a
//! transition budget, with or without the policy as a PUCT prior at the root
//! (`RootAllocation::Puct`, tempered by `temperature`) and with rollouts to
//! the game's end or the value leaf (`s4a-vl-v1`, horizon `h0` or `t0`) at
//! the leaf. Simulations draw their worlds with `WorldMode::CensusV1`
//! (`census_redeterminized_clone_v1` for the focal seat, then fresh future
//! randomness): the search never sees the true hidden state. The pick is the
//! root's `frozen_choice`; a search with no edge of eight backups falls back
//! to the most probable action (counted). Picks are scored separately by the
//! pricing mode's forced continuations, never by the search's own estimates.

use super::arms::Limits;
use super::bank::{fixed_work_roles, guide_ctx};
use super::blunder_census::softmax;
use super::blunder_price::restore;
use super::canon2::{KeyMode, KeyStats, Keyer};
use super::play::{acting, decision};
use super::replay_labels::LabelJob;
use super::root_allocation::{RootAllocation, RootPrior, SearchOptions};
use super::seeds::RootSeeds;
use super::tree::{live_index, Found, Tree};
use super::value_leaf::{Horizon, ValueLeafV1};
use super::world::{DeckPrior, WorldMode};
use crate::ids::PlayerId;
use crate::learned_bo3_v1::public_evaluation::Play;
use crate::paired_bo1_harness_v1::{
    paired_policy_seeds_v1, PairedBo1PolicyInputV1, PairedBo1PolicyV1,
};
use crate::rl_session::FastActorSessionV1;
use crate::sideboard_play_policy_v1::FrozenPlayPolicyV1;
use serde::Deserialize;
use serde_json::{json, Value};

/// The seed model of every chooser search (its worlds and policy streams);
/// the scoring continuations derive theirs from the pricing mode's salts.
pub(super) const SEED_MODEL: &str = "searchprobe-20261111";
/// Salt of the sampled-action reference's policy seed.
const SAMPLE_SALT: u64 = 0x5341_4d50;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct State {
    pub(super) id: String,
    pub(super) class: String,
    pub(super) index: usize,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(super) struct Chooser {
    pub(super) name: String,
    /// Selection-transition budget (`Limits::select_cap`).
    pub(super) budget: u64,
    /// Root prior temperature; None is E's UCB rule at the root.
    pub(super) temperature: Option<f64>,
    /// PUCT exploration constant (required with a temperature).
    pub(super) c_puct: Option<f64>,
    /// `rollout`, or a value-leaf horizon `h0` or `t0`.
    pub(super) leaf: String,
}

impl Chooser {
    pub(super) fn options(&self, logits: &[f32]) -> Result<SearchOptions, String> {
        let root_prior = match (self.temperature, self.c_puct) {
            (Some(t), Some(c)) if t > 0.0 && c > 0.0 => Some(RootPrior::tempered(logits, t, c)),
            (None, None) => None,
            _ => return Err(format!("chooser {}: temperature and c_puct go together", self.name)),
        };
        let value_leaf = match self.leaf.as_str() {
            "rollout" => None,
            h => Some(ValueLeafV1 {
                horizon: Horizon::parse(h)?,
            }),
        };
        Ok(SearchOptions {
            allocation: if root_prior.is_some() {
                RootAllocation::Puct
            } else {
                RootAllocation::Uct
            },
            root_prior,
            value_leaf,
            world: Some(WorldMode::CensusV1),
            ..SearchOptions::default()
        })
    }
}

/// The root's frozen choice as a live index, its per-edge (live, n, mean),
/// and whether the root node exists.
fn root_pick(root: &FastActorSessionV1, tree: &Tree) -> Result<(Option<u32>, Value), String> {
    let err = |e: super::play::PlayErr| format!("{e:?}");
    let d = decision(root).ok_or("root is terminal")?;
    let mut stats = KeyStats::default();
    let mut keyer = Keyer::new(KeyMode::Legacy, &mut stats);
    let st = keyer.step(root, d).map_err(err)?;
    let (key, pk, pe, cn) = keyer.storage(&st);
    Ok(match tree.find(&key, &pk, &pe, cn).map_err(err)? {
        Found::Hit(node) => {
            let pick = node
                .frozen_choice()
                .map(|e| live_index(cn, &cn.edges[e]))
                .transpose()
                .map_err(err)?;
            let mut edges = Vec::new();
            for e in 0..node.n.len() {
                let live = live_index(cn, &cn.edges[e]).map_err(err)?;
                let mean = (node.n[e] > 0).then(|| node.mean(e));
                edges.push(json!([live, node.n[e], mean]));
            }
            (pick, json!({"found": true, "visits": node.visits, "edges": edges}))
        }
        Found::Miss => (None, json!({"found": false})),
    })
}

/// A search that stopped early (a fault, or `incomplete`: world sampling that
/// kept failing, a floating-point or option refusal) is an error for the
/// whole state, never a row: its pick would rest on less work than the
/// budget. Ordinary budget exhaustion (`cap_reached`) and a root without an
/// edge of eight backups (the counted fallback) are not failures.
pub(super) fn check_selection(name: &str, sel: &super::arms::Selection) -> Result<(), String> {
    if sel.incomplete || !sel.faults.is_empty() {
        return Err(format!(
            "chooser {name}: search incomplete or faulted ({} faults: {:?})",
            sel.faults.len(),
            sel.faults.first()
        ));
    }
    Ok(())
}

/// One state's row.
pub(super) fn search_state(
    job: &LabelJob,
    state: &State,
    candidate: &mut Play,
    opponent: &FrozenPlayPolicyV1,
    choosers: &[Chooser],
) -> Result<Value, String> {
    let focal = PlayerId(job.focal);
    let s = restore(job, state.index)?;
    let d = decision(&s).ok_or("no decision")?;
    if acting(&d) != focal {
        return Err("the state is not the focal seat's decision".into());
    }
    let legal = d.legal_action_count as usize;
    candidate
        .reset_for_game_v1([0, 0])
        .map_err(|e| e.to_string())?;
    let policy = candidate
        .legacy_policy_mut()
        .ok_or("the candidate must be a Legacy source")?;
    let scores = policy.score_fast_session_v1(&s)?;
    let logits = scores.logits.clone();
    if logits.len() != legal {
        return Err("the policy's logits do not cover the live menu".into());
    }
    let probs = softmax(&logits);
    let argmax = (0..legal)
        .fold(0, |b, i| if logits[i] > logits[b] { i } else { b }) as u32;
    let focal_policy = policy.fork_for_search_v1()?;
    let sample_seed = super::blunder_price::mix(&state.id, state.index, 0, SAMPLE_SALT);
    candidate
        .reset_for_game_v1(paired_policy_seeds_v1(sample_seed))
        .map_err(|e| e.to_string())?;
    let sampled = candidate
        .select_action_v1(PairedBo1PolicyInputV1::new(&s, d))
        .map_err(|e| e.to_string())?;
    let prior = DeckPrior::new(&[]);
    let seeds = RootSeeds {
        model: SEED_MODEL.into(),
        root: format!("{}/{}", state.id, state.index),
    };
    let mut out = serde_json::Map::new();
    for chooser in choosers {
        let options = chooser.options(&logits)?;
        let limits = Limits {
            select_cap: chooser.budget,
            ..Limits::FORMAL
        };
        let ctx = guide_ctx(&s, focal, 0, &seeds, &prior, limits)?;
        let mut roles = fixed_work_roles(&focal_policy, std::slice::from_ref(opponent))?;
        let cpu0 = super::process_cpu::seconds()?;
        let started = std::time::Instant::now();
        let (sel, tree) = roles.select_e_options(&ctx, None, 1, None, None, None, &options);
        let wall = started.elapsed().as_secs_f64();
        let cpu = super::process_cpu::seconds()? - cpu0;
        check_selection(&chooser.name, &sel)?;
        let (pick, root) = root_pick(&s, &tree)?;
        if pick.is_some_and(|p| p as usize >= legal) {
            return Err(format!("chooser {}: pick outside the menu", chooser.name));
        }
        out.insert(
            chooser.name.clone(),
            json!({"pick": pick.unwrap_or(argmax), "fallback": pick.is_none(), "cpu": cpu,
                "wall": wall, "transitions": sel.transitions, "attempted": sel.attempted,
                "completed": sel.completed, "nonnatural": sel.nonnatural,
                "rejected": sel.attempted.saturating_sub(sel.completed + sel.nonnatural),
                "incomplete": sel.incomplete, "faults": sel.faults, "root": root,
                "extra": sel.extra.get("root_allocation").cloned().unwrap_or(Value::Null),
                "vl": sel.extra.get("value_leaf").map(|v| v["census"].clone()).unwrap_or(Value::Null)}),
        );
    }
    Ok(
        json!({"kind": "search", "id": state.id, "class": state.class, "index": state.index,
        "focal": job.focal, "legal": legal, "turn": s.game_state().turn, "probs": probs,
        "argmax": argmax, "sampled": sampled, "value_head": scores.value, "choosers": out}),
    )
}

/// Reads `PROBE_CHOOSERS` (a JSON list) and checks names are distinct.
pub(super) fn choosers() -> Result<Vec<Chooser>, String> {
    let path = std::env::var("PROBE_CHOOSERS").map_err(|_| "PROBE_CHOOSERS is required")?;
    let list: Vec<Chooser> = serde_json::from_slice(
        &std::fs::read(&path).map_err(|e| format!("{path}: {e}"))?,
    )
    .map_err(|e| format!("{path}: {e}"))?;
    let mut names: Vec<&str> = list.iter().map(|c| c.name.as_str()).collect();
    names.sort_unstable();
    names.dedup();
    if names.len() != list.len() || list.is_empty() {
        return Err("PROBE_CHOOSERS needs distinct names".into());
    }
    Ok(list)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chooser(t: Option<f64>, c: Option<f64>, leaf: &str) -> Chooser {
        Chooser {
            name: "x".into(),
            budget: 4096,
            temperature: t,
            c_puct: c,
            leaf: leaf.into(),
        }
    }

    #[test]
    fn options_select_allocation_prior_and_leaf() {
        let logits = [2.0f32, 0.0, -2.0];
        let o = chooser(None, None, "rollout").options(&logits).unwrap();
        assert_eq!(o.allocation, RootAllocation::Uct);
        assert!(o.root_prior.is_none() && o.value_leaf.is_none());
        assert_eq!(o.world, Some(WorldMode::CensusV1));
        let o = chooser(Some(4.0), Some(1.0), "h0").options(&logits).unwrap();
        assert_eq!(o.allocation, RootAllocation::Puct);
        let p = o.root_prior.unwrap();
        assert!((p.probs.iter().sum::<f64>() - 1.0).abs() < 1e-12);
        // Temperature 4 flattens: exp(0.5) and exp(-0.5) ratios to the middle.
        assert!((p.probs[0] / p.probs[1] - 0.5f64.exp()).abs() < 1e-12);
        assert_eq!(o.value_leaf.unwrap().horizon, Horizon::H0);
        assert!(chooser(Some(4.0), None, "rollout").options(&logits).is_err());
        assert!(chooser(None, None, "h9").options(&logits).is_err());
    }

    #[test]
    fn incomplete_or_faulted_searches_fail_the_state() {
        use super::super::arms::Selection;
        let done = Selection {
            cap_reached: true,
            completed: 40,
            ..Selection::default()
        };
        assert!(check_selection("x", &done).is_ok());
        let incomplete = Selection {
            incomplete: true,
            ..done.clone()
        };
        assert!(check_selection("x", &incomplete).is_err());
        let faulted = Selection {
            faults: vec!["boom".into()],
            ..done
        };
        assert!(check_selection("x", &faulted).is_err());
    }
}
