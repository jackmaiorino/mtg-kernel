//! Adapter from E's repaired storage edge to the exact live legal coordinate.
pub(crate) use super::seeds::RootSeeds;
use super::{
    arms::{Limits, Roles, RootCtx},
    canon2::{KeyMode, KeyStats, Keyer},
    tree::{self, Found},
    world::DeckPrior,
};
use crate::{
    rl_session::{FastActorDecisionV1, FastActorSessionV1},
    sideboard_play_policy_v1::FrozenPlayPolicyV1,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

pub(crate) const METHOD: &str = "e-supported-choice-ce-v1-20261012";
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct VisibleFrame {
    pub(crate) observation: Value,
    pub(crate) menu: Vec<Value>,
    pub(crate) selected: u32,
}
pub(crate) fn visible_frame(
    session: &FastActorSessionV1,
    d: FastActorDecisionV1,
    selected: u32,
) -> Result<VisibleFrame, String> {
    let (observation, menu) = tree::raw_decision(session, d).map_err(|e| format!("{e:?}"))?;
    if selected as usize >= menu.len() {
        return Err("E selected action outside visible menu".into());
    }
    Ok(VisibleFrame {
        observation,
        menu,
        selected,
    })
}
/// No policy RNG or real session mutation; all search roles are owned clones.
#[allow(clippy::too_many_arguments)]
pub(crate) fn query(
    session: &FastActorSessionV1,
    d: FastActorDecisionV1,
    history: &[VisibleFrame],
    teacher: &FrozenPlayPolicyV1,
    opponent: &FrozenPlayPolicyV1,
    seeds: &RootSeeds,
    prior_indices: &[usize],
    select_cap: u64,
) -> Result<(Option<usize>, Value), String> {
    let prior = DeckPrior::new(prior_indices);
    let ctx = RootCtx {
        visible_history: history,
        root: session,
        d,
        focal: super::play::acting(&d),
        opp: 0,
        seeds,
        prior: &prior,
        defs: super::labels::spy_defs(),
        cast_root: false,
        probs: Vec::new(),
        limits: Limits {
            select_cap,
            eval_worlds: 0,
            eval_cap: 0,
        },
        urgency: Some(1.5),
        key_mode: KeyMode::Repaired,
    };
    let mut roles = Roles {
        focal: teacher.clone(),
        opps: vec![opponent.clone()],
        scorer: teacher.clone(),
        inner_focal: teacher.clone(),
        inner_opps: vec![opponent.clone()],
    };
    let (selection, tree) = roles.select_e(&ctx);
    if !selection.faults.is_empty() {
        return Err(format!("E instrument fault: {:?}", selection.faults));
    }
    let mut stats = KeyStats::default();
    let mut keyer = Keyer::with_history(KeyMode::Repaired, &mut stats, history)
        .map_err(|e| format!("{e:?}"))?;
    let step = keyer.step(session, d).map_err(|e| format!("{e:?}"))?;
    let (key, parent, edge, canon) = keyer.storage(&step);
    let (raw_observation, raw_menu) =
        tree::raw_decision(session, d).map_err(|e| format!("{e:?}"))?;
    let mut packet = json!({"method":METHOD,"key_mode":"repaired-v2","retention":1,
        "fpu":1.5,"max_depth":32,"min_support":8,"requested_transitions":select_cap,
        "sampler":super::world::SAMPLER_VERSION,"prior":prior.ids(),"history":history,
        "raw_observation":raw_observation,"ordered_action_semantics":raw_menu,
        "root_key":super::hex(&key),"canonical_edges":canon.edges,"canonical_live_menu":canon.menu,
        "tree_sha256":tree.hash(),"cost":selection.json(),"status":"unsupported_root"});
    let Found::Hit(node) = tree
        .find(&key, &parent, &edge, canon)
        .map_err(|e| format!("{e:?}"))?
    else {
        return Ok((None, packet));
    };
    if node.n.len() != canon.edges.len()
        || node.wins.len() != node.n.len()
        || node.n.iter().zip(&node.wins).any(|(n, w)| w > n)
    {
        return Err("E corrupt root statistics".into());
    }
    packet["n"] = json!(node.n);
    packet["wins"] = json!(node.wins);
    packet["tie_order"] = json!(node.perm);
    let permutation = canon
        .edges
        .iter()
        .map(|e| tree::live_index(canon, e).map_err(|e| format!("{e:?}")))
        .collect::<Result<Vec<_>, _>>()?;
    packet["canonical_to_live"] = json!(permutation);
    let Some(index) = node.frozen_choice() else {
        return Ok((None, packet));
    };
    let live = permutation[index] as usize;
    let mut target = vec![0_u8; d.legal_action_count as usize];
    target[live] = 1;
    packet["target"] = json!(target);
    packet["selected_canonical"] = json!(index);
    packet["selected_live"] = json!(live);
    packet["status"] = json!("supported_choice");
    packet["actual_fallback"] = json!(false);
    Ok((Some(live), packet))
}
