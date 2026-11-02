//! AC1's realizability construction (W1 design v7, section 13; rulings 35,
//! 37.6 and 39).
//!
//! For a decision d of world W and an export's canonical hidden contents,
//! the harness builds W* from W with three kinds of change, all legal
//! alternatives of the same game:
//! - a forced deal: definitions exchanged among the owner's objects that V
//!   never tracked up to d (equivalent to another shuffle at the start);
//! - forced shuffle outcomes: two library objects exchange positions right
//!   after a shuffle of their owner's library, when V tracks neither from
//!   then to d;
//! - forced hidden choices: at an owner's effect choice (a Brainstorm
//!   put-back, a scry order), another object of the hand is chosen.
//!
//! W's later actions are replayed with native ids mapped through the
//! exchanges. W* passes when V's views equal W's at every V decision up to
//! d and W*'s true hidden zones equal the export's canonical contents:
//! per-zone name multisets of the owner's untracked cards, pinned objects'
//! zones (and, for the correct producer, export(W*) = export(W)).
//!
//! The search looks for exchanges that put every misplaced object V once
//! tracked (revealed, then hidden; a pinned source) in its canonical zone;
//! never-tracked objects are then matched by the deal. It is a
//! construction, checked by replay: a failure for the correct producer is
//! reported, never assumed away.

use super::harness::{seat, swap_objects, view};
use super::tests::export_at;
use super::*;
use crate::ids::{ObjectId, PlayerId};
use crate::rl_session::{FastActorResponseV1, FastActorSessionV1};
use crate::state::{GameState, ObservationNoteKindV1};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};

/// W: its start and each chosen candidate with native ids.
#[derive(Clone)]
pub(crate) struct NativeGameV1 {
    pub start: FastActorSessionV1,
    pub actions: Vec<Value>,
    /// W's own forced shuffle outcomes (a fixture's edits): library
    /// positions exchanged at a decision, already reflected in `actions`.
    pub edits: Vec<(usize, ObjectId, ObjectId)>,
}

/// A candidate's semantics with ids mapped through `sigma` and incarnation
/// numbers dropped (an exchanged object keeps its own incarnation).
fn mapped(v: &Value, sigma: &HashMap<u64, u64>) -> Value {
    match v {
        Value::Object(m) => Value::Object(
            m.iter()
                .filter(|(k, _)| k.as_str() != "zone_change_count")
                .map(|(k, x)| {
                    if k == "arena_id" {
                        if let Some(n) = x.as_u64() {
                            return (k.clone(), Value::from(*sigma.get(&n).unwrap_or(&n)));
                        }
                    }
                    (k.clone(), mapped(x, sigma))
                })
                .collect(),
        ),
        Value::Array(xs) => Value::Array(xs.iter().map(|x| mapped(x, sigma)).collect()),
        _ => v.clone(),
    }
}

/// The object a choice names, if any.
fn named_object(v: &Value) -> Option<u32> {
    v["target"]["object"]["arena_id"]
        .as_u64()
        .or_else(|| v["candidate"]["arena_id"].as_u64())
        .or_else(|| v["card"]["arena_id"].as_u64())
        .map(|n| n as u32)
}

fn hidden_choice(kind: &str) -> bool {
    matches!(
        kind,
        "choose_effect_target" | "choose_effect_option" | "choose_cost_target"
    )
}

/// One exchange that makes W*.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum MoveV1 {
    /// At step `at`, the owner's choice names `x` where W's named `y`;
    /// from then on the two objects exchange roles.
    Choice { at: usize, x: u32, y: u32 },
    /// At decision `at`, the first after a shuffle of the owner's library,
    /// `x` and `y` exchange library positions (and roles).
    Shuffle { at: usize, x: u32, y: u32 },
}

/// An owner's effect choice in a replay: the object chosen and the other
/// hand objects it could have named.
#[derive(Clone, Debug)]
struct ChoicePointV1 {
    at: usize,
    chosen: u32,
    others: Vec<u32>,
}

struct RunV1 {
    s: FastActorSessionV1,
    views: Vec<Option<Vec<u8>>>,
    choices: Vec<ChoicePointV1>,
    /// Decision indices right after a shuffle of the owner's library, with
    /// the owner's library at that decision.
    shuffles: Vec<(usize, Vec<ObjectId>)>,
}

/// Replays W's first `d` steps with the deal exchanges and moves applied.
fn replay(
    game: &NativeGameV1,
    deal: &[(ObjectId, ObjectId)],
    moves: &[MoveV1],
    d: usize,
    owner: PlayerId,
    viewer: PlayerId,
) -> Result<RunV1, String> {
    let mut s = if deal.is_empty() {
        game.start.clone()
    } else {
        let mut st = game.start.game_state().clone();
        for (a, b) in deal {
            swap_objects(&mut st, *a, *b);
        }
        FastActorSessionV1::import_infoset_v1(&game.start, st, 0)
    };
    let mut sigma: HashMap<u64, u64> = HashMap::new();
    let swap = |sigma: &mut HashMap<u64, u64>, x: u32, y: u32| {
        let (x, y) = (u64::from(x), u64::from(y));
        // sigma maps W's object to W*'s: compose with the transposition.
        let mut inv: HashMap<u64, u64> = sigma.iter().map(|(k, v)| (*v, *k)).collect();
        let (wx, wy) = (
            *inv.get(&x).unwrap_or(&x),
            *inv.get(&y).unwrap_or(&y),
        );
        sigma.insert(wx, y);
        sigma.insert(wy, x);
        inv.clear();
    };
    let mut run = RunV1 {
        s: s.clone(),
        views: Vec::new(),
        choices: Vec::new(),
        shuffles: Vec::new(),
    };
    for i in 0..d {
        for (at, a, b) in &game.edits {
            if *at == i {
                let context = s
                    .surface_context_v1()
                    .ok_or("no surface context at W's own edit")?;
                let mut st = s.game_state().clone();
                for p in &mut st.players {
                    let (pa, pb) = (
                        p.library.iter().position(|o| o == a),
                        p.library.iter().position(|o| o == b),
                    );
                    if let (Some(pa), Some(pb)) = (pa, pb) {
                        p.library.swap(pa, pb);
                    }
                }
                s = FastActorSessionV1::import_infoset_with_surface_v1(&s, st, 0, Some(context));
            }
        }
        for m in moves {
            if let MoveV1::Shuffle { at, x, y } = *m {
                if at == i {
                    let context = s
                        .surface_context_v1()
                        .ok_or("no surface context at a forced shuffle outcome")?;
                    let mut st = s.game_state().clone();
                    let lib = &mut st.players[owner.index()].library;
                    let (Some(px), Some(py)) = (
                        lib.iter().position(|o| o.0 == x),
                        lib.iter().position(|o| o.0 == y),
                    ) else {
                        return Err("a forced shuffle outcome names a card outside the library".into());
                    };
                    lib.swap(px, py);
                    s = FastActorSessionV1::import_infoset_with_surface_v1(&s, st, 0, Some(context));
                    swap(&mut sigma, x, y);
                }
            }
        }
        let FastActorResponseV1::Decision(dd) = s.current_response() else {
            return Err(format!("terminal before step {i}"));
        };
        let p = seat(dd.acting_player);
        for m in moves {
            if let MoveV1::Choice { at, x, y } = *m {
                if at == i {
                    swap(&mut sigma, x, y);
                }
            }
        }
        if p == viewer && dd.legal_action_count >= 2 {
            run.views.push(view(&s));
        }
        let want = mapped(&game.actions[i], &sigma);
        let sem = s.diagnostic_current_action_semantics().unwrap_or_default();
        let vals: Vec<Value> = sem
            .iter()
            .map(|x| serde_json::to_value(x).expect("semantics serialize"))
            .collect();
        let empty = HashMap::new();
        let Some(index) = vals.iter().position(|v| mapped(v, &empty) == want) else {
            return Err(format!("step {i}: no candidate matches {want}"));
        };
        let kind = vals[index]["action_kind"].as_str().unwrap_or("");
        if p == owner && hidden_choice(kind) {
            let hand = &s.game_state().players[owner.index()].hand;
            if let Some(chosen) = named_object(&vals[index]).filter(|o| hand.contains(&ObjectId(*o))) {
                let others: Vec<u32> = vals
                    .iter()
                    .filter(|v| v["action_kind"] == vals[index]["action_kind"])
                    .filter_map(named_object)
                    .filter(|o| *o != chosen && hand.contains(&ObjectId(*o)))
                    .collect();
                run.choices.push(ChoicePointV1 {
                    at: i,
                    chosen,
                    others,
                });
            }
        }
        let before = s.game_state().observation_journal.notes().len();
        s.step(dd.episode_id, dd.step, index as u32)
            .map_err(|e| format!("step {i}: {e:?}"))?;
        let notes = s.game_state().observation_journal.notes();
        if notes[before.min(notes.len())..].iter().any(|n| {
            matches!(n.kind, ObservationNoteKindV1::LibraryRandomized { owner: o } if o == owner)
        }) {
            run.shuffles
                .push((i + 1, s.game_state().players[owner.index()].library.clone()));
        }
    }
    run.s = s;
    Ok(run)
}

/// Canonical hidden contents of the owner: untracked hand and library
/// definitions, and pinned definitions by zone.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ContentsV1 {
    pub hand: BTreeMap<u16, u32>,
    pub library: BTreeMap<u16, u32>,
    pub pins: BTreeMap<(u16, bool), u32>,
}

fn add(m: &mut BTreeMap<u16, u32>, d: u16) {
    *m.entry(d).or_default() += 1;
}

/// The export's canonical contents of the opponent's hidden zones.
pub(crate) fn export_contents(x: &InfosetV1, owner: PlayerId) -> ContentsV1 {
    let mut c = ContentsV1::default();
    let p = &x.state.players[owner.index()];
    for (zone_hand, ids) in [(true, &p.hand), (false, &p.library)] {
        for id in ids {
            let def = x.state.objects.get(*id).card_def;
            match x.handles[id.0 as usize] {
                HandleV1::U { pinned: true, .. } => {
                    *c.pins.entry((def, zone_hand)).or_default() += 1
                }
                HandleV1::U { .. } => add(if zone_hand { &mut c.hand } else { &mut c.library }, def),
                _ => {}
            }
        }
    }
    c
}

/// The true contents of the owner's hidden zones, as V's projection
/// partitions them, with each untracked object's class.
struct TruthV1 {
    contents: ContentsV1,
    /// Untracked hidden objects V tracked earlier (fixed definitions).
    seen: Vec<(ObjectId, u16, bool)>,
    /// Untracked hidden objects V never tracked.
    never: Vec<(ObjectId, u16, bool)>,
    pins: Vec<(ObjectId, u16, bool)>,
}

fn truth(state: &GameState, viewer: PlayerId, owner: PlayerId) -> TruthV1 {
    let projection =
        super::projection::project_v1(state, state.observation_journal.notes(), viewer);
    let pins: BTreeSet<ObjectId> = super::export::contract_pins(state).into_iter().collect();
    let mut t = TruthV1 {
        contents: ContentsV1::default(),
        seen: Vec::new(),
        never: Vec::new(),
        pins: Vec::new(),
    };
    let p = &state.players[owner.index()];
    for (in_hand, ids) in [(true, &p.hand), (false, &p.library)] {
        for (pos, id) in ids.iter().enumerate() {
            if projection.tracked(*id)
                || (!in_hand && projection.library[owner.index()].contains_key(&(pos as u32)))
            {
                continue;
            }
            let def = state.objects.get(*id).card_def;
            if pins.contains(id) {
                *t.contents.pins.entry((def, in_hand)).or_default() += 1;
                t.pins.push((*id, def, in_hand));
                continue;
            }
            add(
                if in_hand {
                    &mut t.contents.hand
                } else {
                    &mut t.contents.library
                },
                def,
            );
            let entry = (*id, def, in_hand);
            if projection.objects[id.0 as usize].spans.is_empty() {
                t.never.push(entry);
            } else {
                t.seen.push(entry);
            }
        }
    }
    t
}

fn take(m: &mut BTreeMap<u16, u32>, d: u16) -> bool {
    match m.get_mut(&d) {
        Some(c) if *c > 0 => {
            *c -= 1;
            true
        }
        _ => false,
    }
}

/// Objects in the wrong zone for `target`: seen objects whose definition
/// the target zone cannot absorb, and pins in the wrong zone.
fn misplaced(t: &TruthV1, target: &ContentsV1) -> Vec<(ObjectId, bool)> {
    let (mut hand, mut library) = (target.hand.clone(), target.library.clone());
    let mut out = Vec::new();
    for (id, def, in_hand) in &t.seen {
        let room = if *in_hand { &mut hand } else { &mut library };
        if !take(room, *def) {
            out.push((*id, *in_hand));
        }
    }
    let mut pins = target.pins.clone();
    for (id, def, in_hand) in &t.pins {
        let key = (*def, *in_hand);
        if pins.get(&key).copied().unwrap_or(0) > 0 {
            *pins.get_mut(&key).expect("checked") -= 1;
        } else {
            out.push((*id, *in_hand));
        }
    }
    out
}

/// The deal exchanges that give the never-tracked objects the target's
/// remaining definitions, or None if the multisets disagree.
fn deal_for(t: &TruthV1, target: &ContentsV1) -> Option<Vec<(ObjectId, ObjectId)>> {
    let mut need = target.hand.clone();
    for (_, def, in_hand) in &t.seen {
        if *in_hand && !take(&mut need, *def) {
            return None;
        }
    }
    let hand: Vec<&(ObjectId, u16, bool)> = t.never.iter().filter(|x| x.2).collect();
    let library: Vec<&(ObjectId, u16, bool)> = t.never.iter().filter(|x| !x.2).collect();
    let mut wrong = Vec::new();
    for x in &hand {
        if !take(&mut need, x.1) {
            wrong.push(x.0);
        }
    }
    let mut used = BTreeSet::new();
    let mut swaps = Vec::new();
    for a in wrong {
        let b = library
            .iter()
            .find(|y| !used.contains(&y.0) && need.get(&y.1).copied().unwrap_or(0) > 0)?;
        take(&mut need, b.1);
        used.insert(b.0);
        swaps.push((a, b.0));
    }
    need.values().all(|c| *c == 0).then_some(swaps)
}

/// A constructed W*.
#[derive(Clone, Debug)]
pub(crate) struct RealizedV1 {
    pub moves: Vec<MoveV1>,
    pub deal: Vec<(ObjectId, ObjectId)>,
}

/// Builds W* for decision step `d` of `game` and `target` (the owner is
/// V's opponent). `check_export` also requires export(W*) = `export_w`.
pub(crate) fn realize(
    game: &NativeGameV1,
    d: usize,
    viewer: PlayerId,
    target: &ContentsV1,
    export_w: Option<&[u8]>,
) -> Result<RealizedV1, String> {
    const MAX_NODES: usize = 400;
    let owner = viewer.opponent();
    let base = replay(game, &[], &[], d, owner, viewer)?;
    let mut queue: VecDeque<Vec<MoveV1>> = VecDeque::from([Vec::new()]);
    let mut visited: BTreeSet<Vec<MoveV1>> = BTreeSet::new();
    let mut last = String::from("no candidate W*");
    let mut nodes = 0;
    while let Some(moves) = queue.pop_front() {
        nodes += 1;
        if nodes > MAX_NODES {
            return Err(format!("search budget spent; last: {last}"));
        }
        let run = match replay(game, &[], &moves, d, owner, viewer) {
            Ok(r) => r,
            Err(e) => {
                last = e;
                continue;
            }
        };
        if run.views != base.views {
            last = format!("{moves:?}: V's views differ");
            continue;
        }
        let t = truth(run.s.game_state(), viewer, owner);
        let wrong = misplaced(&t, target);
        if wrong.is_empty() {
            let Some(deal) = deal_for(&t, target) else {
                last = format!("{moves:?}: the deal cannot match the target multisets");
                continue;
            };
            let star = replay(game, &deal, &moves, d, owner, viewer)?;
            if star.views != base.views {
                last = format!("{moves:?} with the deal: V's views differ");
                continue;
            }
            let got = truth(star.s.game_state(), viewer, owner).contents;
            if got != *target {
                last = format!("{moves:?}: W* contents {got:?} differ from {target:?}");
                continue;
            }
            if let Some(bytes) = export_w {
                let x = export_at(&star.s, viewer, ExportOptionsV1::default())
                    .map_err(|e| format!("export(W*) refused: {}", e.code_name()))?;
                if x.bytes != bytes {
                    return Err(format!("{moves:?}: export(W*) differs from export(W)"));
                }
            }
            return Ok(RealizedV1 { moves, deal });
        }
        if moves.len() >= 4 {
            last = format!("{moves:?}: {} objects still misplaced", wrong.len());
            continue;
        }
        // Exchanges that move a misplaced object to the other zone.
        let end_zone: HashMap<u32, bool> = {
            let st = run.s.game_state();
            let p = &st.players[owner.index()];
            p.hand
                .iter()
                .map(|o| (o.0, true))
                .chain(p.library.iter().map(|o| (o.0, false)))
                .collect()
        };
        let mut next: Vec<MoveV1> = Vec::new();
        for (m, in_hand) in &wrong {
            let m = m.0;
            for c in &run.choices {
                // m is in the hand at d: it must be the one chosen instead
                // of an object W chose (or the reverse).
                if c.others.contains(&m) && end_zone.get(&c.chosen) == Some(&!in_hand) {
                    next.push(MoveV1::Choice {
                        at: c.at,
                        x: m,
                        y: c.chosen,
                    });
                }
                if c.chosen == m {
                    for o in &c.others {
                        if end_zone.get(o) == Some(&!in_hand) {
                            next.push(MoveV1::Choice {
                                at: c.at,
                                x: *o,
                                y: m,
                            });
                        }
                    }
                }
            }
            for (at, lib) in &run.shuffles {
                if !lib.iter().any(|o| o.0 == m) {
                    continue;
                }
                for o in lib.iter().take(12) {
                    if o.0 != m && end_zone.get(&o.0) == Some(&!in_hand) {
                        next.push(MoveV1::Shuffle {
                            at: *at,
                            x: m,
                            y: o.0,
                        });
                    }
                }
            }
        }
        for mv in next {
            let mut ms = moves.clone();
            ms.push(mv);
            ms.sort();
            if visited.insert(ms.clone()) {
                queue.push_back(ms);
            }
        }
    }
    Err(last)
}

/// The session of `game` at decision step `d` (W itself, no exchanges).
pub(crate) fn at_step(game: &NativeGameV1, d: usize, viewer: PlayerId) -> FastActorSessionV1 {
    replay(game, &[], &[], d, viewer.opponent(), viewer)
        .expect("W replays")
        .s
}

/// W* for the export of `mutant` (None: the correct producer) at decision
/// step `d`. The outer error is a refusal (no export); the inner one a
/// construction failure.
pub(crate) fn realize_at(
    game: &NativeGameV1,
    d: usize,
    viewer: PlayerId,
    mutant: Option<MutantV1>,
) -> Result<Result<RealizedV1, String>, String> {
    let s = at_step(game, d, viewer);
    let x = export_at(&s, viewer, ExportOptionsV1 { mutant })
        .map_err(|e| e.code_name().to_string())?;
    let target = export_contents(&x, viewer.opponent());
    let bytes = mutant.is_none().then_some(x.bytes.as_slice());
    Ok(realize(game, d, viewer, &target, bytes))
}

#[cfg(test)]
mod tests {
    use super::super::fixtures::{
        cawgates_lorien, fable_bounce, record, spy_versus_terror, w_pin, AtV1, PinTopV1, ScriptV1,
    };
    use super::*;

    fn native_of(script: &ScriptV1) -> NativeGameV1 {
        NativeGameV1 {
            start: script.start.clone(),
            actions: script.native.clone(),
            edits: script.edits.clone(),
        }
    }

    /// Every exported decision of a fixture run is realizable for the
    /// correct producer; returns the number checked.
    fn all_realizable(name: &str, records: &[AtV1], game: &NativeGameV1, v: PlayerId) -> u64 {
        let mut n = 0;
        for r in records.iter().filter(|r| r.p.is_ok()) {
            match realize_at(game, r.step, v, None) {
                Ok(Ok(_)) => n += 1,
                other => panic!("{name}: P is not realizable at step {}: {other:?}", r.step),
            }
        }
        n
    }

    fn unrealizable(game: &NativeGameV1, d: usize, v: PlayerId, m: MutantV1) -> bool {
        matches!(realize_at(game, d, v, Some(m)), Ok(Err(_)))
    }

    #[test]
    fn realizability_and_validity_mutants_on_fixtures() {
        let mut checked = 0;
        // Spy versus Terror: L11 (tracker-rule facts only) is not realizable.
        let (records, script) = record(
            spy_versus_terror(["Thought Scour", "Mental Note"]),
            PlayerId::P0,
            30,
        );
        let game = native_of(&script);
        checked += all_realizable("spy-versus-terror", &records, &game, PlayerId::P0);
        let with_node: Vec<&AtV1> = records
            .iter()
            .filter(|r| !r.nodes.is_empty() && r.p.is_ok())
            .collect();
        assert!(!with_node.is_empty());
        let l11 = with_node
            .iter()
            .filter(|r| unrealizable(&game, r.step, PlayerId::P0, MutantV1::L11))
            .count();
        eprintln!(
            "L11 unrealizable at {l11} of {} node decisions (Spy versus Terror)",
            with_node.len()
        );
        assert_eq!(l11, with_node.len(), "L11 fails wherever the node holds");

        // Fable bounce: L12 (arrivals counted) is not realizable after the
        // bounce.
        for put_back in [
            ["Spell Pierce", "Preordain"],
            ["Squadron Hawk", "Squadron Hawk"],
        ] {
            let (records, script) = record(fable_bounce(put_back), PlayerId::P1, 30);
            let game = native_of(&script);
            checked += all_realizable("fable-bounce", &records, &game, PlayerId::P1);
            let last = records
                .iter()
                .rev()
                .find(|r| r.p.is_ok() && !r.nodes.is_empty())
                .expect("a node decision");
            assert!(
                unrealizable(&game, last.step, PlayerId::P1, MutantV1::L12),
                "L12 is realizable after the bounce ({put_back:?})"
            );
        }

        // CawGates Lorien: realizable up to the R8b refusal (every export).
        let (records, script) = record(
            cawgates_lorien(["Island", "Squadron Hawk"]),
            PlayerId::P1,
            20,
        );
        checked += all_realizable("cawgates-lorien", &records, &native_of(&script), PlayerId::P1);

        // W-pin-1 and W-pin-2: every variant realizable; L9 is not while two
        // pins compete for one draw (where its export differs from P's).
        let mut l9 = 0;
        for (pins, tops) in [
            (1, vec![PinTopV1::Neither, PinTopV1::First]),
            (
                2,
                vec![PinTopV1::Neither, PinTopV1::First, PinTopV1::Second],
            ),
        ] {
            for top in tops {
                let (records, script) = record(w_pin(pins, top), PlayerId::P1, 12);
                let game = native_of(&script);
                checked += all_realizable(
                    &format!("w-pin-{pins} {top:?}"),
                    &records,
                    &game,
                    PlayerId::P1,
                );
                if pins != 2 {
                    continue;
                }
                for r in records.iter().filter(|r| r.pinned.len() == 2 && r.p.is_ok()) {
                    let s = at_step(&game, r.step, PlayerId::P1);
                    let l9_bytes = export_at(
                        &s,
                        PlayerId::P1,
                        ExportOptionsV1 {
                            mutant: Some(MutantV1::L9),
                        },
                    )
                    .map(|x| x.bytes)
                    .ok();
                    if l9_bytes.as_ref() == r.p.as_ref().ok() {
                        continue;
                    }
                    assert!(
                        unrealizable(&game, r.step, PlayerId::P1, MutantV1::L9),
                        "L9 is realizable at step {} ({top:?})",
                        r.step
                    );
                    l9 += 1;
                }
            }
        }
        eprintln!(
            "realizability: {checked} fixture decisions realized for P; L9 unrealizable at {l9} competing-pin decisions"
        );
        assert!(l9 > 0, "W-pin-2 has decisions where two pins compete for one draw");
    }

    /// L10 on the AC3-L shuffle-trigger case (6.5 case A) in the W-pin
    /// fixture: with Lembas sacrificed and its shuffle trigger waiting, P's
    /// export continues like the true state; L10's export refuses, halts
    /// on the validator, or continues differently.
    #[test]
    fn l10_fails_on_the_waiting_shuffle_trigger() {
        let v = PlayerId::P1;
        let (records, script) = record(w_pin(1, PinTopV1::Neither), v, 12);
        let game = native_of(&script);
        let lembas = super::super::fixtures::id("Lembas");
        let mut cases = 0;
        let mut l10_fails = 0;
        for r in records.iter().filter(|r| r.p.is_ok()) {
            let s = at_step(&game, r.step, v);
            let st = s.game_state();
            let waiting = st.stack.len() == 2
                && st.players[0]
                    .graveyard
                    .iter()
                    .any(|x| st.objects.get(*x).card_def == lembas);
            if !waiting {
                continue;
            }
            cases += 1;
            let from = st.engine.event_history.len();
            let continue_from = |state: GameState| -> Result<Value, String> {
                let mut c = FastActorSessionV1::import_infoset_v1(&s, state, 0);
                super::super::tests::resolve_waiting(&mut c, 1, 60);
                if let FastActorResponseV1::Terminal(t) = c.current_response() {
                    if t.terminal_classification == crate::rl::TerminalClassificationV1::Halted {
                        return Err(format!("halted: {}", t.terminal_reason));
                    }
                }
                Ok(super::super::tests::public_events(&c, from))
            };
            let mut truth = s.clone();
            super::super::tests::resolve_waiting(&mut truth, 1, 60);
            let expected = super::super::tests::public_events(&truth, from);
            let p = export_at(&s, v, ExportOptionsV1::default()).expect("P exports");
            assert_eq!(
                continue_from(p.state).as_ref(),
                Ok(&expected),
                "P's continuation matches the true state"
            );
            let fails = match export_at(
                &s,
                v,
                ExportOptionsV1 {
                    mutant: Some(MutantV1::L10),
                },
            ) {
                Err(_) => true,
                Ok(x) => continue_from(x.state).as_ref() != Ok(&expected),
            };
            l10_fails += u64::from(fails);
        }
        eprintln!("L10: fails at {l10_fails} of {cases} shuffle-trigger decisions");
        assert!(cases > 0, "the fixture reaches the waiting shuffle trigger");
        assert_eq!(l10_fails, cases);
    }

    #[test]
    fn realizability_on_engine_games() {
        let mut checked = 0;
        let mut failures = Vec::new();
        for (decks, seed) in [
            (["CawGates", "Faeries"], 11u64),
            (["Wildfire", "Burn"], 12),
            (["Faeries", "Terror"], 33),
        ] {
            let g = super::super::harness::record_game(decks, seed, 300);
            let game = NativeGameV1 {
                start: g.start.clone(),
                actions: g.native.clone(),
                edits: Vec::new(),
            };
            for viewer in [PlayerId::P0, PlayerId::P1] {
                let mut s = g.start.clone();
                let mut k = 0;
                for (i, a) in g.native.iter().enumerate() {
                    let FastActorResponseV1::Decision(d) = s.current_response() else {
                        break;
                    };
                    if seat(d.acting_player) == viewer && d.legal_action_count >= 2 {
                        k += 1;
                        if k % 6 == 0 {
                            match realize_at(&game, i, viewer, None) {
                                Ok(Ok(_)) => checked += 1,
                                Ok(Err(e)) => {
                                    failures.push(format!("{decks:?} {viewer:?} step {i}: {e}"))
                                }
                                Err(_) => {}
                            }
                        }
                    }
                    let sem = s.diagnostic_current_action_semantics().unwrap_or_default();
                    let index = sem
                        .iter()
                        .position(|x| serde_json::to_value(x).ok().as_ref() == Some(a))
                        .expect("replay");
                    s.step(d.episode_id, d.step, index as u32)
                        .expect("replay");
                }
            }
        }
        eprintln!("realizability on engine games: {checked} realized, failures {failures:#?}");
        assert!(checked > 0);
        assert!(failures.is_empty());
    }
}
