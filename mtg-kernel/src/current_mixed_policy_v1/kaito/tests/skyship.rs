use super::*;
use super::{
    searslicer::through_combat,
    surveil_many::{checkpoint, sessions},
    targeted_entry::{cast, choose},
};
use crate::{
    policy_observation_v6::tests::put,
    state::{GameState, StackItemKind, Step, Zone},
};

#[test]
fn paid_buccaneer_raid_requires_prior_actual_attack_entry_counter_source_attacker_departure_and_restore_both_seats(
) {
    let id = crate::card_def::card_id_by_name("Skyship Buccaneer").unwrap();
    assert_eq!(
        super::super::super::roots::registered_permanent_printing(id).unwrap(),
        Some(super::super::super::roots::baseline(id).unwrap())
    );
    let forest = crate::card_def::card_id_by_name("Forest").unwrap();
    for p in [PlayerId::P0, PlayerId::P1] {
        for attacked in [false, true] {
            for late in [false, true] {
                let mut s = GameState::new_from_libraries_with_starting_player_v1(
                    &[forest; 40],
                    &[forest; 40],
                    crate::rl::card_name,
                    110_410,
                    p,
                );
                s.step = Step::Main1;
                let lands = (0..5)
                    .map(|_| put(&mut s, p, "Island", Zone::Battlefield))
                    .collect::<Vec<_>>();
                for name in ["Island", "Swamp"] {
                    for _ in 0..3 {
                        put(&mut s, p.opponent(), name, Zone::Battlefield);
                    }
                }
                let source = put(&mut s, p, "Skyship Buccaneer", Zone::Hand);
                let attacker = put(&mut s, p, "Savannah Lions", Zone::Battlefield);
                let counter = put(&mut s, p.opponent(), "Counterspell", Zone::Hand);
                let removal = put(&mut s, p.opponent(), "Hero's Downfall", Zone::Hand);
                let (mut own, mut native) = sessions(s);
                if late {
                    through_combat(&mut own, &mut native, attacker, attacked);
                }
                cast(&mut own, &mut native, source);
                assert_eq!(own.fixture_state().objects.get(source).zone, Zone::Stack);
                assert!(lands
                    .iter()
                    .all(|&o| own.fixture_state().objects.get(o).tapped));
                assert_eq!(own.fixture_state().players[p.index()].mana_pool, [0; 6]);
                assert_eq!(
                    crate::engine::object_mana_value(own.fixture_state(), source),
                    5
                );
                checkpoint(&mut own, &mut native);
                let hand = own.fixture_state().players[p.index()].hand.len();
                let library = own.fixture_state().players[p.index()].library.len();
                let saved = own.snapshot();
                let raw = native.snapshot_v1();
                let fires = late && attacked;
                for response in 0..if fires { 4 } else { 2 } {
                    let mut expected = None;
                    for _ in 0..2 {
                        own.restore(&saved).unwrap();
                        native.kaito_restore_v75(&raw).unwrap();
                        if response == 1 {
                            cast(&mut own, &mut native, counter);
                            act(&mut own, &mut native, |a| {
                                is_original_spell_target(a, source)
                            });
                        } else if fires {
                            for _ in 0..64 {
                                if own.fixture_state().stack.iter().any(|i| {
                                    i.source == source && i.kind == StackItemKind::TriggeredAbility
                                }) {
                                    break;
                                }
                                act(&mut own, &mut native, |a| {
                                    matches!(
                                        a,
                                        A::Pass { .. }
                                            | A::OrderTriggers { .. }
                                            | A::ChooseTriggerOrderNext { .. }
                                    )
                                });
                            }
                            assert!(own
                                .fixture_state()
                                .stack
                                .iter()
                                .any(|i| i.source == source
                                    && i.kind == StackItemKind::TriggeredAbility));
                            assert_eq!(
                                own.fixture_state().objects.get(source).zone,
                                Zone::Battlefield
                            );
                            assert_eq!(own.fixture_state().players[p.index()].hand.len(), hand);
                            checkpoint(&mut own, &mut native);
                            if response == 2 || response == 3 {
                                cast(&mut own, &mut native, removal);
                                choose(
                                    &mut own,
                                    &mut native,
                                    removal,
                                    if response == 2 { source } else { attacker },
                                );
                            }
                        }
                        settle(&mut own, &mut native);
                        if !late {
                            through_combat(&mut own, &mut native, attacker, attacked);
                        }
                        let draws = usize::from(fires && response != 1);
                        let state = own.fixture_state();
                        assert_eq!(state.players[p.index()].hand.len(), hand + draws);
                        assert_eq!(state.players[p.index()].library.len(), library - draws);
                        assert_eq!(
                            state.objects.get(source).zone,
                            if response == 1 || response == 2 {
                                Zone::Graveyard
                            } else {
                                Zone::Battlefield
                            }
                        );
                        assert_eq!(
                            state.objects.get(attacker).zone,
                            if response == 3 {
                                Zone::Graveyard
                            } else {
                                Zone::Battlefield
                            }
                        );
                        assert!(state.stack.is_empty() && state.engine.pending_triggers.is_empty());
                        if let Some(ref old) = expected {
                            assert_eq!(state, old);
                        } else {
                            expected = Some(state.clone());
                        }
                        checkpoint(&mut own, &mut native);
                    }
                }
            }
        }
    }
}
