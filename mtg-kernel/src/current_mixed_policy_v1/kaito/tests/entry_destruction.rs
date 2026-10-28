use super::targeted_entry::{actions, cast, choose, wait_target};
use super::*;
use crate::{
    ids::ObjectId,
    policy_observation_v6::tests::put,
    state::{GameState, Step, Zone},
};

fn targets(a: &A, source: ObjectId, object: ObjectId) -> bool {
    match a {
        A::ChooseEffectTarget {
            source: s,
            target: crate::rl::TargetRefV1::Object { object: o },
            ..
        }
        | A::ChooseTarget {
            source: s,
            target: crate::rl::TargetRefV1::Object { object: o },
            ..
        } => s.arena_id == source.0 && o.arena_id == object.0,
        _ => false,
    }
}
fn finish(
    own: &mut KaitoSession,
    native: &mut FastActorSessionV1,
    source: ObjectId,
    p: PlayerId,
    accept: bool,
) -> usize {
    let mut choices = 0;
    for _ in 0..128 {
        let s = own.fixture_state();
        if s.stack.is_empty()
            && s.engine.pending_effect.is_none()
            && s.engine.pending_triggers.is_empty()
        {
            return choices;
        }
        let menu = actions(own);
        if menu
            .iter()
            .any(|a| matches!(a, A::ChooseEffectOption { source: s, .. } if s.arena_id == source.0))
        {
            for index in [0, 1] {
                assert!(menu.iter().any(|a| matches!(a, A::ChooseEffectOption { actor, source: s, option_index, option_count: 2 } if s.arena_id == source.0 && *actor == crate::rl::PlayerSeatV1::from(p) && *option_index == index)));
            }
            let saved = own.snapshot();
            let raw = native.snapshot_v1();
            own.restore(&saved).unwrap();
            native.kaito_restore_v75(&raw).unwrap();
            act(
                own,
                native,
                |a| matches!(a, A::ChooseEffectOption { source: s, option_index, option_count: 2, .. } if s.arena_id == source.0 && *option_index == u16::from(accept)),
            );
            choices += 1;
        } else {
            act(own, native, |a| {
                matches!(
                    a,
                    A::Pass { .. } | A::OrderTriggers { .. } | A::ChooseTriggerOrderNext { .. }
                )
            });
        }
    }
    panic!("targeted entry destruction did not finish");
}

#[test]
fn paid_golem_opposing_nonland_targets_destroy_source_departure_counter_and_restore_both_seats() {
    paid_destruction(false);
}
#[test]
fn paid_sage_mandatory_target_optional_resolution_own_target_departure_counter_and_restore_both_seats(
) {
    paid_destruction(true);
}

fn paid_destruction(sage: bool) {
    let name = if sage {
        "Reclamation Sage"
    } else {
        "Meteor Golem"
    };
    let id = crate::card_def::card_id_by_name(name).unwrap();
    assert_eq!(
        super::super::super::roots::registered_permanent_printing(id).unwrap(),
        Some(super::super::super::roots::baseline(id).unwrap())
    );
    let forest = crate::card_def::card_id_by_name("Forest").unwrap();
    let price = if sage { 3 } else { 7 };
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut state = GameState::new_from_libraries_with_starting_player_v1(
            &[forest; 40],
            &[forest; 40],
            crate::rl::card_name,
            110_377,
            p,
        );
        state.step = Step::Main1;
        for _ in 0..price {
            put(&mut state, p, "Forest", Zone::Battlefield);
        }
        for land in ["Island", "Swamp", "Forest"] {
            for _ in 0..3 {
                put(&mut state, p.opponent(), land, Zone::Battlefield);
            }
        }
        let other_land = state.players[p.opponent().index()].battlefield[0];
        let own_hawk = put(&mut state, p, "Healer's Hawk", Zone::Battlefield);
        let other_hawk = put(&mut state, p.opponent(), "Healer's Hawk", Zone::Battlefield);
        let own_boots = put(&mut state, p, "Swiftfoot Boots", Zone::Battlefield);
        let other_boots = put(
            &mut state,
            p.opponent(),
            "Swiftfoot Boots",
            Zone::Battlefield,
        );
        let source = put(&mut state, p, name, Zone::Hand);
        let counter = put(&mut state, p.opponent(), "Counterspell", Zone::Hand);
        let downfall = put(&mut state, p.opponent(), "Hero's Downfall", Zone::Hand);
        let wings = put(&mut state, p.opponent(), "Broken Wings", Zone::Hand);
        let mut native = FastActorSessionV1::from_kaito_v75_fixture_state(state.clone());
        let mut own =
            KaitoSession::from_session(FastActorSessionV1::from_kaito_v75_fixture_state(state))
                .unwrap();
        cast(&mut own, &mut native, source);
        assert_eq!(own.fixture_state().objects.get(source).zone, Zone::Stack);
        assert_eq!(
            own.fixture_state().players[p.index()]
                .battlefield
                .iter()
                .filter(|&&o| own.fixture_state().objects.get(o).tapped)
                .count(),
            price
        );
        let saved = own.snapshot();
        let raw = native.snapshot_v1();
        for response in 0..if sage { 6 } else { 5 } {
            let target = if sage && response == 5 {
                own_boots
            } else if !sage && response == 4 {
                other_hawk
            } else {
                other_boots
            };
            let owner = if target == own_boots { p } else { p.opponent() };
            let mut expected = None;
            for _ in 0..2 {
                own.restore(&saved).unwrap();
                native.kaito_restore_v75(&raw).unwrap();
                let generation = own.fixture_state().objects.get(target).zone_change_count;
                let count = if response == 1 {
                    cast(&mut own, &mut native, counter);
                    act(&mut own, &mut native, |a| {
                        is_original_spell_target(a, source)
                    });
                    finish(&mut own, &mut native, source, p, true)
                } else {
                    wait_target(&mut own, &mut native, source, target);
                    let menu = actions(&own);
                    for invalid in [source, own_hawk, other_land] {
                        assert!(!menu.iter().any(|a| targets(a, source, invalid)));
                    }
                    if sage {
                        assert!(menu.iter().any(|a| targets(a, source, own_boots)));
                        assert!(!menu.iter().any(|a| targets(a, source, other_hawk)));
                    } else {
                        assert!(!menu.iter().any(|a| targets(a, source, own_boots)));
                        assert!(menu.iter().any(|a| targets(a, source, other_hawk)));
                    }
                    assert!(menu
                        .iter()
                        .filter(|a| targets(a, source, target))
                        .all(|a| matches!(
                            a,
                            A::ChooseEffectTarget {
                                min_targets: 1,
                                max_targets: 1,
                                ..
                            } | A::ChooseTarget { remaining: 1, .. }
                        )));
                    // A targeted "may" ability still has to select one target.
                    assert!(!menu.iter().any(|a| matches!(a, A::FinishEffectSelection { source: s, selected_count: 0, .. } | A::FinishTargetSelection { source: s, selected_count: 0, .. } if s.arena_id == source.0)));
                    let choosing = own.snapshot();
                    let choosing_raw = native.snapshot_v1();
                    own.restore(&choosing).unwrap();
                    native.kaito_restore_v75(&choosing_raw).unwrap();
                    choose(&mut own, &mut native, source, target);
                    let pending = own.snapshot();
                    let pending_raw = native.snapshot_v1();
                    own.restore(&pending).unwrap();
                    native.kaito_restore_v75(&pending_raw).unwrap();
                    if response == 2 {
                        cast(&mut own, &mut native, downfall);
                        choose(&mut own, &mut native, downfall, source);
                    }
                    if response == 3 {
                        cast(&mut own, &mut native, wings);
                        choose(&mut own, &mut native, wings, target);
                    }
                    finish(&mut own, &mut native, source, p, !(sage && response == 4))
                };
                assert_eq!(count, usize::from(sage && response != 1 && response != 3));
                let s = own.fixture_state();
                let destroyed = response != 1 && !(sage && response == 4);
                let object = s.objects.get(target);
                assert_eq!(
                    object.zone,
                    if destroyed {
                        Zone::Graveyard
                    } else {
                        Zone::Battlefield
                    }
                );
                assert_eq!(object.zone_change_count, generation + u32::from(destroyed));
                assert_eq!(object.owner, owner);
                assert_eq!(object.controller, owner);
                assert_eq!(
                    s.objects.get(source).zone,
                    if response == 1 || response == 2 {
                        Zone::Graveyard
                    } else {
                        Zone::Battlefield
                    }
                );
                assert_eq!(s.players[p.index()].mana_pool, [0; 6]);
                assert_eq!(s.players[p.opponent().index()].mana_pool, [0; 6]);
                assert_eq!(s.players[p.index()].life, 20);
                assert_eq!(s.players[p.opponent().index()].life, 20);
                assert_eq!(
                    s.players[p.opponent().index()]
                        .battlefield
                        .iter()
                        .filter(|&&o| s.objects.get(o).tapped)
                        .count(),
                    match response {
                        1 => 2,
                        2 | 3 => 3,
                        _ => 0,
                    }
                );
                if let Some(ref previous) = expected {
                    assert_eq!(s, previous);
                } else {
                    expected = Some(s.clone());
                }
            }
        }
    }
}

#[test]
fn paid_entry_destruction_without_legal_targets_has_no_resolution_choice_and_keeps_cost_both_seats()
{
    let forest = crate::card_def::card_id_by_name("Forest").unwrap();
    for sage in [false, true] {
        let name = if sage {
            "Reclamation Sage"
        } else {
            "Meteor Golem"
        };
        let price = if sage { 3 } else { 7 };
        for p in [PlayerId::P0, PlayerId::P1] {
            let mut state = GameState::new_from_libraries_with_starting_player_v1(
                &[forest; 40],
                &[forest; 40],
                crate::rl::card_name,
                110_378,
                p,
            );
            state.step = Step::Main1;
            for _ in 0..price {
                put(&mut state, p, "Forest", Zone::Battlefield);
            }
            put(&mut state, p, "Healer's Hawk", Zone::Battlefield);
            put(&mut state, p.opponent(), "Forest", Zone::Battlefield);
            let source = put(&mut state, p, name, Zone::Hand);
            let mut native = FastActorSessionV1::from_kaito_v75_fixture_state(state.clone());
            let mut own =
                KaitoSession::from_session(FastActorSessionV1::from_kaito_v75_fixture_state(state))
                    .unwrap();
            cast(&mut own, &mut native, source);
            let saved = own.snapshot();
            let raw = native.snapshot_v1();
            let mut expected = None;
            for _ in 0..2 {
                own.restore(&saved).unwrap();
                native.kaito_restore_v75(&raw).unwrap();
                assert_eq!(finish(&mut own, &mut native, source, p, true), 0);
                let s = own.fixture_state();
                assert_eq!(s.objects.get(source).zone, Zone::Battlefield);
                assert_eq!(s.players[p.index()].mana_pool, [0; 6]);
                assert_eq!(
                    s.players[p.index()]
                        .battlefield
                        .iter()
                        .filter(|&&o| s.objects.get(o).tapped)
                        .count(),
                    price
                );
                if let Some(ref previous) = expected {
                    assert_eq!(s, previous);
                } else {
                    expected = Some(s.clone());
                }
            }
        }
    }
}
