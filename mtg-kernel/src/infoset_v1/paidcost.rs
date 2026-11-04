//! Lead ruling 8, amendment 43.5 (design 6.9): a paid-cost reference to an
//! object that has since left V's tracking becomes a tombstone when no
//! pending resolution reads it; otherwise R1 stays.
//!
//! The fixture is the smoke8 Lembas pattern from V's side: V (Wildfire)
//! sacrifices its own Lembas as a cost, Lembas's shuffle trigger resolves
//! first, and the ability waits while the Lembas is in V's library, out of
//! V's tracking. The two worlds force different library positions for the
//! shuffled Lembas, which V cannot see.

use super::fixtures::{
    activate, cast, compare_records, edit, force_deal, gate, id, new_game, pick, play_land,
    record_strict, restart, ScriptV1,
};
use super::tests::{decklists, export_at, first_diff, public_events, resolve_waiting};
use super::*;
use crate::ids::{ObjectId, PlayerId};
use crate::rl_session::FastActorSessionV1;
use crate::state::{GameState, Zone};

/// The sacrifice outlet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OutletV1 {
    /// Lembas's own Food ability ({2}, {T}, sacrifice Lembas: gain 3 life).
    /// Its source contract names the Lembas, so 6.3 pins it.
    Food,
    /// Krark-Clan Shaman (sacrifice an artifact: 1 damage to each creature
    /// without flying). Only the paid-cost record names the Lembas: the
    /// amendment 43.5 case.
    Shaman,
}

impl OutletV1 {
    fn name(self) -> &'static str {
        match self {
            Self::Food => "Lembas",
            Self::Shaman => "Krark-Clan Shaman",
        }
    }

    /// The strict-view dump tag (the fixture glob matches `*-v<n>`).
    fn tag(self) -> &'static str {
        match self {
            Self::Food => "food",
            Self::Shaman => "shaman",
        }
    }
}

fn lembas_in_library(st: &GameState) -> bool {
    let lembas = id("Lembas");
    st.players[0]
        .library
        .iter()
        .any(|x| st.objects.get(*x).card_def == lembas)
}

/// The waiting ability of `outlet` (by source definition).
fn waiting(st: &GameState, outlet: OutletV1) -> Option<usize> {
    let def = id(outlet.name());
    st.stack
        .iter()
        .position(|i| st.objects.get(i.source).card_def == def && !i.v4.paid_cost_refs.is_empty())
}

/// The Lembas paid-cost fixture. V is Wildfire (P0) against Faeries. On
/// turn 3 V activates `outlet`, sacrificing its Lembas; the shuffle trigger
/// resolves while the ability waits; then the shuffle's outcome is forced:
/// the Lembas goes to library position 1 (`deep` false) or to the bottom
/// (`deep` true).
pub(crate) fn lembas_paid(outlet: OutletV1, deep: bool) -> ScriptV1 {
    let (v, o) = (PlayerId::P0, PlayerId::P1);
    let live = new_game(["Wildfire", "Faeries"], 76, v);
    let mut st = live.game_state().clone();
    force_deal(
        &mut st,
        v,
        &[
            "Mountain",
            "Swamp",
            "Forest",
            "Lembas",
            "Krark-Clan Shaman",
            "Cast Down",
            "Cast Down",
        ],
        &["Swamp", "Cast Down", "Forest", "Cast Down", "Swamp"],
    );
    force_deal(
        &mut st,
        o,
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
    let mut intents = vec![
        play_land("Mountain"),
        play_land("Swamp"),
        cast("Lembas"),
        play_land("Forest"),
    ];
    match outlet {
        OutletV1::Food => {
            // Three lands: {2} for the ability.
            intents.push(activate("Lembas"));
        }
        OutletV1::Shaman => {
            intents.push(cast("Krark-Clan Shaman"));
            intents.push(activate("Krark-Clan Shaman"));
            intents.push(pick("Lembas"));
        }
    }
    intents.push(gate(
        "the shuffle trigger resolved",
        move |st: &GameState| waiting(st, outlet).is_some() && lembas_in_library(st),
    ));
    intents.push(edit(
        "force the shuffle outcome",
        move |st: &mut GameState| {
            let at = st.players[0]
                .library
                .iter()
                .position(|x| st.objects.get(*x).card_def == lembas)
                .expect("the Lembas is in the library");
            let lib = &mut st.players[0].library;
            let to = if deep { lib.len() - 1 } else { 1 };
            lib.swap(to, at);
        },
    ));
    script.push(v, intents);
    script.keep(
        v,
        &["Mountain", "Swamp", "Forest", "Lembas", "Krark-Clan Shaman"],
    );
    script.push(
        o,
        vec![
            play_land("Island"),
            play_land("Island"),
            play_land("Island"),
        ],
    );
    script
}

/// Runs the fixture to V's first decision with the ability waiting and the
/// Lembas in the library (after the forced shuffle outcome).
fn at_the_wait(outlet: OutletV1, deep: bool) -> FastActorSessionV1 {
    let v = PlayerId::P0;
    let mut script = lembas_paid(outlet, deep);
    let mut found: Option<FastActorSessionV1> = None;
    script.run(400, 6, &mut |s, p, _| {
        let st = s.game_state();
        if found.is_none()
            && p == v
            && waiting(st, outlet).is_some()
            && lembas_in_library(st)
            && script_edited(st, deep)
        {
            found = Some(s.clone());
        }
    });
    found.unwrap_or_else(|| {
        panic!(
            "{outlet:?}: no V decision with the ability waiting and the Lembas in the library:\n{}",
            script.trace.join("\n")
        )
    })
}

/// Whether the forced shuffle outcome is in place.
fn script_edited(st: &GameState, deep: bool) -> bool {
    let lembas = id("Lembas");
    let lib = &st.players[0].library;
    let at = if deep { lib.len() - 1 } else { 1 };
    lib.get(at)
        .is_some_and(|x| st.objects.get(*x).card_def == lembas)
}

/// The export of `state` for V with the session's journal and marks.
fn export_state(s: &FastActorSessionV1, state: &GameState) -> Result<InfosetV1, ExportErrorV1> {
    let v = PlayerId::P0;
    let lists = decklists(s);
    export_v1(
        &ExportInputV1 {
            state,
            journal: state.observation_journal.notes(),
            viewer: v,
            decklists: [&lists[0], &lists[1]],
            marks: Some(&s.infoset_marks_v1(v)),
        },
        ExportOptionsV1::default(),
    )
}

/// Amendment 43.5, first branch: no pending resolution reads the paid
/// Lembas (Krark-Clan Shaman's damage), so the waiting ability's paid-cost
/// record names a tombstone: Lembas's name, owner, generation 0, in no
/// zone; the record keeps its own zone (the graveyard it was paid to).
/// The exported state resolves the ability to the true public outcome.
#[test]
fn paid_cost_unread_becomes_a_tombstone() {
    let s = at_the_wait(OutletV1::Shaman, false);
    let st = s.game_state();
    let i = waiting(st, OutletV1::Shaman).expect("the ability waits");
    let paid = st.stack[i].v4.paid_cost_refs[0];
    assert_eq!(paid.card_def, id("Lembas"));
    assert_eq!(st.objects.get(paid.object).zone, Zone::Library);
    let x = export_at(&s, PlayerId::P0, ExportOptionsV1::default())
        .unwrap_or_else(|e| panic!("43.5 exports the waiting Shaman ability: {e:?}"));
    let item = x
        .state
        .stack
        .iter()
        .find(|it| !it.v4.paid_cost_refs.is_empty())
        .expect("the exported ability");
    let r = item.v4.paid_cost_refs[0];
    match &x.handles[r.object.0 as usize] {
        HandleV1::H { named: true, .. } => {}
        h => panic!("the paid-cost record names {h:?}, not a named tombstone"),
    }
    let t = x.state.objects.get(r.object);
    assert_eq!(
        (t.card_def, t.owner, t.zone, t.zone_change_count),
        (id("Lembas"), PlayerId::P0, Zone::Command, 0)
    );
    assert_eq!((r.zone, r.zone_change_count), (Zone::Graveyard, 0));
    // The tombstone is not the Lembas's library occupant: no library slot
    // names it.
    assert!(!x.state.players[0].library.contains(&r.object));
    // AC3: the exported state resolves the ability as the true state does.
    let mut truth = s.clone();
    let mut canon = FastActorSessionV1::import_infoset_v1(&s, x.state.clone(), 0);
    let from = st.engine.event_history.len();
    let below = i + 1;
    resolve_waiting(&mut truth, below, 20);
    resolve_waiting(&mut canon, below, 20);
    assert!(
        truth.game_state().stack.len() < below && canon.game_state().stack.len() < below,
        "the ability resolves in both"
    );
    assert!(
        canon.game_state().engine.halted.is_none(),
        "no engine fault"
    );
    let (et, ec) = (public_events(&truth, from), public_events(&canon, from));
    assert!(
        et == ec,
        "the Shaman continuation differs at {}",
        first_diff(&et, &ec, String::new())
    );
}

/// Amendment 43.5, second branch: when a pending resolution reads the paid
/// object (its characteristics, or its last-known power), the reference
/// keeps R1. State-only edits of the same decision.
#[test]
fn paid_cost_read_by_a_resolution_stays_r1() {
    use crate::effect::{EffectOp, PlayerRef};
    let s = at_the_wait(OutletV1::Shaman, false);
    let i = waiting(s.game_state(), OutletV1::Shaman).expect("the ability waits");
    let r1 = |st: &GameState, what: &str| match export_state(&s, st) {
        Err(e) => {
            let d = format!("{e:?}");
            assert!(
                d.contains("R1") && d.contains("a live reference names an untracked object"),
                "{what}: {d}"
            );
        }
        Ok(_) => panic!("{what}: exported, expected R1"),
    };
    // The control: unedited, it exports.
    assert!(export_state(&s, s.game_state()).is_ok());
    // A resolution that reads the paid card's mana value.
    let mut st = s.game_state().clone();
    st.stack[i].inline_effect = Some(EffectOp::GainLifeEqualToPaidCostManaValue {
        player: PlayerRef::Controller,
    });
    r1(&st, "reads the paid card's characteristics");
    // Captured last-known power.
    let mut st = s.game_state().clone();
    st.stack[i].v4.paid_cost_refs[0].power_lki = Some(0);
    r1(&st, "reads last-known power");
    // Occurrences: every live occurrence of the record must sit in a
    // qualifying stack item. A copy in a pending resolution's context
    // removes it from the unread set (fail closed); a copy in the event
    // log (history, not a resolution) does not.
    let st = s.game_state();
    let key = (
        paid_object(st, i),
        st.stack[i].v4.paid_cost_refs[0].zone_change_count,
    );
    let tagged = super::rewrite::tagged_value(st);
    let record = tagged["stack"][i]["v4"]["paid_cost_refs"][0].clone();
    assert!(super::export::unread_paid_costs_v1(st, &tagged, PlayerId::P0).contains(&key));
    let mut pending = tagged.clone();
    pending["engine"]["w1_test_pending"] =
        serde_json::json!({"ctx": {"paid_cost_refs": [record.clone()]}});
    assert!(!super::export::unread_paid_costs_v1(st, &pending, PlayerId::P0).contains(&key));
    let mut logged = tagged.clone();
    logged["engine"]["event_log"] = serde_json::json!([{"paid_cost_refs": [record]}]);
    assert!(super::export::unread_paid_costs_v1(st, &logged, PlayerId::P0).contains(&key));
    // A payment V did not see: no tombstone.
    let mut hidden = st.clone();
    hidden.stack[i].v4.paid_cost_refs[0].visible_to_mask = 0b10;
    let hidden_tagged = super::rewrite::tagged_value(&hidden);
    assert!(
        !super::export::unread_paid_costs_v1(&hidden, &hidden_tagged, PlayerId::P0).contains(&key)
    );
}

fn paid_object(st: &GameState, i: usize) -> ObjectId {
    st.stack[i].v4.paid_cost_refs[0].object
}

/// The Lembas fixture in both worlds (lead ruling 8): the shuffled Lembas
/// at library position 1 or at the bottom. P agrees at every compared V
/// decision. With Krark-Clan Shaman the waiting decisions export through
/// 43.5's tombstone; with Lembas's own Food ability the contract pins the
/// Lembas (6.3) and they export through the pin.
#[test]
fn lembas_paid_cost_fixture_in_both_worlds() {
    let v = PlayerId::P0;
    for outlet in [OutletV1::Shaman, OutletV1::Food] {
        let runs: Vec<_> = [false, true]
            .iter()
            .map(|&deep| {
                record_strict(
                    lembas_paid(outlet, deep),
                    v,
                    6,
                    &format!("fx-lembaspaid-{}-v{}", outlet.tag(), u8::from(deep)),
                )
            })
            .collect();
        let report = compare_records(&runs[0].0, &runs[1].0);
        eprintln!("lembas paid cost, {outlet:?}, position 1 vs bottom: {report:?}");
        assert_eq!(report.p_differs, 0, "{outlet:?}: P differs: {report:?}");
        // The waiting decisions, in both worlds, export (none refuses).
        for deep in [false, true] {
            let s = at_the_wait(outlet, deep);
            let x = export_at(&s, v, ExportOptionsV1::default()).unwrap_or_else(|e| {
                panic!("{outlet:?} deep {deep}: the waiting decision refuses: {e:?}")
            });
            let item = x
                .state
                .stack
                .iter()
                .find(|it| !it.v4.paid_cost_refs.is_empty())
                .expect("the exported ability");
            let h = &x.handles[item.v4.paid_cost_refs[0].object.0 as usize];
            match (outlet, h) {
                (OutletV1::Shaman, HandleV1::H { named: true, .. })
                | (OutletV1::Food, HandleV1::U { pinned: true, .. }) => {}
                _ => panic!("{outlet:?} deep {deep}: the paid-cost record names {h:?}"),
            }
        }
        assert!(
            report.compared > 0,
            "{outlet:?}: the pair compares decisions: {report:?}"
        );
    }
}

/// Sol round 1: with the Shaman ability waiting, an extra draw gives V a
/// Lembas, the paid copy A or another copy B. V's records agree, so P must
/// agree: R1b refuses in both (a later Lembas of V's may be the paid copy),
/// before tracking decides whether the reference is a tombstone.
#[test]
fn drawing_the_paid_lembas_or_its_twin_exports_alike() {
    use crate::event::{propose_and_commit, ProposedEvent};
    let s = at_the_wait(OutletV1::Shaman, false);
    let base = s.game_state().clone();
    let i = waiting(&base, OutletV1::Shaman).expect("the ability waits");
    let a = paid_object(&base, i);
    let lembas = id("Lembas");
    let b = base.players[0]
        .library
        .iter()
        .copied()
        .find(|x| *x != a && base.objects.get(*x).card_def == lembas)
        .expect("a second Lembas in V's library");
    let run = |top: ObjectId| {
        let mut st = base.clone();
        let at = st.players[0]
            .library
            .iter()
            .position(|x| *x == top)
            .expect("in the library");
        st.players[0].library.swap(0, at);
        propose_and_commit(&mut st, ProposedEvent::draw(PlayerId::P0));
        assert!(st.players[0].hand.contains(&top), "V drew the chosen copy");
        export_state(&s, &st)
            .map(|x| x.bytes)
            .map_err(|e| e.code_name().to_string())
    };
    let (pa, pb) = (run(a), run(b));
    assert_eq!(pa, pb, "drawing the paid Lembas or its twin exports alike");
    assert_eq!(pa, Err("R1b".to_string()), "R1b refuses in both worlds");
}

/// The opponent's Shaman payment for Sol round 2. V is Faeries (P1).
/// Wildfire casts two Lembas and Krark-Clan Shaman and sacrifices one
/// Lembas to the Shaman; the session stops at V's first decision with the
/// Shaman ability and the Lembas's shuffle trigger waiting.
fn opponent_payment() -> FastActorSessionV1 {
    let (o, v) = (PlayerId::P0, PlayerId::P1);
    let live = new_game(["Wildfire", "Faeries"], 77, o);
    let mut st = live.game_state().clone();
    force_deal(
        &mut st,
        o,
        &[
            "Mountain",
            "Swamp",
            "Forest",
            "Lembas",
            "Lembas",
            "Krark-Clan Shaman",
            "Swamp",
        ],
        &["Cast Down", "Swamp", "Cast Down", "Forest", "Cast Down"],
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
            play_land("Swamp"),
            cast("Lembas"),
            play_land("Forest"),
            cast("Krark-Clan Shaman"),
            cast("Lembas"),
            activate("Krark-Clan Shaman"),
            pick("Lembas"),
        ],
    );
    script.keep(
        o,
        &["Mountain", "Swamp", "Forest", "Lembas", "Krark-Clan Shaman"],
    );
    script.push(
        v,
        vec![
            play_land("Island"),
            play_land("Island"),
            play_land("Island"),
        ],
    );
    let lembas = id("Lembas");
    let mut found: Option<FastActorSessionV1> = None;
    script.run(400, 4, &mut |s, p, _| {
        let st = s.game_state();
        if found.is_none()
            && p == v
            && waiting(st, OutletV1::Shaman).is_some()
            && st
                .stack
                .iter()
                .any(|i| st.objects.get(i.source).card_def == lembas)
        {
            found = Some(s.clone());
        }
    });
    found.unwrap_or_else(|| {
        panic!(
            "no V decision with the Shaman ability and the shuffle trigger waiting:
{}",
            script.trace.join(
                "
"
            )
        )
    })
}

/// Sol round 2: the opponent's paid Lembas A (its shuffle trigger removed,
/// so A stays in the graveyard) and a twin B (sacrificed by state surgery)
/// return from the graveyard to the opponent's hand together, in one batch
/// of V's transcript; then A or B goes to the library. V's records agree
/// whichever copy went, so P must agree: R1 in both (`same_name_batch`,
/// read before tracking decides the tombstone), never an export in the
/// world where A went to the library (untracked, 43.5-eligible).
#[test]
fn a_paid_copy_and_its_twin_leaving_in_one_batch_refuse_alike() {
    use crate::event::{propose_and_commit, ProposedEvent};
    let (o, v) = (PlayerId::P0, PlayerId::P1);
    let s = opponent_payment();
    let lembas = id("Lembas");
    let mut base = s.game_state().clone();
    let i = waiting(&base, OutletV1::Shaman).expect("the Shaman ability waits");
    let a = paid_object(&base, i);
    assert_eq!(base.objects.get(a).zone, Zone::Graveyard);
    let b = base.players[o.index()]
        .battlefield
        .iter()
        .copied()
        .find(|x| base.objects.get(*x).card_def == lembas)
        .expect("the other Lembas is on the battlefield");
    base.stack
        .retain(|it| base.objects.get(it.source).card_def != lembas);
    propose_and_commit(&mut base, ProposedEvent::zone_change(b, Zone::Graveyard));
    base.engine.pending_triggers.clear();
    propose_and_commit(&mut base, ProposedEvent::zone_change(a, Zone::Hand));
    propose_and_commit(&mut base, ProposedEvent::zone_change(b, Zone::Hand));
    let lists = decklists(&s);
    let marks = s.infoset_marks_v1(v);
    let run = |gone: ObjectId| {
        let mut st = base.clone();
        propose_and_commit(&mut st, ProposedEvent::zone_change(gone, Zone::Library));
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
    };
    let (pa, pb) = (run(a), run(b));
    eprintln!(
        "paid copy and twin, one batch: A went {:?} / B went {:?}",
        pa.as_ref().map(Vec::len),
        pb.as_ref().map(Vec::len)
    );
    assert!(
        pa.as_ref().is_err_and(|e| e.contains("R1"))
            && pb.as_ref().is_err_and(|e| e.contains("R1")),
        "both worlds refuse R1"
    );
    assert_eq!(pa, pb, "P agrees whichever copy went");
}
