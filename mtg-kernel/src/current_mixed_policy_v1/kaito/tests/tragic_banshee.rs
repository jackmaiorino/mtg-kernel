use super::*;
use super::{
    surveil_many::{checkpoint, sessions},
    targeted_entry::{cast, choose, wait_target},
};
use crate::{
    policy_observation_v6::tests::put,
    state::{GameState, Step, Zone},
};

#[test]
fn paid_banshee_resolution_time_morbid_source_departure_target_loss_counter_and_restore_both_seats()
{
    let id = crate::card_def::card_id_by_name("Tragic Banshee").unwrap();
    assert_eq!(
        super::super::super::roots::registered_permanent_printing(id).unwrap(),
        Some(super::super::super::roots::baseline(id).unwrap())
    );
    let forest = crate::card_def::card_id_by_name("Forest").unwrap();
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = GameState::new_from_libraries_with_starting_player_v1(
            &[forest; 40],
            &[forest; 40],
            crate::rl::card_name,
            110_388,
            p,
        );
        s.step = Step::Main1;
        for _ in 0..5 {
            put(&mut s, p, "Swamp", Zone::Battlefield);
        }
        for _ in 0..3 {
            put(&mut s, p.opponent(), "Swamp", Zone::Battlefield);
        }
        for _ in 0..2 {
            put(&mut s, p.opponent(), "Island", Zone::Battlefield);
        }
        let target = put(&mut s, p.opponent(), "Serra Angel", Zone::Battlefield);
        let other = put(&mut s, p, "Healer's Hawk", Zone::Battlefield);
        let source = put(&mut s, p, "Tragic Banshee", Zone::Hand);
        let counter = put(&mut s, p.opponent(), "Counterspell", Zone::Hand);
        let removal = put(&mut s, p.opponent(), "Hero's Downfall", Zone::Hand);
        let (mut own, mut native) = sessions(s);
        cast(&mut own, &mut native, source);
        assert_eq!(own.fixture_state().objects.get(source).zone, Zone::Stack);
        assert_eq!(
            crate::engine::object_mana_value(own.fixture_state(), source),
            5
        );
        assert_eq!(own.fixture_state().players[p.index()].mana_pool, [0; 6]);
        assert_eq!(
            own.fixture_state().players[p.index()]
                .battlefield
                .iter()
                .filter(|&&o| own.fixture_state().objects.get(o).tapped)
                .count(),
            5
        );
        assert!(!own.fixture_state().creature_died_this_turn_v1());
        let paid = own.snapshot();
        let raw = native.snapshot_v1();
        for response in 0..5 {
            let mut expected = None;
            for _ in 0..2 {
                own.restore(&paid).unwrap();
                native.kaito_restore_v75(&raw).unwrap();
                if response == 1 {
                    act(&mut own, &mut native, |a| matches!(a, A::Pass { .. }));
                    cast(&mut own, &mut native, counter);
                    act(&mut own, &mut native, |a| {
                        is_original_spell_target(a, source)
                    });
                } else {
                    wait_target(&mut own, &mut native, source, target);
                    assert_eq!(
                        own.fixture_state().objects.get(source).zone,
                        Zone::Battlefield
                    );
                    assert!(!own.fixture_state().creature_died_this_turn_v1());
                    checkpoint(&mut own, &mut native);
                    choose(&mut own, &mut native, source, target);
                    if response >= 2 {
                        act(&mut own, &mut native, |a| matches!(a, A::Pass { .. }));
                        cast(&mut own, &mut native, removal);
                        choose(
                            &mut own,
                            &mut native,
                            removal,
                            match response {
                                2 => source,
                                3 => other,
                                _ => target,
                            },
                        );
                    }
                }
                settle(&mut own, &mut native);
                let state = own.fixture_state();
                assert_eq!(
                    state.objects.get(source).zone,
                    if response == 1 || response == 2 {
                        Zone::Graveyard
                    } else {
                        Zone::Battlefield
                    }
                );
                assert_eq!(
                    state.objects.get(other).zone,
                    if response == 3 {
                        Zone::Graveyard
                    } else {
                        Zone::Battlefield
                    }
                );
                assert_eq!(
                    state.objects.get(target).zone,
                    if response >= 2 {
                        Zone::Graveyard
                    } else {
                        Zone::Battlefield
                    }
                );
                if response < 2 {
                    assert_eq!(
                        crate::engine::checked_effective_pt(state, target).unwrap(),
                        if response == 0 { (3, 3) } else { (4, 4) }
                    );
                }
                if state.objects.get(source).zone == Zone::Battlefield {
                    assert_eq!(
                        crate::engine::checked_effective_pt(state, source).unwrap(),
                        (5, 3)
                    );
                }
                assert_eq!((state.players[0].life, state.players[1].life), (20, 20));
                if let Some(ref old) = expected {
                    assert_eq!(state, old);
                } else {
                    expected = Some(state.clone());
                }
            }
        }
    }
}
