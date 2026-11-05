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
    byte_window, f_cast, known_hand_casts, local_menu, record_game, seat, GameV1, PairReportV1,
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
/// casts one and fetches nothing; turn 3 it casts the other drawn Hawk and
/// fetches the two library Hawks (A and B, revealed). Turn 4 it casts A or
/// B (F-cast varies which): the bot-facing transcript names the opponent's
/// hand cards by name only, so the remaining copy is the same to V either
/// way, while its physical identity is not.
///
/// Amendment 43.7 re-derivation: before it, A was fetched on turn 2 and B
/// on turn 3, so their span starts differed (the rule before 43.4 named the
/// remaining copy differently). Under 43.7 an identified departure of a
/// name ends one of V's facts of that name whichever copy left, so casting
/// the second drawn Hawk after A's reveal would end A's fact and leave one
/// known Hawk: both revealed copies must arrive after the last unknown Hawk
/// leaves. Mutant L14 (bound copies tracked) still tells A from B.
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
            finish(),
            play_land("Island"),
            cast_untracked("Squadron Hawk", v),
            pick("Squadron Hawk"),
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
    // Amendment 43.7 re-derivation: an identified departure ends one of
    // V's facts of the name whichever copy left, so after the cast both
    // worlds hold the same facts and L14 (bound copies tracked) binds the
    // remaining copy alike; it departs from P (P binds no hand copy) at
    // every compared decision, the same way in both worlds. Before 43.7 the
    // witness was L14 differing across the worlds.
    let departs: u64 = reports
        .iter()
        .map(|r| r.departs.get("L14").copied().unwrap_or(0))
        .sum();
    eprintln!("F-cast fixture: L14 departs from P at {departs} compared decisions");
    assert!(
        departs > 0,
        "L14 binds the revealed copies where P does not: {reports:?}"
    );
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
