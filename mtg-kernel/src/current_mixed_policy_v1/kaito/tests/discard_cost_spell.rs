use super::*;
use super::{
    surveil_many::{checkpoint, scene, sessions},
    targeted_entry::{actions, cast},
};
use crate::{ids::ObjectId, policy_observation_v6::tests::put, state::Zone};

fn discard(a: &A, card: ObjectId) -> bool {
    matches!(a,A::Discard{cards,..}if cards.len()==1&&cards[0].arena_id==card.0)
}
fn finish(own: &mut KaitoSession, native: &mut FastActorSessionV1) {
    for _ in 0..64 {
        if matches!(own.current_response(), FastActorResponseV1::Terminal(_))
            || own.fixture_state().stack.is_empty()
                && own.fixture_state().engine.pending_effect.is_none()
        {
            return;
        }
        act(own, native, |a| {
            matches!(
                a,
                A::Pass { .. } | A::OrderTriggers { .. } | A::ChooseTriggerOrderNext { .. }
            )
        });
    }
    panic!("mandatory-discard draw did not finish");
}
#[test]
fn paid_thrill_mandatory_land_or_creature_discard_counter_two_draws_and_restore_both_seats() {
    let id = crate::card_def::card_id_by_name("Thrill of Possibility").unwrap();
    assert_eq!(
        super::super::super::roots::registered_spell_printing(id).unwrap(),
        Some(super::super::super::roots::baseline(id).unwrap())
    );
    for p in [PlayerId::P0, PlayerId::P1] {
        let (mut s, source, counter, _) = scene(p, "Thrill of Possibility", 40);
        for _ in 0..4 {
            put(&mut s, p, "Mountain", Zone::Battlefield);
        }
        let choices = [
            put(&mut s, p, "Forest", Zone::Hand),
            put(&mut s, p, "Serra Angel", Zone::Hand),
        ];
        let (mut own, mut native) = sessions(s);
        cast(&mut own, &mut native, source);
        assert_eq!(own.fixture_state().objects.get(source).zone, Zone::Stack);
        let menu = actions(&own);
        assert_eq!(
            menu.iter()
                .filter(|a| matches!(a, A::Discard { .. }))
                .count(),
            2
        );
        assert!(choices
            .iter()
            .all(|&card| menu.iter().any(|a| discard(a, card))));
        assert!(!menu.iter().any(|a| discard(a, source)));
        checkpoint(&mut own, &mut native);
        let selecting = own.snapshot();
        let selecting_raw = native.snapshot_v1();
        for card in choices {
            own.restore(&selecting).unwrap();
            native.kaito_restore_v75(&selecting_raw).unwrap();
            act(&mut own, &mut native, |a| discard(a, card));
            assert_eq!(own.fixture_state().objects.get(card).zone, Zone::Graveyard);
            assert!(own.fixture_state().engine.pending_discard.is_none());
            assert_eq!(
                own.fixture_state().players[p.index()]
                    .battlefield
                    .iter()
                    .filter(|&&o| own.fixture_state().objects.get(o).tapped)
                    .count(),
                2
            );
            let paid = own.snapshot();
            let paid_raw = native.snapshot_v1();
            let hand = own.fixture_state().players[p.index()].hand.clone();
            let drawn = own.fixture_state().players[p.index()]
                .library
                .iter()
                .take(2)
                .copied()
                .collect::<Vec<_>>();
            for countered in [false, true] {
                let mut expected = None;
                for _ in 0..2 {
                    own.restore(&paid).unwrap();
                    native.kaito_restore_v75(&paid_raw).unwrap();
                    if countered {
                        cast(&mut own, &mut native, counter);
                        act(&mut own, &mut native, |a| {
                            is_original_spell_target(a, source)
                        });
                    }
                    finish(&mut own, &mut native);
                    assert!(matches!(
                        own.current_response(),
                        FastActorResponseV1::Decision(_)
                    ));
                    let s = own.fixture_state();
                    assert_eq!(s.objects.get(source).zone, Zone::Graveyard);
                    assert_eq!(s.objects.get(card).zone, Zone::Graveyard);
                    assert_eq!(
                        s.players[p.index()].library.len(),
                        if countered { 40 } else { 38 }
                    );
                    assert_eq!(
                        s.players[p.index()].hand.len(),
                        hand.len() + if countered { 0 } else { 2 }
                    );
                    assert!(hand.iter().all(|o| s.players[p.index()].hand.contains(o)));
                    assert!(drawn
                        .iter()
                        .all(|o| s.players[p.index()].hand.contains(o) != countered));
                    if let Some(expected) = &expected {
                        assert_eq!(s, expected);
                    } else {
                        expected = Some(s.clone());
                    }
                }
            }
        }
    }
}

#[test]
fn paid_thrill_empty_library_loss_keeps_discard_and_mana_and_unpayable_cast_absent_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let (mut s, source, _, _) = scene(p, "Thrill of Possibility", 0);
        for _ in 0..4 {
            put(&mut s, p, "Mountain", Zone::Battlefield);
        }
        let (own, _) = sessions(s.clone());
        assert!(
            !actions(&own)
                .iter()
                .any(|a| matches!(a,A::CastSpell{source:s,..}if s.arena_id==source.0)),
            "additional discard was not payable"
        );
        let card = put(&mut s, p, "Forest", Zone::Hand);
        put(&mut s, p, "Plains", Zone::Hand);
        let (mut own, mut native) = sessions(s);
        cast(&mut own, &mut native, source);
        let saved = own.snapshot();
        let raw = native.snapshot_v1();
        let mut expected = None;
        for _ in 0..2 {
            own.restore(&saved).unwrap();
            native.kaito_restore_v75(&raw).unwrap();
            act(&mut own, &mut native, |a| discard(a, card));
            finish(&mut own, &mut native);
            let FastActorResponseV1::Terminal(t) = own.current_response() else {
                panic!("empty library draw did not lose");
            };
            assert_eq!(t.winner, Some(crate::rl::PlayerSeatV1::from(p.opponent())));
            assert_eq!(
                t.terminal_classification,
                crate::rl::TerminalClassificationV1::Natural
            );
            assert_eq!(own.fixture_state().objects.get(card).zone, Zone::Graveyard);
            assert_eq!(
                own.fixture_state().players[p.index()]
                    .battlefield
                    .iter()
                    .filter(|&&o| own.fixture_state().objects.get(o).tapped)
                    .count(),
                2
            );
            if let Some(expected) = &expected {
                assert_eq!(own.fixture_state(), expected);
            } else {
                expected = Some(own.fixture_state().clone());
            }
        }
    }
}
