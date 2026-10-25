use super::*;
use super::{
    surveil_many::{checkpoint, scene, sessions},
    targeted_entry::{actions, cast, choose},
};
use crate::{card_def::Keywords, ids::ObjectId, policy_observation_v6::tests::put, state::Zone};

fn activation(a: &A, source: ObjectId) -> bool {
    matches!(a,A::ActivateAbility{source:s,ability_index:0,..}if s.arena_id==source.0)
}
fn activate(own: &mut KaitoSession, native: &mut FastActorSessionV1, source: ObjectId) {
    for _ in 0..4 {
        if actions(own).iter().any(|a| activation(a, source)) {
            act(own, native, |a| activation(a, source));
            return;
        }
        act(own, native, |a| matches!(a, A::Pass { .. }));
    }
    panic!("missing utility activation");
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
fn cost_candidate(a: &A, source: ObjectId, victim: ObjectId) -> bool {
    matches!(a,A::ChooseCostTarget{source:s,candidate,..}if s.arena_id==source.0&&candidate.arena_id==victim.0)
}

#[test]
fn paid_ghoul_other_controlled_creature_cost_counter_source_departure_repeat_and_restore_both_seats(
) {
    registered("Hungry Ghoul");
    for p in [PlayerId::P0, PlayerId::P1] {
        let (mut s, source, counter, _) = scene(p, "Hungry Ghoul", 40);
        for _ in 0..8 {
            put(&mut s, p, "Swamp", Zone::Battlefield);
        }
        let donors = [
            put(&mut s, p, "Savannah Lions", Zone::Battlefield),
            put(&mut s, p, "Serra Angel", Zone::Battlefield),
        ];
        // Initial support operands cover tapped and summoning-sick sacrifices.
        s.objects.get_mut(donors[0]).tapped = true;
        let opponent = put(&mut s, p.opponent(), "Serra Angel", Zone::Battlefield);
        let land = put(&mut s, p, "Forest", Zone::Battlefield);
        let removal = put(&mut s, p, "Hero's Downfall", Zone::Hand);
        let (mut own, mut native) = sessions(s);
        let before = tapped(&own, p);
        cast(&mut own, &mut native, source);
        assert_eq!(tapped(&own, p), before + 2);
        let paid = own.snapshot();
        let paid_raw = native.snapshot_v1();
        cast(&mut own, &mut native, counter);
        act(&mut own, &mut native, |a| {
            is_original_spell_target(a, source)
        });
        settle(&mut own, &mut native);
        assert_eq!(
            own.fixture_state().objects.get(source).zone,
            Zone::Graveyard
        );
        assert!(donors
            .iter()
            .all(|&o| own.fixture_state().objects.get(o).zone == Zone::Battlefield));
        own.restore(&paid).unwrap();
        native.kaito_restore_v75(&paid_raw).unwrap();
        settle(&mut own, &mut native);
        let ready = own.snapshot();
        let ready_raw = native.snapshot_v1();
        for removed in [false, true] {
            let mut expected = None;
            for _ in 0..2 {
                own.restore(&ready).unwrap();
                native.kaito_restore_v75(&ready_raw).unwrap();
                let before = tapped(&own, p);
                assert!(own.fixture_state().objects.get(source).summoning_sick);
                activate(&mut own, &mut native, source);
                let choices = actions(&own);
                for excluded in [source, opponent, land] {
                    assert!(!choices.iter().any(|a| cost_candidate(a, source, excluded)));
                }
                assert!(donors
                    .iter()
                    .all(|&o| choices.iter().any(|a| cost_candidate(a, source, o))));
                assert_eq!(
                    tapped(&own, p),
                    before,
                    "cost must wait for its selected sacrifice"
                );
                checkpoint(&mut own, &mut native);
                act(&mut own, &mut native, |a| {
                    cost_candidate(a, source, donors[0])
                });
                assert_eq!(
                    own.fixture_state().objects.get(donors[0]).zone,
                    Zone::Graveyard
                );
                // The tapped sacrifice leaves the battlefield while one mana land taps.
                assert_eq!(tapped(&own, p), before);
                assert_eq!(
                    own.fixture_state().objects.get(source).counters.plus1_plus1,
                    0
                );
                checkpoint(&mut own, &mut native);
                if removed {
                    cast(&mut own, &mut native, removal);
                    choose(&mut own, &mut native, removal, source);
                }
                settle(&mut own, &mut native);
                assert_eq!(
                    own.fixture_state().objects.get(source).zone,
                    if removed {
                        Zone::Graveyard
                    } else {
                        Zone::Battlefield
                    }
                );
                assert_eq!(
                    own.fixture_state().objects.get(source).counters.plus1_plus1,
                    if removed { 0 } else { 1 }
                );
                if !removed {
                    activate(&mut own, &mut native, source);
                    let choices = actions(&own);
                    assert!(!choices.iter().any(|a| cost_candidate(a, source, source)));
                    act(&mut own, &mut native, |a| {
                        cost_candidate(a, source, donors[1])
                    });
                    settle(&mut own, &mut native);
                    assert_eq!(
                        own.fixture_state().objects.get(source).counters.plus1_plus1,
                        2
                    );
                    assert_eq!(
                        own.fixture_state().objects.get(donors[1]).zone,
                        Zone::Graveyard
                    );
                    assert!(
                        !actions(&own).iter().any(|a| activation(a, source)),
                        "source cannot pay its own other-creature cost"
                    );
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

#[test]
fn paid_passage_land_play_colorless_mana_either_controlled_target_departure_cleanup_and_restore_both_seats(
) {
    registered("Rogue's Passage");
    for p in [PlayerId::P0, PlayerId::P1] {
        let (mut s, source, _, removal) = scene(p, "Rogue's Passage", 40);
        let second_land = put(&mut s, p, "Forest", Zone::Hand);
        let recipients = [
            put(&mut s, p, "Serra Angel", Zone::Battlefield),
            put(&mut s, p.opponent(), "Serra Angel", Zone::Battlefield),
        ];
        let (mut own, mut native) = sessions(s);
        act(
            &mut own,
            &mut native,
            |a| matches!(a,A::PlayLand{source:s,..}if s.arena_id==source.0),
        );
        assert_eq!(
            own.fixture_state().objects.get(source).zone,
            Zone::Battlefield
        );
        assert!(!own.fixture_state().objects.get(source).tapped);
        assert!(own.fixture_state().stack.is_empty());
        assert_eq!(
            own.fixture_state().players[p.index()].lands_played_this_turn,
            1
        );
        assert!(!actions(&own)
            .iter()
            .any(|a| matches!(a,A::PlayLand{source:s,..}if s.arena_id==second_land.0)));
        let ready = own.snapshot();
        let ready_raw = native.snapshot_v1();
        act(
            &mut own,
            &mut native,
            |a| matches!(a,A::ActivateManaAbility{source:s,..}if s.arena_id==source.0),
        );
        assert_eq!(
            own.fixture_state().players[p.index()].mana_pool
                [crate::mana::ManaColor::C.pool_index()],
            1
        );
        assert!(own.fixture_state().objects.get(source).tapped);
        assert!(own.fixture_state().stack.is_empty());
        for recipient in recipients {
            for removed in [false, true] {
                let mut expected = None;
                for _ in 0..2 {
                    own.restore(&ready).unwrap();
                    native.kaito_restore_v75(&ready_raw).unwrap();
                    activate(&mut own, &mut native, source);
                    let choices = actions(&own);
                    for wanted in recipients {
                        assert!(choices.iter().any(|a|matches!(a,A::ChooseTarget{source:s,target:crate::rl::TargetRefV1::Object{object},..}if s.arena_id==source.0&&object.arena_id==wanted.0)));
                    }
                    choose(&mut own, &mut native, source, recipient);
                    assert_eq!(tapped(&own, p), 5);
                    assert!(own.fixture_state().objects.get(source).tapped);
                    checkpoint(&mut own, &mut native);
                    if removed {
                        cast(&mut own, &mut native, removal);
                        choose(&mut own, &mut native, removal, recipient);
                    }
                    settle(&mut own, &mut native);
                    assert_eq!(
                        own.fixture_state().objects.get(recipient).zone,
                        if removed {
                            Zone::Graveyard
                        } else {
                            Zone::Battlefield
                        }
                    );
                    if !removed {
                        assert!(crate::engine::effective_keywords(
                            own.fixture_state(),
                            recipient,
                            Keywords::CANT_BE_BLOCKED
                        )
                        .has(Keywords::CANT_BE_BLOCKED));
                        let turn = own.fixture_state().turn_key_v1();
                        for _ in 0..128 {
                            if own.fixture_state().turn_key_v1() != turn {
                                break;
                            }
                            act(&mut own, &mut native, |a| {
                                matches!(
                                    a,
                                    A::Pass { .. }
                                        | A::OrderTriggers { .. }
                                        | A::ChooseTriggerOrderNext { .. }
                                        | A::ChooseAttackerInclusion { include: false, .. }
                                        | A::ChooseBlockerInclusion { include: false, .. }
                                )
                            });
                        }
                        assert_ne!(own.fixture_state().turn_key_v1(), turn);
                        assert!(!crate::engine::effective_keywords(
                            own.fixture_state(),
                            recipient,
                            Keywords::CANT_BE_BLOCKED
                        )
                        .has(Keywords::CANT_BE_BLOCKED));
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
}
