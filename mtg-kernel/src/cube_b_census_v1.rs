//! The cube B-run halt census (declarations A2 and B): one record per game of
//! uniform-random play under the A2.1 caps, in logical order.
//!
//! - Drafted games: draft `i` is an eight-seat draft of [`UniformPicker`]s
//!   seeded `seed([namespace, "draft", i])`, the [`StatedRuleDeckBuilder`]
//!   deck of every seat, then the 28-game round robin with pod seed
//!   `seed([namespace, "pod", i])`, each game seeded by [`game_seed`] as in
//!   every pod. Logical index `28 i + game`.
//! - Dealt games: each episode of a `tools/cube_v1/deal.py` schedule plays
//!   its two selected decks once, in schedule order, from the episode's
//!   `seed` ([`play_seeded_game_tallied`]). Logical index: the episode's
//!   position in the schedule.
//!
//! [`seed`] is `deal.py`'s `seed`. [`stop_kind`] ports the stop-kind
//! normaliser of the earlier census (`census_summary.py`, `cause_of`); the
//! summary script recomputes it from the raw end and is the reference.
//!
//! Measurement only. The pickers and the deck rule are test fixtures and
//! reference baselines, never an opponent or teacher in training.
//!
//! [`UniformPicker`]: crate::cube_draft_v1::UniformPicker

use crate::card_def::CARD_DEFS;
use crate::cube_draft_v1::{
    derive_seed, game_seed, play_pod_game_tallied, play_seeded_game_tallied, pod_game_seats,
    uniform_draft_and_build, CubeDraftError, CubePool, Deck, GameCaps, GameEnd, GameRecord,
    GameTally, SeedStream, POD_GAMES, SEATS,
};
use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fmt::Write as _;

/// Schema tag of a [`GameLine`].
pub const GAME_LINE_SCHEMA: &str = "cube_b_census_game/v1";
/// Schema tag of the run record the census example writes.
pub const RUN_RECORD_SCHEMA: &str = "cube_b_census_run/v1";
/// Games per drafted draft: one round-robin pod.
pub const GAMES_PER_DRAFT: u64 = POD_GAMES as u64;
/// The A2.1 caps: driver decisions, physical decisions, policy steps.
pub const A2_CAPS: GameCaps = GameCaps {
    max_decisions: 16_384,
    max_physical_decisions: 16_384,
    max_policy_steps: 32_768,
};
/// The Windows exit code of a stack overflow (`0xC00000FD`).
pub const STACK_OVERFLOW_EXIT_CODE: u32 = 0xC000_00FD;
/// Characters of a panic or session error message the stop kind keeps.
const MESSAGE_CHARS: usize = 160;
const SEED_MASK: u64 = (1 << 63) - 1;

// ---------------------------------------------------------------------------
// Seeds
// ---------------------------------------------------------------------------

/// One element of a seed tuple: a string or a non-negative integer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeedPart<'a> {
    Str(&'a str),
    Int(u64),
}

fn push_python_json_string(out: &mut String, text: &str) {
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            ' '..='~' => out.push(c),
            _ => {
                let mut units = [0u16; 2];
                for unit in c.encode_utf16(&mut units) {
                    write!(out, "\\u{unit:04x}").expect("writing to a String");
                }
            }
        }
    }
    out.push('"');
}

/// The text Python's `json.dumps(parts, separators=(",", ":"))` writes for
/// the list: no spaces, strings ASCII-escaped as `ensure_ascii` does (`\"`,
/// `\\`, `\b`, `\f`, `\n`, `\r`, `\t`, and `\uXXXX` in lower-case hex, by
/// UTF-16 unit, for every other character outside space to `~`).
pub fn compact_json_list(parts: &[SeedPart<'_>]) -> String {
    let mut out = String::from("[");
    for (index, part) in parts.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        match part {
            SeedPart::Str(text) => push_python_json_string(&mut out, text),
            SeedPart::Int(value) => write!(out, "{value}").expect("writing to a String"),
        }
    }
    out.push(']');
    out
}

/// `tools/cube_v1/deal.py`'s `seed`: SHA-256 of [`compact_json_list`], the
/// first 8 bytes read little-endian, masked to 63 bits.
pub fn seed(parts: &[SeedPart<'_>]) -> u64 {
    let digest = Sha256::digest(compact_json_list(parts).as_bytes());
    let mut first = [0u8; 8];
    first.copy_from_slice(&digest[..8]);
    u64::from_le_bytes(first) & SEED_MASK
}

/// Draft `i`'s seed: `seed([namespace, "draft", i])` (B.3).
pub fn draft_seed(namespace: &str, draft: u64) -> u64 {
    seed(&[
        SeedPart::Str(namespace),
        SeedPart::Str("draft"),
        SeedPart::Int(draft),
    ])
}

/// Pod `i`'s seed: `seed([namespace, "pod", i])` (decision 4 on r3).
pub fn pod_seed(namespace: &str, draft: u64) -> u64 {
    seed(&[
        SeedPart::Str(namespace),
        SeedPart::Str("pod"),
        SeedPart::Int(draft),
    ])
}

// ---------------------------------------------------------------------------
// Stop kinds
// ---------------------------------------------------------------------------

fn first_chars(text: &str, count: usize) -> &str {
    match text.char_indices().nth(count) {
        Some((end, _)) => &text[..end],
        None => text,
    }
}

/// `re.sub(r"source:\d+", "source:*", text)`, for ASCII digits. (Python's
/// `\d` also matches other Unicode decimal digits; the summary script
/// normalises with Python's own pattern and reports any difference.)
fn mask_source_numbers(text: &str) -> String {
    const KEY: &str = "source:";
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find(KEY) {
        let after = &rest[at + KEY.len()..];
        let digits = after.bytes().take_while(u8::is_ascii_digit).count();
        out.push_str(&rest[..at + KEY.len()]);
        if digits > 0 {
            out.push('*');
        }
        rest = &after[digits..];
    }
    out.push_str(rest);
    out
}

/// The stop kind of a game end (A2 P1): `None` for a completed game
/// (`natural_game_over`), else the cause string as `census_summary.py`'s
/// `cause_of` normalises it.
pub fn stop_kind(end: &GameEnd) -> Option<String> {
    match end {
        GameEnd::Natural { .. } => None,
        GameEnd::Terminal { reason, .. } => Some(mask_source_numbers(reason)),
        GameEnd::SessionError { code, message } => Some(format!(
            "session error {code}: {}",
            first_chars(message, MESSAGE_CHARS)
        )),
        GameEnd::DecisionCap => Some("driver_decision_cap".to_owned()),
        GameEnd::Panic { message } => {
            Some(format!("panic: {}", first_chars(message, MESSAGE_CHARS)))
        }
    }
}

/// How a worker process ended without exiting cleanly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessDeath {
    /// An exit code (Windows codes as unsigned 32-bit values).
    Exit(u32),
    /// A Unix signal.
    Signal(i32),
}

impl ProcessDeath {
    /// The stop kind, as the earlier census named process deaths.
    pub fn stop_kind(self) -> String {
        match self {
            ProcessDeath::Exit(STACK_OVERFLOW_EXIT_CODE) => {
                "process dies: stack overflow (0xC00000FD)".to_owned()
            }
            ProcessDeath::Exit(code) => format!("process dies: exit {code}"),
            ProcessDeath::Signal(signal) => format!("process dies: signal {signal}"),
        }
    }

    fn end(self) -> Value {
        match self {
            ProcessDeath::Exit(code) => json!({"kind": "process_died", "exit_code": code}),
            ProcessDeath::Signal(signal) => json!({"kind": "process_died", "signal": signal}),
        }
    }
}

// ---------------------------------------------------------------------------
// Records
// ---------------------------------------------------------------------------

/// One game of the census, as one JSONL line. Fields that do not apply to
/// the mode, or that the game never reached, are `null`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GameLine {
    pub schema: &'static str,
    /// `drafted` or `dealt`.
    pub mode: &'static str,
    /// Logical index: `28 draft + game` (drafted) or the episode's position
    /// in the schedule (dealt).
    pub index: u64,
    /// Draft index (drafted).
    pub draft: Option<u64>,
    /// Pod index (drafted; one pod per draft, so equal to the draft).
    pub pod: Option<u64>,
    /// Pod game index (drafted) or the episode's slot in its iteration
    /// (dealt).
    pub game: u64,
    /// Iteration (update) of the episode in the schedule (dealt).
    pub iteration: Option<u64>,
    /// The episode's `id` (dealt).
    pub episode_id: Option<String>,
    /// The episode's `starting_player` (dealt): always 0, since the session
    /// starts p0 (`selected[0]`); a schedule saying otherwise is refused.
    pub starting_player: Option<u64>,
    /// The episode's `learner_seat` (dealt).
    pub learner_seat: Option<u64>,
    pub draft_seed: Option<u64>,
    pub pod_seed: Option<u64>,
    /// The game seed: [`game_seed`]`(pod_seed, game)` (drafted) or the
    /// episode's `seed` (dealt).
    pub game_seed: u64,
    /// The session `env_seed`.
    pub env_seed: u64,
    /// Seats as session players: pod seats (drafted) or deck indices in the
    /// episode's `selected` list (dealt), p0 first.
    pub seats: [usize; 2],
    /// Each player's deck as sorted registry card ids, one per copy, p0
    /// first.
    pub decks: [Vec<u16>; 2],
    /// The raw end: [`GameEnd`] (tagged `kind`) or a process death.
    pub end: Value,
    /// Whether the game completed (`natural_game_over`).
    pub completed: bool,
    /// The normalised cause of a halted game; `null` for a completed game.
    pub stop_kind: Option<String>,
    /// Decisions the driver made; `null` after a process death.
    pub driver_decisions: Option<u64>,
    /// The terminal reply's counts.
    pub physical_decisions: Option<u64>,
    pub policy_steps: Option<u64>,
    /// `step` and `physical_decision_id` of the last decision reply.
    pub last_step: Option<u64>,
    pub last_physical_decision_id: Option<u64>,
    /// Source card of the last chosen action (A2.5 item 4).
    pub last_action_source: Option<u16>,
    pub last_action_source_name: Option<&'static str>,
    /// Source card of the latest of the last ten chosen actions that had
    /// one (the earlier census's "last action's source" column).
    pub recent_action_source: Option<u16>,
    pub recent_action_source_name: Option<&'static str>,
    /// The winner as a seat of [`GameLine::seats`]'s numbering, and as a
    /// session player (0 for p0, 1 for p1).
    pub winner_seat: Option<usize>,
    pub winner_player: Option<usize>,
    /// A completed game without a winner.
    pub draw: bool,
}

impl GameLine {
    /// The line as compact JSON, without a line break.
    pub fn to_json_line(&self) -> String {
        serde_json::to_string(self).expect("a game line is plain data")
    }
}

fn card_name(id: Option<u16>) -> Option<&'static str> {
    id.and_then(|id| CARD_DEFS.get(usize::from(id)))
        .map(|def| def.name)
}

fn sorted_ids(deck: &Deck) -> Vec<u16> {
    let mut ids = deck.card_ids();
    ids.sort_unstable();
    ids
}

/// Where a game sits in its run, apart from how it was played.
struct Placement {
    mode: &'static str,
    index: u64,
    draft: Option<u64>,
    game: u64,
    iteration: Option<u64>,
    episode_id: Option<String>,
    starting_player: Option<u64>,
    learner_seat: Option<u64>,
    draft_seed: Option<u64>,
    pod_seed: Option<u64>,
    game_seed: u64,
}

fn played_line(
    placement: Placement,
    decks: [&Deck; 2],
    record: &GameRecord,
    tally: &GameTally,
) -> GameLine {
    let winner_seat = record.winner();
    let completed = matches!(record.end, GameEnd::Natural { .. });
    let recent = tally.recent_action_source();
    GameLine {
        schema: GAME_LINE_SCHEMA,
        mode: placement.mode,
        index: placement.index,
        draft: placement.draft,
        pod: placement.draft,
        game: placement.game,
        iteration: placement.iteration,
        episode_id: placement.episode_id,
        starting_player: placement.starting_player,
        learner_seat: placement.learner_seat,
        draft_seed: placement.draft_seed,
        pod_seed: placement.pod_seed,
        game_seed: placement.game_seed,
        env_seed: record.env_seed,
        seats: record.seats,
        decks: [sorted_ids(decks[0]), sorted_ids(decks[1])],
        end: serde_json::to_value(&record.end).expect("a game end is plain data"),
        completed,
        stop_kind: stop_kind(&record.end),
        driver_decisions: Some(record.decisions),
        physical_decisions: tally.physical_decisions,
        policy_steps: tally.policy_steps,
        last_step: tally.last_step,
        last_physical_decision_id: tally.last_physical_decision_id,
        last_action_source: tally.last_action_source,
        last_action_source_name: card_name(tally.last_action_source),
        recent_action_source: recent,
        recent_action_source_name: card_name(recent),
        winner_seat,
        winner_player: winner_seat.and_then(|seat| record.seats.iter().position(|&s| s == seat)),
        draw: completed && winner_seat.is_none(),
    }
}

fn death_line(
    placement: Placement,
    seats: [usize; 2],
    decks: [&Deck; 2],
    death: ProcessDeath,
) -> GameLine {
    GameLine {
        schema: GAME_LINE_SCHEMA,
        mode: placement.mode,
        index: placement.index,
        draft: placement.draft,
        pod: placement.draft,
        game: placement.game,
        iteration: placement.iteration,
        episode_id: placement.episode_id,
        starting_player: placement.starting_player,
        learner_seat: placement.learner_seat,
        draft_seed: placement.draft_seed,
        pod_seed: placement.pod_seed,
        game_seed: placement.game_seed,
        env_seed: derive_seed(placement.game_seed, SeedStream::GameEnv, 0),
        seats,
        decks: [sorted_ids(decks[0]), sorted_ids(decks[1])],
        end: death.end(),
        completed: false,
        stop_kind: Some(death.stop_kind()),
        driver_decisions: None,
        physical_decisions: None,
        policy_steps: None,
        last_step: None,
        last_physical_decision_id: None,
        last_action_source: None,
        last_action_source_name: None,
        recent_action_source: None,
        recent_action_source_name: None,
        winner_seat: None,
        winner_player: None,
        draw: false,
    }
}

// ---------------------------------------------------------------------------
// Drafted games
// ---------------------------------------------------------------------------

/// A drafted draft ready to play: its seeds and every seat's deck.
#[derive(Debug, Clone)]
pub struct DraftedPod {
    pub draft: u64,
    pub draft_seed: u64,
    pub pod_seed: u64,
    pub decks: [Deck; SEATS],
}

impl DraftedPod {
    /// Draft `draft` of `namespace`: uniform pickers and the stated-rule
    /// deck of every seat, from `seed([namespace, "draft", draft])`.
    pub fn prepare(pool: &CubePool, namespace: &str, draft: u64) -> Result<Self, CubeDraftError> {
        let draft_seed = draft_seed(namespace, draft);
        let (_, decks) = uniform_draft_and_build(pool, draft_seed)?;
        Ok(DraftedPod {
            draft,
            draft_seed,
            pod_seed: pod_seed(namespace, draft),
            decks,
        })
    }

    fn placement(&self, game: usize) -> Placement {
        Placement {
            mode: "drafted",
            index: self.draft * GAMES_PER_DRAFT + game as u64,
            draft: Some(self.draft),
            game: game as u64,
            iteration: None,
            episode_id: None,
            starting_player: None,
            learner_seat: None,
            draft_seed: Some(self.draft_seed),
            pod_seed: Some(self.pod_seed),
            game_seed: game_seed(self.pod_seed, game),
        }
    }

    /// Plays pod game `game` under `caps`.
    pub fn play(&self, game: usize, caps: &GameCaps) -> Result<GameLine, CubeDraftError> {
        let (record, tally) = play_pod_game_tallied(&self.decks, self.pod_seed, game, caps)?;
        let decks = [&self.decks[record.seats[0]], &self.decks[record.seats[1]]];
        Ok(played_line(self.placement(game), decks, &record, &tally))
    }

    /// The line of pod game `game` when its worker process died: the
    /// game's seats come from the same seat coin the game uses.
    pub fn death(&self, game: usize, death: ProcessDeath) -> Result<GameLine, CubeDraftError> {
        let seats = pod_game_seats(self.pod_seed, game)?;
        let decks = [&self.decks[seats[0]], &self.decks[seats[1]]];
        Ok(death_line(self.placement(game), seats, decks, death))
    }
}

/// Plays the drafted games with logical indices `[start, end)` in order,
/// handing each line to `emit` as soon as it is played. Each draft is
/// prepared once.
pub fn play_drafted_range<E>(
    pool: &CubePool,
    namespace: &str,
    start: u64,
    end: u64,
    caps: &GameCaps,
    mut emit: impl FnMut(&GameLine) -> Result<(), E>,
) -> Result<(), E>
where
    E: From<CubeDraftError>,
{
    let mut prepared: Option<DraftedPod> = None;
    for index in start..end {
        let draft = index / GAMES_PER_DRAFT;
        let game = (index % GAMES_PER_DRAFT) as usize;
        if prepared.as_ref().is_none_or(|pod| pod.draft != draft) {
            prepared = Some(DraftedPod::prepare(pool, namespace, draft)?);
        }
        let pod = prepared.as_ref().expect("prepared above");
        emit(&pod.play(game, caps)?)?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Dealt games
// ---------------------------------------------------------------------------

/// One episode of a `deal.py` schedule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DealtEpisode {
    pub iteration: u64,
    pub slot: u64,
    pub id: String,
    pub seed: u64,
    pub starting_player: Option<u64>,
    pub learner_seat: Option<u64>,
    /// The two `selected` decks, as decks of registry cards.
    pub decks: [Deck; 2],
}

/// A parsed `deal.py` schedule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DealtSchedule {
    pub schema: Option<String>,
    pub namespace: Option<String>,
    pub run: Option<String>,
    pub kernel_carddb_hash: String,
    pub pool_sha256: Option<String>,
    pub constants_sha256: Option<String>,
    /// Every episode in schedule order (iteration, then slot).
    pub episodes: Vec<DealtEpisode>,
}

fn mainboard_deck(value: &Value, label: &str) -> Result<Deck, String> {
    let ids = value["mainboard"]
        .as_array()
        .ok_or_else(|| format!("{label}: no mainboard list"))?;
    let mut sorted = Vec::with_capacity(ids.len());
    for id in ids {
        let id = id
            .as_u64()
            .filter(|&id| id < CARD_DEFS.len() as u64)
            .ok_or_else(|| format!("{label}: {id} is not a card id of this build"))?;
        sorted.push(id as u16);
    }
    if sorted.is_empty() {
        return Err(format!("{label}: empty mainboard"));
    }
    sorted.sort_unstable();
    let mut deck = Deck::default();
    let mut at = 0;
    while at < sorted.len() {
        let run = sorted[at..]
            .iter()
            .take_while(|&&id| id == sorted[at])
            .count();
        deck.push(sorted[at], run as u32);
        at += run;
    }
    Ok(deck)
}

fn optional_text(value: &Value) -> Option<String> {
    value.as_str().map(str::to_owned)
}

/// Parses a `deal.py` schedule (`{"kernel_carddb_hash": ..., "iterations":
/// [{"episodes": [{"episode": {...}}, ...]}, ...]}`). Refuses a schedule
/// whose `kernel_carddb_hash` is missing or is not `carddb_hash`, and an
/// episode whose session caps are not `caps`' (A2.1: every run passes the
/// same caps).
pub fn parse_dealt_schedule(
    text: &str,
    carddb_hash: &str,
    caps: &GameCaps,
) -> Result<DealtSchedule, String> {
    let doc: Value = serde_json::from_str(text).map_err(|error| format!("schedule: {error}"))?;
    let hash = doc["kernel_carddb_hash"]
        .as_str()
        .ok_or("schedule has no kernel_carddb_hash")?;
    if hash != carddb_hash {
        return Err(format!(
            "schedule kernel_carddb_hash {hash} is not this build's {carddb_hash}"
        ));
    }
    let iterations = doc["iterations"]
        .as_array()
        .ok_or("schedule has no iterations list")?;
    let mut episodes = Vec::new();
    for (iteration, entry) in iterations.iter().enumerate() {
        let list = entry["episodes"]
            .as_array()
            .ok_or_else(|| format!("iteration {iteration} has no episodes list"))?;
        for (slot, item) in list.iter().enumerate() {
            let label = format!("iteration {iteration} episode {slot}");
            let episode = &item["episode"];
            let id = episode["id"]
                .as_str()
                .ok_or_else(|| format!("{label}: no string id"))?;
            let seed = episode["seed"]
                .as_u64()
                .ok_or_else(|| format!("{label}: no integer seed"))?;
            for (key, cap) in [
                ("max_physical_decisions", caps.max_physical_decisions),
                ("max_policy_steps", caps.max_policy_steps),
            ] {
                if episode[key].as_u64() != Some(cap) {
                    return Err(format!(
                        "{label}: {key} {} is not the run's cap {cap}",
                        episode[key]
                    ));
                }
            }
            // The Limited session always starts p0 and plays the default
            // opening and mulligans, so an episode asking for anything else
            // would be played as a different game than the schedule states.
            if episode["starting_player"].as_u64() != Some(0) {
                return Err(format!(
                    "{label}: starting_player {} is not 0 (the session starts selected[0])",
                    episode["starting_player"]
                ));
            }
            for key in ["runtime_rules", "opening", "mulligan_modes"] {
                if !episode[key].is_null() {
                    return Err(format!(
                        "{label}: {key} is not supported by the census session"
                    ));
                }
            }
            let selected = episode["selected"]
                .as_array()
                .filter(|decks| decks.len() == 2)
                .ok_or_else(|| format!("{label}: selected is not a list of two decks"))?;
            let decks = [
                mainboard_deck(&selected[0], &format!("{label} deck 0"))?,
                mainboard_deck(&selected[1], &format!("{label} deck 1"))?,
            ];
            episodes.push(DealtEpisode {
                iteration: iteration as u64,
                slot: slot as u64,
                id: id.to_owned(),
                seed,
                starting_player: episode["starting_player"].as_u64(),
                learner_seat: episode["learner_seat"].as_u64(),
                decks,
            });
        }
    }
    Ok(DealtSchedule {
        schema: optional_text(&doc["schema"]),
        namespace: optional_text(&doc["namespace"]),
        run: optional_text(&doc["run"]),
        kernel_carddb_hash: hash.to_owned(),
        pool_sha256: optional_text(&doc["pool_sha256"]),
        constants_sha256: optional_text(&doc["constants_sha256"]),
        episodes,
    })
}

impl DealtEpisode {
    fn placement(&self, index: u64) -> Placement {
        Placement {
            mode: "dealt",
            index,
            draft: None,
            game: self.slot,
            iteration: Some(self.iteration),
            episode_id: Some(self.id.clone()),
            starting_player: self.starting_player,
            learner_seat: self.learner_seat,
            draft_seed: None,
            pod_seed: None,
            game_seed: self.seed,
        }
    }

    /// Plays the episode once as schedule episode `index`: `selected[0]` as
    /// p0, `selected[1]` as p1, seeded by the episode's `seed`.
    pub fn play(&self, index: u64, caps: &GameCaps) -> GameLine {
        let decks = [&self.decks[0], &self.decks[1]];
        let (record, tally) = play_seeded_game_tallied(decks, self.seed, index as usize, caps);
        played_line(self.placement(index), decks, &record, &tally)
    }

    /// The line of the episode when its worker process died.
    pub fn death(&self, index: u64, death: ProcessDeath) -> GameLine {
        let decks = [&self.decks[0], &self.decks[1]];
        death_line(self.placement(index), [0, 1], decks, death)
    }
}
