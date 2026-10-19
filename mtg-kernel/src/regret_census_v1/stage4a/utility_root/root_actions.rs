//! Forced moves at one authenticated historical new-root decision.
//!
//! `s4a-root-actions` requires games=1, first_game=0, workers=1, the ordinary
//! `BANK_SPEC`, `OPPONENTS`, source policy and `gameplay-decision-trace-v1`,
//! plus `ROOT_ACTIONS_SPEC` and `ROOT_ACTIONS_SPEC_SHA256` (SHA-256 of the
//! exact request file bytes). Request JSON (unknown fields fail):
//! ```text
//! {"schema":"historical-root-actions-v1/request", "id":"one-root",
//!  "bank_spec_sha256":"<exact BANK_SPEC bytes>",
//!  "recorded_build":{"git_head":"<40 hex>","tracked_tree_sha256":"<64 hex>"},
//!  "search":{"path":"search.json","sha256":"<exact single row file bytes>"},
//!  "case":<bank::SourceSpec>,
//!  "root":{"step":N,"physical_decision_id":P,"substep":S},
//!  "actions":[0,2,5],
//!  "repetitions":[{"focal":[11,13],"opponent":[17,19]}],
//!  "max_transitions":2048}
//! ```
//! The search row's saved executable actions before step N are replayed
//! without inference. The decision reached must have the requested
//! coordinates, be a focal multi-action choice whose saved action is in the
//! live menu, and carry either a `new_root` record matching that record's
//! search-root witness exactly, or a `reused` record matching its passive
//! reuse witness exactly (`validate_tree_root`). Actions are strictly
//! increasing live menu indices. 1..=8 repetitions name fresh focal and
//! opponent streams, distinct from each other and from the case's source and
//! guide seeds. For each repetition, then each action in order, a branch
//! forces the action, resets both named streams and plays on with the frozen
//! policies (`branch_retained`), so every action at a root shares a
//! repetition's streams. No search and no hidden-world sampling. The receipt
//! (one row at a fresh `out`) carries the root identity, the saved action
//! and its chooser, the root's ordered menu (action kind and actor-visible
//! label), the focal policy's root logit and value bits, its deployed
//! sampler's exact masses (decimal strings summing to 2**64), and every
//! branch, including a failed branch's partial receipt.

use super::super::canon2::{KeyMode, KeyStats, Keyer};
use super::*;
use std::collections::HashSet;

const REQUEST_SCHEMA: &str = "historical-root-actions-v1/request";
const RECEIPT_SCHEMA: &str = "historical-root-actions-v1/receipt";
const MAX_REPETITIONS: usize = 8;

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Coordinates {
    step: u64,
    physical_decision_id: u64,
    substep: u32,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Request {
    schema: String,
    id: String,
    bank_spec_sha256: String,
    recorded_build: RecordedBuild,
    search: RecordPin,
    case: SourceSpec,
    root: Coordinates,
    actions: Vec<u32>,
    repetitions: Vec<ContinuationSeeds>,
    max_transitions: u64,
}

fn validate_request(req: &Request, sh: &BankShared) -> Result<(), String> {
    sh.spec.validate()?;
    if req.schema != REQUEST_SCHEMA || req.id.is_empty() {
        return Err("root-actions request schema or id differs".into());
    }
    hex_pin(&req.bank_spec_sha256, 64)?;
    hex_pin(&req.recorded_build.git_head, 40)?;
    hex_pin(&req.recorded_build.tracked_tree_sha256, 64)?;
    hex_pin(&req.search.sha256, 64)?;
    validate_case(&req.case, sh)?;
    validate_cap(req.max_transitions)?;
    validate_actions(&req.actions)?;
    validate_repetitions(&req.repetitions, &req.case)
}

fn validate_actions(actions: &[u32]) -> Result<(), String> {
    if actions.is_empty() || actions.windows(2).any(|w| w[0] >= w[1]) {
        return Err("root actions must be nonempty and strictly increasing".into());
    }
    Ok(())
}

/// Fresh continuation streams: no repetition seed repeats another or any
/// seed the case's own source or guide streams use.
fn validate_repetitions(repetitions: &[ContinuationSeeds], src: &SourceSpec) -> Result<(), String> {
    if repetitions.is_empty() || repetitions.len() > MAX_REPETITIONS {
        return Err(format!(
            "root actions need 1..={MAX_REPETITIONS} repetitions"
        ));
    }
    let case: HashSet<u64> = src
        .source_policy_seeds
        .into_iter()
        .chain(src.guide_policy_seeds)
        .collect();
    let mut fresh = HashSet::new();
    for seed in repetitions
        .iter()
        .flat_map(|r| r.focal.into_iter().chain(r.opponent))
    {
        if case.contains(&seed) || !fresh.insert(seed) {
            return Err("root-actions continuation seeds overlap".into());
        }
    }
    Ok(())
}

struct AuthenticatedStep {
    session: FastActorSessionV1,
    identity: Value,
    prefix_len: usize,
    prefix_sha256: String,
    saved_action: u32,
    chooser: Value,
}

/// Replay the search row's saved prefix without inference and bind the
/// decision reached to its recorded new-root witness. Ends before any
/// policy fork, stream reset or forced action.
fn authenticate(
    src: &SourceSpec,
    at: &Coordinates,
    spec: &bank::BankSpec,
    search: &Value,
    historical_binding: &str,
    prefix_transitions: &mut u64,
) -> Result<AuthenticatedStep, String> {
    let actions = row_actions(search, src, &spec.lineage, historical_binding, false)?;
    let index = usize::try_from(at.step).map_err(|_| "root step overflow")?;
    let saved_action = *actions
        .get(index)
        .ok_or("root step is beyond the saved actions")?;
    // A reused root's cursor keyed every focal multi-action decision from its
    // originating search on: rebuild that Keyer history along the replay.
    let reused_origin = reused_origin(search, at.step)?;
    let mut keyer = reused_origin.map(|_| Keyer::new(KeyMode::Repaired, KeyStats::default()));
    let mut root = bank::source_session(spec, src)?;
    for (step, &action) in actions[..index].iter().enumerate() {
        let d = decision(&root).ok_or("replay reached a terminal early")?;
        if d.step != step as u64 || action >= d.legal_action_count {
            return Err("saved-prefix step or executable action differs".into());
        }
        if let (Some((origin, search_index)), Some(keyer)) = (reused_origin, keyer.as_mut()) {
            if d.step >= origin && acting(&d) == PlayerId(src.seat) && d.legal_action_count >= 2 {
                if recorded_decision(search, &d)?["search"].as_u64() != Some(search_index) {
                    return Err("reused root history switched searches".into());
                }
                let keyed = keyer.step(&root, d).map_err(bank::err)?;
                keyer.advance(keyed, action);
            }
        }
        root.step(d.episode_id, d.step, action)
            .map_err(|e| format!("{e:?}"))?;
        *prefix_transitions += 1;
    }
    let d = decision(&root).ok_or("root step is terminal")?;
    if d.step != at.step
        || d.physical_decision_id != at.physical_decision_id
        || d.substep_index != at.substep
        || acting(&d) != PlayerId(src.seat)
        || d.legal_action_count < 2
        || saved_action >= d.legal_action_count
    {
        return Err("root is not the requested focal multi-action decision".into());
    }
    let recorded = recorded_decision(search, &d)?;
    let identity = bank::decision_identity(&root)?;
    validate_tree_root(search, recorded, &identity, saved_action)?;
    if let Some(keyer) = keyer.as_mut() {
        let keyed = keyer.step(&root, d).map_err(bank::err)?;
        let (key, _, _, canon) = keyer.storage(&keyed);
        bind_reused_edge(recorded, &key, canon, saved_action)?;
    }
    let chooser = recorded["chooser"].clone();
    Ok(AuthenticatedStep {
        session: root,
        identity,
        prefix_len: index,
        prefix_sha256: sha(&actions[..index]
            .iter()
            .flat_map(|a| a.to_le_bytes())
            .collect::<Vec<_>>()),
        saved_action,
        chooser,
    })
}

/// The originating search's step and index of a reused root (None otherwise).
fn reused_origin(search: &Value, step: u64) -> Result<Option<(u64, u64)>, String> {
    let records = search["game"]["decisions"]
        .as_array()
        .ok_or("utility row has no decision records")?;
    let Some(record) = records.iter().find(|r| r["step"] == step) else {
        return Ok(None);
    };
    if record["chooser"] != "reused" {
        return Ok(None);
    }
    let index = record["search"]
        .as_u64()
        .ok_or("reused root search index missing")?;
    let origin = search["game"]["searches"]
        .as_array()
        .and_then(|s| s.get(usize::try_from(index).ok()?))
        .and_then(|s| s["step"].as_u64())
        .ok_or("reused root search record missing")?;
    Ok(Some((origin, index)))
}

/// The witness's node and edge are the rebuilt cursor's: node key, storage
/// edge bytes, the saved action's live mapping, and (when recorded) the node
/// statistics' whole storage-to-live permutation.
fn bind_reused_edge(
    recorded: &Value,
    key: &super::super::tree::Key,
    canon: &super::super::tree::Canon,
    saved_action: u32,
) -> Result<(), String> {
    let witness = &recorded["reuse_witness"];
    let edge = witness["storage_edge_index"]
        .as_u64()
        .and_then(|e| usize::try_from(e).ok())
        .ok_or("reused witness edge missing")?;
    let bytes = canon
        .edges
        .get(edge)
        .ok_or("reused witness edge outside the rebuilt menu")?;
    let live = |bytes: &Vec<u8>| super::super::tree::live_index(canon, bytes).map_err(bank::err);
    if witness["node_key"] != super::super::pathlog::key_hex(key)
        || witness["storage_edge_sha256"] != sha(bytes)
        || canon.menu.get(saved_action as usize) != Some(bytes)
        || live(bytes)? != saved_action
    {
        return Err(
            "reused witness node, edge or live mapping differs from the rebuilt cursor".into(),
        );
    }
    let stats = &recorded["node_stats"];
    if !stats.is_null() {
        let map = canon
            .edges
            .iter()
            .map(live)
            .collect::<Result<Vec<u32>, _>>()?;
        if stats["storage_to_live"] != json!(map) {
            return Err("reused node statistics map differs from the rebuilt cursor".into());
        }
    }
    Ok(())
}

/// Every branch run is retained, including a failed branch's partial receipt.
fn diagnose(
    req: &Request,
    sh: &BankShared,
    policy: &FrozenPlayPolicyV1,
    opponent_index: usize,
    search: &Value,
    historical_binding: &str,
) -> Value {
    let mut out = json!({"authenticated":false,"complete":false});
    let mut prefix_transitions = 0u64;
    let mut repetitions: Vec<Value> = Vec::new();
    let (mut transitions, mut inference_calls) = (0u64, 0u64);
    let failure = (|| -> Result<(), String> {
        let root = authenticate(
            &req.case,
            &req.root,
            &sh.spec,
            search,
            historical_binding,
            &mut prefix_transitions,
        )?;
        let d = decision(&root.session).ok_or("authenticated root is terminal")?;
        if req.actions.iter().any(|&a| a >= d.legal_action_count) {
            return Err("a requested action is outside the live menu".into());
        }
        let (observation, menu) = root
            .session
            .actor_visible_decision_v4(d)
            .map_err(|_| "root actor-visible V4 projection unavailable")?;
        if menu.len() != d.legal_action_count as usize {
            return Err("root executable menu width differs".into());
        }
        out["authenticated"] = json!(true);
        out["root"] = root.identity.clone();
        out["root_core_environment_hash"] = json!(format!(
            "{:016x}",
            root.session.privileged_core_environment_hash()
        ));
        out["root_policy_steps"] = json!(root.session.policy_step_count());
        out["root_physical_decisions"] = json!(root.session.physical_decision_count());
        out["shared_prefix_transitions"] = json!(root.prefix_len);
        out["shared_prefix_actions_sha256"] = json!(root.prefix_sha256);
        out["saved_action"] = json!(root.saved_action);
        out["saved_chooser"] = root.chooser.clone();
        out["ordered_menu"] = Value::Array(super::inspect::menu(&observation, &menu)?);
        // The focal policy's own scores at the root, from a separate fork so no
        // branch stream is touched. Consumers compare these bits with the bank
        // that recorded the root: equal bits show this build encodes and scores
        // the root as the bank's build did.
        let mut scorer = policy.fork_for_collection_v3()?;
        let scores = scorer.score_fast_session_v1(&root.session)?;
        if scores.logits.len() != d.legal_action_count as usize {
            return Err("root scores width differs from the live menu".into());
        }
        out["root_policy_logit_bits"] = json!(scores
            .logits
            .iter()
            .map(|x| x.to_bits())
            .collect::<Vec<u32>>());
        out["root_policy_value_bits"] = json!(scores.value.to_bits());
        out["root_deployed_masses"] = json!(scorer
            .deployed_masses_v1(&scores.logits)?
            .iter()
            .map(|m| m.to_string())
            .collect::<Vec<_>>());
        let mut focal = policy.fork_for_collection_v3()?;
        let mut opponent = sh.opponents[opponent_index].fork_for_collection_v3()?;
        for (repetition, seeds) in req.repetitions.iter().enumerate() {
            let mut branches = Vec::new();
            let mut failed = None;
            for &action in &req.actions {
                let (branch, error) = branch_retained(
                    &root.session,
                    action,
                    PlayerId(req.case.seat),
                    &mut focal,
                    &mut opponent,
                    seeds,
                    req.max_transitions,
                )?;
                transitions += branch.transitions;
                inference_calls += branch.inference_calls;
                branches.push(serde_json::to_value(&branch).map_err(|e| e.to_string())?);
                if error.is_some() {
                    failed = error;
                    break;
                }
            }
            repetitions.push(json!({"repetition":repetition,"branches":branches}));
            if let Some(error) = failed {
                return Err(error);
            }
        }
        Ok(())
    })()
    .err();
    out["prefix_transitions"] = json!(prefix_transitions);
    out["repetitions"] = json!(repetitions);
    out["branch_transitions"] = json!(transitions);
    out["branch_inference_calls"] = json!(inference_calls);
    out["total_transitions"] = json!(prefix_transitions + transitions);
    out["complete"] = json!(failure.is_none());
    out["failure"] = json!(failure);
    out
}

pub(super) fn run(
    cfg: &CensusConfigV1,
    policy: &FrozenPlayPolicyV1,
    sh: &BankShared,
    bank_spec_bytes: &[u8],
) -> Result<(), String> {
    if cfg.games != 1 || cfg.first_game != 0 || cfg.workers != 1 {
        return Err("s4a-root-actions requires games=1, first_game=0, workers=1".into());
    }
    let path = std::env::var("ROOT_ACTIONS_SPEC").map_err(|_| "ROOT_ACTIONS_SPEC is required")?;
    let pin = std::env::var("ROOT_ACTIONS_SPEC_SHA256")
        .map_err(|_| "ROOT_ACTIONS_SPEC_SHA256 is required")?;
    let bytes = std::fs::read(&path).map_err(|e| format!("{path}: {e}"))?;
    let req: Request = serde_json::from_slice(pinned_bytes(&bytes, &pin)?)
        .map_err(|e| format!("root-actions request: {e}"))?;
    validate_request(&req, sh)?;
    pinned_bytes(bank_spec_bytes, &req.bank_spec_sha256)?;
    let historical_binding = bank::bank_binding_with_build(
        policy,
        sh,
        &req.recorded_build.git_head,
        &req.recorded_build.tracked_tree_sha256,
    )?;
    let current_binding = bank::bank_binding(policy, sh)?;
    let search = read_record(&req.search)?;
    let opponent_index = sh
        .labels
        .iter()
        .position(|l| *l == req.case.opp)
        .ok_or("unknown opponent")?;
    let diagnostic = std::thread::scope(|scope| {
        std::thread::Builder::new()
            .stack_size(256 << 20)
            .spawn_scoped(scope, || {
                diagnose(
                    &req,
                    sh,
                    policy,
                    opponent_index,
                    &search,
                    &historical_binding,
                )
            })
            .map_err(|e| e.to_string())?
            .join()
            .map_err(|_| "root-actions worker panicked".to_owned())
    })?;
    let complete = diagnostic["complete"] == true;
    let row = json!({"kind":"root_actions","schema":RECEIPT_SCHEMA,
        "request_sha256":pin,"request":req,
        "historical_bank_binding_sha256":historical_binding,
        "current_bank_binding_sha256":current_binding,
        "runtime":crate::gameplay_trace_v1::runtime_identity()?,
        "carddb_hash":format!("{:016x}",crate::card_def::KERNEL_CARDDB_HASH),
        "inherited_episode_limits":{"max_physical_decisions":sh.spec.max_physical_decisions,
            "max_policy_steps":sh.spec.max_policy_steps},
        "git_head":env!("MTG_KERNEL_BUILD_GIT_HEAD"),
        "tracked_tree_sha256":env!("MTG_KERNEL_BUILD_TRACKED_TREE_SHA256"),
        "continuation_convention":"fresh-role-seat-streams-after-forced-root-v1",
        "actions_hash_convention":"sha256-concat-step-u64le-seat-u8-action-u32le",
        "diagnostic":diagnostic});
    let mut output = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&cfg.out)
        .map_err(|e| format!("fresh root-actions output {}: {e}", cfg.out))?;
    writeln!(output, "{row}").map_err(|e| e.to_string())?;
    output.sync_all().map_err(|e| e.to_string())?;
    if !complete {
        return Err("root actions incomplete; the partial receipt is retained".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime_decks::RUNTIME_DECKS;

    fn replay_stack(test: impl FnOnce() + Send + 'static) {
        std::thread::Builder::new()
            .stack_size(256 << 20)
            .spawn(test)
            .unwrap()
            .join()
            .unwrap();
    }

    fn spec(max_physical_decisions: u64, max_policy_steps: u64) -> bank::BankSpec {
        bank::BankSpec {
            schema: bank::SPEC_SCHEMA.into(),
            namespace: "spy-reset-pilot-v1".into(),
            lineage: "fixture".into(),
            runtime_rules: "resolution-boundary-v1".into(),
            select_rule: "fpu-1.5".into(),
            max_physical_decisions,
            max_policy_steps,
            decks: RUNTIME_DECKS
                .iter()
                .map(|d| bank::DeckSpec {
                    label: d.id.into(),
                    mainboard: d.card_ids.to_vec(),
                })
                .collect(),
            sources: vec![],
        }
    }

    fn case(seat: u8) -> SourceSpec {
        SourceSpec {
            id: format!("case-{seat}"),
            own: 0,
            other: 1,
            seat,
            opp: "fixture".into(),
            seed: 101,
            starting_player: seat,
            source_policy_seeds: [2, 3],
            anchor_draw: 5,
            guide_policy_seeds: [7, 11],
            search_root: "search-root".into(),
        }
    }

    fn seeds(base: u64) -> ContinuationSeeds {
        ContinuationSeeds {
            focal: [base, base + 1],
            opponent: [base + 2, base + 3],
        }
    }

    /// Walks a real opening with action 0 to the focal seat's `nth` (from 0)
    /// multi-action decision and returns a search row whose record there is a
    /// new root with the exact live witness, plus the root's coordinates.
    fn later_root(src: &SourceSpec, spec: &bank::BankSpec, nth: usize) -> (Value, Coordinates) {
        let mut session = bank::source_session(spec, src).unwrap();
        let mut prefix = Vec::new();
        let mut seen = 0;
        let d = loop {
            let d = decision(&session).unwrap();
            if acting(&d) == PlayerId(src.seat) && d.legal_action_count >= 2 {
                if seen == nth {
                    break d;
                }
                seen += 1;
            }
            prefix.push(0u32);
            session.step(d.episode_id, d.step, 0).unwrap();
        };
        let identity = bank::decision_identity(&session).unwrap();
        let mut actions = prefix;
        actions.push(1);
        let row = json!({"kind":"budget_game","schema":"search-total-budget-v1/game",
            "lineage":spec.lineage,"bank_binding_sha256":"binding","id":src.id,
            "own":src.own,"other":src.other,"seat":src.seat,"opp":src.opp,
            "seed":src.seed,"starting_player":src.starting_player,
            "live_policy_seeds":src.guide_policy_seeds,"search_root":src.search_root,
            "game":{"actions":actions,"decisions":[{"step":d.step,
                "physical_decision_id":d.physical_decision_id,"substep":d.substep_index,
                "k":d.legal_action_count,"chooser":"new_root","search":0}],
                "searches":[{"search":0,"step":d.step,"root":identity}]}});
        let at = Coordinates {
            step: d.step,
            physical_decision_id: d.physical_decision_id,
            substep: d.substep_index,
        };
        (row, at)
    }

    fn request(actions: Vec<u32>, repetitions: Vec<ContinuationSeeds>) -> Request {
        Request {
            schema: REQUEST_SCHEMA.into(),
            id: "root".into(),
            bank_spec_sha256: "a".repeat(64),
            recorded_build: RecordedBuild {
                git_head: "b".repeat(40),
                tracked_tree_sha256: "c".repeat(64),
            },
            search: RecordPin {
                path: "search.json".into(),
                sha256: "d".repeat(64),
            },
            case: case(0),
            root: Coordinates {
                step: 3,
                physical_decision_id: 2,
                substep: 0,
            },
            actions,
            repetitions,
            max_transitions: 2048,
        }
    }

    #[test]
    fn requests_refuse_unknown_fields_and_bad_action_lists() {
        let text = serde_json::to_string(&request(vec![0, 2], vec![seeds(100)])).unwrap();
        assert!(serde_json::from_str::<Request>(&text).is_ok());
        let mut extra: Value = serde_json::from_str(&text).unwrap();
        extra["mode"] = json!("search-vs-plain");
        assert!(serde_json::from_value::<Request>(extra).is_err());
        assert!(validate_actions(&[0, 2, 5]).is_ok());
        assert!(validate_actions(&[7]).is_ok());
        for bad in [vec![], vec![2, 0], vec![1, 1], vec![0, 3, 3]] {
            assert!(validate_actions(&bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn repetitions_are_bounded_fresh_and_distinct() {
        let src = case(0);
        let eight: Vec<_> = (0..8).map(|j| seeds(100 + 4 * j)).collect();
        assert!(validate_repetitions(&eight, &src).is_ok());
        assert!(validate_repetitions(&[], &src).is_err());
        let nine: Vec<_> = (0..9).map(|j| seeds(100 + 4 * j)).collect();
        assert!(validate_repetitions(&nine, &src).is_err());
        // Shared across two repetitions, or equal to a case's own stream seed.
        assert!(validate_repetitions(&[seeds(100), seeds(102)], &src).is_err());
        for used in [2, 3, 7, 11] {
            let mut overlap = seeds(100);
            overlap.opponent[1] = used;
            assert!(validate_repetitions(&[overlap], &src).is_err(), "{used}");
        }
        // A case whose own source and guide seeds coincide is not refused for it.
        let mut same = case(0);
        same.guide_policy_seeds = same.source_policy_seeds;
        assert!(validate_repetitions(&[seeds(100)], &same).is_ok());
    }

    #[test]
    fn later_new_roots_authenticate_for_both_seats() {
        replay_stack(|| {
            let spec = spec(128, 256);
            for seat in [0, 1] {
                let src = case(seat);
                for nth in [0, 2] {
                    let (row, at) = later_root(&src, &spec, nth);
                    let mut transitions = 0;
                    let root =
                        authenticate(&src, &at, &spec, &row, "binding", &mut transitions).unwrap();
                    assert_eq!(transitions, at.step);
                    assert_eq!(root.prefix_len as u64, at.step);
                    assert_eq!(root.saved_action, 1);
                    assert_eq!(root.identity, row["game"]["searches"][0]["root"]);
                    assert_eq!(decision(&root.session).unwrap().step, at.step);
                }
            }
        });
    }

    #[test]
    fn authentication_refuses_reuse_witness_coordinate_and_action_mismatches() {
        replay_stack(|| {
            let spec = spec(128, 256);
            let src = case(0);
            let (row, at) = later_root(&src, &spec, 2);
            let refused = |row: &Value, at: &Coordinates| {
                authenticate(&src, at, &spec, row, "binding", &mut 0).is_err()
            };
            assert!(!refused(&row, &at));
            let mut reused = row.clone();
            reused["game"]["decisions"][0]["chooser"] = json!("reused");
            assert!(refused(&reused, &at));
            let mut witness = row.clone();
            witness["game"]["searches"][0]["root"]["state_hash"] = json!("0000000000000000");
            assert!(refused(&witness, &at));
            let mut binding = row.clone();
            binding["bank_binding_sha256"] = json!("other");
            assert!(refused(&binding, &at));
            let mut beyond = row.clone();
            beyond["game"]["actions"].as_array_mut().unwrap().pop();
            assert!(refused(&beyond, &at));
            let mut outside = row.clone();
            let k = row["game"]["decisions"][0]["k"].clone();
            *outside["game"]["actions"]
                .as_array_mut()
                .unwrap()
                .last_mut()
                .unwrap() = k;
            assert!(refused(&outside, &at));
            for wrong in [
                Coordinates {
                    physical_decision_id: at.physical_decision_id + 1,
                    ..at_copy(&at)
                },
                Coordinates {
                    substep: at.substep + 1,
                    ..at_copy(&at)
                },
                Coordinates {
                    step: at.step - 1,
                    ..at_copy(&at)
                },
            ] {
                assert!(refused(&row, &wrong));
            }
        });
    }

    /// TP1: a reused record whose witness lacks the rebuilt cursor history is
    /// refused (a relabeled new root has no Keyer history), as is one with no
    /// witness or a missing search index.
    #[test]
    fn reused_roots_without_a_true_cursor_history_are_refused() {
        replay_stack(|| {
            let spec = spec(128, 256);
            for seat in [0, 1] {
                let src = case(seat);
                let (row, at) = later_root(&src, &spec, 2);
                let identity = row["game"]["searches"][0]["root"].clone();
                let saved = row["game"]["actions"][at.step as usize].clone();
                let mut reused = row.clone();
                reused["game"]["searches"][0]["step"] = json!(0);
                reused["game"]["searches"][0]["tree"] = json!({"hash":"ab".repeat(32)});
                let record = &mut reused["game"]["decisions"][0];
                record["chooser"] = json!("reused");
                let refused =
                    |row: &Value| authenticate(&src, &at, &spec, row, "binding", &mut 0).is_err();
                assert!(refused(&reused), "no witness");
                reused["game"]["decisions"][0]["reuse_witness"] = json!({
                    "schema":crate::regret_census_v1::stage4a::reuse_witness::SCHEMA,
                    "decision":identity,"search":0,"search_tree_hash":"ab".repeat(32),
                    "storage_edge_index":1,"applied_live_index":saved,
                    "natural_terminal_backups":12,"focal_wins":7,
                    "node_key":"00".repeat(32),"storage_edge_sha256":"00".repeat(32)});
                assert!(refused(&reused), "forged witness");
                let mut absent = reused.clone();
                absent["game"]["decisions"][0]
                    .as_object_mut()
                    .unwrap()
                    .remove("search");
                assert!(refused(&absent), "missing search index");
            }
        });
    }

    /// TP1: producer-generated reused rows (fixture games played with node
    /// statistics and reuse witnesses, both seats and starting players)
    /// authenticate through the rebuilt cursor; every forged binding is
    /// refused: node key, edge hash, out-of-range edge (with node statistics
    /// removed), permuted node map, tree hash, applied action, counts, search
    /// index, schema and an identity field.
    #[test]
    #[ignore = "native qualification: complete fixture games with small searches; run with cargo test --release"]
    fn producer_reused_rows_bind_to_the_rebuilt_cursor() {
        use crate::regret_census_v1::stage4a::refresh::{self, tests::plain_control_fixture};
        replay_stack(|| {
            let (mut authenticated, mut permuted) = (0, 0);
            for (seat, starting) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                let (policy, sh, mut src) = plain_control_fixture(seat, starting, 100_000, 200_000);
                src.id = format!("tp1-reused-s{seat}-p{starting}");
                src.search_root = "tp1-reused".into();
                let row = refresh::budget_case_with_executor(
                    &sh,
                    &policy,
                    &src,
                    40_000,
                    KeyMode::Repaired,
                    false,
                    None,
                    crate::regret_census_v1::stage4a::reuse_witness::Recording::Enabled,
                    "fixture",
                    refresh::ExecutionMode::Persistent,
                    refresh::BudgetExtras {
                        per_decision: Some((8_000, 4_000)),
                        node_stats: true,
                    },
                )
                .unwrap();
                assert_eq!(
                    row["game"]["valid"], true,
                    "{}",
                    row["game"]["invalid_reason"]
                );
                let binding = row["bank_binding_sha256"].as_str().unwrap().to_owned();
                let decisions = row["game"]["decisions"].as_array().unwrap().clone();
                for (position, d) in decisions.iter().enumerate() {
                    if d["chooser"] != "reused" {
                        continue;
                    }
                    let at = Coordinates {
                        step: d["step"].as_u64().unwrap(),
                        physical_decision_id: d["physical_decision_id"].as_u64().unwrap(),
                        substep: d["substep"].as_u64().unwrap() as u32,
                    };
                    let root = authenticate(&src, &at, &sh.spec, &row, &binding, &mut 0).unwrap();
                    assert_eq!(root.chooser, json!("reused"));
                    authenticated += 1;
                    let map = d["node_stats"]["storage_to_live"].as_array().unwrap();
                    permuted += usize::from(map.iter().enumerate().any(|(e, l)| *l != json!(e)));
                    let refused = |edit: &dyn Fn(&mut Value)| {
                        let mut forged = row.clone();
                        edit(&mut forged["game"]["decisions"][position]);
                        authenticate(&src, &at, &sh.spec, &forged, &binding, &mut 0).is_err()
                    };
                    let witness = |key: &'static str, value: Value| {
                        move |r: &mut Value| r["reuse_witness"][key] = value.clone()
                    };
                    for (key, value) in [
                        ("node_key", json!("00".repeat(32))),
                        ("storage_edge_sha256", json!("00".repeat(32))),
                        ("search_tree_hash", json!("cd".repeat(32))),
                        ("applied_live_index", json!(99)),
                        ("natural_terminal_backups", json!(3)),
                        ("search", json!(999)),
                        ("schema", json!("other")),
                    ] {
                        assert!(refused(&witness(key, value)), "{key}");
                    }
                    assert!(refused(&|r| {
                        let n = r["reuse_witness"]["natural_terminal_backups"]
                            .as_u64()
                            .unwrap();
                        r["reuse_witness"]["focal_wins"] = json!(n + 1);
                    }));
                    assert!(refused(&|r| r["reuse_witness"]["decision"]["state_hash"] =
                        json!("0000000000000000")));
                    assert!(refused(&|r| {
                        r.as_object_mut().unwrap().remove("node_stats");
                        r["reuse_witness"]["storage_edge_index"] = json!(r["k"].as_u64().unwrap());
                    }));
                    assert!(refused(&|r| {
                        let map = r["node_stats"]["storage_to_live"].as_array_mut().unwrap();
                        map.swap(0, 1);
                    }));
                }
            }
            assert!(
                authenticated > 0,
                "the fixture games executed no reused decision"
            );
            eprintln!(
                "{authenticated} reused rows authenticated, {permuted} with a nonidentity live map"
            );
        });
    }

    fn at_copy(at: &Coordinates) -> Coordinates {
        Coordinates {
            step: at.step,
            physical_decision_id: at.physical_decision_id,
            substep: at.substep,
        }
    }

    #[test]
    #[ignore = "native qualification: small fixture policy inference"]
    fn a_forced_branch_does_not_depend_on_its_batch_or_repeat() {
        replay_stack(|| {
            let spec = spec(8, 12);
            let src = case(0);
            let (_, at) = later_root(&src, &spec, 0);
            let mut root = bank::source_session(&spec, &src).unwrap();
            while decision(&root).unwrap().step != at.step {
                let d = decision(&root).unwrap();
                root.step(d.episode_id, d.step, 0).unwrap();
            }
            let policy = FrozenPlayPolicyV1::training_fixture_v3();
            let s = seeds(100);
            let run = |order: &[u32]| {
                let mut focal = policy.fork_for_collection_v3().unwrap();
                let mut opponent = policy.fork_for_collection_v3().unwrap();
                order
                    .iter()
                    .map(|&a| {
                        let (branch, error) = branch_retained(
                            &root,
                            a,
                            PlayerId(src.seat),
                            &mut focal,
                            &mut opponent,
                            &s,
                            32,
                        )
                        .unwrap();
                        assert!(error.is_none());
                        branch
                    })
                    .collect::<Vec<_>>()
            };
            let alone = run(&[0]);
            let after_other = run(&[1, 0]);
            let repeated = run(&[0, 0]);
            assert_eq!(alone[0], after_other[1]);
            assert_eq!(alone[0], repeated[0]);
            assert_eq!(alone[0], repeated[1]);
            assert_eq!(after_other[0].first_action, 1);
        });
    }

    /// A branch resets both policies after the forced move, so its later
    /// decisions are scored by encoders that never saw the game's start. They
    /// must score every decision exactly as a policy that saw the whole game.
    #[test]
    #[ignore = "native qualification: small fixture policy inference"]
    fn a_reset_policy_scores_later_decisions_as_a_continuous_one() {
        replay_stack(|| {
            let spec = spec(64, 128);
            let src = case(0);
            let mut session = bank::source_session(&spec, &src).unwrap();
            let policy = FrozenPlayPolicyV1::training_fixture_v3();
            // One continuous policy per seat, as in a branch.
            let mut continuous = [
                policy.fork_for_collection_v3().unwrap(),
                policy.fork_for_collection_v3().unwrap(),
            ];
            continuous[0].reset_sampling_v1([1, 2]);
            continuous[1].reset_sampling_v1([3, 4]);
            let bits = |s: &crate::sideboard_play_policy_v1::FrozenPlayDecisionScoresV1| {
                let logits: Vec<u32> = s.logits.iter().map(|x| x.to_bits()).collect();
                (logits, s.value.to_bits())
            };
            let mut compared = [0; 2];
            while let Some(d) = decision(&session) {
                let seat = usize::from(acting(&d).0);
                let kept = continuous[seat].score_fast_session_v1(&session).unwrap();
                let mut fresh = policy.fork_for_collection_v3().unwrap();
                fresh.reset_sampling_v1([5, 6]);
                let reset = fresh.score_fast_session_v1(&session).unwrap();
                assert_eq!(bits(&kept), bits(&reset), "step {}", d.step);
                compared[seat] += 1;
                // Vary the line so later decisions see a mixed history.
                let action = (d.step % u64::from(d.legal_action_count)) as u32;
                session.step(d.episode_id, d.step, action).unwrap();
            }
            assert!(compared.iter().all(|&n| n >= 8), "{compared:?}");
        });
    }
}
