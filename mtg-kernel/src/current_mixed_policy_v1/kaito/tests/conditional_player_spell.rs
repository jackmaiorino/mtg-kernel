use super::*;
use crate::{
    policy_observation_v6::tests::put,
    state::{GameState, Step, Zone},
};

#[test]
fn paid_rite_exact_resolution_life_food_bolt_counter_and_pending_restore_both_player_targets_both_seats(
) {
    let id = crate::card_def::card_id_by_name("Hidetsugu's Second Rite").unwrap();
    let meaning = super::super::super::roots::baseline(id).unwrap();
    assert_eq!(
        super::super::super::roots::registered_spell_printing(id).unwrap(),
        Some(meaning)
    );
    let forest = crate::card_def::card_id_by_name("Forest").unwrap();
    for p in [PlayerId::P0, PlayerId::P1] {
        for target in [p, p.opponent()] {
            for initial_life in [10, 11, 13] {
                let mut state = GameState::new_from_libraries_with_starting_player_v1(
                    &[forest; 40],
                    &[forest; 40],
                    crate::rl::card_name,
                    110_354,
                    p,
                );
                state.step = Step::Main1;
                // This is the declared initial life, before any casts/history.
                state.players[target.index()].life = initial_life;
                for name in ["Mountain", "Island"] {
                    for _ in 0..4 {
                        put(&mut state, p, name, Zone::Battlefield);
                    }
                    for _ in 0..2 {
                        put(&mut state, p.opponent(), name, Zone::Battlefield);
                    }
                }
                let food = put(&mut state, target, "Food Token", Zone::Battlefield);
                let counter = put(&mut state, p.opponent(), "Counterspell", Zone::Hand);
                let bolt = put(&mut state, p.opponent(), "Lightning Bolt", Zone::Hand);
                let spell = put(&mut state, p, "Hidetsugu's Second Rite", Zone::Hand);
                let mut native = FastActorSessionV1::from_kaito_v75_fixture_state(state.clone());
                let mut own = KaitoSession::from_session(
                    FastActorSessionV1::from_kaito_v75_fixture_state(state),
                )
                .unwrap();
                act(&mut own, &mut native, |a| {
                    matches!(a,
                    A::CastSpell { source, .. } if source.arena_id == spell.0)
                });
                act(&mut own, &mut native, |a| {
                    matches!(a,
                    A::ChooseTarget { target: crate::rl::TargetRefV1::Player { player }, .. }
                    if *player == crate::rl::PlayerSeatV1::from(target))
                });
                assert_eq!(own.fixture_state().objects.get(spell).zone, Zone::Stack);
                assert_eq!(
                    own.fixture_state().players[p.index()]
                        .battlefield
                        .iter()
                        .filter(|&&o| own.fixture_state().objects.get(o).tapped)
                        .count(),
                    4
                );
                let saved = own.snapshot();
                let raw = native.snapshot_v1();
                // 0: no response, 1: counter, 2: paid Food gain, 3: paid Bolt.
                // In particular13 ->10 turns the damage condition on only later.
                for response in [0, 1, 2, 3] {
                    let mut expected = None;
                    for _ in 0..2 {
                        own.restore(&saved).unwrap();
                        native.kaito_restore_v75(&raw).unwrap();
                        if response == 2 {
                            if target != p {
                                act(&mut own, &mut native, |a| matches!(a, A::Pass { .. }));
                            }
                            act(&mut own, &mut native, |a| {
                                matches!(a,
                                A::ActivateAbility { source, ability_index: 0, .. } if source.arena_id == food.0)
                            });
                            assert!(!own.fixture_state().players[target.index()]
                                .battlefield
                                .contains(&food));
                        } else if response != 0 {
                            act(&mut own, &mut native, |a| matches!(a, A::Pass { .. }));
                            let selected = if response == 1 { counter } else { bolt };
                            act(&mut own, &mut native, |a| {
                                matches!(a,
                                A::CastSpell { source, .. } if source.arena_id == selected.0)
                            });
                            act(&mut own, &mut native, |a| {
                                if response == 1 {
                                    is_original_spell_target(a, spell)
                                } else {
                                    matches!(a, A::ChooseTarget { target: crate::rl::TargetRefV1::Player { player }, .. }
                                    if *player == crate::rl::PlayerSeatV1::from(target))
                                }
                            });
                        }
                        finish_rite(&mut own, &mut native);
                        let at_resolution = initial_life
                            + if response == 2 {
                                3
                            } else if response == 3 {
                                -3
                            } else {
                                0
                            };
                        let damage = if response != 1 && at_resolution == 10 {
                            10
                        } else {
                            0
                        };
                        let state = own.fixture_state();
                        assert_eq!(state.players[target.index()].life, at_resolution - damage);
                        assert_eq!(state.players[target.opponent().index()].life, 20);
                        assert_eq!(state.engine.event_history.iter().filter(|event| matches!(event,
                            crate::event::CommittedEvent::Damage { source, target: crate::state::Target::Player(player), amount: 10 }
                            if *source == spell && *player == target)).count(), usize::from(damage == 10));
                        if at_resolution - damage == 0 {
                            let FastActorResponseV1::Terminal(terminal) = own.current_response()
                            else {
                                panic!("lethal Rite did not end the game")
                            };
                            assert_eq!(
                                terminal.terminal_classification,
                                crate::rl::TerminalClassificationV1::Natural
                            );
                            assert_eq!(
                                terminal.winner,
                                Some(crate::rl::PlayerSeatV1::from(target.opponent()))
                            );
                        } else {
                            assert!(matches!(
                                own.current_response(),
                                FastActorResponseV1::Decision(_)
                            ));
                            assert_eq!(state.objects.get(spell).zone, Zone::Graveyard);
                        }
                        assert_eq!(state.players[p.index()].mana_pool, [0; 6]);
                        assert_eq!(
                            state.players[p.index()]
                                .battlefield
                                .iter()
                                .filter(|&&o| state.objects.get(o).tapped)
                                .count(),
                            4 + if response == 2 && target == p { 2 } else { 0 }
                        );
                        if let Some(ref previous) = expected {
                            assert_eq!(state, previous);
                        } else {
                            expected = Some(state.clone());
                        }
                    }
                }
            }
        }
    }
}

fn finish_rite(own: &mut KaitoSession, native: &mut FastActorSessionV1) {
    for _ in 0..64 {
        if matches!(own.current_response(), FastActorResponseV1::Terminal(_)) {
            return;
        }
        let state = own.fixture_state();
        if state.stack.is_empty()
            && state.engine.pending_effect.is_none()
            && state.engine.pending_discard.is_none()
            && state.engine.pending_triggers.is_empty()
        {
            return;
        }
        act(own, native, |a| matches!(a, A::Pass { .. }));
    }
    panic!("Rite response stack did not finish")
}
