use super::*;
use super::{
    ordinary_equipment::reach_main,
    surveil_many::{checkpoint, sessions},
    targeted_entry::{cast, choose, wait_target},
};
use crate::{
    policy_observation_v6::tests::put,
    state::{GameState, Step, Zone},
};

#[test]
fn paid_kraken_tapped_or_untapped_target_additive_stun_source_departure_counter_target_loss_and_restore_both_seats(
) {
    let id = crate::card_def::card_id_by_name("Grappling Kraken").unwrap();
    assert_eq!(
        super::super::super::roots::registered_permanent_printing(id).unwrap(),
        Some(super::super::super::roots::baseline(id).unwrap())
    );
    let forest = crate::card_def::card_id_by_name("Forest").unwrap();
    for p in [PlayerId::P0, PlayerId::P1] {
        for initially_tapped in [false, true] {
            for old_stun in [0, 2] {
                let mut s = GameState::new_from_libraries_with_starting_player_v1(
                    &[forest; 40],
                    &[forest; 40],
                    crate::rl::card_name,
                    110_390,
                    p,
                );
                s.step = Step::Main1;
                for _ in 0..6 {
                    put(&mut s, p, "Island", Zone::Battlefield);
                }
                for _ in 0..3 {
                    put(&mut s, p.opponent(), "Swamp", Zone::Battlefield);
                }
                for _ in 0..2 {
                    put(&mut s, p.opponent(), "Island", Zone::Battlefield);
                }
                let target = put(&mut s, p.opponent(), "Aegis Turtle", Zone::Battlefield);
                // Initial status/counters exercise an already tapped target
                // and addition to existing stun, not prior paid effects.
                s.objects.get_mut(target).tapped = initially_tapped;
                s.objects.get_mut(target).counters.stun = old_stun;
                let source = put(&mut s, p, "Grappling Kraken", Zone::Hand);
                let counter = put(&mut s, p.opponent(), "Counterspell", Zone::Hand);
                let removal = put(&mut s, p.opponent(), "Hero's Downfall", Zone::Hand);
                let (mut own, mut native) = sessions(s);
                cast(&mut own, &mut native, source);
                assert_eq!(own.fixture_state().objects.get(source).zone, Zone::Stack);
                assert_eq!(
                    crate::engine::object_mana_value(own.fixture_state(), source),
                    6
                );
                assert_eq!(own.fixture_state().players[p.index()].mana_pool, [0; 6]);
                assert_eq!(
                    own.fixture_state().players[p.index()]
                        .battlefield
                        .iter()
                        .filter(|&&o| own.fixture_state().objects.get(o).tapped)
                        .count(),
                    6
                );
                let paid = own.snapshot();
                let raw = native.snapshot_v1();
                for response in 0..4 {
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
                            checkpoint(&mut own, &mut native);
                            choose(&mut own, &mut native, source, target);
                            if response >= 2 {
                                act(&mut own, &mut native, |a| matches!(a, A::Pass { .. }));
                                cast(&mut own, &mut native, removal);
                                choose(
                                    &mut own,
                                    &mut native,
                                    removal,
                                    if response == 2 { source } else { target },
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
                            state.objects.get(target).zone,
                            if response == 3 {
                                Zone::Graveyard
                            } else {
                                Zone::Battlefield
                            }
                        );
                        if response == 3 {
                            assert_eq!(state.objects.get(target).counters.stun, 0);
                        } else if response == 1 {
                            assert_eq!(state.objects.get(target).tapped, initially_tapped);
                            assert_eq!(state.objects.get(target).counters.stun, old_stun);
                        } else {
                            assert!(state.objects.get(target).tapped);
                            assert_eq!(state.objects.get(target).counters.stun, old_stun + 1);
                        }
                        if response == 0 && old_stun == 0 {
                            reach_main(&mut own, &mut native, p.opponent());
                            assert!(own.fixture_state().objects.get(target).tapped);
                            assert_eq!(own.fixture_state().objects.get(target).counters.stun, 0);
                            checkpoint(&mut own, &mut native);
                            reach_main(&mut own, &mut native, p);
                            reach_main(&mut own, &mut native, p.opponent());
                            assert!(!own.fixture_state().objects.get(target).tapped);
                            assert_eq!(own.fixture_state().objects.get(target).counters.stun, 0);
                        }
                        if let Some(ref old) = expected {
                            assert_eq!(own.fixture_state(), old);
                        } else {
                            expected = Some(own.fixture_state().clone());
                        }
                    }
                }
            }
        }
    }
}
