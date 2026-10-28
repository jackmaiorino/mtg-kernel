use super::*;
use super::{
    library_search::optional,
    surveil_many::{checkpoint, sessions},
    targeted_entry::{cast, choose, wait_target},
};
use crate::{
    ids::ObjectId,
    policy_observation_v6::tests::put,
    state::{GameState, Step, Zone},
};

fn scene(
    p: PlayerId,
    name: Option<&str>,
) -> (GameState, ObjectId, Option<ObjectId>, ObjectId, ObjectId) {
    let forest = crate::card_def::card_id_by_name("Forest").unwrap();
    let mut s = GameState::new_from_libraries_with_starting_player_v1(
        &[forest; 40],
        &[forest; 40],
        crate::rl::card_name,
        110_389,
        p,
    );
    s.step = Step::Main1;
    for _ in 0..6 {
        put(&mut s, p, "Forest", Zone::Battlefield);
    }
    for _ in 0..3 {
        put(&mut s, p.opponent(), "Swamp", Zone::Battlefield);
    }
    for _ in 0..2 {
        put(&mut s, p.opponent(), "Island", Zone::Battlefield);
    }
    let target = name.map(|name| put(&mut s, p.opponent(), name, Zone::Battlefield));
    let source = put(&mut s, p, "Affectionate Indrik", Zone::Hand);
    let counter = put(&mut s, p.opponent(), "Counterspell", Zone::Hand);
    let removal = put(&mut s, p.opponent(), "Hero's Downfall", Zone::Hand);
    (s, source, target, counter, removal)
}

#[test]
fn paid_indrik_mandatory_target_optional_fight_simultaneous_lifelink_source_loss_counter_and_restore_both_seats(
) {
    let id = crate::card_def::card_id_by_name("Affectionate Indrik").unwrap();
    assert_eq!(
        super::super::super::roots::registered_permanent_printing(id).unwrap(),
        Some(super::super::super::roots::baseline(id).unwrap())
    );
    for p in [PlayerId::P0, PlayerId::P1] {
        for name in ["Serra Angel", "Healer's Hawk"] {
            let (s, source, target, counter, removal) = scene(p, Some(name));
            let target = target.unwrap();
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
                for accept in [false, true] {
                    if matches!(response, 1 | 3) && !accept {
                        continue;
                    }
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
                            if response != 3 {
                                optional(&mut own, &mut native, source, accept);
                            }
                        }
                        settle(&mut own, &mut native);
                        let state = own.fixture_state();
                        let fought = response == 0 && accept;
                        assert_eq!(
                            state.objects.get(source).zone,
                            if response == 1 || response == 2 || fought && name == "Serra Angel" {
                                Zone::Graveyard
                            } else {
                                Zone::Battlefield
                            }
                        );
                        assert_eq!(
                            state.objects.get(target).zone,
                            if response == 3 || fought {
                                Zone::Graveyard
                            } else {
                                Zone::Battlefield
                            }
                        );
                        if state.objects.get(source).zone == Zone::Battlefield {
                            assert_eq!(state.objects.get(source).damage, u32::from(fought));
                            assert_eq!(
                                crate::engine::checked_effective_pt(state, source).unwrap(),
                                (4, 4)
                            );
                        }
                        if state.objects.get(target).zone == Zone::Battlefield {
                            assert_eq!(state.objects.get(target).damage, 0);
                        }
                        assert_eq!(state.players[p.index()].life, 20);
                        assert_eq!(
                            state.players[p.opponent().index()].life,
                            if fought && name == "Healer's Hawk" {
                                21
                            } else {
                                20
                            }
                        );
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

#[test]
fn paid_indrik_without_opposing_creature_keeps_body_and_cost_without_target_or_fight_choice_both_seats(
) {
    for p in [PlayerId::P0, PlayerId::P1] {
        let (mut s, source, _, _, _) = scene(p, None);
        let friendly = put(&mut s, p, "Healer's Hawk", Zone::Battlefield);
        let (mut own, mut native) = sessions(s);
        cast(&mut own, &mut native, source);
        let paid = own.snapshot();
        let raw = native.snapshot_v1();
        let mut expected = None;
        for _ in 0..2 {
            own.restore(&paid).unwrap();
            native.kaito_restore_v75(&raw).unwrap();
            // settle accepts no optional-cost answer. An invented may or
            // target prompt therefore fails this actual paid no-target case.
            settle(&mut own, &mut native);
            let state = own.fixture_state();
            assert_eq!(state.objects.get(source).zone, Zone::Battlefield);
            assert_eq!(state.objects.get(friendly).zone, Zone::Battlefield);
            assert_eq!(state.objects.get(friendly).damage, 0);
            assert_eq!(state.objects.get(source).damage, 0);
            assert_eq!(
                state.players[p.index()]
                    .battlefield
                    .iter()
                    .filter(|&&o| state.objects.get(o).tapped)
                    .count(),
                6
            );
            if let Some(ref old) = expected {
                assert_eq!(state, old);
            } else {
                expected = Some(state.clone());
            }
        }
    }
}
