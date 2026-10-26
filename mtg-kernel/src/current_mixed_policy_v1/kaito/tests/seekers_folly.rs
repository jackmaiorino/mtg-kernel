use super::*;
use super::{
    surveil_many::{checkpoint, sessions},
    targeted_entry::{actions, cast},
};
use crate::{
    ids::ObjectId,
    policy_observation_v6::tests::put,
    state::{GameState, Step, Zone},
};

fn setup(
    p: PlayerId,
    count: usize,
    board: bool,
) -> (
    KaitoSession,
    FastActorSessionV1,
    ObjectId,
    Option<ObjectId>,
    Vec<ObjectId>,
) {
    let forest = crate::card_def::card_id_by_name("Forest").unwrap();
    let mut s = GameState::new_from_libraries_with_starting_player_v1(
        &[forest; 40],
        &[forest; 40],
        crate::rl::card_name,
        110_349,
        p,
    );
    s.step = Step::Main1;
    for _ in 0..4 {
        put(&mut s, p, "Swamp", Zone::Battlefield);
        put(&mut s, p.opponent(), "Island", Zone::Battlefield);
    }
    let source = put(&mut s, p, "Seeker's Folly", Zone::Hand);
    let mut objects = Vec::new();
    for name in ["Forest", "Savannah Lions", "Giant Growth"]
        .iter()
        .take(count)
    {
        objects.push(put(&mut s, p.opponent(), name, Zone::Hand));
    }
    let counter = if board {
        objects = vec![
            put(&mut s, p, "Savannah Lions", Zone::Battlefield),
            put(&mut s, p.opponent(), "Llanowar Elves", Zone::Battlefield),
            put(&mut s, p.opponent(), "Serra Angel", Zone::Battlefield),
            put(&mut s, p.opponent(), "Brineborn Cutthroat", Zone::Hand),
        ];
        Some(put(&mut s, p.opponent(), "Counterspell", Zone::Hand))
    } else {
        None
    };
    let (own, native) = sessions(s);
    (own, native, source, counter, objects)
}
fn hidden(s: &GameState, p: PlayerId) {
    let view = crate::rl::observe_v2(s, &crate::surface_v2::HarnessSurfaceV2::new(), p, 0).unwrap();
    assert!(view.known_hand_cards[p.opponent().index()].is_empty());
}
fn begin(
    own: &mut KaitoSession,
    native: &mut FastActorSessionV1,
    source: ObjectId,
    p: PlayerId,
    mode: u8,
) {
    cast(own, native, source);
    assert_eq!(
        actions(own)
            .iter()
            .filter_map(|a| if let A::ChooseSpellMode { mode_index, .. } = a {
                Some(*mode_index)
            } else {
                None
            })
            .collect::<Vec<_>>(),
        [0, 1]
    );
    checkpoint(own, native);
    act(
        own,
        native,
        |a| matches!(a,A::ChooseSpellMode{mode_index,..}if *mode_index==mode),
    );
    if mode == 0 {
        let menu = actions(own);
        assert_eq!(
            menu.iter()
                .filter(|a| matches!(a, A::ChooseTarget { .. }))
                .count(),
            1
        );
        act(
            own,
            native,
            |a| matches!(a,A::ChooseTarget{target:crate::rl::TargetRefV1::Player{player},..}if *player==p.opponent().into()),
        );
    }
    assert!(own.fixture_state().engine.pending_cast.is_none());
    assert_eq!(own.fixture_state().objects.get(source).zone, Zone::Stack);
    assert_eq!(
        own.fixture_state().players[p.index()]
            .battlefield
            .iter()
            .filter(|&&o| own.fixture_state().objects.get(o).tapped)
            .count(),
        3
    );
}
#[test]
fn paid_folly_opponent_private_two_card_choices_empty_single_hand_payment_and_restore_both_seats() {
    let id = crate::card_def::card_id_by_name("Seeker's Folly").unwrap();
    assert_eq!(
        super::super::super::roots::registered_spell_printing(id).unwrap(),
        Some(super::super::super::roots::baseline(id).unwrap())
    );
    for p in [PlayerId::P0, PlayerId::P1] {
        for count in [0, 1, 3] {
            let (mut own, mut native, source, _, hand) = setup(p, count, false);
            hidden(own.fixture_state(), p);
            begin(&mut own, &mut native, source, p, 0);
            if count < 2 {
                let saved = own.snapshot();
                let raw = native.snapshot_v1();
                settle(&mut own, &mut native);
                assert!(own.fixture_state().players[p.opponent().index()]
                    .hand
                    .is_empty());
                assert!(hand
                    .iter()
                    .all(|&o| own.fixture_state().objects.get(o).zone == Zone::Graveyard));
                assert_eq!(
                    own.fixture_state().objects.get(source).zone,
                    Zone::Graveyard
                );
                hidden(own.fixture_state(), p);
                checkpoint(&mut own, &mut native);
                let result = own.fixture_state().clone();
                own.restore(&saved).unwrap();
                native.kaito_restore_v75(&raw).unwrap();
                settle(&mut own, &mut native);
                assert_eq!(own.fixture_state(), &result);
                continue;
            }
            for _ in 0..32 {
                if actions(&own).iter().any(|a| matches!(a, A::Discard { .. })) {
                    break;
                }
                act(&mut own, &mut native, |a| matches!(a, A::Pass { .. }));
            }
            let FastActorResponseV1::Decision(h) = own.current_response() else {
                panic!()
            };
            assert_eq!(h.acting_player, p.opponent().into());
            hidden(own.fixture_state(), p);
            let choices = actions(&own)
                .into_iter()
                .filter(|a| matches!(a, A::Discard { .. }))
                .collect::<Vec<_>>();
            assert_eq!(choices.len(), 3);
            let saved = own.snapshot();
            let raw = native.snapshot_v1();
            for first in choices {
                let A::Discard { cards, .. } = &first else {
                    unreachable!()
                };
                assert_eq!(cards.len(), 1);
                let first_id = cards[0].arena_id;
                own.restore(&saved).unwrap();
                native.kaito_restore_v75(&raw).unwrap();
                act(&mut own, &mut native, |a| a == &first);
                // The surface selects one card at a time. The native engine
                // receives the entire two-card discard only after both picks.
                assert_eq!(
                    own.fixture_state().players[p.opponent().index()].hand.len(),
                    3
                );
                assert!(hand
                    .iter()
                    .all(|&o| own.fixture_state().objects.get(o).zone == Zone::Hand));
                assert_eq!(own.fixture_state().objects.get(source).zone, Zone::Stack);
                hidden(own.fixture_state(), p);
                let FastActorResponseV1::Decision(h) = own.current_response() else {
                    panic!()
                };
                assert_eq!(h.acting_player, p.opponent().into());
                let seconds = actions(&own)
                    .into_iter()
                    .filter(|a| matches!(a, A::Discard { .. }))
                    .collect::<Vec<_>>();
                assert_eq!(seconds.len(), 2);
                checkpoint(&mut own, &mut native);
                let partial = own.snapshot();
                let partial_raw = native.snapshot_v1();
                for second in seconds {
                    let A::Discard { cards, .. } = &second else {
                        unreachable!()
                    };
                    assert_eq!(cards.len(), 1);
                    assert_ne!(first_id, cards[0].arena_id);
                    let chosen = [first_id, cards[0].arena_id];
                    let mut expected = None;
                    for _ in 0..2 {
                        own.restore(&partial).unwrap();
                        native.kaito_restore_v75(&partial_raw).unwrap();
                        act(&mut own, &mut native, |a| a == &second);
                        settle(&mut own, &mut native);
                        let s = own.fixture_state();
                        assert_eq!(s.objects.get(source).zone, Zone::Graveyard);
                        assert_eq!(s.players[p.opponent().index()].hand.len(), 1);
                        for &o in &hand {
                            assert_eq!(
                                s.objects.get(o).zone,
                                if chosen.contains(&o.0) {
                                    Zone::Graveyard
                                } else {
                                    Zone::Hand
                                }
                            );
                        }
                        hidden(s, p);
                        assert_eq!(s.players[p.index()].mana_pool, [0; 6]);
                        if let Some(ref state) = expected {
                            assert_eq!(s, state);
                        } else {
                            expected = Some(s.clone());
                        }
                    }
                }
            }
        }
    }
}
#[test]
fn paid_folly_untargeted_opponent_set_lethal_weakening_later_flash_entry_cleanup_counter_and_restore_both_seats(
) {
    for p in [PlayerId::P0, PlayerId::P1] {
        let (mut own, mut native, source, counter, objects) = setup(p, 0, true);
        begin(&mut own, &mut native, source, p, 1);
        let saved = own.snapshot();
        let raw = native.snapshot_v1();
        for countered in [false, true] {
            let mut expected = None;
            for _ in 0..2 {
                own.restore(&saved).unwrap();
                native.kaito_restore_v75(&raw).unwrap();
                if countered {
                    act(&mut own, &mut native, |a| matches!(a, A::Pass { .. }));
                    cast(&mut own, &mut native, counter.unwrap());
                    act(&mut own, &mut native, |a| {
                        is_original_spell_target(a, source)
                    });
                }
                settle(&mut own, &mut native);
                let s = own.fixture_state();
                assert_eq!(s.objects.get(source).zone, Zone::Graveyard);
                assert_eq!(
                    crate::engine::checked_effective_pt(s, objects[0]).unwrap(),
                    (2, 1)
                );
                assert_eq!(
                    s.objects.get(objects[1]).zone,
                    if countered {
                        Zone::Battlefield
                    } else {
                        Zone::Graveyard
                    }
                );
                assert_eq!(
                    crate::engine::checked_effective_pt(s, objects[2]).unwrap(),
                    if countered { (4, 4) } else { (3, 3) }
                );
                cast(&mut own, &mut native, objects[3]);
                settle(&mut own, &mut native);
                assert_eq!(
                    crate::engine::checked_effective_pt(own.fixture_state(), objects[3]).unwrap(),
                    (2, 1)
                );
                super::ordinary_equipment::reach_main(&mut own, &mut native, p.opponent());
                assert_eq!(
                    crate::engine::checked_effective_pt(own.fixture_state(), objects[2]).unwrap(),
                    (4, 4)
                );
                if let Some(ref s) = expected {
                    assert_eq!(own.fixture_state(), s);
                } else {
                    expected = Some(own.fixture_state().clone());
                }
            }
        }
    }
}
