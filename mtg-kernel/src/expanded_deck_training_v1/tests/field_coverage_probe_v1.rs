//! Field coverage probe (measurement only, test only): plays every ordered
//! pair of a deck pool through the FlatV7 encoder the trainer uses, with
//! London pregame rows, and records where the encoder or the session stops.
//!
//! Policy: the V4 fixture transferred to the FlatV7 identity
//! (`training_fixture_v4().into_flat_v7_transfer_v1()`), fixed weights, no
//! checkpoint, so it loads in any catalog build (for example
//! `eternal-fixtures`).
//!
//! Episodes: every ordered pair `(a, b)` of the pool (mirrors included) times
//! `MTG_KERNEL_FIELD_PROBE_SEEDS` seeds times both starting players, London
//! v1 with learned mulligans on both seats, runtime rules
//! `resolution-boundary-v1`, game one only. Episode index
//! `((a * n + b) * seeds + s) * 2 + p`; the seed depends on the index only.
//! The probe refuses to start (a panic before any game) with zero decks,
//! zero seeds or zero episodes, and the summary carries both the expected
//! episode count and the reported game count, so an empty run never reads as
//! a clean one.
//!
//! Pass A drives the session itself (the collector's session, seeds and
//! `select_with_training_tensor_v4`). Every pass A result carries
//! `stop_decision_index`, the number of decisions played before the game
//! ended or stopped (null when it stopped before a session existed). A
//! completed Pass A session records successful score-and-selection calls and
//! successful steps separately. Its digests cover the ordered V6 scoring
//! observations, the canonical JSON bytes from the FlatV7 tensorizer (including
//! London inputs), encoded tensors and scores, and submitted ordinals. A step
//! error includes the final scored row and attempted ordinal in the digests.
//!
//! A selection error is recorded with its decision, the full error text, the
//! full input diagnosis (`diagnose_current_decision_input_v4`), the candidate
//! index the diagnosis implicates when it names one (`at <i>`) with that
//! candidate's semantic in full, the whole action menu's semantics up to
//! 64 KiB per game (`menu_truncated: true` when the cap is hit) and the card
//! names the menu references. Selection errors are classified by the text
//! prefix the scorer puts on each stage (`sideboard_play_policy_v1.rs`,
//! `score_fast_session_v1`, `score_owned`, `sample_scores`):
//!   - `encoder_refusal`: `V4 actor-visible encoding: ` (stage
//!     `action_slice` when the error is `Action(..)`, the flat action slice;
//!     `observation_contract` for `ObservationContract`, which includes the
//!     London pregame facts; `encoding_other` for the remaining
//!     `FlatDecisionErrorV2` kinds), or `V7 visible tensorization: ` and its
//!     V4, V8 and V3 siblings (stage `tensorization`);
//!   - `selection_failure_other`: forward errors (`explicit V7 frozen
//!     feature transfer: ` and siblings, `invalid frozen inference output`,
//!     the no-state-digest check, stage `inference`), the pinned float
//!     environment (`no-state-digest contract: `, stage `environment`),
//!     sampling preconditions (`reset both policy seeds`, `scorer and legal
//!     action counts differ`, stage `sampling`), and policy or session state
//!     preconditions;
//!   - `unknown`: any other text (for example a sampler error, which has no
//!     prefix), never guessed into either class above.
//!
//! Pass B runs the same episode through `collect_episode`, the trainer's own
//! collection with trajectory validation. On natural completion it hashes
//! the ordered decision records returned by the collector. On other results
//! the collector does not return any records. It returns natural, non-natural
//! with its terminal, or `other` with the trainer's text; it carries no stop
//! index for `other`. The row's `comparison` therefore states exactly what
//! was compared: `outcome_agreement` (same outcome class and, for terminals,
//! the same terminal record; for stops, the same error text prefix up to the
//! first `:` or `;`), `error_text_equal` (full text, when both passes
//! stopped on an error) and `stop_index_agreement`: `compared_equal` or
//! `compared_different` only when both passes reached a terminal (policy
//! steps, physical decisions and, when natural, the trajectory length are
//! compared), otherwise `unverified`. `outcome_agreement` is null when either
//! pass panicked. In uniform mode pass A encodes every decision through the
//! same call, then plays a uniform legal action from a SplitMix64 stream of
//! the episode seed; pass B cannot follow those choices, so it is not run and
//! `comparison` is null.
//!
//! Panics: each pass of each game runs under `catch_unwind`. A panic is
//! recorded on that game as class `panic` with its pass (`a` or `b`) and
//! message, the worker's policy is replaced by a fresh fork, and the probe
//! keeps going. After every game row and the summary are printed, the test
//! FAILS if any game panicked.
//!
//! Output: one JSON line per game, sorted by episode index, then a summary
//! line with the SHA-256 of the per-game lines (joined by newlines). No
//! timing in per-game lines, so they compare byte for byte across thread
//! counts. Summary `stops` keys group games by class, stage, decision kind
//! and the error text before `; decision=` with digits folded; the full
//! evidence is in the game rows.
//!
//! Environment:
//!   MTG_KERNEL_PROBE_DECKS          JSON list of {label, mainboard, sideboard},
//!                                   rows [count, name] (required)
//!   MTG_KERNEL_FIELD_PROBE_SEEDS    seeds per ordered pair and seat (default 2)
//!   MTG_KERNEL_FIELD_PROBE_THREADS  worker threads (default 1)
//!   MTG_KERNEL_FIELD_PROBE_SELECT   `policy` (default) or `uniform`

use super::*;
use crate::sideboard_play_policy_v1::public_inputs::last_flat_v7_observation_for_test;
use std::collections::BTreeMap;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::time::Instant;

const MAX_PHYSICAL_DECISIONS: u64 = 100_000;
const MAX_POLICY_STEPS: u64 = 200_000;
const SEED_BASE: u64 = 0x5eed_f1e1_d000_0000;
const UNIFORM_STREAM: u64 = 0x756e_6966_6f72_6d00;
/// Bytes of action-menu semantics printed for one failing decision.
const MENU_BYTE_CAP: usize = 64 * 1024;
/// The trainer's text for the policy step cap (`collect_episode`).
const POLICY_STEP_LIMIT_TEXT: &str = "policy step limit";

/// Encoder and tensorizer error prefixes (`score_fast_session_v1`,
/// `score_owned`) and whether the stage is tensorization.
const ENCODER_PREFIXES: &[(&str, bool)] = &[
    ("V4 actor-visible encoding: ", false),
    ("V3 actor-visible encoding: ", false),
    ("actor-visible encoding: ", false),
    ("V8 visible tensorization: ", true),
    ("V7 visible tensorization: ", true),
    ("V4 visible tensorization: ", true),
    ("V3 visible tensorization: ", true),
    ("visible tensorization: ", true),
];

/// Non-encoder selection error prefixes and their stage.
const OTHER_SELECTION_PREFIXES: &[(&str, &str)] = &[
    ("explicit V3 frozen feature transfer: ", "inference"),
    ("explicit V4 frozen feature transfer: ", "inference"),
    ("explicit V7 frozen feature transfer: ", "inference"),
    ("explicit V8 inclusion forward: ", "inference"),
    ("frozen scalar inference: ", "inference"),
    ("invalid frozen inference output", "inference"),
    ("no-state-digest check: ", "inference"),
    ("no-state-digest contract: ", "environment"),
    ("reset both policy seeds before a game", "sampling"),
    ("scorer and legal action counts differ", "sampling"),
    (
        "training requires explicit V4 successor features",
        "policy_state",
    ),
    ("missing fresh successor tensor", "policy_state"),
    ("cannot collect a terminal decision", "session_state"),
    ("cannot score a terminal session", "session_state"),
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Select {
    Policy,
    Uniform,
}

fn env_usize(name: &str, default: usize) -> usize {
    std::env::var(name).map_or(default, |v| {
        v.parse()
            .unwrap_or_else(|_| panic!("{name} must be a count"))
    })
}

/// (class, stage) of a selection error, from its stage prefix.
fn classify_selection_error(error: &str) -> (&'static str, &'static str) {
    for &(prefix, tensorization) in ENCODER_PREFIXES {
        if let Some(rest) = error.strip_prefix(prefix) {
            let stage = if tensorization {
                "tensorization"
            } else if rest.starts_with("Action(") {
                "action_slice"
            } else if rest.starts_with("ObservationContract") {
                "observation_contract"
            } else {
                "encoding_other"
            };
            return ("encoder_refusal", stage);
        }
    }
    for &(prefix, stage) in OTHER_SELECTION_PREFIXES {
        if error.starts_with(prefix) {
            return ("selection_failure_other", stage);
        }
    }
    ("unknown", "unknown")
}

/// The text before the first `:` or `;`, trimmed: the stage an error names.
fn error_prefix(error: &str) -> &str {
    error.split([':', ';']).next().unwrap_or("").trim()
}

/// The candidate index a V4 input diagnosis names (`at <i>:` or
/// `at <i> role`), if any.
fn implicated_candidate(diagnosis: &str) -> Option<usize> {
    for (position, _) in diagnosis.match_indices(" at ") {
        let rest = &diagnosis[position + " at ".len()..];
        let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
        let tail = &rest[digits.len()..];
        if !digits.is_empty() && (tail.starts_with(':') || tail.starts_with(" role ")) {
            if let Ok(index) = digits.parse() {
                return Some(index);
            }
        }
    }
    None
}

/// The menu's semantics in order, while their total stays within
/// `MENU_BYTE_CAP`, and whether any were left out.
fn capped_menu(texts: &[String]) -> (Vec<&str>, bool) {
    let mut used = 0usize;
    let mut shown = Vec::new();
    for text in texts {
        if used + text.len() > MENU_BYTE_CAP {
            return (shown, true);
        }
        used += text.len();
        shown.push(text.as_str());
    }
    (shown, false)
}

fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(text) = payload.downcast_ref::<&str>() {
        (*text).to_owned()
    } else if let Some(text) = payload.downcast_ref::<String>() {
        text.clone()
    } else {
        "non-string panic payload".to_owned()
    }
}

/// The deck pool, every deck admitted by the trainer's registration rule or
/// the probe stops before any game.
fn load_pool() -> Vec<ExpandedDeckListV1> {
    let path = std::env::var("MTG_KERNEL_PROBE_DECKS").expect("set MTG_KERNEL_PROBE_DECKS");
    let decks: Vec<Value> = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert!(
        !decks.is_empty(),
        "MTG_KERNEL_PROBE_DECKS lists no decks: refusing an empty run"
    );
    let mut lists = Vec::new();
    let mut refused = Vec::new();
    for deck in &decks {
        let label = deck["label"].as_str().unwrap().to_owned();
        let mut unknown = Vec::new();
        let mut ids = |rows: &Value| -> Vec<u16> {
            let mut out = Vec::new();
            for row in rows.as_array().unwrap() {
                let name = row[1].as_str().unwrap();
                let count = row[0].as_u64().unwrap() as usize;
                match crate::card_def::card_id_by_name(name) {
                    Some(id) => out.extend(std::iter::repeat_n(id, count)),
                    None => unknown.push(name.to_owned()),
                }
            }
            out
        };
        let mainboard = ids(&deck["mainboard"]);
        let sideboard = ids(&deck["sideboard"]);
        if !unknown.is_empty() {
            refused.push(json!({"label": label, "reason": "unknown card names", "cards": unknown}));
            continue;
        }
        match crate::sideboard::RegisteredDeckV1::new_executable_v1(
            &label,
            mainboard.clone(),
            sideboard.clone(),
        ) {
            Ok(_) => lists.push(ExpandedDeckListV1 {
                label,
                mainboard,
                sideboard,
            }),
            Err(error) => refused.push(json!({"label": label, "reason": format!("{error:?}")})),
        }
    }
    println!(
        "{}",
        json!({"probe": "field_coverage_deck_pool", "decks": decks.len(),
            "executable": lists.len(), "refused": refused})
    );
    assert!(refused.is_empty(), "every pool deck must be executable");
    assert!(lists.len() >= 2, "too few decks");
    lists
}

fn episodes(lists: &[ExpandedDeckListV1], seeds: usize) -> Vec<ExpandedEpisodeV1> {
    let n = lists.len();
    let mut out = Vec::with_capacity(n * n * seeds * 2);
    for a in 0..n {
        for b in 0..n {
            for _ in 0..seeds {
                for p in 0..2u8 {
                    let index = out.len();
                    out.push(ExpandedEpisodeV1 {
                        id: format!("field-coverage-{index}"),
                        seed: SEED_BASE.wrapping_add(index as u64),
                        starting_player: p,
                        learner_seat: 0,
                        opponent: None,
                        opponent_kind: None,
                        opponent_search: None,
                        registered: [lists[a].clone(), lists[b].clone()],
                        selected: [lists[a].clone(), lists[b].clone()],
                        postboard: false,
                        max_physical_decisions: MAX_PHYSICAL_DECISIONS,
                        max_policy_steps: MAX_POLICY_STEPS,
                        runtime_rules: Some(EpisodeRuntimeRulesV1::ResolutionBoundaryV1),
                        reset: None,
                        opening: Some(PauperOpeningV1::LondonV1),
                        mulligan_modes: Some([
                            PauperMulliganModeV1::Learned,
                            PauperMulliganModeV1::Learned,
                        ]),
                    });
                }
            }
        }
    }
    out
}

/// Card names of every object the semantics reference (`arena_id: N` in
/// their debug text), looked up in the live game state.
fn referenced_cards(session: &FastActorSessionV1, text: &str) -> Vec<String> {
    let state = session.game_state();
    let mut names = BTreeSet::new();
    for (position, _) in text.match_indices("arena_id: ") {
        let digits: String = text[position + "arena_id: ".len()..]
            .chars()
            .take_while(char::is_ascii_digit)
            .collect();
        if let Ok(id) = digits.parse::<u32>() {
            if let Some(object) = state.objects.try_get(crate::ids::ObjectId(id)) {
                if let Some(def) = crate::card_def::CARD_DEFS.get(usize::from(object.card_def)) {
                    names.insert(def.name.to_owned());
                }
            }
        }
    }
    names.into_iter().collect()
}

fn terminal_json(terminal: &RlSessionTerminalV1) -> Value {
    json!({
        "classification": format!("{:?}", terminal.terminal_classification),
        "reason": terminal.terminal_reason,
        "winner": format!("{:?}", terminal.winner),
        "policy_steps": terminal.policy_step_count,
        "physical_decisions": terminal.physical_decision_count,
    })
}

/// Compact Serde JSON plus LF for each record, in the order encountered.
fn hash_json_record(hash: &mut Sha256, record: &impl Serialize) {
    hash.update(serde_json::to_vec(record).expect("probe digest record serializes"));
    hash.update(b"\n");
}

/// Pass A: the collector's session, seeds and selection call, driven here
/// so a stop can be diagnosed on the live decision.
fn pass_a(policy: &mut FrozenPlayPolicyV1, episode: &ExpandedEpisodeV1, select: Select) -> Value {
    let configs = match episode.configurations() {
        Ok(configs) => configs,
        Err(error) => {
            return json!({"class": "configuration_refusal", "stop_decision_index": null,
                "error": error})
        }
    };
    if let Err(error) = require_opening_feature_identity_v1(episode, &[Some(&*policy)]) {
        return json!({"class": "configuration_refusal", "stop_decision_index": null,
            "error": error});
    }
    let mut session = match episode_session_v1(episode, &configs) {
        Ok(session) => session,
        Err(error) => {
            return json!({"class": "session_refusal", "stop_decision_index": null,
                "error": error})
        }
    };
    policy.reset_sampling_v1(paired_policy_seeds_v1(episode.seed));
    let mut uniform = SplitMix64::seed(episode.seed ^ UNIFORM_STREAM);
    let mut decisions = 0u64;
    let mut kinds: BTreeMap<String, u64> = BTreeMap::new();
    // Preserve the original input digest: decision index, session state
    // hash, and round-trip Debug text for the tensor and scores.
    let mut inputs = Sha256::new();
    let mut observations = Sha256::new();
    let mut canonical_observations = Sha256::new();
    let mut selected_ordinals = Sha256::new();
    let mut scored = 0u64;
    let mut result = loop {
        let d = match session.current_response() {
            FastActorResponseV1::Terminal(terminal) => {
                let class = if terminal.terminal_classification == TerminalClassificationV1::Natural
                {
                    "natural"
                } else {
                    "non_natural"
                };
                break json!({"class": class, "stop_decision_index": decisions,
                    "decisions": decisions, "kinds": kinds,
                    "terminal": terminal_json(&terminal)});
            }
            FastActorResponseV1::Decision(d) => d,
        };
        let kind = format!("{:?}", d.decision_kind);
        if decisions >= episode.max_policy_steps {
            break json!({"class": "policy_step_limit", "stop_decision_index": decisions,
                "decisions": decisions, "kinds": kinds, "error": POLICY_STEP_LIMIT_TEXT});
        }
        let selected = match policy.select_with_training_tensor_v4(&session) {
            Ok((selected, scores, tensor)) => {
                inputs.update(decisions.to_le_bytes());
                inputs.update(session.diagnostic_state_hash().to_le_bytes());
                inputs.update(format!("{tensor:?}").as_bytes());
                inputs.update(format!("{scores:?}").as_bytes());
                match select {
                    Select::Policy => selected,
                    Select::Uniform => {
                        (uniform.next_u64() % u64::from(d.legal_action_count)) as u32
                    }
                }
            }
            Err(error) => {
                let (class, stage) = classify_selection_error(&error);
                let texts: Vec<String> = session
                    .diagnostic_current_action_semantics()
                    .iter()
                    .flatten()
                    .map(|semantic| format!("{semantic:?}"))
                    .collect();
                let cards = referenced_cards(&session, &texts.join(" "));
                let diagnosis = session.diagnose_current_decision_input_v4(d);
                let implicated = implicated_candidate(&diagnosis);
                let implicated_semantic = implicated.and_then(|i| texts.get(i).cloned());
                let (menu, menu_truncated) = capped_menu(&texts);
                break json!({
                    "class": class,
                    "stage": stage,
                    "stop_decision_index": decisions,
                    "decisions": decisions,
                    "kinds": kinds,
                    "decision_kind": kind,
                    "acting_player": format!("{:?}", d.acting_player),
                    "legal_action_count": d.legal_action_count,
                    "physical_decision_id": d.physical_decision_id,
                    "error": error,
                    "diagnosis": diagnosis,
                    "implicated_candidate_index": implicated,
                    "implicated_candidate_semantic": implicated_semantic,
                    "menu_size": texts.len(),
                    "menu_byte_cap": MENU_BYTE_CAP,
                    "menu_truncated": menu_truncated,
                    "menu": menu,
                    "cards": cards,
                });
            }
        };
        // The same V6 getter as build_scoring_owned_v4, on the same decision
        // binding immediately after select and before the session advances.
        let observation = session
            .flat_policy_observation_v4(d)
            .expect("successfully scored decision still has its scoring observation");
        hash_json_record(&mut observations, &observation);
        canonical_observations.update(last_flat_v7_observation_for_test(policy));
        canonical_observations.update(b"\n");
        selected_ordinals.update(selected.to_le_bytes());
        scored += 1;
        *kinds.entry(kind.clone()).or_default() += 1;
        if let Err(error) = session.step(d.episode_id, d.step, selected) {
            break json!({"class": "step_error", "stop_decision_index": decisions,
                "decisions": decisions, "kinds": kinds, "decision_kind": kind,
                "error": error.to_string()});
        }
        decisions += 1;
    };
    result["inputs_sha256"] = json!(hex(&inputs.finalize()));
    result["scoring_observations_sha256"] = json!(hex(&observations.finalize()));
    result["canonical_observations_sha256"] = json!(hex(&canonical_observations.finalize()));
    result["selected_ordinals_sha256"] = json!(hex(&selected_ordinals.finalize()));
    result["scored_decisions"] = json!(scored);
    result["stepped_decisions"] = json!(decisions);
    result
}

/// Pass B: the trainer's collection of the same episode.
fn pass_b(
    policy: &mut FrozenPlayPolicyV1,
    learner: &ExpandedSeatBehaviorV1,
    episode: &ExpandedEpisodeV1,
) -> Value {
    match collect_episode(policy, learner, None, episode) {
        Ok(trajectory) => {
            let mut records = Sha256::new();
            for record in &trajectory.decisions {
                hash_json_record(&mut records, record);
            }
            json!({"class": "natural", "decisions": trajectory.decisions.len(),
                "decision_records_sha256": hex(&records.finalize()),
                "terminal": terminal_json(&trajectory.terminal)})
        }
        Err(CollectEpisodeErrorV1::NonNatural(terminal)) => {
            json!({"class": "non_natural", "terminal": terminal_json(&terminal)})
        }
        Err(CollectEpisodeErrorV1::Other(error)) => {
            json!({"class": "other", "error_prefix": error_prefix(&error), "error": error})
        }
    }
}

/// What pass A and pass B can be compared on (see the module doc).
fn compare(a: &Value, b: &Value) -> Value {
    let a_class = a["class"].as_str().unwrap_or("");
    let b_class = b["class"].as_str().unwrap_or("");
    let same_stop = |x: &Value, y: &Value| {
        x["terminal"]["policy_steps"] == y["terminal"]["policy_steps"]
            && x["terminal"]["physical_decisions"] == y["terminal"]["physical_decisions"]
    };
    let stop_word = |equal: bool| {
        if equal {
            "compared_equal"
        } else {
            "compared_different"
        }
    };
    if a_class == "panic" || b_class == "panic" {
        return json!({"outcome_agreement": null, "error_text_equal": null,
            "stop_index_agreement": "unverified"});
    }
    match (a_class, b_class) {
        ("natural", "natural") => {
            let stops = same_stop(a, b) && a["decisions"] == b["decisions"];
            json!({"outcome_agreement": stops && a["terminal"] == b["terminal"],
                "error_text_equal": null, "stop_index_agreement": stop_word(stops)})
        }
        ("non_natural", "non_natural") => {
            let stops = same_stop(a, b);
            json!({"outcome_agreement": stops && a["terminal"] == b["terminal"],
                "error_text_equal": null, "stop_index_agreement": stop_word(stops)})
        }
        (_, "other") if a["error"].is_string() => {
            let a_error = a["error"].as_str().unwrap_or("");
            let b_error = b["error"].as_str().unwrap_or("");
            json!({"outcome_agreement": error_prefix(a_error) == error_prefix(b_error),
                "error_text_equal": a_error == b_error,
                "stop_index_agreement": "unverified"})
        }
        _ => json!({"outcome_agreement": false, "error_text_equal": null,
            "stop_index_agreement": "unverified"}),
    }
}

/// Summary grouping key of one pass's stop, or None for a natural game.
fn stop_key(pass: &Value) -> Option<String> {
    let class = pass["class"].as_str().unwrap_or("");
    let fold = |text: &str| -> String {
        text.split("; decision=")
            .next()
            .unwrap_or("")
            .chars()
            .map(|c| if c.is_ascii_digit() { '#' } else { c })
            .collect()
    };
    match class {
        "natural" | "not_run" => None,
        "non_natural" => Some(format!(
            "non_natural | {}",
            pass["terminal"]["reason"].as_str().unwrap_or("")
        )),
        "policy_step_limit" => Some("policy_step_limit".to_owned()),
        "panic" => Some(format!(
            "panic | pass {} | {}",
            pass["pass"].as_str().unwrap_or(""),
            fold(pass["message"].as_str().unwrap_or(""))
        )),
        _ => Some(format!(
            "{class} | {} | {} | {}",
            pass["stage"].as_str().unwrap_or(""),
            pass["decision_kind"].as_str().unwrap_or(""),
            fold(pass["error"].as_str().unwrap_or(""))
        )),
    }
}

#[test]
#[ignore = "measurement only: set MTG_KERNEL_PROBE_DECKS"]
fn field_coverage_probe_v1() {
    let select = match std::env::var("MTG_KERNEL_FIELD_PROBE_SELECT").as_deref() {
        Err(_) | Ok("policy") => Select::Policy,
        Ok("uniform") => Select::Uniform,
        Ok(other) => panic!("MTG_KERNEL_FIELD_PROBE_SELECT must be policy or uniform, not {other}"),
    };
    let seeds = env_usize("MTG_KERNEL_FIELD_PROBE_SEEDS", 2);
    assert!(
        seeds > 0,
        "MTG_KERNEL_FIELD_PROBE_SEEDS must be at least 1: zero seeds would run no game"
    );
    let threads = env_usize("MTG_KERNEL_FIELD_PROBE_THREADS", 1).max(1);
    let lists = load_pool();
    let episodes = episodes(&lists, seeds);
    assert!(
        !episodes.is_empty(),
        "no episodes to run: refusing an empty run"
    );
    let base = FrozenPlayPolicyV1::training_fixture_v4()
        .into_flat_v7_transfer_v1()
        .unwrap();
    let select_name = if select == Select::Policy {
        "policy"
    } else {
        "uniform"
    };
    let started = Instant::now();
    let mut rows: Vec<(usize, Value)> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..threads)
            .map(|t| {
                let mut policy = base.fork_for_collection_v3().unwrap();
                let spare = base.fork_for_collection_v3().unwrap();
                let episodes = &episodes;
                scope.spawn(move || {
                    let learner = test_behavior(&policy, false);
                    let mut out = Vec::new();
                    for index in (t..episodes.len()).step_by(threads) {
                        let episode = &episodes[index];
                        let result_a =
                            catch_unwind(AssertUnwindSafe(|| pass_a(&mut policy, episode, select)));
                        let a = match result_a {
                            Ok(a) => a,
                            Err(payload) => {
                                policy = spare.fork_for_collection_v3().unwrap();
                                json!({"class": "panic", "pass": "a",
                                    "message": panic_message(&*payload)})
                            }
                        };
                        let (b, comparison) = match select {
                            Select::Policy => {
                                let result_b = catch_unwind(AssertUnwindSafe(|| {
                                    pass_b(&mut policy, &learner, episode)
                                }));
                                let b = match result_b {
                                    Ok(b) => b,
                                    Err(payload) => {
                                        policy = spare.fork_for_collection_v3().unwrap();
                                        json!({"class": "panic", "pass": "b",
                                            "message": panic_message(&*payload)})
                                    }
                                };
                                let comparison = compare(&a, &b);
                                (b, comparison)
                            }
                            Select::Uniform => (json!({"class": "not_run"}), Value::Null),
                        };
                        let panicked = a["class"] == "panic" || b["class"] == "panic";
                        out.push((
                            index,
                            json!({
                                "probe": "field_coverage_game",
                                "index": index,
                                "episode_id": episode.id,
                                "decks": [episode.selected[0].label, episode.selected[1].label],
                                "seed": episode.seed,
                                "starting_player": episode.starting_player,
                                "select": select_name,
                                "panicked": panicked,
                                "pass_a": a,
                                "pass_b": b,
                                "comparison": comparison,
                            }),
                        ));
                    }
                    out
                })
            })
            .collect();
        handles
            .into_iter()
            .flat_map(|h| h.join().unwrap())
            .collect()
    });
    rows.sort_by_key(|(index, _)| *index);
    let mut hasher = Sha256::new();
    let mut classes_a: BTreeMap<String, u64> = BTreeMap::new();
    let mut classes_b: BTreeMap<String, u64> = BTreeMap::new();
    let mut refusals: BTreeMap<String, (u64, BTreeSet<String>, BTreeSet<String>)> = BTreeMap::new();
    let mut pairs: BTreeMap<String, [u64; 2]> = BTreeMap::new();
    let mut disagreements = 0u64;
    let mut not_compared = 0u64;
    let mut stop_index_unverified = 0u64;
    let mut panicked_games: Vec<usize> = Vec::new();
    for (position, (index, row)) in rows.iter().enumerate() {
        let line = row.to_string();
        println!("{line}");
        if position > 0 {
            hasher.update(b"\n");
        }
        hasher.update(line.as_bytes());
        let a = &row["pass_a"];
        let b = &row["pass_b"];
        *classes_a
            .entry(a["class"].as_str().unwrap_or("").to_owned())
            .or_default() += 1;
        *classes_b
            .entry(b["class"].as_str().unwrap_or("").to_owned())
            .or_default() += 1;
        let comparison = &row["comparison"];
        if !comparison.is_null() {
            match comparison["outcome_agreement"].as_bool() {
                Some(false) => disagreements += 1,
                Some(true) => {}
                None => not_compared += 1,
            }
            if comparison["stop_index_agreement"] == "unverified" {
                stop_index_unverified += 1;
            }
        }
        if row["panicked"] == Value::Bool(true) {
            panicked_games.push(*index);
        }
        let pair = format!(
            "{} vs {}",
            row["decks"][0].as_str().unwrap_or(""),
            row["decks"][1].as_str().unwrap_or("")
        );
        pairs.entry(pair.clone()).or_default()
            [row["starting_player"].as_u64().unwrap_or(0) as usize] += 1;
        let pass_b_panic = (b["class"] == "panic").then_some(b);
        for pass in std::iter::once(a).chain(pass_b_panic) {
            let Some(key) = stop_key(pass) else {
                continue;
            };
            let entry = refusals.entry(key).or_default();
            entry.0 += 1;
            entry.1.insert(pair.clone());
            for card in pass["cards"].as_array().into_iter().flatten() {
                entry.2.insert(card.as_str().unwrap_or("").to_owned());
            }
        }
    }
    let stops: Vec<Value> = refusals
        .into_iter()
        .map(|(key, (games, pairs, cards))| {
            json!({"key": key, "games": games, "pairs": pairs, "cards": cards})
        })
        .collect();
    println!(
        "{}",
        json!({
            "probe": "field_coverage_summary",
            "decks": lists.iter().map(|l| l.label.clone()).collect::<Vec<_>>(),
            "episodes_expected": episodes.len(),
            "games": rows.len(),
            "seeds": seeds,
            "threads": threads,
            "select": select_name,
            "runtime_rules": "resolution-boundary-v1",
            "opening": "london-v1",
            "feature_contract_digest": base.feature_identity_v1().feature_contract_digest,
            "card_db_hash": format!("{KERNEL_CARDDB_HASH:016x}"),
            // Source-bound identities of this build, recorded so a change
            // that moves them is visible next to the per-game digests.
            "mixed_policy_feature_identity": crate::current_mixed_policy_v1::feature_identity(),
            "tree_learner_feature_identity_v1":
                crate::current_tree_learner_v1::feature_identity_v1(),
            "pass_a_classes": classes_a,
            "pass_b_classes": classes_b,
            "disagreements": disagreements,
            "not_compared": not_compared,
            "stop_index_unverified": stop_index_unverified,
            "panics": panicked_games.len(),
            "panicked_games": panicked_games,
            "ordered_pairs_reached": pairs.len(),
            "pairs_missing_a_seat": pairs.iter().filter(|(_, seats)| seats.contains(&0)).count(),
            "stops": stops,
            "games_sha256": hex(&hasher.finalize()),
            "seconds": (started.elapsed().as_secs_f64() * 1000.0).round() / 1000.0,
        })
    );
    assert_eq!(rows.len(), episodes.len(), "every episode reports");
    assert!(
        panicked_games.is_empty(),
        "{} game(s) panicked: {:?}",
        panicked_games.len(),
        panicked_games
    );
}
