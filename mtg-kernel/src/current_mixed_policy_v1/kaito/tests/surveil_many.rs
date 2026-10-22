use super::targeted_entry::{actions, cast, choose};
use super::*;
use crate::{
    ids::ObjectId,
    policy_observation_v6::tests::put,
    state::{GameState, Step, Zone},
};
fn quiet(a: &A) -> bool {
    matches!(
        a,
        A::Pass { .. } | A::OrderTriggers { .. } | A::ChooseTriggerOrderNext { .. }
    )
}
pub(super) fn scene(
    p: PlayerId,
    name: &str,
    size: usize,
) -> (GameState, ObjectId, ObjectId, ObjectId) {
    let basics = ["Forest", "Plains", "Island", "Mountain", "Swamp"]
        .map(|name| crate::card_def::card_id_by_name(name).unwrap());
    let own = (0..size)
        .map(|i| basics[i % basics.len()])
        .collect::<Vec<_>>();
    let other = vec![basics[0]; 40];
    let (left, right) = if p == PlayerId::P0 {
        (own, other)
    } else {
        (other, own)
    };
    let mut s = GameState::new_from_libraries_with_starting_player_v1(
        &left,
        &right,
        crate::rl::card_name,
        110_395,
        p,
    );
    s.step = Step::Main1;
    for _ in 0..8 {
        put(&mut s, p, "Island", Zone::Battlefield);
    }
    for land in ["Island", "Swamp"] {
        for _ in 0..3 {
            put(&mut s, p.opponent(), land, Zone::Battlefield);
        }
    }
    put(&mut s, p.opponent(), "Forest", Zone::Battlefield);
    let source = put(&mut s, p, name, Zone::Hand);
    let counter = put(&mut s, p.opponent(), "Counterspell", Zone::Hand);
    let removal = put(&mut s, p.opponent(), "Hero's Downfall", Zone::Hand);
    (s, source, counter, removal)
}
pub(super) fn sessions(s: GameState) -> (KaitoSession, FastActorSessionV1) {
    let native = FastActorSessionV1::from_kaito_v75_fixture_state(s.clone());
    (
        KaitoSession::from_session(FastActorSessionV1::from_kaito_v75_fixture_state(s)).unwrap(),
        native,
    )
}
pub(super) fn checkpoint(own: &mut KaitoSession, native: &mut FastActorSessionV1) {
    let saved = own.snapshot();
    let raw = native.snapshot_v1();
    let expected = own.fixture_state().clone();
    own.restore(&saved).unwrap();
    native.kaito_restore_v75(&raw).unwrap();
    assert_eq!(own.fixture_state(), &expected);
}
fn target(a: &A, source: ObjectId, object: ObjectId) -> bool {
    matches!(a,A::ChooseEffectTarget{source:s,target:crate::rl::TargetRefV1::Object{object:o},..}if s.arena_id==source.0&&o.arena_id==object.0)
}
pub(super) fn wait_selection(
    own: &mut KaitoSession,
    native: &mut FastActorSessionV1,
    source: ObjectId,
    expected: &[ObjectId],
    minimum: u16,
) {
    for _ in 0..128 {
        let menu = actions(own);
        if menu
            .iter()
            .any(|a| matches!(a,A::ChooseEffectTarget{source:s,..}if s.arena_id==source.0))
        {
            let mut actual = menu
                .iter()
                .filter_map(|a| match a {
                    A::ChooseEffectTarget {
                        source: s,
                        target: crate::rl::TargetRefV1::Object { object },
                        ..
                    } if s.arena_id == source.0 => Some(ObjectId(object.arena_id)),
                    _ => None,
                })
                .collect::<Vec<_>>();
            let mut wanted = expected.to_vec();
            actual.sort_unstable();
            wanted.sort_unstable();
            assert_eq!(actual, wanted);
            let FastActorResponseV1::Decision(h) = own.current_response() else {
                panic!()
            };
            let d = own.encode(h).unwrap();
            let choice = d
                .backend
                .observation()
                .common
                .common
                .projection
                .engine_context
                .pending_effect
                .as_ref()
                .unwrap()
                .choice
                .as_ref()
                .unwrap();
            let crate::rl::PendingEffectChoiceSemanticV4::Targets {
                min_targets,
                max_targets,
                ..
            } = choice
            else {
                panic!("private partition prompt")
            };
            assert_eq!(
                (*min_targets, *max_targets),
                (minimum, expected.len() as u16)
            );
            checkpoint(own, native);
            return;
        }
        act(own, native, quiet);
    }
    panic!("actual private partition/order prompt absent");
}
pub(super) fn order(
    own: &mut KaitoSession,
    native: &mut FastActorSessionV1,
    source: ObjectId,
    objects: &[ObjectId],
) {
    for (index, &object) in objects.iter().enumerate() {
        if !actions(own).iter().any(|a| target(a, source, object)) {
            // Native ordering appends its forced final card without a policy action.
            assert_eq!(index + 1, objects.len());
            break;
        }
        act(own, native, |a| target(a, source, object));
        checkpoint(own, native);
    }
    if actions(own)
        .iter()
        .any(|a| matches!(a,A::FinishEffectSelection{source:s,..}if s.arena_id==source.0))
    {
        act(
            own,
            native,
            |a| matches!(a,A::FinishEffectSelection{source:s,..}if s.arena_id==source.0),
        );
        checkpoint(own, native);
    }
}
fn partition(
    own: &mut KaitoSession,
    native: &mut FastActorSessionV1,
    p: PlayerId,
    source: ObjectId,
    looked: &[ObjectId],
    graveyard: &[ObjectId],
    kept: &[ObjectId],
) {
    wait_selection(own, native, source, looked, 0);
    let library = own.fixture_state().players[p.index()].library.clone();
    let old_graveyard = own.fixture_state().players[p.index()].graveyard.clone();
    order(own, native, source, graveyard);
    if kept.len() >= 2 {
        assert_eq!(own.fixture_state().players[p.index()].library, library);
        assert_eq!(
            own.fixture_state().players[p.index()].graveyard,
            old_graveyard
        );
        wait_selection(own, native, source, kept, kept.len() as u16);
        order(own, native, source, kept);
    }
    settle(own, native);
    let expected_library = kept
        .iter()
        .copied()
        .chain(library[looked.len()..].iter().copied())
        .collect::<Vec<_>>();
    let expected_graveyard = old_graveyard
        .into_iter()
        .chain(graveyard.iter().copied())
        .collect::<Vec<_>>();
    assert_eq!(
        own.fixture_state().players[p.index()].library,
        expected_library
    );
    assert_eq!(
        own.fixture_state().players[p.index()].graveyard,
        expected_graveyard
    );
    assert_eq!(own.fixture_state().players[p.index()].life, 20);
}
fn paid_partitions(name: &str, count: usize) {
    let id = crate::card_def::card_id_by_name(name).unwrap();
    assert_eq!(
        super::super::super::roots::registered_permanent_printing(id).unwrap(),
        Some(super::super::super::roots::baseline(id).unwrap())
    );
    for p in [PlayerId::P0, PlayerId::P1] {
        let (s, source, _, _) = scene(p, name, 40);
        let (mut own, mut native) = sessions(s);
        cast(&mut own, &mut native, source);
        let paid = own.snapshot();
        let raw = native.snapshot_v1();
        for mask in 0..(1 << count) {
            for reverse_kept in [false, true] {
                let mut expected = None;
                for _ in 0..2 {
                    own.restore(&paid).unwrap();
                    native.kaito_restore_v75(&raw).unwrap();
                    let looked = own.fixture_state().players[p.index()].library[..count].to_vec();
                    let graveyard = looked
                        .iter()
                        .enumerate()
                        .rev()
                        .filter(|(i, _)| mask & (1 << i) != 0)
                        .map(|(_, &o)| o)
                        .collect::<Vec<_>>();
                    let mut kept = looked
                        .iter()
                        .copied()
                        .filter(|o| !graveyard.contains(o))
                        .collect::<Vec<_>>();
                    if reverse_kept {
                        kept.reverse();
                    }
                    partition(&mut own, &mut native, p, source, &looked, &graveyard, &kept);
                    assert_eq!(
                        own.fixture_state().objects.get(source).zone,
                        Zone::Battlefield
                    );
                    assert_eq!(own.fixture_state().players[p.index()].mana_pool, [0; 6]);
                    if let Some(ref previous) = expected {
                        assert_eq!(own.fixture_state(), previous);
                    } else {
                        expected = Some(own.fixture_state().clone());
                    }
                }
            }
        }
    }
}
#[test]
fn paid_duo_all_two_card_partitions_kept_orders_and_each_private_restore_both_seats() {
    paid_partitions("Lightshell Duo", 2);
}
#[test]
fn paid_inkmage_all_three_card_partitions_kept_orders_and_each_private_restore_both_seats() {
    paid_partitions("Cephalid Inkmage", 3);
}
#[test]
fn paid_both_surveil_printings_counter_short_empty_source_departure_and_restore_both_seats() {
    for (name, count) in [("Lightshell Duo", 2), ("Cephalid Inkmage", 3)] {
        for p in [PlayerId::P0, PlayerId::P1] {
            for size in 0..count {
                let (s, source, counter, removal) = scene(p, name, size);
                let (mut own, mut native) = sessions(s);
                cast(&mut own, &mut native, source);
                let paid = own.snapshot();
                let raw = native.snapshot_v1();
                for branch in 0..3 {
                    let mut expected = None;
                    for _ in 0..2 {
                        own.restore(&paid).unwrap();
                        native.kaito_restore_v75(&raw).unwrap();
                        let library = own.fixture_state().players[p.index()].library.clone();
                        if branch == 0 {
                            cast(&mut own, &mut native, counter);
                            act(&mut own, &mut native, |a| {
                                is_original_spell_target(a, source)
                            });
                            settle(&mut own, &mut native);
                            assert_eq!(own.fixture_state().players[p.index()].library, library);
                        } else {
                            if branch == 2 {
                                super::library_search::wait_entered(&mut own, &mut native, source);
                                cast(&mut own, &mut native, removal);
                                choose(&mut own, &mut native, removal, source);
                            }
                            if size == 0 {
                                settle(&mut own, &mut native);
                            } else {
                                let mut moved = library.clone();
                                moved.reverse();
                                partition(&mut own, &mut native, p, source, &library, &moved, &[]);
                            }
                            assert!(own.fixture_state().players[p.index()].library.is_empty());
                        }
                        assert_eq!(
                            own.fixture_state().objects.get(source).zone,
                            if branch == 1 {
                                Zone::Battlefield
                            } else {
                                Zone::Graveyard
                            }
                        );
                        assert_eq!(own.fixture_state().players[p.index()].life, 20);
                        if let Some(ref previous) = expected {
                            assert_eq!(own.fixture_state(), previous);
                        } else {
                            expected = Some(own.fixture_state().clone());
                        }
                    }
                }
            }
        }
    }
}
#[test]
fn paid_duo_prowess_survives_countered_spell_expires_naturally_and_loses_departed_source_both_seats(
) {
    for p in [PlayerId::P0, PlayerId::P1] {
        let (mut s, source, counter, removal) = scene(p, "Lightshell Duo", 40);
        let spell = put(&mut s, p, "Think Twice", Zone::Hand);
        let (mut own, mut native) = sessions(s);
        cast(&mut own, &mut native, source);
        let looked = own.fixture_state().players[p.index()].library[..2].to_vec();
        partition(&mut own, &mut native, p, source, &looked, &[], &looked);
        let ready = own.snapshot();
        let raw = native.snapshot_v1();
        for removed in [false, true] {
            let mut expected = None;
            for _ in 0..2 {
                own.restore(&ready).unwrap();
                native.kaito_restore_v75(&raw).unwrap();
                let library = own.fixture_state().players[p.index()].library.clone();
                cast(&mut own, &mut native, spell);
                checkpoint(&mut own, &mut native);
                cast(&mut own, &mut native, counter);
                act(&mut own, &mut native, |a| {
                    is_original_spell_target(a, spell)
                });
                if removed {
                    cast(&mut own, &mut native, removal);
                    choose(&mut own, &mut native, removal, source);
                }
                settle(&mut own, &mut native);
                assert_eq!(own.fixture_state().objects.get(spell).zone, Zone::Graveyard);
                assert_eq!(own.fixture_state().players[p.index()].library, library);
                if !removed {
                    assert_eq!(
                        crate::engine::checked_effective_pt(own.fixture_state(), source).unwrap(),
                        (4, 5)
                    );
                    super::ordinary_equipment::reach_main(&mut own, &mut native, p.opponent());
                    assert_eq!(
                        crate::engine::checked_effective_pt(own.fixture_state(), source).unwrap(),
                        (3, 4)
                    );
                } else {
                    assert_eq!(
                        own.fixture_state().objects.get(source).zone,
                        Zone::Graveyard
                    );
                }
                if let Some(ref previous) = expected {
                    assert_eq!(own.fixture_state(), previous);
                } else {
                    expected = Some(own.fixture_state().clone());
                }
            }
        }
    }
}
#[test]
fn paid_inkmage_threshold_crosses_seven_then_real_ooze_exile_removes_unblockable_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let (mut s, source, _, _) = scene(p, "Cephalid Inkmage", 40);
        for _ in 0..6 {
            put(&mut s, p, "Forest", Zone::Graveyard);
        }
        for _ in 0..7 {
            put(&mut s, p.opponent(), "Forest", Zone::Graveyard);
        }
        let ooze = put(&mut s, p.opponent(), "Scavenging Ooze", Zone::Battlefield);
        let (mut own, mut native) = sessions(s);
        cast(&mut own, &mut native, source);
        super::library_search::wait_entered(&mut own, &mut native, source);
        assert!(!crate::engine::has_effective_keyword(
            own.fixture_state(),
            source,
            crate::card_def::Keywords::CANT_BE_BLOCKED
        ));
        let looked = own.fixture_state().players[p.index()].library[..3].to_vec();
        partition(
            &mut own,
            &mut native,
            p,
            source,
            &looked,
            &looked[..1],
            &looked[1..],
        );
        assert!(crate::engine::has_effective_keyword(
            own.fixture_state(),
            source,
            crate::card_def::Keywords::CANT_BE_BLOCKED
        ));
        let ready = own.snapshot();
        let raw = native.snapshot_v1();
        let mut expected = None;
        for _ in 0..2 {
            own.restore(&ready).unwrap();
            native.kaito_restore_v75(&raw).unwrap();
            let activation = |a: &A| matches!(a,A::ActivateAbility{source:s,ability_index:0,..}if s.arena_id==ooze.0);
            if !actions(&own).iter().any(activation) {
                act(&mut own, &mut native, |a| matches!(a, A::Pass { .. }));
            }
            act(&mut own, &mut native, activation);
            choose(&mut own, &mut native, ooze, looked[0]);
            checkpoint(&mut own, &mut native);
            settle(&mut own, &mut native);
            assert_eq!(own.fixture_state().objects.get(looked[0]).zone, Zone::Exile);
            assert_eq!(own.fixture_state().players[p.index()].graveyard.len(), 6);
            assert!(!crate::engine::has_effective_keyword(
                own.fixture_state(),
                source,
                crate::card_def::Keywords::CANT_BE_BLOCKED
            ));
            assert_eq!(own.fixture_state().players[p.opponent().index()].life, 20);
            assert_eq!(
                own.fixture_state().objects.get(ooze).counters.plus1_plus1,
                0
            );
            if let Some(ref previous) = expected {
                assert_eq!(own.fixture_state(), previous);
            } else {
                expected = Some(own.fixture_state().clone());
            }
        }
    }
}
