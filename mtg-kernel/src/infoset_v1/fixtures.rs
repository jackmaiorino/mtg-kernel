//! AC1 fixtures as full engine worlds (W1 design v7, 6.3, 6.4 and 13;
//! ruling 44).
//!
//! Each fixture is an engine game from the reset. A forced deal exchanges
//! definitions among one owner's pristine objects before any decision (a
//! different shuffle of the same deck), and a script plays both seats
//! through the flat-action session: scripted intents in order, otherwise a
//! default that passes, never attacks and picks the smallest decision-local
//! candidate. Paired worlds differ only in an opponent's hidden choice or a
//! forced shuffle outcome; V's views are compared at every V decision and
//! the correct producer's payloads must agree byte for byte.

use super::harness::{local_menu, seat, swap_objects, view, PairReportV1};
use super::tests::export_at;
use super::*;
use crate::ids::{ObjectId, PlayerId};
use crate::rl_session::{FastActorResponseV1, FastActorSessionV1};
use crate::runtime_decks::runtime_deck_by_id;
use crate::state::{GameState, Zone};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

pub(crate) fn id(name: &str) -> u16 {
    crate::card_def::card_id_by_name(name).unwrap_or_else(|| panic!("no card named {name}"))
}

/// The leakage mutants every pair is checked against (design 13).
pub(crate) const LEAKAGE_V1: [MutantV1; 9] = [
    MutantV1::L1,
    MutantV1::L2,
    MutantV1::L3,
    MutantV1::L4,
    MutantV1::L5,
    MutantV1::L6,
    MutantV1::L7,
    MutantV1::L8,
    MutantV1::L13,
];

/// A new game of `decks` (by seat) with `first` taking the first turn.
pub(crate) fn new_game(decks: [&str; 2], seed: u64, first: PlayerId) -> FastActorSessionV1 {
    let mainboards = decks.map(|d| {
        runtime_deck_by_id(d)
            .unwrap_or_else(|| panic!("deck {d}"))
            .card_ids
            .to_vec()
    });
    FastActorSessionV1::reset_with_explicit_decks_and_limits_flat_action_v3_environment_v2_with_starting_player_v1(
        seed,
        seed,
        2_000,
        20_000,
        decks.map(str::to_string),
        mainboards,
        first,
    )
    .expect("reset")
}

/// Forced deal: `owner`'s hand starts with `hand` and its library with
/// `top`, by exchanging definitions among the owner's pristine objects
/// (nothing has been observed before the first decision).
pub(crate) fn force_deal(state: &mut GameState, owner: PlayerId, hand: &[&str], top: &[&str]) {
    let p = state.players[owner.index()].clone();
    assert!(hand.len() <= p.hand.len(), "hand longer than the deal");
    let order: Vec<ObjectId> = p.hand.iter().chain(&p.library).copied().collect();
    let wanted: Vec<(ObjectId, u16)> = hand
        .iter()
        .enumerate()
        .map(|(i, n)| (p.hand[i], id(n)))
        .chain(top.iter().enumerate().map(|(j, n)| (p.library[j], id(n))))
        .collect();
    let mut placed: BTreeSet<ObjectId> = BTreeSet::new();
    for (slot, def) in wanted {
        if state.objects.get(slot).card_def != def {
            let donor = order
                .iter()
                .copied()
                .find(|o| {
                    !placed.contains(o) && *o != slot && state.objects.get(*o).card_def == def
                })
                .unwrap_or_else(|| panic!("no spare copy of def {def} for the deal"));
            swap_objects(state, slot, donor);
        }
        placed.insert(slot);
    }
}

/// The session restarted from `state` (same template, same counters).
pub(crate) fn restart(template: &FastActorSessionV1, state: GameState) -> FastActorSessionV1 {
    FastActorSessionV1::import_infoset_v1(template, state, 0)
}

type CandPred = Box<dyn Fn(&Value) -> bool>;
type StatePred = Box<dyn Fn(&GameState) -> bool>;
type Surgery = Box<dyn Fn(&mut GameState)>;

/// One scripted step of a seat.
pub(crate) enum IntentV1 {
    /// Take the first candidate matching the predicate when one is offered.
    Take(String, CandPred),
    /// Hold the queue (default answers) until the state predicate holds.
    Gate(String, StatePred),
    /// Edit the state at this decision (a forced shuffle outcome), then
    /// continue. Every variant of a fixture restarts at the same point.
    Edit(String, Surgery),
}

pub(crate) fn kind(v: &Value) -> &str {
    v["action_kind"].as_str().unwrap_or("")
}

fn source_def(v: &Value) -> Option<u64> {
    v["source"]["card_db_id"].as_u64()
}

/// The card a choice names (a target, a cost candidate or a card).
fn named_def(v: &Value) -> Option<u64> {
    v["target"]["object"]["card_db_id"]
        .as_u64()
        .or_else(|| v["candidate"]["card_db_id"].as_u64())
        .or_else(|| v["card"]["card_db_id"].as_u64())
}

pub(crate) fn play_land(name: &str) -> IntentV1 {
    let d = u64::from(id(name));
    IntentV1::Take(
        format!("play {name}"),
        Box::new(move |v| kind(v) == "play_land" && source_def(v) == Some(d)),
    )
}

pub(crate) fn cast(name: &str) -> IntentV1 {
    let d = u64::from(id(name));
    IntentV1::Take(
        format!("cast {name}"),
        Box::new(move |v| kind(v) == "cast_spell" && source_def(v) == Some(d)),
    )
}

pub(crate) fn activate(name: &str) -> IntentV1 {
    let d = u64::from(id(name));
    IntentV1::Take(
        format!("activate {name}"),
        Box::new(move |v| kind(v) == "activate_ability" && source_def(v) == Some(d)),
    )
}

/// Choose a card named `name` (a target, an effect choice or a cost).
pub(crate) fn pick(name: &str) -> IntentV1 {
    let d = u64::from(id(name));
    IntentV1::Take(
        format!("pick {name}"),
        Box::new(move |v| {
            matches!(
                kind(v),
                "choose_target" | "choose_effect_target" | "choose_cost_target"
            ) && named_def(v) == Some(d)
        }),
    )
}

/// Choose a card whose name is none of `names` (a role, not a name: the
/// first such candidate in decision-local order).
pub(crate) fn pick_other_than(names: &[&str]) -> IntentV1 {
    let ds: Vec<u64> = names.iter().map(|n| u64::from(id(n))).collect();
    IntentV1::Take(
        format!("pick a card other than {names:?}"),
        Box::new(move |v| {
            matches!(
                kind(v),
                "choose_target" | "choose_effect_target" | "choose_cost_target"
            ) && named_def(v).is_some_and(|d| !ds.contains(&d))
        }),
    )
}

/// Discard a card named `name` (cleanup to hand size).
pub(crate) fn discard(name: &str) -> IntentV1 {
    let d = u64::from(id(name));
    IntentV1::Take(
        format!("discard {name}"),
        Box::new(move |v| kind(v) == "discard" && v["cards"][0]["card_db_id"].as_u64() == Some(d)),
    )
}

/// Answer yes to an optional payment or a may-choice.
pub(crate) fn yes() -> IntentV1 {
    IntentV1::Take(
        "yes".into(),
        Box::new(|v| {
            kind(v).starts_with("choose_")
                && ["value", "use_cost", "pay", "cast_it"]
                    .iter()
                    .any(|k| v[*k] == Value::Bool(true))
        }),
    )
}

pub(crate) fn finish() -> IntentV1 {
    IntentV1::Take(
        "finish".into(),
        Box::new(|v| {
            matches!(
                kind(v),
                "finish_effect_selection" | "finish_target_selection"
            )
        }),
    )
}

pub(crate) fn gate(what: &str, p: impl Fn(&GameState) -> bool + 'static) -> IntentV1 {
    IntentV1::Gate(what.into(), Box::new(p))
}

pub(crate) fn edit(what: &str, f: impl Fn(&mut GameState) + 'static) -> IntentV1 {
    IntentV1::Edit(what.into(), Box::new(f))
}

/// The default answer: pass, attack and block with nothing, decline, target
/// the other player, finish a selection, else the smallest decision-local
/// candidate (independent of native ids).
fn default_choice(vals: &[Value], local: &[Vec<u8>], p: PlayerId, keep: &BTreeSet<u64>) -> usize {
    let me = if p == PlayerId::P0 { "p0" } else { "p1" };
    let rank = |v: &Value| -> u8 {
        let empty = |k: &str| v[k].as_array().is_some_and(Vec::is_empty);
        match kind(v) {
            "pass" => 0,
            "declare_attackers" if empty("attackers") => 0,
            "declare_blockers_for_attacker" if empty("blockers") => 0,
            "choose_attacker_inclusion" | "choose_blocker_inclusion"
                if v["include"] == Value::Bool(false) =>
            {
                0
            }
            "choose_target" | "choose_effect_target"
                if v["target"]["player"].as_str().is_some_and(|x| x != me) =>
            {
                1
            }
            "finish_effect_selection" | "finish_target_selection" => 2,
            // Cleanup discards never take a card the script still needs.
            "discard"
                if v["cards"][0]["card_db_id"]
                    .as_u64()
                    .is_some_and(|d| !keep.contains(&d)) =>
            {
                3
            }
            "discard" => 8,
            "activate_mana_ability"
            | "play_land"
            | "cast_spell"
            | "activate_ability"
            | "plot_spell" => 9,
            _ => 5,
        }
    };
    (0..vals.len())
        .min_by(|&a, &b| {
            rank(&vals[a])
                .cmp(&rank(&vals[b]))
                .then_with(|| local[a].cmp(&local[b]))
        })
        .expect("a decision has candidates")
}

/// A scripted engine game.
pub(crate) struct ScriptV1 {
    pub s: FastActorSessionV1,
    /// The session the script started from.
    pub start: FastActorSessionV1,
    pub intents: [VecDeque<IntentV1>; 2],
    pub trace: Vec<String>,
    pub steps: usize,
    /// Each chosen candidate with native ids (realizability replays).
    pub native: Vec<Value>,
    /// Library positions the script's edits exchanged: (decision, a, b).
    pub edits: Vec<(usize, ObjectId, ObjectId)>,
    /// Cards a seat's default cleanup discard never takes.
    pub keep: [BTreeSet<u64>; 2],
    /// The strict-view dump (rulings 4 and 5): V, its world, the file.
    pub strict: Option<(
        PlayerId,
        super::transcript::WorldV1,
        super::transcript::DumpV1,
    )>,
}

impl ScriptV1 {
    pub(crate) fn new(s: FastActorSessionV1) -> Self {
        Self {
            start: s.clone(),
            s,
            intents: [VecDeque::new(), VecDeque::new()],
            trace: Vec::new(),
            steps: 0,
            native: Vec::new(),
            edits: Vec::new(),
            keep: [BTreeSet::new(), BTreeSet::new()],
            strict: None,
        }
    }

    pub(crate) fn keep(&mut self, p: PlayerId, names: &[&str]) {
        self.keep[p.index()].extend(names.iter().map(|n| u64::from(id(n))));
    }

    pub(crate) fn push(&mut self, p: PlayerId, xs: Vec<IntentV1>) {
        self.intents[p.index()].extend(xs);
    }

    pub(crate) fn done(&self) -> bool {
        self.intents.iter().all(VecDeque::is_empty)
    }

    fn fail(&self, why: &str) -> ! {
        // Every scripted answer, then the last few decisions.
        let n = self.trace.len();
        let tail: Vec<&String> = self
            .trace
            .iter()
            .enumerate()
            .filter(|(i, t)| *i + 12 >= n || !t.contains(": default ["))
            .map(|(_, t)| t)
            .rev()
            .collect();
        let pending: Vec<String> = self
            .intents
            .iter()
            .map(|q| {
                q.iter()
                    .map(|i| match i {
                        IntentV1::Take(w, _) | IntentV1::Gate(w, _) | IntentV1::Edit(w, _) => {
                            w.clone()
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .collect();
        panic!(
            "{why}; pending intents P0 [{}] P1 [{}]; last decisions (newest first):\n{}",
            pending[0],
            pending[1],
            tail.iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join("\n")
        );
    }

    /// One decision. `at` sees every decision with at least two candidates
    /// before it is answered. Returns false at a terminal.
    pub(crate) fn step(
        &mut self,
        at: &mut dyn FnMut(&FastActorSessionV1, PlayerId, usize),
    ) -> bool {
        loop {
            let FastActorResponseV1::Decision(d) = self.s.current_response() else {
                return false;
            };
            let p = seat(d.acting_player);
            // Gates and edits run before the decision is answered.
            match self.intents[p.index()].front() {
                Some(IntentV1::Gate(_, g)) if g(self.s.game_state()) => {
                    self.intents[p.index()].pop_front();
                    continue;
                }
                Some(IntentV1::Edit(..)) => {
                    let Some(IntentV1::Edit(w, f)) = self.intents[p.index()].pop_front() else {
                        unreachable!()
                    };
                    if self.s.surface_context_v1().is_none() {
                        self.fail(&format!("edit {w} at a decision without a surface context"));
                    }
                    let mut st = self.s.game_state().clone();
                    f(&mut st);
                    // Recorded even when nothing moved: a replay restarts
                    // the session at the same decision.
                    let mut edit = None;
                    for p in [PlayerId::P0, PlayerId::P1] {
                        let (old, new) = (
                            &self.s.game_state().players[p.index()].library,
                            &st.players[p.index()].library,
                        );
                        let moved: Vec<usize> =
                            (0..old.len()).filter(|&i| old[i] != new[i]).collect();
                        if moved.len() == 2 {
                            edit = Some((self.steps, old[moved[0]], old[moved[1]]));
                        } else if !moved.is_empty() {
                            self.fail("an edit must exchange exactly two library positions");
                        }
                    }
                    let first = self.s.game_state().players[0].library[0];
                    self.edits.push(edit.unwrap_or((self.steps, first, first)));
                    let context = self.s.surface_context_v1();
                    self.s =
                        FastActorSessionV1::import_infoset_with_surface_v1(&self.s, st, 0, context);
                    self.trace.push(format!("edit {w}"));
                    continue;
                }
                _ => {}
            }
            let mut line = self.strict.as_mut().map(|(v, world, _)| {
                let mut line = serde_json::json!({"k": self.steps,
                    "a": [format!("{:?}", d.acting_player)], "w": [world.decision(&self.s)]});
                if p == *v && d.legal_action_count >= 2 {
                    line["v"] = strict_v_info(&self.s, *v);
                }
                line
            });
            if d.legal_action_count >= 2 {
                at(&self.s, p, self.steps);
            }
            let sem = self
                .s
                .diagnostic_current_action_semantics()
                .unwrap_or_default();
            let vals: Vec<Value> = sem
                .iter()
                .map(|x| serde_json::to_value(x).expect("semantics serialize"))
                .collect();
            let local = local_menu(&self.s);
            let mut why = "default".to_string();
            let mut choice = None;
            if let Some(IntentV1::Take(w, pred)) = self.intents[p.index()].front() {
                if let Some(i) = vals.iter().position(pred) {
                    choice = Some(i);
                    why = w.clone();
                }
            }
            if choice.is_some() {
                self.intents[p.index()].pop_front();
            }
            let i =
                choice.unwrap_or_else(|| default_choice(&vals, &local, p, &self.keep[p.index()]));
            if let (Some((_, world, dump)), Some(line)) = (self.strict.as_mut(), line.as_mut()) {
                line["c"] = serde_json::json!([world.raw_candidate(i)]);
                dump.line(line);
            }
            self.native.push(vals[i].clone());
            let st = self.s.game_state();
            self.trace.push(format!(
                "t{} {:?} P{} {:?} {} of {}: {} [{}]",
                st.turn,
                st.step,
                p.0,
                d.decision_kind,
                i,
                vals.len(),
                why,
                vals.iter()
                    .map(|v| v.to_string())
                    .collect::<Vec<_>>()
                    .join(" | ")
                    .chars()
                    .take(1_500)
                    .collect::<String>()
            ));
            if let Err(e) = self.s.step(d.episode_id, d.step, i as u32) {
                self.fail(&format!("step failed: {e:?}"));
            }
            self.steps += 1;
            return true;
        }
    }

    /// Plays until every intent is consumed, then `extra` more decisions.
    pub(crate) fn run(
        &mut self,
        max: usize,
        extra: usize,
        at: &mut dyn FnMut(&FastActorSessionV1, PlayerId, usize),
    ) {
        let mut after = 0;
        for _ in 0..max {
            if self.done() {
                if after >= extra {
                    return;
                }
                after += 1;
            }
            if !self.step(at) {
                if !self.done() {
                    self.fail("the game ended before the script did");
                }
                return;
            }
        }
        if !self.done() {
            self.fail("the script did not finish");
        }
    }
}

/// What V sees and what each producer exports at one V decision.
#[derive(Clone, Debug)]
pub(crate) struct AtV1 {
    /// The decision's index among the script's steps.
    pub step: usize,
    pub view: Option<Vec<u8>>,
    pub p: Result<Vec<u8>, String>,
    pub mutants: Vec<(MutantV1, Result<Vec<u8>, String>)>,
    /// The survivor tree's nodes, as (sorted names, m).
    pub nodes: Vec<(Vec<String>, i64)>,
    /// Exported handles that are `known_name` occupants or pins, by name.
    pub known: Vec<String>,
    pub pinned: Vec<String>,
}

pub(crate) fn observe(
    s: &FastActorSessionV1,
    v: PlayerId,
    step: usize,
    mutants: &[MutantV1],
) -> AtV1 {
    let x = export_at(s, v, ExportOptionsV1::default());
    let state = s.game_state();
    let projection = super::projection::project_v1(state, state.observation_journal.notes(), v);
    let nodes = projection
        .survivors
        .nodes()
        .iter()
        .map(|n| {
            let mut names: Vec<String> =
                n.k.iter()
                    .flat_map(|(d, c)| {
                        std::iter::repeat_n(
                            crate::card_def::CARD_DEFS[*d as usize].name.to_string(),
                            *c as usize,
                        )
                    })
                    .collect();
            names.sort();
            (names, n.m)
        })
        .collect();
    let (mut known, mut pinned) = (Vec::new(), Vec::new());
    if let Ok(x) = &x {
        for (id, o) in x.state.objects.iter() {
            match x.handles[id.0 as usize] {
                HandleV1::U {
                    known_name: true, ..
                } => known.push(o.name.clone()),
                HandleV1::U { pinned: true, .. } => pinned.push(format!("{}@{:?}", o.name, o.zone)),
                _ => {}
            }
        }
    }
    known.sort();
    pinned.sort();
    AtV1 {
        step,
        view: view(s),
        p: super::harness::p_outcome(s, v),
        mutants: mutants
            .iter()
            .map(|m| {
                (
                    *m,
                    // As delivered (G5), like P.
                    super::harness::deliverable_outcome(s, v, ExportOptionsV1 { mutant: Some(*m) }),
                )
            })
            .collect(),
        nodes,
        known,
        pinned,
    }
}

/// P's and each leakage mutant's delivered outcome at a V decision (as
/// digests) and the exported `known_name` handles by name, for the strict
/// checker.
fn strict_v_info(s: &FastActorSessionV1, v: PlayerId) -> Value {
    use super::transcript::digest;
    let at = observe(s, v, 0, &LEAKAGE_V1);
    let m: serde_json::Map<String, Value> = at
        .mutants
        .iter()
        .map(|(m, o)| (format!("{m:?}"), serde_json::json!([digest(o)])))
        .collect();
    serde_json::json!({"p": [digest(&at.p)], "ph": [super::harness::head(&at.p)], "m": m,
                       "known": [at.known]})
}

/// As [`record`], also dumping every decision for the strict view under
/// `name` (when `W1_STRICT_DUMP_DIR` is set).
pub(crate) fn record_strict(
    mut script: ScriptV1,
    v: PlayerId,
    extra: usize,
    name: &str,
) -> (Vec<AtV1>, ScriptV1) {
    if super::transcript::DumpV1::enabled() {
        let header = serde_json::json!({"family": "fixture", "tag": name, "viewer": format!("{v:?}")});
        script.strict = Some((
            v,
            super::transcript::WorldV1::new(v),
            super::transcript::DumpV1::open(name, header),
        ));
    }
    let (out, mut script) = record(script, v, extra);
    if let Some((_, _, dump)) = script.strict.take() {
        dump.finish("script");
    }
    (out, script)
}

/// Lockstep comparison of two worlds' V-decision records (truncated at the
/// first view difference).
pub(crate) fn compare_records(a: &[AtV1], b: &[AtV1]) -> PairReportV1 {
    let mut report = PairReportV1::default();
    if a.len() != b.len() {
        report.truncated = true;
    }
    for (x, y) in a.iter().zip(b) {
        if x.view != y.view {
            report.truncated = true;
            break;
        }
        report.compared += 1;
        if let Err(code) = &x.p {
            *report.p_refusals.entry(code.clone()).or_default() += 1;
        }
        if x.p != y.p {
            report.p_differs += 1;
            continue;
        }
        for ((m, mx), (_, my)) in x.mutants.iter().zip(&y.mutants) {
            if mx != my {
                *report.witnesses.entry(format!("{m:?}")).or_default() += 1;
            }
        }
    }
    report
}

/// Runs one variant of a fixture, recording every V decision.
pub(crate) fn record(mut script: ScriptV1, v: PlayerId, extra: usize) -> (Vec<AtV1>, ScriptV1) {
    let mut out = Vec::new();
    let mut seen = BTreeSet::new();
    script.run(400, extra, &mut |s, p, step| {
        if p == v {
            let at = observe(s, v, step, &LEAKAGE_V1);
            if let Err(code) = &at.p {
                if seen.insert(code.clone()) {
                    if let Err(e) = export_at(s, v, ExportOptionsV1::default()) {
                        eprintln!("first {code} refusal at step {step}: {e:?}");
                    }
                }
            }
            out.push(at);
        }
    });
    (out, script)
}

// ---------------------------------------------------------------------------
// The fixtures.

/// Spy versus Terror (6.4, ruling 40.1; an off-board engine fixture). V is
/// Spy (P0). Terror (P1) goes first: Island, Island; Spy's Mesmeric Fiend
/// reveals Brainstorm, Terror, Ponder, Mystic, Counterspell and an Island
/// and exiles Counterspell; Terror plays the Island and Brainstorms,
/// returning `put_back`. The survivor tree is ({Terror, Ponder, Mystic}, 1).
pub(crate) fn spy_versus_terror(put_back: [&str; 2]) -> ScriptV1 {
    let (v, o) = (PlayerId::P0, PlayerId::P1);
    let live = new_game(["Spy", "Terror"], 71, o);
    let mut st = live.game_state().clone();
    force_deal(
        &mut st,
        o,
        &[
            "Island",
            "Island",
            "Brainstorm",
            "Tolarian Terror",
            "Ponder",
            "Murmuring Mystic",
            "Counterspell",
        ],
        &[
            "Island",
            "Cryptic Serpent",
            "Thought Scour",
            "Mental Note",
            "Deep Analysis",
        ],
    );
    force_deal(
        &mut st,
        v,
        &[
            "Swamp",
            "Forest",
            "Mesmeric Fiend",
            "Forest",
            "Wall of Roots",
            "Generous Ent",
            "Land Grant",
        ],
        &["Forest", "Saruli Caretaker", "Sagu Wildling"],
    );
    let mut script = ScriptV1::new(restart(&live, st));
    script.push(
        v,
        vec![
            play_land("Swamp"),
            play_land("Forest"),
            cast("Mesmeric Fiend"),
            pick("Counterspell"),
        ],
    );
    script.push(
        o,
        vec![
            play_land("Island"),
            play_land("Island"),
            gate("the Fiend's exile", |st: &GameState| {
                st.exile
                    .iter()
                    .any(|x| st.objects.get(*x).card_def == id("Counterspell"))
            }),
            play_land("Island"),
            cast("Brainstorm"),
            pick(put_back[0]),
            pick(put_back[1]),
        ],
    );
    script
}

/// The Fable bounce (ruling 44; CawGates versus Faeries, off-board). V is
/// Faeries (P1). CawGates casts Squadron Hawk and fetches the other three,
/// then Brainstorms returning `put_back`; Faeries Snaps the battlefield Hawk
/// back. The survivor tree is ({Hawk x3}, 1); the bounced Hawk is tracked
/// and never counts toward it. `hawks` of the two returned cards are Hawks
/// (the others are untracked cards: the Hawk search shuffles the library,
/// so the Brainstorm's draws are named by role).
pub(crate) fn fable_bounce(hawks: usize) -> ScriptV1 {
    let (o, v) = (PlayerId::P0, PlayerId::P1);
    let live = new_game(["CawGates", "Faeries"], 72, o);
    let mut st = live.game_state().clone();
    force_deal(
        &mut st,
        o,
        &[
            "Azorius Guildgate",
            "Island",
            "Island",
            "Squadron Hawk",
            "Brainstorm",
            "Prismatic Strands",
            "Journey to Nowhere",
        ],
        &[
            "Sacred Cat",
            "Thraben Charm",
            "Spell Pierce",
            "Preordain",
            "The Modern Age",
        ],
    );
    force_deal(
        &mut st,
        v,
        &[
            "Island",
            "Island",
            "Island",
            "Snap",
            "Island",
            "Force Spike",
            "Dispel",
        ],
        &["Island", "Island", "Island"],
    );
    let mut script = ScriptV1::new(restart(&live, st));
    script.push(
        o,
        vec![
            play_land("Azorius Guildgate"),
            play_land("Island"),
            cast("Squadron Hawk"),
            pick("Squadron Hawk"),
            pick("Squadron Hawk"),
            pick("Squadron Hawk"),
            play_land("Island"),
            cast("Brainstorm"),
            if hawks >= 1 {
                pick("Squadron Hawk")
            } else {
                pick_other_than(&["Squadron Hawk"])
            },
            if hawks >= 2 {
                pick("Squadron Hawk")
            } else {
                pick_other_than(&["Squadron Hawk"])
            },
        ],
    );
    script.keep(o, &["Island", "Squadron Hawk", "Brainstorm"]);
    script.push(
        v,
        vec![
            play_land("Island"),
            play_land("Island"),
            gate("Brainstorm resolved", |st: &GameState| {
                st.players[0]
                    .graveyard
                    .iter()
                    .any(|x| st.objects.get(*x).card_def == id("Brainstorm"))
            }),
            play_land("Island"),
            cast("Snap"),
            pick("Squadron Hawk"),
        ],
    );
    script
}

/// The zone change count of the card a choice names.
fn named_generation(v: &Value) -> Option<u64> {
    v["target"]["object"]["zone_change_count"]
        .as_u64()
        .or_else(|| v["candidate"]["zone_change_count"].as_u64())
        .or_else(|| v["card"]["zone_change_count"].as_u64())
}

/// Choose a copy of `name` by its history: `bounced` takes a copy that has
/// been on the battlefield (at least three zone changes: drawn, cast,
/// resolved, returned), otherwise one that has not.
pub(crate) fn pick_copy(name: &str, bounced: bool) -> IntentV1 {
    let d = u64::from(id(name));
    IntentV1::Take(
        format!(
            "pick a {} {name}",
            if bounced { "bounced" } else { "fresh" }
        ),
        Box::new(move |v| {
            matches!(
                kind(v),
                "choose_target" | "choose_effect_target" | "choose_cost_target"
            ) && named_def(v) == Some(d)
                && named_generation(v).is_some_and(|g| (g >= 3) == bounced)
        }),
    )
}

/// The L8 fixture (count facts bound to copies whose exported attributes
/// differ; CawGates versus Faeries, off-board). V is Faeries (P1).
/// CawGates casts Squadron Hawk and fetches the other three; Faeries Snaps
/// the battlefield Hawk back, so the hand holds four tracked Hawks, one of
/// them with a battlefield history (its span began on the stack). CawGates
/// then Brainstorms and puts back `put_back`, and the tracker keeps two
/// Hawk count facts. P exports two `known_name` Hawks in every variant;
/// mutant L8 binds the facts to true remaining Hawks with `C:` handles, so
/// its bytes show whether the bounced copy (exported generation above 1)
/// is among them.
pub(crate) fn l8_bound_copies(put_back: L8PutBackV1, seed: u64) -> ScriptV1 {
    let (o, v) = (PlayerId::P0, PlayerId::P1);
    let live = new_game(["CawGates", "Faeries"], seed, o);
    let mut st = live.game_state().clone();
    force_deal(
        &mut st,
        o,
        &[
            "Azorius Guildgate",
            "Island",
            "Island",
            "Squadron Hawk",
            "Brainstorm",
            "Prismatic Strands",
            "Journey to Nowhere",
        ],
        &[
            "Sacred Cat",
            "Thraben Charm",
            "Spell Pierce",
            "Preordain",
            "The Modern Age",
        ],
    );
    force_deal(
        &mut st,
        v,
        &[
            "Island",
            "Island",
            "Island",
            "Snap",
            "Island",
            "Force Spike",
            "Dispel",
        ],
        &["Island", "Island", "Island"],
    );
    let mut script = ScriptV1::new(restart(&live, st));
    let returns = match put_back {
        L8PutBackV1::NoHawk => [
            pick_other_than(&["Squadron Hawk"]),
            pick_other_than(&["Squadron Hawk"]),
        ],
        L8PutBackV1::TwoFresh => [
            pick_copy("Squadron Hawk", false),
            pick_copy("Squadron Hawk", false),
        ],
        L8PutBackV1::BouncedAndFresh => [
            pick_copy("Squadron Hawk", true),
            pick_copy("Squadron Hawk", false),
        ],
    };
    let mut o_script = vec![
        play_land("Azorius Guildgate"),
        play_land("Island"),
        cast("Squadron Hawk"),
        pick("Squadron Hawk"),
        pick("Squadron Hawk"),
        pick("Squadron Hawk"),
        gate("the Hawk bounced", |st: &GameState| {
            st.players[0].hand.iter().any(|x| {
                let o = st.objects.get(*x);
                o.card_def == id("Squadron Hawk") && o.zone_change_count >= 3
            })
        }),
        play_land("Island"),
        cast("Brainstorm"),
    ];
    o_script.extend(returns);
    script.push(o, o_script);
    script.keep(o, &["Island", "Squadron Hawk", "Brainstorm"]);
    script.push(
        v,
        vec![
            play_land("Island"),
            play_land("Island"),
            cast("Snap"),
            pick("Squadron Hawk"),
        ],
    );
    script
}

/// What the L8 fixture's Brainstorm puts back.
#[derive(Clone, Copy, Debug)]
pub(crate) enum L8PutBackV1 {
    NoHawk,
    TwoFresh,
    BouncedAndFresh,
}

/// The CawGates Lorien fixture (Astra r6-1, R8b). V is Faeries (P1).
/// CawGates fetches the other three Hawks (its third turn), plays its third
/// Island and islandcycles a Lorien for the last library Island (its fourth
/// turn), Brainstorms returning that Island and `with`, and islandcycles the
/// second Lorien. F is ({Hawk x3, Island}, 2) after the Brainstorm; the
/// second search finds the Island and refuses (R8b) from then on. `with` is
/// "Squadron Hawk" or None (an untracked card).
pub(crate) fn cawgates_lorien(with: Option<&str>) -> ScriptV1 {
    let (o, v) = (PlayerId::P0, PlayerId::P1);
    let live = new_game(["CawGates", "Faeries"], 73, o);
    let mut st = live.game_state().clone();
    force_deal(
        &mut st,
        o,
        &[
            "Azorius Guildgate",
            "Island",
            "Island",
            "Squadron Hawk",
            "Lorien Revealed",
            "Lorien Revealed",
            "Brainstorm",
        ],
        &["Island", "Sacred Cat", "Thraben Charm", "Spell Pierce"],
    );
    force_deal(
        &mut st,
        v,
        &[
            "Island",
            "Island",
            "Island",
            "Island",
            "Snap",
            "Force Spike",
            "Dispel",
        ],
        &["Island", "Island", "Island", "Island"],
    );
    let mut script = ScriptV1::new(restart(&live, st));
    script.push(
        o,
        vec![
            play_land("Azorius Guildgate"),
            play_land("Island"),
            play_land("Island"),
            cast("Squadron Hawk"),
            pick("Squadron Hawk"),
            pick("Squadron Hawk"),
            pick("Squadron Hawk"),
            play_land("Island"),
            activate("Lorien Revealed"),
            pick("Island"),
            cast("Brainstorm"),
            pick("Island"),
            match with {
                Some(name) => pick(name),
                None => pick_other_than(&["Squadron Hawk", "Island", "Lorien Revealed"]),
            },
            activate("Lorien Revealed"),
            pick("Island"),
        ],
    );
    script.keep(
        o,
        &["Island", "Squadron Hawk", "Brainstorm", "Lorien Revealed"],
    );
    script.push(
        v,
        vec![
            play_land("Island"),
            play_land("Island"),
            play_land("Island"),
            play_land("Island"),
        ],
    );
    script
}

/// Which pinned Lembas a forced shuffle outcome puts on top before the
/// unidentified draw (F-pin).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PinTopV1 {
    Neither,
    First,
    Second,
}

/// W-pin-1 (`pins` = 1) and W-pin-2 (`pins` = 2) as paired engine worlds
/// (6.3). V is Faeries (P1). Wildfire sacrifices its Lembas with the Food
/// abilities waiting; each shuffle trigger resolves (the Lembas is shuffled
/// into the library), then Krark-Clan Shaman sacrifices Ichor Wellspring
/// and its trigger draws one card, unidentified, while the abilities wait.
/// `top` forces the last shuffle's outcome: neither pin, or one of them, on
/// top of the library.
pub(crate) fn w_pin(pins: usize, top: PinTopV1) -> ScriptV1 {
    let (o, v) = (PlayerId::P0, PlayerId::P1);
    let live = new_game(["Wildfire", "Faeries"], 74, o);
    let mut st = live.game_state().clone();
    force_deal(
        &mut st,
        o,
        &[
            "Mountain",
            "Swamp",
            "Swamp",
            "Forest",
            "Lembas",
            "Lembas",
            "Krark-Clan Shaman",
        ],
        &[
            "Forest",
            "Ichor Wellspring",
            "Cast Down",
            "Cast Down",
            "Cast Down",
        ],
    );
    force_deal(
        &mut st,
        v,
        &[
            "Island",
            "Island",
            "Island",
            "Spellstutter Sprite",
            "Island",
            "Force Spike",
            "Dispel",
        ],
        &["Island", "Island", "Island", "Island"],
    );
    let lembas = id("Lembas");
    let mut script = ScriptV1::new(restart(&live, st));
    let mut o_intents = vec![
        play_land("Mountain"),
        play_land("Swamp"),
        cast("Lembas"),
        play_land("Swamp"),
        cast("Krark-Clan Shaman"),
        cast("Lembas"),
        play_land("Forest"),
        cast("Ichor Wellspring"),
        play_land("Forest"),
        activate("Lembas"),
        gate(
            "the first shuffle trigger resolved",
            move |st: &GameState| {
                st.stack.len() == 1
                    && st.players[0]
                        .library
                        .iter()
                        .any(|x| st.objects.get(*x).card_def == lembas)
            },
        ),
    ];
    if pins == 2 {
        o_intents.push(activate("Lembas"));
        o_intents.push(gate(
            "the second shuffle trigger resolved",
            move |st: &GameState| {
                st.stack.len() == 2
                    && st.players[0]
                        .library
                        .iter()
                        .filter(|x| st.objects.get(**x).card_def == lembas)
                        .count()
                        >= 2
            },
        ));
    }
    o_intents.push(edit(
        "force the shuffle outcome",
        move |st: &mut GameState| {
            // The pinned sources, in contract order.
            let pinned: Vec<ObjectId> = super::export::contract_pins(st)
                .into_iter()
                .filter(|x| st.objects.get(*x).zone == Zone::Library)
                .collect();
            let lib = &mut st.players[0].library;
            let want = match top {
                PinTopV1::Neither => None,
                PinTopV1::First => pinned.first().copied(),
                PinTopV1::Second => pinned.get(1).copied(),
            };
            match want {
                Some(x) => {
                    let at = lib
                        .iter()
                        .position(|y| *y == x)
                        .expect("pin in the library");
                    lib.swap(0, at);
                }
                None => {
                    let at = lib
                        .iter()
                        .position(|y| !pinned.contains(y))
                        .expect("a card that is not a pin");
                    lib.swap(0, at);
                }
            }
        },
    ));
    o_intents.push(activate("Krark-Clan Shaman"));
    if pins == 1 {
        // The other Lembas is still an artifact on the battlefield; with
        // both sacrificed the Wellspring is the only candidate.
        o_intents.push(pick("Ichor Wellspring"));
    }
    script.push(o, o_intents);
    script.push(
        v,
        vec![
            play_land("Island"),
            play_land("Island"),
            play_land("Island"),
            play_land("Island"),
            play_land("Island"),
        ],
    );
    script
}

fn sum_into(totals: &mut BTreeMap<String, u64>, r: &PairReportV1) {
    for (m, n) in &r.witnesses {
        *totals.entry(m.clone()).or_default() += n;
    }
}

/// The records of every variant of a fixture, and their pairwise reports
/// against the first variant.
fn pairs(name: &str, runs: Vec<(Vec<AtV1>, ScriptV1)>) -> (Vec<Vec<AtV1>>, Vec<PairReportV1>) {
    let records: Vec<Vec<AtV1>> = runs.into_iter().map(|(r, _)| r).collect();
    let reports: Vec<PairReportV1> = records[1..]
        .iter()
        .map(|b| compare_records(&records[0], b))
        .collect();
    for (i, r) in reports.iter().enumerate() {
        eprintln!("{name} variant 0 vs {}: {r:?}", i + 1);
        assert_eq!(
            r.p_differs, 0,
            "{name}: the correct producer differs: {r:?}"
        );
    }
    (records, reports)
}

#[test]
fn spy_versus_terror_engine_fixture() {
    let v = PlayerId::P0;
    let variants = [
        ["Thought Scour", "Mental Note"],
        ["Tolarian Terror", "Deep Analysis"],
        ["Ponder", "Murmuring Mystic"],
        ["Cryptic Serpent", "Tolarian Terror"],
    ];
    let runs: Vec<_> = variants
        .iter()
        .map(|pb| record(spy_versus_terror(*pb), v, 30))
        .collect();
    let (records, reports) = pairs("spy-versus-terror", runs);
    // After the Brainstorm, V's decisions carry the survivor node.
    let node = (
        vec![
            "Murmuring Mystic".to_string(),
            "Ponder".to_string(),
            "Tolarian Terror".to_string(),
        ],
        1,
    );
    let after: Vec<&AtV1> = records[0]
        .iter()
        .filter(|r| r.nodes.contains(&node))
        .collect();
    assert!(
        !after.is_empty(),
        "the survivor node ({{Terror, Ponder, Mystic}}, 1) appears"
    );
    for r in &after {
        assert!(r.p.is_ok(), "exported while the node holds: {:?}", r.p);
        assert_eq!(r.known.len(), 1, "one known_name survivor: {:?}", r.known);
    }
    // Terror's next draw takes a put-back card back: the draw-back
    // amendment to 6.4 undoes that put-back's removal, so the node is
    // ({Terror, Ponder, Mystic}, 2) and two survivors are exported (no R8).
    let restored = (node.0.clone(), 2);
    let back: Vec<&AtV1> = records[0]
        .iter()
        .filter(|r| r.nodes.contains(&restored))
        .collect();
    assert!(
        !back.is_empty(),
        "the restored node appears after the draw-back"
    );
    for r in &back {
        assert!(r.p.is_ok(), "exported after the draw-back: {:?}", r.p);
        assert_eq!(r.known.len(), 2, "two known_name survivors: {:?}", r.known);
    }
    assert!(
        records[0].iter().all(|r| r.p != Err("R8".to_string())),
        "no R8 after an in-order draw-back"
    );
    eprintln!(
        "spy-versus-terror canonical survivors: {:?}, after the draw-back {:?}",
        after[0].known, back[0].known
    );
    assert!(reports.iter().all(|r| r.compared > 0 && !r.truncated));
}

#[test]
fn fable_bounce_engine_fixture() {
    let v = PlayerId::P1;
    let runs: Vec<_> = [0, 2, 1]
        .iter()
        .map(|hawks| record(fable_bounce(*hawks), v, 30))
        .collect();
    let (records, reports) = pairs("fable-bounce", runs);
    let node = (vec!["Squadron Hawk".to_string(); 3], 1);
    let bounced: Vec<&AtV1> = records[0]
        .iter()
        .filter(|r| r.nodes.contains(&node))
        .collect();
    assert!(
        !bounced.is_empty(),
        "the survivor node ({{Hawk x3}}, 1) appears"
    );
    // The last export while the node holds (after the bounce, before the
    // owner's next draw takes a put-back card back: R8).
    let last = bounced
        .iter()
        .rev()
        .find(|r| r.p.is_ok())
        .expect("an export while the node holds");
    assert_eq!(
        last.known,
        vec!["Squadron Hawk".to_string()],
        "one untracked Hawk survivor is still exported after the bounce"
    );
    assert!(reports.iter().all(|r| r.compared > 0));
}

#[test]
fn cawgates_lorien_engine_fixture() {
    let v = PlayerId::P1;
    // Both variants return the fetched Island, so the second Lorien finds
    // it in both worlds.
    let runs: Vec<_> = [Some("Squadron Hawk"), None]
        .iter()
        .map(|with| record(cawgates_lorien(*with), v, 20))
        .collect();
    let (records, reports) = pairs("cawgates-lorien", runs);
    let node = (
        vec![
            "Island".to_string(),
            "Squadron Hawk".to_string(),
            "Squadron Hawk".to_string(),
            "Squadron Hawk".to_string(),
        ],
        2,
    );
    for rec in &records {
        let i = rec
            .iter()
            .position(|r| r.nodes.contains(&node))
            .expect("the node ({Hawk x3, Island}, 2) appears");
        let first_refusal = rec[i..]
            .iter()
            .position(|r| r.p == Err("R8b".to_string()))
            .map(|k| i + k)
            .expect("R8b refuses once the second Lorien finds the Island");
        assert!(
            rec[i..first_refusal]
                .iter()
                .all(|r| r.p.is_ok() || r.p == Err("R3".into())),
            "exports up to the second Lorien"
        );
        assert!(
            rec[first_refusal..]
                .iter()
                .all(|r| r.p == Err("R8b".into()) || !r.nodes.contains(&node)),
            "R8b holds while the node lives"
        );
    }
    // The worlds' hands differ after the Brainstorm, so later default
    // answers may diverge publicly: the pair truncates there, after R8b.
    assert!(reports
        .iter()
        .all(|r| r.compared > 0 && r.p_refusals.get("R8b").copied().unwrap_or(0) > 0));
}

#[test]
fn w_pin_engine_fixtures() {
    let v = PlayerId::P1;
    let mut totals = BTreeMap::new();
    for pins in [1, 2] {
        let tops: &[PinTopV1] = if pins == 1 {
            &[PinTopV1::Neither, PinTopV1::First]
        } else {
            &[PinTopV1::Neither, PinTopV1::First, PinTopV1::Second]
        };
        let runs: Vec<_> = tops
            .iter()
            .map(|t| record(w_pin(pins, *t), v, 12))
            .collect();
        let (records, reports) = pairs(&format!("w-pin-{pins}"), runs);
        // While the abilities wait after the draw, the canonical choice
        // keeps every pin in the library (6.3: zero transfers satisfy C1
        // and capacity allows it).
        let waiting: Vec<&AtV1> = records[0]
            .iter()
            .filter(|r| r.pinned.len() == pins)
            .collect();
        assert!(
            !waiting.is_empty(),
            "w-pin-{pins}: pinned occupants exported"
        );
        for r in &waiting {
            assert!(
                r.pinned.iter().all(|p| p.ends_with("@Library")),
                "w-pin-{pins}: {:?}",
                r.pinned
            );
        }
        for r in &reports {
            assert!(r.compared > 0, "w-pin-{pins}: {r:?}");
            sum_into(&mut totals, r);
        }
    }
    eprintln!("F-pin witnesses: {totals:?}");
    assert!(
        totals.get("L6").copied().unwrap_or(0) > 0,
        "L6 has F-pin witnesses"
    );
}

/// L8 (count-fact binding) witnesses: compared decisions where the correct
/// producer exports equal bytes in both worlds and mutant L8 differs. The
/// bounced Hawk's span began on the stack, so L8's `C:` binding exports its
/// generation above 1, while a fresh Hawk exports 1. The witness pair puts
/// back no Hawk in one world and the bounced Hawk and a fresh one in the
/// other, so L8's bound copies differ in history; the control pair puts
/// back two fresh Hawks, so the bounced copy stays in both worlds and L8
/// must not differ (binding alone is invisible between equal copies).
#[test]
fn l8_bound_copies_engine_fixture() {
    // Ruling 2 (c): the delivered comparison counts witnesses only at
    // decisions the entrant receives, so the fixture runs in several game
    // seeds (the forced deals and script are the same; later draws differ)
    // and sums them; every seed's control must stay at 0.
    let (mut total, mut total_control, mut total_samples) = (0u64, 0u64, 0u64);
    for seed in [72u64, 73, 74, 75, 76, 77] {
        let (w, c, n) = l8_bound_copies_witnesses(seed);
        total += w;
        total_control += c;
        total_samples += n;
    }
    eprintln!(
        "l8-bound-copies over six seeds: {total} delivered witnesses; control {total_control} of {total_samples}"
    );
    assert!(
        total >= 5,
        "L8 has at least 5 delivered witnesses (design 13): {total}"
    );
    assert!(
        total_samples >= 5,
        "the control compares at least 5 post-Brainstorm decisions: {total_samples}"
    );
    assert_eq!(
        total_control, 0,
        "binding between equal copies leaves L8's bytes unchanged"
    );
}

/// Ruling 4 (3) diagnostics: where the L8 control pair (no Hawk put back vs
/// two fresh Hawks put back) shows an L8 difference while P agrees and both
/// worlds export two `known_name` Hawks, the bytes of L8's two exports.
#[test]
fn l8_control_difference_bytes() {
    let v = PlayerId::P1;
    let hawks = vec!["Squadron Hawk".to_string(); 2];
    for seed in [73u64, 74, 75] {
        let (a, _) = record(l8_bound_copies(L8PutBackV1::NoHawk, seed), v, 30);
        let (b, _) = record(l8_bound_copies(L8PutBackV1::TwoFresh, seed), v, 30);
        let mut shown = 0;
        for (x, y) in a.iter().zip(&b) {
            if !(x.p.is_ok() && x.p == y.p && x.known == hawks && y.known == hawks) {
                continue;
            }
            let l8 = |r: &AtV1| {
                r.mutants
                    .iter()
                    .find(|(m, _)| *m == MutantV1::L8)
                    .map(|(_, o)| o.clone())
                    .expect("L8")
            };
            let (mx, my) = (l8(x), l8(y));
            if mx == my {
                continue;
            }
            // The first differing byte of the two exports, with context.
            let (bx, by) = (mx.clone().unwrap_or_default(), my.clone().unwrap_or_default());
            let at = bx.iter().zip(&by).position(|(u, v)| u != v).unwrap_or(bx.len().min(by.len()));
            let window = |b: &[u8]| String::from_utf8_lossy(&b[at.saturating_sub(160)..(at + 160).min(b.len())]).to_string();
            eprintln!(
                "L8 control seed {seed} script step {} (former view equal {}): lengths {} / {}, first difference at byte {at}:
  W  ...{}...
  W' ...{}...",
                x.step,
                x.view == y.view,
                bx.len(),
                by.len(),
                window(&bx),
                window(&by)
            );
            shown += 1;
            if shown >= 2 {
                break;
            }
        }
    }
}

/// One seed of the L8 fixture: (witnesses, control witnesses, control samples).
fn l8_bound_copies_witnesses(seed: u64) -> (u64, u64, u64) {
    let v = PlayerId::P1;
    let runs: Vec<_> = [
        L8PutBackV1::NoHawk,
        L8PutBackV1::TwoFresh,
        L8PutBackV1::BouncedAndFresh,
    ]
    .iter()
    .map(|p| {
        record_strict(
            l8_bound_copies(*p, seed),
            v,
            30,
            &format!("l8-seed{seed}-{p:?}"),
        )
    })
    .collect();
    let (records, reports) = pairs(&format!("l8-bound-copies seed {seed}"), runs);
    // Compared decisions after the Brainstorm (P exports two `known_name`
    // Hawks, equal bytes, in both worlds), split by whether L8 differs:
    // (witnesses, agreeing samples).
    let hawks = vec!["Squadron Hawk".to_string(); 2];
    let after_brainstorm = |b: &[AtV1]| -> (u64, u64) {
        let (mut differ, mut agree) = (0, 0);
        for (x, y) in records[0].iter().zip(b) {
            if x.view != y.view {
                break;
            }
            if !(x.p.is_ok() && x.p == y.p && x.known == hawks && y.known == hawks) {
                continue;
            }
            let l8 = |r: &AtV1| {
                r.mutants
                    .iter()
                    .find(|(m, _)| *m == MutantV1::L8)
                    .map(|(_, o)| o.clone())
            };
            if l8(x) != l8(y) {
                differ += 1;
            } else {
                agree += 1;
            }
        }
        (differ, agree)
    };
    let (control, control_samples) = after_brainstorm(&records[1]);
    let (bounced_out, _) = after_brainstorm(&records[2]);
    eprintln!(
        "l8-bound-copies: L8 witnesses with P exporting two known_name Hawks: bounced and fresh put back {bounced_out}; two fresh put back (control) {control} of {}",
        control + control_samples
    );
    assert!(
        reports.iter().all(|r| r.compared > 0),
        "both pairs compare decisions: {reports:?}"
    );
    (bounced_out, control, control + control_samples)
}
