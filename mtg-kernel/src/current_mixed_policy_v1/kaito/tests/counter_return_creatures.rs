use super::*;
use super::{
    surveil_many::{checkpoint, scene, sessions},
    targeted_entry::{actions, cast, choose},
};
use crate::{ids::ObjectId, policy_observation_v6::tests::put, state::Zone};

fn activation(a: &A, source: ObjectId) -> bool {
    matches!(a, A::ActivateAbility { source:s, ability_index:0, .. } if s.arena_id==source.0)
}
fn activate(own: &mut KaitoSession, native: &mut FastActorSessionV1, source: ObjectId) {
    for _ in 0..4 {
        if actions(own).iter().any(|a| activation(a, source)) {
            act(own, native, |a| activation(a, source));
            return;
        }
        act(own, native, |a| matches!(a, A::Pass { .. }));
    }
    panic!("missing printed activation");
}
fn tapped(own: &KaitoSession, p: PlayerId) -> usize {
    own.fixture_state().players[p.index()]
        .battlefield
        .iter()
        .filter(|&&id| own.fixture_state().objects.get(id).tapped)
        .count()
}
fn registered(name: &str) {
    let id = crate::card_def::card_id_by_name(name).unwrap();
    assert_eq!(
        super::super::super::roots::registered_permanent_printing(id).unwrap(),
        Some(super::super::super::roots::baseline(id).unwrap())
    );
}

#[test]
fn paid_skeleton_cast_counter_death_tapped_graveyard_return_duplicate_activation_and_restore_both_seats(
) {
    registered("Reassembling Skeleton");
    for p in [PlayerId::P0, PlayerId::P1] {
        let (mut s, source, counter, _) = scene(p, "Reassembling Skeleton", 40);
        for _ in 0..10 {
            put(&mut s, p, "Swamp", Zone::Battlefield);
        }
        let removal = put(&mut s, p, "Hero's Downfall", Zone::Hand);
        let (mut own, mut native) = sessions(s);
        cast(&mut own, &mut native, source);
        assert_eq!(tapped(&own, p), 2);
        let paid = own.snapshot();
        let raw = native.snapshot_v1();
        for countered in [false, true] {
            own.restore(&paid).unwrap();
            native.kaito_restore_v75(&raw).unwrap();
            if countered {
                cast(&mut own, &mut native, counter);
                act(&mut own, &mut native, |a| {
                    is_original_spell_target(a, source)
                });
            }
            settle(&mut own, &mut native);
            if !countered {
                assert_eq!(
                    own.fixture_state().objects.get(source).zone,
                    Zone::Battlefield
                );
                assert!(
                    !actions(&own).iter().any(|a| activation(a, source)),
                    "graveyard ability leaked onto battlefield"
                );
                cast(&mut own, &mut native, removal);
                choose(&mut own, &mut native, removal, source);
                settle(&mut own, &mut native);
            }
            assert_eq!(
                own.fixture_state().objects.get(source).zone,
                Zone::Graveyard
            );
            let incarnation = own.fixture_state().objects.get(source).zone_change_count;
            let before = tapped(&own, p);
            activate(&mut own, &mut native, source);
            assert_eq!(tapped(&own, p), before + 2);
            checkpoint(&mut own, &mut native);
            let held = own.snapshot();
            let held_raw = native.snapshot_v1();
            for duplicate in [false, true] {
                let mut expected = None;
                for _ in 0..2 {
                    own.restore(&held).unwrap();
                    native.kaito_restore_v75(&held_raw).unwrap();
                    if duplicate {
                        activate(&mut own, &mut native, source);
                    }
                    settle(&mut own, &mut native);
                    let object = own.fixture_state().objects.get(source);
                    assert_eq!(object.zone, Zone::Battlefield);
                    assert_eq!(
                        object.zone_change_count,
                        incarnation + 1,
                        "the older activation must not return a new incarnation"
                    );
                    assert_eq!(object.controller, p);
                    assert!(object.tapped && object.summoning_sick);
                    assert_eq!(tapped(&own, p), before + 2 + usize::from(duplicate) * 2 + 1);
                    if let Some(expected) = &expected {
                        assert_eq!(own.fixture_state(), expected);
                    } else {
                        expected = Some(own.fixture_state().clone());
                    }
                }
            }
        }
    }
}

#[test]
fn paid_snarespinner_sorcery_controlled_target_counter_source_departure_repeat_and_restore_both_seats(
) {
    registered("Treetop Snarespinner");
    for p in [PlayerId::P0, PlayerId::P1] {
        let (mut s, source, counter, removal) = scene(p, "Treetop Snarespinner", 40);
        for _ in 0..12 {
            put(&mut s, p, "Forest", Zone::Battlefield);
        }
        let other = put(&mut s, p, "Serra Angel", Zone::Battlefield);
        let opponent = put(&mut s, p.opponent(), "Serra Angel", Zone::Battlefield);
        let (mut own, mut native) = sessions(s);
        cast(&mut own, &mut native, source);
        assert_eq!(tapped(&own, p), 4);
        let paid = own.snapshot();
        let raw = native.snapshot_v1();
        cast(&mut own, &mut native, counter);
        act(&mut own, &mut native, |a| {
            is_original_spell_target(a, source)
        });
        settle(&mut own, &mut native);
        assert_eq!(
            own.fixture_state().objects.get(source).zone,
            Zone::Graveyard
        );
        own.restore(&paid).unwrap();
        native.kaito_restore_v75(&raw).unwrap();
        settle(&mut own, &mut native);
        let ready = own.snapshot();
        let ready_raw = native.snapshot_v1();
        for (target, removed) in [(source, false), (other, false), (other, true)] {
            let mut expected = None;
            for _ in 0..2 {
                own.restore(&ready).unwrap();
                native.kaito_restore_v75(&ready_raw).unwrap();
                assert!(own.fixture_state().objects.get(source).summoning_sick);
                activate(&mut own, &mut native, source);
                assert!(!actions(&own).iter().any(|a|matches!(a,A::ChooseTarget{target:crate::rl::TargetRefV1::Object{object},..} if object.arena_id==opponent.0)));
                choose(&mut own, &mut native, source, target);
                assert_eq!(tapped(&own, p), 7);
                assert!(
                    !actions(&own).iter().any(|a| activation(a, source)),
                    "sorcery activation allowed with occupied stack"
                );
                checkpoint(&mut own, &mut native);
                if removed {
                    cast(&mut own, &mut native, removal);
                    choose(&mut own, &mut native, removal, source);
                }
                settle(&mut own, &mut native);
                assert_eq!(
                    own.fixture_state().objects.get(target).counters.plus1_plus1,
                    1
                );
                if removed {
                    assert_eq!(
                        own.fixture_state().objects.get(source).zone,
                        Zone::Graveyard
                    );
                } else {
                    activate(&mut own, &mut native, source);
                    choose(&mut own, &mut native, source, target);
                    settle(&mut own, &mut native);
                    assert_eq!(
                        own.fixture_state().objects.get(target).counters.plus1_plus1,
                        2
                    );
                    assert_eq!(tapped(&own, p), 10);
                }
                if let Some(expected) = &expected {
                    assert_eq!(own.fixture_state(), expected);
                } else {
                    expected = Some(own.fixture_state().clone());
                }
            }
        }
    }
}
