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
fn target(own: &mut KaitoSession, native: &mut FastActorSessionV1, object: ObjectId) {
    act(
        own,
        native,
        |a| matches!(a,A::ChooseTarget{target:crate::rl::TargetRefV1::Object{object:o},..}if o.arena_id==object.0),
    );
}
fn first_strike(s: &GameState, object: ObjectId, expected: bool) {
    assert_eq!(
        crate::engine::has_effective_keyword(s, object, Keywords::FIRST_STRIKE),
        expected
    );
}
fn paid_lands(s: &GameState, p: PlayerId) -> usize {
    s.players[p.index()]
        .battlefield
        .iter()
        .filter(|&&o| {
            let object = s.objects.get(o);
            object.tapped && crate::engine::object_has_type(s, o, crate::card_def::CardType::Land)
        })
        .count()
}
#[test]
fn paid_paladin_own_turn_self_counter_type_control_departure_counter_and_natural_turn_restore_both_seats(
) {
    let id = crate::card_def::card_id_by_name("Inspiring Paladin").unwrap();
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
            110_373,
            p,
        );
        state.step = Step::Main1;
        for land in ["Plains", "Swamp"] {
            for _ in 0..8 {
                put(&mut state, p, land, Zone::Battlefield);
            }
        }
        for land in ["Island", "Swamp"] {
            for _ in 0..3 {
                put(&mut state, p.opponent(), land, Zone::Battlefield);
            }
        }
        let hawk = put(&mut state, p, "Healer's Hawk", Zone::Battlefield);
        let plain = put(&mut state, p, "Helpful Hunter", Zone::Battlefield);
        let opposing = put(&mut state, p.opponent(), "Healer's Hawk", Zone::Battlefield);
        let source = put(&mut state, p, "Inspiring Paladin", Zone::Hand);
        let familiar = put(&mut state, p, "Nine-Lives Familiar", Zone::Hand);
        let own_flight = put(&mut state, p, "Fleeting Flight", Zone::Hand);
        let other_flight = put(&mut state, p, "Fleeting Flight", Zone::Hand);
        let counter = put(&mut state, p.opponent(), "Counterspell", Zone::Hand);
        let removal = put(&mut state, p.opponent(), "Hero's Downfall", Zone::Hand);
        let mut native = FastActorSessionV1::from_kaito_v75_fixture_state(state.clone());
        let mut own =
            KaitoSession::from_session(FastActorSessionV1::from_kaito_v75_fixture_state(state))
                .unwrap();
        cast(&mut own, &mut native, source);
        assert_eq!(own.fixture_state().objects.get(source).zone, Zone::Stack);
        assert_eq!(paid_lands(own.fixture_state(), p), 3);
        let saved = own.snapshot();
        let raw = native.snapshot_v1();
        for response in [0, 1, 2] {
            let mut expected = None;
            for _ in 0..2 {
                own.restore(&saved).unwrap();
                native.kaito_restore_v75(&raw).unwrap();
                if response == 1 {
                    act(&mut own, &mut native, |a| matches!(a, A::Pass { .. }));
                    cast(&mut own, &mut native, counter);
                    act(&mut own, &mut native, |a| {
                        is_original_spell_target(a, source)
                    });
                }
                settle(&mut own, &mut native);
                first_strike(own.fixture_state(), source, response != 1);
                first_strike(own.fixture_state(), hawk, false);
                first_strike(own.fixture_state(), plain, false);
                cast(&mut own, &mut native, familiar);
                settle(&mut own, &mut native);
                assert_eq!(
                    own.fixture_state().objects.get(familiar).counters.revival,
                    8
                );
                assert_eq!(
                    own.fixture_state()
                        .objects
                        .get(familiar)
                        .counters
                        .plus1_plus1,
                    0
                );
                first_strike(own.fixture_state(), familiar, false);
                cast(&mut own, &mut native, own_flight);
                target(&mut own, &mut native, hawk);
                settle(&mut own, &mut native);
                first_strike(own.fixture_state(), hawk, response != 1);
                cast(&mut own, &mut native, other_flight);
                target(&mut own, &mut native, opposing);
                let pending = own.snapshot();
                let pending_raw = native.snapshot_v1();
                own.restore(&pending).unwrap();
                native.kaito_restore_v75(&pending_raw).unwrap();
                settle(&mut own, &mut native);
                assert_eq!(
                    own.fixture_state().objects.get(hawk).counters.plus1_plus1,
                    1
                );
                assert_eq!(
                    own.fixture_state()
                        .objects
                        .get(opposing)
                        .counters
                        .plus1_plus1,
                    1
                );
                first_strike(own.fixture_state(), opposing, false);
                if response == 2 {
                    act(&mut own, &mut native, |a| matches!(a, A::Pass { .. }));
                    cast(&mut own, &mut native, removal);
                    target(&mut own, &mut native, source);
                    settle(&mut own, &mut native);
                }
                let active = response == 0;
                assert_eq!(
                    own.fixture_state().objects.get(source).zone,
                    if active {
                        Zone::Battlefield
                    } else {
                        Zone::Graveyard
                    }
                );
                first_strike(own.fixture_state(), source, active);
                first_strike(own.fixture_state(), hawk, active);
                first_strike(own.fixture_state(), plain, false);
                first_strike(own.fixture_state(), familiar, false);
                first_strike(own.fixture_state(), opposing, false);
                assert_eq!(own.fixture_state().players[p.index()].mana_pool, [0; 6]);
                assert_eq!(paid_lands(own.fixture_state(), p), 8);
                super::ordinary_equipment::reach_main(&mut own, &mut native, p.opponent());
                for object in [source, hawk, plain, familiar, opposing] {
                    first_strike(own.fixture_state(), object, false);
                }
                super::ordinary_equipment::reach_main(&mut own, &mut native, p);
                first_strike(own.fixture_state(), source, active);
                first_strike(own.fixture_state(), hawk, active);
                first_strike(own.fixture_state(), plain, false);
                first_strike(own.fixture_state(), familiar, false);
                first_strike(own.fixture_state(), opposing, false);
                assert_eq!(own.fixture_state().players[p.index()].life, 20);
                assert_eq!(own.fixture_state().players[p.opponent().index()].life, 20);
                if let Some(ref previous) = expected {
                    assert_eq!(own.fixture_state(), previous);
                } else {
                    expected = Some(own.fixture_state().clone());
                }
            }
        }
    }
}
