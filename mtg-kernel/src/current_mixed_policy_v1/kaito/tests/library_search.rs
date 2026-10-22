use super::targeted_entry::{actions, cast, choose, wait_target};
use super::*;
use crate::{
    ids::ObjectId,
    policy_observation_v6::tests::put,
    state::{GameState, Step, Zone},
};
fn quiet(a: &A) -> bool {
    matches!(
        a,
        A::Pass { .. } | A::OrderTriggers { .. } | A::ChooseTriggerOrderNext { .. }
    )
}
fn targets(a: &A, source: ObjectId) -> Option<ObjectId> {
    match a {
        A::ChooseEffectTarget {
            source: s,
            target: crate::rl::TargetRefV1::Object { object },
            ..
        } if s.arena_id == source.0 => Some(ObjectId(object.arena_id)),
        _ => None,
    }
}
pub(super) fn optional(
    own: &mut KaitoSession,
    native: &mut FastActorSessionV1,
    source: ObjectId,
    accept: bool,
) {
    for _ in 0..128 {
        if actions(own)
            .iter()
            .any(|a| matches!(a,A::ChooseEffectOption{source:s,..} if s.arena_id==source.0))
        {
            let saved = own.snapshot();
            let raw = native.snapshot_v1();
            own.restore(&saved).unwrap();
            native.kaito_restore_v75(&raw).unwrap();
            act(
                own,
                native,
                |a| matches!(a,A::ChooseEffectOption{source:s,option_count:2,option_index,..} if s.arena_id==source.0 && *option_index==u16::from(accept)),
            );
            return;
        }
        act(own, native, quiet);
    }
    panic!("genuine optional search absent");
}
pub(super) fn finish_zero(
    own: &mut KaitoSession,
    native: &mut FastActorSessionV1,
    source: ObjectId,
) {
    for _ in 0..128 {
        if actions(own).iter().any(|a|matches!(a,A::FinishEffectSelection{source:s,selected_count:0,..} if s.arena_id==source.0)) {
            let saved=own.snapshot();let raw=native.snapshot_v1();
            own.restore(&saved).unwrap();native.kaito_restore_v75(&raw).unwrap();
            act(own,native,|a|matches!(a,A::FinishEffectSelection{source:s,selected_count:0,..} if s.arena_id==source.0));return;
        }
        act(own, native, quiet);
    }
    panic!("actual empty search finish absent");
}
pub(super) fn wait_entered(
    own: &mut KaitoSession,
    native: &mut FastActorSessionV1,
    source: ObjectId,
) {
    for _ in 0..128 {
        if own.fixture_state().objects.get(source).zone == Zone::Battlefield {
            assert!(!own.fixture_state().stack.is_empty());
            return;
        }
        act(own, native, quiet);
    }
    panic!("paid source entry absent");
}
fn scene(p: PlayerId, name: &str, empty: bool) -> GameState {
    let forest = crate::card_def::card_id_by_name("Forest").unwrap();
    // This low-level constructor installs libraries without drawing a hand.
    // The empty-library branch must start with no cards in that zone.
    let cards = vec![forest; if empty { 0 } else { 40 }];
    let mut s = GameState::new_from_libraries_with_starting_player_v1(
        &cards,
        &cards,
        crate::rl::card_name,
        110_383,
        p,
    );
    s.step = Step::Main1;
    let price = if name == "Campus Guide" {
        2
    } else if name == "Grim Tutor" {
        3
    } else {
        7
    };
    for _ in 0..price {
        put(&mut s, p, "Swamp", Zone::Battlefield);
    }
    for land in ["Island", "Swamp"] {
        for _ in 0..3 {
            put(&mut s, p.opponent(), land, Zone::Battlefield);
        }
    }
    s
}
fn owner(s: GameState) -> (KaitoSession, FastActorSessionV1) {
    let native = FastActorSessionV1::from_kaito_v75_fixture_state(s.clone());
    (
        KaitoSession::from_session(FastActorSessionV1::from_kaito_v75_fixture_state(s)).unwrap(),
        native,
    )
}

#[test]
fn paid_campus_optional_decline_fail_to_find_revealed_top_source_departure_counter_and_restore_both_seats(
) {
    paid("Campus Guide");
}
#[test]
fn paid_demon_mandatory_any_card_private_hand_source_departure_counter_and_restore_both_seats() {
    paid("Rune-Scarred Demon");
}
#[test]
fn paid_grim_mandatory_any_card_private_hand_then_life_counter_and_restore_both_seats() {
    paid("Grim Tutor");
}
fn paid(name: &str) {
    let campus = name == "Campus Guide";
    let spell = name == "Grim Tutor";
    let id = crate::card_def::card_id_by_name(name).unwrap();
    let printing = if spell {
        super::super::super::roots::registered_spell_printing(id)
    } else {
        super::super::super::roots::registered_permanent_printing(id)
    }
    .unwrap();
    assert_eq!(
        printing,
        Some(super::super::super::roots::baseline(id).unwrap())
    );
    for p in [PlayerId::P0, PlayerId::P1] {
        let q = p.opponent();
        let mut s = scene(p, name, false);
        let source = put(&mut s, p, name, Zone::Hand);
        let instant = put(&mut s, p, "Lightning Bolt", Zone::Library);
        let land = s.players[p.index()].library[0];
        let counter = put(&mut s, q, "Counterspell", Zone::Hand);
        let removal = put(&mut s, q, "Hero's Downfall", Zone::Hand);
        let (mut own, mut native) = owner(s);
        cast(&mut own, &mut native, source);
        let saved = own.snapshot();
        let raw = native.snapshot_v1();
        for branch in 0..if spell { 3 } else { 5 } {
            let mut expected = None;
            for _ in 0..2 {
                own.restore(&saved).unwrap();
                native.kaito_restore_v75(&raw).unwrap();
                let library = own.fixture_state().players[p.index()].library.clone();
                let hand = own.fixture_state().players[p.index()].hand.len();
                let rng = own.fixture_state().legacy_rng().copied();
                let knowledge = own.fixture_state().library_knowledge.clone();
                let selected = if !campus && branch == 4 {
                    library[1]
                } else if campus || branch == 2 {
                    land
                } else {
                    instant
                };
                if branch == 0 {
                    cast(&mut own, &mut native, counter);
                    act(&mut own, &mut native, |a| {
                        is_original_spell_target(a, source)
                    });
                } else {
                    if !spell && branch == 2 {
                        wait_entered(&mut own, &mut native, source);
                        cast(&mut own, &mut native, removal);
                        act(
                            &mut own,
                            &mut native,
                            |a| matches!(a,A::ChooseTarget{target:crate::rl::TargetRefV1::Object{object},..} if object.arena_id==source.0),
                        );
                    }
                    if campus {
                        optional(&mut own, &mut native, source, branch != 3);
                    }
                    if !(campus && branch == 3) {
                        wait_target(&mut own, &mut native, source, selected);
                        let menu = actions(&own);
                        let mut actual = menu
                            .iter()
                            .filter_map(|a| targets(a, source))
                            .collect::<Vec<_>>();
                        actual.sort_unstable();
                        let mut expected_cards = library
                            .iter()
                            .copied()
                            .filter(|&o| !campus || o != instant)
                            .collect::<Vec<_>>();
                        expected_cards.sort_unstable();
                        assert_eq!(actual, expected_cards);
                        assert_eq!(menu.iter().any(|a|matches!(a,A::FinishEffectSelection{source:s,selected_count:0,..} if s.arena_id==source.0)),campus);
                        let choosing = own.snapshot();
                        let choosing_raw = native.snapshot_v1();
                        own.restore(&choosing).unwrap();
                        native.kaito_restore_v75(&choosing_raw).unwrap();
                        if campus && branch == 4 {
                            finish_zero(&mut own, &mut native, source);
                        } else {
                            choose(&mut own, &mut native, source, selected);
                        }
                    }
                }
                settle(&mut own, &mut native);
                let state = own.fixture_state();
                assert_eq!(
                    state.objects.get(source).zone,
                    if spell || branch == 0 || branch == 2 {
                        Zone::Graveyard
                    } else {
                        Zone::Battlefield
                    }
                );
                let tapped = state.players[p.index()]
                    .battlefield
                    .iter()
                    .filter(|&&o| state.objects.get(o).tapped)
                    .count();
                assert_eq!(
                    tapped,
                    if campus {
                        2
                    } else if spell {
                        3
                    } else {
                        7
                    }
                );
                assert_eq!(state.players[p.index()].mana_pool, [0; 6]);
                assert_eq!(
                    state.players[p.index()].life,
                    if spell && branch != 0 { 17 } else { 20 }
                );
                assert_eq!(state.players[q.index()].life, 20);
                if branch == 0 || (campus && branch == 3) {
                    assert_eq!(state.players[p.index()].library, library);
                    assert_eq!(state.legacy_rng().copied(), rng);
                    assert_eq!(state.library_knowledge, knowledge);
                    assert_eq!(state.players[p.index()].hand.len(), hand);
                } else if campus {
                    assert_eq!(state.players[p.index()].hand.len(), hand);
                    assert_eq!(state.players[p.index()].library.len(), library.len());
                    assert_ne!(state.legacy_rng().copied(), rng);
                    if branch != 4 {
                        assert_eq!(state.players[p.index()].library[0], selected);
                        for viewer in [p, q] {
                            assert!(state
                                .known_library_cards(viewer, p)
                                .iter()
                                .any(|k| k.position == 0 && k.object == selected));
                        }
                    }
                } else {
                    assert_eq!(state.players[p.index()].library.len(), library.len() - 1);
                    assert_eq!(state.players[p.index()].hand.len(), hand + 1);
                    assert!(state.players[p.index()].hand.contains(&selected));
                    assert!(
                        state.known_hand_cards(q, p).is_empty(),
                        "unrestricted search must not reveal the selected card"
                    );
                }
                if let Some(ref prior) = expected {
                    assert_eq!(state, prior);
                } else {
                    expected = Some(state.clone());
                }
            }
        }
    }
}

#[test]
fn paid_all_three_empty_library_searches_keep_costs_life_private_finish_and_restore_both_seats() {
    for name in ["Campus Guide", "Rune-Scarred Demon", "Grim Tutor"] {
        for p in [PlayerId::P0, PlayerId::P1] {
            let mut s = scene(p, name, true);
            assert!(s.players[p.index()].library.is_empty());
            let source = put(&mut s, p, name, Zone::Hand);
            let (mut own, mut native) = owner(s);
            cast(&mut own, &mut native, source);
            let saved = own.snapshot();
            let raw = native.snapshot_v1();
            let mut expected = None;
            for _ in 0..2 {
                own.restore(&saved).unwrap();
                native.kaito_restore_v75(&raw).unwrap();
                let hand = own.fixture_state().players[p.index()].hand.clone();
                if name == "Campus Guide" {
                    optional(&mut own, &mut native, source, true);
                }
                finish_zero(&mut own, &mut native, source);
                settle(&mut own, &mut native);
                let state = own.fixture_state();
                assert!(state.players[p.index()].library.is_empty());
                assert_eq!(state.players[p.index()].hand, hand);
                assert_eq!(
                    state.players[p.index()].life,
                    if name == "Grim Tutor" { 17 } else { 20 }
                );
                assert_eq!(
                    state.objects.get(source).zone,
                    if name == "Grim Tutor" {
                        Zone::Graveyard
                    } else {
                        Zone::Battlefield
                    }
                );
                if let Some(ref prior) = expected {
                    assert_eq!(state, prior);
                } else {
                    expected = Some(state.clone());
                }
            }
        }
    }
}
