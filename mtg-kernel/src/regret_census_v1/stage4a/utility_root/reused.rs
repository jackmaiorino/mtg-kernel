//! Fresh witnessed reuse only. Historical utility-root refusals stay intact.
//!
//! `s4a-reuse-prefix` reads REUSE_PREFIX_SPEC and its SHA256, and writes one
//! bounded acquisition receipt. `s4a-reused-action` reads REUSED_ACTION_SPEC
//! and its SHA256, and writes one authenticated paired continuation receipt.
//! Both require games=1, first_game=0, workers=1 and a fresh output path.
//! A witness binds producer-record identity and a reproducible mapping; it
//! is not a serialized-tree membership proof or an action-quality label.

use super::super::canon2::{KeyMode, KeyStats, Keyer};
use super::super::{pathlog, refresh, reuse_witness, tree};
use super::*;

const ACQUISITION_SCHEMA: &str = "s4a-reused-prefix/v1/request";
const REQUEST_SCHEMA: &str = "s4a-reused-action/v1/request";
const SEARCH_CAP: u64 = 320_000;

#[cfg(test)]
mod tests;

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Acquisition {
    schema: String,
    bank_spec_sha256: String,
    case: SourceSpec,
    acquisition_search_cap: u64,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
enum Comparison {
    ReusedVsRaw,
    SameAction,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct RootCoordinates {
    step: u64,
    physical_decision_id: u64,
    substep: u32,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ComparisonRequest {
    schema: String,
    id: String,
    bank_spec_sha256: String,
    recorded_build: RecordedBuild,
    prefix: RecordPin,
    case: SourceSpec,
    root: RootCoordinates,
    mode: Comparison,
    comparator_policy_seeds: Option<[u64; 2]>,
    continuation_seeds: ContinuationSeeds,
    max_transitions: u64,
}

fn request_bytes(name: &str) -> Result<(Vec<u8>, String), String> {
    let path = std::env::var(name).map_err(|_| format!("{name} is required"))?;
    let pin_name = format!("{name}_SHA256");
    let pin = std::env::var(&pin_name).map_err(|_| format!("{pin_name} is required"))?;
    let bytes = std::fs::read(&path).map_err(|e| format!("{path}: {e}"))?;
    pinned_bytes(&bytes, &pin)?;
    Ok((bytes, pin))
}

fn single_job(cfg: &CensusConfigV1) -> Result<(), String> {
    if cfg.games != 1 || cfg.first_game != 0 || cfg.workers != 1 {
        return Err("reused-action modes require games=1, first_game=0, workers=1".into());
    }
    Ok(())
}

fn write_fresh(cfg: &CensusConfigV1, row: &Value) -> Result<(), String> {
    let mut output = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&cfg.out)
        .map_err(|e| format!("fresh reused-action output {}: {e}", cfg.out))?;
    writeln!(output, "{row}").map_err(|e| e.to_string())?;
    output.sync_all().map_err(|e| e.to_string())
}

pub(in super::super) fn run_acquisition(
    cfg: &CensusConfigV1,
    policy: &FrozenPlayPolicyV1,
    sh: &BankShared,
    bank_bytes: &[u8],
) -> Result<(), String> {
    single_job(cfg)?;
    let (bytes, pin) = request_bytes("REUSE_PREFIX_SPEC")?;
    let req: Acquisition = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    sh.spec.validate()?;
    validate_case(&req.case, sh)?;
    if req.schema != ACQUISITION_SCHEMA || req.acquisition_search_cap != SEARCH_CAP {
        return Err("reuse acquisition schema or fixed search cap differs".into());
    }
    if sh.limits.json() != super::super::arms::Limits::FORMAL.json() {
        return Err("reuse acquisition requires the original executor limits".into());
    }
    pinned_bytes(bank_bytes, &req.bank_spec_sha256)?;
    let binding = bank::bank_binding(policy, sh)?;
    let mut row = std::thread::scope(|scope| {
        std::thread::Builder::new()
            .stack_size(256 << 20)
            .spawn_scoped(scope, || {
                refresh::reuse_prefix_case(sh, policy, &req.case, SEARCH_CAP, &binding)
            })
            .map_err(|e| e.to_string())?
            .join()
            .map_err(|_| "reuse acquisition worker panicked".to_owned())?
    })?;
    row["request"] = serde_json::to_value(req).map_err(|e| e.to_string())?;
    row["request_sha256"] = json!(pin);
    write_fresh(cfg, &row)?;
    if row["complete"] != true {
        return Err("reuse acquisition failed; request-bound partial receipt preserved".into());
    }
    Ok(())
}

fn validate_comparison(req: &ComparisonRequest, sh: &BankShared) -> Result<(), String> {
    sh.spec.validate()?;
    validate_case(&req.case, sh)?;
    validate_cap(req.max_transitions)?;
    if req.schema != REQUEST_SCHEMA || req.id.is_empty() {
        return Err("reused-action request schema or id differs".into());
    }
    hex_pin(&req.bank_spec_sha256, 64)?;
    hex_pin(&req.recorded_build.git_head, 40)?;
    hex_pin(&req.recorded_build.tracked_tree_sha256, 64)?;
    hex_pin(&req.prefix.sha256, 64)?;
    if (req.mode == Comparison::ReusedVsRaw) != req.comparator_policy_seeds.is_some() {
        return Err("raw comparison requires seeds; same-action requests must not draw".into());
    }
    // Role/seat streams must be separately named, not acquisition streams.
    let mut seeds = std::collections::HashSet::new();
    for seed in req
        .case
        .source_policy_seeds
        .into_iter()
        .chain(req.case.guide_policy_seeds)
        .chain(req.continuation_seeds.focal)
        .chain(req.continuation_seeds.opponent)
        .chain(req.comparator_policy_seeds.into_iter().flatten())
    {
        if !seeds.insert(seed) {
            return Err("reused-action policy seed namespaces overlap".into());
        }
    }
    Ok(())
}

fn count(value: &Value, key: &str) -> Result<u64, String> {
    value[key]
        .as_u64()
        .ok_or_else(|| format!("missing or invalid {key}"))
}

fn search_at(row: &Value, index: u64) -> Result<&Value, String> {
    let index = usize::try_from(index).map_err(|_| "search index overflow")?;
    let record = row["game"]["searches"]
        .as_array()
        .and_then(|v| v.get(index))
        .ok_or("originating search missing")?;
    if record["search"] != index as u64 || record["root"]["step"] != record["step"] {
        return Err("originating search index/root step differs".into());
    }
    Ok(record)
}

fn validate_envelope(
    req: &ComparisonRequest,
    row: &Value,
    lineage: &str,
    binding: &str,
) -> Result<Vec<u32>, String> {
    if row["kind"] != "reused_prefix"
        || row["schema"] != "s4a-reused-prefix/v1"
        || row["status"] != "acquired"
        || row["complete"] != true
        || row["lineage"] != lineage
        || row["bank_binding_sha256"] != binding
        || row["git_head"] != req.recorded_build.git_head
        || row["tracked_tree_sha256"] != req.recorded_build.tracked_tree_sha256
        || row["case"] != serde_json::to_value(&req.case).map_err(|e| e.to_string())?
        || row["allocation"] != json!({"initial":128000,"refresh":64000,"total":2048000})
        || row["acquisition_search_cap"] != SEARCH_CAP
        || row["interchange"] != false
        || row["backup"] != "mean"
        || row["game"]["key_mode"] != "repaired-v2"
        || row["game"]["reuse_witnesses"]["schema"] != reuse_witness::SCHEMA
        || row["game"]["reuse_witnesses"]["enabled"] != true
        || !row["error"].is_null()
        || !row["blocked_search"].is_null()
    {
        return Err("acquired prefix envelope, case, recipe or build binding differs".into());
    }
    let actions: Vec<u32> = serde_json::from_value(row["game"]["actions"].clone())
        .map_err(|e| format!("prefix action array: {e}"))?;
    let selected = &row["selected"];
    let index = count(selected, "action_index")?;
    if index != req.root.step
        || usize::try_from(index).ok().and_then(|i| i.checked_add(1)) != Some(actions.len())
        || selected["action"]
            != actions
                .last()
                .copied()
                .map(Value::from)
                .unwrap_or(Value::Null)
        || count(&row["game"], "real_transitions")? != actions.len() as u64
        || count(&row["game"], "transitions_used")? > SEARCH_CAP
    {
        return Err("selected prefix action boundary or spending differs".into());
    }
    Ok(actions)
}

fn validate_witness(
    witness: &Value,
    recorded: &Value,
    identity: &Value,
    search: &Value,
    key: &tree::Key,
    canon: &tree::Canon,
    action: u32,
) -> Result<(), String> {
    let edge = usize::try_from(count(witness, "storage_edge_index")?)
        .map_err(|_| "storage edge overflow")?;
    let bytes = canon
        .edges
        .get(edge)
        .ok_or("storage edge outside reconstructed menu")?;
    let backups = count(witness, "natural_terminal_backups")?;
    let wins = count(witness, "focal_wins")?;
    let mean = wins as f64 / backups as f64;
    if witness["schema"] != reuse_witness::SCHEMA
        || witness["decision"] != *identity
        || witness["search"] != search["search"]
        || recorded["search"] != search["search"]
        || witness["search_tree_hash"] != search["tree"]["hash"]
        || witness["node_key"] != pathlog::key_hex(key)
        || witness["storage_edge_sha256"] != sha(bytes)
        || witness["applied_live_index"] != action
        || canon.menu.get(action as usize) != Some(bytes)
        || tree::live_index(canon, bytes).map_err(bank::err)? != action
        || backups < tree::MIN_EXEC_BACKUPS
        || wins > backups
        || recorded["edge_backups"] != backups
        || witness["mean_focal_win_rate"].as_f64() != Some(mean)
        || witness["selection_backup"] != "mean"
        || witness["selection_value"].as_f64() != Some(mean)
    {
        return Err("reused witness identity, evidence or live/storage mapping differs".into());
    }
    hex_pin(
        search["tree"]["hash"].as_str().ok_or("missing tree hash")?,
        64,
    )
}

struct ReusedRoot {
    session: FastActorSessionV1,
    identity: Value,
    action: u32,
    prefix_len: usize,
    prefix_sha256: String,
    witness: Value,
}

fn reconstruct_reused(
    req: &ComparisonRequest,
    sh: &BankShared,
    row: &Value,
    binding: &str,
    replayed: &mut u64,
) -> Result<ReusedRoot, String> {
    let actions = validate_envelope(req, row, &sh.spec.lineage, binding)?;
    let index = actions.len() - 1;
    let selected = &row["selected"];
    let search_index = count(selected, "originating_search")?;
    let search = search_at(row, search_index)?;
    let origin = count(search, "step")?;
    let origin_physical = count(&search["root"], "physical_decision_id")?;
    if origin >= index as u64
        || origin_physical >= req.root.physical_decision_id
        || selected["originating_search_step"] != origin
        || selected["originating_search_physical_decision_id"] != origin_physical
    {
        return Err("selected reuse is not a later physical decision of its search".into());
    }
    let mut session = bank::source_session(&sh.spec, &req.case)?;
    let mut keyer = Keyer::new(KeyMode::Repaired, KeyStats::default());
    let mut focal_records = 0usize;
    let mut result = None;
    for (step, &action) in actions.iter().enumerate() {
        let d = decision(&session).ok_or("prefix replay terminated early")?;
        if d.step != step as u64 || action >= d.legal_action_count {
            return Err("prefix replay step or executable action differs".into());
        }
        if step as u64 == origin
            && (acting(&d) != PlayerId(req.case.seat)
                || d.legal_action_count < 2
                || bank::decision_identity(&session)? != search["root"])
        {
            return Err("originating search live identity differs".into());
        }
        if acting(&d) == PlayerId(req.case.seat) && d.legal_action_count >= 2 {
            let recorded = recorded_decision(row, &d)?;
            focal_records += 1;
            if recorded["chooser"] == "reused" && step < index {
                let earlier = search_at(row, count(recorded, "search")?)?;
                if d.physical_decision_id > count(&earlier["root"], "physical_decision_id")? {
                    return Err("prefix skipped an earlier eligible reused decision".into());
                }
            }
            if step as u64 >= origin {
                if recorded["search"] != search_index {
                    return Err("prefix switched searches within selected Keyer history".into());
                }
                let keyed = keyer.step(&session, d).map_err(bank::err)?;
                if step == index {
                    let identity = bank::decision_identity(&session)?;
                    if recorded["chooser"] != "reused"
                        || identity["physical_decision_id"] != req.root.physical_decision_id
                        || identity["substep"] != req.root.substep
                        || selected["decision"] != identity
                        || selected["witness"] != recorded["reuse_witness"]
                    {
                        return Err("selected reused decision identity differs".into());
                    }
                    let (key, _, _, canon) = keyer.storage(&keyed);
                    validate_witness(
                        &selected["witness"],
                        recorded,
                        &identity,
                        search,
                        &key,
                        canon,
                        action,
                    )?;
                    result = Some(ReusedRoot {
                        session: session.clone(),
                        identity,
                        action,
                        prefix_len: index,
                        prefix_sha256: sha(&actions[..index]
                            .iter()
                            .flat_map(|a| a.to_le_bytes())
                            .collect::<Vec<_>>()),
                        witness: selected["witness"].clone(),
                    });
                }
                keyer.advance(keyed, action);
            }
        }
        session
            .step(d.episode_id, d.step, action)
            .map_err(|e| format!("{e:?}"))?;
        *replayed += 1;
    }
    if row["game"]["decisions"].as_array().map(Vec::len) != Some(focal_records)
        || selected["post_action_diagnostic_state_hash"]
            != format!("{:016x}", session.diagnostic_state_hash())
        || selected["post_action_core_environment_hash"]
            != format!("{:016x}", session.privileged_core_environment_hash())
        || selected["post_action_policy_steps"] != session.policy_step_count()
        || selected["post_action_physical_decisions"] != session.physical_decision_count()
        || selected["post_action_terminal"]
            != match session.current_response() {
                crate::rl_session::FastActorResponseV1::Terminal(value) => json!(value),
                crate::rl_session::FastActorResponseV1::Decision(_) => Value::Null,
            }
        || selected["post_action_identity"]
            != decision(&session)
                .map(|_| bank::decision_identity(&session))
                .transpose()?
                .unwrap_or(Value::Null)
    {
        return Err("prefix post-action state or decision coverage differs".into());
    }
    result.ok_or_else(|| "selected prefix boundary was not a focal choice".into())
}

fn diagnose_reused(
    req: &ComparisonRequest,
    sh: &BankShared,
    policy: &FrozenPlayPolicyV1,
    prefix: &Value,
    binding: &str,
) -> Value {
    let mut diagnostic = json!({"authenticated":false,"complete":false});
    let mut replayed = 0u64;
    let mut comparator_calls = Counters::default();
    let mut raw_action = None;
    let (mut a, mut b): (Option<Branch>, Option<Branch>) = (None, None);
    let mut phase = "authentication";
    let failure = (|| -> Result<(), String> {
        let root = reconstruct_reused(req, sh, prefix, binding, &mut replayed)?;
        diagnostic["authenticated"] = json!(true);
        diagnostic["root"] = root.identity;
        diagnostic["witness"] = root.witness;
        diagnostic["prefix_before_action_transitions"] = json!(root.prefix_len);
        diagnostic["prefix_before_action_sha256"] = json!(root.prefix_sha256);
        diagnostic["reused_action"] = json!(root.action);
        phase = "comparator";
        if let Some(seeds) = req.comparator_policy_seeds {
            let mut comparator = policy.fork_for_collection_v3()?;
            comparator.reset_sampling_v1(seeds);
            let d = decision(&root.session).ok_or("authenticated root is terminal")?;
            raw_action = Some(
                act(&mut comparator, &root.session, &d, &mut comparator_calls)
                    .map_err(bank::err)?,
            );
        }
        if comparator_calls.inference != u64::from(raw_action.is_some()) {
            return Err("raw comparator did not make exactly one policy call".into());
        }
        let comparison_action = raw_action.unwrap_or(root.action);
        diagnostic["comparison_action"] = json!(comparison_action);
        let opponent_index = sh
            .labels
            .iter()
            .position(|x| *x == req.case.opp)
            .ok_or("opponent missing")?;
        let mut policy = policy.fork_for_collection_v3()?;
        let mut opponent = sh.opponents[opponent_index].fork_for_collection_v3()?;
        let mut run = |action| {
            branch_retained(
                &root.session,
                action,
                PlayerId(req.case.seat),
                &mut policy,
                &mut opponent,
                &req.continuation_seeds,
                req.max_transitions,
            )
        };
        phase = "reused_branch";
        let (receipt, error) = run(root.action)?;
        a = Some(receipt);
        if let Some(error) = error {
            return Err(error);
        }
        phase = "comparison_branch";
        let (receipt, error) = run(comparison_action)?;
        b = Some(receipt);
        if let Some(error) = error {
            return Err(error);
        }
        phase = "parity";
        if root.action == comparison_action {
            if a != b {
                return Err("equal-action scientific branch receipts differ".into());
            }
            diagnostic["equal_action_parity"] = json!(true);
        }
        Ok(())
    })()
    .err();
    let ar = a.as_ref().and_then(|x| x.result);
    let br = b.as_ref().and_then(|x| x.result);
    let ((alo, ahi), (blo, bhi)) = (score_range(ar), score_range(br));
    diagnostic["complete"] = json!(failure.is_none());
    diagnostic["failure"] = failure
        .map(|error| json!({"phase":phase,"error":error}))
        .unwrap_or(Value::Null);
    diagnostic["authentication_transitions"] = json!(replayed);
    diagnostic["sampled_raw_action"] = json!(raw_action);
    diagnostic["comparator_inference_calls"] = json!(comparator_calls.inference);
    // A failed selection may have consumed a draw; do not invent a count.
    diagnostic["comparator_sampler_draws"] =
        if comparator_calls.inference > 0 && raw_action.is_none() {
            Value::Null
        } else {
            json!(u64::from(raw_action.is_some()))
        };
    diagnostic["paired_resolved"] = json!(ar.is_some() && br.is_some());
    diagnostic["paired_signed_result"] =
        json!(ar.zip(br).map(|(x, y)| i64::from(x) - i64::from(y)));
    diagnostic["paired_signed_lower"] = json!(alo - bhi);
    diagnostic["paired_signed_upper"] = json!(ahi - blo);
    diagnostic["total_transitions"] = json!(
        replayed
            + a.as_ref().map_or(0, |x| x.transitions)
            + b.as_ref().map_or(0, |x| x.transitions)
    );
    diagnostic["total_inference_calls"] = json!(
        comparator_calls.inference
            + a.as_ref().map_or(0, |x| x.inference_calls)
            + b.as_ref().map_or(0, |x| x.inference_calls)
    );
    diagnostic["reused"] = json!(a);
    diagnostic["comparison"] = json!(b);
    diagnostic
}

pub(in super::super) fn run_comparison(
    cfg: &CensusConfigV1,
    policy: &FrozenPlayPolicyV1,
    sh: &BankShared,
    bank_bytes: &[u8],
) -> Result<(), String> {
    single_job(cfg)?;
    let (bytes, pin) = request_bytes("REUSED_ACTION_SPEC")?;
    let req: ComparisonRequest = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    validate_comparison(&req, sh)?;
    pinned_bytes(bank_bytes, &req.bank_spec_sha256)?;
    let historical_binding = bank::bank_binding_with_build(
        policy,
        sh,
        &req.recorded_build.git_head,
        &req.recorded_build.tracked_tree_sha256,
    )?;
    let prefix = read_record(&req.prefix)?;
    let diagnostic = std::thread::scope(|scope| {
        std::thread::Builder::new()
            .stack_size(256 << 20)
            .spawn_scoped(scope, || {
                diagnose_reused(&req, sh, policy, &prefix, &historical_binding)
            })
            .map_err(|e| e.to_string())?
            .join()
            .map_err(|_| "reused-action worker panicked".to_owned())
    })?;
    let complete = diagnostic["complete"] == true;
    write_fresh(
        cfg,
        &json!({"kind":"reused_action","schema":"s4a-reused-action/v1/receipt",
        "request_sha256":pin,"request":req,"historical_bank_binding_sha256":historical_binding,
        "current_bank_binding_sha256":bank::bank_binding(policy,sh)?,
        "git_head":env!("MTG_KERNEL_BUILD_GIT_HEAD"),
        "tracked_tree_sha256":env!("MTG_KERNEL_BUILD_TRACKED_TREE_SHA256"),
        "carddb_hash":format!("{:016x}",crate::card_def::KERNEL_CARDDB_HASH),
        "continuation_convention":"fresh-role-seat-streams-after-forced-root-v1",
        "score_convention":"signed win=1 draw=0 loss=-1; ordinary game-score differences are signed differences / 2",
        "tree_membership":"verified by producer; consumer verifies record binding and replayed mapping",
        "complete":complete,
        "fault_counter_convention":"fault transitions include any charged failed application; action hashes include only successful applications",
        "diagnostic":diagnostic}),
    )?;
    if !complete {
        return Err("reused-action failed; request-bound partial receipt preserved".into());
    }
    Ok(())
}
