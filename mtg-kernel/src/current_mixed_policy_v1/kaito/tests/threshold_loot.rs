use super::*;
use super::{
    surveil_many::{checkpoint, order, scene, sessions, wait_selection},
    targeted_entry::{actions, cast, choose},
};
use crate::{
    card_def::Keywords,
    ids::ObjectId,
    policy_observation_v6::tests::put,
    state::{GameState, Zone},
};
fn quiet(a: &A) -> bool {
    matches!(
        a,
        A::Pass { .. } | A::OrderTriggers { .. } | A::ChooseTriggerOrderNext { .. }
    )
}
fn prepare(
    p: PlayerId,
    name: &str,
    size: usize,
    graveyard: usize,
) -> (GameState, ObjectId, ObjectId, ObjectId, ObjectId) {
    let (mut s, source, counter, removal) = scene(p, name, size);
    for _ in 0..2 {
        put(&mut s, p, "Swamp", Zone::Battlefield);
    }
    for _ in 0..graveyard {
        put(&mut s, p, "Forest", Zone::Graveyard);
    }
    for _ in 0..7 {
        put(&mut s, p.opponent(), "Forest", Zone::Graveyard);
    }
    let ooze = put(&mut s, p.opponent(), "Scavenging Ooze", Zone::Battlefield);
    (s, source, counter, removal, ooze)
}
fn exile(own: &mut KaitoSession, native: &mut FastActorSessionV1, ooze: ObjectId, card: ObjectId) {
    let activation =
        |a: &A| matches!(a,A::ActivateAbility{source:s,ability_index:0,..}if s.arena_id==ooze.0);
    if !actions(own).iter().any(activation) {
        act(own, native, |a| matches!(a, A::Pass { .. }));
    }
    act(own, native, activation);
    choose(own, native, ooze, card);
    checkpoint(own, native);
    settle(own, native);
    assert_eq!(own.fixture_state().objects.get(card).zone, Zone::Exile);
}
fn loot(
    own: &mut KaitoSession,
    native: &mut FastActorSessionV1,
    p: PlayerId,
    discard_drawn: bool,
) -> ObjectId {
    let hand = own.fixture_state().players[p.index()].hand.clone();
    let library = own.fixture_state().players[p.index()].library.clone();
    let drawn = library.first().copied();
    let chosen = if discard_drawn {
        drawn.unwrap()
    } else {
        hand[0]
    };
    for _ in 0..128 {
        let menu = actions(own);
        if menu.iter().any(|a| matches!(a, A::Discard { .. })) {
            let mut actual = menu
                .iter()
                .filter_map(|a| match a {
                    A::Discard { actor, cards } if cards.len() == 1 => {
                        assert_eq!(*actor, crate::rl::PlayerSeatV1::from(p));
                        Some(cards[0].arena_id)
                    }
                    _ => None,
                })
                .collect::<Vec<_>>();
            let mut expected = hand
                .iter()
                .map(|o| o.0)
                .chain(drawn.map(|o| o.0))
                .collect::<Vec<_>>();
            actual.sort_unstable();
            expected.sort_unstable();
            assert_eq!(actual, expected);
            assert_eq!(
                own.fixture_state().players[p.index()].library,
                &library[usize::from(drawn.is_some())..]
            );
            assert!(
                !own.fixture_state().players[p.index()].has_lost,
                "SBA loss must wait for draw-then-discard resolution"
            );
            checkpoint(own, native);
            act(
                own,
                native,
                |a| matches!(a,A::Discard{cards,..}if cards.len()==1&&cards[0].arena_id==chosen.0),
            );
            settle(own, native);
            assert_eq!(
                own.fixture_state().objects.get(chosen).zone,
                Zone::Graveyard
            );
            assert_eq!(
                own.fixture_state().players[p.index()].hand.len(),
                hand.len() + usize::from(drawn.is_some()) - 1
            );
            return chosen;
        }
        act(own, native, quiet);
    }
    panic!("actual mandatory draw-then-discard choice absent");
}
#[test]
fn paid_shriekmass_private_mill_order_short_empty_counter_departure_threshold_and_restore_both_seats(
) {
    let id = crate::card_def::card_id_by_name("Billowing Shriekmass").unwrap();
    assert_eq!(
        super::super::super::roots::registered_permanent_printing(id).unwrap(),
        Some(super::super::super::roots::baseline(id).unwrap())
    );
    for p in [PlayerId::P0, PlayerId::P1] {
        for size in [0, 1, 2, 40] {
            let (s, source, counter, removal, ooze) = prepare(p, "Billowing Shriekmass", size, 4);
            let (mut own, mut native) = sessions(s);
            cast(&mut own, &mut native, source);
            assert_eq!(
                own.fixture_state().players[p.index()]
                    .battlefield
                    .iter()
                    .filter(|&&o| own.fixture_state().objects.get(o).tapped)
                    .count(),
                4
            );
            let paid = own.snapshot();
            let raw = native.snapshot_v1();
            for branch in 0..4 {
                if branch == 3 && size < 3 {
                    continue;
                }
                for reverse in [false, true] {
                    let mut expected = None;
                    for _ in 0..2 {
                        own.restore(&paid).unwrap();
                        native.kaito_restore_v75(&raw).unwrap();
                        let library = own.fixture_state().players[p.index()].library.clone();
                        let prefix = library[..library.len().min(3)].to_vec();
                        let old_graveyard =
                            own.fixture_state().players[p.index()].graveyard.clone();
                        let mut ordered = prefix.clone();
                        if reverse {
                            ordered.reverse();
                        }
                        if branch == 0 {
                            cast(&mut own, &mut native, counter);
                            act(&mut own, &mut native, |a| {
                                is_original_spell_target(a, source)
                            });
                            settle(&mut own, &mut native);
                            assert_eq!(own.fixture_state().players[p.index()].library, library);
                        } else {
                            super::library_search::wait_entered(&mut own, &mut native, source);
                            assert_eq!(
                                crate::engine::checked_effective_pt(own.fixture_state(), source)
                                    .unwrap(),
                                (2, 3)
                            );
                            if branch == 2 {
                                cast(&mut own, &mut native, removal);
                                choose(&mut own, &mut native, removal, source);
                            }
                            if prefix.len() >= 2 {
                                wait_selection(
                                    &mut own,
                                    &mut native,
                                    source,
                                    &prefix,
                                    prefix.len() as u16,
                                );
                                assert_eq!(own.fixture_state().players[p.index()].library, library);
                                order(&mut own, &mut native, source, &ordered);
                            }
                            settle(&mut own, &mut native);
                            let expected_graveyard = old_graveyard
                                .iter()
                                .copied()
                                .chain((branch == 2).then_some(source))
                                .chain(ordered.iter().copied())
                                .collect::<Vec<_>>();
                            assert_eq!(
                                own.fixture_state().players[p.index()].graveyard,
                                expected_graveyard
                            );
                            assert_eq!(
                                own.fixture_state().players[p.index()].library,
                                &library[prefix.len()..]
                            );
                            if branch == 3 {
                                assert_eq!(
                                    crate::engine::checked_effective_pt(
                                        own.fixture_state(),
                                        source
                                    )
                                    .unwrap(),
                                    (4, 4)
                                );
                                exile(&mut own, &mut native, ooze, ordered[0]);
                            }
                            if branch != 2 {
                                assert_eq!(
                                    crate::engine::checked_effective_pt(
                                        own.fixture_state(),
                                        source
                                    )
                                    .unwrap(),
                                    if prefix.len() == 3 && branch != 3 {
                                        (4, 4)
                                    } else {
                                        (2, 3)
                                    }
                                );
                                assert!(crate::engine::has_effective_keyword(
                                    own.fixture_state(),
                                    source,
                                    Keywords::FLYING
                                ));
                            }
                        }
                        assert_eq!(
                            own.fixture_state().objects.get(source).zone,
                            if branch == 0 || branch == 2 {
                                Zone::Graveyard
                            } else {
                                Zone::Battlefield
                            }
                        );
                        assert!(!own.fixture_state().players[p.index()].drew_from_empty);
                        assert!(!own.fixture_state().players[p.index()].has_lost);
                        assert_eq!(own.fixture_state().players[p.index()].life, 20);
                        if let Some(ref prior) = expected {
                            assert_eq!(own.fixture_state(), prior);
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
fn paid_scavenger_entry_old_or_new_discard_counter_departure_threshold_exile_and_restore_both_seats(
) {
    let id = crate::card_def::card_id_by_name("Dreadwing Scavenger").unwrap();
    assert_eq!(
        super::super::super::roots::registered_permanent_printing(id).unwrap(),
        Some(super::super::super::roots::baseline(id).unwrap())
    );
    for p in [PlayerId::P0, PlayerId::P1] {
        let (mut s, source, counter, removal, ooze) = prepare(p, "Dreadwing Scavenger", 40, 6);
        let old = put(&mut s, p, "Mountain", Zone::Hand);
        let (mut own, mut native) = sessions(s);
        cast(&mut own, &mut native, source);
        assert_eq!(
            own.fixture_state().players[p.index()]
                .battlefield
                .iter()
                .filter(|&&o| own.fixture_state().objects.get(o).tapped)
                .count(),
            3
        );
        let paid = own.snapshot();
        let raw = native.snapshot_v1();
        for branch in 0..4 {
            for discard_drawn in [false, true] {
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
                        assert!(own.fixture_state().players[p.index()].hand.contains(&old));
                    } else {
                        super::library_search::wait_entered(&mut own, &mut native, source);
                        assert_eq!(
                            crate::engine::checked_effective_pt(own.fixture_state(), source)
                                .unwrap(),
                            (2, 2)
                        );
                        assert!(!crate::engine::has_effective_keyword(
                            own.fixture_state(),
                            source,
                            Keywords::DEATHTOUCH
                        ));
                        if branch == 2 {
                            cast(&mut own, &mut native, removal);
                            choose(&mut own, &mut native, removal, source);
                        }
                        let discarded = loot(&mut own, &mut native, p, discard_drawn);
                        assert_eq!(discarded, if discard_drawn { library[0] } else { old });
                        if branch == 3 {
                            assert_eq!(
                                crate::engine::checked_effective_pt(own.fixture_state(), source)
                                    .unwrap(),
                                (3, 3)
                            );
                            assert!(crate::engine::has_effective_keyword(
                                own.fixture_state(),
                                source,
                                Keywords::DEATHTOUCH
                            ));
                            exile(&mut own, &mut native, ooze, discarded);
                        }
                        if branch != 2 {
                            let threshold = branch != 3;
                            assert_eq!(
                                crate::engine::checked_effective_pt(own.fixture_state(), source)
                                    .unwrap(),
                                if threshold { (3, 3) } else { (2, 2) }
                            );
                            assert_eq!(
                                crate::engine::has_effective_keyword(
                                    own.fixture_state(),
                                    source,
                                    Keywords::DEATHTOUCH
                                ),
                                threshold
                            );
                        }
                        assert_eq!(
                            own.fixture_state().players[p.index()].library,
                            &library[1..]
                        );
                    }
                    assert_eq!(
                        own.fixture_state().objects.get(source).zone,
                        if branch == 0 || branch == 2 {
                            Zone::Graveyard
                        } else {
                            Zone::Battlefield
                        }
                    );
                    assert_eq!(own.fixture_state().players[p.index()].mana_pool, [0; 6]);
                    assert_eq!(own.fixture_state().players[p.opponent().index()].life, 20);
                    if let Some(ref prior) = expected {
                        assert_eq!(own.fixture_state(), prior);
                    } else {
                        expected = Some(own.fixture_state().clone());
                    }
                }
            }
        }
    }
}
#[test]
fn paid_scavenger_natural_attack_loots_once_after_source_departure_and_restores_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let (mut s, source, _, removal, _) = prepare(p, "Dreadwing Scavenger", 40, 0);
        put(&mut s, p, "Mountain", Zone::Hand);
        let (mut own, mut native) = sessions(s);
        cast(&mut own, &mut native, source);
        loot(&mut own, &mut native, p, true);
        assert!(own.fixture_state().objects.get(source).summoning_sick);
        super::ordinary_equipment::reach_main(&mut own, &mut native, p.opponent());
        super::ordinary_equipment::reach_main(&mut own, &mut native, p);
        assert!(!own.fixture_state().objects.get(source).summoning_sick);
        super::syphoner::declare_paid_source(&mut own, &mut native, source);
        assert!(own.fixture_state().objects.get(source).tapped);
        checkpoint(&mut own, &mut native);
        let attacking = own.snapshot();
        let raw = native.snapshot_v1();
        for removed in [false, true] {
            for discard_drawn in [false, true] {
                let mut expected = None;
                for _ in 0..2 {
                    own.restore(&attacking).unwrap();
                    native.kaito_restore_v75(&raw).unwrap();
                    let library = own.fixture_state().players[p.index()].library.clone();
                    if removed {
                        cast(&mut own, &mut native, removal);
                        choose(&mut own, &mut native, removal, source);
                    }
                    loot(&mut own, &mut native, p, discard_drawn);
                    assert_eq!(
                        own.fixture_state().players[p.index()].library,
                        &library[1..]
                    );
                    assert_eq!(
                        own.fixture_state().objects.get(source).zone,
                        if removed {
                            Zone::Graveyard
                        } else {
                            Zone::Battlefield
                        }
                    );
                    if let Some(ref prior) = expected {
                        assert_eq!(own.fixture_state(), prior);
                    } else {
                        expected = Some(own.fixture_state().clone());
                    }
                }
            }
        }
    }
}
#[test]
fn paid_scavenger_empty_draw_still_discards_then_loses_naturally_and_restores_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let (mut s, source, _, _, _) = prepare(p, "Dreadwing Scavenger", 0, 0);
        let old = put(&mut s, p, "Mountain", Zone::Hand);
        let (mut own, mut native) = sessions(s);
        cast(&mut own, &mut native, source);
        let paid = own.snapshot();
        let raw = native.snapshot_v1();
        let mut expected = None;
        for _ in 0..2 {
            own.restore(&paid).unwrap();
            native.kaito_restore_v75(&raw).unwrap();
            assert_eq!(loot(&mut own, &mut native, p, false), old);
            assert!(own.fixture_state().players[p.index()].drew_from_empty);
            assert!(own.fixture_state().players[p.index()].has_lost);
            let FastActorResponseV1::Terminal(terminal) = own.current_response() else {
                panic!("empty draw did not end naturally after discard")
            };
            assert_eq!(
                terminal.terminal_classification,
                crate::rl::TerminalClassificationV1::Natural
            );
            assert_eq!(
                terminal.winner,
                Some(crate::rl::PlayerSeatV1::from(p.opponent()))
            );
            if let Some(ref prior) = expected {
                assert_eq!(own.fixture_state(), prior);
            } else {
                expected = Some(own.fixture_state().clone());
            }
        }
    }
}
