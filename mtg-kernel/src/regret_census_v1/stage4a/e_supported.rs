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
    let mut prior_history = history.to_vec();
    prior_history.push(visible_frame(session, d, 0)?);
    let prior =
        DeckPrior::new(prior_indices).with_visible_history(&prior_history, super::play::acting(&d));
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
    let root_mapping = canon
        .edges
        .iter()
        .map(|e| tree::live_index(canon, e).map_err(|e| format!("{e:?}")))
        .collect::<Result<Vec<_>, _>>()?;
    let root_ties = keyer.perm(&step, seeds).map_err(|e| format!("{e:?}"))?;
    let mut packet = json!({"method":METHOD,"key_mode":"repaired-v2","retention":1,
        "fpu":1.5,"max_depth":32,"min_support":8,"requested_transitions":select_cap,
        "sampler":super::world::SAMPLER_VERSION,"prior":prior.ids(),"history":history,
        "raw_observation":raw_observation,"ordered_action_semantics":raw_menu,
        "root_key":super::hex(&key),"canonical_edges":canon.edges,"canonical_live_menu":canon.menu,
        "tree_sha256":tree.hash(),"cost":selection.json(),"status":"unsupported_root",
        "canonical_to_live":root_mapping,"tie_order":root_ties,
        "n":vec![0_u64;canon.edges.len()],"wins":vec![0_u64;canon.edges.len()]});
    let Found::Hit(node) = tree
        .find(&key, &parent, &edge, canon)
        .map_err(|e| format!("{e:?}"))?
    else {
        return Ok((None, packet));
    };
    let (permutation, choice) = bind_supported_choice(node, canon)?;
    packet["n"] = json!(node.n);
    packet["wins"] = json!(node.wins);
    packet["tie_order"] = json!(node.perm);
    packet["canonical_to_live"] = json!(permutation);
    let Some(index) = choice else {
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

fn bind_supported_choice(
    node: &tree::Node,
    canon: &tree::Canon,
) -> Result<(Vec<u32>, Option<usize>), String> {
    let width = canon.edges.len();
    let mut ties = node.perm.clone();
    ties.sort_unstable();
    if node.n.len() != width
        || node.wins.len() != width
        || node.canon.edges != canon.edges
        || ties != (0..width).collect::<Vec<_>>()
        || node.n.iter().zip(&node.wins).any(|(n, w)| w > n)
        || node.n.iter().try_fold(0_u64, |sum, n| sum.checked_add(*n)) != Some(node.visits)
    {
        return Err("E corrupt root statistics or action binding".into());
    }
    let permutation = canon
        .edges
        .iter()
        .map(|e| tree::live_index(canon, e).map_err(|e| format!("{e:?}")))
        .collect::<Result<Vec<_>, _>>()?;
    let mut sorted = permutation.clone();
    sorted.sort_unstable();
    if sorted != (0..width as u32).collect::<Vec<_>>() {
        return Err("E ambiguous live action binding".into());
    }
    Ok((permutation, node.frozen_choice()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn e_supported_binding_preserves_live_duplicates_support_and_seeded_ties() {
        let (canon, _) = tree::canon_values(
            json!({"step_index":7}),
            vec![
                json!({"action_kind":"z"}),
                json!({"action_kind":"a"}),
                json!({"action_kind":"a"}),
            ],
        )
        .unwrap();
        let mut node =
            tree::Node::with_perm([0; 32], b"root".to_vec(), canon.clone(), 1, vec![2, 0, 1]);
        node.n = vec![8, 8, 100];
        node.wins = vec![8, 8, 0];
        node.visits = 116;
        let (mapping, choice) = bind_supported_choice(&node, &canon).unwrap();
        assert_eq!(mapping, vec![1, 2, 0]);
        assert_eq!(choice, Some(0));
        // Execution compares Boolean means among supported edges, not visit share.
        node.perm = vec![1, 0, 2];
        assert_eq!(bind_supported_choice(&node, &canon).unwrap().1, Some(1));
        node.n = vec![7, 7, 7];
        node.wins = vec![7, 7, 0];
        node.visits = 21;
        assert_eq!(bind_supported_choice(&node, &canon).unwrap().1, None);
        node.wins[0] = 8;
        assert!(bind_supported_choice(&node, &canon).is_err());
        node.wins[0] = 7;
        node.perm = vec![0, 0, 1];
        assert!(bind_supported_choice(&node, &canon).is_err());
    }
    #[test]
    fn e_supported_keyer_preserves_the_supplied_visible_prefix() {
        let history = vec![VisibleFrame {
            observation: json!({"step_index":1}),
            menu: vec![json!({"action_kind":"a"}), json!({"action_kind":"b"})],
            selected: 1,
        }];
        let mut statistics = KeyStats::default();
        let mut keyer = Keyer::with_history(KeyMode::Repaired, &mut statistics, &history).unwrap();
        let root = keyer
            .key_raw(json!({"step_index":2}), vec![json!({"action_kind":"c"})])
            .unwrap();
        let with_prefix = keyer.storage(&root).0;
        let mut statistics = KeyStats::default();
        let mut keyer = Keyer::new(KeyMode::Repaired, &mut statistics);
        let root = keyer
            .key_raw(json!({"step_index":2}), vec![json!({"action_kind":"c"})])
            .unwrap();
        assert_ne!(with_prefix, keyer.storage(&root).0);
    }
}
