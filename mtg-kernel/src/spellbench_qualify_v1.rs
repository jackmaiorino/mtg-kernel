//! Qualification for Spellbench kernel bots: replays each logged game and
//! proves, decision by decision, that the bridge gave the bot the evaluation
//! harness's input (tensor), that the bot's scorer produced the harness's
//! logits, and that the bot chose the harness's row with the same seat stream.
//! The harness is `FrozenPlayPolicyV1` scoring its own flat-action V3
//! `FastActorSessionV1`, the evaluation path of the Phase 1 lineage.

use crate::agent_bridge_flat_v4_encode::tensor_bits_v1;
use crate::agent_bridge_v1::{
    seat_str_v1, spellbench_episode_id_v1, SpellbenchBridgeOptionsV1, SpellbenchBridgeServerV1,
    SpellbenchEngineIdentityV1, SPELLBENCH_FORMAT_PAUPER_BO1_V1,
};
use crate::expanded_deck_training_v1::load_expanded_inference_v1;
use crate::rl::TerminalClassificationV1;
use crate::rl_session::{FastActorResponseV1, FastActorSessionV1};
use crate::sideboard_play_policy_v1::FrozenPlayPolicyV1;
use crate::spellbench_scorer_v1::{
    SpellbenchScorerConfigV1, SELECTION_ARGMAX_FIRST_V1, SELECTION_SAMPLED_WIDE_V1,
};
use crate::state::SplitMix64;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::hash_map::Entry;
use std::collections::{BTreeMap, HashMap};
use std::io::Write;
use std::path::{Path, PathBuf};

pub const QUALIFICATION_REPORT_SCHEMA_V1: &str = "mtg-kernel-spellbench-qualification/v1";
pub const BOT_LOG_SCHEMA_V1: &str = "spellbench-kernel-bot-log/v1";

const SEATS_V1: [&str; 2] = ["p0", "p1"];

// Every field is part of the checked wire even where the replay ignores it.
#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct BotLogHeaderV1 {
    schema: String,
    kind: String,
    game_id: String,
    seat: String,
    bot_name: String,
    bot_version: String,
    selection: String,
    stream_seed: u64,
    model_state_sha256: String,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct BotLogDecisionV1 {
    kind: String,
    step: u64,
    candidate_id: u32,
    selected_row: u32,
    sample_seed: Option<u64>,
    logits_bits: Vec<u32>,
    value_bits: u32,
    request_sha256: String,
    elapsed_us: u64,
}

#[derive(Clone)]
pub(crate) struct BotLogV1 {
    header: BotLogHeaderV1,
    decisions: Vec<BotLogDecisionV1>,
}

#[derive(Clone)]
pub(crate) struct LedgerGameV1 {
    game_id: String,
    game_seed: u64,
    decks: [String; 2],
    bot_names: [String; 2],
    classification: String,
    outcome: String,
    step_count: u64,
    /// The terminal's reason and physical decision count.
    reason: String,
    decision_count: u64,
    /// The engine identity the tournament recorded.
    engine: Value,
}

#[derive(Debug, Default)]
pub(crate) struct QualificationCountsV1 {
    games_replayed: u64,
    games_skipped: u64,
    /// Games no `--bot` bot played (builtins only): nothing to qualify.
    games_not_applicable: u64,
    decisions: u64,
    tensor_mismatches: u64,
    logit_mismatches: u64,
    choice_mismatches: u64,
    outcome_mismatches: u64,
    per_deck: BTreeMap<String, (u64, u64)>,
    /// Each replayed game's first mismatch, for the diagnostics stream.
    notes: Vec<String>,
}

/// What a `--bot` config pins: every log of that bot must carry this model
/// state and this selection rule, and the harness recomputes under the rule.
#[derive(Debug, Clone)]
pub(crate) struct PinnedBotV1 {
    model_state_sha256: String,
    selection: &'static str,
}

fn replay_server() -> SpellbenchBridgeServerV1 {
    SpellbenchBridgeServerV1::with_options(
        SpellbenchEngineIdentityV1::with_source_revision(None),
        SpellbenchBridgeOptionsV1 {
            x_kernel_flat_v4: true,
            ..SpellbenchBridgeOptionsV1::default()
        },
    )
}

fn reset_line(
    game_id: &str,
    decks: [&str; 2],
    seed: u64,
    limits: (u64, u64),
    request_id: &str,
) -> String {
    json!({"request_type": "reset", "protocol": "spellbench/v1", "request_id": request_id, "game_id": game_id,
        "format": "pauper-bo1",
        "seats": [{"seat": "p0", "deck": {"catalog_id": decks[0]}}, {"seat": "p1", "deck": {"catalog_id": decks[1]}}],
        "game_seed": seed, "max_decisions": limits.0, "max_steps": limits.1}).to_string()
}

fn step_line(decision: &Value, candidate_id: u32, request_id: &str) -> String {
    json!({"request_type": "step", "protocol": "spellbench/v1", "request_id": request_id,
        "game_id": decision["game_id"], "expected_step": decision["step"],
        "selection": {"candidate_id": candidate_id,
            "semantic_echo": decision["candidates"][candidate_id as usize]["semantic"]}})
    .to_string()
}

fn first_argmax(logits: &[f32]) -> usize {
    let mut selected = 0;
    for index in 1..logits.len() {
        if logits[index] > logits[selected] {
            selected = index;
        }
    }
    selected
}

/// Keeps a game's first mismatch for the diagnostics stream, so a failed
/// qualification names the decision to look at.
fn note_mismatch_v1(
    counts: &mut QualificationCountsV1,
    noted: &mut bool,
    game_id: &str,
    what: std::fmt::Arguments<'_>,
) {
    if !std::mem::replace(noted, true) {
        counts
            .notes
            .push(format!("{game_id}: first mismatch: {what}"));
    }
}

/// The harness's ending in the bridge's words, `[classification, winner]`,
/// or None while the harness still stands at a decision.
fn harness_ending_v1(harness: &FastActorSessionV1) -> Option<[Value; 2]> {
    let FastActorResponseV1::Terminal(terminal) = harness.current_response() else {
        return None;
    };
    let classification = match terminal.terminal_classification {
        TerminalClassificationV1::Natural => "natural",
        TerminalClassificationV1::Truncated => "truncated",
        TerminalClassificationV1::Halted => "halted",
    };
    Some([
        json!(classification),
        json!(terminal.winner.map(seat_str_v1)),
    ])
}

/// Replays one game under the selection rules its logs declare (the entry
/// point for tests). The qualification replays under the pinned configs'
/// rules instead, after refusing any log that declares another rule.
#[cfg(test)]
pub(crate) fn replay_game_v1(
    game: &LedgerGameV1,
    limits: (u64, u64),
    logs: &[BotLogV1; 2],
    policies: [&mut FrozenPlayPolicyV1; 2],
    counts: &mut QualificationCountsV1,
) -> Result<(), String> {
    let rules = [
        logs[0].header.selection.as_str(),
        logs[1].header.selection.as_str(),
    ];
    replay_under_rules_v1(game, limits, logs, rules, policies, counts)
}

/// Replays one game, recomputing seat s's choices under `rules[s]`. Stops at
/// the first choice mismatch (positions diverge). The caller names the game
/// in any error.
fn replay_under_rules_v1(
    game: &LedgerGameV1,
    limits: (u64, u64),
    logs: &[BotLogV1; 2],
    rules: [&str; 2],
    mut policies: [&mut FrozenPlayPolicyV1; 2],
    counts: &mut QualificationCountsV1,
) -> Result<(), String> {
    let episode_id = spellbench_episode_id_v1(&game.game_id);
    let mut server = replay_server();
    let decks = [game.decks[0].as_str(), game.decks[1].as_str()];
    let mut response: Value = serde_json::from_str(&server.handle_line(&reset_line(
        &game.game_id,
        decks,
        game.game_seed,
        limits,
        "q-0",
    )))
    .map_err(|error| error.to_string())?;
    if response["response_type"] == "error" {
        let refusal = &response["error"];
        return Err(format!("the replay bridge refused the reset: {refusal}"));
    }
    if response["provenance"] != game.engine {
        let replay = &response["provenance"];
        let ledger = &game.engine;
        return Err(format!(
            "the ledger's engine {ledger} is not the replay bridge's {replay}"
        ));
    }
    let mut harness = FastActorSessionV1::reset_with_decks_and_limits_flat_action_v3(
        episode_id,
        game.game_seed,
        limits.0,
        limits.1,
        game.decks.clone(),
    )
    .map_err(|error| error.message)?;
    let seeds = [logs[0].header.stream_seed, logs[1].header.stream_seed];
    for policy in policies.iter_mut() {
        policy.reset_sampling_v1(seeds);
    }
    // The seat streams the harness samples with, drawn again here to check
    // each logged sample seed.
    let mut streams = seeds.map(SplitMix64::seed);
    let mut cursor = [0_usize; 2];
    let mut request_index = 0_u64;
    let mut noted = false;
    while response["response_type"] == "decision" {
        let seat = usize::from(response["acting_seat"] == "p1");
        let seat_name = SEATS_V1[seat];
        let step = response["step"].as_u64().ok_or("decision without step")?;
        let entry = logs[seat]
            .decisions
            .get(cursor[seat])
            .ok_or("log shorter than game")?;
        cursor[seat] += 1;
        if entry.step != step {
            return Err(format!("log step {} but decision step {step}", entry.step));
        }
        let extension = &response["extensions"]["x_kernel_flat_v4"];
        let rows: Vec<u32> = serde_json::from_value(extension["row_candidate_ids"].clone())
            .map_err(|error| error.to_string())?;
        let policy = &mut *policies[seat];
        let scores = policy.score_fast_session_v1(&harness)?;
        let harness_tensor =
            serde_json::to_value(tensor_bits_v1(policy.last_scored_training_tensor_v4()?))
                .map_err(|error| error.to_string())?;
        counts.decisions += 1;
        if extension["tensor"] != harness_tensor {
            counts.tensor_mismatches += 1;
            note_mismatch_v1(
                counts,
                &mut noted,
                &game.game_id,
                format_args!("tensor at step {step} ({seat_name})"),
            );
        }
        let harness_logits: Vec<u32> = scores.logits.iter().map(|value| value.to_bits()).collect();
        if entry.logits_bits != harness_logits || entry.value_bits != scores.value.to_bits() {
            counts.logit_mismatches += 1;
            note_mismatch_v1(
                counts,
                &mut noted,
                &game.game_id,
                format_args!("logits at step {step} ({seat_name})"),
            );
        }
        let (harness_row, sample_seed) = match rules[seat] {
            SELECTION_SAMPLED_WIDE_V1 => (
                policy.select_fast_session_v1(&harness)? as usize,
                Some(streams[seat].next_u64()),
            ),
            SELECTION_ARGMAX_FIRST_V1 => (first_argmax(&scores.logits), None),
            other => return Err(format!("unknown selection {other}")),
        };
        if rows.get(harness_row) != Some(&entry.candidate_id)
            || entry.selected_row as usize != harness_row
            || entry.sample_seed != sample_seed
        {
            counts.choice_mismatches += 1;
            note_mismatch_v1(
                counts,
                &mut noted,
                &game.game_id,
                format_args!("choice at step {step} ({seat_name})"),
            );
            return Ok(());
        }
        request_index += 1;
        let line = server.handle_line(&step_line(
            &response,
            entry.candidate_id,
            &format!("q-{request_index}"),
        ));
        response = serde_json::from_str(&line).map_err(|error| error.to_string())?;
        // A halted game is over: a halt only the raw session has would leave
        // the harness without the same step.
        if response["classification"] != "halted" {
            harness
                .step(episode_id, step, harness_row as u32)
                .map_err(|error| error.message)?;
        }
    }
    if response["response_type"] != "terminal" {
        let error = &response["error"];
        return Err(format!("the replay bridge answered {error}"));
    }
    for (seat, log) in logs.iter().enumerate() {
        if cursor[seat] != log.decisions.len() {
            return Err(format!(
                "{} log longer than game ({} logged, {} played)",
                SEATS_V1[seat],
                log.decisions.len(),
                cursor[seat]
            ));
        }
    }
    let replay = [
        "classification",
        "outcome",
        "reason",
        "step_count",
        "decision_count",
    ]
    .map(|field| response[field].clone());
    let ledger = [
        json!(game.classification),
        json!(game.outcome),
        json!(game.reason),
        json!(game.step_count),
        json!(game.decision_count),
    ];
    // Unless the bridge halted, the harness must have reached the same end.
    let harness_end = harness_ending_v1(&harness);
    let bridge_end = [
        response["classification"].clone(),
        response["winner"].clone(),
    ];
    let harness_agrees =
        response["classification"] == "halted" || harness_end.as_ref() == Some(&bridge_end);
    if replay != ledger || !harness_agrees {
        counts.outcome_mismatches += 1;
        note_mismatch_v1(
            counts,
            &mut noted,
            &game.game_id,
            format_args!(
                "outcome: replay {}, ledger {}, harness {}",
                Value::from(replay.to_vec()),
                Value::from(ledger.to_vec()),
                harness_end.map_or(Value::Null, |ending| Value::from(ending.to_vec()))
            ),
        );
    }
    counts.games_replayed += 1;
    for (seat, deck) in game.decks.iter().enumerate() {
        // A deck both seats played counts the game once.
        let entry = counts.per_deck.entry(deck.clone()).or_default();
        entry.0 += u64::from(seat == 0 || game.decks[0] != *deck);
        entry.1 += cursor[seat] as u64;
    }
    Ok(())
}

pub struct SpellbenchQualificationArgsV1 {
    pub tournament_dir: PathBuf,
    pub decision_log_dir: PathBuf,
    pub bot_configs: BTreeMap<String, PathBuf>,
}

fn read_json(path: &Path) -> Result<Value, String> {
    let bytes = std::fs::read(path).map_err(|error| format!("{}: {error}", path.display()))?;
    serde_json::from_slice(&bytes).map_err(|error| format!("{}: {error}", path.display()))
}

/// One seat's decision log, or None when there is no log file (a bot that
/// keeps no log). A log that exists but cannot be read is an error.
fn read_log(path: &Path) -> Result<Option<BotLogV1>, String> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("{}: {error}", path.display())),
    };
    let located = |error: serde_json::Error| format!("{}: {error}", path.display());
    let mut lines = text.lines();
    let first = lines
        .next()
        .ok_or_else(|| format!("{}: empty log", path.display()))?;
    let header: BotLogHeaderV1 = serde_json::from_str(first).map_err(located)?;
    if header.schema != BOT_LOG_SCHEMA_V1 || header.kind != "header" {
        return Err(format!("{}: not a bot log", path.display()));
    }
    let decisions = lines
        .map(|line| serde_json::from_str(line).map_err(located))
        .collect::<Result<Vec<BotLogDecisionV1>, String>>()?;
    Ok(Some(BotLogV1 { header, decisions }))
}

/// Loads the policy a `--bot` scorer config names, with what the config pins.
type BotPolicyLoaderV1<'a> =
    dyn FnMut(&Path) -> Result<(FrozenPlayPolicyV1, PinnedBotV1), String> + 'a;

/// The selection rules the harness can recompute, by their canonical names.
fn selection_rule_v1(selection: &str) -> Option<&'static str> {
    [SELECTION_SAMPLED_WIDE_V1, SELECTION_ARGMAX_FIRST_V1]
        .into_iter()
        .find(|rule| *rule == selection)
}

/// The policy a `--bot` scorer config names, and what the config pins.
fn load_bot_policy_v1(config_path: &Path) -> Result<(FrozenPlayPolicyV1, PinnedBotV1), String> {
    let config: SpellbenchScorerConfigV1 = serde_json::from_value(read_json(config_path)?)
        .map_err(|error| format!("{}: {error}", config_path.display()))?;
    let selection = selection_rule_v1(&config.selection).ok_or_else(|| {
        let selection = &config.selection;
        format!("{}: unknown selection {selection}", config_path.display())
    })?;
    let (policy, identity) = load_expanded_inference_v1(&config.source)?;
    let model_state_sha256 = identity.state_sha256;
    Ok((
        policy,
        PinnedBotV1 {
            model_state_sha256,
            selection,
        },
    ))
}

pub fn run_spellbench_qualification_v1(
    args: &SpellbenchQualificationArgsV1,
) -> Result<Value, String> {
    qualify_with_loader_v1(args, &mut load_bot_policy_v1, &mut std::io::stderr())
}

/// One `matches.jsonl` row, every field the replay reads required.
fn ledger_game_v1(line: &str) -> Result<LedgerGameV1, String> {
    let row: Value = serde_json::from_str(line).map_err(|error| error.to_string())?;
    let text = |value: &Value, field: &str| {
        value
            .as_str()
            .map(String::from)
            .ok_or_else(|| format!("{field} is not a string"))
    };
    let number = |value: &Value, field: &str| {
        value
            .as_u64()
            .ok_or_else(|| format!("{field} is not an unsigned integer"))
    };
    if row["format"] != SPELLBENCH_FORMAT_PAUPER_BO1_V1 {
        let format = &row["format"];
        return Err(format!(
            "format {format} is not {SPELLBENCH_FORMAT_PAUPER_BO1_V1}"
        ));
    }
    for (seat, name) in SEATS_V1.into_iter().enumerate() {
        if row["seats"][seat]["seat"] != name {
            return Err(format!("seats[{seat}] is not {name}"));
        }
    }
    Ok(LedgerGameV1 {
        game_id: text(&row["game_id"], "game_id")?,
        game_seed: number(&row["game_seed"], "game_seed")?,
        decks: [
            text(&row["decks"][0]["catalog_id"], "decks[0].catalog_id")?,
            text(&row["decks"][1]["catalog_id"], "decks[1].catalog_id")?,
        ],
        bot_names: [
            text(&row["seats"][0]["name"], "seats[0].name")?,
            text(&row["seats"][1]["name"], "seats[1].name")?,
        ],
        classification: text(&row["classification"], "classification")?,
        outcome: text(&row["outcome"], "outcome")?,
        step_count: number(&row["step_count"], "step_count")?,
        reason: text(&row["reason"], "reason")?,
        decision_count: number(&row["decision_count"], "decision_count")?,
        engine: row["engine"].clone(),
    })
}

/// `run_spellbench_qualification_v1` with the policy loader passed in (the
/// tests load the fixture policy) and the diagnostics stream (stderr).
///
/// A game no `--bot` bot played is not applicable. Any other game must be
/// replayed: one with a seat that has no `--bot` config, or whose log is
/// missing or unreadable, is skipped, named on the diagnostics stream, and
/// fails the qualification.
fn qualify_with_loader_v1(
    args: &SpellbenchQualificationArgsV1,
    load: &mut BotPolicyLoaderV1<'_>,
    diagnostics: &mut dyn Write,
) -> Result<Value, String> {
    let config = read_json(&args.tournament_dir.join("config.json"))?;
    let limits = (
        config["max_decisions"]
            .as_u64()
            .ok_or("config.max_decisions")?,
        config["max_steps"].as_u64().ok_or("config.max_steps")?,
    );
    let ledger_path = args.tournament_dir.join("matches.jsonl");
    let ledger = std::fs::read_to_string(&ledger_path)
        .map_err(|error| format!("{}: {error}", ledger_path.display()))?;
    let mut policies: HashMap<(String, usize), FrozenPlayPolicyV1> = HashMap::new();
    let mut pinned: HashMap<String, PinnedBotV1> = HashMap::new();
    let mut counts = QualificationCountsV1::default();
    for (index, line) in ledger.lines().enumerate() {
        let game = ledger_game_v1(line)
            .map_err(|error| format!("{}:{}: {error}", ledger_path.display(), index + 1))?;
        let configured = game
            .bot_names
            .each_ref()
            .map(|name| args.bot_configs.contains_key(name));
        if configured == [false, false] {
            counts.games_not_applicable += 1;
            continue;
        }
        let paths = SEATS_V1.map(|seat| {
            args.decision_log_dir
                .join(format!("{}.{seat}.jsonl", game.game_id))
        });
        let reads = [0, 1].map(|seat| configured[seat].then(|| read_log(&paths[seat])));
        let mut unreplayable = Vec::new();
        for (seat, read) in reads.iter().enumerate() {
            let why = match read {
                Some(Ok(Some(_))) => continue,
                None => "no --bot config".to_string(),
                Some(Ok(None)) => format!("no decision log {}", paths[seat].display()),
                Some(Err(error)) => format!("unreadable decision log: {error}"),
            };
            let name = &game.bot_names[seat];
            unreplayable.push(format!("{} ({name}): {why}", SEATS_V1[seat]));
        }
        if !unreplayable.is_empty() {
            counts.games_skipped += 1;
            for why in unreplayable {
                let game_id = &game.game_id;
                let _ = writeln!(
                    diagnostics,
                    "spellbench qualify: {game_id}: not replayed: {why}"
                );
            }
            continue;
        }
        let [Some(Ok(Some(log0))), Some(Ok(Some(log1)))] = reads else {
            unreachable!("both seats have a log");
        };
        for (seat, log) in [&log0, &log1].into_iter().enumerate() {
            let name = &game.bot_names[seat];
            if let Entry::Vacant(slot) = policies.entry((name.clone(), seat)) {
                let (policy, bot) = load(&args.bot_configs[name])?;
                pinned.insert(name.clone(), bot);
                slot.insert(policy);
            }
            let header = &log.header;
            let (game_id, seat_name) = (&game.game_id, SEATS_V1[seat]);
            if header.game_id != *game_id || header.seat != seat_name || header.bot_name != *name {
                return Err(format!(
                    "{game_id}: {seat_name} log header (game {}, seat {}, bot {}) is not this \
                     row's (game {game_id}, seat {seat_name}, bot {name})",
                    header.game_id, header.seat, header.bot_name
                ));
            }
            // Every log, not only the one read when its bot's policy loads.
            let bot = &pinned[name];
            if header.model_state_sha256 != bot.model_state_sha256 {
                return Err(format!(
                    "{game_id}: {name}: log model {} is not config model {}",
                    header.model_state_sha256, bot.model_state_sha256
                ));
            }
            if header.selection != bot.selection {
                return Err(format!(
                    "{game_id}: {name}: log selection {} is not config selection {}",
                    header.selection, bot.selection
                ));
            }
        }
        // The harness recomputes under the configs' rules, not the logs'.
        let rules = game.bot_names.each_ref().map(|name| pinned[name].selection);
        let mut p0 = policies
            .remove(&(game.bot_names[0].clone(), 0))
            .expect("loaded");
        let mut p1 = policies
            .remove(&(game.bot_names[1].clone(), 1))
            .expect("loaded");
        let result = replay_under_rules_v1(
            &game,
            limits,
            &[log0, log1],
            rules,
            [&mut p0, &mut p1],
            &mut counts,
        );
        policies.insert((game.bot_names[0].clone(), 0), p0);
        policies.insert((game.bot_names[1].clone(), 1), p1);
        for note in counts.notes.drain(..) {
            let _ = writeln!(diagnostics, "spellbench qualify: {note}");
        }
        result.map_err(|error| format!("{}: {error}", game.game_id))?;
    }
    let clean = counts.tensor_mismatches
        + counts.logit_mismatches
        + counts.choice_mismatches
        + counts.outcome_mismatches
        == 0;
    let pass = clean && counts.games_replayed > 0 && counts.games_skipped == 0;
    Ok(json!({
        "schema": QUALIFICATION_REPORT_SCHEMA_V1,
        "games_replayed": counts.games_replayed, "games_skipped": counts.games_skipped,
        "games_not_applicable": counts.games_not_applicable, "decisions": counts.decisions,
        "tensor_mismatches": counts.tensor_mismatches, "logit_mismatches": counts.logit_mismatches,
        "choice_mismatches": counts.choice_mismatches, "outcome_mismatches": counts.outcome_mismatches,
        "per_deck": counts.per_deck.iter().map(|(deck, (games, decisions))| (deck.clone(), json!({"games": games, "decisions": decisions}))).collect::<serde_json::Map<_, _>>(),
        "verdict": if pass { "PASS" } else { "FAIL" },
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spellbench_scorer_v1::SpellbenchScorerV1;
    use crate::state::SplitMix64;

    /// Plays one game through the in-process bridge with two in-process
    /// scorers exactly as the Python bot drives them, returning its ledger
    /// row and both seats' decision logs.
    fn play_logged_game(
        game_id: &str,
        deck: &str,
        seed: u64,
        limits: (u64, u64),
        stream_seeds: [u64; 2],
    ) -> (LedgerGameV1, [BotLogV1; 2]) {
        let scorers = [0, 1].map(|_| fixture_scorer(SELECTION_SAMPLED_WIDE_V1));
        play_game_with_scorers(
            scorers,
            ["fixture", "fixture"],
            game_id,
            [deck, deck],
            seed,
            limits,
            stream_seeds,
        )
    }

    fn fixture_scorer(selection: &str) -> SpellbenchScorerV1 {
        SpellbenchScorerV1::new(
            FrozenPlayPolicyV1::training_fixture_v4(),
            json!({"fixture": true}),
            "0".repeat(64),
            selection,
        )
        .unwrap()
    }

    /// `play_logged_game` with the given scorers, bot names and decks. Each
    /// log header takes its selection and model state from its scorer's
    /// ready record; like the bot, only a sampled seat draws from its stream.
    fn play_game_with_scorers(
        mut scorers: [SpellbenchScorerV1; 2],
        bot_names: [&str; 2],
        game_id: &str,
        decks: [&str; 2],
        seed: u64,
        limits: (u64, u64),
        stream_seeds: [u64; 2],
    ) -> (LedgerGameV1, [BotLogV1; 2]) {
        let ready = scorers.each_ref().map(SpellbenchScorerV1::ready_record);
        let mut streams = stream_seeds.map(SplitMix64::seed);
        let mut logs = [0, 1].map(|seat| BotLogV1 {
            header: BotLogHeaderV1 {
                schema: BOT_LOG_SCHEMA_V1.into(),
                kind: "header".into(),
                game_id: game_id.into(),
                seat: SEATS_V1[seat].into(),
                bot_name: bot_names[seat].into(),
                bot_version: "1.0.0".into(),
                selection: ready[seat]["selection"].as_str().unwrap().into(),
                stream_seed: stream_seeds[seat],
                model_state_sha256: ready[seat]["model_state_sha256"].as_str().unwrap().into(),
            },
            decisions: Vec::new(),
        });
        let mut server = replay_server();
        let mut response: Value = serde_json::from_str(
            &server.handle_line(&reset_line(game_id, decks, seed, limits, "q-0")),
        )
        .unwrap();
        let mut index = 0;
        while response["response_type"] == "decision" {
            let seat = usize::from(response["acting_seat"] == "p1");
            let extension = &response["extensions"]["x_kernel_flat_v4"];
            let sampled = logs[seat].header.selection == SELECTION_SAMPLED_WIDE_V1;
            let sample_seed = sampled.then(|| streams[seat].next_u64());
            let request = json!({"schema": "mtg-kernel-spellbench-scorer-request/v1", "request_id": format!("{game_id}:{}", response["step"]),
                "game_id": game_id, "seat": response["acting_seat"], "step": response["step"],
                "feature_contract_digest": extension["feature_contract_digest"], "feature_encoding_digest": extension["feature_encoding_digest"],
                "row_candidate_ids": extension["row_candidate_ids"], "sample_seed": sample_seed, "tensor": extension["tensor"]});
            let choice = scorers[seat]
                .score_line(&serde_json::to_vec(&request).unwrap())
                .unwrap();
            let candidate_id = choice["selected_candidate_id"].as_u64().unwrap() as u32;
            logs[seat].decisions.push(BotLogDecisionV1 {
                kind: "decision".into(),
                step: response["step"].as_u64().unwrap(),
                candidate_id,
                selected_row: choice["selected_row"].as_u64().unwrap() as u32,
                sample_seed,
                logits_bits: serde_json::from_value(choice["logits_bits"].clone()).unwrap(),
                value_bits: choice["value_bits"].as_u64().unwrap() as u32,
                request_sha256: choice["request_sha256"].as_str().unwrap().into(),
                elapsed_us: 0,
            });
            index += 1;
            response = serde_json::from_str(&server.handle_line(&step_line(
                &response,
                candidate_id,
                &format!("q-{index}"),
            )))
            .unwrap();
        }
        let game = LedgerGameV1 {
            game_id: game_id.into(),
            game_seed: seed,
            decks: decks.map(String::from),
            bot_names: bot_names.map(String::from),
            classification: response["classification"].as_str().unwrap().into(),
            outcome: response["outcome"].as_str().unwrap().into(),
            step_count: response["step_count"].as_u64().unwrap(),
            reason: response["reason"].as_str().unwrap().into(),
            decision_count: response["decision_count"].as_u64().unwrap(),
            engine: response["provenance"].clone(),
        };
        (game, logs)
    }

    fn harness_pair() -> [FrozenPlayPolicyV1; 2] {
        [
            FrozenPlayPolicyV1::training_fixture_v4(),
            FrozenPlayPolicyV1::training_fixture_v4(),
        ]
    }

    #[test]
    fn faithful_logs_replay_with_zero_mismatches() {
        for deck in ["Burn", "Spy"] {
            let (game, logs) =
                play_logged_game(&format!("q-{deck}"), deck, 21, (300, 3_000), [11, 12]);
            let mut counts = QualificationCountsV1::default();
            let [mut p0, mut p1] = harness_pair();
            replay_game_v1(&game, (300, 3_000), &logs, [&mut p0, &mut p1], &mut counts).unwrap();
            assert_eq!(counts.games_replayed, 1);
            assert!(counts.decisions > 20);
            assert_eq!(
                (
                    counts.tensor_mismatches,
                    counts.logit_mismatches,
                    counts.choice_mismatches,
                    counts.outcome_mismatches
                ),
                (0, 0, 0, 0),
                "{deck}"
            );
        }
    }

    #[test]
    fn tampered_logits_and_choices_are_counted() {
        let (game, mut logs) = play_logged_game("q-tamper", "Burn", 22, (300, 3_000), [13, 14]);
        logs[0].decisions[0].logits_bits[0] ^= 1;
        let mut counts = QualificationCountsV1::default();
        let [mut p0, mut p1] = harness_pair();
        replay_game_v1(&game, (300, 3_000), &logs, [&mut p0, &mut p1], &mut counts).unwrap();
        assert_eq!(counts.logit_mismatches, 1);

        let (game, mut logs) = play_logged_game("q-tamper-2", "Burn", 23, (300, 3_000), [15, 16]);
        let first = logs[0]
            .decisions
            .iter_mut()
            .find(|decision| decision.logits_bits.len() >= 2)
            .unwrap();
        first.candidate_id += 1; // the bot "chose" something the harness did not
        let mut counts = QualificationCountsV1::default();
        let [mut p0, mut p1] = harness_pair();
        replay_game_v1(&game, (300, 3_000), &logs, [&mut p0, &mut p1], &mut counts).unwrap();
        assert_eq!(counts.choice_mismatches, 1);
        assert_eq!(
            counts.games_replayed, 0,
            "a diverged replay is not counted as replayed"
        );
    }

    /// Short games for the tests below: at most 40 physical decisions.
    const SHORT: (u64, u64) = (40, 400);

    /// A named edit of a ledger game, of a logged decision (with the (logit,
    /// choice) mismatches and replayed games it must produce), or of a ledger
    /// row (with the end of the error it must produce).
    type LedgerTamper = (&'static str, fn(&mut LedgerGameV1));
    type EntryTamper = (&'static str, fn(&mut BotLogDecisionV1), (u64, u64), u64);
    type RowEdit = (&'static str, fn(&mut Value), &'static str);

    #[test]
    fn a_ledger_outcome_the_replay_does_not_reach_is_counted() {
        let (game, logs) = play_logged_game("q-outcome", "Burn", 24, SHORT, [17, 18]);
        let tampers: [LedgerTamper; 3] = [
            ("step_count", |game| game.step_count += 1),
            ("decision_count", |game| game.decision_count += 1),
            ("reason", |game| game.reason.push('!')),
        ];
        for (field, tamper) in tampers {
            let mut ledger = game.clone();
            tamper(&mut ledger);
            let mut counts = QualificationCountsV1::default();
            let [mut p0, mut p1] = harness_pair();
            replay_game_v1(&ledger, SHORT, &logs, [&mut p0, &mut p1], &mut counts).unwrap();
            assert_eq!(
                (counts.games_replayed, counts.outcome_mismatches),
                (1, 1),
                "{field}"
            );
            assert_eq!(counts.notes.len(), 1, "{field}: {:?}", counts.notes);
            let note = &counts.notes[0];
            assert!(
                note.starts_with("q-outcome: first mismatch: outcome: replay ["),
                "{field}: {note}"
            );
        }
    }

    #[test]
    fn tampered_rows_seeds_and_values_are_counted() {
        let (game, logs) = play_logged_game("q-fields", "Burn", 30, SHORT, [31, 32]);
        // The tampered field of p0's first decision; the (logit, choice)
        // mismatches and replayed games it must produce.
        let tampers: [EntryTamper; 3] = [
            ("selected_row", |entry| entry.selected_row += 1, (0, 1), 0),
            (
                "sample_seed",
                |entry| entry.sample_seed = entry.sample_seed.map(|seed| seed ^ 1),
                (0, 1),
                0,
            ),
            ("value_bits", |entry| entry.value_bits ^= 1, (1, 0), 1),
        ];
        for (field, tamper, mismatches, replayed) in tampers {
            let mut logs = logs.clone();
            tamper(&mut logs[0].decisions[0]);
            let mut counts = QualificationCountsV1::default();
            let [mut p0, mut p1] = harness_pair();
            replay_game_v1(&game, SHORT, &logs, [&mut p0, &mut p1], &mut counts).unwrap();
            assert_eq!(
                (
                    (counts.logit_mismatches, counts.choice_mismatches),
                    counts.games_replayed
                ),
                (mismatches, replayed),
                "{field}"
            );
        }
    }

    #[test]
    fn a_log_longer_than_its_game_is_refused() {
        let (game, mut logs) = play_logged_game("q-longer", "Burn", 25, SHORT, [19, 20]);
        let played = logs[1].decisions.len();
        let extra = logs[1].decisions[0].clone();
        logs[1].decisions.push(extra);
        let mut counts = QualificationCountsV1::default();
        let [mut p0, mut p1] = harness_pair();
        assert_eq!(
            replay_game_v1(&game, SHORT, &logs, [&mut p0, &mut p1], &mut counts),
            Err(format!(
                "p1 log longer than game ({} logged, {played} played)",
                played + 1
            ))
        );
    }

    /// A fresh directory under the system temp dir.
    fn scratch_dir(label: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "mtg-kernel-spellbench-qualify-v1-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    /// Every `--bot` loads the fixture policy, whose model state the fixture
    /// scorers report as "0" x 64. A config named `*-argmax.scorer.json` pins
    /// argmax selection; any other pins sampled selection.
    fn fixture_loader(config: &Path) -> Result<(FrozenPlayPolicyV1, PinnedBotV1), String> {
        let argmax = config.to_string_lossy().ends_with("-argmax.scorer.json");
        let pinned = PinnedBotV1 {
            model_state_sha256: "0".repeat(64),
            selection: if argmax {
                SELECTION_ARGMAX_FIRST_V1
            } else {
                SELECTION_SAMPLED_WIDE_V1
            },
        };
        Ok((FrozenPlayPolicyV1::training_fixture_v4(), pinned))
    }

    /// The qualification with the fixture loader: its result and the
    /// diagnostics lines it wrote.
    fn qualify_fixture(
        args: &SpellbenchQualificationArgsV1,
    ) -> (Result<Value, String>, Vec<String>) {
        let mut diagnostics = Vec::new();
        let result = qualify_with_loader_v1(args, &mut fixture_loader, &mut diagnostics);
        let text = String::from_utf8(diagnostics).unwrap();
        (result, text.lines().map(String::from).collect())
    }

    /// `game`'s `matches.jsonl` row, in the arena's field names (those the
    /// verifier reads, plus the schema).
    fn ledger_row(game: &LedgerGameV1) -> Value {
        json!({"schema": "spellbench-match-ledger/v1", "game_id": game.game_id, "format": "pauper-bo1",
            "game_seed": game.game_seed,
            "seats": [{"seat": "p0", "name": game.bot_names[0]}, {"seat": "p1", "name": game.bot_names[1]}],
            "decks": [{"catalog_id": game.decks[0]}, {"catalog_id": game.decks[1]}],
            "classification": game.classification, "outcome": game.outcome, "reason": game.reason,
            "step_count": game.step_count, "decision_count": game.decision_count, "engine": game.engine})
    }

    /// Writes a tournament as the arena and `kernel_flat_bot.py` leave it:
    /// `config.json` (the verifier reads the caps), `matches.jsonl` and the
    /// given decision logs. Only the `configured` bots get a `--bot` config
    /// (a placeholder path: the fixture loader reads only its name).
    fn write_tournament(
        dir: &Path,
        limits: (u64, u64),
        rows: &[Value],
        logs: &[&BotLogV1],
        configured: &[&str],
    ) -> SpellbenchQualificationArgsV1 {
        let (tournament_dir, decision_log_dir) = (dir.join("tournament"), dir.join("logs"));
        std::fs::create_dir_all(&tournament_dir).unwrap();
        std::fs::create_dir_all(&decision_log_dir).unwrap();
        let config = json!({"schema": "spellbench-tournament-config/v1", "max_decisions": limits.0, "max_steps": limits.1});
        std::fs::write(tournament_dir.join("config.json"), format!("{config}\n")).unwrap();
        let ledger: String = rows.iter().map(|row| format!("{row}\n")).collect();
        std::fs::write(tournament_dir.join("matches.jsonl"), ledger).unwrap();
        for log in logs {
            write_log(&decision_log_dir, log);
        }
        let bot_configs = configured
            .iter()
            .map(|name| (name.to_string(), dir.join(format!("{name}.scorer.json"))))
            .collect();
        SpellbenchQualificationArgsV1 {
            tournament_dir,
            decision_log_dir,
            bot_configs,
        }
    }

    /// One decision log in the layout `kernel_flat_bot.py` writes.
    fn write_log(dir: &Path, log: &BotLogV1) {
        let header = &log.header;
        let mut text = json!({"schema": header.schema, "kind": header.kind, "game_id": header.game_id, "seat": header.seat,
            "bot_name": header.bot_name, "bot_version": header.bot_version, "selection": header.selection,
            "stream_seed": header.stream_seed, "model_state_sha256": header.model_state_sha256}).to_string();
        for decision in &log.decisions {
            text.push('\n');
            text.push_str(&json!({"kind": decision.kind, "step": decision.step, "candidate_id": decision.candidate_id,
                "selected_row": decision.selected_row, "sample_seed": decision.sample_seed, "logits_bits": decision.logits_bits,
                "value_bits": decision.value_bits, "request_sha256": decision.request_sha256, "elapsed_us": decision.elapsed_us}).to_string());
        }
        text.push('\n');
        std::fs::write(
            dir.join(format!("{}.{}.jsonl", header.game_id, header.seat)),
            text,
        )
        .unwrap();
    }

    #[test]
    fn the_verifier_reads_the_arena_ledger_and_the_bot_logs() {
        // Burn against Spy; p1's bot is pinned to argmax, so its log carries
        // no sample seeds.
        let scorers = [
            fixture_scorer(SELECTION_SAMPLED_WIDE_V1),
            fixture_scorer(SELECTION_ARGMAX_FIRST_V1),
        ];
        let (game, logs) = play_game_with_scorers(
            scorers,
            ["fixture", "fixture-argmax"],
            "q-files",
            ["Burn", "Spy"],
            26,
            SHORT,
            [21, 22],
        );
        assert!(logs[1]
            .decisions
            .iter()
            .all(|decision| decision.sample_seed.is_none()));
        // Bots without a --bot config (builtins, say): nothing to qualify.
        let builtins = LedgerGameV1 {
            game_id: "q-builtins".into(),
            bot_names: ["uniform".into(), "heuristic".into()],
            ..game.clone()
        };
        let dir = scratch_dir("files");
        let args = write_tournament(
            &dir.join("both"),
            SHORT,
            &[ledger_row(&game), ledger_row(&builtins)],
            &[&logs[0], &logs[1]],
            &["fixture", "fixture-argmax"],
        );
        let decisions = [0, 1].map(|seat| logs[seat].decisions.len());
        let (report, diagnostics) = qualify_fixture(&args);
        assert_eq!(
            report.unwrap(),
            json!({"schema": "mtg-kernel-spellbench-qualification/v1", "games_replayed": 1, "games_skipped": 0,
                "games_not_applicable": 1, "decisions": decisions[0] + decisions[1], "tensor_mismatches": 0,
                "logit_mismatches": 0, "choice_mismatches": 0, "outcome_mismatches": 0,
                "per_deck": {"Burn": {"games": 1, "decisions": decisions[0]}, "Spy": {"games": 1, "decisions": decisions[1]}},
                "verdict": "PASS"})
        );
        assert_eq!(diagnostics, Vec::<String>::new());
        // Nothing replayed is no pass.
        let args = write_tournament(
            &dir.join("none"),
            SHORT,
            &[ledger_row(&builtins)],
            &[],
            &["fixture"],
        );
        let report = qualify_fixture(&args).0.unwrap();
        assert_eq!(
            (&report["games_not_applicable"], &report["verdict"]),
            (&json!(1), &json!("FAIL"))
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// Qualifies `args`, which hold one clean game and one that cannot be
    /// replayed; checks the FAIL and returns the one diagnostics line.
    fn only_skip(args: &SpellbenchQualificationArgsV1) -> String {
        let (report, diagnostics) = qualify_fixture(args);
        let report = report.unwrap();
        assert_eq!(
            (
                &report["games_replayed"],
                &report["games_skipped"],
                &report["verdict"]
            ),
            (&json!(1), &json!(1), &json!("FAIL")),
            "{report}"
        );
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
        diagnostics[0].clone()
    }

    #[test]
    fn every_game_a_configured_bot_played_must_be_replayed() {
        let (game, logs) = play_logged_game("q-skip", "Burn", 33, SHORT, [34, 35]);
        let (clean, clean_logs) = play_logged_game("q-clean", "Burn", 36, SHORT, [37, 38]);
        let rows = [ledger_row(&clean), ledger_row(&game)];
        let dir = scratch_dir("skip");
        // Both seats configured; p1's log is missing.
        let args = write_tournament(
            &dir.join("missing"),
            SHORT,
            &rows,
            &[&clean_logs[0], &clean_logs[1], &logs[0]],
            &["fixture"],
        );
        let missing = args.decision_log_dir.join("q-skip.p1.jsonl");
        assert_eq!(
            only_skip(&args),
            format!(
                "spellbench qualify: q-skip: not replayed: p1 (fixture): no decision log {}",
                missing.display()
            )
        );
        // A configured bot against one without a --bot config: that seat's
        // choices cannot be recomputed.
        let mixed = LedgerGameV1 {
            bot_names: ["fixture".into(), "uniform".into()],
            ..game.clone()
        };
        let args = write_tournament(
            &dir.join("mixed"),
            SHORT,
            &[ledger_row(&clean), ledger_row(&mixed)],
            &[&clean_logs[0], &clean_logs[1], &logs[0]],
            &["fixture"],
        );
        assert_eq!(
            only_skip(&args),
            "spellbench qualify: q-skip: not replayed: p1 (uniform): no --bot config"
        );
        // A log that exists but cannot be read.
        let args = write_tournament(
            &dir.join("unreadable"),
            SHORT,
            &rows,
            &[&clean_logs[0], &clean_logs[1], &logs[0], &logs[1]],
            &["fixture"],
        );
        let unreadable = args.decision_log_dir.join("q-skip.p0.jsonl");
        std::fs::remove_file(&unreadable).unwrap();
        std::fs::create_dir(&unreadable).unwrap();
        let line = only_skip(&args);
        let prefix = format!(
            "spellbench qualify: q-skip: not replayed: p0 (fixture): unreadable decision log: {}: ",
            unreadable.display()
        );
        assert!(line.starts_with(&prefix), "{line}");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// The error a fixture-bot qualification of `rows` and `logs` stops with.
    fn refusal(dir: &Path, rows: &[Value], logs: &[&BotLogV1]) -> String {
        let args = write_tournament(dir, SHORT, rows, logs, &["fixture"]);
        qualify_fixture(&args).0.unwrap_err()
    }

    #[test]
    fn logs_that_are_not_the_pinned_bots_are_refused() {
        let (first, first_logs) = play_logged_game("q-refuse-1", "Burn", 27, SHORT, [23, 24]);
        let (second, mut second_logs) = play_logged_game("q-refuse-2", "Burn", 28, SHORT, [25, 26]);
        let dir = scratch_dir("refuse");
        // Every log's model is checked, not only the one read when its
        // policy is loaded: p1's policy is loaded by the first game.
        second_logs[1].header.model_state_sha256 = "1".repeat(64);
        assert_eq!(
            refusal(
                &dir.join("model"),
                &[ledger_row(&first), ledger_row(&second)],
                &[
                    &first_logs[0],
                    &first_logs[1],
                    &second_logs[0],
                    &second_logs[1]
                ],
            ),
            format!(
                "q-refuse-2: fixture: log model {} is not config model {}",
                "1".repeat(64),
                "0".repeat(64)
            )
        );
        // Played by argmax, qualified against a config pinned to sampled
        // selection: every log's rule is checked, like its model.
        let argmax = [0, 1].map(|_| fixture_scorer(SELECTION_ARGMAX_FIRST_V1));
        let (third, third_logs) = play_game_with_scorers(
            argmax,
            ["fixture", "fixture"],
            "q-refuse-3",
            ["Burn", "Burn"],
            29,
            SHORT,
            [27, 28],
        );
        assert_eq!(
            refusal(
                &dir.join("selection"),
                &[ledger_row(&first), ledger_row(&third)],
                &[&first_logs[0], &first_logs[1], &third_logs[0], &third_logs[1]],
            ),
            "q-refuse-3: fixture: log selection argmax-first-v1 is not config selection sampled-wide-v1"
        );
        // A header that names another bot, or another game (a misfiled log).
        let mut renamed = first_logs[0].clone();
        renamed.header.bot_name = "g115".into();
        assert_eq!(
            refusal(
                &dir.join("bot"),
                &[ledger_row(&first)],
                &[&renamed, &first_logs[1]],
            ),
            "q-refuse-1: p0 log header (game q-refuse-1, seat p0, bot g115) is not this row's \
             (game q-refuse-1, seat p0, bot fixture)"
        );
        let args = write_tournament(
            &dir.join("misfiled"),
            SHORT,
            &[ledger_row(&first)],
            &[&second_logs[0], &first_logs[1]],
            &["fixture"],
        );
        std::fs::rename(
            args.decision_log_dir.join("q-refuse-2.p0.jsonl"),
            args.decision_log_dir.join("q-refuse-1.p0.jsonl"),
        )
        .unwrap();
        assert_eq!(
            qualify_fixture(&args).0.unwrap_err(),
            "q-refuse-1: p0 log header (game q-refuse-2, seat p0, bot fixture) is not this row's \
             (game q-refuse-1, seat p0, bot fixture)"
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn ledger_rows_the_replay_cannot_honor_are_refused() {
        let (game, logs) = play_logged_game("q-row", "Burn", 39, SHORT, [40, 41]);
        let dir = scratch_dir("rows");
        // Field problems name the ledger line.
        let edits: [RowEdit; 3] = [
            (
                "deck",
                |row| row["decks"][1] = json!({}),
                ":1: decks[1].catalog_id is not a string",
            ),
            (
                "seats",
                |row| row["seats"].as_array_mut().unwrap().reverse(),
                ":1: seats[0] is not p0",
            ),
            (
                "format",
                |row| row["format"] = json!("pauper-bo3"),
                ":1: format \"pauper-bo3\" is not pauper-bo1",
            ),
        ];
        for (label, edit, suffix) in edits {
            let mut row = ledger_row(&game);
            edit(&mut row);
            let args = write_tournament(
                &dir.join(label),
                SHORT,
                &[row],
                &[&logs[0], &logs[1]],
                &["fixture"],
            );
            let ledger = args.tournament_dir.join("matches.jsonl");
            assert_eq!(
                qualify_fixture(&args).0.unwrap_err(),
                format!("{}{suffix}", ledger.display()),
                "{label}"
            );
        }
        let args = write_tournament(
            &dir.join("json"),
            SHORT,
            &[ledger_row(&game)],
            &[&logs[0], &logs[1]],
            &["fixture"],
        );
        let ledger = args.tournament_dir.join("matches.jsonl");
        std::fs::write(&ledger, format!("{}\nnot json\n", ledger_row(&game))).unwrap();
        let error = qualify_fixture(&args).0.unwrap_err();
        assert!(
            error.starts_with(&format!("{}:2: ", ledger.display())),
            "{error}"
        );
        // Another engine build, and a deck the replay bridge refuses.
        let mut row = ledger_row(&game);
        row["engine"]["card_pool_identity"] = json!("another-pool");
        let error = refusal(&dir.join("engine"), &[row], &[&logs[0], &logs[1]]);
        assert!(
            error.starts_with("q-row: the ledger's engine {") && error.contains("another-pool"),
            "{error}"
        );
        let mut row = ledger_row(&game);
        row["decks"] = json!([{"catalog_id": "Terror"}, {"catalog_id": "Terror"}]);
        let error = refusal(&dir.join("terror"), &[row], &[&logs[0], &logs[1]]);
        assert!(
            error.starts_with("q-row: the replay bridge refused the reset: ")
                && error.contains("unsupported_deck"),
            "{error}"
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// Needs SPELLBENCH_KERNEL_BOT_CONFIG_DIR=E:/spellbench-archive/kernel-bots.
    #[test]
    #[ignore = "loads the real Phase 1 checkpoints"]
    fn real_models_qualify_through_the_public_entry_point() {
        let dir = PathBuf::from(std::env::var("SPELLBENCH_KERNEL_BOT_CONFIG_DIR").unwrap());
        let names = ["g115", "a48"];
        let bot_configs: BTreeMap<String, PathBuf> = names
            .iter()
            .map(|name| (name.to_string(), dir.join(format!("{name}.scorer.json"))))
            .collect();
        let scorers = names.map(|name| {
            let config: SpellbenchScorerConfigV1 =
                serde_json::from_value(read_json(&bot_configs[name]).unwrap()).unwrap();
            let (policy, identity) = load_expanded_inference_v1(&config.source).unwrap();
            let model = serde_json::to_value(&identity).unwrap();
            SpellbenchScorerV1::new(policy, model, identity.state_sha256, &config.selection)
                .unwrap()
        });
        let (game, logs) = play_game_with_scorers(
            scorers,
            names,
            "q-real",
            ["Elves", "Elves"],
            29,
            SHORT,
            [27, 28],
        );
        let scratch = scratch_dir("real");
        let args = SpellbenchQualificationArgsV1 {
            bot_configs,
            ..write_tournament(
                &scratch,
                SHORT,
                &[ledger_row(&game)],
                &[&logs[0], &logs[1]],
                &names,
            )
        };
        let report = run_spellbench_qualification_v1(&args).unwrap();
        let decisions = logs[0].decisions.len() + logs[1].decisions.len();
        assert_eq!(report["verdict"], "PASS", "{report}");
        assert_eq!(
            (&report["games_replayed"], &report["decisions"]),
            (&json!(1), &json!(decisions))
        );
        std::fs::remove_dir_all(&scratch).unwrap();
    }
}
