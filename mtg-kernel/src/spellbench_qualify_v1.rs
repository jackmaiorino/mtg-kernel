//! Qualification for Spellbench kernel bots: replays each logged game and
//! proves, decision by decision, that the bridge gave the bot the evaluation
//! harness's input (tensor), that the bot's scorer produced the harness's
//! logits, and that the bot chose the harness's row with the same seat stream.
//! The harness is `FrozenPlayPolicyV1` scoring its own flat-action V3
//! `FastActorSessionV1`, the evaluation path of the Phase 1 lineage.

use crate::agent_bridge_flat_v4_encode::tensor_bits_v1;
use crate::agent_bridge_v1::{
    spellbench_episode_id_v1, SpellbenchBridgeOptionsV1, SpellbenchBridgeServerV1,
    SpellbenchEngineIdentityV1,
};
use crate::expanded_deck_training_v1::load_expanded_inference_v1;
use crate::rl_session::FastActorSessionV1;
use crate::sideboard_play_policy_v1::FrozenPlayPolicyV1;
use crate::spellbench_scorer_v1::{
    SpellbenchScorerConfigV1, SELECTION_ARGMAX_FIRST_V1, SELECTION_SAMPLED_WIDE_V1,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::hash_map::Entry;
use std::collections::{BTreeMap, HashMap};
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

pub(crate) struct BotLogV1 {
    header: BotLogHeaderV1,
    decisions: Vec<BotLogDecisionV1>,
}

pub(crate) struct LedgerGameV1 {
    game_id: String,
    game_seed: u64,
    decks: [String; 2],
    bot_names: [String; 2],
    classification: String,
    outcome: String,
    step_count: u64,
}

#[derive(Debug, Default)]
pub(crate) struct QualificationCountsV1 {
    games_replayed: u64,
    games_skipped: u64,
    decisions: u64,
    tensor_mismatches: u64,
    logit_mismatches: u64,
    choice_mismatches: u64,
    outcome_mismatches: u64,
    per_deck: BTreeMap<String, (u64, u64)>,
}

fn replay_server() -> SpellbenchBridgeServerV1 {
    SpellbenchBridgeServerV1::with_options(
        SpellbenchEngineIdentityV1::with_source_revision(None),
        SpellbenchBridgeOptionsV1 {
            x_kernel_flat_v4: true,
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

/// Writes a game's first mismatch to stderr, once per game, so a failed
/// qualification names the decision to look at.
fn note_mismatch_v1(noted: &mut bool, game_id: &str, what: std::fmt::Arguments<'_>) {
    if !std::mem::replace(noted, true) {
        eprintln!("spellbench qualify: {game_id}: first mismatch: {what}");
    }
}

/// Replays one game. Stops at the first choice mismatch (positions diverge).
pub(crate) fn replay_game_v1(
    game: &LedgerGameV1,
    limits: (u64, u64),
    logs: &[BotLogV1; 2],
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
    let mut cursor = [0_usize; 2];
    let mut request_index = 0_u64;
    let mut decisions = 0_u64;
    let mut noted = false;
    while response["response_type"] == "decision" {
        let seat = usize::from(response["acting_seat"] == "p1");
        let seat_name = SEATS_V1[seat];
        let step = response["step"].as_u64().ok_or("decision without step")?;
        let entry = logs[seat]
            .decisions
            .get(cursor[seat])
            .ok_or_else(|| format!("{}: log shorter than game", game.game_id))?;
        cursor[seat] += 1;
        if entry.step != step {
            return Err(format!(
                "{}: log step {} but decision step {step}",
                game.game_id, entry.step
            ));
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
        decisions += 1;
        if extension["tensor"] != harness_tensor {
            counts.tensor_mismatches += 1;
            note_mismatch_v1(
                &mut noted,
                &game.game_id,
                format_args!("tensor at step {step} ({seat_name})"),
            );
        }
        let harness_logits: Vec<u32> = scores.logits.iter().map(|value| value.to_bits()).collect();
        if entry.logits_bits != harness_logits {
            counts.logit_mismatches += 1;
            note_mismatch_v1(
                &mut noted,
                &game.game_id,
                format_args!("logits at step {step} ({seat_name})"),
            );
        }
        let harness_row = match logs[seat].header.selection.as_str() {
            SELECTION_SAMPLED_WIDE_V1 => policy.select_fast_session_v1(&harness)? as usize,
            SELECTION_ARGMAX_FIRST_V1 => first_argmax(&scores.logits),
            other => return Err(format!("unknown selection {other}")),
        };
        if rows.get(harness_row) != Some(&entry.candidate_id) {
            counts.choice_mismatches += 1;
            note_mismatch_v1(
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
        harness
            .step(episode_id, step, harness_row as u32)
            .map_err(|error| error.message)?;
    }
    for (seat, log) in logs.iter().enumerate() {
        if cursor[seat] != log.decisions.len() {
            return Err(format!(
                "{}: {} log longer than game ({} logged, {} played)",
                game.game_id,
                SEATS_V1[seat],
                log.decisions.len(),
                cursor[seat]
            ));
        }
    }
    if response["classification"] != game.classification.as_str()
        || response["outcome"] != game.outcome.as_str()
        || response["step_count"] != game.step_count
    {
        counts.outcome_mismatches += 1;
        note_mismatch_v1(
            &mut noted,
            &game.game_id,
            format_args!(
                "outcome: replay {} {} step_count {}, ledger {:?} {:?} step_count {}",
                response["classification"],
                response["outcome"],
                response["step_count"],
                game.classification,
                game.outcome,
                game.step_count
            ),
        );
    }
    counts.games_replayed += 1;
    let deck = counts.per_deck.entry(game.decks[0].clone()).or_default();
    deck.0 += 1;
    deck.1 += decisions;
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

/// Loads the policy a `--bot` scorer config names, with its model state
/// sha256.
type BotPolicyLoaderV1<'a> = dyn FnMut(&Path) -> Result<(FrozenPlayPolicyV1, String), String> + 'a;

/// The policy a `--bot` scorer config names, and its model state sha256.
fn load_bot_policy_v1(config_path: &Path) -> Result<(FrozenPlayPolicyV1, String), String> {
    let config: SpellbenchScorerConfigV1 = serde_json::from_value(read_json(config_path)?)
        .map_err(|error| format!("{}: {error}", config_path.display()))?;
    let (policy, identity) = load_expanded_inference_v1(&config.source)?;
    Ok((policy, identity.state_sha256))
}

pub fn run_spellbench_qualification_v1(
    args: &SpellbenchQualificationArgsV1,
) -> Result<Value, String> {
    qualify_with_loader_v1(args, &mut load_bot_policy_v1)
}

/// `run_spellbench_qualification_v1` with the policy loader passed in (the
/// tests load the fixture policy).
fn qualify_with_loader_v1(
    args: &SpellbenchQualificationArgsV1,
    load: &mut BotPolicyLoaderV1<'_>,
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
    let mut model_states: HashMap<String, String> = HashMap::new();
    let mut counts = QualificationCountsV1::default();
    for line in ledger.lines() {
        let row: Value = serde_json::from_str(line).map_err(|error| error.to_string())?;
        let game = LedgerGameV1 {
            game_id: row["game_id"].as_str().ok_or("game_id")?.to_string(),
            game_seed: row["game_seed"].as_u64().ok_or("game_seed")?,
            decks: [0, 1].map(|seat| {
                row["decks"][seat]["catalog_id"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string()
            }),
            bot_names: [0, 1].map(|seat| {
                row["seats"][seat]["name"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string()
            }),
            classification: row["classification"]
                .as_str()
                .ok_or("classification")?
                .to_string(),
            outcome: row["outcome"].as_str().ok_or("outcome")?.to_string(),
            step_count: row["step_count"].as_u64().ok_or("step_count")?,
        };
        let paths = ["p0", "p1"].map(|seat| {
            args.decision_log_dir
                .join(format!("{}.{seat}.jsonl", game.game_id))
        });
        let (Some(log0), Some(log1)) = (read_log(&paths[0])?, read_log(&paths[1])?) else {
            counts.games_skipped += 1;
            continue;
        };
        for (seat, log) in [&log0, &log1].into_iter().enumerate() {
            let name = &game.bot_names[seat];
            if let Entry::Vacant(slot) = policies.entry((name.clone(), seat)) {
                let config_path = args
                    .bot_configs
                    .get(name)
                    .ok_or_else(|| format!("no --bot config for {name}"))?;
                let (policy, state) = load(config_path)?;
                model_states.insert(name.clone(), state);
                slot.insert(policy);
            }
            // Every log, not only the one read when its bot's policy loads.
            let state = &model_states[name];
            if log.header.model_state_sha256 != *state {
                return Err(format!(
                    "{}: {name}: log model {} is not config model {state}",
                    game.game_id, log.header.model_state_sha256
                ));
            }
        }
        let mut p0 = policies
            .remove(&(game.bot_names[0].clone(), 0))
            .expect("loaded");
        let mut p1 = policies
            .remove(&(game.bot_names[1].clone(), 1))
            .expect("loaded");
        let result = replay_game_v1(
            &game,
            limits,
            &[log0, log1],
            [&mut p0, &mut p1],
            &mut counts,
        );
        policies.insert((game.bot_names[0].clone(), 0), p0);
        policies.insert((game.bot_names[1].clone(), 1), p1);
        result?;
    }
    let clean = counts.tensor_mismatches
        + counts.logit_mismatches
        + counts.choice_mismatches
        + counts.outcome_mismatches
        == 0;
    Ok(json!({
        "schema": QUALIFICATION_REPORT_SCHEMA_V1,
        "games_replayed": counts.games_replayed, "games_skipped": counts.games_skipped, "decisions": counts.decisions,
        "tensor_mismatches": counts.tensor_mismatches, "logit_mismatches": counts.logit_mismatches,
        "choice_mismatches": counts.choice_mismatches, "outcome_mismatches": counts.outcome_mismatches,
        "per_deck": counts.per_deck.iter().map(|(deck, (games, decisions))| (deck.clone(), json!({"games": games, "decisions": decisions}))).collect::<serde_json::Map<_, _>>(),
        "verdict": if clean && counts.games_replayed > 0 { "PASS" } else { "FAIL" },
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
            deck,
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

    /// `play_logged_game` with the given scorers and bot names. Each log
    /// header takes its selection and model state from its scorer's ready
    /// record; like the bot, only a sampled seat draws from its stream.
    fn play_game_with_scorers(
        mut scorers: [SpellbenchScorerV1; 2],
        bot_names: [&str; 2],
        game_id: &str,
        deck: &str,
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
                seat: ["p0", "p1"][seat].into(),
                bot_name: bot_names[seat].into(),
                bot_version: "1.0.0".into(),
                selection: ready[seat]["selection"].as_str().unwrap().into(),
                stream_seed: stream_seeds[seat],
                model_state_sha256: ready[seat]["model_state_sha256"].as_str().unwrap().into(),
            },
            decisions: Vec::new(),
        });
        let mut server = replay_server();
        let mut response: Value = serde_json::from_str(&server.handle_line(&reset_line(
            game_id,
            [deck, deck],
            seed,
            limits,
            "q-0",
        )))
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
            decks: [deck.into(), deck.into()],
            bot_names: bot_names.map(String::from),
            classification: response["classification"].as_str().unwrap().into(),
            outcome: response["outcome"].as_str().unwrap().into(),
            step_count: response["step_count"].as_u64().unwrap(),
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

    #[test]
    fn a_ledger_outcome_the_replay_does_not_reach_is_counted() {
        let (mut game, logs) = play_logged_game("q-outcome", "Burn", 24, SHORT, [17, 18]);
        game.step_count += 1;
        let mut counts = QualificationCountsV1::default();
        let [mut p0, mut p1] = harness_pair();
        replay_game_v1(&game, SHORT, &logs, [&mut p0, &mut p1], &mut counts).unwrap();
        assert_eq!((counts.games_replayed, counts.outcome_mismatches), (1, 1));
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
                "q-longer: p1 log longer than game ({} logged, {played} played)",
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
    /// scorers report as "0" x 64.
    fn fixture_loader(_config: &Path) -> Result<(FrozenPlayPolicyV1, String), String> {
        Ok((FrozenPlayPolicyV1::training_fixture_v4(), "0".repeat(64)))
    }

    /// Writes a tournament as the arena and `kernel_flat_bot.py` leave it:
    /// `config.json` (the verifier reads the caps), one `matches.jsonl` row
    /// per game (the fields the verifier reads, plus the schema) and, for
    /// games that have them, both seats' decision logs. Every bot's config
    /// path is a placeholder: the tests' loader ignores it.
    fn write_tournament(
        dir: &Path,
        limits: (u64, u64),
        games: &[(&LedgerGameV1, Option<&[BotLogV1; 2]>)],
    ) -> SpellbenchQualificationArgsV1 {
        let (tournament_dir, decision_log_dir) = (dir.join("tournament"), dir.join("logs"));
        std::fs::create_dir_all(&tournament_dir).unwrap();
        std::fs::create_dir_all(&decision_log_dir).unwrap();
        let config = json!({"schema": "spellbench-tournament-config/v1", "max_decisions": limits.0, "max_steps": limits.1});
        std::fs::write(tournament_dir.join("config.json"), format!("{config}\n")).unwrap();
        let mut ledger = String::new();
        for (game, logs) in games {
            let row = json!({"schema": "spellbench-match-ledger/v1", "game_id": game.game_id, "game_seed": game.game_seed,
                "seats": [{"seat": "p0", "name": game.bot_names[0]}, {"seat": "p1", "name": game.bot_names[1]}],
                "decks": [{"catalog_id": game.decks[0]}, {"catalog_id": game.decks[1]}],
                "classification": game.classification, "outcome": game.outcome, "step_count": game.step_count});
            ledger.push_str(&format!("{row}\n"));
            for log in logs.iter().flat_map(|logs| logs.iter()) {
                write_log(&decision_log_dir, log);
            }
        }
        std::fs::write(tournament_dir.join("matches.jsonl"), ledger).unwrap();
        let bot_configs = games
            .iter()
            .flat_map(|(game, _)| game.bot_names.clone())
            .map(|name| (name.clone(), dir.join(format!("{name}.scorer.json"))))
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
        // p1 selects by argmax: its log carries no sample seeds.
        let scorers = [
            fixture_scorer(SELECTION_SAMPLED_WIDE_V1),
            fixture_scorer(SELECTION_ARGMAX_FIRST_V1),
        ];
        let (game, logs) = play_game_with_scorers(
            scorers,
            ["fixture", "fixture-argmax"],
            "q-files",
            "Burn",
            26,
            SHORT,
            [21, 22],
        );
        assert!(logs[1]
            .decisions
            .iter()
            .all(|decision| decision.sample_seed.is_none()));
        // A game whose bots kept no logs (a builtin opponent, say) is skipped.
        let unlogged = LedgerGameV1 {
            game_id: "q-unlogged".into(),
            game_seed: 5,
            decks: ["Spy".into(), "Spy".into()],
            bot_names: ["uniform".into(), "fixture".into()],
            classification: "natural".into(),
            outcome: "p0_win".into(),
            step_count: 9,
        };
        let dir = scratch_dir("files");
        let args = write_tournament(
            &dir.join("both"),
            SHORT,
            &[(&game, Some(&logs)), (&unlogged, None)],
        );
        let decisions = logs[0].decisions.len() + logs[1].decisions.len();
        assert_eq!(
            qualify_with_loader_v1(&args, &mut fixture_loader).unwrap(),
            json!({"schema": "mtg-kernel-spellbench-qualification/v1", "games_replayed": 1, "games_skipped": 1,
                "decisions": decisions, "tensor_mismatches": 0, "logit_mismatches": 0, "choice_mismatches": 0,
                "outcome_mismatches": 0, "per_deck": {"Burn": {"games": 1, "decisions": decisions}}, "verdict": "PASS"})
        );
        // Nothing replayed is no pass.
        let args = write_tournament(&dir.join("none"), SHORT, &[(&unlogged, None)]);
        let report = qualify_with_loader_v1(&args, &mut fixture_loader).unwrap();
        assert_eq!(
            (&report["games_replayed"], &report["verdict"]),
            (&json!(0), &json!("FAIL"))
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn logs_the_verifier_cannot_tie_to_their_model_or_read_are_refused() {
        let (first, first_logs) = play_logged_game("q-refuse-1", "Burn", 27, SHORT, [23, 24]);
        let (second, mut second_logs) = play_logged_game("q-refuse-2", "Burn", 28, SHORT, [25, 26]);
        // Every log's model is checked, not only the one read when its
        // policy is loaded: p1's policy is loaded by the first game.
        second_logs[1].header.model_state_sha256 = "1".repeat(64);
        let dir = scratch_dir("refuse");
        let args = write_tournament(
            &dir.join("model"),
            SHORT,
            &[(&first, Some(&first_logs)), (&second, Some(&second_logs))],
        );
        assert_eq!(
            qualify_with_loader_v1(&args, &mut fixture_loader),
            Err(format!(
                "q-refuse-2: fixture: log model {} is not config model {}",
                "1".repeat(64),
                "0".repeat(64)
            ))
        );
        // A log that exists but cannot be read is an error, not a missing log.
        let args = write_tournament(
            &dir.join("unreadable"),
            SHORT,
            &[(&first, Some(&first_logs))],
        );
        let log = args.decision_log_dir.join("q-refuse-1.p0.jsonl");
        std::fs::remove_file(&log).unwrap();
        std::fs::create_dir(&log).unwrap();
        let error = qualify_with_loader_v1(&args, &mut fixture_loader).unwrap_err();
        assert!(
            error.starts_with(&format!("{}: ", log.display())),
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
        let (game, logs) =
            play_game_with_scorers(scorers, names, "q-real", "Elves", 29, SHORT, [27, 28]);
        let scratch = scratch_dir("real");
        let args = SpellbenchQualificationArgsV1 {
            bot_configs,
            ..write_tournament(&scratch, SHORT, &[(&game, Some(&logs))])
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
