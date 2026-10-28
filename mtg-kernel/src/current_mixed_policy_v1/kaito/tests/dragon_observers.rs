use super::*;
use crate::{
    event::CommittedEvent,
    policy_observation_v6::tests::put,
    state::{GameState, Step, Zone},
};

#[test]
fn paid_whelp_real_dragon_and_noncreature_cast_union_counter_departure_filters_and_restore_both_seats(
) {
    let id = crate::card_def::card_id_by_name("Firespitter Whelp").unwrap();
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
            110_368,
            p,
        );
        state.step = Step::Main1;
        for _ in 0..12 {
            put(&mut state, p, "Mountain", Zone::Battlefield);
        }
        for _ in 0..2 {
            put(&mut state, p, "Forest", Zone::Battlefield);
        }
        for land in ["Island", "Mountain"] {
            for _ in 0..3 {
                put(&mut state, p.opponent(), land, Zone::Battlefield);
            }
        }
        let first = put(&mut state, p, "Firespitter Whelp", Zone::Hand);
        let second = put(&mut state, p, "Firespitter Whelp", Zone::Hand);
        let boots = put(&mut state, p, "Swiftfoot Boots", Zone::Hand);
        let elf = put(&mut state, p, "Llanowar Elves", Zone::Hand);
        let counter = put(&mut state, p.opponent(), "Counterspell", Zone::Hand);
        let removal = put(&mut state, p.opponent(), "Lightning Bolt", Zone::Hand);
        let opposing_bolt = put(&mut state, p.opponent(), "Lightning Bolt", Zone::Hand);
        let mut native = FastActorSessionV1::from_kaito_v75_fixture_state(state.clone());
        let mut own =
            KaitoSession::from_session(FastActorSessionV1::from_kaito_v75_fixture_state(state))
                .unwrap();
        act(
            &mut own,
            &mut native,
            |a| matches!(a,A::CastSpell{source,..}if source.arena_id==first.0),
        );
        settle(&mut own, &mut native);
        assert_eq!(
            own.fixture_state().objects.get(first).zone,
            Zone::Battlefield
        );
        assert_eq!(own.fixture_state().players[p.opponent().index()].life, 20);
        act(
            &mut own,
            &mut native,
            |a| matches!(a,A::CastSpell{source,..}if source.arena_id==second.0),
        );
        assert_eq!(own.fixture_state().objects.get(second).zone, Zone::Stack);
        assert_eq!(
            own.fixture_state().players[p.index()]
                .battlefield
                .iter()
                .filter(|&&o| own.fixture_state().objects.get(o).tapped)
                .count(),
            6
        );
        let saved = own.snapshot();
        let raw = native.snapshot_v1();
        for response in [0, 1, 2] {
            let mut expected = None;
            for _ in 0..2 {
                own.restore(&saved).unwrap();
                native.kaito_restore_v75(&raw).unwrap();
                if response != 0 {
                    act(&mut own, &mut native, |a| matches!(a, A::Pass { .. }));
                    let spell = if response == 1 { counter } else { removal };
                    act(
                        &mut own,
                        &mut native,
                        |a| matches!(a,A::CastSpell{source,..}if source.arena_id==spell.0),
                    );
                    act(&mut own, &mut native, |a| {
                        if response == 1 {
                            is_original_spell_target(a, second)
                        } else {
                            matches!(a,A::ChooseTarget{target:crate::rl::TargetRefV1::Object{object},..}if object.arena_id==first.0)
                        }
                    });
                }
                settle(&mut own, &mut native);
                assert_eq!(own.fixture_state().players[p.opponent().index()].life, 19);
                assert_eq!(
                    own.fixture_state().objects.get(first).zone,
                    if response == 2 {
                        Zone::Graveyard
                    } else {
                        Zone::Battlefield
                    }
                );
                assert_eq!(
                    own.fixture_state().objects.get(second).zone,
                    if response == 1 {
                        Zone::Graveyard
                    } else {
                        Zone::Battlefield
                    }
                );
                let active = if response == 0 { 2 } else { 1 };
                act(
                    &mut own,
                    &mut native,
                    |a| matches!(a,A::CastSpell{source,..}if source.arena_id==boots.0),
                );
                settle(&mut own, &mut native);
                assert_eq!(
                    own.fixture_state().players[p.opponent().index()].life,
                    19 - active
                );
                act(
                    &mut own,
                    &mut native,
                    |a| matches!(a,A::CastSpell{source,..}if source.arena_id==elf.0),
                );
                settle(&mut own, &mut native);
                assert_eq!(
                    own.fixture_state().players[p.opponent().index()].life,
                    19 - active
                );
                act(&mut own, &mut native, |a| matches!(a, A::Pass { .. }));
                act(
                    &mut own,
                    &mut native,
                    |a| matches!(a,A::CastSpell{source,..}if source.arena_id==opposing_bolt.0),
                );
                act(
                    &mut own,
                    &mut native,
                    |a| matches!(a,A::ChooseTarget{target:crate::rl::TargetRefV1::Player{player},..}if *player==crate::rl::PlayerSeatV1::from(p)),
                );
                settle(&mut own, &mut native);
                let s = own.fixture_state();
                assert_eq!(s.players[p.index()].life, 17);
                assert_eq!(s.players[p.opponent().index()].life, 19 - active);
                assert_eq!(s.players[p.index()].mana_pool, [0; 6]);
                assert_eq!(
                    s.players[p.index()]
                        .battlefield
                        .iter()
                        .filter(|&&o| s.objects.get(o).tapped)
                        .count(),
                    9
                );
                for (source, count) in [
                    (first, if response == 2 { 1 } else { 2 }),
                    (second, usize::from(response != 1)),
                ] {
                    assert_eq!(s.engine.event_history.iter().filter(|e|matches!(e,CommittedEvent::Damage{source:actual,amount:1,..}if *actual==source)).count(),count);
                    if s.objects.get(source).zone == Zone::Battlefield {
                        assert_eq!(
                            crate::engine::checked_effective_pt(s, source).unwrap(),
                            (2, 2)
                        );
                        assert!(crate::engine::has_effective_keyword(
                            s,
                            source,
                            crate::card_def::Keywords::FLYING
                        ));
                    }
                }
                if let Some(ref previous) = expected {
                    assert_eq!(s, previous);
                } else {
                    expected = Some(s.clone());
                }
            }
        }
    }
}
