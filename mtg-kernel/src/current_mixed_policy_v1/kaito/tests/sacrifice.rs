use super::surveil_many::{checkpoint, sessions};
use super::targeted_entry::{actions, cast, choose};
use super::*;
use crate::{
    ids::ObjectId,
    policy_observation_v6::tests::put,
    state::{GameState, Step, Zone},
};

fn activation(a: &A, source: ObjectId) -> bool {
    matches!(a,A::ActivateAbility{source:s,ability_index:0,..}if s.arena_id==source.0)
}
fn priority(own: &mut KaitoSession, native: &mut FastActorSessionV1, source: ObjectId) {
    if !actions(own).iter().any(|a| activation(a, source)) {
        act(own, native, |a| matches!(a, A::Pass { .. }));
    }
    assert!(actions(own).iter().any(|a| activation(a, source)));
}

#[test]
fn paid_firebrand_haste_sacrifice_each_players_creature_or_player_target_departure_counter_restore_both_seats(
) {
    paid("Fanatical Firebrand");
}
#[test]
fn paid_commando_opposing_turn_flash_sacrifice_each_players_artifact_or_enchantment_departure_counter_restore_both_seats(
) {
    paid("Cathar Commando");
}
fn paid(name: &str) {
    let firebrand = name == "Fanatical Firebrand";
    let id = crate::card_def::card_id_by_name(name).unwrap();
    assert_eq!(
        super::super::super::roots::registered_permanent_printing(id).unwrap(),
        Some(super::super::super::roots::baseline(id).unwrap())
    );
    let island = crate::card_def::card_id_by_name("Island").unwrap();
    for p in [PlayerId::P0, PlayerId::P1] {
        let q = p.opponent();
        for target_controller in [p, q] {
            // Firebrand covers both player and creature kinds. Commando covers
            // both printed type alternatives, including our own permanents.
            for second_kind in [false, true] {
                let player_target = firebrand && second_kind;
                let active = if firebrand { p } else { q };
                let mut s = GameState::new_from_libraries_with_starting_player_v1(
                    &[island; 40],
                    &[island; 40],
                    crate::rl::card_name,
                    110_400,
                    active,
                );
                s.step = Step::Main1;
                for land in ["Plains", "Mountain"] {
                    for _ in 0..3 {
                        put(&mut s, p, land, Zone::Battlefield);
                    }
                }
                for land in ["Island", "Forest"] {
                    for _ in 0..3 {
                        put(&mut s, q, land, Zone::Battlefield);
                    }
                }
                let source = put(&mut s, p, name, Zone::Hand);
                let counter = put(&mut s, q, "Counterspell", Zone::Hand);
                let removal = put(
                    &mut s,
                    q,
                    if firebrand {
                        "Unsummon"
                    } else {
                        "Broken Wings"
                    },
                    Zone::Hand,
                );
                let target_name = if firebrand {
                    "Savannah Lions"
                } else if second_kind {
                    "Garruk's Uprising"
                } else {
                    "Goldvein Pick"
                };
                let target = put(&mut s, target_controller, target_name, Zone::Battlefield);
                let (mut own, mut native) = sessions(s);
                cast(&mut own, &mut native, source);
                assert_eq!(own.fixture_state().active_player, active);
                let cast_saved = own.snapshot();
                let cast_raw = native.snapshot_v1();
                // A real spell counter consumes the original printed payment.
                let mut previous = None;
                for _ in 0..2 {
                    own.restore(&cast_saved).unwrap();
                    native.kaito_restore_v75(&cast_raw).unwrap();
                    cast(&mut own, &mut native, counter);
                    act(&mut own, &mut native, |a| {
                        is_original_spell_target(a, source)
                    });
                    settle(&mut own, &mut native);
                    assert_eq!(
                        own.fixture_state().objects.get(source).zone,
                        Zone::Graveyard
                    );
                    assert_eq!(own.fixture_state().players[p.index()].mana_pool, [0; 6]);
                    if let Some(ref old) = previous {
                        assert_eq!(own.fixture_state(), old);
                    } else {
                        previous = Some(own.fixture_state().clone());
                    }
                }
                own.restore(&cast_saved).unwrap();
                native.kaito_restore_v75(&cast_raw).unwrap();
                settle(&mut own, &mut native);
                priority(&mut own, &mut native, source);
                assert!(own.fixture_state().objects.get(source).summoning_sick);
                let ready = own.snapshot();
                let ready_raw = native.snapshot_v1();
                for departed in [false, true] {
                    if player_target && departed {
                        continue;
                    }
                    let mut expected = None;
                    for _ in 0..2 {
                        own.restore(&ready).unwrap();
                        native.kaito_restore_v75(&ready_raw).unwrap();
                        let tapped_before = own.fixture_state().players[p.index()]
                            .battlefield
                            .iter()
                            .filter(|&&o| own.fixture_state().objects.get(o).tapped)
                            .count();
                        act(&mut own, &mut native, |a| activation(a, source));
                        // Announcing the target prompt has not sacrificed the source.
                        assert_eq!(
                            own.fixture_state().objects.get(source).zone,
                            Zone::Battlefield
                        );
                        checkpoint(&mut own, &mut native);
                        if player_target {
                            act(
                                &mut own,
                                &mut native,
                                |a| matches!(a,A::ChooseTarget{target:crate::rl::TargetRefV1::Player{player},..}if *player==crate::rl::PlayerSeatV1::from(target_controller)),
                            );
                        } else {
                            choose(&mut own, &mut native, source, target);
                        }
                        assert_eq!(
                            own.fixture_state().objects.get(source).zone,
                            Zone::Graveyard
                        );
                        assert_eq!(own.fixture_state().players[p.index()].mana_pool, [0; 6]);
                        let tapped_after = own.fixture_state().players[p.index()]
                            .battlefield
                            .iter()
                            .filter(|&&o| own.fixture_state().objects.get(o).tapped)
                            .count();
                        assert_eq!(tapped_after, tapped_before + usize::from(!firebrand));
                        assert_eq!(
                            own.fixture_state().objects.get(target).zone,
                            Zone::Battlefield
                        );
                        assert_eq!(
                            own.fixture_state().players[target_controller.index()].life,
                            20
                        );
                        checkpoint(&mut own, &mut native);
                        if departed {
                            cast(&mut own, &mut native, removal);
                            choose(&mut own, &mut native, removal, target);
                        }
                        settle(&mut own, &mut native);
                        let s = own.fixture_state();
                        assert_eq!(s.objects.get(source).zone, Zone::Graveyard);
                        assert_eq!(
                            s.objects.get(target).zone,
                            if player_target {
                                Zone::Battlefield
                            } else if departed && firebrand {
                                Zone::Hand
                            } else {
                                Zone::Graveyard
                            }
                        );
                        assert_eq!(
                            s.players[target_controller.index()].life,
                            if player_target { 19 } else { 20 }
                        );
                        assert!(!actions(&own).iter().any(|a| activation(a, source)));
                        if let Some(ref old) = expected {
                            assert_eq!(s, old);
                        } else {
                            expected = Some(s.clone());
                        }
                    }
                }
            }
        }
    }
}
