//! Bounded recorded-action reconstruction. No model or sampler is instantiated.
use crate::flat_policy_v2::*;
use crate::flat_policy_v4::{FlatDecisionEncoderV4, FlatScoringDecisionViewV4};
use crate::human_bo3_v1::{HumanActionRequestV1, HumanDecisionProjectorV1};
use crate::human_opening_v1::HumanOpeningV1;
use crate::ids::PlayerId;
use crate::native_flat_tensorizer_v4::*;
use crate::rl::PlayerSeatV1;
use crate::rl_session::{FastActorResponseV1, FastActorSessionV1};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

const LIMITS: [(&str, u64); 7] = [
    ("human", 292),
    ("case0-g115-seat0", 201),
    ("case0-g115-seat1", 114),
    ("case1-g115-seat0", 8),
    ("case1-g115-seat1", 11),
    ("case2-g115-seat1", 23),
    ("natural-burn", 65),
];
const CASES: [&str; 22] = [
    "H107", "H262", "H271", "H273", "H292", "G004", "G019", "G114", "P036", "P024", "P026", "P115",
    "P117", "N113", "C110", "C255", "C265", "C004", "C204", "C011", "C014", "C026",
];
const MAX_INPUT: u64 = 64 * 1024 * 1024;
const MAX_OUTPUT: usize = 128 * 1024 * 1024;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct Pin {
    path: PathBuf,
    sha256: String,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct Prefix {
    id: String,
    source_pin: Pin,
    human_responses: Option<Pin>,
    episode_id: u64,
    environment_seed: u64,
    deck_ids: [String; 2],
    mainboards: [Vec<u16>; 2],
    starting_player: u8,
    human_seat: u8,
    opening: Vec<Value>,
    opening_inputs: Vec<Value>,
    action_ceiling: u64,
    decisions: Vec<Value>,
    targets: Vec<Value>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    schema: String,
    mode: String,
    preparer: Pin,
    source_pins: Vec<Pin>,
    prefixes: Vec<Prefix>,
    max_opening_responses: u64,
    #[serde(default)]
    duplicate_of: Option<Pin>,
    #[serde(default)]
    primary_report: Option<Pin>,
}
fn ensure(ok: bool, message: &str) -> Result<(), String> {
    if ok {
        Ok(())
    } else {
        Err(message.into())
    }
}
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn bytes(value: &Value) -> Result<Vec<u8>, String> {
    serde_json::to_vec(value).map_err(|e| e.to_string())
}
fn sha(value: &Value) -> Result<String, String> {
    Ok(hash(&bytes(value)?))
}
fn value<T: Serialize>(item: T) -> Result<Value, String> {
    serde_json::to_value(item).map_err(|e| e.to_string())
}
fn read(path: &Path) -> Result<Vec<u8>, String> {
    ensure(
        fs::metadata(path).map_err(|e| e.to_string())?.len() <= MAX_INPUT,
        "input exceeds64MiB",
    )?;
    fs::read(path).map_err(|e| e.to_string())
}
fn pinned(pin: &Pin) -> Result<Vec<u8>, String> {
    ensure(pin.path.is_absolute(), "source path must be absolute")?;
    let result = read(&pin.path)?;
    ensure(hash(&result) == pin.sha256, "pinned input hash mismatch")?;
    Ok(result)
}
fn field_u64(v: &Value, key: &str) -> Result<u64, String> {
    v[key]
        .as_u64()
        .ok_or_else(|| format!("missing integer {key}"))
}
fn actor(v: &Value) -> Result<PlayerSeatV1, String> {
    serde_json::from_value(v.clone()).map_err(|e| e.to_string())
}

fn validate(request: &Request) -> Result<(), String> {
    ensure(
        request.schema == "gameplay-checkpoint-reconstruction/v1",
        "request schema differs",
    )?;
    ensure(
        request.max_opening_responses == 32,
        "opening ceiling differs",
    )?;
    ensure(
        !request.source_pins.is_empty() && request.source_pins.len() <= 16,
        "source pin count",
    )?;
    pinned(&request.preparer)?;
    for pin in &request.source_pins {
        pinned(pin)?;
    }
    let mut ids = BTreeSet::new();
    let mut cases = BTreeSet::new();
    for prefix in &request.prefixes {
        ensure(
            request.source_pins.contains(&prefix.source_pin),
            "prefix source not pinned in request",
        )?;
        if let Some(pin) = &prefix.human_responses {
            ensure(
                request.source_pins.contains(pin),
                "human responses not pinned in request",
            )?;
        }
        validate_prefix_sources(prefix)?;
        ensure(ids.insert(prefix.id.as_str()), "duplicate prefix")?;
        ensure(
            LIMITS.contains(&(prefix.id.as_str(), prefix.action_ceiling)),
            "prefix/action ceiling differs",
        )?;
        ensure(
            prefix.decisions.len() as u64 == prefix.action_ceiling + 1,
            "prefix records incomplete",
        )?;
        ensure(
            prefix.starting_player <= 1 && prefix.human_seat <= 1,
            "invalid opening seat",
        )?;
        ensure(
            !prefix.opening.is_empty() && prefix.opening.len() <= 6,
            "opening action count",
        )?;
        ensure(
            prefix.mainboards.iter().all(|d| d.len() == 60),
            "expected60-card ordered mainboards",
        )?;
        for (step, decision) in prefix.decisions.iter().enumerate() {
            ensure(
                field_u64(decision, "step")? == step as u64,
                "noncontiguous source timeline",
            )?;
            actor(&decision["actor"])?;
        }
        for target in &prefix.targets {
            let id = target["case_id"].as_str().ok_or("missing case id")?;
            ensure(
                CASES.contains(&id) && cases.insert(id),
                "invalid/duplicate case",
            )?;
            let step = field_u64(target, "step")?;
            ensure(step <= prefix.action_ceiling, "target after prefix bound")?;
            ensure(
                target["actor"] == prefix.decisions[step as usize]["actor"],
                "target actor mismatch",
            )?;
        }
    }
    match request.mode.as_str() {
        "primary" => {
            ensure(
                request.duplicate_of.is_none() && request.primary_report.is_none(),
                "primary has duplicate metadata",
            )?;
            ensure(
                request.prefixes.len() == 7 && cases.len() == 22,
                "primary must include all7 prefixes/all22 cases",
            )
        }
        "duplicate" => {
            let original_pin = request
                .duplicate_of
                .as_ref()
                .ok_or("missing original request pin")?;
            let original: Request =
                serde_json::from_slice(&pinned(original_pin)?).map_err(|e| e.to_string())?;
            ensure(
                original.mode == "primary",
                "duplicate of nonprimary request",
            )?;
            let report: Value = serde_json::from_slice(&pinned(
                request
                    .primary_report
                    .as_ref()
                    .ok_or("missing primary report")?,
            )?)
            .map_err(|e| e.to_string())?;
            ensure(
                report["complete"] == true && report["input_sha256"] == original_pin.sha256,
                "primary completion/request mismatch",
            )?;
            let completed = report["prefixes"]
                .as_array()
                .ok_or("missing primary prefixes")?;
            let mut eligible: Vec<_> = original
                .prefixes
                .iter()
                .filter(|p| {
                    completed
                        .iter()
                        .any(|r| r["id"] == p.id && r["full_prefix_success"] == true)
                })
                .collect();
            eligible.sort_by_key(|p| (p.action_ceiling, &p.id));
            let shortest = eligible
                .first()
                .ok_or("no fully successful prefix to duplicate")?;
            ensure(
                request.prefixes.len() == 1 && &request.prefixes[0] == *shortest,
                "duplicate differs from shortest successful original prefix",
            )?;
            ensure(
                request.source_pins == original.source_pins
                    && request.preparer == original.preparer,
                "duplicate source identity differs",
            )
        }
        _ => Err("unknown reconstruction mode".into()),
    }
}

fn json_lines(raw: &[u8]) -> Result<Vec<Value>, String> {
    std::str::from_utf8(raw)
        .map_err(|e| e.to_string())?
        .lines()
        .map(|line| serde_json::from_str(line).map_err(|e| e.to_string()))
        .collect()
}

/// Bind prepared actions back to the actual immutable originals, independently
/// of the Python preparer. Probability/terminal fields are never consulted.
fn validate_prefix_sources(prefix: &Prefix) -> Result<(), String> {
    let raw = pinned(&prefix.source_pin)?;
    let (originals, setup, config, responses) = if prefix.id == "human" {
        let lines = json_lines(&raw)?;
        let config = lines.first().ok_or("empty human journal")?["payload"]["config"].clone();
        let start = lines
            .iter()
            .find(|r| r["event"] == "game_start")
            .ok_or("missing human game start")?["payload"]
            .clone();
        let originals: Vec<_> = lines
            .iter()
            .enumerate()
            .filter(|(_, r)| {
                r["event"] == "offline_model_decision"
                    || (r["event"] == "human_command" && r["payload"]["command"] == "action")
            })
            .map(|(i, r)| (format!("line {} /payload", i + 1), r["payload"].clone()))
            .collect();
        let response_pin = prefix
            .human_responses
            .as_ref()
            .ok_or("missing human responses pin")?;
        let responses = json_lines(&pinned(response_pin)?)?;
        let source_opening: Vec<_> = lines
            .iter()
            .filter(|r| {
                r["event"] == "human_command"
                    && matches!(
                        r["payload"]["command"].as_str(),
                        Some("mulligan" | "keep" | "bottom")
                    )
            })
            .map(|r| r["payload"].clone())
            .collect();
        ensure(
            source_opening.len() == prefix.opening.len(),
            "prepared human opening count differs",
        )?;
        for (source, prepared) in source_opening.iter().zip(&prefix.opening) {
            ensure(
                source["command"] == prepared["command"]
                    && (source["command"] != "bottom"
                        || source["hand_indices"] == prepared["hand_indices"]),
                "prepared human opening action differs",
            )?;
            ensure(
                responses.iter().any(|r| {
                    r["view"]["opening_revision"] == source["opening_revision"]
                        && r["view"]["opening"] == prepared["expected_view"]
                }),
                "opening response differs from original",
            )?;
        }
        (originals, start, config, responses)
    } else {
        ensure(
            prefix.human_responses.is_none(),
            "BO3 prefix has human responses",
        )?;
        let document: Value = serde_json::from_slice(&raw).map_err(|e| e.to_string())?;
        let collection = if prefix.id == "natural-burn" {
            &document["collection"]
        } else {
            &document
        };
        let index = if prefix.id == "natural-burn" { 1 } else { 0 };
        let game = &collection["collected"]["trajectory"]["games"][index];
        let rows = game["decisions"]
            .as_array()
            .ok_or("original decisions missing")?;
        let originals = rows
            .iter()
            .enumerate()
            .filter(|(_, r)| r["visible"]["kind"] == "gameplay")
            .map(|(i, r)| {
                (
                    format!(
                        "{}/collected/trajectory/games/{index}/decisions/{i}",
                        if prefix.id == "natural-burn" {
                            "/collection"
                        } else {
                            ""
                        }
                    ),
                    r.clone(),
                )
            })
            .collect();
        let opening_inputs: Vec<_> = rows
            .iter()
            .filter(|r| r["visible"]["kind"] == "mulligan")
            .map(|r| json!({"actor":r["actor"],"input":r["visible"]["input"]}))
            .collect();
        ensure(
            opening_inputs == prefix.opening_inputs
                && prefix.opening.len() == 1
                && prefix.opening[0]["command"] == "keep",
            "prepared BO3 opening differs",
        )?;
        let setup = json!({"start":game["start"],"environment_seed":collection["collected"]["games"][index]["environment_seed"],
            "configurations":collection["config"]["registrations"]});
        (originals, setup, collection["config"].clone(), Vec::new())
    };
    ensure(
        setup["environment_seed"] == prefix.environment_seed
            && setup["start"]["game_index"] == prefix.episode_id
            && setup["start"]["starting_player"] == prefix.starting_player,
        "prepared game setup differs",
    )?;
    let configurations = setup["configurations"]
        .as_array()
        .ok_or("source configurations missing")?;
    for seat in 0..2 {
        ensure(
            configurations[seat]["mainboard"] == value(&prefix.mainboards[seat])?,
            "prepared ordered mainboard differs",
        )?;
        let deck = if prefix.id == "human" {
            &config["registered"][seat]["label"]
        } else {
            &config["deck_ids"][seat]
        };
        ensure(
            *deck == prefix.deck_ids[seat],
            "prepared deck label differs",
        )?;
    }
    ensure(
        originals.len() >= prefix.decisions.len(),
        "original prefix too short",
    )?;
    for ((pointer, source), prepared) in originals.iter().zip(&prefix.decisions) {
        ensure(
            prepared["source_pointer"] == *pointer,
            "source pointer/order differs",
        )?;
        if prefix.id == "human" && source["command"] == "action" {
            ensure(
                prepared["human_action_index"] == source["action_index"],
                "prepared human action differs",
            )?;
            ensure(
                responses.iter().any(|r| {
                    r["view"]["decision"]["prompt_seq"] == source["prompt_seq"]
                        && r["view"]["decision"] == prepared["human_decision"]
                }),
                "human prompt differs from original",
            )?;
        } else {
            let (observation, actions, selected) = if prefix.id == "human" {
                (
                    &source["observation"],
                    &source["actions"],
                    &source["selected_engine_index"],
                )
            } else {
                (
                    &source["visible"]["observation"],
                    &source["visible"]["ordered_actions"],
                    &source["behavior"]["selected_index"],
                )
            };
            ensure(
                prepared["observation"] == *observation
                    && prepared["ordered_actions"] == *actions
                    && prepared["selected_index"] == *selected
                    && prepared["actor"] == source["actor"],
                "prepared model action differs from original",
            )?;
        }
    }
    Ok(())
}

// Permit precisely the existing end-of-combat clearing repair. Never drop fields.
fn combat_reset(old: &Value, new: &Value, phase: &Value) -> bool {
    if !matches!(
        phase.as_str(),
        Some("main1" | "main2" | "end" | "cleanup" | "untap" | "upkeep" | "draw")
    ) {
        return false;
    }
    if !old.is_object() || !new.is_object() {
        return false;
    }
    let mut expected = old.clone();
    for key in ["ordered_attackers", "attacker_to_ordered_blockers"] {
        if !old[key].is_array() {
            return false;
        }
        expected[key] = json!([]);
    }
    for key in ["attackers_declared", "blockers_declared"] {
        if !old[key].is_boolean() {
            return false;
        }
        expected[key] = json!(false);
    }
    expected == *new && old != new
}
fn differences(old: &Value, new: &Value, human: bool) -> Result<Vec<Value>, String> {
    let combat_path = if human {
        "/state/public/combat"
    } else {
        "/projection/combat"
    };
    let phase_path = if human {
        "/state/public/phase"
    } else {
        "/projection/phase"
    };
    let mut a = old.clone();
    let mut allowed = Vec::new();
    if let (Some(x), Some(y), Some(phase)) = (
        old.pointer(combat_path),
        new.pointer(combat_path),
        new.pointer(phase_path),
    ) {
        if combat_reset(x, y, phase) {
            *a.pointer_mut(combat_path).ok_or("combat path missing")? = y.clone();
            allowed.push(json!({"path":combat_path,"old":x,"new":y,"reason":"07ad87e4 clears combat after EndCombat"}));
        }
    }
    if human {
        ensure(
            old["prompt_seq"].is_u64() && new["prompt_seq"].is_u64(),
            "public prompt sequence missing",
        )?;
        a["prompt_seq"] = new["prompt_seq"].clone();
    } else if !allowed.is_empty() && a["visible_projection_hash"] != new["visible_projection_hash"]
    {
        allowed.push(json!({"path":"/visible_projection_hash","old":a["visible_projection_hash"],
            "new":new["visible_projection_hash"],"reason":"hash of explicitly corrected projection"}));
        a["visible_projection_hash"] = new["visible_projection_hash"].clone();
    }
    if a != *new {
        return Err(format!(
            "non-whitelisted state discrepancy at {}",
            first_difference(&a, new, "")
        ));
    }
    Ok(allowed)
}
fn first_difference(a: &Value, b: &Value, path: &str) -> String {
    if let (Some(a), Some(b)) = (a.as_object(), b.as_object()) {
        let keys: BTreeSet<_> = a.keys().chain(b.keys()).collect();
        for key in keys {
            if a.get(key) != b.get(key) {
                let p = format!("{path}/{}", key.replace('~', "~0").replace('/', "~1"));
                return match (a.get(key), b.get(key)) {
                    (Some(x), Some(y)) => first_difference(x, y, &p),
                    _ => p,
                };
            }
        }
    } else if let (Some(a), Some(b)) = (a.as_array(), b.as_array()) {
        for i in 0..a.len().max(b.len()) {
            if a.get(i) != b.get(i) {
                return match (a.get(i), b.get(i)) {
                    (Some(x), Some(y)) => first_difference(x, y, &format!("{path}/{i}")),
                    _ => format!("{path}/{i}"),
                };
            }
        }
    }
    path.into()
}

#[derive(Default)]
struct Buffers {
    objects: Vec<FlatObjectCoreV2>,
    relations: Vec<FlatRelationV2>,
    subtypes: Vec<FlatObjectSubtypeV2>,
    uses: Vec<FlatObjectAbilityUseV2>,
    goads: Vec<FlatObjectGoadV2>,
    dungeons: Vec<FlatCompletedDungeonV2>,
    subtype_changes: Vec<FlatEffectSubtypeChangeV2>,
    contexts: Vec<FlatContextPathElementV2>,
    actions: Vec<FlatScorerActionCoreV2>,
    refs: Vec<FlatScorerActionRefV2>,
}
fn capture(session: &FastActorSessionV1) -> Result<(Value, Value), String> {
    let FastActorResponseV1::Decision(current) = session.current_response() else {
        return Err("target is terminal".into());
    };
    let mut b = Buffers::default();
    let encoded = session
        .encode_current_flat_scoring_decision_owned_v4(
            current,
            &mut FlatDecisionEncoderV4::default(),
            &mut FlatScoringOwnedBuffersV2 {
                objects: &mut b.objects,
                relations: &mut b.relations,
                object_subtypes: &mut b.subtypes,
                ability_uses: &mut b.uses,
                goads: &mut b.goads,
                completed_dungeons: &mut b.dungeons,
                effect_subtype_changes: &mut b.subtype_changes,
                context_path_elements: &mut b.contexts,
                actions: &mut b.actions,
                action_refs: &mut b.refs,
            },
        )
        .map_err(|e| format!("live V4 encode: {e:?}"))?;
    let common = FlatScoringDecisionViewV2::new(
        &encoded.globals,
        &b.objects,
        &b.relations,
        &b.subtypes,
        &b.uses,
        &b.goads,
        &b.dungeons,
        &b.subtype_changes,
        &b.contexts,
        &b.actions,
        &b.refs,
    );
    let mut tensor = NativeFlatDecisionTensorV4::default();
    NativeFlatTensorizerV4::default()
        .fill(
            FlatScoringDecisionViewV4::new(common, &encoded.extensions),
            &mut tensor,
        )
        .map_err(|e| format!("live V4 tensor: {e:?}"))?;
    let (_, _, bound) = session
        .human_current_decision_input_v4(current, current.acting_player)
        .map_err(|e| format!("target binding: {e:?}"))?;
    ensure(bound == encoded.binding, "visible/scorer binding differs")?;
    let b = bound.0;
    let binding = json!({"slice_version":b.slice_version,"ref_role_mapping_version":b.ref_role_mapping_version,
        "card_token_mapping_version":b.card_token_mapping_version,"candidate_commitment_version":b.candidate_commitment_version,
        "card_db_hash":b.card_db_hash,"episode_id":b.episode_id,"environment_revision":b.environment_revision,
        "bound_policy_step_count":b.bound_policy_step_count,"physical_decision_id":b.physical_decision_id,
        "bound_physical_decision_count":b.bound_physical_decision_count,"substep_index":b.substep_index,
        "substep_count":b.substep_count,"acting_player":b.acting_player,"decision_kind":b.decision_kind,
        "legal_action_count":b.legal_action_count,"candidate_order_commitment":b.candidate_order_commitment});
    Ok((
        crate::gameplay_trace_v1::tensor_value(&tensor.common),
        binding,
    ))
}

struct Publisher {
    root: PathBuf,
    written: usize,
}
impl Publisher {
    fn save(&mut self, name: &str, value: &Value) -> Result<String, String> {
        ensure(
            !name.contains('/') && !name.contains('\\') && !name.contains(".."),
            "invalid output name",
        )?;
        let body = bytes(value)?;
        ensure(
            body.len() <= 8 * 1024 * 1024 && self.written + body.len() <= MAX_OUTPUT,
            "output byte ceiling exceeded",
        )?;
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(self.root.join(name))
            .map_err(|e| e.to_string())?;
        file.write_all(&body)
            .and_then(|_| file.sync_all())
            .map_err(|e| e.to_string())?;
        self.written += body.len();
        Ok(hash(&body))
    }
}
fn setup(prefix: &Prefix, opening_count: &mut u64) -> Result<FastActorSessionV1, String> {
    let mut opening = HumanOpeningV1::new(
        prefix.episode_id,
        prefix.environment_seed,
        100000,
        200000,
        prefix.deck_ids.clone(),
        prefix.mainboards.clone(),
        PlayerId(prefix.starting_player),
        PlayerId(prefix.human_seat),
    )?;
    for original in &prefix.opening_inputs {
        *opening_count += 1;
        ensure(*opening_count <= 32, "opening response ceiling exceeded")?;
        let seat = actor(&original["actor"])?;
        let expected = &original["input"];
        let actual = if seat == PlayerSeatV1::P1 {
            opening.automatic_keep_seven_view_v1()?
        } else {
            opening.view()
        };
        let hand: Vec<_> = actual.hand.iter().map(|c| c.card_id).collect();
        ensure(
            value(hand)? == expected["own_hand"]
                && value(actual.starting_player)? == expected["starting_player"]
                && json!(actual.mulligans_taken) == expected["mulligans_taken"]
                && json!(actual.required_bottom) == expected["remaining_bottom"]
                && json!(actual.opponent_hand_count) == expected["opponent_hand_count"]
                && json!(actual.opponent_has_kept) == expected["opponent_has_kept"],
            "BO3 opening hand/context differs",
        )?;
    }
    for action in &prefix.opening {
        *opening_count += 1;
        ensure(*opening_count <= 32, "opening response ceiling exceeded")?;
        if !action["expected_view"].is_null() {
            ensure(
                value(opening.view())? == action["expected_view"],
                "historical opening view differs",
            )?;
        }
        match action["command"].as_str() {
            Some("mulligan") => opening.mulligan()?,
            Some("keep") => opening.keep()?,
            Some("bottom") => opening.bottom(
                &serde_json::from_value::<Vec<u32>>(action["hand_indices"].clone())
                    .map_err(|e| e.to_string())?,
            )?,
            _ => return Err("unsupported opening action".into()),
        }
    }
    opening.into_session()
}

fn prefix_run(
    prefix: &Prefix,
    runtime: &Value,
    output: &mut Publisher,
    opening_count: &mut u64,
) -> Result<Value, String> {
    let mut cases = Vec::new();
    let mut transitions = Vec::new();
    let mut applied = 0_u64;
    let result = (|| -> Result<(), String> {
        let mut session = setup(prefix, opening_count)?;
        let mut human = HumanDecisionProjectorV1::new_v4(if prefix.human_seat == 0 {
            PlayerSeatV1::P0
        } else {
            PlayerSeatV1::P1
        });
        for row in &prefix.decisions {
            let FastActorResponseV1::Decision(current) = session.current_response() else {
                return Err("prefix terminated before target".into());
            };
            ensure(
                current.step == field_u64(row, "step")?
                    && current.acting_player == actor(&row["actor"])?,
                "actor/step binding differs",
            )?;
            let (obs, menu, _) = session
                .human_current_decision_input_v4(current, current.acting_player)
                .map_err(|e| format!("live menu: {e:?}"))?;
            let observation = value(obs)?;
            let actions = value(menu)?;
            let is_human = !row["human_decision"].is_null();
            let (diffs, public_prompt) = if is_human {
                let projected = value(
                    human
                        .project_current(&session, current)
                        .map_err(|e| e.to_string())?,
                )?;
                let diffs = differences(&row["human_decision"], &projected, true)?;
                (diffs, Some(projected))
            } else {
                ensure(
                    actions == row["ordered_actions"],
                    "ordered semantic menu differs",
                )?;
                (differences(&row["observation"], &observation, false)?, None)
            };
            for target in prefix.targets.iter().filter(|t| t["step"] == current.step) {
                let id = target["case_id"].as_str().ok_or("case id missing")?;
                let (input, binding) = capture(&session)?;
                let status = if diffs.is_empty() {
                    "exact"
                } else {
                    "corrected"
                };
                let record = json!({"schema":"gameplay-corrected-input/v1","case_id":id,
                    "origin":target["origin"],"status":status,"observation":observation,"ordered_actions":actions,
                    "semantic_menu_sha256":sha(&actions)?,"binding":binding,"encoded_input_sha256":sha(&input)?,"encoded_input":input,
                    "feature_contract_digest":FEATURE_CONTRACT_DIGEST_V4,"feature_encoding_digest":FEATURE_ENCODING_DIGEST_V4,
                    "card_db_hash":observation["card_db_hash"],"runtime":runtime,"differences":diffs});
                let filename = format!("{}-{id}.json", prefix.id);
                let digest = output.save(&filename, &record)?;
                cases.push(json!({"case_id":id,"status":status,"input_file":filename,"input_sha256":digest}));
            }
            if current.step == prefix.action_ceiling {
                break;
            }
            let selection = if let Some(projected) = public_prompt {
                let public_index = u32::try_from(field_u64(row, "human_action_index")?)
                    .map_err(|e| e.to_string())?;
                human
                    .submit(
                        &mut session,
                        HumanActionRequestV1 {
                            prompt_seq: field_u64(&projected, "prompt_seq")?,
                            action_index: public_index,
                        },
                    )
                    .map_err(|e| e.to_string())?;
                json!({"public_index":public_index,"public_menu_sha256":sha(&projected["actions"])?})
            } else {
                let index =
                    u32::try_from(field_u64(row, "selected_index")?).map_err(|e| e.to_string())?;
                ensure(
                    (index as usize) < actions.as_array().ok_or("menu missing")?.len(),
                    "selected index outside bound menu",
                )?;
                session
                    .step(current.episode_id, current.step, index)
                    .map_err(|e| e.to_string())?;
                json!({"engine_index":index,"semantic":actions[index as usize]})
            };
            applied += 1;
            transitions.push(json!({"step":current.step,"actor":row["actor"],"source_pointer":row["source_pointer"],
                "observation_sha256":sha(&observation)?,"semantic_menu_sha256":sha(&actions)?,"selection":selection,"differences":diffs}));
        }
        ensure(
            applied == prefix.action_ceiling,
            "prefix action count differs",
        )
    })();
    for target in &prefix.targets {
        if !cases.iter().any(|c| c["case_id"] == target["case_id"]) {
            cases.push(json!({"case_id":target["case_id"],"status":"unreconstructable","reason":result.as_ref().err()}));
        }
    }
    cases.sort_by_key(|c| c["case_id"].as_str().unwrap_or("").to_owned());
    let record = json!({"schema":"gameplay-prefix-reconstruction/v1","id":prefix.id,
        "full_prefix_success":result.is_ok(),"action_ceiling":prefix.action_ceiling,"applied_gameplay_actions":applied,
        "cases":cases,"transitions":transitions,"failure":result.err(),"runtime":runtime});
    let name = format!("{}.json", prefix.id);
    let digest = output.save(&name, &record)?;
    Ok(
        json!({"id":prefix.id,"full_prefix_success":record["full_prefix_success"],"applied_gameplay_actions":applied,
        "action_ceiling":prefix.action_ceiling,"cases":record["cases"],"failure":record["failure"],
        "primary_file":{"path":output.root.join(name),"sha256":digest}}),
    )
}

pub fn run() -> Result<(), String> {
    let args: Vec<_> = std::env::args_os().collect();
    ensure(
        args.len() == 3,
        "usage: gameplay_checkpoint_reconstruction_v1 REQUEST.json NEW_OUTPUT_DIR",
    )?;
    let input = read(Path::new(&args[1]))?;
    let request: Request = serde_json::from_slice(&input).map_err(|e| e.to_string())?;
    validate(&request)?;
    let root = PathBuf::from(&args[2]);
    ensure(
        root.is_absolute() && !root.exists(),
        "output must be fresh absolute path",
    )?;
    fs::create_dir(&root).map_err(|e| e.to_string())?;
    let runtime = json!({"binary_sha256":hash(&fs::read(std::env::current_exe().map_err(|e|e.to_string())?).map_err(|e|e.to_string())?),
        "engine_commit":env!("MTG_KERNEL_BUILD_GIT_HEAD"),"tracked_tree_sha256":env!("MTG_KERNEL_BUILD_TRACKED_TREE_SHA256"),
        "tracked_tree_contract":env!("MTG_KERNEL_BUILD_TRACKED_TREE_CONTRACT"),"build_git_clean":env!("MTG_KERNEL_BUILD_GIT_CLEAN"),
        "feature":"gameplay-checkpoint-reconstruction-v1","model_forward_calls":0,"sampler_draws":0});
    let mut output = Publisher { root, written: 0 };
    let mut openings = 0;
    let mut reports = Vec::new();
    for prefix in &request.prefixes {
        reports.push(prefix_run(prefix, &runtime, &mut output, &mut openings)?);
    }
    let applied: u64 = reports
        .iter()
        .filter_map(|r| r["applied_gameplay_actions"].as_u64())
        .sum();
    let report = json!({"schema":"gameplay-checkpoint-reconstruction-result/v1","complete":true,
        "meaning":"bounded attempt complete; inspect every case status","mode":request.mode,"input_sha256":hash(&input),
        "model_forward_calls":0,"sampler_draws":0,"applied_gameplay_actions":applied,"opening_responses":openings,
        "prefixes":reports,"runtime":runtime});
    output.save("report.json", &report)?;
    println!(
        "{}",
        json!({"complete":true,"applied_gameplay_actions":applied,"opening_responses":openings,"model_forward_calls":0})
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn obs(phase: &str) -> Value {
        json!({"projection":{"phase":phase,"combat":{"ordered_attackers":[{"id":3}],
        "attacker_to_ordered_blockers":[],"attackers_declared":true,"blockers_declared":false}},"visible_projection_hash":1,"life":20})
    }
    fn cleared(mut x: Value) -> Value {
        x["projection"]["combat"] = json!({"ordered_attackers":[],"attacker_to_ordered_blockers":[],
        "attackers_declared":false,"blockers_declared":false});
        x["visible_projection_hash"] = json!(2);
        x
    }
    #[test]
    fn only_known_noncombat_reset_is_allowed() {
        let old = obs("main1");
        let new = cleared(old.clone());
        assert_eq!(differences(&old, &new, false).unwrap().len(), 2);
        let combat = obs("declare_attackers");
        assert!(differences(&combat, &cleared(combat.clone()), false).is_err());
        let mut changed = new.clone();
        changed["life"] = json!(19);
        assert!(differences(&old, &changed, false).is_err());
        let mut changed = old.clone();
        changed["visible_projection_hash"] = json!(9);
        assert!(differences(&old, &changed, false).is_err());
    }
    #[test]
    fn public_sequence_normalization_does_not_ignore_menu_or_state() {
        let a = json!({"prompt_seq":12,"actions":[{"action_index":0,"label":"Pass"}],"state":{"life":20}});
        let mut b = a.clone();
        b["prompt_seq"] = json!(1);
        assert!(differences(&a, &b, true).unwrap().is_empty());
        b["actions"][0]["label"] = json!("Sacrifice");
        assert!(differences(&a, &b, true).is_err());
    }
    #[test]
    fn recursive_mismatch_preserves_array_order_and_missing_fields() {
        assert_eq!(
            first_difference(&json!({"a":[1,2]}), &json!({"a":[2,1]}), ""),
            "/a/0"
        );
        assert_eq!(first_difference(&json!({"a":null}), &json!({}), ""), "/a");
        assert_eq!(LIMITS.iter().map(|x| x.1).sum::<u64>(), 714);
    }
}
