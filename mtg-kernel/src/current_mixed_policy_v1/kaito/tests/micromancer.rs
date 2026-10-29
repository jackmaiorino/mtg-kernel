use super::*;
use super::{
    library_search::{finish_zero, wait_entered},
    surveil_many::{checkpoint, scene, sessions},
    targeted_entry::{actions, cast, choose, wait_target},
};
use crate::{ids::ObjectId, policy_observation_v6::tests::put, state::Zone};

fn mode(own: &mut KaitoSession, native: &mut FastActorSessionV1, source: ObjectId, decline: bool) {
    for _ in 0..128 {
        if actions(own)
            .iter()
            .any(|a| matches!(a,A::ChooseEffectOption{source:s,..} if s.arena_id==source.0))
        {
            checkpoint(own, native);
            // Micromancer's actual order is search, decline; Campus uses the reverse order.
            act(
                own,
                native,
                |a| matches!(a,A::ChooseEffectOption{source:s,option_count:2,option_index,..} if s.arena_id==source.0 && *option_index==u16::from(decline)),
            );
            return;
        }
        act(own, native, |a| {
            matches!(
                a,
                A::Pass { .. } | A::OrderTriggers { .. } | A::ChooseTriggerOrderNext { .. }
            )
        });
    }
    panic!("Micromancer did not issue its actual optional search");
}

#[test]
fn paid_micromancer_filtered_revealed_hand_optional_counter_departure_and_restore_both_seats() {
    let id = crate::card_def::card_id_by_name("Micromancer").unwrap();
    assert_eq!(
        super::super::super::roots::registered_permanent_printing(id).unwrap(),
        Some(super::super::super::roots::baseline(id).unwrap())
    );
    for p in [PlayerId::P0, PlayerId::P1] {
        let q = p.opponent();
        let (mut s, source, counter, removal) = scene(p, "Micromancer", 40);
        let instant = put(&mut s, p, "Lightning Bolt", Zone::Library);
        let sorcery = put(&mut s, p, "Boltwave", Zone::Library);
        put(&mut s, p, "Think Twice", Zone::Library);
        put(&mut s, p, "Healer's Hawk", Zone::Library);
        let (mut own, mut native) = sessions(s);
        cast(&mut own, &mut native, source);
        assert_eq!(own.fixture_state().objects.get(source).zone, Zone::Stack);
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
        // Decline, fail to find, instant, sorcery, source departure, countered spell.
        for branch in 0..6 {
            let mut expected = None;
            for _ in 0..2 {
                own.restore(&paid).unwrap();
                native.kaito_restore_v75(&raw).unwrap();
                let library = own.fixture_state().players[p.index()].library.clone();
                let hand = own.fixture_state().players[p.index()].hand.clone();
                let rng = own.fixture_state().legacy_rng().copied();
                let knowledge = own.fixture_state().library_knowledge.clone();
                if branch == 5 {
                    cast(&mut own, &mut native, counter);
                    act(&mut own, &mut native, |a| {
                        is_original_spell_target(a, source)
                    });
                } else {
                    wait_entered(&mut own, &mut native, source);
                    if branch == 4 {
                        cast(&mut own, &mut native, removal);
                        choose(&mut own, &mut native, removal, source);
                    }
                    mode(&mut own, &mut native, source, branch == 0);
                    if branch != 0 {
                        wait_target(&mut own, &mut native, source, instant);
                        let menu = actions(&own);
                        let mut offered = menu
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
                        offered.sort_unstable();
                        let mut wanted = vec![instant, sorcery];
                        wanted.sort_unstable();
                        assert_eq!(offered, wanted);
                        assert!(menu.iter().any(|a|matches!(a,A::FinishEffectSelection{source:s,selected_count:0,..} if s.arena_id==source.0)));
                        checkpoint(&mut own, &mut native);
                        if branch == 1 {
                            finish_zero(&mut own, &mut native, source);
                        } else {
                            choose(
                                &mut own,
                                &mut native,
                                source,
                                if branch == 3 { sorcery } else { instant },
                            );
                        }
                    }
                }
                settle(&mut own, &mut native);
                let state = own.fixture_state();
                assert_eq!(
                    state.objects.get(source).zone,
                    if branch >= 4 {
                        Zone::Graveyard
                    } else {
                        Zone::Battlefield
                    }
                );
                assert_eq!(state.players[p.index()].mana_pool, [0; 6]);
                assert_eq!(state.players[p.index()].life, 20);
                assert_eq!(state.players[q.index()].life, 20);
                if branch == 0 || branch == 5 {
                    assert_eq!(state.players[p.index()].library, library);
                    assert_eq!(state.players[p.index()].hand, hand);
                    assert_eq!(state.legacy_rng().copied(), rng);
                    assert_eq!(state.library_knowledge, knowledge);
                } else {
                    assert_ne!(state.legacy_rng().copied(), rng);
                    let selected = if branch == 1 {
                        None
                    } else {
                        Some(if branch == 3 { sorcery } else { instant })
                    };
                    assert_eq!(
                        state.players[p.index()].library.len(),
                        library.len() - usize::from(selected.is_some())
                    );
                    assert_eq!(
                        state.players[p.index()].hand.len(),
                        hand.len() + usize::from(selected.is_some())
                    );
                    if let Some(selected) = selected {
                        assert!(state.players[p.index()].hand.contains(&selected));
                        assert!(state
                            .known_hand_cards(q, p)
                            .iter()
                            .any(|known| known.object == selected));
                    } else {
                        assert!(state.known_hand_cards(q, p).is_empty());
                    }
                }
                if let Some(ref previous) = expected {
                    assert_eq!(state, previous);
                } else {
                    expected = Some(state.clone());
                }
            }
        }
    }
}

#[test]
fn paid_micromancer_empty_or_no_matching_library_optional_finish_and_restore_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        for size in [0, 40] {
            let (s, source, _, _) = scene(p, "Micromancer", size);
            let (mut own, mut native) = sessions(s);
            cast(&mut own, &mut native, source);
            wait_entered(&mut own, &mut native, source);
            mode(&mut own, &mut native, source, false);
            finish_zero(&mut own, &mut native, source);
            checkpoint(&mut own, &mut native);
            settle(&mut own, &mut native);
            assert_eq!(own.fixture_state().players[p.index()].library.len(), size);
            assert!(own.fixture_state().players[p.index()].hand.is_empty());
            assert_eq!(
                own.fixture_state().objects.get(source).zone,
                Zone::Battlefield
            );
            assert_eq!(own.fixture_state().players[p.index()].mana_pool, [0; 6]);
        }
    }
}
