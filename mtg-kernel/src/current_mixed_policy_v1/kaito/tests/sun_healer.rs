use super::*;
use super::{
    surveil_many::{checkpoint, sessions},
    targeted_entry::{actions, cast, choose, wait_target},
};
use crate::{
    card_def::Keywords,
    ids::ObjectId,
    policy_observation_v6::tests::put,
    state::{GameState, Step, Zone},
};
fn cast_route(
    own: &mut KaitoSession,
    native: &mut FastActorSessionV1,
    source: ObjectId,
    p: PlayerId,
    kicked: bool,
    resources: usize,
) {
    cast(own, native, source);
    let menu = actions(own);
    if resources == 4 {
        assert_eq!(
            menu.iter()
                .filter(|a| matches!(a, A::ChooseKicker { .. }))
                .count(),
            2
        );
        checkpoint(own, native);
        act(
            own,
            native,
            |a| matches!(a,A::ChooseKicker{source:s,pay,..}if s.arena_id==source.0&&*pay==kicked),
        );
    } else {
        assert!(!menu.iter().any(|a| matches!(a, A::ChooseKicker { .. })));
    }
    assert_eq!(own.fixture_state().objects.get(source).zone, Zone::Stack);
    assert_eq!(own.fixture_state().stack.last().unwrap().kicked, kicked);
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
        if kicked { 4 } else { 2 }
    );
    assert_eq!(own.fixture_state().players[p.index()].mana_pool, [0; 6]);
}
fn setup(
    p: PlayerId,
    target_name: Option<&str>,
    resources: usize,
) -> (
    GameState,
    ObjectId,
    Option<ObjectId>,
    ObjectId,
    ObjectId,
    ObjectId,
) {
    let forest = crate::card_def::card_id_by_name("Forest").unwrap();
    let mut s = GameState::new_from_libraries_with_starting_player_v1(
        &[forest; 40],
        &[forest; 40],
        crate::rl::card_name,
        110_404,
        p,
    );
    s.step = Step::Main1;
    for _ in 0..resources {
        put(&mut s, p, "Plains", Zone::Battlefield);
    }
    for name in ["Island", "Swamp"] {
        for _ in 0..3 {
            put(&mut s, p.opponent(), name, Zone::Battlefield);
        }
    }
    put(&mut s, p.opponent(), "Forest", Zone::Battlefield);
    let target = target_name.map(|n| put(&mut s, p, n, Zone::Graveyard));
    for n in ["Forest", "Think Twice", "Serra Angel"] {
        put(&mut s, p, n, Zone::Graveyard);
    }
    put(&mut s, p.opponent(), "Llanowar Elves", Zone::Graveyard);
    let source = put(&mut s, p, "Sun-Blessed Healer", Zone::Hand);
    let counter = put(&mut s, p.opponent(), "Counterspell", Zone::Hand);
    let removal = put(&mut s, p.opponent(), "Hero's Downfall", Zone::Hand);
    let ooze = put(&mut s, p.opponent(), "Scavenging Ooze", Zone::Battlefield);
    (s, source, target, counter, removal, ooze)
}
#[test]
fn paid_sun_healer_base_declined_or_paid_kicker_nonland_permanent_filter_no_target_counter_departure_exile_and_restore_both_seats(
) {
    let id = crate::card_def::card_id_by_name("Sun-Blessed Healer").unwrap();
    assert_eq!(
        super::super::super::roots::registered_permanent_printing(id).unwrap(),
        Some(super::super::super::roots::baseline(id).unwrap())
    );
    for p in [PlayerId::P0, PlayerId::P1] {
        for (resources, kicked) in [(2, false), (4, false), (4, true)] {
            for target_name in [None, Some("Llanowar Elves"), Some("Goldvein Pick")] {
                let (s, source, target, counter, removal, ooze) = setup(p, target_name, resources);
                let (mut own, mut native) = sessions(s);
                cast_route(&mut own, &mut native, source, p, kicked, resources);
                let saved = own.snapshot();
                let raw = native.snapshot_v1();
                for response in 0..if kicked && target.is_some() { 4 } else { 2 } {
                    let mut expected = None;
                    for _ in 0..2 {
                        own.restore(&saved).unwrap();
                        native.kaito_restore_v75(&raw).unwrap();
                        if response == 1 {
                            cast(&mut own, &mut native, counter);
                            act(&mut own, &mut native, |a| {
                                is_original_spell_target(a, source)
                            });
                        } else if kicked && target.is_some() {
                            let target = target.unwrap();
                            wait_target(&mut own, &mut native, source, target);
                            let menu = actions(&own);
                            let legal = menu
                                .iter()
                                .filter_map(|a| match a {
                                    A::ChooseTarget {
                                        target: crate::rl::TargetRefV1::Object { object },
                                        ..
                                    }
                                    | A::ChooseEffectTarget {
                                        target: crate::rl::TargetRefV1::Object { object },
                                        ..
                                    } => Some(object.arena_id),
                                    _ => None,
                                })
                                .collect::<Vec<_>>();
                            assert_eq!(legal, [target.0]);
                            checkpoint(&mut own, &mut native);
                            choose(&mut own, &mut native, source, target);
                            if response == 2 {
                                cast(&mut own, &mut native, removal);
                                choose(&mut own, &mut native, removal, source);
                            }
                            if response == 3 {
                                if !actions(&own).iter().any(|a|matches!(a,A::ActivateAbility{source:s,ability_index:0,..}if s.arena_id==ooze.0)){act(&mut own,&mut native,|a|matches!(a,A::Pass{..}));}
                                act(
                                    &mut own,
                                    &mut native,
                                    |a| matches!(a,A::ActivateAbility{source:s,ability_index:0,..}if s.arena_id==ooze.0),
                                );
                                choose(&mut own, &mut native, ooze, target);
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
                        if response != 1 && response != 2 {
                            assert!(crate::engine::has_effective_keyword(
                                state,
                                source,
                                Keywords::LIFELINK
                            ));
                            assert_eq!(
                                crate::engine::checked_effective_pt(state, source).unwrap(),
                                (3, 1)
                            );
                        }
                        if let Some(target) = target {
                            assert_eq!(
                                state.objects.get(target).zone,
                                if response == 3 {
                                    Zone::Exile
                                } else if kicked && response != 1 {
                                    Zone::Battlefield
                                } else {
                                    Zone::Graveyard
                                }
                            );
                            if kicked && response != 1 && response != 3 {
                                assert_eq!(state.objects.get(target).controller, p);
                                assert!(!state.objects.get(target).tapped);
                            }
                        }
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
#[test]
fn paid_kicked_healer_returns_aura_with_untargeted_ward_host_choice_source_departure_and_restore_both_seats(
) {
    for p in [PlayerId::P0, PlayerId::P1] {
        let (mut s, source, target, _, removal, _) = setup(p, Some("Pacifism"), 4);
        let aura = target.unwrap();
        let host = put(&mut s, p.opponent(), "Tolarian Terror", Zone::Battlefield);
        put(&mut s, p, "Savannah Lions", Zone::Battlefield);
        let (mut own, mut native) = sessions(s);
        cast_route(&mut own, &mut native, source, p, true, 4);
        wait_target(&mut own, &mut native, source, aura);
        choose(&mut own, &mut native, source, aura);
        let saved = own.snapshot();
        let raw = native.snapshot_v1();
        for removed in [false, true] {
            let mut expected = None;
            for _ in 0..2 {
                own.restore(&saved).unwrap();
                native.kaito_restore_v75(&raw).unwrap();
                if removed {
                    cast(&mut own, &mut native, removal);
                    choose(&mut own, &mut native, removal, source);
                }
                wait_target(&mut own, &mut native, source, host);
                checkpoint(&mut own, &mut native);
                choose(&mut own, &mut native, source, host);
                settle(&mut own, &mut native);
                let state = own.fixture_state();
                assert_eq!(state.objects.get(aura).zone, Zone::Battlefield);
                assert_eq!(state.objects.get(aura).v4.attached_to.unwrap().object, host);
                assert_eq!(
                    state.objects.get(source).zone,
                    if removed {
                        Zone::Graveyard
                    } else {
                        Zone::Battlefield
                    }
                );
                assert_eq!(state.players[p.index()].mana_pool, [0; 6]);
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
