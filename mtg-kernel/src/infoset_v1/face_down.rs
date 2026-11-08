//! Issue #568: names, refusals and history must not identify a hidden departure.
use super::*;
use crate::event::{propose_and_commit, CommittedEvent, ProposedEvent};
use crate::ids::{ObjectId, PlayerId};
use crate::state::{FaceDownV1, GameState, Zone};

fn fixture() -> (GameState, [Vec<u16>; 2]) {
    let a = super::fixtures::id("Island");
    let b = super::fixtures::id("Forest");
    let lists = [vec![a, a, b, b], vec![a, b]];
    let mut st = GameState::new_from_libraries(
        &lists[0],
        &lists[1],
        |d| crate::card_def::CARD_DEFS[d as usize].name.to_string(),
        1,
    );
    for _ in 0..4 {
        propose_and_commit(&mut st, ProposedEvent::draw(PlayerId::P0));
    }
    (st, lists)
}

fn input<'a>(st: &'a GameState, lists: &'a [Vec<u16>; 2]) -> ExportInputV1<'a> {
    ExportInputV1 {
        state: st,
        journal: st.observation_journal.notes(),
        viewer: PlayerId::P1,
        decklists: [&lists[0], &lists[1]],
        marks: None,
    }
}

// An event-level fixture for all three departure forms. Production capture
// is separately exercised by the actual foretell and cast tests below.
fn hidden_move(st: &mut GameState, id: ObjectId, to: Zone, disguised: bool) {
    let start = st.engine.event_history.len();
    if to == Zone::Stack {
        // Like casting, update the hand/stack transition before SpellCast.
        // Stack entry is not an effect-leaf ZoneChange operation.
        st.players[0].hand.retain(|&card| card != id);
        let turn = st.turn_key_v1();
        let object = st.objects.get_mut(id);
        object.zone = Zone::Stack;
        object.zone_change_count += 1;
        object
            .v4
            .reset_for_zone_change(object.card_def, Zone::Stack, turn);
    } else {
        propose_and_commit(st, ProposedEvent::zone_change(id, to));
    }
    st.objects.get_mut(id).v4.face_down_v1 = Some(FaceDownV1 {
        disguised,
        lookers: 1,
        hidden_by: None,
        hidden_by_source: None,
    });
    st.observation_journal.record_visibility_v2(start, id, 1);
    if to == Zone::Stack {
        // Casting itself has no ZoneChange in the production stream.
        crate::event::log_spell_cast(st, id, PlayerId::P0);
    }
    st.forget_hand_after_face_down_departure_v1(PlayerId::P0);
}

#[test]
fn v2_hidden_departures_have_identical_facts_survivors_seams_and_refusals() {
    for (to, disguised) in [
        (Zone::Stack, true),
        (Zone::Stack, false),
        (Zone::Exile, false),
        (Zone::Battlefield, true),
    ] {
        let (mut base, lists) = fixture();
        base.enable_hand_departures_by_name_v1();
        let hand = base.players[0].hand.clone();
        base.reveal_hand_card(PlayerId::P1, PlayerId::P0, hand[0])
            .unwrap();
        base.reveal_hand_card(PlayerId::P1, PlayerId::P0, hand[2])
            .unwrap();
        let mut left = base.clone();
        let mut right = base;
        hidden_move(&mut left, hand[0], to, disguised);
        hidden_move(&mut right, hand[2], to, disguised);
        let p = [&left, &right]
            .map(|s| super::projection::project_v2(s, s.observation_journal.notes(), PlayerId::P1));
        assert_eq!(p[0].hand_facts, p[1].hand_facts);
        assert_eq!(p[0].survivors, p[1].survivors);
        assert_eq!(p[0].seams, p[1].seams);
        assert_eq!(p[0].departures, p[1].departures);
        assert!(p[0].departures.is_empty());
        assert_eq!(p[0].departure_known, p[1].departure_known);
        let a = export_v2(&input(&left, &lists)).unwrap_err();
        let b = export_v2(&input(&right, &lists)).unwrap_err();
        assert_eq!(a, b);
        assert!(matches!(
            a,
            ExportErrorV1::Refused {
                code: RefusalCodeV1::R2,
                ..
            }
        ));
        // Sensitivity and compatibility: historical v1 retains its named rule.
        let old = [&left, &right]
            .map(|s| super::projection::project_v1(s, s.observation_journal.notes(), PlayerId::P1));
        assert_ne!(old[0].hand_facts, old[1].hand_facts);
    }
}

#[test]
fn v2_historical_face_down_exports_resume_after_public_zone_reset() {
    for to in [Zone::Stack, Zone::Exile, Zone::Battlefield] {
        let (base, lists) = fixture();
        let hand = base.players[0].hand.clone();
        let mut worlds = [base.clone(), base];
        for (st, departed) in worlds.iter_mut().zip([hand[0], hand[1]]) {
            hidden_move(st, departed, to, true);
            propose_and_commit(st, ProposedEvent::zone_change(departed, Zone::Graveyard));
            assert!(st.objects.get(departed).v4.face_down_v1.is_none());
            let p = super::projection::project_v2(st, st.observation_journal.notes(), PlayerId::P1);
            assert!(
                p.departures.is_empty(),
                "a later reset must not name the old departure"
            );
            assert_eq!(p.departure_known.len(), 1);
        }
        let out = worlds
            .each_ref()
            .map(|s| export_v2(&input(s, &lists)).unwrap());
        assert_eq!(out[0].bytes, out[1].bytes);
        assert_eq!(out[0].sha256, out[1].sha256);
        assert_eq!(out[0].schema, INFOSET_SCHEMA_V2);
        assert!(out[0]
            .handles
            .iter()
            .any(|h| matches!(h, HandleV1::H { named: false, .. })));
    }
}

#[test]
fn v2_turn_up_does_not_rewrite_the_hidden_departure() {
    let (base, lists) = fixture();
    let hand = base.players[0].hand.clone();
    let mut worlds = [base.clone(), base];
    for (st, departed) in worlds.iter_mut().zip([hand[0], hand[1]]) {
        hidden_move(st, departed, Zone::Battlefield, true);
        st.objects.get_mut(departed).v4.face_down_v1 = None;
        st.engine.event_history.push(CommittedEvent::TurnedFaceUp {
            object: departed,
            zone_change_count: st.objects.get(departed).zone_change_count,
        });
        let p = super::projection::project_v2(st, st.observation_journal.notes(), PlayerId::P1);
        assert!(p.departures.is_empty());
        assert!(p.tracked(departed));
        assert_eq!(p.departure_known.len(), 1);
    }
    let out = worlds
        .each_ref()
        .map(|s| export_v2(&input(s, &lists)).unwrap());
    assert_eq!(out[0].bytes, out[1].bytes);
}

#[test]
fn v2_face_down_exile_to_hidden_zones_never_reveals_the_name() {
    for to in [Zone::Hand, Zone::Library] {
        let (base, lists) = fixture();
        let hand = base.players[0].hand.clone();
        let mut worlds = [base.clone(), base];
        for (st, departed) in worlds.iter_mut().zip([hand[0], hand[2]]) {
            hidden_move(st, departed, Zone::Exile, false);
            let index = st.engine.event_history.len();
            propose_and_commit(st, ProposedEvent::zone_change(departed, to));
            assert!(st.known_hand_cards(PlayerId::P1, PlayerId::P0).is_empty());
            assert_eq!(
                st.observation_journal.visibility_v2(index, departed),
                Some(1)
            );
            let p = super::projection::project_v2(st, st.observation_journal.notes(), PlayerId::P1);
            assert!(p.hand_facts.is_empty());
            assert!(p.seams.is_empty());
            assert!(p.entries.is_empty());
            assert!(p.departures.is_empty());
        }
        let out = worlds
            .each_ref()
            .map(|s| export_v2(&input(s, &lists)).unwrap());
        assert_eq!(out[0].bytes, out[1].bytes);
        if to == Zone::Hand {
            // Later explicit reveals remain visible despite the native helper's old row.
            let st = &mut worlds[0];
            st.reveal_hand_card(PlayerId::P1, PlayerId::P0, hand[0])
                .unwrap();
            let p = super::projection::project_v2(st, st.observation_journal.notes(), PlayerId::P1);
            assert_eq!(p.hand_facts.len(), 1);
            assert!(p.hand_facts[0].known);
            assert_eq!(p.hand_facts[0].card_def, st.objects.get(hand[0]).card_def);
        }
    }
}

#[cfg(feature = "eternal-fixtures")]
#[test]
fn v2_actual_foretell_captures_hidden_event_time_visibility() {
    let behold = super::fixtures::id("Behold the Multiverse");
    let island = super::fixtures::id("Island");
    let mut st = GameState::new_from_libraries(
        &[behold, island],
        &[island],
        |d| crate::card_def::CARD_DEFS[d as usize].name.to_string(),
        1,
    );
    propose_and_commit(&mut st, ProposedEvent::draw(PlayerId::P0));
    let card = st.players[0].hand[0];
    st.players[0].mana_pool[5] = 2;
    crate::eternal_pauper_e6a_v1::foretell(&mut st, PlayerId::P0, card).unwrap();
    let index = st
        .engine
        .event_history
        .iter()
        .position(|e| {
            matches!(e,
        CommittedEvent::ZoneChange { object, from: Zone::Hand, to: Zone::Exile, .. }
        if *object == card)
        })
        .unwrap();
    assert_eq!(st.observation_journal.visibility_v2(index, card), Some(1));
    propose_and_commit(&mut st, ProposedEvent::zone_change(card, Zone::Graveyard));
    let p = super::projection::project_v2(&st, st.observation_journal.notes(), PlayerId::P1);
    assert!(p.departures.is_empty());
    assert_eq!(p.departure_known.len(), 1);
}

#[cfg(feature = "eternal-fixtures")]
#[test]
fn v2_actual_disguise_and_morph_casts_capture_hidden_visibility() {
    use crate::engine::{self, Action, CastMode, Decision};
    for name in ["Flourishing Bloom-Kin", "Exalted Angel"] {
        let def = super::fixtures::id(name);
        let island = super::fixtures::id("Island");
        let mut st = GameState::new_from_libraries(
            &[def, island],
            &[island],
            |d| crate::card_def::CARD_DEFS[d as usize].name.to_string(),
            1,
        );
        propose_and_commit(&mut st, ProposedEvent::draw(PlayerId::P0));
        let card = st.players[0].hand[0];
        st.step = crate::state::Step::Main1;
        st.players[0].mana_pool[5] = 3;
        assert!(matches!(
            engine::advance_until_decision(&mut st),
            Decision::CastSpellOrPass { .. }
        ));
        engine::step(&mut st, Action::CastSpell(card)).unwrap();
        if matches!(
            engine::advance_until_decision(&mut st),
            Decision::ChooseCastMode { .. }
        ) {
            engine::step(&mut st, Action::ChooseCastMode(CastMode::Alternative)).unwrap();
        }
        engine::advance_until_decision(&mut st);
        let index = st
            .engine
            .event_history
            .iter()
            .position(|e| {
                matches!(e,
            CommittedEvent::SpellCast { spell, .. } if *spell == card)
            })
            .unwrap();
        assert_eq!(
            st.observation_journal.visibility_v2(index, card),
            Some(1),
            "{name}"
        );
        let p = super::projection::project_v2(&st, st.observation_journal.notes(), PlayerId::P1);
        assert!(p.departures.is_empty(), "{name}");
        assert_eq!(p.departure_known.len(), 1, "{name}");
    }
}

#[test]
fn v2_visibility_provenance_is_private_and_missing_history_refuses() {
    let (mut st, lists) = fixture();
    let id = st.players[0].hand[0];
    let bytes = serde_json::to_vec(&st).unwrap();
    let hash = st.state_hash();
    let notes = serde_json::to_vec(st.observation_journal.notes()).unwrap();
    st.observation_journal.record_visibility_v2(100, id, 1);
    assert_eq!(serde_json::to_vec(&st).unwrap(), bytes);
    assert_eq!(st.state_hash(), hash);
    assert_eq!(
        serde_json::to_vec(st.observation_journal.notes()).unwrap(),
        notes
    );
    propose_and_commit(&mut st, ProposedEvent::zone_change(id, Zone::Graveyard));
    let restored: GameState = serde_json::from_value(serde_json::to_value(&st).unwrap()).unwrap();
    assert!(
        matches!(export_v2(&input(&restored, &lists)), Err(ExportErrorV1::Refused {
        code: RefusalCodeV1::R2, detail,
    }) if detail.contains("provenance"))
    );
}

#[test]
fn v2_schema_and_payload_digest_are_distinct_while_v1_state_is_unchanged() {
    let live = crate::rl_session::FastActorSessionV1::reset_with_decks_and_limits_flat_action_v3(
        1,
        1,
        2_000,
        20_000,
        ["Wildfire".into(), "Wildfire".into()],
    )
    .unwrap();
    let lists = super::tests::decklists(&live);
    let mut source = input(live.game_state(), &lists);
    source.viewer = PlayerId::P0;
    let old = export_v1(&source, ExportOptionsV1::default()).unwrap();
    let new = export_v2(&source).unwrap();
    assert_eq!(
        serde_json::to_value(&old.state).unwrap(),
        serde_json::to_value(&new.state).unwrap()
    );
    assert_eq!(old.schema, INFOSET_SCHEMA_V1);
    assert_eq!(new.schema, INFOSET_SCHEMA_V2);
    assert_ne!(old.sha256, new.sha256);
    assert_eq!(
        old.sha256,
        super::export::canonical_digest_v1(
            &old.state,
            &old.handles.iter().map(HandleV1::label).collect::<Vec<_>>()
        )
    );
    let payload = PayloadV1::new(&new, &live, PlayerId::P0, 0).unwrap();
    assert_eq!(payload.schema, INFOSET_SCHEMA_V2);
    let decoded = PayloadV1::from_bytes(&payload.to_bytes()).unwrap();
    assert_eq!(
        serde_json::to_value(decoded.import().unwrap().game_state()).unwrap(),
        serde_json::to_value(&new.state).unwrap()
    );
    let mut relabelled = payload;
    relabelled.schema = INFOSET_SCHEMA_V1.into();
    relabelled.sha256 = relabelled.envelope_digest();
    assert!(relabelled.import().err().unwrap().contains("state digest"));
}
