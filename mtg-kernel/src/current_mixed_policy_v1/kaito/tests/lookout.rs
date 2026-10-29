use super::targeted_entry::{actions, cast};
use super::*;
use crate::{
    card_def::Keywords,
    ids::ObjectId,
    policy_observation_v6::tests::put,
    state::{GameState, Step, Zone},
};

fn activation(a: &A, source: ObjectId) -> bool {
    matches!(a,A::ActivateAbility {source:s,ability_index:0,..} if s.arena_id==source.0)
}
fn quiet(a: &A) -> bool {
    matches!(
        a,
        A::Pass { .. } | A::OrderTriggers { .. } | A::ChooseTriggerOrderNext { .. }
    )
}
fn priority(own: &mut KaitoSession, native: &mut FastActorSessionV1, p: PlayerId) {
    for _ in 0..2 {
        if actions(own)
            .iter()
            .any(|a| matches!(a,A::Pass{actor} if *actor==crate::rl::PlayerSeatV1::from(p)))
        {
            return;
        }
        act(own, native, quiet);
    }
    panic!("own priority absent");
}
fn lands_tapped(s: &GameState, p: PlayerId) -> usize {
    s.players[p.index()]
        .battlefield
        .iter()
        .filter(|&&o| {
            s.objects.get(o).tapped
                && s.objects.get(o).card_def == crate::card_def::card_id_by_name("Island").unwrap()
        })
        .count()
}

#[test]
fn paid_lookout_natural_readiness_tap_then_private_draw_discard_source_departure_counter_and_restore_both_seats(
) {
    let id = crate::card_def::card_id_by_name("Strix Lookout").unwrap();
    assert_eq!(
        super::super::super::roots::registered_permanent_printing(id).unwrap(),
        Some(super::super::super::roots::baseline(id).unwrap())
    );
    let island = crate::card_def::card_id_by_name("Island").unwrap();
    for p in [PlayerId::P0, PlayerId::P1] {
        let q = p.opponent();
        let mut state = GameState::new_from_libraries_with_starting_player_v1(
            &[island; 40],
            &[island; 40],
            crate::rl::card_name,
            110_382,
            p,
        );
        state.step = Step::Main1;
        for _ in 0..4 {
            put(&mut state, p, "Island", Zone::Battlefield);
        }
        for land in ["Island", "Swamp"] {
            for _ in 0..3 {
                put(&mut state, q, land, Zone::Battlefield);
            }
        }
        let source = put(&mut state, p, "Strix Lookout", Zone::Hand);
        let counter = put(&mut state, q, "Counterspell", Zone::Hand);
        let removal = put(&mut state, q, "Hero's Downfall", Zone::Hand);
        let mut native = FastActorSessionV1::from_kaito_v75_fixture_state(state.clone());
        let mut own =
            KaitoSession::from_session(FastActorSessionV1::from_kaito_v75_fixture_state(state))
                .unwrap();
        cast(&mut own, &mut native, source);
        assert_eq!(lands_tapped(own.fixture_state(), p), 2);
        let paid = own.snapshot();
        let paid_raw = native.snapshot_v1();
        let mut countered_state = None;
        for _ in 0..2 {
            own.restore(&paid).unwrap();
            native.kaito_restore_v75(&paid_raw).unwrap();
            cast(&mut own, &mut native, counter);
            act(&mut own, &mut native, |a| {
                is_original_spell_target(a, source)
            });
            settle(&mut own, &mut native);
            assert_eq!(
                own.fixture_state().objects.get(source).zone,
                Zone::Graveyard
            );
            assert_eq!(lands_tapped(own.fixture_state(), p), 2);
            if let Some(ref prior) = countered_state {
                assert_eq!(own.fixture_state(), prior);
            } else {
                countered_state = Some(own.fixture_state().clone());
            }
        }
        own.restore(&paid).unwrap();
        native.kaito_restore_v75(&paid_raw).unwrap();
        settle(&mut own, &mut native);
        priority(&mut own, &mut native, p);
        assert!(own.fixture_state().objects.get(source).summoning_sick);
        assert!(!actions(&own).iter().any(|a| activation(a, source)));
        super::ordinary_equipment::reach_main(&mut own, &mut native, q);
        super::ordinary_equipment::reach_main(&mut own, &mut native, p);
        priority(&mut own, &mut native, p);
        assert!(!own.fixture_state().objects.get(source).summoning_sick);
        assert!(!own.fixture_state().objects.get(source).tapped);
        for keyword in [Keywords::FLYING, Keywords::VIGILANCE] {
            assert!(crate::engine::has_effective_keyword(
                own.fixture_state(),
                source,
                keyword
            ));
        }
        assert_eq!(
            crate::engine::checked_effective_pt(own.fixture_state(), source).unwrap(),
            (1, 2)
        );
        let ready = own.snapshot();
        let ready_raw = native.snapshot_v1();
        for removed in [false, true] {
            for discard_drawn in [false, true] {
                let mut expected = None;
                for _ in 0..2 {
                    own.restore(&ready).unwrap();
                    native.kaito_restore_v75(&ready_raw).unwrap();
                    let hand = own.fixture_state().players[p.index()].hand.clone();
                    let library = own.fixture_state().players[p.index()].library.clone();
                    let drawn = library[0];
                    let chosen = if discard_drawn { drawn } else { hand[0] };
                    act(&mut own, &mut native, |a| activation(a, source));
                    assert_eq!(lands_tapped(own.fixture_state(), p), 2);
                    assert!(own.fixture_state().objects.get(source).tapped);
                    assert_eq!(own.fixture_state().players[p.index()].library, library);
                    assert_eq!(own.fixture_state().players[p.index()].hand, hand);
                    let pending = own.snapshot();
                    let pending_raw = native.snapshot_v1();
                    own.restore(&pending).unwrap();
                    native.kaito_restore_v75(&pending_raw).unwrap();
                    if removed {
                        cast(&mut own, &mut native, removal);
                        act(
                            &mut own,
                            &mut native,
                            |a| matches!(a,A::ChooseTarget{target:crate::rl::TargetRefV1::Object{object},..} if object.arena_id==source.0),
                        );
                    }
                    let mut reached = false;
                    for _ in 0..128 {
                        let menu = actions(&own);
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
                            let mut expected_hand = hand
                                .iter()
                                .map(|o| o.0)
                                .chain([drawn.0])
                                .collect::<Vec<_>>();
                            actual.sort_unstable();
                            expected_hand.sort_unstable();
                            assert_eq!(actual, expected_hand);
                            assert_eq!(
                                own.fixture_state().players[p.index()].library.len(),
                                library.len() - 1
                            );
                            assert_eq!(
                                own.fixture_state().players[p.index()].hand.len(),
                                hand.len() + 1
                            );
                            let choosing = own.snapshot();
                            let choosing_raw = native.snapshot_v1();
                            own.restore(&choosing).unwrap();
                            native.kaito_restore_v75(&choosing_raw).unwrap();
                            act(
                                &mut own,
                                &mut native,
                                |a| matches!(a,A::Discard{cards,..} if cards.len()==1 && cards[0].arena_id==chosen.0),
                            );
                            reached = true;
                            break;
                        }
                        act(&mut own, &mut native, quiet);
                    }
                    assert!(reached, "actual private discard choice absent");
                    settle(&mut own, &mut native);
                    let s = own.fixture_state();
                    assert_eq!(s.players[p.index()].hand.len(), hand.len());
                    assert_eq!(s.players[p.index()].library, &library[1..]);
                    assert_eq!(s.objects.get(chosen).zone, Zone::Graveyard);
                    assert_eq!(
                        s.objects.get(drawn).zone,
                        if discard_drawn {
                            Zone::Graveyard
                        } else {
                            Zone::Hand
                        }
                    );
                    assert_eq!(
                        s.objects.get(source).zone,
                        if removed {
                            Zone::Graveyard
                        } else {
                            Zone::Battlefield
                        }
                    );
                    assert_eq!(s.players[p.index()].mana_pool, [0; 6]);
                    assert_eq!(s.players[p.index()].life, 20);
                    assert_eq!(s.players[q.index()].life, 20);
                    priority(&mut own, &mut native, p);
                    assert!(
                        !actions(&own).iter().any(|a| activation(a, source)),
                        "tap cost must prevent a second activation"
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
