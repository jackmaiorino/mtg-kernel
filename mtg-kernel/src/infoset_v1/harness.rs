//! AC1 to AC3 harnesses on engine games (design v7, section 13).
//!
//! Worlds are coupled: W' starts from W's state with whole hidden objects
//! permuted (F-deal at the deal, F-slot at a decision), replays W's actions
//! by semantic match and shares W's random stream. A decision is compared
//! only while V's views (V4 observation and menu) are byte-equal; at the
//! first difference the pair is truncated.

use super::tests::export_at;
use super::*;
use crate::ids::{ObjectId, PlayerId};
use crate::rl::PlayerSeatV1;
use crate::rl_session::{FastActorResponseV1, FastActorSessionV1};
use crate::state::{GameState, ObjectStateV4, SplitMix64, Zone};
use std::collections::BTreeMap;

pub(crate) fn seat(p: PlayerSeatV1) -> PlayerId {
    match p {
        PlayerSeatV1::P0 => PlayerId::P0,
        PlayerSeatV1::P1 => PlayerId::P1,
    }
}

/// Viewer-local ids: every `arena_id` becomes its order of first
/// appearance in the value, and hashes over native ids are dropped, so two
/// worlds that differ only in provenance give equal views (the producer's
/// projection keeps native ids inside the environment the same way).
fn localize(value: &mut serde_json::Value, ids: &mut BTreeMap<u64, u64>) {
    match value {
        serde_json::Value::Object(m) => {
            m.retain(|k, _| !k.contains("hash"));
            for (k, v) in m.iter_mut() {
                if k == "arena_id" {
                    if let Some(n) = v.as_u64() {
                        let next = ids.len() as u64;
                        *v = serde_json::Value::from(*ids.entry(n).or_insert(next));
                        continue;
                    }
                }
                localize(v, ids);
            }
        }
        serde_json::Value::Array(xs) => xs.iter_mut().for_each(|x| localize(x, ids)),
        _ => {}
    }
}

/// V's view at the current decision: the V4 observation and menu, with
/// viewer-local ids.
pub(crate) fn view(s: &FastActorSessionV1) -> Option<Vec<u8>> {
    let FastActorResponseV1::Decision(d) = s.current_response() else {
        return None;
    };
    let (obs, menu) = s.actor_visible_decision_v4(d).ok()?;
    let mut v = serde_json::to_value(&(obs, menu)).expect("view serializes");
    localize(&mut v, &mut BTreeMap::new());
    Some(serde_json::to_vec(&v).expect("view serializes"))
}

/// The semantic form of each candidate at the current decision, with
/// decision-local ids.
fn menu(s: &FastActorSessionV1) -> Vec<Vec<u8>> {
    let mut ids = BTreeMap::new();
    s.diagnostic_current_action_semantics()
        .unwrap_or_default()
        .iter()
        .map(|x| {
            let mut v = serde_json::to_value(x).expect("semantics serialize");
            localize(&mut v, &mut ids);
            serde_json::to_vec(&v).expect("semantics serialize")
        })
        .collect()
}

/// A recorded game: the reset session, chosen candidates by semantics.
pub(crate) struct GameV1 {
    pub start: FastActorSessionV1,
    pub actions: Vec<Vec<u8>>,
}

pub(crate) fn record_game(decks: [&str; 2], seed: u64, max_decisions: usize) -> GameV1 {
    let live = FastActorSessionV1::reset_with_decks_and_limits_flat_action_v3(
        seed,
        seed,
        2_000,
        20_000,
        [decks[0].to_string(), decks[1].to_string()],
    )
    .expect("reset");
    let start = FastActorSessionV1::import_infoset_v1(&live, live.game_state().clone(), 0);
    let mut s = start.clone();
    let mut rng = SplitMix64::seed(seed ^ 0x51ed);
    let mut actions = Vec::new();
    for _ in 0..max_decisions {
        let FastActorResponseV1::Decision(d) = s.current_response() else {
            break;
        };
        let a = (rng.next_u64() % u64::from(d.legal_action_count)) as u32;
        actions.push(menu(&s)[a as usize].clone());
        s.step(d.episode_id, d.step, a).expect("legal step");
    }
    GameV1 { start, actions }
}

/// Swaps the definitions of two pristine hidden objects (whole objects:
/// definitions travel with their state, which is pristine by I1).
pub(crate) fn swap_objects(state: &mut GameState, a: ObjectId, b: ObjectId) {
    let (da, na) = {
        let o = state.objects.get(a);
        (o.card_def, o.name.clone())
    };
    let (db, nb) = {
        let o = state.objects.get(b);
        (o.card_def, o.name.clone())
    };
    let oa = state.objects.get_mut(a);
    oa.card_def = db;
    oa.name = nb;
    oa.v4 = ObjectStateV4::from_card_def(db);
    let ob = state.objects.get_mut(b);
    ob.card_def = da;
    ob.name = na;
    ob.v4 = ObjectStateV4::from_card_def(da);
}

/// Exchanges the slots (positions and zones) of two hidden objects of one
/// owner; definitions and incarnations travel with the objects.
pub(crate) fn move_objects(state: &mut GameState, a: ObjectId, b: ObjectId) {
    let owner = state.objects.get(a).owner;
    let p = &mut state.players[owner.index()];
    for v in [&mut p.hand, &mut p.library] {
        for x in v.iter_mut() {
            if *x == a {
                *x = b;
            } else if *x == b {
                *x = a;
            }
        }
    }
    let (za, zb) = (state.objects.get(a).zone, state.objects.get(b).zone);
    state.objects.get_mut(a).zone = zb;
    state.objects.get_mut(b).zone = za;
}

/// One family pair's comparison.
#[derive(Clone, Debug, Default, serde::Serialize)]
pub(crate) struct PairReportV1 {
    pub compared: u64,
    pub truncated: bool,
    /// Correct producer: decisions whose payload bytes or presence differ.
    pub p_differs: u64,
    pub p_refusals: BTreeMap<String, u64>,
    /// Mutant witnesses: compared decisions where P agrees and M differs.
    pub witnesses: BTreeMap<String, u64>,
}

fn outcome(r: &Result<InfosetV1, ExportErrorV1>) -> Result<Vec<u8>, String> {
    match r {
        Ok(x) => Ok(x.bytes.clone()),
        Err(e) => Err(e.code_name().to_string()),
    }
}

/// Plays W and W' in lockstep from `w` and `w2` (equal up to the family's
/// permutation), comparing every viewer decision while views agree.
pub(crate) fn compare_worlds(
    w: FastActorSessionV1,
    w2: FastActorSessionV1,
    actions: &[Vec<u8>],
    viewer: PlayerId,
    mutants: &[MutantV1],
) -> PairReportV1 {
    compare_worlds_with(w, w2, actions, viewer, mutants, false)
}

/// As [`compare_worlds`]; with `index_fallback`, W' takes W's candidate
/// index when no candidate matches semantically (F-prov: the worlds differ
/// only in native ids, which the semantic form carries).
pub(crate) fn compare_worlds_with(
    w: FastActorSessionV1,
    w2: FastActorSessionV1,
    actions: &[Vec<u8>],
    viewer: PlayerId,
    mutants: &[MutantV1],
    index_fallback: bool,
) -> PairReportV1 {
    let mut report = PairReportV1::default();
    let (mut s, mut t) = (w, w2);
    for chosen in actions {
        let (FastActorResponseV1::Decision(d), FastActorResponseV1::Decision(e)) =
            (s.current_response(), t.current_response())
        else {
            break;
        };
        if seat(d.acting_player) == viewer && d.legal_action_count >= 2 {
            if view(&s) != view(&t) {
                report.truncated = true;
                break;
            }
            let p = outcome(&export_at(&s, viewer, ExportOptionsV1::default()));
            let p2 = outcome(&export_at(&t, viewer, ExportOptionsV1::default()));
            report.compared += 1;
            if let Err(code) = &p {
                *report.p_refusals.entry(code.clone()).or_default() += 1;
            }
            if p != p2 {
                report.p_differs += 1;
            } else {
                for m in mutants {
                    let o = ExportOptionsV1 { mutant: Some(*m) };
                    if outcome(&export_at(&s, viewer, o)) != outcome(&export_at(&t, viewer, o)) {
                        *report.witnesses.entry(format!("{m:?}")).or_default() += 1;
                    }
                }
            }
        }
        let a = menu(&s).iter().position(|x| x == chosen);
        let b = menu(&t).iter().position(|x| x == chosen).or(
            if index_fallback && d.legal_action_count == e.legal_action_count {
                a
            } else {
                None
            },
        );
        let (Some(a), Some(b)) = (a, b) else {
            report.truncated = true;
            break;
        };
        if s.step(d.episode_id, d.step, a as u32).is_err()
            || t.step(e.episode_id, e.step, b as u32).is_err()
        {
            report.truncated = true;
            break;
        }
    }
    report
}

/// Objects V never tracked during `game` (whole prefix), per owner.
pub(crate) fn never_seen(game: &GameV1, viewer: PlayerId) -> Vec<ObjectId> {
    let mut s = game.start.clone();
    for chosen in &game.actions {
        let FastActorResponseV1::Decision(d) = s.current_response() else {
            break;
        };
        let Some(a) = menu(&s).iter().position(|x| x == chosen) else {
            break;
        };
        s.step(d.episode_id, d.step, a as u32).expect("replay");
    }
    let state = s.game_state();
    let p = super::projection::project_v1(state, state.observation_journal.notes(), viewer);
    state
        .objects
        .iter()
        .filter(|(id, o)| {
            !o.v4.is_token
                && o.spell_copy_origin.is_none()
                && p.objects[id.0 as usize].spans.is_empty()
                && matches!(o.zone, Zone::Library | Zone::Hand)
        })
        .map(|(id, _)| id)
        .collect()
}

/// F-deal: swap the definitions of two opponent cards V never sees.
pub(crate) fn f_deal(
    game: &GameV1,
    viewer: PlayerId,
    mutants: &[MutantV1],
) -> Option<PairReportV1> {
    let opp = viewer.opponent();
    let hidden = never_seen(game, viewer);
    let start = game.start.game_state();
    let pick: Vec<ObjectId> = hidden
        .iter()
        .copied()
        .filter(|id| start.objects.get(*id).owner == opp)
        .collect();
    let a = *pick.first()?;
    let b = *pick
        .iter()
        .find(|id| start.objects.get(**id).card_def != start.objects.get(a).card_def)?;
    let mut st = start.clone();
    swap_objects(&mut st, a, b);
    let w2 = FastActorSessionV1::import_infoset_v1(&game.start, st, 0);
    Some(compare_worlds(
        game.start.clone(),
        w2,
        &game.actions,
        viewer,
        mutants,
    ))
}

/// F-prov: swap the deal slots (library positions at the deal) of two
/// same-name opponent cards V never sees before both are hidden. Only their
/// provenance (native ids) differs.
pub(crate) fn f_prov(
    game: &GameV1,
    viewer: PlayerId,
    mutants: &[MutantV1],
) -> Option<PairReportV1> {
    let opp = viewer.opponent();
    let start = game.start.game_state();
    let library = &start.players[opp.index()].library;
    let (i, j) = (0..library.len()).find_map(|i| {
        (i + 1..library.len())
            .find(|&j| {
                start.objects.get(library[i]).card_def == start.objects.get(library[j]).card_def
            })
            .map(|j| (i, j))
    })?;
    let mut st = start.clone();
    st.players[opp.index()].library.swap(i, j);
    let w2 = FastActorSessionV1::import_infoset_v1(&game.start, st, 0);
    Some(compare_worlds_with(
        game.start.clone(),
        w2,
        &game.actions,
        viewer,
        mutants,
        true,
    ))
}

/// F-slot: at decision `at`, swap two of V's unknown slots of one owner.
pub(crate) fn f_slot(
    game: &GameV1,
    viewer: PlayerId,
    at: usize,
    mutants: &[MutantV1],
) -> Option<PairReportV1> {
    let mut s = game.start.clone();
    for chosen in game.actions.iter().take(at) {
        let FastActorResponseV1::Decision(d) = s.current_response() else {
            return None;
        };
        let a = menu(&s).iter().position(|x| x == chosen)?;
        s.step(d.episode_id, d.step, a as u32).ok()?;
    }
    let state = s.game_state();
    let p = super::projection::project_v1(state, state.observation_journal.notes(), viewer);
    let opp = viewer.opponent();
    let pinned = super::export::contract_pins(state);
    let unknown: Vec<ObjectId> = state.players[opp.index()]
        .hand
        .iter()
        .chain(&state.players[opp.index()].library)
        .copied()
        .filter(|id| !p.tracked(*id) && !pinned.contains(id))
        .collect();
    let a = *unknown.first()?;
    let b = *unknown
        .iter()
        .find(|id| state.objects.get(**id).card_def != state.objects.get(a).card_def)?;
    let mut st = state.clone();
    // Whole objects change places: each keeps its definition and incarnation
    // and takes the other's slot (and zone).
    move_objects(&mut st, a, b);
    let w2 = FastActorSessionV1::import_infoset_v1(&s, st, 0);
    // Rebuild W the same way so both sessions share construction.
    let w = FastActorSessionV1::import_infoset_v1(&s, s.game_state().clone(), 0);
    Some(compare_worlds(w, w2, &game.actions[at..], viewer, mutants))
}

/// AC3: the imported state's menu is in bijection with the live menu and a
/// random rollout from it runs to the end with zero engine faults.
pub(crate) fn ac3_check(
    live: &FastActorSessionV1,
    export: &InfosetV1,
    seed: u64,
) -> Result<(), String> {
    let imported = FastActorSessionV1::import_infoset_v1(live, export.state.clone(), 0);
    let (FastActorResponseV1::Decision(d), FastActorResponseV1::Decision(e)) =
        (live.current_response(), imported.current_response())
    else {
        return Err(format!(
            "the imported root is not a decision: {:?}",
            imported.current_response()
        ));
    };
    if d.legal_action_count != e.legal_action_count || d.acting_player != e.acting_player {
        return Err(format!(
            "menu size {} vs {}",
            d.legal_action_count, e.legal_action_count
        ));
    }
    if menu(live).len() != menu(&imported).len() {
        return Err("menu length".into());
    }
    let mut s = imported;
    let mut rng = SplitMix64::seed(seed);
    for _ in 0..3_000 {
        let FastActorResponseV1::Decision(d) = s.current_response() else {
            return Ok(());
        };
        let a = (rng.next_u64() % u64::from(d.legal_action_count)) as u32;
        s.step(d.episode_id, d.step, a)
            .map_err(|e| format!("rollout fault: {e:?}"))?;
    }
    Ok(())
}

#[test]
fn ac1_f_deal_and_f_slot_on_engine_games() {
    let mutants = [MutantV1::L1, MutantV1::L2, MutantV1::L3, MutantV1::L7];
    let mut totals: BTreeMap<String, u64> = BTreeMap::new();
    let mut compared = 0;
    let mut truncated = 0;
    for (decks, seed) in [
        (["CawGates", "Faeries"], 11u64),
        (["Wildfire", "Burn"], 12),
        (["Spy", "Elves"], 13),
    ] {
        let game = record_game(decks, seed, 300);
        for viewer in [PlayerId::P0, PlayerId::P1] {
            let mut reports = Vec::new();
            reports.extend(f_deal(&game, viewer, &mutants));
            reports.extend(f_prov(&game, viewer, &mutants));
            for at in [20, 60, 120] {
                reports.extend(f_slot(&game, viewer, at, &mutants));
            }
            for r in reports {
                assert_eq!(
                    r.p_differs, 0,
                    "the correct producer differs across worlds: {r:?}"
                );
                compared += r.compared;
                truncated += u64::from(r.truncated);
                for (m, n) in r.witnesses {
                    *totals.entry(m).or_default() += n;
                }
            }
        }
    }
    eprintln!(
        "AC1 engine-game families: compared {compared}, truncated pairs {truncated}, witnesses {totals:?}"
    );
    assert!(compared > 0);
}

#[test]
fn ac3_menu_and_rollout_smoke_from_exported_states() {
    let mut checked = 0;
    super::tests::play(["CawGates", "Faeries"], 21, 300, |s, v| {
        if let Ok(x) = export_at(s, v, ExportOptionsV1::default()) {
            ac3_check(s, &x, 7).expect("AC3");
            checked += 1;
        }
    });
    assert!(checked > 0, "no exported decision to check");
}
