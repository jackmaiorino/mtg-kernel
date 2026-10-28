use super::*;
use super::{
    surveil_many::{checkpoint, scene, sessions},
    targeted_entry::{actions, cast, choose},
};
use crate::{ids::ObjectId, policy_observation_v6::tests::put, state::Zone};
fn target(a: &A, source: ObjectId, card: ObjectId) -> bool {
    matches!(a,A::ChooseTarget{source:s,target:crate::rl::TargetRefV1::Object{object},..}if s.arena_id==source.0&&object.arena_id==card.0)
}
fn finish_targets(a: &A, source: ObjectId, count: u16) -> bool {
    matches!(a,A::FinishTargetSelection{source:s,selected_count,..}|A::FinishEffectSelection{source:s,selected_count,..}if s.arena_id==source.0&&u64::from(*selected_count)==u64::from(count))
}
fn exile(own: &mut KaitoSession, native: &mut FastActorSessionV1, ooze: ObjectId, card: ObjectId) {
    for _ in 0..4 {
        if actions(own).iter().any(
            |a| matches!(a,A::ActivateAbility{source,ability_index:0,..}if source.arena_id==ooze.0),
        ) {
            break;
        }
        act(own, native, |a| matches!(a, A::Pass { .. }));
    }
    act(
        own,
        native,
        |a| matches!(a,A::ActivateAbility{source,ability_index:0,..}if source.arena_id==ooze.0),
    );
    choose(own, native, ooze, card);
    for _ in 0..16 {
        if own.fixture_state().objects.get(card).zone == Zone::Exile {
            return;
        }
        act(own, native, |a| matches!(a, A::Pass { .. }));
    }
    panic!("genuine graveyard removal did not resolve");
}

#[test]
fn paid_macabre_zero_one_two_targets_partial_or_all_illegal_returned_card_discard_and_restore_both_seats(
) {
    let id = crate::card_def::card_id_by_name("Macabre Waltz").unwrap();
    assert_eq!(
        super::super::super::roots::registered_spell_printing(id).unwrap(),
        Some(super::super::super::roots::baseline(id).unwrap())
    );
    for p in [PlayerId::P0, PlayerId::P1] {
        for count in 0..=2 {
            let (mut s, source, counter, _) = scene(p, "Macabre Waltz", 40);
            for _ in 0..4 {
                put(&mut s, p, "Swamp", Zone::Battlefield);
            }
            for _ in 0..2 {
                put(&mut s, p.opponent(), "Forest", Zone::Battlefield);
            }
            let victims = [
                put(&mut s, p, "Savannah Lions", Zone::Graveyard),
                put(&mut s, p, "Serra Angel", Zone::Graveyard),
            ];
            let excluded = [
                put(&mut s, p, "Forest", Zone::Graveyard),
                put(&mut s, p.opponent(), "Savannah Lions", Zone::Graveyard),
            ];
            let hand = put(&mut s, p, "Forest", Zone::Hand);
            put(&mut s, p, "Plains", Zone::Hand);
            let ooze = put(&mut s, p.opponent(), "Scavenging Ooze", Zone::Battlefield);
            let (mut own, mut native) = sessions(s);
            cast(&mut own, &mut native, source);
            let menu = actions(&own);
            assert!(victims
                .iter()
                .all(|&v| menu.iter().any(|a| target(a, source, v))));
            assert!(excluded
                .iter()
                .all(|&v| !menu.iter().any(|a| target(a, source, v))));
            assert!(menu.iter().any(|a| finish_targets(a, source, 0)));
            checkpoint(&mut own, &mut native);
            for (at, &card) in victims.iter().take(count).enumerate() {
                act(&mut own, &mut native, |a| target(a, source, card));
                if at == 0 {
                    let choices = actions(&own);
                    assert!(!choices.iter().any(|a| target(a, source, card)));
                    assert!(choices.iter().any(|a| finish_targets(a, source, 1)));
                    checkpoint(&mut own, &mut native);
                }
            }
            if count < 2 {
                act(&mut own, &mut native, |a| {
                    finish_targets(a, source, count as u16)
                });
            }
            assert!(own.fixture_state().engine.pending_cast.is_none());
            assert_eq!(
                own.fixture_state().players[p.index()]
                    .battlefield
                    .iter()
                    .filter(|&&o| own.fixture_state().objects.get(o).tapped)
                    .count(),
                2
            );
            let paid = own.snapshot();
            let raw = native.snapshot_v1();
            for response in 0..4 {
                if response >= 2 && count == 0 || response == 2 && count == 1 {
                    continue;
                }
                let removed = if response == 3 {
                    count
                } else if response == 2 {
                    1
                } else {
                    0
                };
                let fizzled = response == 1 || count > 0 && removed == count;
                let mut expected = None;
                for _ in 0..2 {
                    own.restore(&paid).unwrap();
                    native.kaito_restore_v75(&raw).unwrap();
                    if response == 1 {
                        cast(&mut own, &mut native, counter);
                        act(&mut own, &mut native, |a| {
                            is_original_spell_target(a, source)
                        });
                    } else {
                        for &victim in victims.iter().take(removed) {
                            exile(&mut own, &mut native, ooze, victim);
                        }
                    }
                    let mut answered_discard = false;
                    for _ in 0..64 {
                        let discard = actions(&own).iter().any(|a| matches!(a, A::Discard { .. }));
                        if discard {
                            assert!(
                                !fizzled,
                                "countered or all-illegal spell attempted its discard"
                            );
                            for &victim in victims.iter().take(count).skip(removed) {
                                assert_eq!(
                                    own.fixture_state().objects.get(victim).zone,
                                    Zone::Hand
                                );
                            }
                            let chosen = if count > removed {
                                victims[removed]
                            } else {
                                hand
                            };
                            assert!(actions(&own).iter().any(|a|matches!(a,A::Discard{cards,..}if cards.len()==1&&cards[0].arena_id==chosen.0)));
                            checkpoint(&mut own, &mut native);
                            act(
                                &mut own,
                                &mut native,
                                |a| matches!(a,A::Discard{cards,..}if cards.len()==1&&cards[0].arena_id==chosen.0),
                            );
                            answered_discard = true;
                            break;
                        }
                        if own.fixture_state().stack.is_empty() {
                            break;
                        }
                        act(&mut own, &mut native, |a| matches!(a, A::Pass { .. }));
                    }
                    assert_eq!(answered_discard, !fizzled);
                    settle(&mut own, &mut native);
                    assert_eq!(
                        own.fixture_state().objects.get(source).zone,
                        Zone::Graveyard
                    );
                    assert_eq!(
                        own.fixture_state().players[p.index()].hand.len(),
                        if fizzled { 2 } else { 2 + count - removed - 1 }
                    );
                    for (at, &victim) in victims.iter().enumerate() {
                        assert_eq!(
                            own.fixture_state().objects.get(victim).zone,
                            if at < removed {
                                Zone::Exile
                            } else if !fizzled && at < count && at != removed {
                                Zone::Hand
                            } else {
                                Zone::Graveyard
                            }
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
}
