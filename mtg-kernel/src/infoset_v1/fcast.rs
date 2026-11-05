//! F-cast (lead ruling 7 (b), amendment 43.4): which of two revealed
//! same-name copies the opponent casts. The family runs on a scripted
//! fixture, where the copies' spans start turns apart (so the rule before
//! 43.4 names the remaining copy differently in the two worlds), and over
//! every AC1 engine-family game.

use super::fixtures::{
    cast, finish, force_deal, gate, id, kind, new_game, pick, play_land, restart, IntentV1,
    ScriptV1, LEAKAGE_V1,
};
use super::harness::{
    byte_window, f_cast, local_menu, record_game, seat, tracked_hand_casts, GameV1, PairReportV1,
    ENGINE_FAMILY_GAMES,
};
use super::tests::export_at;
use super::*;
use crate::ids::{ObjectId, PlayerId};
use crate::rl_session::FastActorResponseV1;
use crate::state::GameState;
use std::collections::BTreeMap;

/// Cast a copy of `name` that V does not track (by the projection of V's
/// transcript; a script's own choice, never the producer's).
pub(crate) fn cast_untracked(name: &str, viewer: PlayerId) -> IntentV1 {
    let d = u64::from(id(name));
    IntentV1::TakeIn(
        format!("cast an untracked {name}"),
        Box::new(move |s, v| {
            kind(v) == "cast_spell"
                && v["source"]["card_db_id"].as_u64() == Some(d)
                && v["source"]["arena_id"].as_u64().is_some_and(|a| {
                    let st = s.game_state();
                    !super::projection::project_v1(st, st.observation_journal.notes(), viewer)
                        .tracked(ObjectId(a as u32))
                })
        }),
    )
}

/// The F-cast fixture (CawGates versus Faeries, off-board). V is Faeries
/// (P1). CawGates holds two Squadron Hawks it drew (untracked). Turn 2 it
/// casts one and fetches a single Hawk (A, revealed); turn 3 it casts the
/// other drawn Hawk and fetches the last library Hawk (B, revealed), so a
/// land and the turn-3 Hawk start their spans between A's reveal and B's.
/// Turn 4 it casts A or B (F-cast varies which): the bot-facing transcript
/// names the opponent's hand cards by name only, so the remaining copy is
/// the same to V either way, while its span start is not.
pub(crate) fn revealed_copies(seed: u64) -> ScriptV1 {
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
            "Island",
            "Squadron Hawk",
            "Squadron Hawk",
            "Brainstorm",
        ],
        &["Sacred Cat", "Thraben Charm", "Spell Pierce"],
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
        &["Island", "Island", "Island"],
    );
    let hawk = id("Squadron Hawk");
    let mut script = ScriptV1::new(restart(&live, st));
    script.push(
        o,
        vec![
            play_land("Azorius Guildgate"),
            play_land("Island"),
            cast_untracked("Squadron Hawk", v),
            pick("Squadron Hawk"),
            finish(),
            play_land("Island"),
            cast_untracked("Squadron Hawk", v),
            pick("Squadron Hawk"),
            gate("turn 4 with two Hawks in hand", move |st: &GameState| {
                st.turn >= 4
                    && st.players[0]
                        .hand
                        .iter()
                        .filter(|x| st.objects.get(**x).card_def == hawk)
                        .count()
                        == 2
            }),
            cast("Squadron Hawk"),
            play_land("Island"),
        ],
    );
    script.keep(o, &["Island", "Squadron Hawk", "Azorius Guildgate"]);
    script.push(
        v,
        vec![
            play_land("Island"),
            play_land("Island"),
            play_land("Island"),
        ],
    );
    script
}

fn sum_witnesses(totals: &mut BTreeMap<String, u64>, r: &PairReportV1) {
    for (m, n) in &r.witnesses {
        *totals.entry(m.clone()).or_default() += n;
    }
}

/// The first fixture branch replayed to V's next decision: P's bytes agree,
/// L14's (the rule before 43.4) do not. Returns the evidence line.
fn fixture_bytes(game: &GameV1, v: PlayerId) -> String {
    let mut s = game.start.clone();
    for (i, chosen) in game.actions.iter().enumerate() {
        let FastActorResponseV1::Decision(d) = s.current_response() else {
            break;
        };
        let a = local_menu(&s)
            .iter()
            .position(|x| x == chosen)
            .expect("replay");
        let groups = tracked_hand_casts(&s, v);
        if seat(d.acting_player) == v.opponent() && !groups.is_empty() {
            let g = &groups[0].1;
            let x = if g.contains(&a) { a } else { g[0] };
            let y = *g.iter().find(|&&b| b != x).expect("two copies");
            let (mut w, mut w2) = (s.clone(), s.clone());
            w.step(d.episode_id, d.step, x as u32).expect("W");
            w2.step(d.episode_id, d.step, y as u32).expect("W'");
            for later in &game.actions[i + 1..] {
                let (FastActorResponseV1::Decision(e), FastActorResponseV1::Decision(f)) =
                    (w.current_response(), w2.current_response())
                else {
                    break;
                };
                if seat(e.acting_player) == v && e.legal_action_count >= 2 {
                    // Amendment 43.6: while the cast copy's receipt names
                    // its hand epoch, P refuses `same_name_hand` in both
                    // worlds; the 43.4 witness is the first V decision at
                    // which both export.
                    let (p, p2) = (
                        export_at(&w, v, ExportOptionsV1::default()),
                        export_at(&w2, v, ExportOptionsV1::default()),
                    );
                    let (p, p2) = match (p, p2) {
                        (Ok(p), Ok(p2)) => (p, p2),
                        (Err(a), Err(b)) => {
                            assert_eq!(
                                format!("{a:?}"),
                                format!("{b:?}"),
                                "P agrees while it refuses"
                            );
                            let (Some(b), Some(c)) = (
                                local_menu(&w).iter().position(|r| r == later),
                                local_menu(&w2).iter().position(|r| r == later),
                            ) else {
                                panic!("the worlds part before V decides");
                            };
                            w.step(e.episode_id, e.step, b as u32).expect("W");
                            w2.step(f.episode_id, f.step, c as u32).expect("W'");
                            continue;
                        }
                        (a, b) => panic!(
                            "P presence differs: {:?} / {:?}",
                            a.as_ref().err(),
                            b.as_ref().err()
                        ),
                    };
                    assert_eq!(p.bytes, p2.bytes, "P agrees at the first V decision");
                    let o = ExportOptionsV1 {
                        mutant: Some(MutantV1::L14),
                    };
                    let m = export_at(&w, v, o).expect("L14 exports");
                    let m2 = export_at(&w2, v, o).expect("L14 exports");
                    assert_ne!(m.bytes, m2.bytes, "L14 differs at the first V decision");
                    return format!(
                        "branch at decision {i}: P equal ({} bytes, sha256 {}); L14 (before 43.4) {}",
                        p.bytes.len(),
                        &p.sha256[..16],
                        byte_window(&m.bytes, &m2.bytes)
                    );
                }
                let (Some(b), Some(c)) = (
                    local_menu(&w).iter().position(|r| r == later),
                    local_menu(&w2).iter().position(|r| r == later),
                ) else {
                    panic!("the worlds part before V decides");
                };
                w.step(e.episode_id, e.step, b as u32).expect("W");
                w2.step(f.episode_id, f.step, c as u32).expect("W'");
            }
            panic!("no V decision after the branch");
        }
        s.step(d.episode_id, d.step, a as u32).expect("replay");
    }
    panic!("the fixture never offers two revealed Hawks to cast");
}

/// Lead ruling 7 (b): the F-cast variant. On its fixture and over every AC1
/// engine-family game, both viewers, P must agree across the worlds (former
/// view here; the strict checker reads the `fcast-*` dumps). On the fixture,
/// mutant L14 (the rule before 43.4) must differ: that is the pair that
/// differed before the fix, and its bytes are printed.
#[test]
fn ac1_f_cast_revealed_copies() {
    let mut mutants = LEAKAGE_V1.to_vec();
    mutants.push(MutantV1::L14);
    let v = PlayerId::P1;
    let mut script = revealed_copies(81);
    script.run(400, 40, &mut |_, _, _| {});
    let game = GameV1 {
        start: script.start.clone(),
        actions: script.actions.clone(),
        native: script.native.clone(),
    };
    let (reports, tried) = f_cast(&game, v, &mutants, 4, 30, "fcast-fx-revealed-hawks");
    let compared: u64 = reports.iter().map(|r| r.compared).sum();
    let mut fixture_witnesses = BTreeMap::new();
    for r in &reports {
        sum_witnesses(&mut fixture_witnesses, r);
    }
    eprintln!(
        "F-cast fixture: pairs {tried}, compared {compared}, P differences {}, witnesses {fixture_witnesses:?}",
        reports.iter().map(|r| r.p_differs).sum::<u64>()
    );
    assert!(tried >= 1, "the fixture casts one of two revealed Hawks");
    assert!(
        compared > 0,
        "the fixture pairs compare decisions: {reports:?}"
    );
    assert!(
        reports.iter().all(|r| r.p_differs == 0),
        "P differs across the cast copies: {reports:?}"
    );
    assert!(
        fixture_witnesses.get("L14").copied().unwrap_or(0) > 0,
        "the rule before 43.4 differs on the fixture: {reports:?}"
    );
    eprintln!("F-cast fixture bytes: {}", fixture_bytes(&game, v));
    let (mut pairs, mut compared, mut differs, mut truncated) = (0u64, 0u64, 0u64, 0u64);
    let mut witnesses: BTreeMap<String, u64> = BTreeMap::new();
    for (decks, seed) in ENGINE_FAMILY_GAMES {
        let game = record_game(decks, seed, 300);
        for viewer in [PlayerId::P0, PlayerId::P1] {
            let tag = format!("fcast-{}-{}-{seed}-p{}", decks[0], decks[1], viewer.0);
            let (reports, n) = f_cast(&game, viewer, &mutants, 6, 40, &tag);
            pairs += n;
            for r in &reports {
                compared += r.compared;
                differs += r.p_differs;
                truncated += u64::from(r.truncated);
                sum_witnesses(&mut witnesses, r);
            }
        }
    }
    eprintln!(
        "F-cast over the AC1 engine-family games, former view: pairs {pairs}, compared {compared}, P differences {differs}, truncated pairs {truncated}, witnesses {witnesses:?}"
    );
    assert_eq!(differs, 0, "P differs across the cast copies");
}
