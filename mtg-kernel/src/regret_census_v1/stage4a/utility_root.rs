//! One authenticated shared-root action diagnostic. No search or world sampling.
//!
//! `s4a-utility-root` requires games=1, first_game=0, workers=1, the ordinary
//! `BANK_SPEC`, `OPPONENTS`, source policy, and `UTILITY_ROOT_SPEC` plus
//! `UTILITY_ROOT_SPEC_SHA256` (SHA-256 of the exact request file bytes).
//! Request JSON (unknown fields fail):
//! ```text
//! {"schema":"shared-root-utility-v1/request", "id":"one-root",
//!  "bank_spec_sha256":"<exact BANK_SPEC bytes>",
//!  "recorded_build":{"git_head":"<40 hex>","tracked_tree_sha256":"<64 hex>"},
//!  "search":{"path":"search.json","sha256":"<exact single row file bytes>"},
//!  "plain":{"path":"plain.json","sha256":"<exact single row file bytes>"},
//!  "case":<bank::SourceSpec>,
//!  "root":{"kind":"first-divergence"},
//!  "continuation_seeds":{"focal":[11,13],"opponent":[17,19]},
//!  "mode":"same-action", "max_transitions":64}
//! ```
//! Root can instead be `{"kind":"step","step":N}`; N must still be the
//! true first divergence of the complete saved executable action arrays.
//! This initial adapter requires a `new_root` choice whose recorded search
//! root exactly matches the reconstructed decision, observation, menu and
//! diagnostic state hashes. `reused` choices have no exact historical root
//! witness in these rows and are refused.
//! Mode is `same-action` or `search-vs-plain`. The cap is 1..=2048 per
//! branch, including its forced root action. Continuation streams are named
//! by policy role and physical seat, reset AFTER that action. They are fresh
//! continuations, never a reconstruction of either recorded tail sampler.
//! The inherited session's episode limits remain in force. Output is one
//! receipt at a fresh `out` path; no resume, batches or outcome filtering.

use super::bank::{self, BankShared, SourceSpec};
use super::play::{act, acting, apply, decision, Counters, Meter};
use super::{sha, CensusConfigV1};
use crate::ids::PlayerId;
use crate::rl_session::{FastActorDecisionV1, FastActorSessionV1};
use crate::sideboard_play_policy_v1::FrozenPlayPolicyV1;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::io::Write;

const REQUEST_SCHEMA: &str = "shared-root-utility-v1/request";
const MAX_TRANSITIONS: u64 = 2048;

#[cfg(feature = "gameplay-decision-trace-v1")]
mod inspect;

pub(super) fn run_inspect(
    cfg: &CensusConfigV1,
    policy: &FrozenPlayPolicyV1,
    sh: &BankShared,
    bank_spec_bytes: &[u8],
) -> Result<(), String> {
    #[cfg(feature = "gameplay-decision-trace-v1")]
    return inspect::run(cfg, policy, sh, bank_spec_bytes);
    #[cfg(not(feature = "gameplay-decision-trace-v1"))]
    {
        let _ = (cfg, policy, sh, bank_spec_bytes);
        Err("s4a-utility-root-inspect requires gameplay-decision-trace-v1".into())
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct RecordPin {
    path: String,
    sha256: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct RecordedBuild {
    git_head: String,
    tracked_tree_sha256: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
enum Root {
    FirstDivergence,
    Step { step: u64 },
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
enum Mode {
    SameAction,
    SearchVsPlain,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ContinuationSeeds {
    focal: [u64; 2],
    opponent: [u64; 2],
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Request {
    schema: String,
    id: String,
    bank_spec_sha256: String,
    recorded_build: RecordedBuild,
    search: RecordPin,
    plain: RecordPin,
    case: SourceSpec,
    root: Root,
    continuation_seeds: ContinuationSeeds,
    mode: Mode,
    max_transitions: u64,
}

fn hex_pin(pin: &str, length: usize) -> Result<(), String> {
    if pin.len() != length
        || !pin
            .bytes()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
    {
        return Err(format!("expected {length} lowercase hexadecimal pin"));
    }
    Ok(())
}

fn pinned_bytes<'a>(bytes: &'a [u8], expected: &str) -> Result<&'a [u8], String> {
    hex_pin(expected, 64)?;
    if sha(bytes) != expected {
        return Err("SHA-256 pin mismatch".into());
    }
    Ok(bytes)
}

fn read_record(pin: &RecordPin) -> Result<Value, String> {
    let bytes = std::fs::read(&pin.path).map_err(|e| format!("{}: {e}", pin.path))?;
    let bytes = pinned_bytes(&bytes, &pin.sha256)?;
    // from_slice rejects a second JSON value; exact byte pin includes any newline.
    serde_json::from_slice(bytes).map_err(|e| format!("{}: {e}", pin.path))
}

fn validate_request(req: &Request, sh: &BankShared) -> Result<(), String> {
    sh.spec.validate()?;
    if req.schema != REQUEST_SCHEMA || req.id.is_empty() {
        return Err("utility request schema or id differs".into());
    }
    hex_pin(&req.bank_spec_sha256, 64)?;
    hex_pin(&req.recorded_build.git_head, 40)?;
    hex_pin(&req.recorded_build.tracked_tree_sha256, 64)?;
    hex_pin(&req.search.sha256, 64)?;
    hex_pin(&req.plain.sha256, 64)?;
    validate_case(&req.case, sh)?;
    validate_cap(req.max_transitions)
}

fn validate_case(src: &SourceSpec, sh: &BankShared) -> Result<(), String> {
    if src.id.is_empty()
        || src.search_root.is_empty()
        || src.own >= sh.spec.decks.len()
        || src.other >= sh.spec.decks.len()
        || src.seat > 1
        || src.starting_player > 1
        || !sh.labels.contains(&src.opp)
    {
        return Err("invalid utility case coordinates or opponent".into());
    }
    Ok(())
}

fn validate_cap(cap: u64) -> Result<(), String> {
    if !(1..=MAX_TRANSITIONS).contains(&cap) {
        return Err(format!(
            "utility max_transitions must be 1..={MAX_TRANSITIONS}"
        ));
    }
    Ok(())
}

/// Rows store final live menu indices in game.actions. Never read tree edges.
fn row_actions(
    row: &Value,
    src: &SourceSpec,
    lineage: &str,
    binding: &str,
    plain: bool,
) -> Result<Vec<u32>, String> {
    let arm = row.get("arm").and_then(Value::as_str);
    if (plain && arm != Some("plain-v1"))
        || (!plain && arm.is_some() && arm != Some("search-v1"))
        || (!plain && row.get("arm").is_some() && arm.is_none())
        || row["kind"] != "budget_game"
        || row["schema"] != "search-total-budget-v1/game"
        || row["lineage"] != lineage
        || row["bank_binding_sha256"] != binding
    {
        return Err("utility row schema, arm, lineage or bank binding differs".into());
    }
    for (key, expected) in [
        ("id", json!(src.id)),
        ("own", json!(src.own)),
        ("other", json!(src.other)),
        ("seat", json!(src.seat)),
        ("opp", json!(src.opp)),
        ("seed", json!(src.seed)),
        ("starting_player", json!(src.starting_player)),
        ("live_policy_seeds", json!(src.guide_policy_seeds)),
        ("search_root", json!(src.search_root)),
    ] {
        if row[key] != expected {
            return Err(format!("utility row case field {key} differs"));
        }
    }
    serde_json::from_value(row["game"]["actions"].clone())
        .map_err(|e| format!("utility executable action array: {e}"))
}

fn first_divergence(search: &[u32], plain: &[u32], root: Root) -> Result<usize, String> {
    let index = search
        .iter()
        .zip(plain)
        .position(|(a, b)| a != b)
        .ok_or("recorded action arrays have no common-root divergence")?;
    if let Root::Step { step } = root {
        if step != index as u64 {
            return Err("declared step is not the first full-action divergence".into());
        }
    }
    Ok(index)
}

fn recorded_decision<'a>(row: &'a Value, d: &FastActorDecisionV1) -> Result<&'a Value, String> {
    let rows = row["game"]["decisions"]
        .as_array()
        .ok_or("utility row has no decision records")?;
    let mut matches = rows.iter().filter(|r| r["step"] == d.step);
    let r = matches.next().ok_or("focal root decision record missing")?;
    if matches.next().is_some()
        || r["physical_decision_id"] != d.physical_decision_id
        || r["substep"] != d.substep_index
        || r["k"] != d.legal_action_count
    {
        return Err("focal root decision record identity differs or is duplicated".into());
    }
    Ok(r)
}

fn candidates(
    search: &[u32],
    plain: &[u32],
    index: usize,
    width: u32,
) -> Result<(u32, u32), String> {
    let a = *search.get(index).ok_or("missing saved search action")?;
    let b = *plain.get(index).ok_or("missing saved plain action")?;
    if width < 2 || a >= width || b >= width || a == b {
        return Err("saved root actions are equal, forced or outside the live menu".into());
    }
    Ok((a, b))
}

fn validate_choosers(search: &Value, plain: &Value) -> Result<(), String> {
    if !matches!(search["chooser"].as_str(), Some("new_root" | "reused"))
        || plain["chooser"] != "plain"
    {
        return Err("first divergence lacks a supported saved search choice/plain control".into());
    }
    Ok(())
}

/// An in-range saved index is insufficient if replay semantics changed.
/// Bind it to the exact historical decision/menu/state witness emitted by
/// refresh::Searches::run before the original search. Reused nodes do not
/// carry such a witness at their later decision in ordinary budget rows.
fn validate_historical_root(
    search_row: &Value,
    search_decision: &Value,
    reconstructed: &Value,
) -> Result<(), String> {
    if search_decision["chooser"] != "new_root" {
        return Err(
            "utility reused root refused: no exact historical decision/menu/state witness".into(),
        );
    }
    let index = search_decision["search"]
        .as_u64()
        .and_then(|i| usize::try_from(i).ok())
        .ok_or("utility historical search index missing or invalid")?;
    let record = search_row["game"]["searches"]
        .as_array()
        .and_then(|records| records.get(index))
        .ok_or("utility historical search root record missing")?;
    if record["search"] != index as u64 || record["step"] != reconstructed["step"] {
        return Err("utility historical search record index or step differs".into());
    }
    for key in [
        "step",
        "physical_decision_id",
        "substep",
        "k",
        "obs_hash",
        "menu_hash",
        "state_hash",
    ] {
        if reconstructed.get(key).is_none()
            || record["root"].get(key).is_none()
            || record["root"][key] != reconstructed[key]
        {
            return Err(format!(
                "utility historical root witness {key} differs or is absent"
            ));
        }
    }
    Ok(())
}

#[derive(Debug, Serialize, PartialEq, Eq)]
struct Branch {
    first_action: u32,
    end: &'static str,
    result: Option<i8>,
    transitions: u64,
    inference_calls: u64,
    actions_sha256: String,
    final_diagnostic_state_hash: String,
    final_core_environment_hash: String,
    final_policy_steps: u64,
    final_physical_decisions: u64,
}

fn hash_action(hash: &mut Sha256, d: &FastActorDecisionV1, action: u32) {
    hash.update(d.step.to_le_bytes());
    hash.update([acting(d).0]);
    hash.update(action.to_le_bytes());
}

#[allow(
    clippy::too_many_arguments,
    reason = "explicit branch roles, seeds and cap"
)]
fn branch(
    root: &FastActorSessionV1,
    first: u32,
    focal: PlayerId,
    policy: &mut FrozenPlayPolicyV1,
    opponent: &mut FrozenPlayPolicyV1,
    seeds: &ContinuationSeeds,
    cap: u64,
) -> Result<Branch, String> {
    validate_cap(cap)?;
    let mut session = root.clone();
    let d = decision(&session).ok_or("utility branch root is terminal")?;
    if first >= d.legal_action_count {
        return Err("utility forced action is outside the live menu".into());
    }
    let defs = super::labels::spy_defs();
    let mut meter = Meter::new(cap);
    let mut counters = Counters::default();
    let mut actions_hash = Sha256::new();
    // Applying a saved action consumes no policy draw. Reset only afterwards.
    apply(&mut session, &d, first, &mut meter, focal, None, &defs).map_err(bank::err)?;
    hash_action(&mut actions_hash, &d, first);
    policy.reset_sampling_v1(seeds.focal);
    opponent.reset_sampling_v1(seeds.opponent);
    while let Some(d) = decision(&session) {
        // Check before inference: no unused draw/call beyond the transition cap.
        if meter.spent == meter.cap {
            break;
        }
        let action = if acting(&d) == focal {
            act(policy, &session, &d, &mut counters)
        } else {
            act(opponent, &session, &d, &mut counters)
        }
        .map_err(bank::err)?;
        apply(&mut session, &d, action, &mut meter, focal, None, &defs).map_err(bank::err)?;
        hash_action(&mut actions_hash, &d, action);
    }
    let end = bank::outcome(&session, focal).unwrap_or("truncated");
    let result = match end {
        "win" => Some(1),
        "draw" => Some(0),
        "loss" => Some(-1),
        _ => None,
    };
    Ok(Branch {
        first_action: first,
        end,
        result,
        transitions: meter.spent,
        inference_calls: counters.inference,
        actions_sha256: super::hex(&actions_hash.finalize()),
        final_diagnostic_state_hash: format!("{:016x}", session.diagnostic_state_hash()),
        final_core_environment_hash: format!("{:016x}", session.privileged_core_environment_hash()),
        final_policy_steps: session.policy_step_count(),
        final_physical_decisions: session.physical_decision_count(),
    })
}

fn score_range(result: Option<i8>) -> (i64, i64) {
    result.map_or((-1, 1), |x| (i64::from(x), i64::from(x)))
}

struct AuthenticatedRoot {
    session: FastActorSessionV1,
    identity: Value,
    prefix_len: usize,
    prefix_sha256: String,
    search_action: u32,
    plain_action: u32,
    search_chooser: Value,
}

/// Shared authentication ends before policy forks, stream resets or root actions.
fn reconstruct(
    src: &SourceSpec,
    requested_root: Root,
    spec: &bank::BankSpec,
    search: &Value,
    plain: &Value,
    historical_binding: &str,
    prefix_transitions: &mut u64,
) -> Result<AuthenticatedRoot, String> {
    let search_actions = row_actions(search, src, &spec.lineage, historical_binding, false)?;
    let plain_actions = row_actions(plain, src, &spec.lineage, historical_binding, true)?;
    let index = first_divergence(&search_actions, &plain_actions, requested_root)?;
    // Both full action arrays share this exact prefix, including opponent
    // decisions and forced actions. Replay once, without policy inference.
    let mut root = bank::source_session(spec, src)?;
    for &action in &search_actions[..index] {
        let d = decision(&root).ok_or("replay reached a terminal early")?;
        if action >= d.legal_action_count {
            return Err("shared-prefix action outside the live menu".into());
        }
        root.step(d.episode_id, d.step, action)
            .map_err(|e| format!("{e:?}"))?;
        *prefix_transitions += 1;
    }
    let d = decision(&root).ok_or("first divergence is terminal")?;
    if d.step != index as u64 || acting(&d) != PlayerId(src.seat) {
        return Err("first divergence is not the requested focal decision".into());
    }
    let search_decision = recorded_decision(search, &d)?;
    let plain_decision = recorded_decision(plain, &d)?;
    validate_choosers(search_decision, plain_decision)?;
    let (search_action, plain_action) =
        candidates(&search_actions, &plain_actions, index, d.legal_action_count)?;
    // Candidates are fixed before privileged state hashes/clones or outcomes.
    let root_identity = bank::decision_identity(&root)?;
    validate_historical_root(search, search_decision, &root_identity)?;
    Ok(AuthenticatedRoot {
        session: root,
        identity: root_identity,
        prefix_len: index,
        prefix_sha256: sha(&search_actions[..index]
            .iter()
            .flat_map(|a| a.to_le_bytes())
            .collect::<Vec<_>>()),
        search_action,
        plain_action,
        search_chooser: search_decision["chooser"].clone(),
    })
}

fn diagnose(
    req: &Request,
    sh: &BankShared,
    policy: &FrozenPlayPolicyV1,
    search: &Value,
    plain: &Value,
    historical_binding: &str,
) -> Result<Value, String> {
    let src = &req.case;
    let authenticated = reconstruct(
        src,
        req.root,
        &sh.spec,
        search,
        plain,
        historical_binding,
        &mut 0,
    )?;
    let root = authenticated.session;
    let root_identity = authenticated.identity;
    let index = authenticated.prefix_len;
    let search_action = authenticated.search_action;
    let plain_action = authenticated.plain_action;
    let root_core_hash = format!("{:016x}", root.privileged_core_environment_hash());
    let opp = sh
        .labels
        .iter()
        .position(|l| *l == src.opp)
        .ok_or("unknown opponent")?;
    let mut focal_policy = policy.fork_for_collection_v3()?;
    let mut opponent_policy = sh.opponents[opp].fork_for_collection_v3()?;
    let mut run = |action| {
        branch(
            &root,
            action,
            PlayerId(src.seat),
            &mut focal_policy,
            &mut opponent_policy,
            &req.continuation_seeds,
            req.max_transitions,
        )
    };
    let a = run(search_action)?;
    let b = run(if req.mode == Mode::SameAction {
        search_action
    } else {
        plain_action
    })?;
    if req.mode == Mode::SameAction && a != b {
        return Err("same-action scientific branch receipts differ".into());
    }
    let ((alo, ahi), (blo, bhi)) = (score_range(a.result), score_range(b.result));
    Ok(json!({
        "root":root_identity,"root_core_environment_hash":root_core_hash,
        "root_policy_steps":root.policy_step_count(),
        "root_physical_decisions":root.physical_decision_count(),
        "shared_prefix_transitions":index,
        "shared_prefix_actions_sha256":authenticated.prefix_sha256,
        "saved_search_action":search_action,"saved_plain_action":plain_action,
        "saved_search_chooser":authenticated.search_chooser,
        "search":a,"comparison":b,
        "same_action_parity":(req.mode == Mode::SameAction).then_some(true),
        "paired_resolved":a.result.is_some() && b.result.is_some(),
        "paired_signed_result":a.result.zip(b.result).map(|(x,y)| i64::from(x)-i64::from(y)),
        "paired_signed_lower":alo-bhi,"paired_signed_upper":ahi-blo,
        "total_transitions":index as u64 + a.transitions + b.transitions,
        "total_inference_calls":a.inference_calls+b.inference_calls
    }))
}

pub(super) fn run(
    cfg: &CensusConfigV1,
    policy: &FrozenPlayPolicyV1,
    sh: &BankShared,
    bank_spec_bytes: &[u8],
) -> Result<(), String> {
    if cfg.games != 1 || cfg.first_game != 0 || cfg.workers != 1 {
        return Err("s4a-utility-root requires games=1, first_game=0, workers=1".into());
    }
    let path = std::env::var("UTILITY_ROOT_SPEC").map_err(|_| "UTILITY_ROOT_SPEC is required")?;
    let pin = std::env::var("UTILITY_ROOT_SPEC_SHA256")
        .map_err(|_| "UTILITY_ROOT_SPEC_SHA256 is required")?;
    let bytes = std::fs::read(&path).map_err(|e| format!("{path}: {e}"))?;
    let req: Request = serde_json::from_slice(pinned_bytes(&bytes, &pin)?)
        .map_err(|e| format!("utility request: {e}"))?;
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
    let plain = read_record(&req.plain)?;
    let diagnostic = std::thread::scope(|scope| {
        std::thread::Builder::new()
            .stack_size(256 << 20)
            .spawn_scoped(scope, || {
                diagnose(&req, sh, policy, &search, &plain, &historical_binding)
            })
            .map_err(|e| e.to_string())?
            .join()
            .map_err(|_| "utility root worker panicked".to_owned())?
    })?;
    let row = json!({"kind":"utility_root","schema":"shared-root-utility-v1/receipt",
        "request_sha256":pin,"request":req,
        "historical_bank_binding_sha256":historical_binding,
        "current_bank_binding_sha256":current_binding,
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
        .map_err(|e| format!("fresh utility output {}: {e}", cfg.out))?;
    writeln!(output, "{row}").map_err(|e| e.to_string())?;
    output.sync_all().map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn case() -> SourceSpec {
        SourceSpec {
            id: "case".into(),
            own: 0,
            other: 1,
            seat: 0,
            opp: "fixture".into(),
            seed: 101,
            starting_player: 1,
            source_policy_seeds: [2, 3],
            anchor_draw: 5,
            guide_policy_seeds: [7, 11],
            search_root: "search-root".into(),
        }
    }

    fn row(src: &SourceSpec, plain: bool) -> Value {
        let mut r = json!({"kind":"budget_game","schema":"search-total-budget-v1/game",
            "lineage":"fixture", "bank_binding_sha256":"binding", "id":src.id,
            "own":src.own,"other":src.other,"seat":src.seat,"opp":src.opp,
            "seed":src.seed,"starting_player":src.starting_player,
            "live_policy_seeds":src.guide_policy_seeds,"search_root":src.search_root,
            "game":{"actions":[0,3,1]}});
        if plain {
            r["arm"] = json!("plain-v1");
        }
        r
    }

    #[test]
    fn exact_row_pin_rejects_malformed_changed_and_extra_row_bytes() {
        let bytes = b"{\"game\":{\"actions\":[0,3]}}\n";
        let pin = sha(bytes);
        assert!(pinned_bytes(bytes, &pin).is_ok());
        assert!(pinned_bytes(bytes, "bad").is_err());
        assert!(pinned_bytes(&bytes[..bytes.len() - 1], &pin).is_err());
        assert!(pinned_bytes(bytes, &"A".repeat(64)).is_err());
        let two_rows = [bytes.as_slice(), bytes.as_slice()].concat();
        assert!(
            serde_json::from_slice::<Value>(pinned_bytes(&two_rows, &sha(&two_rows)).unwrap())
                .is_err()
        );
    }

    #[test]
    fn rows_bind_schema_case_role_seeds_and_installed_bank() {
        let src = case();
        let original = row(&src, false);
        assert_eq!(
            row_actions(&original, &src, "fixture", "binding", false).unwrap(),
            vec![0, 3, 1]
        );
        for (field, value) in [
            ("schema", json!("wrong")),
            ("id", json!("other")),
            ("own", json!(1)),
            ("other", json!(0)),
            ("seat", json!(1)),
            ("opp", json!("unknown")),
            ("seed", json!(102)),
            ("starting_player", json!(0)),
            ("live_policy_seeds", json!([7, 12])),
            ("search_root", json!("other-root")),
            ("lineage", json!("other-model")),
            ("bank_binding_sha256", json!("other-binding")),
        ] {
            let mut r = original.clone();
            r[field] = value;
            assert!(
                row_actions(&r, &src, "fixture", "binding", false).is_err(),
                "{field}"
            );
        }
        let mut plain = row(&src, true);
        assert!(row_actions(&plain, &src, "fixture", "binding", true).is_ok());
        plain.as_object_mut().unwrap().remove("arm");
        assert!(row_actions(&plain, &src, "fixture", "binding", true).is_err());
        for arm in [json!(null), json!("plain-v1"), json!("unknown")] {
            let mut r = original.clone();
            r["arm"] = arm;
            assert!(row_actions(&r, &src, "fixture", "binding", false).is_err());
        }
        let mut explicit = original.clone();
        explicit["arm"] = json!("search-v1");
        assert!(row_actions(&explicit, &src, "fixture", "binding", false).is_ok());
    }

    #[test]
    fn first_divergence_requires_identical_full_prefix_and_both_root_actions() {
        let a = [0, 2, 0, 4];
        let b = [0, 2, 1, 3];
        assert_eq!(first_divergence(&a, &b, Root::FirstDivergence).unwrap(), 2);
        assert_eq!(first_divergence(&a, &b, Root::Step { step: 2 }).unwrap(), 2);
        for step in [0, 1, 3, u64::MAX] {
            assert!(first_divergence(&a, &b, Root::Step { step }).is_err());
        }
        assert!(first_divergence(&a, &a, Root::FirstDivergence).is_err());
        assert!(first_divergence(&a, &a[..2], Root::FirstDivergence).is_err());
        assert!(first_divergence(&[], &b, Root::FirstDivergence).is_err());
    }

    #[test]
    fn candidates_use_saved_executable_indices_and_reject_invalid_menus() {
        assert_eq!(candidates(&[0, 3], &[0, 1], 1, 4).unwrap(), (3, 1));
        for (a, b, index, width) in [
            (vec![0, 3], vec![0, 1], 1, 3),
            (vec![0], vec![0], 0, 4),
            (vec![1], vec![0], 0, 1),
            (vec![1], vec![], 0, 4),
        ] {
            assert!(candidates(&a, &b, index, width).is_err());
        }
    }

    #[test]
    fn first_divergence_requires_a_supported_search_chooser() {
        let plain = json!({"chooser":"plain"});
        for chooser in ["new_root", "reused"] {
            assert!(validate_choosers(&json!({"chooser":chooser}), &plain).is_ok());
        }
        for chooser in [json!(null), json!("plain"), json!("unsupported")] {
            assert!(validate_choosers(&json!({"chooser":chooser}), &plain).is_err());
        }
        assert!(validate_choosers(&json!({"chooser":"new_root"}), &json!({})).is_err());
    }

    #[test]
    fn historical_root_witness_rejects_same_width_menu_reordering_and_missing_fields() {
        let root = json!({"step":3,"physical_decision_id":2,"substep":0,"k":4,
            "obs_hash":"historical-observation","menu_hash":"historical-ordered-menu",
            "state_hash":"historical-state"});
        let row = json!({"game":{"searches":[{"search":0,"step":3,"root":root}]}});
        let decision = json!({"chooser":"new_root","search":0});
        assert!(validate_historical_root(&row, &decision, &root).is_ok());
        for key in [
            "step",
            "physical_decision_id",
            "substep",
            "k",
            "obs_hash",
            "menu_hash",
            "state_hash",
        ] {
            let mut changed = root.clone();
            changed[key] = if changed[key].is_string() {
                json!("changed")
            } else {
                json!(99)
            };
            assert!(
                validate_historical_root(&row, &decision, &changed).is_err(),
                "{key}"
            );
            let mut missing = row.clone();
            missing["game"]["searches"][0]["root"]
                .as_object_mut()
                .unwrap()
                .remove(key);
            assert!(
                validate_historical_root(&missing, &decision, &root).is_err(),
                "{key}"
            );
        }
        let mut reordered = root.clone();
        reordered["menu_hash"] = json!("same-four-actions-different-order");
        assert_eq!(reordered["k"], root["k"]);
        assert!(validate_historical_root(&row, &decision, &reordered).is_err());
        for invalid in [
            json!({"chooser":"reused","search":0}),
            json!({"chooser":"new_root","search":1}),
            json!({"chooser":"new_root"}),
        ] {
            assert!(validate_historical_root(&row, &invalid, &root).is_err());
        }
        let mut wrong_step = row.clone();
        wrong_step["game"]["searches"][0]["step"] = json!(2);
        assert!(validate_historical_root(&wrong_step, &decision, &root).is_err());
        let mut wrong_index = row.clone();
        wrong_index["game"]["searches"][0]["search"] = json!(1);
        assert!(validate_historical_root(&wrong_index, &decision, &root).is_err());
    }

    #[test]
    fn strict_caps_and_unresolved_score_bounds() {
        assert!(validate_cap(0).is_err());
        assert!(validate_cap(1).is_ok());
        assert!(validate_cap(MAX_TRANSITIONS).is_ok());
        assert!(validate_cap(MAX_TRANSITIONS + 1).is_err());
        assert_eq!(score_range(None), (-1, 1));
        assert_eq!(score_range(Some(0)), (0, 0));
        assert_eq!(score_range(Some(-1)), (-1, -1));
    }

    #[test]
    #[ignore = "native qualification: small fixture policy inference"]
    fn same_action_scientific_receipts_match_and_retain_episode_headroom() {
        use super::super::bank::{BankSpec, DeckSpec, SPEC_SCHEMA};
        use crate::runtime_decks::RUNTIME_DECKS;
        let spec = BankSpec {
            schema: SPEC_SCHEMA.into(),
            namespace: "spy-reset-pilot-v1".into(),
            lineage: "fixture".into(),
            runtime_rules: "resolution-boundary-v1".into(),
            select_rule: "fpu-1.5".into(),
            max_physical_decisions: 8,
            max_policy_steps: 12,
            decks: RUNTIME_DECKS
                .iter()
                .map(|d| DeckSpec {
                    label: d.id.into(),
                    mainboard: d.card_ids.to_vec(),
                })
                .collect(),
            sources: vec![],
        };
        let src = case();
        let opening = bank::source_session(&spec, &src).unwrap();
        let root = bank::walk(opening, &[0], |_, _, _| Ok(())).unwrap();
        let mut policy = FrozenPlayPolicyV1::training_fixture_v3();
        let mut opponent = policy.fork_for_collection_v3().unwrap();
        let seeds = ContinuationSeeds {
            focal: [13, 17],
            opponent: [19, 23],
        };
        let mut run = |cap| {
            branch(
                &root,
                0,
                PlayerId(src.seat),
                &mut policy,
                &mut opponent,
                &seeds,
                cap,
            )
            .unwrap()
        };
        let capped = run(1);
        assert_eq!(capped, run(1));
        assert_eq!(capped.transitions, 1);
        assert_eq!(capped.inference_calls, 0);
        assert_eq!(capped.end, "truncated");
        let full = run(32);
        assert_eq!(full, run(32));
        assert_eq!(full.end, "nonnatural");
        assert!(full.transitions < 32);
        assert!(full.final_policy_steps <= spec.max_policy_steps);
    }
}
