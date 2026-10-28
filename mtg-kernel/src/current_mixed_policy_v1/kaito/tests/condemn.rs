use super::*;
use super::{
    surveil_many::{checkpoint, sessions},
    targeted_entry::{actions, cast, choose},
};
use crate::{
    event::{self, ProposedEvent},
    ids::ObjectId,
    policy_observation_v6::tests::put,
    state::{GameState, Step, Zone},
};

fn declare(own: &mut KaitoSession, native: &mut FastActorSessionV1, target: ObjectId) {
    for _ in 0..128 {
        if own.fixture_state().engine.combat.attackers == [target] {
            return;
        }
        let menu = actions(own);
        let selected = menu
            .iter()
            .find(|a| match a {
                A::ChooseAttackerInclusion {
                    attacker, include, ..
                } => *include == (attacker.arena_id == target.0),
                A::DeclareAttackers { attackers, .. } => {
                    attackers.len() == 1 && attackers[0].arena_id == target.0
                }
                _ => false,
            })
            .or_else(|| menu.iter().find(|a| matches!(a, A::Pass { .. })))
            .expect("real Condemn attack declaration absent")
            .clone();
        act(own, native, |a| a == &selected);
    }
    panic!("Condemn attack declaration did not finish");
}

#[test]
fn paid_condemn_actual_attacker_current_toughness_owner_library_controller_life_counter_and_restore_both_seats(
) {
    let id = crate::card_def::card_id_by_name("Condemn").unwrap();
    assert_eq!(
        super::super::super::roots::registered_spell_printing(id).unwrap(),
        Some(super::super::super::roots::baseline(id).unwrap())
    );
    let forest = crate::card_def::card_id_by_name("Forest").unwrap();
    for p in [PlayerId::P0, PlayerId::P1] {
        let attacker = p.opponent();
        for owner in [p, attacker] {
            for counters in [0, 2] {
                let mut s = GameState::new_from_libraries_with_starting_player_v1(
                    &[forest; 40],
                    &[forest; 40],
                    crate::rl::card_name,
                    110_384,
                    attacker,
                );
                s.step = Step::DeclareAttackers;
                let land = put(&mut s, p, "Plains", Zone::Battlefield);
                for _ in 0..2 {
                    put(&mut s, attacker, "Island", Zone::Battlefield);
                }
                put(&mut s, attacker, "Swamp", Zone::Battlefield);
                let target = put(&mut s, owner, "Serra Angel", Zone::Hand);
                event::propose_and_commit(
                    &mut s,
                    ProposedEvent::zone_change_to_battlefield_under_controller(target, attacker),
                );
                s.objects.get_mut(target).summoning_sick = false;
                // Initial counters test current effective toughness, not a paid counter spell.
                s.objects.get_mut(target).counters.plus1_plus1 = counters;
                let idle = put(&mut s, attacker, "Llanowar Elves", Zone::Battlefield);
                let source = put(&mut s, p, "Condemn", Zone::Hand);
                let counter = put(&mut s, attacker, "Counterspell", Zone::Hand);
                let stab = put(&mut s, attacker, "Stab", Zone::Hand);
                let library = s.players[owner.index()].library.clone();
                let (mut own, mut native) = sessions(s);
                declare(&mut own, &mut native, target);
                checkpoint(&mut own, &mut native);
                cast(&mut own, &mut native, source);
                let targets = actions(&own)
                    .into_iter()
                    .filter_map(|a| match a {
                        A::ChooseTarget {
                            target: crate::rl::TargetRefV1::Object { object },
                            ..
                        } => Some(object.arena_id),
                        _ => None,
                    })
                    .collect::<Vec<_>>();
                assert_eq!(targets, [target.0]);
                assert!(!targets.contains(&idle.0));
                checkpoint(&mut own, &mut native);
                choose(&mut own, &mut native, source, target);
                assert_eq!(own.fixture_state().objects.get(source).zone, Zone::Stack);
                assert_eq!(
                    crate::engine::object_mana_value(own.fixture_state(), source),
                    1
                );
                assert!(own.fixture_state().objects.get(land).tapped);
                assert_eq!(own.fixture_state().players[p.index()].mana_pool, [0; 6]);
                let paid = own.snapshot();
                let raw = native.snapshot_v1();
                for response in 0..3 {
                    let mut expected = None;
                    for _ in 0..2 {
                        own.restore(&paid).unwrap();
                        native.kaito_restore_v75(&raw).unwrap();
                        if response != 0 {
                            act(&mut own, &mut native, |a| matches!(a, A::Pass { .. }));
                            if response == 1 {
                                cast(&mut own, &mut native, counter);
                                act(&mut own, &mut native, |a| {
                                    is_original_spell_target(a, source)
                                });
                            } else {
                                cast(&mut own, &mut native, stab);
                                choose(&mut own, &mut native, stab, target);
                            }
                        }
                        settle(&mut own, &mut native);
                        let state = own.fixture_state();
                        assert_eq!(state.objects.get(source).zone, Zone::Graveyard);
                        assert_eq!(state.objects.get(target).owner, owner);
                        assert_eq!(state.objects.get(idle).zone, Zone::Battlefield);
                        assert_eq!(state.players[p.index()].life, 20);
                        if response == 1 {
                            assert_eq!(state.objects.get(target).zone, Zone::Battlefield);
                            assert_eq!(state.players[owner.index()].library, library);
                            assert_eq!(state.players[attacker.index()].life, 20);
                        } else {
                            assert_eq!(state.objects.get(target).zone, Zone::Library);
                            assert_eq!(state.objects.get(target).controller, owner);
                            assert_eq!(
                                state.players[owner.index()].library,
                                [library.clone(), vec![target]].concat()
                            );
                            assert!(!state.engine.combat.attackers.contains(&target));
                            assert_eq!(
                                state.players[attacker.index()].life,
                                24 + counters - if response == 2 { 2 } else { 0 }
                            );
                            assert_eq!(state.objects.get(target).counters.plus1_plus1, 0);
                        }
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
}
