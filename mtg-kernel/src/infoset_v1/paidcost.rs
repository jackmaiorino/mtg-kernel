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
            // The only artifact: without neutral choices the engine pays
            // a single-candidate sacrifice itself.
            if super::harness::ac1_neutral() {
                intents.push(pick("Lembas"));
            }
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
    // Public returns to the hand: V's native rows know both copies, as the
    // engine's return effects record them.
    for x in [a, b] {
        propose_and_commit(&mut base, ProposedEvent::zone_change(x, Zone::Hand));
        base.reveal_hand_card(v, o, x)
            .expect("reveal the returned copy");
    }
    let lists = decklists(&s);
    let marks = s.infoset_marks_v1(v);
    let run = |gone: ObjectId| {
        let mut st = base.clone();
        // A private put-back from the hand (Brainstorm's path): every
        // nonowner hand fact is invalidated, then the card goes on top.
        st.clear_nonowner_hand_knowledge(o);
        propose_and_commit(&mut st, ProposedEvent::private_top_library_insert(gone));
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
    // Amendment 43.6 reads first: the two equal-key copies sat in the hand
    // when one was put back, so `same_name_hand` refuses before
    // `same_name_batch` would.
    let refused = |p: &Result<Vec<u8>, String>| {
        p.as_ref().is_err_and(|e| {
            e.contains("R1")
                && (e.contains(super::export::R1_SAME_NAME_HAND_DETAIL)
                    || e.contains(super::export::R1_SAME_NAME_BATCH_DETAIL))
        })
    };
    assert!(
        refused(&pa) && refused(&pb),
        "both worlds refuse R1 (same_name_hand or same_name_batch)"
    );
    assert_eq!(pa, pb, "P agrees whichever copy went");
}

/// Amendment 43.6 (lead ruling 9 (b)), Sol round 3's pair. The opponent's
/// Shaman payment waits with its paid Lembas A in the graveyard (A's
/// shuffle trigger removed). A returns to the opponent's hand, then (after
/// a V decision, a separate batch) a second Lembas `second` does; one of
/// them is discarded and shuffled away. Returns P for V.
fn hand_world(second: HandSecondV1, discard_a: bool) -> Result<Vec<u8>, String> {
    hand_world_with(second, discard_a, HandRefV1::Paid)
}

/// The live reference the controls keep to A.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HandRefV1 {
    /// The Shaman's paid-cost record, made before A entered the hand.
    Paid,
    /// No reference survives (the Shaman ability removed): equal-key
    /// copies alone do not refuse.
    None,
    /// The discarded copy stays in the graveyard and the record is re-made
    /// fresh at its graveyard epoch, after it left the hand.
    Fresh,
}

fn hand_world_with(
    second: HandSecondV1,
    discard_a: bool,
    reference: HandRefV1,
) -> Result<Vec<u8>, String> {
    use crate::event::{propose_and_commit, ProposedEvent};
    let (o, v) = (PlayerId::P0, PlayerId::P1);
    let s = opponent_payment();
    let lembas = id("Lembas");
    let mut st = s.game_state().clone();
    let i = waiting(&st, OutletV1::Shaman).expect("the Shaman ability waits");
    let a = paid_object(&st, i);
    st.stack
        .retain(|it| st.objects.get(it.source).card_def != lembas);
    let b = match second {
        HandSecondV1::FromGraveyard => {
            let b = st.players[o.index()]
                .battlefield
                .iter()
                .copied()
                .find(|x| st.objects.get(*x).card_def == lembas)
                .expect("the other Lembas is on the battlefield");
            propose_and_commit(&mut st, ProposedEvent::zone_change(b, Zone::Graveyard));
            st.engine.pending_triggers.clear();
            b
        }
        HandSecondV1::RevealedFromLibrary => st.players[o.index()]
            .library
            .iter()
            .copied()
            .find(|x| st.objects.get(*x).card_def == lembas)
            .expect("a Lembas in the opponent's library"),
    };
    let mut marks = s.infoset_marks_v1(v);
    propose_and_commit(&mut st, ProposedEvent::zone_change(a, Zone::Hand));
    st.reveal_hand_card(v, o, a).expect("reveal A");
    // A V decision between the two returns: separate batches.
    marks.push((
        st.engine.event_history.len(),
        st.observation_journal.notes().len(),
    ));
    propose_and_commit(&mut st, ProposedEvent::zone_change(b, Zone::Hand));
    st.reveal_hand_card(v, o, b).expect("reveal B");
    marks.push((
        st.engine.event_history.len(),
        st.observation_journal.notes().len(),
    ));
    let gone = if discard_a { a } else { b };
    propose_and_commit(&mut st, ProposedEvent::zone_change(gone, Zone::Graveyard));
    st.engine.pending_triggers.clear();
    match reference {
        HandRefV1::Paid => {
            propose_and_commit(&mut st, ProposedEvent::zone_change(gone, Zone::Library));
        }
        HandRefV1::None => {
            propose_and_commit(&mut st, ProposedEvent::zone_change(gone, Zone::Library));
            st.stack.retain(|it| it.v4.paid_cost_refs.is_empty());
        }
        HandRefV1::Fresh => {
            let zcc = st.objects.get(a).zone_change_count;
            let i = waiting(&st, OutletV1::Shaman).expect("the Shaman ability waits");
            st.stack[i].v4.paid_cost_refs[0].zone_change_count = zcc;
        }
    }
    let lists = decklists(&s);
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

/// How the second Lembas reaches the opponent's hand.
#[derive(Clone, Copy, Debug)]
enum HandSecondV1 {
    /// From the graveyard, like A: equal 43.4 keys (a public-zone fact).
    FromGraveyard,
    /// Drawn unseen, then revealed: a different fact, different keys.
    RevealedFromLibrary,
}

/// Amendment 43.6: equal-key copies, either one discarded. Both orders
/// refuse R1 `same_name_hand`, with equal outcomes.
#[test]
fn same_name_hand_refuses_in_both_orders() {
    let (pa, pb) = (
        hand_world(HandSecondV1::FromGraveyard, true),
        hand_world(HandSecondV1::FromGraveyard, false),
    );
    eprintln!(
        "same-name hand: A discarded {:?} / B discarded {:?}",
        pa.as_ref().map(Vec::len),
        pb.as_ref().map(Vec::len)
    );
    let hand = |p: &Result<Vec<u8>, String>| {
        p.as_ref()
            .is_err_and(|e| e.contains("R1") && e.contains(super::export::R1_SAME_NAME_HAND_DETAIL))
    };
    assert!(hand(&pa) && hand(&pb), "R1 same_name_hand in both orders");
    assert_eq!(pa, pb, "P agrees whichever copy left");
}

/// Amendment 43.6's negative control: the copies' 43.4 keys differ (one
/// returned from the graveyard, one revealed after an unseen draw), so
/// either discard still exports.
#[test]
fn different_hand_keys_still_export() {
    for discard_a in [true, false] {
        let p = hand_world(HandSecondV1::RevealedFromLibrary, discard_a);
        assert!(
            p.is_ok(),
            "discard {}: different keys export: {:?}",
            if discard_a { "A" } else { "the revealed copy" },
            p.as_ref().err()
        );
    }
}

/// Sol review of 43.6, round 1 controls: equal-key copies with no surviving
/// historical reference export in both orders (current zone membership is
/// no reference across the hand), and a reference made fresh at the
/// discarded copy's graveyard epoch exports.
#[test]
fn same_name_hand_controls_export() {
    for discard_a in [true, false] {
        let p = hand_world_with(HandSecondV1::FromGraveyard, discard_a, HandRefV1::None);
        assert!(
            p.is_ok(),
            "no reference, discard A {discard_a}: {:?}",
            p.as_ref().err()
        );
    }
    let p = hand_world_with(HandSecondV1::FromGraveyard, true, HandRefV1::Fresh);
    assert!(
        p.is_ok(),
        "a fresh graveyard reference: {:?}",
        p.as_ref().err()
    );
}

/// Lead ruling 11 (R2, Astra item 1): a hand with one unbound name-level
/// fact and one bound copy of the same name. Three copies of a card of the
/// opponent return from the graveyard to its hand (bound facts), two cards
/// are put back unidentified (one unbound fact of that name remains, for
/// the third copy), then a fourth copy returns (bound). One copy of that
/// name is discarded: the unbound copy (`discard_bound` false) or the
/// fourth (true). No live reference remains. V's records agree whichever
/// copy went. Returns P for V.
fn mixed_hand_world(discard_bound: bool) -> Result<Vec<u8>, String> {
    mixed_hand_world_with(discard_bound, false)
}

/// As [`mixed_hand_world`]; with `again`, the discarded copy then returns
/// to the hand from the graveyard and the fourth copy is discarded (Sol
/// round 1: the mark must survive which fact that departure ends).
fn mixed_hand_world_with(discard_bound: bool, again: bool) -> Result<Vec<u8>, String> {
    use crate::event::{propose_and_commit, ProposedEvent};
    let (o, v) = (PlayerId::P0, PlayerId::P1);
    let s = opponent_payment();
    let lembas = id("Lembas");
    let mut st = s.game_state().clone();
    // No live reference: the Shaman ability and the shuffle trigger go.
    st.stack.retain(|it| {
        it.v4.paid_cost_refs.is_empty() && st.objects.get(it.source).card_def != lembas
    });
    st.engine.pending_triggers.clear();
    // The first name with four copies in the opponent's library.
    let lib = st.players[o.index()].library.clone();
    let mut defs: Vec<u16> = lib.iter().map(|x| st.objects.get(*x).card_def).collect();
    defs.sort_unstable();
    defs.dedup();
    let def = defs
        .into_iter()
        .find(|d| {
            lib.iter()
                .filter(|x| st.objects.get(**x).card_def == *d)
                .count()
                >= 4
        })
        .expect("a name with four copies in the opponent's library");
    let copies: Vec<ObjectId> = lib
        .iter()
        .copied()
        .filter(|x| st.objects.get(*x).card_def == def)
        .take(4)
        .collect();
    let mut marks = s.infoset_marks_v1(v);
    let mark = |st: &GameState, marks: &mut Vec<(usize, usize)>| {
        marks.push((
            st.engine.event_history.len(),
            st.observation_journal.notes().len(),
        ));
    };
    for &x in &copies {
        propose_and_commit(&mut st, ProposedEvent::zone_change(x, Zone::Graveyard));
    }
    st.engine.pending_triggers.clear();
    mark(&st, &mut marks);
    for &x in &copies[..3] {
        propose_and_commit(&mut st, ProposedEvent::zone_change(x, Zone::Hand));
        st.reveal_hand_card(v, o, x)
            .expect("reveal the returned copy");
    }
    mark(&st, &mut marks);
    // Two private put-backs (Brainstorm's path).
    for &x in &copies[..2] {
        st.clear_nonowner_hand_knowledge(o);
        propose_and_commit(&mut st, ProposedEvent::private_top_library_insert(x));
        mark(&st, &mut marks);
    }
    let fourth = copies[3];
    propose_and_commit(&mut st, ProposedEvent::zone_change(fourth, Zone::Hand));
    st.reveal_hand_card(v, o, fourth)
        .expect("reveal the fourth copy");
    mark(&st, &mut marks);
    let gone = if discard_bound { fourth } else { copies[2] };
    propose_and_commit(&mut st, ProposedEvent::zone_change(gone, Zone::Graveyard));
    st.engine.pending_triggers.clear();
    if again {
        mark(&st, &mut marks);
        propose_and_commit(&mut st, ProposedEvent::zone_change(gone, Zone::Hand));
        st.reveal_hand_card(v, o, gone)
            .expect("reveal the returned copy");
        mark(&st, &mut marks);
        propose_and_commit(&mut st, ProposedEvent::zone_change(fourth, Zone::Graveyard));
        st.engine.pending_triggers.clear();
    }
    let lists = decklists(&s);
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

/// Lead ruling 11 (R2): discarding the unbound copy or the bound fourth
/// copy gives equal P: R1 `same_name_hand` in both worlds. Prints the first
/// byte difference when the exports differ.
#[test]
fn mixed_bound_and_count_hand_facts_agree() {
    let (unbound, bound) = (mixed_hand_world(false), mixed_hand_world(true));
    eprintln!(
        "mixed hand: unbound copy discarded {:?} / bound copy discarded {:?}",
        unbound.as_ref().map(Vec::len),
        bound.as_ref().map(Vec::len)
    );
    if let (Ok(a), Ok(b)) = (&unbound, &bound) {
        if let (Ok(x), Ok(y)) = (
            serde_json::from_slice::<serde_json::Value>(a),
            serde_json::from_slice::<serde_json::Value>(b),
        ) {
            eprintln!(
                "first JSON difference: {}",
                first_diff(&x, &y, String::new())
            );
        }
        if let Some(at) = (0..a.len().min(b.len())).find(|&i| a[i] != b[i]) {
            let window = |x: &[u8]| {
                String::from_utf8_lossy(&x[at.saturating_sub(300)..(at + 300).min(x.len())])
                    .to_string()
            };
            eprintln!("first difference at byte {at}\n--- unbound discarded:\n{}\n--- bound discarded:\n{}", window(a), window(b));
        }
    }
    assert_eq!(unbound, bound, "P agrees whichever copy was discarded");
    // Before the repair both exported, 110,202 against 110,855 bytes: the
    // fourth copy survived as `C:16` beside 16 other tracked objects in one
    // world, and as a name-level `U:` occupant in the other.
    assert!(
        unbound.as_ref().is_err_and(|e| e.contains("R1")
            && e.contains(super::export::R1_SAME_NAME_HAND_DETAIL)),
        "R1 same_name_hand in both worlds: {unbound:?}"
    );
}

/// Sol round 1 on ruling 11 (R2), finding 2: after the mixed departure, the
/// discarded copy returns and the fourth copy is discarded again. One world
/// ends the marked bound fact, the other a fresh one; both must refuse.
#[test]
fn mixed_hand_mark_survives_a_second_departure() {
    let (a, b) = (
        mixed_hand_world_with(false, true),
        mixed_hand_world_with(true, true),
    );
    eprintln!(
        "mixed hand, second departure: {:?} / {:?}",
        a.as_ref().map(Vec::len),
        b.as_ref().map(Vec::len)
    );
    assert_eq!(a, b, "P agrees after the second departure");
    assert!(
        a.as_ref()
            .is_err_and(|e| e.contains(super::export::R1_SAME_NAME_HAND_DETAIL)),
        "R1 same_name_hand in both worlds: {a:?}"
    );
}

/// Sol round 1 on ruling 11 (R2), finding 3: a copy D returns from the
/// graveyard to the opponent's hand (a bound fact) and a same-name copy C
/// is drawn unseen (no fact); then D or C is discarded. The adapter's
/// tracker drops D's fact in both worlds (kernel_history_v2.py
/// `_hand_move`: own fact, else an unbound fact of the name, else any fact
/// of the name). Returns P for V.
fn bound_and_unknown_world(discard_bound: bool) -> Result<Vec<u8>, String> {
    use crate::event::{propose_and_commit, ProposedEvent};
    let (o, v) = (PlayerId::P0, PlayerId::P1);
    let s = opponent_payment();
    let lembas = id("Lembas");
    let mut st = s.game_state().clone();
    st.stack.retain(|it| {
        it.v4.paid_cost_refs.is_empty() && st.objects.get(it.source).card_def != lembas
    });
    st.engine.pending_triggers.clear();
    let lib = st.players[o.index()].library.clone();
    let mut defs: Vec<u16> = lib.iter().map(|x| st.objects.get(*x).card_def).collect();
    defs.sort_unstable();
    defs.dedup();
    let def = defs
        .into_iter()
        .find(|d| {
            lib.iter()
                .filter(|x| st.objects.get(**x).card_def == *d)
                .count()
                >= 2
        })
        .expect("a name with two copies in the opponent's library");
    let copies: Vec<ObjectId> = lib
        .iter()
        .copied()
        .filter(|x| st.objects.get(*x).card_def == def)
        .take(2)
        .collect();
    let (d, c) = (copies[0], copies[1]);
    let mut marks = s.infoset_marks_v1(v);
    let mark = |st: &GameState, marks: &mut Vec<(usize, usize)>| {
        marks.push((
            st.engine.event_history.len(),
            st.observation_journal.notes().len(),
        ));
    };
    propose_and_commit(&mut st, ProposedEvent::zone_change(d, Zone::Graveyard));
    st.engine.pending_triggers.clear();
    mark(&st, &mut marks);
    propose_and_commit(&mut st, ProposedEvent::zone_change(d, Zone::Hand));
    st.reveal_hand_card(v, o, d).expect("reveal D");
    mark(&st, &mut marks);
    // C on top, then an unseen draw.
    let lib = &mut st.players[o.index()].library;
    let at = lib.iter().position(|x| *x == c).expect("C in the library");
    let c_id = lib.remove(at);
    lib.insert(0, c_id);
    propose_and_commit(&mut st, ProposedEvent::draw(o));
    assert_eq!(st.objects.get(c).zone, Zone::Hand, "C was drawn");
    mark(&st, &mut marks);
    let gone = if discard_bound { d } else { c };
    propose_and_commit(&mut st, ProposedEvent::zone_change(gone, Zone::Graveyard));
    st.engine.pending_triggers.clear();
    let lists = decklists(&s);
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

/// Finding 3's two worlds: discard the bound copy D or the unseen copy C.
/// Before the repair both exported, 99,865 against 100,474 bytes (D kept as
/// `C:16` when C was discarded). The projection now follows the adapter: the
/// departure ends D's fact in both worlds, so P agrees.
#[test]
fn bound_and_unknown_hand_copies_agree() {
    let (unknown, bound) = (
        bound_and_unknown_world(false),
        bound_and_unknown_world(true),
    );
    eprintln!(
        "bound and unknown copies: unknown discarded {:?} / bound discarded {:?}",
        unknown.as_ref().map(Vec::len),
        bound.as_ref().map(Vec::len)
    );
    if let (Ok(a), Ok(b)) = (&unknown, &bound) {
        if let Some(at) = (0..a.len().min(b.len())).find(|&i| a[i] != b[i]) {
            let window = |x: &[u8]| {
                String::from_utf8_lossy(&x[at.saturating_sub(200)..(at + 200).min(x.len())])
                    .to_string()
            };
            eprintln!(
                "first difference at byte {at}\n--- unknown discarded:\n{}\n--- bound discarded:\n{}",
                window(a),
                window(b)
            );
        }
    }
    assert_eq!(unknown, bound, "P agrees whichever copy was discarded");
}

/// Sol round 2 on ruling 11, findings 1 and 2. The opponent's Shaman
/// ability waits with its paid-cost record. A Lembas D reaches the
/// opponent's hand from the graveyard (a bound fact) and a Lembas C is drawn
/// unseen (no fact); then D or C (`discard_d`) is discarded. With
/// `paid_is_d`, D is the paid Lembas and the record keeps its graveyard
/// epoch, from before D entered the hand (finding 2). Otherwise D comes from
/// the library and the record is re-made to name the discarded copy at its
/// hand epoch, as a cast receipt names its origin (finding 1). Returns P.
fn reference_world(paid_is_d: bool, discard_d: bool) -> Result<Vec<u8>, String> {
    reference_world_with(paid_is_d, discard_d, HandRefAtV1::Gone, false)
}

/// Which copy a re-made record names at its hand epoch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HandRefAtV1 {
    /// The discarded copy (a cast receipt's origin).
    Gone,
    /// D, whether it stayed or left (Sol round 3 finding 1).
    D,
}

/// As [`reference_world`]; `named_at` picks the copy the re-made record names,
/// and with `twin` a second Lembas B also returns to the hand from the
/// graveyard before C is drawn (equal 43.4 keys with D, Sol round 3
/// finding 2).
fn reference_world_with(
    paid_is_d: bool,
    discard_d: bool,
    named_at: HandRefAtV1,
    twin: bool,
) -> Result<Vec<u8>, String> {
    use crate::event::{propose_and_commit, ProposedEvent};
    let (o, v) = (PlayerId::P0, PlayerId::P1);
    let s = opponent_payment();
    let lembas = id("Lembas");
    let mut st = s.game_state().clone();
    let i = waiting(&st, OutletV1::Shaman).expect("the Shaman ability waits");
    let paid = paid_object(&st, i);
    // The shuffle trigger goes: the paid Lembas stays in the graveyard.
    st.stack
        .retain(|it| st.objects.get(it.source).card_def != lembas);
    st.engine.pending_triggers.clear();
    let in_library: Vec<ObjectId> = st.players[o.index()]
        .library
        .iter()
        .copied()
        .filter(|x| st.objects.get(*x).card_def == lembas)
        .collect();
    assert!(
        in_library.len() >= 2 + usize::from(twin),
        "enough Lembas in the opponent's library"
    );
    let (d, c) = if paid_is_d {
        (paid, in_library[0])
    } else {
        (in_library[0], in_library[1])
    };
    let mut marks = s.infoset_marks_v1(v);
    let mark = |st: &GameState, marks: &mut Vec<(usize, usize)>| {
        marks.push((
            st.engine.event_history.len(),
            st.observation_journal.notes().len(),
        ));
    };
    if !paid_is_d {
        propose_and_commit(&mut st, ProposedEvent::zone_change(d, Zone::Graveyard));
        st.engine.pending_triggers.clear();
        mark(&st, &mut marks);
    }
    propose_and_commit(&mut st, ProposedEvent::zone_change(d, Zone::Hand));
    st.reveal_hand_card(v, o, d).expect("reveal D");
    st.engine.pending_triggers.clear();
    mark(&st, &mut marks);
    if twin {
        let b = in_library[2];
        propose_and_commit(&mut st, ProposedEvent::zone_change(b, Zone::Graveyard));
        st.engine.pending_triggers.clear();
        mark(&st, &mut marks);
        propose_and_commit(&mut st, ProposedEvent::zone_change(b, Zone::Hand));
        st.reveal_hand_card(v, o, b).expect("reveal B");
        st.engine.pending_triggers.clear();
        mark(&st, &mut marks);
    }
    let lib = &mut st.players[o.index()].library;
    let at = lib.iter().position(|x| *x == c).expect("C in the library");
    let c_id = lib.remove(at);
    lib.insert(0, c_id);
    propose_and_commit(&mut st, ProposedEvent::draw(o));
    assert_eq!(st.objects.get(c).zone, Zone::Hand, "C was drawn");
    mark(&st, &mut marks);
    let gone = if discard_d { d } else { c };
    propose_and_commit(&mut st, ProposedEvent::zone_change(gone, Zone::Graveyard));
    st.engine.pending_triggers.clear();
    let i = waiting(&st, OutletV1::Shaman).expect("the Shaman ability waits");
    if !paid_is_d {
        let named = if named_at == HandRefAtV1::Gone {
            gone
        } else {
            d
        };
        // The epoch it had in the hand: one below its graveyard epoch if it
        // left, its current epoch if it stayed.
        let zcc = st.objects.get(named).zone_change_count;
        let hand = if st.objects.get(named).zone == Zone::Hand {
            zcc
        } else {
            zcc - 1
        };
        let r = &mut st.stack[i].v4.paid_cost_refs[0];
        r.object = named;
        r.zone_change_count = hand;
    }
    let lists = decklists(&s);
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

/// Finding 2: a reference from before D entered the hand refuses
/// `same_name_hand` whichever copy was discarded.
#[test]
fn a_reference_from_before_the_hand_refuses_alike() {
    let (a, b) = (reference_world(true, true), reference_world(true, false));
    eprintln!("reference before the hand: D discarded {a:?} / C discarded {b:?}");
    assert!(
        a.as_ref()
            .is_err_and(|e| e.contains(super::export::R1_SAME_NAME_HAND_DETAIL)),
        "R1 same_name_hand: {a:?}"
    );
    assert_eq!(a, b, "P agrees whichever copy was discarded");
}

/// Finding 1: a reference to the discarded copy at its hand epoch (as a
/// cast receipt names its origin) gives equal P whichever copy it was.
#[test]
fn a_reference_at_the_hand_epoch_agrees() {
    let (a, b) = (reference_world(false, true), reference_world(false, false));
    eprintln!(
        "reference at the hand epoch: D discarded {:?} / C discarded {:?}",
        a.as_ref().map(Vec::len),
        b.as_ref().map(Vec::len)
    );
    if let (Ok(x), Ok(y)) = (&a, &b) {
        if let Some(at) = (0..x.len().min(y.len())).find(|&i| x[i] != y[i]) {
            let window = |z: &[u8]| {
                String::from_utf8_lossy(&z[at.saturating_sub(200)..(at + 200).min(z.len())])
                    .to_string()
            };
            eprintln!(
                "first difference at byte {at}\n--- D:\n{}\n--- C:\n{}",
                window(x),
                window(y)
            );
        }
    }
    assert_eq!(a, b, "P agrees whichever copy was discarded");
}

/// Sol round 3 on ruling 11, finding 1: the record names D at its hand
/// epoch whichever copy was discarded. Equal P.
#[test]
fn a_reference_to_d_at_its_hand_epoch_agrees() {
    let run = |discard_d| reference_world_with(false, discard_d, HandRefAtV1::D, false);
    let (a, b) = (run(true), run(false));
    eprintln!("reference to D at its hand epoch: D discarded {a:?} / C discarded {b:?}");
    assert_eq!(a, b, "P agrees whichever copy was discarded");
}

/// Sol round 3 on ruling 11, finding 2: equal-key bound D and B, unseen C;
/// D or C is discarded and the record names the discarded copy at its hand
/// epoch. Equal P.
#[test]
fn equal_keys_and_an_unseen_copy_agree() {
    let run = |discard_d| reference_world_with(false, discard_d, HandRefAtV1::Gone, true);
    let (a, b) = (run(true), run(false));
    eprintln!("equal keys and an unseen copy: D discarded {a:?} / C discarded {b:?}");
    assert_eq!(a, b, "P agrees whichever copy was discarded");
}

/// Sol round 4 on ruling 11: Lembas A and B are drawn unseen; A is
/// revealed; an unrelated card is put back unidentified (A's fact lapses,
/// A stays); then A (`reveal_a`) or B is revealed and discarded, and the
/// Shaman's record is re-made to name it at its hand epoch (a cast
/// receipt's shape). A's lapse precedes the span the reference resolves
/// to, so it exempts nothing: P agrees. Returns P.
fn stale_lapse_world(reveal_a: bool) -> Result<Vec<u8>, String> {
    use crate::event::{propose_and_commit, ProposedEvent};
    let (o, v) = (PlayerId::P0, PlayerId::P1);
    let s = opponent_payment();
    let lembas = id("Lembas");
    let mut st = s.game_state().clone();
    st.stack
        .retain(|it| st.objects.get(it.source).card_def != lembas);
    st.engine.pending_triggers.clear();
    let in_library: Vec<ObjectId> = st.players[o.index()]
        .library
        .iter()
        .copied()
        .filter(|x| st.objects.get(*x).card_def == lembas)
        .collect();
    assert!(
        in_library.len() >= 2,
        "two Lembas in the opponent's library"
    );
    let (a, b) = (in_library[0], in_library[1]);
    let mut marks = s.infoset_marks_v1(v);
    let mark = |st: &GameState, marks: &mut Vec<(usize, usize)>| {
        marks.push((
            st.engine.event_history.len(),
            st.observation_journal.notes().len(),
        ));
    };
    for x in [a, b] {
        let lib = &mut st.players[o.index()].library;
        let at = lib.iter().position(|y| *y == x).expect("in the library");
        let moved = lib.remove(at);
        lib.insert(0, moved);
        propose_and_commit(&mut st, ProposedEvent::draw(o));
    }
    mark(&st, &mut marks);
    st.reveal_hand_card(v, o, a).expect("reveal A");
    mark(&st, &mut marks);
    let other = st.players[o.index()]
        .hand
        .iter()
        .copied()
        .find(|x| st.objects.get(*x).card_def != lembas)
        .expect("another card in the hand");
    st.clear_nonowner_hand_knowledge(o);
    propose_and_commit(&mut st, ProposedEvent::private_top_library_insert(other));
    mark(&st, &mut marks);
    let shown = if reveal_a { a } else { b };
    st.reveal_hand_card(v, o, shown).expect("reveal a Lembas");
    mark(&st, &mut marks);
    propose_and_commit(&mut st, ProposedEvent::zone_change(shown, Zone::Graveyard));
    st.engine.pending_triggers.clear();
    let i = waiting(&st, OutletV1::Shaman).expect("the Shaman ability waits");
    let zcc = st.objects.get(shown).zone_change_count;
    let r = &mut st.stack[i].v4.paid_cost_refs[0];
    r.object = shown;
    r.zone_change_count = zcc - 1;
    let lists = decklists(&s);
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

#[test]
fn a_stale_lapse_exempts_nothing() {
    let (a, b) = (stale_lapse_world(true), stale_lapse_world(false));
    eprintln!("stale lapse: A revealed {a:?} / B revealed {b:?}");
    assert_eq!(a, b, "P agrees whichever copy was revealed");
}
