//! Search diagnostics for the stage 2-3 plan (diagnostic only: no training,
//! store records or gates).
//!
//! - `roots`: the focal policy (`source`) plays one seat against an opponent
//!   model from `OPPONENTS`; one row per focal multi-action decision with a
//!   replay key (`step`, `menu_hash`, `t1_action`), a phase x menu stratum and
//!   mechanism tags. `tools/t1_diagnosis/freeze_roots.py` freezes a corpus.
//! - `cond`: for each root in `ROOTS`, selects a root action under four
//!   conditions (policy top-K or low-probability coverage, crossed with plain
//!   or improved continuation) at equal transition budgets, then evaluates
//!   every choice on fresh paired determinizations against the root game's
//!   actual opponent model.
//! - `cross`: for each root, crosses T1's root action or a search-selected
//!   alternative with T1, turn-long or game-long improved continuation, and
//!   logs every focal decision where improved play deviates from T1.
//!
//! The search logic names no card or deck. Mechanism tags exist only in the
//! `roots` labelling (`root_tags`).

use super::{
    load_policy_v1, mix, named, seat_index, softmax, variant, CensusConfigV1, MAX_PHYSICAL,
};
use crate::ids::PlayerId;
use crate::paired_bo1_harness_v1::paired_policy_seeds_v1;
use crate::rl::TerminalClassificationV1;
use crate::rl_session::{FastActorDecisionV1, FastActorResponseV1, FastActorSessionV1};
use crate::runtime_decks::RUNTIME_DECKS;
use crate::sideboard_play_policy_v1::FrozenPlayPolicyV1;
use crate::state::{SplitMix64, Step};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::io::Write;
use std::sync::Mutex;
use std::time::Instant;

const DEFAULT_BUDGETS: [u64; 2] = [4000, 16000];
const DEFAULT_EVAL_PLAYOUTS: usize = 8;
const DEFAULT_M_INNER: usize = 2;
const DEFAULT_HORIZON: Horizon = Horizon::Decisions(6);
/// Selection stops after this many consecutive rounds lost to clone failures.
const MAX_FAILED_ROUNDS: u32 = 16;
const MAX_ROUNDS: u64 = 100_000;
const SEM_CHARS: usize = 240;

pub(super) fn is_search_mode(mode: &str) -> bool {
    matches!(mode, "roots" | "cond" | "cross")
}

/// How far the improved continuation searches before plain T1 takes over.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum Horizon {
    /// Until the root's turn ends.
    Turn,
    /// The next N focal multi-action decisions.
    Decisions(u32),
    /// Every focal multi-action decision to the end of the game.
    Game,
}

impl Horizon {
    fn parse(text: &str) -> Result<Self, String> {
        match text.trim() {
            "turn" => Ok(Self::Turn),
            "game" => Ok(Self::Game),
            "decisions" => Ok(DEFAULT_HORIZON),
            other => other
                .strip_prefix("decisions:")
                .and_then(|n| n.parse().ok())
                .map(Self::Decisions)
                .ok_or_else(|| format!("unknown HORIZON {other}")),
        }
    }

    fn label(self) -> String {
        match self {
            Self::Turn => "turn".into(),
            Self::Game => "game".into(),
            Self::Decisions(n) => format!("decisions:{n}"),
        }
    }
}

/// Focal follow-up after the root action.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Follow {
    T1,
    Improved(Horizon),
}

impl Follow {
    fn label(self) -> String {
        match self {
            Self::T1 => "t1".into(),
            Self::Improved(h) => format!("improved:{}", h.label()),
        }
    }
}

/// When selection records a budget level. `Round` records it after the round
/// that crosses the budget completes, so a level can overspend by most of a
/// round. `Exact` records it at the crossing from the complete rounds so far
/// (a partial round never enters the means) and counts the partial round's
/// transitions as spent, so a level overspends by at most one playout.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BudgetStop {
    Round,
    Exact,
}

impl BudgetStop {
    fn parse(text: &str) -> Result<Self, String> {
        match text.trim() {
            "round" => Ok(Self::Round),
            "exact" => Ok(Self::Exact),
            other => Err(format!("unknown BUDGET_STOP {other}")),
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Round => "round",
            Self::Exact => "exact",
        }
    }
}

/// Opponent seat in an outer playout: T1 (the searcher's opponent model in
/// selection) or one of the `OPPONENTS` models (the actual opponent, used in
/// evaluation).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum OppSel {
    T1,
    Model(usize),
}

/// Read-only state shared by all workers.
pub(super) struct SearchSharedV1 {
    labels: Vec<String>,
    opponents: Vec<FrozenPlayPolicyV1>,
    roots: Vec<Value>,
    budgets: Vec<u64>,
    eval_playouts: usize,
    m_inner: usize,
    horizon: Horizon,
    budget_stop: BudgetStop,
}

fn env_or<T: std::str::FromStr>(name: &str, default: T) -> Result<T, String> {
    match std::env::var(name) {
        Ok(v) => v.trim().parse().map_err(|_| format!("bad {name}={v}")),
        Err(_) => Ok(default),
    }
}

impl SearchSharedV1 {
    pub(super) fn load(cfg: &CensusConfigV1) -> Result<Self, String> {
        let entries: Vec<String> = match std::env::var("OPPONENTS") {
            Ok(v) => v
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .collect(),
            Err(_) => vec![std::env::var("REF_SOURCE").unwrap_or_else(|_| cfg.source.clone())],
        };
        if entries.is_empty() {
            return Err("OPPONENTS lists no model".into());
        }
        let mut labels = Vec::new();
        let mut opponents = Vec::new();
        for entry in entries {
            let (label, path) = match entry.split_once('=') {
                Some((l, p)) => (l.to_owned(), p.to_owned()),
                None => (
                    std::path::Path::new(&entry)
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or(&entry)
                        .to_owned(),
                    entry.clone(),
                ),
            };
            opponents.push(load_policy_v1(&path)?);
            labels.push(label);
        }
        let roots = if cfg.mode == "cond" || cfg.mode == "cross" {
            let path = std::env::var("ROOTS").map_err(|_| "ROOTS names no roots file")?;
            let text = std::fs::read_to_string(&path).map_err(|e| format!("{path}: {e}"))?;
            text.lines()
                .filter(|l| !l.trim().is_empty())
                .map(|l| serde_json::from_str(l).map_err(|e| e.to_string()))
                .collect::<Result<Vec<Value>, _>>()?
        } else {
            Vec::new()
        };
        let mut budgets = match std::env::var("BUDGETS") {
            Ok(v) => v
                .split(',')
                .map(|x| {
                    x.trim()
                        .parse::<u64>()
                        .map_err(|_| format!("bad BUDGETS {v}"))
                })
                .collect::<Result<Vec<_>, _>>()?,
            Err(_) => DEFAULT_BUDGETS.to_vec(),
        };
        budgets.sort_unstable();
        budgets.dedup();
        if budgets.is_empty() {
            return Err("BUDGETS is empty".into());
        }
        let horizon = match std::env::var("HORIZON") {
            Ok(v) => Horizon::parse(&v)?,
            Err(_) => DEFAULT_HORIZON,
        };
        let budget_stop = match std::env::var("BUDGET_STOP") {
            Ok(v) => BudgetStop::parse(&v)?,
            Err(_) => BudgetStop::Round,
        };
        let shared = Self {
            labels,
            opponents,
            roots,
            budgets,
            eval_playouts: env_or("EVAL_PLAYOUTS", DEFAULT_EVAL_PLAYOUTS)?,
            m_inner: env_or("M_INNER", DEFAULT_M_INNER)?.max(1),
            horizon,
            budget_stop,
        };
        eprintln!(
            "search: opponents {:?}, roots {}, budgets {:?} (stop {}), eval {}, m_inner {}, horizon {}",
            shared.labels,
            shared.roots.len(),
            shared.budgets,
            shared.budget_stop.label(),
            shared.eval_playouts,
            shared.m_inner,
            shared.horizon.label()
        );
        Ok(shared)
    }

    /// Number of work items for root-driven modes.
    pub(super) fn root_count(&self) -> u64 {
        self.roots.len() as u64
    }

    pub(super) fn fork(&self, t1: &FrozenPlayPolicyV1) -> Result<SearchWorkerV1, String> {
        Ok(SearchWorkerV1 {
            focal: t1.fork_for_collection_v3()?,
            opps: self
                .opponents
                .iter()
                .map(FrozenPlayPolicyV1::fork_for_collection_v3)
                .collect::<Result<_, _>>()?,
            t1_opp: t1.fork_for_collection_v3()?,
            scorer: t1.fork_for_collection_v3()?,
            inner_focal: t1.fork_for_collection_v3()?,
            inner_opp: t1.fork_for_collection_v3()?,
        })
    }
}

/// Per-worker policy instances. Each role has its own instance so that one
/// role's sampling stream never moves another's: `focal` and `opps` replay
/// the root game and then play the outer playouts' main line; `t1_opp` is the
/// opponent in selection playouts; `inner_*` play the improved continuation's
/// inner playouts; `scorer` only computes probabilities (scoring is stateless).
pub(super) struct SearchWorkerV1 {
    focal: FrozenPlayPolicyV1,
    opps: Vec<FrozenPlayPolicyV1>,
    t1_opp: FrozenPlayPolicyV1,
    scorer: FrozenPlayPolicyV1,
    inner_focal: FrozenPlayPolicyV1,
    inner_opp: FrozenPlayPolicyV1,
}

/// Everything the game index decides.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct GameSetup {
    game: u64,
    seed: u64,
    /// Deck index (into RUNTIME_DECKS) per seat.
    decks: [usize; 2],
    focal: usize,
    model: usize,
    starting: u8,
}

fn game_setup(cfg: &CensusConfigV1, models: usize, game: u64) -> GameSetup {
    let n = cfg.decks.len() as u64;
    let m = models.max(1) as u64;
    let focal_deck = cfg.decks[(game % n) as usize];
    let opp_deck = cfg.decks[((game / n) % n) as usize];
    let focal = ((game / (n * n)) % 2) as usize;
    let model = ((game / (2 * n * n)) % m) as usize;
    let starting = ((game / (2 * n * n * m)) % 2) as u8;
    let mut decks = [opp_deck; 2];
    decks[focal] = focal_deck;
    GameSetup {
        game,
        seed: mix(cfg.base_seed ^ mix(game)),
        decks,
        focal,
        model,
        starting,
    }
}

fn new_session(setup: &GameSetup) -> Result<FastActorSessionV1, String> {
    let decks = [
        &RUNTIME_DECKS[setup.decks[0]],
        &RUNTIME_DECKS[setup.decks[1]],
    ];
    FastActorSessionV1::reset_with_explicit_decks_and_limits_flat_action_v3_environment_v2_with_starting_player_v1(
        1,
        setup.seed,
        MAX_PHYSICAL,
        MAX_PHYSICAL * 128,
        [decks[0].id.to_owned(), decks[1].id.to_owned()],
        [decks[0].card_ids.to_vec(), decks[1].card_ids.to_vec()],
        PlayerId(setup.starting),
    )
    .map_err(|e| format!("{e:?}"))
}

/// Focal score of a terminal response: 1 win, 0 loss, 0.5 draw or non-natural.
fn terminal_score(response: &FastActorResponseV1, seat: usize) -> Option<(f64, bool)> {
    let FastActorResponseV1::Terminal(t) = response else {
        return None;
    };
    let natural = t.terminal_classification == TerminalClassificationV1::Natural;
    let score = match t.winner {
        Some(w) if natural => f64::from(u8::from(seat_index(w) == seat)),
        _ => 0.5,
    };
    Some((score, natural))
}

/// Plays the root game with the base policies (focal seat `focal`, the other
/// seat `opp`), both reset to the game's paired policy seeds. `visit` runs
/// after every selection; returning true stops before that action is applied,
/// leaving the session at that decision. Roots mode and replay both use this
/// one loop, so a replay consumes exactly the same sampling draws.
fn drive_game(
    setup: &GameSetup,
    focal: &mut FrozenPlayPolicyV1,
    opp: &mut FrozenPlayPolicyV1,
    mut visit: impl FnMut(&FastActorSessionV1, &FastActorDecisionV1, u32) -> Result<bool, String>,
) -> Result<(FastActorSessionV1, Option<(f64, bool)>), String> {
    let mut session = new_session(setup)?;
    let seeds = paired_policy_seeds_v1(setup.seed);
    focal.reset_sampling_v1(seeds);
    opp.reset_sampling_v1(seeds);
    loop {
        let response = session.current_response();
        if let Some(t) = terminal_score(&response, setup.focal) {
            return Ok((session, Some(t)));
        }
        let FastActorResponseV1::Decision(d) = response else {
            unreachable!("non-terminal response is a decision")
        };
        let a = if seat_index(d.acting_player) == setup.focal {
            focal.select_fast_session_v1(&session)?
        } else {
            opp.select_fast_session_v1(&session)?
        };
        if visit(&session, &d, a)? {
            return Ok((session, None));
        }
        session
            .step(d.episode_id, d.step, a)
            .map_err(|e| format!("{e:?}"))?;
    }
}

fn is_combat(step: Step) -> bool {
    matches!(
        step,
        Step::BeginCombat
            | Step::DeclareAttackers
            | Step::DeclareBlockers
            | Step::CombatDamage
            | Step::EndCombat
    )
}

/// Replay key and stratum of the current decision.
#[derive(Clone, Debug, PartialEq)]
struct MenuInfo {
    turn: u32,
    phase: String,
    own_turn: bool,
    k: usize,
    menu_hash: String,
    phase_group: &'static str,
    menu_group: &'static str,
    tags: Vec<&'static str>,
    /// Named semantics, kept only for tagged menus (mechanism inspection).
    menu: Vec<String>,
}

/// SHA-256 of the newline-joined debug strings of the legal-action semantics.
fn menu_hash(debug: &[String]) -> String {
    let mut h = Sha256::new();
    for d in debug {
        h.update(d.as_bytes());
        h.update(b"\n");
    }
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

fn menu_info(session: &FastActorSessionV1, actor: usize) -> Result<MenuInfo, String> {
    let sem = session
        .diagnostic_current_action_semantics()
        .ok_or("missing semantics")?;
    let debug: Vec<String> = sem.iter().map(|s| format!("{s:?}")).collect();
    let kinds: Vec<String> = debug.iter().map(|d| variant(d)).collect();
    let st = session.game_state();
    let own_turn = st.active_player.0 as usize == actor;
    let phase_group = if !own_turn {
        "opp_turn"
    } else if matches!(st.step, Step::Main1 | Step::Main2) {
        "own_main"
    } else if is_combat(st.step) {
        "own_combat"
    } else {
        "other"
    };
    let has = |pred: &dyn Fn(&str) -> bool| kinds.iter().any(|k| pred(k));
    let menu_group = if has(&|k| {
        matches!(
            k,
            "ChooseAttackerInclusion"
                | "ChooseBlockerInclusion"
                | "DeclareAttackers"
                | "DeclareBlockersForAttacker"
        )
    }) {
        "combat"
    } else if has(&|k| {
        k.starts_with("Choose")
            && (k.contains("Target")
                || k.contains("Color")
                || k.contains("Mode")
                || k == "ChooseEffectOption")
    }) {
        "choice"
    } else if has(&|k| matches!(k, "CastSpell" | "ActivateAbility")) {
        "action"
    } else {
        "other"
    };
    let named_sem: Vec<Value> = sem
        .iter()
        .map(|x| named(serde_json::to_value(x).unwrap_or_default()))
        .collect();
    let tags = root_tags(&named_sem, own_turn, is_combat(st.step));
    Ok(MenuInfo {
        turn: st.turn,
        phase: format!("{:?}", st.step),
        own_turn,
        k: sem.len(),
        menu_hash: menu_hash(&debug),
        phase_group,
        menu_group,
        menu: if tags.is_empty() {
            Vec::new()
        } else {
            named_sem
                .iter()
                .map(|v| v.to_string().chars().take(SEM_CHARS).collect())
                .collect()
        },
        tags,
    })
}

/// Mechanism tags for the frozen mechanism set. This is the only place that
/// names cards; the search itself never reads these tags.
fn root_tags(named_sem: &[Value], own_turn: bool, combat: bool) -> Vec<&'static str> {
    let source = |v: &Value| -> String {
        v["source"]
            .as_str()
            .or_else(|| v["card"].as_str())
            .unwrap_or("")
            .to_owned()
    };
    let kind = |v: &Value| v["action_kind"].as_str().unwrap_or("").to_owned();
    let mut tags = Vec::new();
    if named_sem.iter().any(|v| {
        matches!(
            source(v).as_str(),
            "Balustrade Spy" | "Dread Return" | "Lotleth Giant"
        )
    }) {
        tags.push("spy");
    }
    if !own_turn
        && combat
        && named_sem
            .iter()
            .any(|v| kind(v) == "cast_spell" && source(v) == "Prismatic Strands")
    {
        tags.push("cawgates_strands_opp_combat");
    }
    if named_sem.iter().any(|v| {
        kind(v) == "choose_effect_color"
            && matches!(source(v).as_str(), "Citadel Gate" | "Sea Gate")
    }) {
        tags.push("cawgates_gate_colour");
    }
    if named_sem
        .iter()
        .any(|v| kind(v) == "activate_ability" && source(v) == "Basilisk Gate")
    {
        tags.push("cawgates_basilisk");
    }
    tags
}

fn sem_strings(session: &FastActorSessionV1) -> Vec<String> {
    session
        .diagnostic_current_action_semantics()
        .unwrap_or_default()
        .iter()
        .map(|x| {
            named(serde_json::to_value(x).unwrap_or_default())
                .to_string()
                .chars()
                .take(SEM_CHARS)
                .collect()
        })
        .collect()
}

fn root_row(
    setup: &GameSetup,
    label: &str,
    d: &FastActorDecisionV1,
    a: u32,
    info: &MenuInfo,
) -> Value {
    json!({"kind":"root","game":setup.game,"seed":setup.seed,"step":d.step,"turn":info.turn,
        "phase":info.phase,"own_turn":info.own_turn,"actor":setup.focal,"k":info.k,
        "menu_hash":info.menu_hash,"phase_group":info.phase_group,"menu_group":info.menu_group,
        "stratum":format!("{}|{}",info.phase_group,info.menu_group),"tags":info.tags,
        "deck":RUNTIME_DECKS[setup.decks[setup.focal]].id,
        "opp_deck":RUNTIME_DECKS[setup.decks[1-setup.focal]].id,
        "opp_model":label,"opp_model_index":setup.model,"focal_seat":setup.focal,
        "starting_player":setup.starting,"t1_action":a,
        "menu":if info.menu.is_empty() { Value::Null } else { json!(info.menu) }})
}

/// Mode `roots`: one row per focal decision with at least two legal actions.
pub(super) fn run_roots_game(
    cfg: &CensusConfigV1,
    shared: &SearchSharedV1,
    w: &mut SearchWorkerV1,
    game: u64,
    sink: &Mutex<std::fs::File>,
) -> Result<(), String> {
    let setup = game_setup(cfg, shared.labels.len(), game);
    let label = &shared.labels[setup.model];
    let mut rows = Vec::new();
    let (session, terminal) =
        drive_game(&setup, &mut w.focal, &mut w.opps[setup.model], |s, d, a| {
            if seat_index(d.acting_player) == setup.focal && d.legal_action_count >= 2 {
                rows.push(root_row(&setup, label, d, a, &menu_info(s, setup.focal)?));
            }
            Ok(false)
        })?;
    let (score, natural) = terminal.ok_or("game did not finish")?;
    let turns = session.game_state().turn;
    let mut out = json!({"kind":"roots_game","game":game,"seed":setup.seed,
        "deck":RUNTIME_DECKS[setup.decks[setup.focal]].id,
        "opp_deck":RUNTIME_DECKS[setup.decks[1-setup.focal]].id,"opp_model":label,
        "focal_seat":setup.focal,"starting_player":setup.starting,"focal_score":score,
        "natural":natural,"turns":turns,"roots":rows.len()})
    .to_string();
    out.push('\n');
    for mut r in rows {
        r["focal_score"] = json!(score);
        r["natural"] = json!(natural);
        r["game_turns"] = json!(turns);
        out.push_str(&r.to_string());
        out.push('\n');
    }
    let mut f = sink.lock().map_err(|_| "sink poisoned")?;
    f.write_all(out.as_bytes()).map_err(|e| e.to_string())?;
    f.flush().map_err(|e| e.to_string())
}

/// Replays a root's game to its recorded step and checks the replay key.
/// Returns the session at the root and T1's sampled root action.
fn replay_core(
    setup: &GameSetup,
    focal: &mut FrozenPlayPolicyV1,
    opp: &mut FrozenPlayPolicyV1,
    step: u64,
) -> Result<(FastActorSessionV1, u32), String> {
    let mut found = None;
    let (session, terminal) = drive_game(setup, focal, opp, |_, d, a| {
        if d.step > step {
            return Err(format!("replay passed step {step} at {}", d.step));
        }
        if d.step == step {
            found = Some(a);
            return Ok(true);
        }
        Ok(false)
    })?;
    if terminal.is_some() {
        return Err(format!("replay reached the end before step {step}"));
    }
    Ok((session, found.ok_or("replay stopped without a root")?))
}

fn check_root(
    session: &FastActorSessionV1,
    t1_action: u32,
    root: &Value,
    focal: usize,
) -> Result<(), String> {
    let info = menu_info(session, focal)?;
    let FastActorResponseV1::Decision(d) = session.current_response() else {
        return Err("replayed root is terminal".into());
    };
    let checks = [
        ("turn", json!(info.turn), root["turn"].clone()),
        (
            "actor",
            json!(seat_index(d.acting_player)),
            root["actor"].clone(),
        ),
        ("k", json!(info.k), root["k"].clone()),
        (
            "menu_hash",
            json!(info.menu_hash),
            root["menu_hash"].clone(),
        ),
        ("t1_action", json!(t1_action), root["t1_action"].clone()),
    ];
    for (name, got, want) in checks {
        if got != want {
            return Err(format!("replay mismatch: {name} {got} != recorded {want}"));
        }
    }
    Ok(())
}

struct Replayed {
    setup: GameSetup,
    session: FastActorSessionV1,
    t1_action: u32,
    secs: f64,
}

fn replay_root(
    cfg: &CensusConfigV1,
    shared: &SearchSharedV1,
    w: &mut SearchWorkerV1,
    root: &Value,
) -> Result<Replayed, String> {
    let started = Instant::now();
    let game = root["game"].as_u64().ok_or("root has no game")?;
    let step = root["step"].as_u64().ok_or("root has no step")?;
    let setup = game_setup(cfg, shared.labels.len(), game);
    let expect = [
        ("seed", json!(setup.seed), root["seed"].clone()),
        ("focal_seat", json!(setup.focal), root["focal_seat"].clone()),
        (
            "deck",
            json!(RUNTIME_DECKS[setup.decks[setup.focal]].id),
            root["deck"].clone(),
        ),
        (
            "opp_deck",
            json!(RUNTIME_DECKS[setup.decks[1 - setup.focal]].id),
            root["opp_deck"].clone(),
        ),
        (
            "opp_model",
            json!(shared.labels[setup.model]),
            root["opp_model"].clone(),
        ),
    ];
    for (name, got, want) in expect {
        if got != want {
            return Err(format!(
                "replay mismatch: setup {name} {got} != recorded {want} (base_seed, decks or OPPONENTS differ)"
            ));
        }
    }
    let (session, t1_action) = replay_core(&setup, &mut w.focal, &mut w.opps[setup.model], step)?;
    check_root(&session, t1_action, root, setup.focal)?;
    Ok(Replayed {
        setup,
        session,
        t1_action,
        secs: started.elapsed().as_secs_f64(),
    })
}

/// Indices ordered by descending probability (ties by index).
fn prob_order(probs: &[f64]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..probs.len()).collect();
    order.sort_by(|&a, &b| {
        probs[b]
            .partial_cmp(&probs[a])
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    order
}

/// `take` uniform draws without replacement from `pool`.
fn uniform_draw(pool: &[usize], take: usize, rng: &mut SplitMix64) -> Vec<usize> {
    let mut pool = pool.to_vec();
    let take = take.min(pool.len());
    for i in 0..take {
        let j = i + (rng.next_u64() % (pool.len() - i) as u64) as usize;
        pool.swap(i, j);
    }
    pool.truncate(take);
    pool
}

/// Root candidates: (top-K, coverage set, coverage differs).
fn candidate_sets(probs: &[f64], k_max: usize, seed: u64) -> (Vec<usize>, Vec<usize>, bool) {
    let order = prob_order(probs);
    let k_max = k_max.max(1);
    if order.len() <= k_max {
        return (order.clone(), order, false);
    }
    let top: Vec<usize> = order[..k_max].to_vec();
    let head = k_max.div_ceil(2);
    let mut rng = SplitMix64::seed(seed);
    let mut cover: Vec<usize> = order[..head].to_vec();
    cover.extend(uniform_draw(&order[head..], k_max / 2, &mut rng));
    let mut a = top.clone();
    let mut b = cover.clone();
    a.sort_unstable();
    b.sort_unstable();
    (top, cover, a != b)
}

/// Best mean, ties to the higher policy probability, then lower index.
fn best_by_mean(cands: &[usize], means: &[f64], probs: &[f64]) -> usize {
    let mut best = 0;
    for i in 1..cands.len() {
        let better = means[i] > means[best]
            || (means[i] == means[best] && probs[cands[i]] > probs[cands[best]]);
        if better {
            best = i;
        }
    }
    best
}

/// One outer playout. Transitions and wall time are counted even when the
/// playout fails.
#[derive(Clone, Debug, Default)]
struct Outcome {
    score: f64,
    natural: bool,
    transitions: u64,
    wall: f64,
    inner_failures: u32,
    searched: u32,
    deviations: Vec<Value>,
    failed: Option<String>,
}

/// Plays both seats to the end with plain policies; counts transitions.
fn plain_to_end(
    s: &mut FastActorSessionV1,
    seat: usize,
    f: &mut FrozenPlayPolicyV1,
    o: &mut FrozenPlayPolicyV1,
    transitions: &mut u64,
) -> Result<(f64, bool), String> {
    loop {
        let response = s.current_response();
        if let Some(t) = terminal_score(&response, seat) {
            return Ok(t);
        }
        let FastActorResponseV1::Decision(d) = response else {
            unreachable!("non-terminal response is a decision")
        };
        let a = if seat_index(d.acting_player) == seat {
            f.select_fast_session_v1(s)?
        } else {
            o.select_fast_session_v1(s)?
        };
        s.step(d.episode_id, d.step, a)
            .map_err(|e| format!("{e:?}"))?;
        *transitions += 1;
    }
}

fn policy_seeds(det: u64) -> ([u64; 2], [u64; 2]) {
    (
        [mix(det ^ 0xA1), mix(det ^ 0xB2)],
        [mix(det ^ 0xC3), mix(det ^ 0xD4)],
    )
}

/// One plain T1 playout from a fresh determinization of `s` after `action`.
fn inner_playout(
    s: &FastActorSessionV1,
    seat: usize,
    det: u64,
    action: u32,
    f: &mut FrozenPlayPolicyV1,
    o: &mut FrozenPlayPolicyV1,
    transitions: &mut u64,
) -> Result<f64, String> {
    let FastActorResponseV1::Decision(d) = s.current_response() else {
        return Err("inner root is terminal".into());
    };
    let mut c = s.census_redeterminized_clone_v1(det)?;
    c.step(d.episode_id, d.step, action)
        .map_err(|e| format!("inner root step {action}: {e:?}"))?;
    *transitions += 1;
    let (fs, os) = policy_seeds(det);
    f.reset_sampling_v1(fs);
    o.reset_sampling_v1(os);
    Ok(plain_to_end(&mut c, seat, f, o, transitions)?.0)
}

/// Improved continuation at one focal decision: the top two actions plus one
/// uniform draw from the rest, plus T1's sampled action if it is not among
/// them, `m_inner` plain T1 playouts each on shared determinizations. The
/// sampled action is kept unless another candidate's mean is strictly higher,
/// so every deviation from T1 is driven by search, not by tie-breaking.
/// Any inner failure falls back to T1's sampled action and is counted.
#[allow(
    clippy::too_many_arguments,
    reason = "explicit policy roles keep the sampling streams separate"
)]
fn improve_decision(
    s: &FastActorSessionV1,
    seat: usize,
    sampled: u32,
    dec_seed: u64,
    m_inner: usize,
    scorer: &mut FrozenPlayPolicyV1,
    inner_f: &mut FrozenPlayPolicyV1,
    inner_o: &mut FrozenPlayPolicyV1,
    out: &mut Outcome,
    log: bool,
) -> u32 {
    scorer.reset_sampling_v1([1, 2]);
    let probs = match scorer.score_fast_session_v1(s) {
        Ok(scores) => softmax(&scores.logits),
        Err(_) => {
            out.inner_failures += 1;
            return sampled;
        }
    };
    let order = prob_order(&probs);
    let mut cands: Vec<usize> = order.iter().copied().take(2).collect();
    if order.len() > 2 {
        let mut rng = SplitMix64::seed(mix(dec_seed ^ 0x0C0F));
        cands.extend(uniform_draw(&order[2..], 1, &mut rng));
    }
    let sampled_in_cands = cands.contains(&(sampled as usize));
    if !sampled_in_cands {
        cands.push(sampled as usize);
    }
    let mut sums = vec![0f64; cands.len()];
    for j in 0..m_inner {
        let det = mix(dec_seed ^ (j as u64 + 1));
        for (i, &c) in cands.iter().enumerate() {
            match inner_playout(
                s,
                seat,
                det,
                c as u32,
                inner_f,
                inner_o,
                &mut out.transitions,
            ) {
                Ok(score) => sums[i] += score,
                Err(_) => {
                    out.inner_failures += 1;
                    return sampled;
                }
            }
        }
    }
    let means: Vec<f64> = sums.iter().map(|x| x / m_inner as f64).collect();
    let own = cands
        .iter()
        .position(|&c| c == sampled as usize)
        .expect("sampled action is a candidate");
    let best = best_by_mean(&cands, &means, &probs);
    let chosen = if means[best] > means[own] {
        cands[best] as u32
    } else {
        sampled
    };
    if log && chosen != sampled {
        let st = s.game_state();
        let sem = sem_strings(s);
        let step = match s.current_response() {
            FastActorResponseV1::Decision(d) => d.step,
            FastActorResponseV1::Terminal(_) => 0,
        };
        out.deviations.push(
            json!({"turn":st.turn,"step":step,"phase":format!("{:?}",st.step),
            "own_turn":st.active_player.0 as usize == seat,"k":probs.len(),
            "sampled":sampled,"chosen":chosen,
            "sampled_sem":sem.get(sampled as usize),"chosen_sem":sem.get(chosen as usize),
            "cands":cands,"cand_sem":cands.iter().map(|&c| sem.get(c)).collect::<Vec<_>>(),
            "cand_probs":cands.iter().map(|&c| probs[c]).collect::<Vec<_>>(),
            "sampled_prob":probs.get(sampled as usize),"means":means,
            "sampled_in_cands":sampled_in_cands,
            "all_equal":means.iter().all(|m| *m == means[0])}),
        );
    }
    chosen
}

type CacheKey = (u64, u32, Follow, OppSel);

/// Playouts are deterministic in (det, action, follow, opponent), so
/// conditions that share a candidate share its playouts. Nominal cost is
/// still charged to every condition that uses a playout; `actual_*` is the
/// compute actually spent.
#[derive(Default)]
struct PlayoutCache {
    map: HashMap<CacheKey, Outcome>,
    actual_transitions: u64,
    actual_wall: f64,
    actual_playouts: u64,
}

impl SearchWorkerV1 {
    /// Outer playout from a fresh determinization `det` of the root, after
    /// `action`, with the focal follow-up `follow` against `opp`.
    #[allow(
        clippy::too_many_arguments,
        reason = "one playout is fully described by these inputs"
    )]
    fn playout(
        &mut self,
        root: &FastActorSessionV1,
        seat: usize,
        det: u64,
        action: u32,
        follow: Follow,
        opp: OppSel,
        m_inner: usize,
        log: bool,
    ) -> Outcome {
        let started = Instant::now();
        let mut out = Outcome::default();
        if let Err(e) =
            self.playout_body(root, seat, det, action, follow, opp, m_inner, log, &mut out)
        {
            out.failed = Some(e);
            out.score = f64::NAN;
        }
        out.wall = started.elapsed().as_secs_f64();
        out
    }

    #[allow(
        clippy::too_many_arguments,
        reason = "one playout is fully described by these inputs"
    )]
    fn playout_body(
        &mut self,
        root: &FastActorSessionV1,
        seat: usize,
        det: u64,
        action: u32,
        follow: Follow,
        opp: OppSel,
        m_inner: usize,
        log: bool,
        out: &mut Outcome,
    ) -> Result<(), String> {
        let SearchWorkerV1 {
            focal,
            opps,
            t1_opp,
            scorer,
            inner_focal,
            inner_opp,
        } = self;
        let opp = match opp {
            OppSel::T1 => t1_opp,
            OppSel::Model(i) => &mut opps[i],
        };
        let FastActorResponseV1::Decision(d) = root.current_response() else {
            return Err("root is terminal".into());
        };
        let root_turn = root.game_state().turn;
        let mut s = root.census_redeterminized_clone_v1(det)?;
        s.step(d.episode_id, d.step, action)
            .map_err(|e| format!("root step {action}: {e:?}"))?;
        out.transitions += 1;
        let (fs, os) = policy_seeds(det);
        focal.reset_sampling_v1(fs);
        opp.reset_sampling_v1(os);
        let Follow::Improved(horizon) = follow else {
            let (score, natural) = plain_to_end(&mut s, seat, focal, opp, &mut out.transitions)?;
            out.score = score;
            out.natural = natural;
            return Ok(());
        };
        let mut ordinal = 0u32;
        loop {
            let response = s.current_response();
            if let Some((score, natural)) = terminal_score(&response, seat) {
                out.score = score;
                out.natural = natural;
                return Ok(());
            }
            let FastActorResponseV1::Decision(e) = response else {
                unreachable!("non-terminal response is a decision")
            };
            let a = if seat_index(e.acting_player) == seat {
                let sampled = focal.select_fast_session_v1(&s)?;
                let within = match horizon {
                    Horizon::Turn => s.game_state().turn == root_turn,
                    Horizon::Decisions(n) => ordinal < n,
                    Horizon::Game => true,
                };
                if e.legal_action_count >= 2 && within {
                    ordinal += 1;
                    out.searched += 1;
                    let dec_seed = mix(det ^ mix(0x1A7E_0000 ^ u64::from(ordinal)));
                    improve_decision(
                        &s,
                        seat,
                        sampled,
                        dec_seed,
                        m_inner,
                        scorer,
                        inner_focal,
                        inner_opp,
                        out,
                        log,
                    )
                } else {
                    sampled
                }
            } else {
                opp.select_fast_session_v1(&s)?
            };
            s.step(e.episode_id, e.step, a)
                .map_err(|err| format!("{err:?}"))?;
            out.transitions += 1;
        }
    }

    #[allow(
        clippy::too_many_arguments,
        reason = "one playout is fully described by these inputs"
    )]
    fn cached_playout(
        &mut self,
        cache: &mut PlayoutCache,
        root: &FastActorSessionV1,
        seat: usize,
        det: u64,
        action: u32,
        follow: Follow,
        opp: OppSel,
        m_inner: usize,
        log: bool,
    ) -> Outcome {
        let key = (det, action, follow, opp);
        if let Some(o) = cache.map.get(&key) {
            return o.clone();
        }
        let o = self.playout(root, seat, det, action, follow, opp, m_inner, log);
        cache.actual_transitions += o.transitions;
        cache.actual_wall += o.wall;
        cache.actual_playouts += 1;
        cache.map.insert(key, o.clone());
        o
    }

    /// Round-robin selection over `cands` with follow-up `follow` (opponent
    /// modelled as T1), recording the choice at each budget level.
    #[allow(
        clippy::too_many_arguments,
        reason = "selection is fully described by these inputs"
    )]
    fn select(
        &mut self,
        cache: &mut PlayoutCache,
        root: &FastActorSessionV1,
        seat: usize,
        cands: &[usize],
        probs: &[f64],
        sem: &[String],
        follow: Follow,
        ns: u64,
        budgets: &[u64],
        m_inner: usize,
        stop: BudgetStop,
    ) -> Value {
        let n = cands.len();
        let mut sums = vec![0f64; n];
        let mut rounds = 0u64;
        let mut attempted = 0u64;
        let (mut transitions, mut wall) = (0u64, 0f64);
        let (mut failed_rounds, mut consecutive) = (0u32, 0u32);
        let (mut inner_failures, mut searched) = (0u64, 0u64);
        let mut failures: Vec<String> = Vec::new();
        let mut levels: Vec<Value> = Vec::new();
        let record = |levels: &mut Vec<Value>,
                      budget: u64,
                      sums: &[f64],
                      rounds: u64,
                      transitions: u64,
                      wall: f64,
                      exhausted: bool| {
            let means: Vec<f64> = sums
                .iter()
                .map(|s| {
                    if rounds > 0 {
                        s / rounds as f64
                    } else {
                        f64::NAN
                    }
                })
                .collect();
            let best = if rounds > 0 {
                best_by_mean(cands, &means, probs)
            } else {
                0
            };
            let chosen = cands[best];
            levels.push(
                json!({"budget":budget,"transitions":transitions,"wall":wall,
                "rounds":rounds,"means":if rounds > 0 { json!(means) } else { Value::Null },
                "chosen":chosen,"chosen_prob":probs[chosen],"chosen_sem":sem[chosen],
                "exhausted":exhausted}),
            );
        };
        let mut next_level = 0usize;
        while next_level < budgets.len() {
            if attempted >= MAX_ROUNDS || consecutive >= MAX_FAILED_ROUNDS {
                while next_level < budgets.len() {
                    record(
                        &mut levels,
                        budgets[next_level],
                        &sums,
                        rounds,
                        transitions,
                        wall,
                        true,
                    );
                    next_level += 1;
                }
                break;
            }
            let det = mix(ns ^ (attempted + 1));
            attempted += 1;
            let mut scores = Vec::with_capacity(n);
            let (mut ok, mut abandoned) = (true, false);
            for (i, &c) in cands.iter().enumerate() {
                let o = self.cached_playout(
                    cache,
                    root,
                    seat,
                    det,
                    c as u32,
                    follow,
                    OppSel::T1,
                    m_inner,
                    false,
                );
                transitions += o.transitions;
                wall += o.wall;
                inner_failures += u64::from(o.inner_failures);
                searched += u64::from(o.searched);
                if let Some(e) = o.failed {
                    if failures.len() < 3 {
                        failures.push(e.chars().take(300).collect());
                    }
                    ok = false;
                    break;
                }
                scores.push(o.score);
                if stop == BudgetStop::Exact && i + 1 < n {
                    while next_level < budgets.len() && transitions >= budgets[next_level] {
                        record(
                            &mut levels,
                            budgets[next_level],
                            &sums,
                            rounds,
                            transitions,
                            wall,
                            false,
                        );
                        next_level += 1;
                    }
                    if next_level == budgets.len() {
                        abandoned = true;
                        break;
                    }
                }
            }
            if abandoned {
                break;
            }
            if ok {
                for (s, x) in sums.iter_mut().zip(&scores) {
                    *s += x;
                }
                rounds += 1;
                consecutive = 0;
            } else {
                failed_rounds += 1;
                consecutive += 1;
            }
            while next_level < budgets.len() && transitions >= budgets[next_level] {
                record(
                    &mut levels,
                    budgets[next_level],
                    &sums,
                    rounds,
                    transitions,
                    wall,
                    false,
                );
                next_level += 1;
            }
        }
        json!({"follow":follow.label(),"budget_stop":stop.label(),"candidates":cands,
            "candidate_probs":cands.iter().map(|&c| probs[c]).collect::<Vec<_>>(),
            "levels":levels,"rounds_attempted":attempted,"failed_rounds":failed_rounds,
            "failures":failures,"inner_failures":inner_failures,"searched_decisions":searched})
    }

    /// E paired playouts per job from fresh determinizations of the root.
    /// A determinization on which any job fails is discarded for every job
    /// (pairing is kept) and counted.
    #[allow(
        clippy::too_many_arguments,
        reason = "evaluation is fully described by these inputs"
    )]
    fn evaluate(
        &mut self,
        root: &FastActorSessionV1,
        seat: usize,
        jobs: &[(u32, Follow)],
        opp: OppSel,
        ns: u64,
        e: usize,
        m_inner: usize,
        log: bool,
    ) -> Value {
        let max_attempts = 3 * e as u64 + 8;
        let mut per_job: Vec<Vec<Outcome>> = vec![Vec::new(); jobs.len()];
        let mut dets = Vec::new();
        let (mut attempted, mut discarded, mut failed) = (0u64, 0u32, 0u32);
        let (mut actual_transitions, mut actual_wall) = (0u64, 0f64);
        let mut failures: Vec<String> = Vec::new();
        while dets.len() < e && attempted < max_attempts {
            let det = mix(ns ^ (attempted + 1));
            attempted += 1;
            let mut outs = Vec::with_capacity(jobs.len());
            for &(a, follow) in jobs {
                let o = self.playout(root, seat, det, a, follow, opp, m_inner, log);
                actual_transitions += o.transitions;
                actual_wall += o.wall;
                if let Some(err) = &o.failed {
                    failed += 1;
                    if failures.len() < 3 {
                        failures.push(err.chars().take(300).collect());
                    }
                    break;
                }
                outs.push(o);
            }
            if outs.len() < jobs.len() {
                discarded += 1;
                continue;
            }
            for (j, o) in outs.into_iter().enumerate() {
                per_job[j].push(o);
            }
            dets.push(det);
        }
        let jobs_json: Vec<Value> = jobs
            .iter()
            .zip(&per_job)
            .map(|(&(a, follow), outs)| {
                let deviations: Vec<Value> = outs
                    .iter()
                    .enumerate()
                    .flat_map(|(i, o)| {
                        o.deviations.iter().map(move |d| {
                            let mut d = d.clone();
                            d["playout"] = json!(i);
                            d
                        })
                    })
                    .collect();
                json!({"action":a,"follow":follow.label(),
                    "scores":outs.iter().map(|o| o.score).collect::<Vec<_>>(),
                    "natural":outs.iter().map(|o| o.natural).collect::<Vec<_>>(),
                    "transitions":outs.iter().map(|o| o.transitions).collect::<Vec<_>>(),
                    "wall":outs.iter().map(|o| o.wall).collect::<Vec<_>>(),
                    "non_natural":outs.iter().filter(|o| !o.natural).count(),
                    "inner_failures":outs.iter().map(|o| o.inner_failures).sum::<u32>(),
                    "searched_decisions":outs.iter().map(|o| o.searched).sum::<u32>(),
                    "deviations":deviations})
            })
            .collect();
        json!({"playouts":dets.len(),"attempted":attempted,"discarded_dets":discarded,
            "failed_playouts":failed,"failures":failures,"actual_transitions":actual_transitions,
            "actual_wall":actual_wall,"jobs":jobs_json})
    }
}

fn root_seed(setup: &GameSetup, step: u64) -> u64 {
    mix(setup.seed ^ mix(0x5200_0000_0000 ^ step))
}

/// Root identity fields carried into every result row.
fn root_id(root: &Value) -> Value {
    let mut id = serde_json::Map::new();
    for key in [
        "root_id",
        "set",
        "mechanism_tag",
        "game",
        "step",
        "turn",
        "phase",
        "own_turn",
        "actor",
        "k",
        "menu_hash",
        "stratum",
        "tags",
        "deck",
        "opp_deck",
        "opp_model",
        "focal_score",
    ] {
        if let Some(v) = root.get(key) {
            id.insert(key.into(), v.clone());
        }
    }
    Value::Object(id)
}

fn write_row(sink: &Mutex<std::fs::File>, row: &Value) -> Result<(), String> {
    let mut f = sink.lock().map_err(|_| "sink poisoned")?;
    writeln!(f, "{row}").map_err(|e| e.to_string())?;
    f.flush().map_err(|e| e.to_string())
}

/// Probabilities, semantics and candidate sets at a replayed root.
struct RootView {
    probs: Vec<f64>,
    sem: Vec<String>,
    top: Vec<usize>,
    cover: Vec<usize>,
    coverage_differs: bool,
}

fn root_view(
    cfg: &CensusConfigV1,
    w: &mut SearchWorkerV1,
    rp: &Replayed,
    seed: u64,
) -> Result<RootView, String> {
    w.scorer.reset_sampling_v1([1, 2]);
    let probs = softmax(&w.scorer.score_fast_session_v1(&rp.session)?.logits);
    let (top, cover, coverage_differs) = candidate_sets(&probs, cfg.max_actions, mix(seed ^ 0xB0D));
    Ok(RootView {
        sem: sem_strings(&rp.session),
        probs,
        top,
        cover,
        coverage_differs,
    })
}

/// Mode `cond`: stage-2 conditions A-D at each budget level, then paired
/// evaluation of every choice and of T1's own root action.
pub(super) fn run_cond_root(
    cfg: &CensusConfigV1,
    shared: &SearchSharedV1,
    w: &mut SearchWorkerV1,
    index: u64,
    sink: &Mutex<std::fs::File>,
) -> Result<(), String> {
    let root_started = Instant::now();
    let root = &shared.roots[index as usize];
    let rp = replay_root(cfg, shared, w, root)?;
    let seat = rp.setup.focal;
    let seed = root_seed(&rp.setup, root["step"].as_u64().unwrap_or(0));
    let view = root_view(cfg, w, &rp, seed)?;
    let improved = Follow::Improved(shared.horizon);
    let conditions: [(&str, &Vec<usize>, Follow); 4] = [
        ("A", &view.top, Follow::T1),
        ("B", &view.cover, Follow::T1),
        ("C", &view.top, improved),
        ("D", &view.cover, improved),
    ];
    let sel_ns = mix(seed ^ 0x5E1);
    let mut cache = PlayoutCache::default();
    let sel_started = Instant::now();
    let mut selection = serde_json::Map::new();
    for (name, cands, follow) in conditions {
        let s = w.select(
            &mut cache,
            &rp.session,
            seat,
            cands,
            &view.probs,
            &view.sem,
            follow,
            sel_ns,
            &shared.budgets,
            shared.m_inner,
            shared.budget_stop,
        );
        eprintln!(
            "root {index} condition {name}: {:.1}s elapsed, actual {} transitions",
            sel_started.elapsed().as_secs_f64(),
            cache.actual_transitions
        );
        selection.insert(name.into(), s);
    }
    let sel_elapsed = sel_started.elapsed().as_secs_f64();
    // Evaluation jobs: each distinct (action, follow-up) once.
    let mut jobs: Vec<(u32, Follow)> = vec![(rp.t1_action, Follow::T1)];
    let mut job_of = serde_json::Map::new();
    job_of.insert("ref".into(), json!(0));
    for (name, _, follow) in conditions {
        for level in selection[name]["levels"].as_array().into_iter().flatten() {
            let a = level["chosen"].as_u64().unwrap_or(0) as u32;
            let job = (a, follow);
            let j = jobs.iter().position(|x| *x == job).unwrap_or_else(|| {
                jobs.push(job);
                jobs.len() - 1
            });
            job_of.insert(format!("{name}@{}", level["budget"]), json!(j));
        }
    }
    let eval_ns = mix(seed ^ 0xE7A1);
    let eval = w.evaluate(
        &rp.session,
        seat,
        &jobs,
        OppSel::Model(rp.setup.model),
        eval_ns,
        shared.eval_playouts,
        shared.m_inner,
        false,
    );
    let mut row = root_id(root);
    let extra = json!({"kind":"cond","root_index":index,"t1_action":rp.t1_action,
        "t1_action_sem":view.sem[rp.t1_action as usize],
        "t1_action_prob":view.probs[rp.t1_action as usize],
        "probs":view.probs,"top":view.top,"cover":view.cover,
        "coverage_differs":view.coverage_differs,
        "candidate_sem":view.top.iter().chain(&view.cover).map(|&c| (c.to_string(), json!(view.sem[c]))).collect::<serde_json::Map<_,_>>(),
        "horizon":shared.horizon.label(),"m_inner":shared.m_inner,"budgets":shared.budgets,
        "budget_stop":shared.budget_stop.label(),"selection":selection,"selection_wall":sel_elapsed,
        "selection_actual":{"transitions":cache.actual_transitions,"wall":cache.actual_wall,"playouts":cache.actual_playouts},
        "eval_jobs":job_of,"eval":eval,"replay_secs":rp.secs,
        "root_wall":root_started.elapsed().as_secs_f64()});
    if let (Value::Object(r), Value::Object(x)) = (&mut row, extra) {
        r.extend(x);
    }
    write_row(sink, &row)
}

/// Mode `cross`: T1's root action or the search alternative (condition B at
/// the top budget), crossed with T1, turn-long and game-long improved
/// continuation; deviations from T1 are logged for classification.
pub(super) fn run_cross_root(
    cfg: &CensusConfigV1,
    shared: &SearchSharedV1,
    w: &mut SearchWorkerV1,
    index: u64,
    sink: &Mutex<std::fs::File>,
) -> Result<(), String> {
    let root_started = Instant::now();
    let root = &shared.roots[index as usize];
    let rp = replay_root(cfg, shared, w, root)?;
    let seat = rp.setup.focal;
    let seed = root_seed(&rp.setup, root["step"].as_u64().unwrap_or(0));
    let view = root_view(cfg, w, &rp, seed)?;
    let mut cache = PlayoutCache::default();
    let sel_started = Instant::now();
    let b = w.select(
        &mut cache,
        &rp.session,
        seat,
        &view.cover,
        &view.probs,
        &view.sem,
        Follow::T1,
        mix(seed ^ 0x5E1),
        &shared.budgets,
        shared.m_inner,
        shared.budget_stop,
    );
    let sel_elapsed = sel_started.elapsed().as_secs_f64();
    let top_level = b["levels"]
        .as_array()
        .and_then(|l| l.last())
        .cloned()
        .unwrap_or(Value::Null);
    let mut alt = top_level["chosen"].as_u64().unwrap_or(0) as usize;
    let mut forced = false;
    if alt == rp.t1_action as usize {
        forced = true;
        let means: Vec<f64> = top_level["means"]
            .as_array()
            .map(|m| m.iter().map(|x| x.as_f64().unwrap_or(f64::NAN)).collect())
            .unwrap_or_else(|| vec![f64::NAN; view.cover.len()]);
        let others: Vec<usize> = (0..view.cover.len())
            .filter(|&i| view.cover[i] != alt)
            .collect();
        let cands: Vec<usize> = others.iter().map(|&i| view.cover[i]).collect();
        let m: Vec<f64> = others
            .iter()
            .map(|&i| {
                if means[i].is_nan() {
                    f64::NEG_INFINITY
                } else {
                    means[i]
                }
            })
            .collect();
        alt = cands[best_by_mean(&cands, &m, &view.probs)];
    }
    let follows = [
        Follow::T1,
        Follow::Improved(Horizon::Turn),
        Follow::Improved(Horizon::Game),
    ];
    let inits = [("t1", rp.t1_action), ("alt", alt as u32)];
    let mut jobs = Vec::new();
    let mut cells = Vec::new();
    for (init, a) in inits {
        for f in follows {
            jobs.push((a, f));
            cells.push(format!("{init}|{}", f.label()));
        }
    }
    let eval = w.evaluate(
        &rp.session,
        seat,
        &jobs,
        OppSel::Model(rp.setup.model),
        mix(seed ^ 0xE7A1),
        shared.eval_playouts,
        shared.m_inner,
        true,
    );
    let mut row = root_id(root);
    let extra = json!({"kind":"cross","root_index":index,"t1_action":rp.t1_action,
        "t1_action_sem":view.sem[rp.t1_action as usize],
        "t1_action_prob":view.probs[rp.t1_action as usize],
        "alt":alt,"alt_sem":view.sem[alt],"alt_prob":view.probs[alt],"alt_forced":forced,
        "cover":view.cover,"b_selection":b,"selection_wall":sel_elapsed,
        "m_inner":shared.m_inner,"budgets":shared.budgets,"budget_stop":shared.budget_stop.label(),"cells":cells,"eval":eval,
        "replay_secs":rp.secs,"root_wall":root_started.elapsed().as_secs_f64()});
    if let (Value::Object(r), Value::Object(x)) = (&mut row, extra) {
        r.extend(x);
    }
    write_row(sink, &row)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_cfg() -> CensusConfigV1 {
        CensusConfigV1 {
            source: String::new(),
            out: String::new(),
            first_game: 0,
            games: 1,
            base_seed: 2026100711,
            root_prob: 0.0,
            rollouts: 1,
            max_actions: 4,
            workers: 1,
            decks: (0..9).collect(),
            mode: "roots".into(),
            pilot_deck: 0,
        }
    }

    #[test]
    fn search_root_replays_with_matching_menu_hash() {
        let cfg = test_cfg();
        // Game 4 * 9 + 7: focal CawGates (7) against Spy (4), focal seat 0.
        let setup = game_setup(&cfg, 1, 4 * 9 + 7);
        assert_eq!(setup.decks, [7, 4]);
        let mut focal = FrozenPlayPolicyV1::training_fixture_v4();
        let mut opp = FrozenPlayPolicyV1::training_fixture_v4();
        let mut rows = Vec::new();
        let (_, _) = drive_game(&setup, &mut focal, &mut opp, |s, d, a| {
            if seat_index(d.acting_player) == setup.focal && d.legal_action_count >= 2 {
                rows.push(root_row(
                    &setup,
                    "fixture",
                    d,
                    a,
                    &menu_info(s, setup.focal)?,
                ));
            }
            Ok(rows.len() >= 12)
        })
        .unwrap();
        assert!(rows.len() >= 2, "too few focal multi-action decisions");
        for root in [&rows[1], rows.last().unwrap()] {
            let mut focal = FrozenPlayPolicyV1::training_fixture_v4();
            let mut opp = FrozenPlayPolicyV1::training_fixture_v4();
            let (session, a) =
                replay_core(&setup, &mut focal, &mut opp, root["step"].as_u64().unwrap()).unwrap();
            check_root(&session, a, root, setup.focal).unwrap();
            assert_eq!(
                json!(menu_info(&session, setup.focal).unwrap().menu_hash),
                root["menu_hash"]
            );
        }
        // A wrong hash is a mismatch, never a silent skip.
        let mut bad = rows[1].clone();
        bad["menu_hash"] = json!("0".repeat(64));
        let mut focal = FrozenPlayPolicyV1::training_fixture_v4();
        let mut opp = FrozenPlayPolicyV1::training_fixture_v4();
        let (session, a) =
            replay_core(&setup, &mut focal, &mut opp, bad["step"].as_u64().unwrap()).unwrap();
        assert!(check_root(&session, a, &bad, setup.focal)
            .unwrap_err()
            .contains("menu_hash"));
    }

    #[test]
    fn exact_budget_stop_records_levels_at_the_crossing() {
        let cfg = test_cfg();
        let setup = game_setup(&cfg, 1, 4 * 9 + 7);
        let fixture = FrozenPlayPolicyV1::training_fixture_v4;
        let (mut focal, mut opp) = (fixture(), fixture());
        let mut root = None;
        drive_game(&setup, &mut focal, &mut opp, |s, d, a| {
            if seat_index(d.acting_player) == setup.focal && d.legal_action_count >= 3 {
                let row = root_row(&setup, "fixture", d, a, &menu_info(s, setup.focal)?);
                root = Some(row["step"].as_u64().unwrap());
            }
            Ok(root.is_some())
        })
        .unwrap();
        let (mut focal, mut opp) = (fixture(), fixture());
        let (session, _) = replay_core(&setup, &mut focal, &mut opp, root.unwrap()).unwrap();
        let cands = [0usize, 1, 2];
        let (probs, sem) = (vec![0.5, 0.3, 0.2], vec![String::new(); 3]);
        let budgets = [1u64, 3000];
        let select = |stop| {
            let mut w = SearchWorkerV1 {
                focal: fixture(),
                opps: vec![fixture()],
                t1_opp: fixture(),
                scorer: fixture(),
                inner_focal: fixture(),
                inner_opp: fixture(),
            };
            w.select(
                &mut PlayoutCache::default(),
                &session,
                setup.focal,
                &cands,
                &probs,
                &sem,
                Follow::T1,
                7,
                &budgets,
                1,
                stop,
            )
        };
        let (round, exact) = (select(BudgetStop::Round), select(BudgetStop::Exact));
        assert_eq!(exact["budget_stop"], json!("exact"));
        let level = |s: &Value, i: usize, key: &str| s["levels"][i][key].as_u64().unwrap();
        // Budget 1 is crossed by the first playout: the round-stop level
        // waits for the whole round, the exact level has no complete round.
        assert_eq!(level(&round, 0, "rounds"), 1);
        assert_eq!(level(&exact, 0, "rounds"), 0);
        assert!(exact["levels"][0]["means"].is_null());
        assert!(level(&exact, 0, "transitions") < level(&round, 0, "transitions"));
        for i in 0..budgets.len() {
            assert!(level(&exact, i, "transitions") >= budgets[i]);
            assert!(level(&exact, i, "transitions") <= level(&round, i, "transitions"));
            let (r, e) = (level(&round, i, "rounds"), level(&exact, i, "rounds"));
            assert!(
                e == r || e + 1 == r,
                "level {i}: exact {e} rounds, round {r}"
            );
            if e == r {
                assert_eq!(exact["levels"][i]["means"], round["levels"][i]["means"]);
            }
        }
        assert!(level(&round, 1, "rounds") >= 2, "budget too small to test");
        assert_eq!(BudgetStop::parse("exact").unwrap(), BudgetStop::Exact);
        assert!(BudgetStop::parse("never").is_err());
    }

    #[test]
    fn candidate_sets_cover_low_probability_actions() {
        let probs = [0.4, 0.3, 0.1, 0.08, 0.06, 0.04, 0.02];
        let (top, cover, _) = candidate_sets(&probs, 4, 7);
        assert_eq!(top, vec![0, 1, 2, 3]);
        assert_eq!(&cover[..2], &[0, 1]);
        assert_eq!(cover.len(), 4);
        assert!(cover[2..].iter().all(|&c| c >= 2));
        assert_ne!(cover[2], cover[3]);
        let (top, cover, differs) = candidate_sets(&probs[..3], 4, 7);
        assert_eq!(top, cover);
        assert!(!differs);
        assert_eq!(
            Horizon::parse("decisions:3").unwrap(),
            Horizon::Decisions(3)
        );
        assert_eq!(Horizon::parse("turn").unwrap(), Horizon::Turn);
        assert!(Horizon::parse("forever").is_err());
    }
}
