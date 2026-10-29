use super::*;
use super::{
    surveil_many::{checkpoint, sessions},
    targeted_entry::{cast, choose},
};
use crate::{
    ids::ObjectId,
    policy_observation_v6::tests::put,
    state::{GameState, Step, Zone},
};

fn begin_combat(own: &mut KaitoSession, native: &mut FastActorSessionV1, p: PlayerId) {
    for _ in 0..128 {
        if own.fixture_state().active_player == p
            && own.fixture_state().step == Step::BeginCombat
            && own.fixture_state().engine.pending_triggers.is_empty()
        {
            return;
        }
        act(own, native, |a| {
            matches!(
                a,
                A::Pass { .. } | A::OrderTriggers { .. } | A::ChooseTriggerOrderNext { .. }
            )
        });
    }
    panic!("Nessian paid scene did not reach natural beginning of combat");
}

#[test]
fn paid_nessian_other_current_power_threshold_excludes_self_opponent_and_rechecks_after_real_response_both_seats(
) {
    let id = crate::card_def::card_id_by_name("Nessian Hornbeetle").unwrap();
    assert_eq!(
        super::super::super::roots::registered_permanent_printing(id).unwrap(),
        Some(super::super::super::roots::baseline(id).unwrap())
    );
    let forest = crate::card_def::card_id_by_name("Forest").unwrap();
    for p in [PlayerId::P0, PlayerId::P1] {
        // 0: another 2 power, 1: another 3 power, 2: another 4 power,
        // 3: the Hornbeetle itself has 4 power. The opposing Angel always has 4.
        for mode in 0..4 {
            let mut s = GameState::new_from_libraries_with_starting_player_v1(
                &[forest; 40],
                &[forest; 40],
                crate::rl::card_name,
                110_391,
                p,
            );
            s.step = Step::Main1;
            for _ in 0..2 {
                put(&mut s, p, "Forest", Zone::Battlefield);
            }
            for _ in 0..4 {
                put(&mut s, p, "Plains", Zone::Battlefield);
            }
            for _ in 0..3 {
                put(&mut s, p.opponent(), "Swamp", Zone::Battlefield);
            }
            let other = put(&mut s, p, "Savannah Lions", Zone::Battlefield);
            let opposing = put(&mut s, p.opponent(), "Serra Angel", Zone::Battlefield);
            let source = put(&mut s, p, "Nessian Hornbeetle", Zone::Hand);
            let flights = [
                put(&mut s, p, "Fleeting Flight", Zone::Hand),
                put(&mut s, p, "Fleeting Flight", Zone::Hand),
            ];
            let removal = put(&mut s, p.opponent(), "Hero's Downfall", Zone::Hand);
            let stab = put(&mut s, p.opponent(), "Stab", Zone::Hand);
            let (mut own, mut native) = sessions(s);
            cast(&mut own, &mut native, source);
            assert_eq!(own.fixture_state().objects.get(source).zone, Zone::Stack);
            assert_eq!(
                crate::engine::object_mana_value(own.fixture_state(), source),
                2
            );
            assert_eq!(
                own.fixture_state().players[p.index()]
                    .battlefield
                    .iter()
                    .filter(|&&o| own.fixture_state().objects.get(o).tapped)
                    .count(),
                2
            );
            settle(&mut own, &mut native);
            let boosts = match mode {
                0 => 0,
                1 => 1,
                _ => 2,
            };
            for flight in flights.into_iter().take(boosts) {
                cast(&mut own, &mut native, flight);
                choose(
                    &mut own,
                    &mut native,
                    flight,
                    if mode == 3 { source } else { other },
                );
                settle(&mut own, &mut native);
            }
            assert_eq!(own.fixture_state().players[p.index()].mana_pool, [0; 6]);
            assert_eq!(
                own.fixture_state().players[p.index()]
                    .battlefield
                    .iter()
                    .filter(|&&o| own.fixture_state().objects.get(o).tapped)
                    .count(),
                2 + boosts
            );
            let saved = own.snapshot();
            let raw = native.snapshot_v1();
            for response in 0..if mode == 2 { 5 } else { 1 } {
                let mut expected = None;
                for _ in 0..2 {
                    own.restore(&saved).unwrap();
                    native.kaito_restore_v75(&raw).unwrap();
                    begin_combat(&mut own, &mut native, p);
                    let triggered = own.fixture_state().stack.iter().any(|item| {
                        item.source == source
                            && item.kind == crate::state::StackItemKind::TriggeredAbility
                    });
                    assert_eq!(triggered, mode == 2);
                    checkpoint(&mut own, &mut native);
                    if response != 0 {
                        act(&mut own, &mut native, |a| matches!(a, A::Pass { .. }));
                        let (spell, target): (ObjectId, ObjectId) = match response {
                            1 => (removal, other),
                            2 => (stab, other),
                            3 => (removal, source),
                            _ => (removal, opposing),
                        };
                        cast(&mut own, &mut native, spell);
                        choose(&mut own, &mut native, spell, target);
                    }
                    settle(&mut own, &mut native);
                    let state = own.fixture_state();
                    assert_eq!(
                        state.objects.get(source).zone,
                        if response == 3 {
                            Zone::Graveyard
                        } else {
                            Zone::Battlefield
                        }
                    );
                    if response != 3 {
                        let count = i32::from(mode == 3) * 2
                            + i32::from(mode == 2 && matches!(response, 0 | 4));
                        assert_eq!(state.objects.get(source).counters.plus1_plus1, count);
                        assert_eq!(
                            crate::engine::checked_effective_pt(state, source).unwrap(),
                            (2 + count, 2 + count)
                        );
                    }
                    assert_eq!(
                        state.objects.get(other).zone,
                        if response == 1 {
                            Zone::Graveyard
                        } else {
                            Zone::Battlefield
                        }
                    );
                    assert_eq!(
                        state.objects.get(opposing).zone,
                        if response == 4 {
                            Zone::Graveyard
                        } else {
                            Zone::Battlefield
                        }
                    );
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
}
