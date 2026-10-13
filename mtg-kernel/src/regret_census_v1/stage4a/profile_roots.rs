//! Search cost microchecks (collab LANES/search-efficiency-plan-20261012),
//! mode `s4a-search-profile`.
//!
//! For one ordinary-start source case, the live plain policies play the case
//! once (guide seeds, no search) to fix a development game. Its focal
//! decisions with two or more legal actions are the eligible roots; roots are
//! chosen by temporal position alone (fractions of that list), never by
//! outcome, and the game's outcome is not recorded. At each chosen root one
//! persistent search runs exactly as an `s4a-budget` refresh does (same
//! context, keys, selection rule and accounting) with the requested
//! selection-transition budget and its own seed root. Rows carry the
//! selection record, a primary hash over every deterministic search output,
//! wall time, the root's supported (teacher) choice beside the plain game's
//! own choice there, and, with the `s4a-profile-v1` feature, the exclusive
//! cost split.

use super::arms::Limits;
use super::bank::{
    bank_roles, guide_ctx, plain_logged, source_session, walk, BankShared, SourceSpec,
};
use super::canon2::{KeyMode, KeyStats, Keyer};
use super::play::{acting, decision};
use super::seeds::RootSeeds;
use super::tree::{live_index, Found, Tree, MIN_EXEC_BACKUPS};
use crate::ids::PlayerId;
use crate::rl_session::FastActorSessionV1;
use crate::sideboard_play_policy_v1::FrozenPlayPolicyV1;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::time::Instant;

/// One search per requested position. `positions` are fractions in [0, 1)
/// of the eligible focal decisions, in order.
pub(super) fn profile_case(
    sh: &BankShared,
    policy: &FrozenPlayPolicyV1,
    src: &SourceSpec,
    positions: &[(String, f64)],
    budget: u64,
    mode: KeyMode,
    binding: &str,
) -> Result<Vec<Value>, String> {
    let focal = PlayerId(src.seat);
    let opp = sh
        .labels
        .iter()
        .position(|l| *l == src.opp)
        .ok_or("unknown guide opponent")?;
    let mut live_focal = policy.fork_for_collection_v3()?;
    let mut live_opp = sh.opponents[opp].fork_for_collection_v3()?;
    live_focal.reset_sampling_v1(src.guide_policy_seeds);
    live_opp.reset_sampling_v1(src.guide_policy_seeds);
    let mut s = source_session(&sh.spec, src)?;
    let mut actions = Vec::new();
    plain_logged(&mut s, focal, &mut live_focal, &mut live_opp, &mut actions)?;
    let mut eligible: Vec<(u64, u32)> = Vec::new();
    walk(source_session(&sh.spec, src)?, &actions, |_, d, _| {
        if acting(d) == focal && d.legal_action_count >= 2 {
            eligible.push((d.step, d.legal_action_count));
        }
        Ok(())
    })?;
    if eligible.is_empty() {
        return Err("source has no eligible focal decision".into());
    }
    let mut rows = Vec::new();
    for (label, frac) in positions {
        let index = ((frac * eligible.len() as f64) as usize).min(eligible.len() - 1);
        let (step, width) = eligible[index];
        let root = walk(
            source_session(&sh.spec, src)?,
            &actions[..step as usize],
            |_, _, _| Ok(()),
        )?;
        let seeds = RootSeeds {
            model: sh.spec.lineage.clone(),
            root: format!("{}/search-profile/{label}", src.search_root),
        };
        let limits = Limits {
            select_cap: budget,
            ..sh.limits
        };
        let mut ctx = guide_ctx(&root, focal, opp, &seeds, &sh.prior, limits)?;
        ctx.key_mode = mode;
        let mut roles = bank_roles(policy, &sh.opponents)?;
        let mut diag = super::arms::SearchDiag::default();
        let _ = crate::search_profile_v1::take();
        let started = Instant::now();
        let (sel, tree) = roles.select_e_diagnosed(&ctx, None, 1, None, None, Some(&mut diag));
        let wall = started.elapsed().as_secs_f64();
        let profile = crate::search_profile_v1::take_json();
        let teacher = root_teacher(&root, mode, &tree, actions[step as usize])?;
        let mut keys = diag.keys.json();
        keys["keying_seconds"] = Value::Null;
        let primary = json!({"selection":sel.json(),"keys":keys});
        let primary_sha256: String = Sha256::digest(primary.to_string().as_bytes())
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        rows.push(
            json!({"kind":"search_profile","schema":"search-efficiency-v1/root",
            "id":src.id,"lineage":sh.spec.lineage,"own":src.own,"other":src.other,"seat":src.seat,
            "opp":src.opp,"position":label,"fraction":frac,"eligible_index":index,
            "eligible_count":eligible.len(),"root_step":step,"root_width":width,
            "game_actions":actions.len(),"budget":budget,"key_mode":mode.label(),
            "bank_binding_sha256":binding,"selection":sel.json(),
            "keying_seconds":diag.keys.seconds,"primary_sha256":primary_sha256,
            "wall":wall,"teacher":teacher,"key_memo":diag.key_memo,"profile":profile}),
        );
    }
    Ok(rows)
}

/// The root's frozen supported choice (as the teacher contract binds it) and
/// the plain game's own choice at that root, as live action indices.
fn root_teacher(
    root: &FastActorSessionV1,
    mode: KeyMode,
    tree: &Tree,
    played: u32,
) -> Result<Value, String> {
    let err = |e: super::play::PlayErr| format!("{e:?}");
    let d = decision(root).ok_or("root is terminal")?;
    let mut stats = KeyStats::default();
    let mut keyer = Keyer::new(mode, &mut stats);
    let st = keyer.step(root, d).map_err(err)?;
    let (key, pk, pe, cn) = keyer.storage(&st);
    Ok(match tree.find(&key, &pk, &pe, cn).map_err(err)? {
        Found::Hit(node) => {
            let choice = node.frozen_choice();
            let live = choice
                .map(|e| live_index(cn, &cn.edges[e]))
                .transpose()
                .map_err(err)?;
            json!({"root_found":true,"supported":choice.is_some(),"teacher_live":live,
                "played_live":played,"agree":live.map(|l| l == played),"root_visits":node.visits,
                "supported_edges":node.n.iter().filter(|n| **n >= MIN_EXEC_BACKUPS).count(),
                "edges":node.n.len()})
        }
        Found::Miss => json!({"root_found":false,"supported":false,"played_live":played}),
    })
}
