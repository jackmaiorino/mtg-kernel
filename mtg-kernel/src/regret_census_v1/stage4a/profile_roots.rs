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
    match std::env::var("PROFILE_EXPERIMENT").as_deref() {
        Ok("1") => {
            return super::allocation_experiment::profile_case(
                sh, policy, src, positions, budget, mode, binding,
            )
        }
        Ok("0") | Err(_) => {}
        _ => return Err("PROFILE_EXPERIMENT must be 0 or 1".into()),
    }
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
        // Opt-in value-leaf search: N attempted simulations under a guard of
        // N x 512 transitions instead of the transition budget.
        let value_leaf = value_leaf_env()?;
        let limits = Limits {
            select_cap: value_leaf.map_or(budget, |(_, n)| n * VL_GUARD_PER_SIMULATION),
            ..sh.limits
        };
        let mut ctx = guide_ctx(&root, focal, opp, &seeds, &sh.prior, limits)?;
        ctx.key_mode = mode;
        let mut roles = bank_roles(policy, &sh.opponents)?;
        let mut diag = super::arms::SearchDiag::default();
        let _ = crate::search_profile_v1::take();
        let _ = crate::native_flat_tensorizer_v2::take_digest_memo_counts_v1();
        #[cfg(feature = "s4a-profile-v1")]
        let _ = crate::search_profile_v1::digest::take_json();
        let started = Instant::now();
        let mut records = Vec::new();
        let (sel, tree) = match value_leaf {
            None => {
                roles.select_e_diagnosed(&ctx, None, 1, Some(&mut records), None, Some(&mut diag))
            }
            Some((vl, n)) => roles.select_e_options(
                &ctx,
                None,
                1,
                Some(&mut records),
                None,
                Some(&mut diag),
                &super::root_allocation::SearchOptions {
                    work_limit: Some(n),
                    value_leaf: Some(vl),
                    ..super::root_allocation::SearchOptions::default()
                },
            ),
        };
        let wall = started.elapsed().as_secs_f64();
        let digest_memo = crate::native_flat_tensorizer_v2::take_digest_memo_counts_v1()
            .map(|(hits, misses)| json!({"hits":hits,"misses":misses}));
        #[allow(unused_mut, reason = "digest counters exist only with the feature")]
        let mut profile = crate::search_profile_v1::take_json();
        #[cfg(feature = "s4a-profile-v1")]
        {
            profile["digest"] = crate::search_profile_v1::digest::take_json();
        }
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
            "wall":wall,"vl_split":vl_split(&records),"teacher":teacher,"key_memo":diag.key_memo,"inference_cache":diag.inference_cache,"digest_memo":digest_memo,"profile":profile}),
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

/// The value-leaf guard: transitions allowed per attempted simulation.
const VL_GUARD_PER_SIMULATION: u64 = 512;

/// `S4A_VALUE_LEAF=h0|t0` with `S4A_VL_SIMS=N` selects the value-leaf search
/// (`value_leaf.rs`); both unset keeps the formal E search.
fn value_leaf_env() -> Result<Option<(super::value_leaf::ValueLeafV1, u64)>, String> {
    match (
        std::env::var("S4A_VALUE_LEAF"),
        std::env::var("S4A_VL_SIMS"),
    ) {
        (Err(_), Err(_)) => Ok(None),
        (Ok(h), Ok(n)) => {
            let horizon = super::value_leaf::Horizon::parse(&h)?;
            let n: u64 = n.parse().map_err(|_| "S4A_VL_SIMS must be a count")?;
            if n == 0 || n > 1 << 24 {
                return Err("S4A_VL_SIMS must be in 1..=16777216".into());
            }
            Ok(Some((super::value_leaf::ValueLeafV1 { horizon }, n)))
        }
        _ => Err("set both S4A_VALUE_LEAF and S4A_VL_SIMS, or neither".into()),
    }
}

/// Value-leaf cost split (W5 Stage 2a M4-lite; descriptive, never read by the
/// search): over accepted simulations, mean transitions and policy calls in
/// the tree prefix (to the expansion, or to the first choice past the depth
/// limit) and from the simulation's start to each value-leaf point (H = 0,
/// T0, T1), with the count of simulations that reached each point before a
/// natural, nonnatural or truncated ending.
fn vl_split(records: &[super::arms::SimRecord]) -> Value {
    use super::arms::{SimRecord, Snap};
    let accepted: Vec<&SimRecord> = records
        .iter()
        .filter(|r| r.outcome != SimRecord::REJECTED)
        .collect();
    let n = accepted.len().max(1) as f64;
    let mean =
        |f: &dyn Fn(&SimRecord) -> u64| accepted.iter().map(|r| f(*r)).sum::<u64>() as f64 / n;
    let prefix = |r: &SimRecord| r.expansion.or(r.depth_limit);
    let point = |get: fn(&SimRecord) -> Option<Snap>| {
        let reached: Vec<Snap> = accepted.iter().filter_map(|r| get(*r)).collect();
        let m = reached.len().max(1) as f64;
        json!({"reached":reached.len(),
            "mean_transitions":reached.iter().map(|x| x.transitions).sum::<u64>() as f64 / m,
            "mean_calls":reached.iter().map(|x| x.calls).sum::<u64>() as f64 / m})
    };
    json!({"schema":"w5-vl-split/v1","accepted":accepted.len(),
        "anchored":accepted.iter().filter(|r| prefix(**r).is_some()).count(),
        "mean_transitions":mean(&|r| r.transitions),"mean_calls":mean(&|r| r.policy_calls),
        "prefix":{"mean_transitions":mean(&|r| prefix(r).map_or(r.transitions, |x| x.transitions)),
            "mean_calls":mean(&|r| prefix(r).map_or(r.policy_calls, |x| x.calls))},
        "h0":point(|r| r.leaf_h0),"t0":point(|r| r.leaf_t0),"t1":point(|r| r.leaf_t1)})
}
