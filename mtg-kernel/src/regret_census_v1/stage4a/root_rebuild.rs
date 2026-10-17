//! Root rebuild for search-evidence distillation (SD1), mode
//! `s4a-root-rebuild`: a no-search replay of one recorded `s4a-budget`
//! search game that checks its record and, at every decision where a new
//! search tree executed its root edge (chooser `new_root`), rebuilds the map
//! from the root's storage edges to live actions.
//!
//! The map follows the executor's own code path (`refresh.rs` `Cursor`): a
//! fresh repaired-v2 keyer with no interchange and no visible history (the
//! guide context of every search), one step at the root, the storage view,
//! then `live_index` for every edge. The record's root hashes are legacy
//! canonical hashes, so they cannot tell a legacy view from the repaired
//! one; the executed edge's map can, and it is checked.
//!
//! Every failed check voids the game with its reason (nothing is dropped
//! silently):
//! - record: a search-arm `budget_game` row with repaired-v2 keys and no
//!   interchange or best-line backup fields, a valid natural game and, when
//!   a schedule is given (`S4A_ROOT_SCHEDULE`), every case field the record
//!   carries equal to its case's there (id, decks, seat, opponent, starting
//!   player, seed, guide seeds as `live_policy_seeds`, search root). Per
//!   search: root visits are the sum of n (a sum past 2^64 voids), n and
//!   wins have the root's menu size (its recorded root decision's k), and
//!   wins never exceed n. Per decision, by chooser: at `new_root` the named
//!   search was started at the decision's step, and n at its frozen choice
//!   equals the decision's edge backups and is at least eight (the floor
//!   below which the executor executes no edge); at `reused` the named
//!   search is earlier and the edge backups (a deeper node's count, which
//!   the record does not keep) are at least eight; at `plain` the edge
//!   backups are null and a named search is not later, at the same step
//!   only when `refreshed` is set. The searches correspond one to one, in
//!   order, to the decisions with `refreshed` set (the first "initial"),
//!   each with that decision's step and `refreshed` as its step and reason,
//!   and every decision names the search of the latest such decision at or
//!   before it (`refresh.rs`, where only a search moves the cursor);
//! - replay: every recorded action is legal at its recorded step (and, for
//!   focal choices, physical decision, substep and k, with the recorded Spy
//!   line flags, which the bank checks too); forced focal decisions take
//!   action 0; the end and the line-tracker `sequence` equal the record's
//!   (what `s4a-budget-replay`'s `matches_live` compares); at each
//!   `new_root` decision the rebuilt root's identity (observation, menu and
//!   state hashes) equals the search's recorded root, the map has one entry
//!   per edge of n, and the frozen choice maps to the executed action.
//!
//! Input errors ([`rows`]): a listed record file that cannot be read, or a
//! line of one that is not JSON, writes a void row with the schema, the
//! listed file, a null id and the error, and the run goes on to the next
//! line or file. A row of another kind or lineage is skipped without a
//! row. Two kinds of input error stop the whole run with no rows: a missing
//! or unreadable record list (`BUDGET_ROWS`), and any schedule error (a
//! schedule that cannot be read or is not UTF-8, a line of it that is not
//! JSON, a case without an id, or an id twice).
//!
//! A kept game also reports its focal starts (as `s4a-budget-replay` writes
//! them), each root's storage view (`untied`, `tie_minimum`, or the fallback
//! that replaced it, `work_limit` at a root), the decisions the evidence rule
//! leaves out (reused-tree and plain choices, the latter with `refreshed` and
//! `remaining_before`, and forced focal decisions). Nothing is searched and
//! no policy is called.

use super::bank::{
    decision_identity, err, guide_record, outcome, source_session, BankSpec, SourceSpec,
};
use super::canon2::{KeyMode, KeyStats, Keyer, Step as KeyStep};
use super::labels::{classify, spy_defs, SpyDefs, Suffix};
use super::play::{acting, apply, decision, observe, Meter};
use super::tree::{live_index, MIN_EXEC_BACKUPS};
use crate::ids::PlayerId;
use crate::rl_session::{FastActorDecisionV1, FastActorSessionV1};
use serde_json::{json, Value};
use std::collections::BTreeMap;

pub(super) const ROW_KIND: &str = "root_rebuild";
pub(super) const SCHEMA: &str = "sd1-root-rebuild-v1/game";
const RECORD_SCHEMA: &str = "search-total-budget-v1/game";
const KEY_MODE: &str = "repaired-v2";
const ENDS: [&str; 3] = ["win", "loss", "draw"];
const IDENTITY: [&str; 4] = ["step", "physical_decision_id", "substep", "k"];

/// A root's storage view and the live action of each of its edges.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct RootMap {
    /// The live candidate of each storage edge, in storage order.
    pub(super) map: Vec<u32>,
    /// `untied` (no tie class matters: the legacy canonical view),
    /// `tie_minimum` (the exact minimum over tie orders) or the label of the
    /// fallback that replaced it (`work_limit` at a root).
    pub(super) view: &'static str,
}

/// The map and view of one keyed root step, as `Cursor::live` maps an edge.
fn storage_map(keyer: &Keyer<KeyStats>, st: &KeyStep) -> Result<RootMap, String> {
    let (_, _, _, cn) = keyer.storage(st);
    let map = (0..cn.edges.len())
        .map(|e| live_index(cn, &cn.edges[e]).map_err(err))
        .collect::<Result<Vec<_>, _>>()?;
    let stats = &keyer.stats;
    let view = match stats.fallbacks.keys().next() {
        Some(label) => *label,
        None if stats.exact_resolved > 0 => "tie_minimum",
        None => "untied",
    };
    Ok(RootMap { map, view })
}

/// The root map of a new search at the live decision `d` of `s`: the
/// executor's cursor for a new tree (repaired-v2 keys, no interchange, no
/// history), stepped once.
pub(super) fn root_map(s: &FastActorSessionV1, d: FastActorDecisionV1) -> Result<RootMap, String> {
    let mut keyer = Keyer::new(KeyMode::Repaired, KeyStats::default()).with_interchange(false);
    let st = keyer.step(s, d).map_err(err)?;
    storage_map(&keyer, &st)
}

/// `root_map` on a raw actor-visible observation and menu (the input
/// `Keyer::step` reads from a live decision).
#[cfg(test)]
fn root_map_raw(obs: Value, menu: Vec<Value>) -> Result<RootMap, String> {
    let mut keyer = Keyer::new(KeyMode::Repaired, KeyStats::default()).with_interchange(false);
    let st = keyer.key_raw(obs, menu).map_err(err)?;
    storage_map(&keyer, &st)
}

/// The Spy line flags of a focal choice of `a`, by their record names, as
/// the executor records them (`refresh.rs`) and the bank checks them.
fn line_flags(
    s: &FastActorSessionV1,
    a: u32,
    focal: PlayerId,
    defs: &SpyDefs,
) -> [(&'static str, bool); 6] {
    let sem = s.diagnostic_current_action_semantics().unwrap_or_default();
    let (mut cast, mut own, mut giant) = (false, false, false);
    for x in &sem {
        let (c, o, g) = classify(x, s.game_state(), focal, defs);
        cast |= c;
        own |= o;
        giant |= g;
    }
    let (cc, oc, gc) = sem.get(a as usize).map_or((false, false, false), |x| {
        classify(x, s.game_state(), focal, defs)
    });
    [
        ("spy_cast_offered", cast),
        ("spy_cast_chosen", cc),
        ("spy_self_offered", own),
        ("spy_self_chosen", oc),
        ("dr_giant_offered", giant),
        ("dr_giant_chosen", gc),
    ]
}

fn count(v: &Value, what: &str) -> Result<u64, String> {
    v.as_u64()
        .ok_or_else(|| format!("{what} is missing or not a count"))
}

fn counts(v: &Value, what: &str) -> Result<Vec<u64>, String> {
    v.as_array()
        .ok_or_else(|| format!("{what} is missing"))?
        .iter()
        .map(|x| count(x, what))
        .collect()
}

/// Every case field a record carries, by its name in the record and in the
/// frozen schedule's case (the case's guide seeds are the record's
/// `live_policy_seeds`).
const SCHEDULED: [(&str, &str); 9] = [
    ("id", "id"),
    ("own", "own"),
    ("other", "other"),
    ("seat", "seat"),
    ("opp", "opp"),
    ("starting_player", "starting_player"),
    ("seed", "seed"),
    ("live_policy_seeds", "guide_policy_seeds"),
    ("search_root", "search_root"),
];

/// A frozen schedule (JSONL of cases, as `s4a-budget` reads them): each
/// case's fields a record carries, by id, and the file's SHA-256.
pub(super) struct Schedule {
    pub(super) sha256: String,
    cases: BTreeMap<String, Vec<Value>>,
}

impl Schedule {
    pub(super) fn parse(text: &str, sha256: String) -> Result<Self, String> {
        let mut cases = BTreeMap::new();
        for line in text.lines().filter(|l| !l.trim().is_empty()) {
            let case: Value = serde_json::from_str(line).map_err(|e| format!("schedule: {e}"))?;
            let id = case["id"]
                .as_str()
                .ok_or("schedule: a case has no id")?
                .to_owned();
            let fields = SCHEDULED.iter().map(|(_, f)| case[*f].clone()).collect();
            if cases.insert(id.clone(), fields).is_some() {
                return Err(format!("schedule: case {id} appears twice"));
            }
        }
        Ok(Self { sha256, cases })
    }

    /// Every case field the record carries equals its case's.
    fn check(&self, row: &Value) -> Result<(), String> {
        let id = row["id"].as_str().unwrap_or_default();
        let case = self
            .cases
            .get(id)
            .ok_or_else(|| format!("the schedule has no case {id}"))?;
        match SCHEDULED
            .iter()
            .zip(case)
            .find(|((f, _), v)| row[*f] != **v)
        {
            Some(((f, _), _)) => Err(format!("the record's {f} differs from its schedule case")),
            None => Ok(()),
        }
    }
}

/// A `new_root` decision's search and its root counts (after
/// `record_checks`).
struct Root<'a> {
    search: usize,
    record: &'a Value,
    n: Vec<u64>,
    wins: Vec<u64>,
    frozen_choice: usize,
}

/// The search a `new_root` decision executed from.
fn root_of<'a>(game: &'a Value, d: &Value) -> Result<Root<'a>, String> {
    let at = format!("step {}", d["step"]);
    let search = count(&d["search"], &format!("{at}: the decision's search"))? as usize;
    let record = game["searches"]
        .get(search)
        .ok_or_else(|| format!("{at}: search {search} is not recorded"))?;
    let root = &record["tree"]["root"];
    let n = counts(&root["n"], &format!("{at}: root n"))?;
    let frozen_choice =
        count(&root["frozen_choice"], &format!("{at}: the frozen choice"))? as usize;
    if frozen_choice >= n.len() {
        return Err(format!(
            "{at}: the frozen choice is not an edge of search {search}'s root"
        ));
    }
    Ok(Root {
        search,
        record,
        n,
        wins: counts(&root["wins"], &format!("{at}: root wins"))?,
        frozen_choice,
    })
}

/// One search's root counts: visits are the sum of n (a sum past 2^64
/// voids), n and wins have the root's menu size, and wins never exceed n.
/// The menu size is the recorded root decision's k: the tree's own
/// `menu_size` is written as n's length, so it cannot differ from it.
/// Returns the search's step.
fn search_checks(k: usize, s: &Value) -> Result<u64, String> {
    let at = format!("search {k}");
    if s["search"] != json!(k) {
        return Err(format!("{at} is recorded out of order"));
    }
    let step = count(&s["step"], &format!("{at}'s step"))?;
    let root = &s["tree"]["root"];
    let n = counts(&root["n"], &format!("{at}: root n"))?;
    let wins = counts(&root["wins"], &format!("{at}: root wins"))?;
    let sum = n
        .iter()
        .try_fold(0u64, |sum, &x| sum.checked_add(x))
        .ok_or_else(|| format!("{at}: the sum of root n overflows"))?;
    if count(&root["visits"], &format!("{at}: root visits"))? != sum {
        return Err(format!("{at}: root visits differ from the sum of n"));
    }
    let size = count(&s["root"]["k"], &format!("{at}: the root's menu size k"))?;
    if n.len() as u64 != size || wins.len() != n.len() {
        return Err(format!(
            "{at}: the lengths of n and wins differ from the root's menu size"
        ));
    }
    if wins.iter().zip(&n).any(|(w, t)| w > t) {
        return Err(format!("{at}: root wins exceed tries"));
    }
    Ok(step)
}

/// One recorded focal choice, by its chooser, against the steps of the
/// record's searches.
fn decision_checks(game: &Value, steps: &[u64], d: &Value) -> Result<(), String> {
    let at = format!("step {}", d["step"]);
    let step = count(&d["step"], &format!("{at}: the decision's step"))?;
    let named = || -> Result<(usize, u64), String> {
        let k = count(&d["search"], &format!("{at}: the decision's search"))? as usize;
        let s = steps
            .get(k)
            .ok_or_else(|| format!("{at}: search {k} is not recorded"))?;
        Ok((k, *s))
    };
    match d["chooser"].as_str() {
        Some("new_root") => {
            let (k, s) = named()?;
            if s != step {
                return Err(format!("{at}: search {k} is not a search at this step"));
            }
            let root = root_of(game, d)?;
            let backups = root.n[root.frozen_choice];
            if d["edge_backups"].as_u64() != Some(backups) {
                return Err(format!(
                    "{at}: n at the frozen choice differs from the decision's edge backups"
                ));
            }
            if backups < MIN_EXEC_BACKUPS {
                return Err(format!(
                    "{at}: the new root's executed edge has fewer than eight backups"
                ));
            }
        }
        Some("reused") => {
            let (k, s) = named()?;
            if s >= step {
                return Err(format!(
                    "{at}: a reused-tree decision names search {k}, which is not earlier"
                ));
            }
            if d["edge_backups"]
                .as_u64()
                .is_none_or(|b| b < MIN_EXEC_BACKUPS)
            {
                return Err(format!(
                    "{at}: a reused-tree decision's edge backups are not at least eight"
                ));
            }
        }
        Some("plain") => {
            if !d["edge_backups"].is_null() {
                return Err(format!("{at}: a plain decision records edge backups"));
            }
            if !d["search"].is_null() {
                let (k, s) = named()?;
                if s > step {
                    return Err(format!("{at}: a plain decision names a later search {k}"));
                }
                if s == step && d["refreshed"].is_null() {
                    return Err(format!(
                        "{at}: a plain decision names search {k} of its own step without a refresh"
                    ));
                }
            }
        }
        _ => return Err(format!("{at}: unknown chooser")),
    }
    Ok(())
}

/// The record checks that need no replay.
fn record_checks(row: &Value) -> Result<(), String> {
    if row["kind"] != "budget_game" || row["schema"] != RECORD_SCHEMA {
        return Err("not a budget_game record".into());
    }
    if row.get("arm").is_some() {
        return Err("not a search-arm record".into());
    }
    if row.get("interchange").is_some() || row.get("backup").is_some() {
        return Err("the record carries an interchange or backup field".into());
    }
    let game = &row["game"];
    if game["key_mode"] != KEY_MODE {
        return Err(format!(
            "the record's key mode {} is not {KEY_MODE}",
            game["key_mode"]
        ));
    }
    if game["valid"] != true || !ENDS.iter().any(|e| game["end"] == *e) {
        return Err("the record is not a valid natural game".into());
    }
    let steps = game["searches"]
        .as_array()
        .ok_or("the record has no searches")?
        .iter()
        .enumerate()
        .map(|(k, s)| search_checks(k, s))
        .collect::<Result<Vec<_>, _>>()?;
    let decisions = game["decisions"]
        .as_array()
        .ok_or("the record has no decisions")?;
    for d in decisions {
        decision_checks(game, &steps, d)?;
    }
    // Only a search (the initial one, then each refresh) moves the cursor.
    let refreshed: Vec<&Value> = decisions
        .iter()
        .filter(|d| !d["refreshed"].is_null())
        .collect();
    if refreshed.len() != steps.len() {
        return Err(format!(
            "the record has {} searches but {} decisions with refreshed set",
            steps.len(),
            refreshed.len()
        ));
    }
    for (k, (s, d)) in game["searches"]
        .as_array()
        .into_iter()
        .flatten()
        .zip(&refreshed)
        .enumerate()
    {
        if s["step"] != d["step"] || s["reason"] != d["refreshed"] {
            return Err(format!(
                "search {k}'s step or reason differs from its refreshed decision's"
            ));
        }
    }
    if refreshed
        .first()
        .is_some_and(|d| d["refreshed"] != "initial")
    {
        return Err("the first search's reason is not initial".into());
    }
    let mut latest: Option<u64> = None;
    for d in decisions {
        if !d["refreshed"].is_null() {
            latest = Some(latest.map_or(0, |k| k + 1));
        }
        if latest.is_none_or(|k| d["search"] != json!(k)) {
            return Err(format!(
                "step {}: the decision's search is not the latest refreshed decision's",
                d["step"]
            ));
        }
    }
    Ok(())
}

/// The case a record names, as `s4a-budget-replay` rebuilds it.
fn case(spec: &BankSpec, row: &Value) -> Result<SourceSpec, String> {
    let deck = |name: &str| -> Result<usize, String> {
        let i = count(&row[name], name)? as usize;
        if i >= spec.decks.len() {
            return Err(format!("{name} names no deck of the bank spec"));
        }
        Ok(i)
    };
    let seat = count(&row["seat"], "seat")?;
    let starting_player = count(&row["starting_player"], "starting_player")?;
    if seat > 1 || starting_player > 1 {
        return Err("seat or starting player is not 0 or 1".into());
    }
    Ok(SourceSpec {
        id: row["id"].as_str().ok_or("the record has no id")?.to_owned(),
        own: deck("own")?,
        other: deck("other")?,
        seat: seat as u8,
        opp: row["opp"].as_str().unwrap_or_default().to_owned(),
        seed: count(&row["seed"], "seed")?,
        starting_player: starting_player as u8,
        source_policy_seeds: [0, 0],
        anchor_draw: 0,
        guide_policy_seeds: [0, 0],
        search_root: String::new(),
    })
}

/// The replay and its root checks; Err is the reason the game is void.
fn replay(spec: &BankSpec, row: &Value) -> Result<Value, String> {
    let game = &row["game"];
    let src = case(spec, row)?;
    let actions: Vec<u32> = serde_json::from_value(game["actions"].clone())
        .map_err(|_| "the record's actions are not live indices")?;
    let decisions = game["decisions"]
        .as_array()
        .ok_or("the record has no decisions")?;
    let focal = PlayerId(src.seat);
    let defs = spy_defs();
    let opening = source_session(spec, &src)?;
    let mut s = opening.clone();
    let mut suffix = Suffix::new(true);
    let mut meter = Meter::new(u64::MAX);
    let mut recorded = decisions.iter();
    let (mut roots, mut one_action) = (Vec::new(), 0u64);
    for (i, &a) in actions.iter().enumerate() {
        let d = decision(&s).ok_or_else(|| format!("the replay ends before action {i}"))?;
        if d.step != i as u64 {
            return Err(format!("replay step {} differs from action {i}", d.step));
        }
        if a >= d.legal_action_count {
            return Err(format!(
                "step {i}: action {a} is outside the legal menu of {}",
                d.legal_action_count
            ));
        }
        if acting(&d) == focal {
            if d.legal_action_count >= 2 {
                let r = recorded
                    .next()
                    .ok_or_else(|| format!("step {i}: an unrecorded focal choice"))?;
                let live = [
                    d.step,
                    d.physical_decision_id,
                    u64::from(d.substep_index),
                    u64::from(d.legal_action_count),
                ];
                if IDENTITY.iter().zip(live).any(|(f, v)| r[*f] != v) {
                    return Err(format!(
                        "step {i}: the focal choice differs from its record's step, physical decision, substep or k"
                    ));
                }
                if line_flags(&s, a, focal, &defs)
                    .iter()
                    .any(|(f, v)| r[*f] != *v)
                {
                    return Err(format!(
                        "step {i}: the Spy line flags differ from the record"
                    ));
                }
                if r["chooser"] == "new_root" {
                    let root = root_of(game, r)?;
                    if decision_identity(&s)? != root.record["root"] {
                        return Err(format!(
                            "step {i}: the rebuilt root's identity (observation, menu or state hash) differs from search {}'s record",
                            root.search
                        ));
                    }
                    let rm = root_map(&s, d).map_err(|e| format!("step {i}: root map: {e}"))?;
                    if rm.map.len() != root.n.len() {
                        return Err(format!("step {i}: the root map's size differs from n"));
                    }
                    if rm.map[root.frozen_choice] != a {
                        return Err(format!(
                            "step {i}: the frozen choice maps to live action {} but the record executed {a}",
                            rm.map[root.frozen_choice]
                        ));
                    }
                    roots.push(json!({"step":d.step,"physical_decision_id":d.physical_decision_id,
                        "substep":d.substep_index,"k":d.legal_action_count,"search":root.search,
                        "view":rm.view,"map":rm.map,"frozen_choice":root.frozen_choice,"action":a,
                        "edge_backups":r["edge_backups"],"visits":root.record["tree"]["root"]["visits"],
                        "n":root.n,"wins":root.wins}));
                }
                observe(Some(&mut suffix), &s, a, focal, &defs);
            } else {
                one_action += 1;
                if a != 0 {
                    return Err(format!("step {i}: a forced focal decision takes action 0"));
                }
            }
        }
        apply(&mut s, &d, a, &mut meter, focal, Some(&mut suffix), &defs)
            .map_err(|e| format!("step {i}: {}", err(e)))?;
    }
    if recorded.next().is_some() {
        return Err("recorded focal choices outlast the replay".into());
    }
    let end = outcome(&s, focal);
    if json!(end) != game["end"] || suffix.json() != game["sequence"] {
        return Err(format!(
            "the replay's end ({}) or line-tracker sequence differs from the record",
            json!(end)
        ));
    }
    let (starts, _) = guide_record(&opening, focal, &actions)?;
    let tally = |c: &str| decisions.iter().filter(|d| d["chooser"] == c).count();
    let plain: Vec<Value> = decisions
        .iter()
        .filter(|d| d["chooser"] == "plain")
        .map(|d| json!({"step":d["step"],"refreshed":d["refreshed"],"remaining_before":d["remaining_before"]}))
        .collect();
    Ok(
        json!({"end":end,"actions":actions.len(),"starts":starts,"roots":roots,
        "first_choice_step":decisions.first().map(|d| d["step"].clone()),
        "choosers":{"new_root":tally("new_root"),"reused":tally("reused"),"plain":tally("plain")},
        "plain":plain,"one_action":one_action}),
    )
}

/// One recorded search game, checked and rebuilt: a `root_rebuild` row,
/// `void` null for a kept game or the reason it is void.
pub(super) fn game(spec: &BankSpec, row: &Value, schedule: Option<&Schedule>) -> Value {
    let mut out = header(row, schedule);
    match record_checks(row)
        .and_then(|()| schedule.map_or(Ok(()), |s| s.check(row)))
        .and_then(|()| replay(spec, row))
    {
        Ok(rebuilt) => {
            out["void"] = Value::Null;
            if let (Value::Object(o), Value::Object(r)) = (&mut out, rebuilt) {
                o.extend(r);
            }
        }
        Err(reason) => out["void"] = json!(reason),
    }
    out
}

/// A record's case fields, as every `root_rebuild` row starts, with the
/// SHA-256 of the schedule it was checked against (null without one).
fn header(row: &Value, schedule: Option<&Schedule>) -> Value {
    json!({"kind":ROW_KIND,"schema":SCHEMA,"id":row["id"],"lineage":row["lineage"],
        "own":row["own"],"other":row["other"],"seat":row["seat"],"opp":row["opp"],
        "seed":row["seed"],"starting_player":row["starting_player"],
        "key_mode":row["game"]["key_mode"],"schedule_sha256":schedule.map(|s| s.sha256.clone())})
}

/// The void row of a listed file that cannot be read, or of one of its
/// lines that is not JSON: no case field can be read, so the id is null.
fn input_void(file: &str, reason: String) -> String {
    format!(
        "{}\n",
        json!({"schema":SCHEMA,"file":file,"id":null,"void":reason})
    )
}

/// The rows of a record list (one record file per line), in list and line
/// order: a `root_rebuild` row for each `budget_game` row of the spec's
/// lineage, rebuilt on its own thread (a panic voids its game), and an
/// input void row for each listed file that cannot be read and each line
/// that is not JSON, after which the run continues. Rows of another kind or
/// lineage are skipped without a row.
pub(super) fn rows(spec: &BankSpec, list: &str, schedule: Option<&Schedule>) -> String {
    let mut out = String::new();
    for file in list.lines().map(str::trim).filter(|l| !l.is_empty()) {
        let text = match std::fs::read_to_string(file) {
            Ok(text) => text,
            Err(e) => {
                out.push_str(&input_void(
                    file,
                    format!("the listed file cannot be read: {e}"),
                ));
                continue;
            }
        };
        for (i, line) in text.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            let row: Value = match serde_json::from_str(line) {
                Ok(row) => row,
                Err(e) => {
                    out.push_str(&input_void(
                        file,
                        format!("line {} of the listed file is not JSON: {e}", i + 1),
                    ));
                    continue;
                }
            };
            if row["kind"] != "budget_game" || row["lineage"] != spec.lineage {
                continue;
            }
            let mut rebuilt = std::thread::scope(|scope| {
                std::thread::Builder::new()
                    .stack_size(256 << 20)
                    .spawn_scoped(scope, || game(spec, &row, schedule))
                    .map_err(|e| e.to_string())?
                    .join()
                    .map_err(|_| "root rebuild worker panicked".to_owned())
            })
            .unwrap_or_else(|e| {
                let mut v = header(&row, schedule);
                v["void"] = json!(e);
                v
            });
            rebuilt["file"] = json!(file);
            out.push_str(&format!("{rebuilt}\n"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::super::bank::{DeckSpec, SPEC_SCHEMA};
    use super::*;
    use crate::runtime_decks::RUNTIME_DECKS;

    /// A creature entry and a stack object, as `canon2.rs`'s fixtures.
    fn cr(id: u64) -> Value {
        json!({"stable":{"arena_id":id,"card_db_id":19,"zone":"Battlefield","zone_change_count":3},
            "damage":0,"tapped":false,"attachments":[]})
    }

    fn obj(id: u64) -> Value {
        json!({"arena_id":id,"card_db_id":19,"zone":"Battlefield","zone_change_count":3})
    }

    /// A view: the focal battlefield and, when given, a stack spell targeting
    /// `targets`.
    fn view(bf: Vec<Value>, targets: &[u64]) -> Value {
        let stack = if targets.is_empty() {
            json!([])
        } else {
            json!([{"source":{"arena_id":40,"card_db_id":41,"zone":"Stack","zone_change_count":0},
                "targets":targets.iter().map(|&t| json!({"object":obj(t),"target_kind":"object"})).collect::<Vec<_>>()}])
        };
        json!({"projection":{"battlefield":[[{"stable":{"arena_id":90,"card_db_id":5,"zone":"Battlefield",
            "zone_change_count":1},"damage":0,"tapped":false,"attachments":[]}], bf],"stack":stack,
            "object_relations":[]},"step_index":7})
    }

    fn pass() -> Value {
        json!({"action_kind":"pass"})
    }

    fn attack(id: u64) -> Value {
        json!({"action_kind":"attack","source":{"arena_id":id,"zone_change_count":3}})
    }

    /// The live action of each storage edge under `mode`'s storage view.
    fn map_in(mode: KeyMode, obs: Value, menu: Vec<Value>) -> Vec<u32> {
        let mut keyer = Keyer::new(mode, KeyStats::default());
        let st = keyer.key_raw(obs, menu).unwrap();
        storage_map(&keyer, &st).unwrap().map
    }

    fn is_permutation(map: &[u32]) -> bool {
        let mut sorted = map.to_vec();
        sorted.sort_unstable();
        sorted.into_iter().eq(0..map.len() as u32)
    }

    /// A root with no tie class whose live order is not its sorted order:
    /// "attack" sorts before "pass", so storage edge 0 is live action 1.
    #[test]
    fn known_map_where_live_order_differs_from_sorted_order() {
        let rm = root_map_raw(view(vec![cr(11)], &[]), vec![pass(), attack(11)]).unwrap();
        assert_eq!(
            rm,
            RootMap {
                map: vec![1, 0],
                view: "untied"
            }
        );
        assert_eq!(
            map_in(
                KeyMode::Legacy,
                view(vec![cr(11)], &[]),
                vec![pass(), attack(11)]
            ),
            rm.map
        );
    }

    /// Exact duplicate menu entries carry occurrence ordinals, so each binds
    /// one live action: the first "pass" sorts before "pass#1".
    #[test]
    fn duplicate_menu_entries_bind_through_occurrence_ordinals() {
        let rm = root_map_raw(view(vec![cr(11)], &[]), vec![pass(), attack(11), pass()]).unwrap();
        assert_eq!(rm.map, vec![1, 0, 2]);
        assert_eq!(rm.view, "untied");
        let rm = root_map_raw(
            view(vec![cr(11)], &[]),
            vec![attack(11), pass(), pass(), pass()],
        )
        .unwrap();
        assert_eq!(rm.map, vec![0, 1, 2, 3]);
    }

    /// Two identical creatures, one targeted on the stack, with an attack
    /// for each: a tie class that matters. The legacy view numbers the
    /// copies in engine order, so its map depends on the battlefield order;
    /// the repaired view takes the exact minimum over both orders, so its
    /// map does not, and at one of the two orders it differs from the
    /// legacy map.
    #[test]
    fn repaired_and_legacy_orders_differ_at_a_tied_root() {
        let menu = vec![pass(), attack(11), attack(12)];
        let orders = [vec![cr(11), cr(12)], vec![cr(12), cr(11)]];
        let repaired: Vec<RootMap> = orders
            .iter()
            .map(|bf| root_map_raw(view(bf.clone(), &[11]), menu.clone()).unwrap())
            .collect();
        let legacy: Vec<Vec<u32>> = orders
            .iter()
            .map(|bf| map_in(KeyMode::Legacy, view(bf.clone(), &[11]), menu.clone()))
            .collect();
        assert!(repaired.iter().all(|r| r.view == "tie_minimum"));
        assert_eq!(
            repaired[0].map, repaired[1].map,
            "repaired maps ignore engine order"
        );
        assert_ne!(legacy[0], legacy[1], "legacy maps follow engine order");
        assert!(legacy.iter().any(|l| *l != repaired[0].map));
        for m in legacy.iter().chain(repaired.iter().map(|r| &r.map)) {
            assert!(is_permutation(m));
            // Both attacks precede "pass" (live 0) in storage order.
            assert_eq!(m[2], 0);
        }
    }

    /// Six identical creatures, one targeted: 720 tie orders exceed the work
    /// bound, so the root's storage view is the WorkLimit fallback, which is
    /// the legacy canonical view.
    #[test]
    fn work_limit_root_maps_through_the_legacy_view() {
        let bf: Vec<Value> = (11..=16).map(cr).collect();
        let mut menu = vec![pass()];
        menu.extend((11..=16).map(attack));
        let rm = root_map_raw(view(bf.clone(), &[13]), menu.clone()).unwrap();
        assert_eq!(rm.view, "work_limit");
        assert!(is_permutation(&rm.map));
        assert_eq!(rm.map, map_in(KeyMode::Legacy, view(bf, &[13]), menu));
        assert_eq!(rm.map[6], 0, "pass sorts after every attack");
    }

    fn spec() -> BankSpec {
        BankSpec {
            schema: SPEC_SCHEMA.into(),
            namespace: "spy-reset-pilot-v1".into(),
            lineage: "fixture".into(),
            runtime_rules: "resolution-boundary-v1".into(),
            select_rule: "fpu-1.5".into(),
            max_physical_decisions: 100_000,
            max_policy_steps: 200_000,
            decks: RUNTIME_DECKS
                .iter()
                .map(|d| DeckSpec {
                    label: d.id.into(),
                    mainboard: d.card_ids.to_vec(),
                })
                .collect(),
            sources: Vec::new(),
        }
    }

    /// A synthetic search-arm record of a fixture game: every focal choice
    /// plays a fixed pattern of live actions, and the choosers cycle through
    /// the executor's four cases: a `new_root` decision whose new search
    /// root executes that action through the root map (n 10 on the executed
    /// edge with 6 wins, 2 tries and no win elsewhere); a reused-tree
    /// decision on the latest search (9 backups at its deeper node); a plain
    /// decision after the allowance ran out; and a plain decision right
    /// after a refresh whose new root has no edge with eight backups.
    fn record(spec: &BankSpec) -> Value {
        let src = SourceSpec {
            id: "fixture-o4-v5-s0".into(),
            own: 4,
            other: 5,
            seat: 0,
            opp: "T1".into(),
            seed: 11,
            starting_player: 0,
            source_policy_seeds: [0, 0],
            anchor_draw: 0,
            guide_policy_seeds: [0, 0],
            search_root: String::new(),
        };
        let focal = PlayerId(0);
        let defs = spy_defs();
        let mut s = source_session(spec, &src).unwrap();
        let mut suffix = Suffix::new(true);
        let mut meter = Meter::new(u64::MAX);
        let (mut actions, mut decisions, mut searches) = (Vec::new(), Vec::new(), Vec::new());
        let mut choices = 0u64;
        while let Some(d) = decision(&s) {
            let a = if acting(&d) == focal && d.legal_action_count >= 2 {
                let a = ((d.step * 7 + 3) % u64::from(d.legal_action_count)) as u32;
                let case = choices % 4;
                choices += 1;
                let chooser = ["new_root", "reused", "plain", "plain"][case as usize];
                let mut entry = json!({"step":d.step,"physical_decision_id":d.physical_decision_id,
                    "substep":d.substep_index,"k":d.legal_action_count,"chooser":chooser,
                    "search":searches.len().saturating_sub(1),"lookup":"supported","refreshed":null,
                    "edge_backups":null,"remaining_before":64_000,"remaining_after":64_000});
                for (f, v) in line_flags(&s, a, focal, &defs) {
                    entry[f] = json!(v);
                }
                if case == 0 || case == 3 {
                    // A new search at this decision.
                    let rm = root_map(&s, d).unwrap();
                    let mut n = vec![2u64; rm.map.len()];
                    let mut wins = vec![0u64; rm.map.len()];
                    let mut frozen = Value::Null;
                    if case == 0 {
                        let x = rm.map.iter().position(|&l| l == a).unwrap();
                        (n[x], wins[x]) = (10, 6);
                        frozen = json!(x);
                        entry["edge_backups"] = json!(10);
                    } else {
                        entry["lookup"] = json!("unsupported");
                        entry["remaining_before"] = json!(64_000);
                    }
                    let k = searches.len();
                    let reason = if k == 0 { "initial" } else { "miss" };
                    searches.push(json!({"search":k,"step":d.step,"reason":reason,"budget":64_000,
                        "root":decision_identity(&s).unwrap(),
                        "tree":{"nodes":1,"hash":"fixture","root":{"visits":n.iter().sum::<u64>(),"n":n,
                            "wins":wins,"menu_size":rm.map.len(),"qualified_edges":u64::from(case == 0),
                            "frozen_choice":frozen}}}));
                    entry["search"] = json!(k);
                    entry["refreshed"] = json!(reason);
                }
                if case == 1 {
                    entry["edge_backups"] = json!(9);
                }
                if case == 2 {
                    entry["lookup"] = json!("miss");
                    entry["remaining_before"] = json!(0);
                    entry["remaining_after"] = json!(0);
                }
                decisions.push(entry);
                observe(Some(&mut suffix), &s, a, focal, &defs);
                a
            } else {
                0
            };
            actions.push(a);
            apply(&mut s, &d, a, &mut meter, focal, Some(&mut suffix), &defs).unwrap();
        }
        let end = outcome(&s, focal).unwrap();
        assert_ne!(end, "nonnatural");
        json!({"kind":"budget_game","schema":RECORD_SCHEMA,"id":src.id,"lineage":"fixture",
            "own":src.own,"other":src.other,"seat":src.seat,"opp":src.opp,"seed":src.seed,
            "starting_player":src.starting_player,"live_policy_seeds":[0,0],"search_root":"fixture",
            "bank_binding_sha256":"fixture","allocation":{"initial":128_000,"refresh":64_000,"total":2_048_000},
            "game":{"valid":true,"invalid_reason":null,"end":end,"actions":actions,"decisions":decisions,
                "learner_starts":0,"searches":searches,"transitions_used":0,"key_mode":KEY_MODE,
                "search_keys":{},"execution_keys":{},"sequence":suffix.json(),"real_transitions":meter.spent,
                "wall":0.0},
            "wall":0.0})
    }

    fn void(spec: &BankSpec, row: &Value) -> String {
        game(spec, row, None)["void"]
            .as_str()
            .unwrap_or_else(|| panic!("the game was kept"))
            .to_owned()
    }

    /// A consistent record replays without search: its roots carry the
    /// maps that execute the recorded actions, its starts equal
    /// `s4a-budget-replay`'s, and every check failure voids the game with
    /// its reason, a hash mismatch at a root included.
    #[test]
    fn root_rebuild_keeps_a_consistent_record_and_voids_each_failed_check() {
        let spec = spec();
        let row = record(&spec);
        let out = game(&spec, &row, None);
        assert!(out["void"].is_null(), "{}", out["void"]);
        let roots = out["roots"].as_array().unwrap();
        let recorded = row["game"]["decisions"].as_array().unwrap();
        let new_roots = recorded
            .iter()
            .filter(|d| d["chooser"] == "new_root")
            .count();
        assert!(new_roots >= 5, "too few roots in the fixture game");
        // Reused-tree and plain decisions of both kinds pass the
        // chooser-scoped checks.
        let tally = |f: &dyn Fn(&Value) -> bool| recorded.iter().filter(|d| f(d)).count();
        assert!(tally(&|d| d["chooser"] == "reused") >= 5);
        assert!(tally(&|d| d["chooser"] == "plain" && d["refreshed"].is_null()) >= 5);
        assert!(tally(&|d| d["chooser"] == "plain" && !d["refreshed"].is_null()) >= 5);
        assert_eq!(
            out["choosers"]["reused"],
            tally(&|d| d["chooser"] == "reused")
        );
        assert_eq!(roots.len(), new_roots);
        assert_eq!(out["choosers"]["new_root"], new_roots);
        let mut reordered = 0;
        for r in roots {
            let map: Vec<u32> = serde_json::from_value(r["map"].clone()).unwrap();
            assert!(is_permutation(&map));
            let x = r["frozen_choice"].as_u64().unwrap() as usize;
            assert_eq!(json!(map[x]), r["action"]);
            assert_eq!(
                row["game"]["actions"][r["step"].as_u64().unwrap() as usize],
                r["action"]
            );
            reordered += usize::from(map.iter().enumerate().any(|(e, &l)| l != e as u32));
        }
        assert!(
            reordered > 0,
            "every root's live order was its storage order"
        );
        assert_eq!(
            out["plain"].as_array().unwrap().len(),
            recorded.iter().filter(|d| d["chooser"] == "plain").count()
        );
        assert!(out["one_action"].as_u64().unwrap() > 0);
        let sh = super::super::bank::BankShared {
            spec: spec.clone(),
            labels: Vec::new(),
            opponents: Vec::new(),
            prior: super::super::world::DeckPrior::new(&[0]),
            limits: super::super::arms::Limits::FORMAL,
        };
        let src = case(&spec, &row).unwrap();
        let actions: Vec<u32> = serde_json::from_value(row["game"]["actions"].clone()).unwrap();
        let replayed = super::super::refresh::budget_replay(&sh, &src, &actions).unwrap();
        assert_eq!(out["starts"], replayed["starts"]);
        assert_eq!(out["end"], replayed["end"]);

        let first = recorded
            .iter()
            .position(|d| d["chooser"] == "new_root")
            .unwrap();
        let reused = recorded
            .iter()
            .position(|d| d["chooser"] == "reused")
            .unwrap();
        let exhausted = recorded
            .iter()
            .position(|d| d["chooser"] == "plain" && d["refreshed"].is_null())
            .unwrap();
        let refreshed = recorded
            .iter()
            .position(|d| d["chooser"] == "plain" && !d["refreshed"].is_null())
            .unwrap();
        let last = row["game"]["searches"].as_array().unwrap().len() - 1;
        // A plain decision after the allowance ran out, on a later search.
        let stale = recorded
            .iter()
            .rposition(|d| d["chooser"] == "plain" && d["refreshed"].is_null())
            .unwrap();
        assert!(recorded[stale]["search"].as_u64().unwrap() > 0);
        let k = recorded[first]["search"].as_u64().unwrap() as usize;
        // The search of a refresh whose new root executed nothing: only
        // its own plain decision names it, so no replay check reads it.
        let unexecuted = recorded[refreshed]["search"].as_u64().unwrap() as usize;
        assert_eq!(
            recorded
                .iter()
                .filter(|d| d["search"] == json!(unexecuted))
                .count(),
            1
        );
        type Change = Box<dyn Fn(&mut Value)>;
        let at = move |r: &mut Value| -> Value { r["game"]["searches"][k].clone() };
        let set = move |r: &mut Value, v: Value| r["game"]["searches"][k] = v;
        let edit = move |f: fn(&mut Value)| -> Change {
            Box::new(move |r: &mut Value| {
                let mut s = at(r);
                f(&mut s);
                set(r, s);
            })
        };
        let cases: Vec<(&str, Change)> = vec![
            (
                "observation, menu or state hash",
                edit(|s| s["root"]["obs_hash"] = json!("0".repeat(64))),
            ),
            (
                "observation, menu or state hash",
                edit(|s| s["root"]["state_hash"] = json!("0".repeat(16))),
            ),
            (
                "observation, menu or state hash",
                edit(|s| s["root"]["physical_decision_id"] = json!(1_000_000)),
            ),
            (
                "key mode",
                Box::new(|r| r["game"]["key_mode"] = json!("legacy")),
            ),
            (
                "interchange or backup",
                Box::new(|r| r["interchange"] = json!("interchange-orbit-v1")),
            ),
            (
                "interchange or backup",
                Box::new(|r| r["backup"] = json!("bestline-m8")),
            ),
            ("search-arm", Box::new(|r| r["arm"] = json!("plain-v1"))),
            (
                "valid natural game",
                Box::new(|r| r["game"]["valid"] = json!(false)),
            ),
            ("visits", edit(|s| s["tree"]["root"]["visits"] = json!(1))),
            (
                "wins exceed",
                edit(|s| {
                    let x = s["tree"]["root"]["frozen_choice"].as_u64().unwrap() as usize;
                    s["tree"]["root"]["wins"][x] = json!(11);
                }),
            ),
            (
                "not a search at this step",
                edit(|s| s["step"] = json!(1_000_000)),
            ),
            (
                "edge backups",
                Box::new(move |r| r["game"]["decisions"][first]["edge_backups"] = json!(9)),
            ),
            // Another edge made the frozen choice with consistent counts: the
            // record checks pass, and only the map exposes it.
            (
                "frozen choice maps to live action",
                edit(|s| {
                    let t = &mut s["tree"]["root"];
                    let x = t["frozen_choice"].as_u64().unwrap() as usize;
                    let y = usize::from(x == 0);
                    let (n, w) = (t["n"].clone(), t["wins"].clone());
                    (t["n"][x], t["n"][y]) = (n[y].clone(), n[x].clone());
                    (t["wins"][x], t["wins"][y]) = (w[y].clone(), w[x].clone());
                    t["frozen_choice"] = json!(y);
                }),
            ),
            (
                "lengths of n and wins",
                edit(|s| {
                    s["tree"]["root"]["wins"]
                        .as_array_mut()
                        .unwrap()
                        .push(json!(0))
                }),
            ),
            // n one edge longer than its root's k, with the tree's menu size
            // and visits written as the executor writes them (from n), at a
            // search that only its record checks read.
            (
                "lengths of n and wins differ from the root's menu size",
                Box::new(move |r| {
                    let t = &mut r["game"]["searches"][unexecuted]["tree"]["root"];
                    for f in ["n", "wins"] {
                        t[f].as_array_mut().unwrap().push(json!(0));
                    }
                    t["menu_size"] = json!(t["n"].as_array().unwrap().len());
                }),
            ),
            // A new root executed an edge below the executor's floor, with
            // consistent counts and edge backups.
            (
                "fewer than eight backups",
                Box::new(move |r| {
                    let t = &mut r["game"]["searches"][k]["tree"]["root"];
                    let x = t["frozen_choice"].as_u64().unwrap() as usize;
                    let visits = t["visits"].as_u64().unwrap() - t["n"][x].as_u64().unwrap() + 5;
                    (t["n"][x], t["wins"][x], t["visits"]) = (json!(5), json!(3), json!(visits));
                    r["game"]["decisions"][first]["edge_backups"] = json!(5);
                }),
            ),
            // n sums past 2^64: a release build would wrap the sum.
            (
                "sum of root n overflows",
                edit(|s| s["tree"]["root"]["n"][0] = json!(u64::MAX)),
            ),
            (
                "unknown chooser",
                Box::new(move |r| r["game"]["decisions"][reused]["chooser"] = json!("random")),
            ),
            (
                "not at least eight",
                Box::new(move |r| r["game"]["decisions"][reused]["edge_backups"] = json!(7)),
            ),
            (
                "which is not earlier",
                Box::new(move |r| r["game"]["decisions"][reused]["search"] = json!(last)),
            ),
            (
                "records edge backups",
                Box::new(move |r| r["game"]["decisions"][exhausted]["edge_backups"] = json!(9)),
            ),
            (
                "a later search",
                Box::new(move |r| r["game"]["decisions"][exhausted]["search"] = json!(last)),
            ),
            (
                "without a refresh",
                Box::new(move |r| r["game"]["decisions"][refreshed]["refreshed"] = Value::Null),
            ),
            (
                "outside the legal menu",
                Box::new(|r| r["game"]["actions"][0] = json!(10_000)),
            ),
            (
                "Spy line flags",
                Box::new(|r| {
                    let flag = &mut r["game"]["decisions"][1]["spy_cast_offered"];
                    *flag = json!(*flag != true);
                }),
            ),
            (
                "line-tracker sequence",
                Box::new(|r| r["game"]["sequence"] = json!(["fixture"])),
            ),
            // The searches and the refreshed decisions correspond one to one.
            (
                "searches but",
                Box::new(move |r| r["game"]["decisions"][reused]["refreshed"] = json!("miss")),
            ),
            (
                "step or reason differs",
                edit(|s| s["reason"] = json!("depth_limit")),
            ),
            (
                "is not initial",
                Box::new(move |r| {
                    r["game"]["searches"][0]["reason"] = json!("miss");
                    r["game"]["decisions"][first]["refreshed"] = json!("miss");
                }),
            ),
            (
                "not the latest refreshed decision's",
                Box::new(move |r| r["game"]["decisions"][stale]["search"] = json!(0)),
            ),
            (
                "end",
                Box::new(|r| {
                    let flipped = if r["game"]["end"] == "win" {
                        "loss"
                    } else {
                        "win"
                    };
                    r["game"]["end"] = json!(flipped);
                }),
            ),
            (
                "outlast",
                Box::new(move |r| {
                    let mut extra = r["game"]["decisions"][stale].clone();
                    extra["step"] = json!(1_000_000);
                    extra["search"] = json!(last);
                    r["game"]["decisions"].as_array_mut().unwrap().push(extra);
                }),
            ),
        ];
        for (expected, change) in cases {
            let mut bad = row.clone();
            change(&mut bad);
            let reason = void(&spec, &bad);
            assert!(reason.contains(expected), "{expected}: {reason}");
        }

        // The frozen schedule's case: matching case fields keep the game; a
        // differing seed or guide seed, or a missing case, voids it.
        let case = |seed: u64| {
            json!({"id":row["id"],"own":4,"other":5,"seat":0,"opp":"T1","seed":seed,
                "starting_player":0,"source_policy_seeds":[7,7],"anchor_draw":3,
                "guide_policy_seeds":[0,0],"search_root":"fixture"})
        };
        let schedule = |cases: &[Value]| {
            let text: String = cases.iter().map(|c| format!("{c}\n")).collect();
            Schedule::parse(&text, "f".repeat(64)).unwrap()
        };
        let kept = game(&spec, &row, Some(&schedule(&[case(11)])));
        assert!(kept["void"].is_null(), "{}", kept["void"]);
        assert_eq!(kept["schedule_sha256"], json!("f".repeat(64)));
        assert!(out["schedule_sha256"].is_null());
        let moved = game(&spec, &row, Some(&schedule(&[case(12)])));
        assert!(moved["void"].as_str().unwrap().contains("seed differs"));
        let mut guided = case(11);
        guided["guide_policy_seeds"] = json!([0, 1]);
        let guided = game(&spec, &row, Some(&schedule(&[guided])));
        assert!(guided["void"]
            .as_str()
            .unwrap()
            .contains("live_policy_seeds differs"));
        let mut other = case(11);
        other["id"] = json!("elsewhere");
        let missing = game(&spec, &row, Some(&schedule(&[other])));
        assert!(missing["void"].as_str().unwrap().contains("no case"));
        assert!(Schedule::parse(&format!("{}\n{}\n", case(11), case(11)), String::new()).is_err());
    }

    /// An input error voids only what it reaches, and the run goes on: a
    /// listed file that cannot be read (missing, or not UTF-8) and a line
    /// that is not JSON each write a void row with the listed file and a
    /// null id; the file's other lines and the later files are still read,
    /// and rows of another kind or lineage write nothing. Every row keeps
    /// the same key order.
    #[test]
    fn root_rebuild_rows_void_input_errors_and_continue() {
        let spec = spec();
        let dir = std::env::temp_dir().join(format!("sd1-root-rebuild-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = |name: &str| dir.join(name).to_str().unwrap().to_owned();
        // A budget_game row of the lineage without the record schema: its
        // record checks void it, with no replay.
        let budget =
            |id: &str| json!({"kind":"budget_game","lineage":"fixture","id":id}).to_string();
        let lines = |rows: &[String]| rows.iter().map(|r| format!("{r}\n")).collect::<String>();
        let mixed = lines(&[
            budget("a"),
            "{\"kind\":".into(),
            String::new(),
            json!({"kind":"budget_game","lineage":"other","id":"x"}).to_string(),
            json!({"kind":"budget_replay","lineage":"fixture","id":"y"}).to_string(),
            budget("b"),
        ]);
        std::fs::write(path("mixed.jsonl"), mixed).unwrap();
        std::fs::write(path("binary.jsonl"), b"\xff\xfe\n").unwrap();
        std::fs::write(path("last.jsonl"), lines(&[budget("c")])).unwrap();
        let list = format!(
            "{}\n  {}  \r\n\n{}\n{}\n",
            path("missing.jsonl"),
            path("mixed.jsonl"),
            path("binary.jsonl"),
            path("last.jsonl")
        );
        let out = rows(&spec, &list, None);
        let _ = std::fs::remove_dir_all(&dir);
        let expected = [
            (
                Value::Null,
                "missing.jsonl",
                "the listed file cannot be read: ",
            ),
            (json!("a"), "mixed.jsonl", "not a budget_game record"),
            (
                Value::Null,
                "mixed.jsonl",
                "line 2 of the listed file is not JSON: ",
            ),
            (json!("b"), "mixed.jsonl", "not a budget_game record"),
            (
                Value::Null,
                "binary.jsonl",
                "the listed file cannot be read: ",
            ),
            (json!("c"), "last.jsonl", "not a budget_game record"),
        ];
        assert_eq!(out.lines().count(), expected.len(), "{out}");
        for (line, (id, file, reason)) in out.lines().zip(expected) {
            let row: Value = serde_json::from_str(line).unwrap();
            assert_eq!(row["id"], id, "{line}");
            assert_eq!(row["file"], json!(path(file)), "{line}");
            assert_eq!(row["schema"], SCHEMA, "{line}");
            assert!(row["void"].as_str().unwrap().starts_with(reason), "{line}");
            if id.is_null() {
                let keys: Vec<&str> = row
                    .as_object()
                    .unwrap()
                    .keys()
                    .map(String::as_str)
                    .collect();
                assert_eq!(keys, ["file", "id", "schema", "void"], "{line}");
            }
            let at = ["\"file\":", "\"id\":", "\"schema\":", "\"void\":"]
                .map(|k| line.find(k).unwrap_or_else(|| panic!("{k} in {line}")));
            assert!(at.windows(2).all(|w| w[0] < w[1]), "{line}");
        }
    }
}
