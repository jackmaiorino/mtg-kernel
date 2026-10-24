use super::*;
use crate::{
    card_def::Keywords,
    ids::ObjectId,
    policy_observation_v6::tests::put,
    state::{GameState, Step, Zone},
};
fn cast(own: &mut KaitoSession, native: &mut FastActorSessionV1, source: ObjectId) {
    act(
        own,
        native,
        |a| matches!(a,A::CastSpell{source:s,..}if s.arena_id==source.0),
    );
}
fn counter(
    own: &mut KaitoSession,
    native: &mut FastActorSessionV1,
    spell: ObjectId,
    victim: ObjectId,
) {
    act(own, native, |a| matches!(a, A::Pass { .. }));
    cast(own, native, spell);
    act(own, native, |a| is_original_spell_target(a, victim));
}
fn check(s: &GameState, object: ObjectId, base: (i32, i32), boosted: bool) {
    assert_eq!(
        crate::engine::checked_effective_pt(s, object).unwrap(),
        (base.0 + i32::from(boosted), base.1)
    );
    assert_eq!(
        crate::engine::has_effective_keyword(s, object, Keywords::TRAMPLE),
        boosted
    );
    assert_eq!(s.objects.get(object).counters.plus1_plus1, 0);
}
#[test]
fn paid_balmor_instant_sorcery_filter_counter_departure_resolution_set_late_entry_cleanup_and_restore_both_seats(
) {
    let id = crate::card_def::card_id_by_name("Balmor, Battlemage Captain").unwrap();
    assert_eq!(
        super::super::super::roots::registered_permanent_printing(id).unwrap(),
        Some(super::super::super::roots::baseline(id).unwrap())
    );
    let forest = crate::card_def::card_id_by_name("Forest").unwrap();
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut state = GameState::new_from_libraries_with_starting_player_v1(
            &[forest; 40],
            &[forest; 40],
            crate::rl::card_name,
            110_372,
            p,
        );
        state.step = Step::Main1;
        for land in ["Island", "Mountain", "Plains"] {
            for _ in 0..5 {
                put(&mut state, p, land, Zone::Battlefield);
            }
        }
        for land in ["Island", "Swamp"] {
            for _ in 0..3 {
                put(&mut state, p.opponent(), land, Zone::Battlefield);
            }
        }
        let hawk = put(&mut state, p, "Healer's Hawk", Zone::Battlefield);
        let opposing = put(&mut state, p.opponent(), "Healer's Hawk", Zone::Battlefield);
        let source = put(&mut state, p, "Balmor, Battlemage Captain", Zone::Hand);
        let boots = put(&mut state, p, "Swiftfoot Boots", Zone::Hand);
        let bolt = put(&mut state, p, "Lightning Bolt", Zone::Hand);
        let brineborn = put(&mut state, p, "Brineborn Cutthroat", Zone::Hand);
        let late = put(&mut state, p, "Healer's Hawk", Zone::Hand);
        let counterspell = put(&mut state, p.opponent(), "Counterspell", Zone::Hand);
        let removal = put(&mut state, p.opponent(), "Hero's Downfall", Zone::Hand);
        let mut native = FastActorSessionV1::from_kaito_v75_fixture_state(state.clone());
        let mut own =
            KaitoSession::from_session(FastActorSessionV1::from_kaito_v75_fixture_state(state))
                .unwrap();
        cast(&mut own, &mut native, source);
        assert_eq!(own.fixture_state().objects.get(source).zone, Zone::Stack);
        assert_eq!(
            own.fixture_state().players[p.index()]
                .battlefield
                .iter()
                .filter(|&&o| own.fixture_state().objects.get(o).tapped)
                .count(),
            2
        );
        let saved = own.snapshot();
        let raw = native.snapshot_v1();
        for response in [0, 1, 2, 3, 4] {
            let mut expected = None;
            for _ in 0..2 {
                own.restore(&saved).unwrap();
                native.kaito_restore_v75(&raw).unwrap();
                if response == 4 {
                    counter(&mut own, &mut native, counterspell, source);
                }
                settle(&mut own, &mut native);
                assert_eq!(
                    own.fixture_state().objects.get(source).zone,
                    if response == 4 {
                        Zone::Graveyard
                    } else {
                        Zone::Battlefield
                    }
                );
                cast(&mut own, &mut native, boots);
                settle(&mut own, &mut native);
                check(own.fixture_state(), hawk, (1, 1), false);
                check(own.fixture_state(), opposing, (1, 1), false);
                if response != 4 {
                    check(own.fixture_state(), source, (1, 3), false);
                }
                cast(&mut own, &mut native, bolt);
                act(
                    &mut own,
                    &mut native,
                    |a| matches!(a,A::ChooseTarget{target:crate::rl::TargetRefV1::Player{player},..}if *player==crate::rl::PlayerSeatV1::from(p.opponent())),
                );
                let pending = own.snapshot();
                let pending_raw = native.snapshot_v1();
                own.restore(&pending).unwrap();
                native.kaito_restore_v75(&pending_raw).unwrap();
                match response {
                    1 => counter(&mut own, &mut native, counterspell, bolt),
                    2 => {
                        act(&mut own, &mut native, |a| matches!(a, A::Pass { .. }));
                        cast(&mut own, &mut native, removal);
                        act(
                            &mut own,
                            &mut native,
                            |a| matches!(a,A::ChooseTarget{target:crate::rl::TargetRefV1::Object{object},..}if object.arena_id==source.0),
                        );
                    }
                    3 => cast(&mut own, &mut native, brineborn),
                    _ => {}
                }
                settle(&mut own, &mut native);
                let boosted = response != 4;
                check(own.fixture_state(), hawk, (1, 1), boosted);
                check(own.fixture_state(), opposing, (1, 1), false);
                assert_eq!(
                    own.fixture_state().objects.get(source).zone,
                    if response == 2 || response == 4 {
                        Zone::Graveyard
                    } else {
                        Zone::Battlefield
                    }
                );
                if response != 2 && response != 4 {
                    check(own.fixture_state(), source, (1, 3), true);
                }
                if response == 3 {
                    check(own.fixture_state(), brineborn, (2, 1), true);
                }
                assert_eq!(
                    own.fixture_state().players[p.opponent().index()].life,
                    if response == 1 { 20 } else { 17 }
                );
                cast(&mut own, &mut native, late);
                settle(&mut own, &mut native);
                check(own.fixture_state(), late, (1, 1), false);
                assert_eq!(own.fixture_state().players[p.index()].mana_pool, [0; 6]);
                assert_eq!(
                    own.fixture_state().players[p.index()]
                        .battlefield
                        .iter()
                        .filter(|&&o| own.fixture_state().objects.get(o).tapped)
                        .count(),
                    if response == 3 { 8 } else { 6 }
                );
                super::ordinary_equipment::reach_main(&mut own, &mut native, p.opponent());
                check(own.fixture_state(), hawk, (1, 1), false);
                check(own.fixture_state(), late, (1, 1), false);
                check(own.fixture_state(), opposing, (1, 1), false);
                if response != 2 && response != 4 {
                    check(own.fixture_state(), source, (1, 3), false);
                }
                if response == 3 {
                    check(own.fixture_state(), brineborn, (2, 1), false);
                }
                assert_eq!(own.fixture_state().players[p.index()].life, 20);
                if let Some(ref previous) = expected {
                    assert_eq!(own.fixture_state(), previous);
                } else {
                    expected = Some(own.fixture_state().clone());
                }
            }
        }
    }
}
