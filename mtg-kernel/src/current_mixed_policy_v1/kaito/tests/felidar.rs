use super::*;
use super::{
    surveil_many::{checkpoint, sessions},
    targeted_entry::{actions, cast, choose},
};
use crate::{
    card_def::Keywords,
    ids::ObjectId,
    policy_observation_v6::tests::put,
    state::{GameState, Step, Zone},
};

fn target(a: &A, source: ObjectId, expected: ObjectId) -> bool {
    match a {
        A::ChooseTarget {
            source: s,
            target: crate::rl::TargetRefV1::Object { object },
            ..
        }
        | A::ChooseEffectTarget {
            source: s,
            target: crate::rl::TargetRefV1::Object { object },
            ..
        } => s.arena_id == source.0 && object.arena_id == expected.0,
        _ => false,
    }
}
#[test]
fn paid_felidar_zero_one_two_distinct_other_targets_partial_choice_illegality_source_departure_counter_and_restore_both_seats(
) {
    let id = crate::card_def::card_id_by_name("Felidar Savior").unwrap();
    assert_eq!(
        super::super::super::roots::registered_permanent_printing(id).unwrap(),
        Some(super::super::super::roots::baseline(id).unwrap())
    );
    let forest = crate::card_def::card_id_by_name("Forest").unwrap();
    for p in [PlayerId::P0, PlayerId::P1] {
        for count in 0usize..=2 {
            let mut s = GameState::new_from_libraries_with_starting_player_v1(
                &[forest; 40],
                &[forest; 40],
                crate::rl::card_name,
                110_396,
                p,
            );
            s.step = Step::Main1;
            let lands = (0..4)
                .map(|_| put(&mut s, p, "Plains", Zone::Battlefield))
                .collect::<Vec<_>>();
            for name in ["Island", "Swamp"] {
                for _ in 0..3 {
                    put(&mut s, p.opponent(), name, Zone::Battlefield);
                }
            }
            // Initial creatures/counters are public support operands. The
            // actual Savior cast, targeting, responses and counters are native.
            let targets = [
                put(&mut s, p, "Llanowar Elves", Zone::Battlefield),
                put(&mut s, p, "Rat Token", Zone::Battlefield),
            ];
            s.objects.get_mut(targets[0]).tapped = true;
            s.objects.get_mut(targets[1]).counters.plus1_plus1 = 2;
            let foreign = put(&mut s, p.opponent(), "Healer's Hawk", Zone::Battlefield);
            let source = put(&mut s, p, "Felidar Savior", Zone::Hand);
            let counter = put(&mut s, p.opponent(), "Counterspell", Zone::Hand);
            let removal = put(&mut s, p.opponent(), "Hero's Downfall", Zone::Hand);
            let (mut own, mut native) = sessions(s);
            cast(&mut own, &mut native, source);
            assert_eq!(own.fixture_state().objects.get(source).zone, Zone::Stack);
            assert_eq!(
                crate::engine::object_mana_value(own.fixture_state(), source),
                4
            );
            assert!(lands
                .iter()
                .all(|&o| own.fixture_state().objects.get(o).tapped));
            assert_eq!(own.fixture_state().players[p.index()].mana_pool, [0; 6]);
            let paid = own.snapshot();
            let paid_raw = native.snapshot_v1();
            if count == 0 {
                cast(&mut own, &mut native, counter);
                act(&mut own, &mut native, |a| {
                    is_original_spell_target(a, source)
                });
                settle(&mut own, &mut native);
                assert_eq!(
                    own.fixture_state().objects.get(source).zone,
                    Zone::Graveyard
                );
                assert_eq!(
                    own.fixture_state()
                        .objects
                        .get(targets[0])
                        .counters
                        .plus1_plus1,
                    0
                );
                assert_eq!(
                    own.fixture_state()
                        .objects
                        .get(targets[1])
                        .counters
                        .plus1_plus1,
                    2
                );
                checkpoint(&mut own, &mut native);
                own.restore(&paid).unwrap();
                native.kaito_restore_v75(&paid_raw).unwrap();
            }
            for _ in 0..128 {
                if actions(&own).iter().any(|a| target(a, source, targets[0])) {
                    break;
                }
                act(&mut own, &mut native, |a| {
                    matches!(
                        a,
                        A::Pass { .. } | A::OrderTriggers { .. } | A::ChooseTriggerOrderNext { .. }
                    )
                });
            }
            assert_eq!(
                own.fixture_state().objects.get(source).zone,
                Zone::Battlefield
            );
            assert!(crate::engine::has_effective_keyword(
                own.fixture_state(),
                source,
                Keywords::LIFELINK
            ));
            let menu = actions(&own);
            for excluded in [source, foreign, lands[0]] {
                assert!(!menu.iter().any(|a| target(a, source, excluded)));
            }
            assert!(menu.iter().any(|a| target(a, source, targets[0])));
            assert!(menu.iter().any(|a| target(a, source, targets[1])));
            if count > 0 {
                act(&mut own, &mut native, |a| target(a, source, targets[0]));
                assert!(!actions(&own).iter().any(|a| target(a, source, targets[0])));
                checkpoint(&mut own, &mut native);
            }
            if count == 2 {
                act(&mut own, &mut native, |a| target(a, source, targets[1]));
            } else {
                act(
                    &mut own,
                    &mut native,
                    |a| matches!(a,A::FinishTargetSelection{source:s,selected_count,..}|A::FinishEffectSelection{source:s,selected_count,..}if s.arena_id==source.0&&usize::from(*selected_count)==count),
                );
            }
            let chosen = own.snapshot();
            let raw = native.snapshot_v1();
            for response in 0..4 {
                if response == 1 && count == 0 || response == 2 && count < 2 {
                    continue;
                }
                let mut expected = None;
                for _ in 0..2 {
                    own.restore(&chosen).unwrap();
                    native.kaito_restore_v75(&raw).unwrap();
                    if response != 0 {
                        cast(&mut own, &mut native, removal);
                        choose(
                            &mut own,
                            &mut native,
                            removal,
                            match response {
                                1 => targets[0],
                                2 => targets[1],
                                _ => source,
                            },
                        );
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
                    for (at, &o) in targets.iter().enumerate() {
                        let removed = response == at + 1;
                        assert_eq!(
                            state.objects.get(o).zone,
                            if removed {
                                Zone::Graveyard
                            } else {
                                Zone::Battlefield
                            }
                        );
                        if !removed {
                            assert_eq!(
                                state.objects.get(o).counters.plus1_plus1,
                                if at == 1 { 2 } else { 0 } + i32::from(at < count)
                            );
                        }
                    }
                    assert_eq!(state.objects.get(source).counters.plus1_plus1, 0);
                    assert_eq!(state.objects.get(foreign).counters.plus1_plus1, 0);
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
