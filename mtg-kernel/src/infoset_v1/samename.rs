//! Lead ruling 7 (a): a live reference across an unlinked visible move keeps
//! its link (6.5) only while V can tell which copy moved. Two same-name
//! contract sources that leave in one batch of V's transcript refuse R1
//! (`same_name_batch`), in both worlds.

use super::fixtures::{
    cast, force_deal, id, kind, new_game, pick, play_land, record_strict, restart, AtV1, IntentV1,
    ScriptV1,
};
use super::harness::PairReportV1;
use super::tests::export_at;
use super::*;
use crate::ids::PlayerId;
use crate::rl_session::FastActorSessionV1;
use crate::state::GameState;

/// Activate the `nth` copy of `name` (by arena id, ascending) among the
/// copies whose ability the menu offers.
fn activate_copy(name: &str, nth: usize) -> IntentV1 {
    let d = u64::from(id(name));
    let ours = move |v: &serde_json::Value| {
        kind(v) == "activate_ability" && v["source"]["card_db_id"].as_u64() == Some(d)
    };
    IntentV1::TakeIn(
        format!("activate {name} copy {nth}"),
        Box::new(move |s: &FastActorSessionV1, v| {
            if !ours(v) {
                return false;
            }
            let mut copies: Vec<u64> = s
                .diagnostic_current_action_semantics()
                .unwrap_or_default()
                .iter()
                .map(|x| serde_json::to_value(x).expect("semantics serialize"))
                .filter(|x| ours(x))
                .filter_map(|x| x["source"]["arena_id"].as_u64())
                .collect();
            copies.sort_unstable();
            copies.dedup();
            copies.get(nth).copied() == v["source"]["arena_id"].as_u64()
        }),
    )
}

const SHAMAN: &str = "Krark-Clan Shaman";

/// Waiting Shaman abilities.
fn shaman_items(st: &GameState) -> usize {
    let shaman = id(SHAMAN);
    st.stack
        .iter()
        .filter(|i| {
            i.kind == crate::state::StackItemKind::ActivatedAbility
                && i.v4.ability_source_contract.is_some()
                && st.objects.get(i.source).card_def == shaman
        })
        .count()
}

/// The same-name batch fixture (Wildfire versus Faeries, off-board). V is
/// Faeries (P1). Wildfire casts two Krark-Clan Shamans and two Ichor
/// Wellsprings, then activates Shaman copy `first`, sacrificing a
/// Wellspring. The test stops at V's first decision with that ability
/// waiting and makes both Shamans leave the battlefield in one batch (state
/// surgery standing for a sweeper's simultaneous deaths). Scripted sequences
/// could not form that batch: the activating player does not keep priority,
/// and the Shaman's own ability resolved before a second activation.
pub(crate) fn same_name_batch(first: usize) -> ScriptV1 {
    let (o, v) = (PlayerId::P0, PlayerId::P1);
    let live = new_game(["Wildfire", "Faeries"], 75, o);
    let mut st = live.game_state().clone();
    force_deal(
        &mut st,
        o,
        &[
            "Mountain",
            "Krark-Clan Shaman",
            "Mountain",
            "Krark-Clan Shaman",
            "Swamp",
            "Ichor Wellspring",
            "Ichor Wellspring",
        ],
        &["Swamp", "Cast Down", "Cast Down", "Forest", "Cast Down"],
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
    let mut script = ScriptV1::new(restart(&live, st));
    script.push(
        o,
        vec![
            play_land("Mountain"),
            cast(SHAMAN),
            play_land("Mountain"),
            cast(SHAMAN),
            play_land("Swamp"),
            cast("Ichor Wellspring"),
            play_land("Swamp"),
            cast("Ichor Wellspring"),
            activate_copy(SHAMAN, first),
            pick("Ichor Wellspring"),
        ],
    );
    script.keep(o, &["Mountain", "Swamp", SHAMAN, "Ichor Wellspring"]);
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

/// V's first decision with Shaman copy `first`'s ability waiting.
fn at_the_wait(first: usize) -> FastActorSessionV1 {
    let v = PlayerId::P1;
    let mut script = same_name_batch(first);
    let mut found: Option<FastActorSessionV1> = None;
    script.run(400, 4, &mut |s, p, _| {
        if found.is_none() && p == v && shaman_items(s.game_state()) == 1 {
            found = Some(s.clone());
        }
    });
    found.unwrap_or_else(|| {
        panic!(
            "no V decision with the Shaman ability waiting:
{}",
            script.trace.join(
                "
"
            )
        )
    })
}

/// P at that decision after the Shamans leave the battlefield (both, or
/// only the source), in one batch of V's transcript.
fn p_after(s: &FastActorSessionV1, both: bool) -> Result<Vec<u8>, String> {
    use crate::event::{propose_and_commit, ProposedEvent};
    let v = PlayerId::P1;
    let shaman = id(SHAMAN);
    let mut st = s.game_state().clone();
    let source = st
        .stack
        .iter()
        .find(|i| {
            i.kind == crate::state::StackItemKind::ActivatedAbility
                && st.objects.get(i.source).card_def == shaman
        })
        .map(|i| i.source)
        .expect("the waiting ability");
    let mut leave: Vec<crate::ids::ObjectId> = st.players[0]
        .battlefield
        .iter()
        .copied()
        .filter(|x| st.objects.get(*x).card_def == shaman && (both || *x == source))
        .collect();
    leave.sort_unstable();
    assert_eq!(
        leave.len(),
        if both { 2 } else { 1 },
        "the Shamans that leave"
    );
    for x in leave {
        propose_and_commit(
            &mut st,
            ProposedEvent::zone_change(x, crate::state::Zone::Graveyard),
        );
    }
    st.engine.pending_triggers.clear();
    let lists = super::tests::decklists(s);
    let marks = s.infoset_marks_v1(v);
    export_v1(
        &ExportInputV1 {
            state: &st,
            journal: st.observation_journal.notes(),
            viewer: v,
            decklists: [&lists[0], &lists[1]],
            marks: Some(&marks),
        },
        ExportOptionsV1::default(),
    )
    .map(|x| x.bytes)
    .map_err(|e| format!("{e:?}"))
}

/// Lead ruling 7 (a), fixture witness. Whichever Shaman activated, P
/// agrees across the worlds at every compared scripted V decision; at the
/// wait, the ability's single source keeps 6.5's link (exports); when both
/// Shamans leave in one batch, P refuses R1 `same_name_batch` in both
/// worlds, with equal outcomes; when only the source leaves, it exports.
#[test]
fn same_name_batch_refuses_in_both_worlds() {
    let v = PlayerId::P1;
    let runs: Vec<(Vec<AtV1>, ScriptV1)> = [0usize, 1]
        .iter()
        .map(|&first| {
            record_strict(
                same_name_batch(first),
                v,
                4,
                &format!("fx-samename-v{first}"),
            )
        })
        .collect();
    let report: PairReportV1 = super::fixtures::compare_records(&runs[0].0, &runs[1].0);
    eprintln!("same-name batch, first copy 0 vs 1: {report:?}");
    assert_eq!(
        report.p_differs, 0,
        "P differs across the worlds: {report:?}"
    );
    assert!(
        report.compared > 0,
        "the pair compares decisions: {report:?}"
    );
    let waits = [at_the_wait(0), at_the_wait(1)];
    for s in &waits {
        let st = s.game_state();
        let on_battlefield = st.players[0]
            .battlefield
            .iter()
            .filter(|x| st.objects.get(**x).card_def == id(SHAMAN))
            .count();
        assert_eq!(
            on_battlefield, 2,
            "both Shamans are on the battlefield at the wait"
        );
        assert!(
            export_at(s, v, ExportOptionsV1::default()).is_ok(),
            "a single waiting source exports"
        );
        assert!(
            p_after(s, false).is_ok(),
            "the source leaving alone exports"
        );
    }
    let (pa, pb) = (p_after(&waits[0], true), p_after(&waits[1], true));
    eprintln!(
        "both Shamans leave: {:?} / {:?}",
        pa.as_ref().map(Vec::len),
        pb.as_ref().map(Vec::len)
    );
    let batch = |p: &Result<Vec<u8>, String>| {
        p.as_ref().is_err_and(|e| {
            e.contains("R1") && e.contains(super::export::R1_SAME_NAME_BATCH_DETAIL)
        })
    };
    assert!(
        batch(&pa) && batch(&pb),
        "R1 same_name_batch in both worlds"
    );
    assert_eq!(pa, pb, "P agrees across the worlds");
}

/// Lead ruling 11 (R1, Fable Q1): the waiting ability's source and its twin
/// leave the battlefield in one batch of V's transcript, one into the
/// graveyard (an amendment 43.3 split) and one into the library unseen (a
/// lapse). V's records agree whichever copy went where (the graveyard move
/// comes first in both worlds). Returns P for V after the moves.
fn p_mixed(s: &FastActorSessionV1, source_to_library: bool) -> Result<Vec<u8>, String> {
    let v = PlayerId::P1;
    let mut st = s.game_state().clone();
    mixed_batch(&mut st, source_to_library);
    let lists = super::tests::decklists(s);
    let marks = s.infoset_marks_v1(v);
    export_v1(
        &ExportInputV1 {
            state: &st,
            journal: st.observation_journal.notes(),
            viewer: v,
            decklists: [&lists[0], &lists[1]],
            marks: Some(&marks),
        },
        ExportOptionsV1::default(),
    )
    .map(|x| x.bytes)
    .map_err(|e| format!("{e:?}"))
}

/// The mixed batch: the waiting Shaman ability's source and the other
/// Shaman leave the battlefield, the first named one to the graveyard, then
/// the other to the bottom of the library (hidden from V).
fn mixed_batch(st: &mut GameState, source_to_library: bool) {
    use crate::event::{propose_and_commit, ProposedEvent};
    let shaman = id(SHAMAN);
    let source = st
        .stack
        .iter()
        .find(|i| {
            i.kind == crate::state::StackItemKind::ActivatedAbility
                && st.objects.get(i.source).card_def == shaman
        })
        .map(|i| i.source)
        .expect("the waiting ability");
    let twin = st.players[0]
        .battlefield
        .iter()
        .copied()
        .find(|x| *x != source && st.objects.get(*x).card_def == shaman)
        .expect("the other Shaman is on the battlefield");
    let (to_graveyard, to_library) = if source_to_library {
        (twin, source)
    } else {
        (source, twin)
    };
    propose_and_commit(
        st,
        ProposedEvent::zone_change(to_graveyard, crate::state::Zone::Graveyard),
    );
    propose_and_commit(st, ProposedEvent::private_bottom_library_insert(to_library));
    st.engine.pending_triggers.clear();
}

/// Lead ruling 11 (R1): in both orders of a mixed lapse/split batch, P
/// refuses R1 `same_name_batch` with equal outcomes. Before the repair the
/// batch test skipped the referenced copy's own lapse, so the world where
/// the source lapsed exported (the source is pinned as a contract source)
/// while the world where it split refused.
#[test]
fn mixed_lapse_split_batch_refuses_in_both_orders() {
    let s = at_the_wait(0);
    let (split, lapse) = (p_mixed(&s, false), p_mixed(&s, true));
    eprintln!(
        "mixed batch: source split {:?} / source lapsed {:?}",
        split.as_ref().map(Vec::len),
        lapse.as_ref().map(Vec::len)
    );
    let batch = |p: &Result<Vec<u8>, String>| {
        p.as_ref().is_err_and(|e| {
            e.contains("R1") && e.contains(super::export::R1_SAME_NAME_BATCH_DETAIL)
        })
    };
    assert!(
        batch(&split) && batch(&lapse),
        "R1 same_name_batch in both orders: {split:?} / {lapse:?}"
    );
    assert_eq!(split, lapse, "P agrees whichever copy lapsed");
}

/// The mixed-batch AC1 fixture (lead ruling 11, R1): the same-name script
/// with Shaman copy 0 activated; at V's first decision with the ability
/// waiting, an edit makes the source and its twin leave in one batch, one
/// split and one lapsed (world 0: the source splits; world 1: it lapses).
/// The rest of the game is scripted as before.
pub(crate) fn mixed_batch_fixture(source_to_library: bool) -> ScriptV1 {
    let v = PlayerId::P1;
    let mut script = same_name_batch(0);
    // After V's land plays: V needs mana for a decision while the ability
    // waits (the activating player does not keep priority).
    let intents = &mut script.intents[v.index()];
    intents.push_back(super::fixtures::gate(
        "the Shaman ability waits",
        |st: &GameState| shaman_items(st) == 1,
    ));
    intents.push_back(super::fixtures::edit(
        "the source and its twin leave in one batch, one split, one lapsed",
        move |st: &mut GameState| mixed_batch(st, source_to_library),
    ));
    script
}

/// AC1 over the mixed-batch fixture: P agrees across the worlds at every
/// compared V decision, and the decisions while the ability waits refuse R1
/// `same_name_batch` in both worlds. Strict-view dumps
/// `fx-mixedbatch-v0/v1`.
#[test]
fn mixed_batch_fixture_in_both_worlds() {
    let v = PlayerId::P1;
    let runs: Vec<(Vec<AtV1>, ScriptV1)> = [false, true]
        .iter()
        .map(|&lapse| {
            record_strict(
                mixed_batch_fixture(lapse),
                v,
                4,
                &format!("fx-mixedbatch-v{}", u8::from(lapse)),
            )
        })
        .collect();
    let report: PairReportV1 = super::fixtures::compare_records(&runs[0].0, &runs[1].0);
    eprintln!("mixed batch fixture, source split vs lapsed: {report:?}");
    for (lapse, (_, script)) in [false, true].iter().zip(&runs) {
        assert!(
            script.trace.iter().any(|t| t.starts_with("edit ")),
            "world {lapse}: the edit ran"
        );
    }
    assert_eq!(
        report.p_differs, 0,
        "P differs across the worlds: {report:?}"
    );
    assert!(
        report.compared > 0,
        "the pair compares decisions: {report:?}"
    );
    assert!(
        report
            .p_refusals
            .get("R1:same_name_batch")
            .copied()
            .unwrap_or(0)
            > 0,
        "the waiting decisions refuse R1 same_name_batch in both worlds: {report:?}"
    );
}
