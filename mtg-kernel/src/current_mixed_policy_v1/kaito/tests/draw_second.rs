use super::*;
use super::{
    ordinary_equipment::reach_main,
    surveil_many::{checkpoint, sessions},
    targeted_entry::{cast, choose},
};
use crate::{
    card_def::Keywords,
    ids::ObjectId,
    policy_observation_v6::tests::put,
    state::{GameState, StackItemKind, Step, Zone},
};
#[test]
fn paid_mystic_second_draw_only_opposing_draw_source_departure_third_draw_and_restore_both_seats() {
    paid("Mischievous Mystic");
}
#[test]
fn paid_horde_second_draw_current_or_departure_copy_values_no_retroactive_copy_trigger_and_restore_both_seats(
) {
    paid("Homunculus Horde");
}
fn produced(s: &GameState, p: PlayerId, source: ObjectId, mystic: bool) -> Vec<ObjectId> {
    let id = crate::card_def::card_id_by_name(if mystic {
        "Faerie Token"
    } else {
        "Homunculus Horde"
    })
    .unwrap();
    s.players[p.index()]
        .battlefield
        .iter()
        .copied()
        .filter(|&o| o != source && s.objects.get(o).card_def == id && s.objects.get(o).v4.is_token)
        .collect()
}
fn paid(name: &str) {
    let id = crate::card_def::card_id_by_name(name).unwrap();
    let mystic = name == "Mischievous Mystic";
    assert_eq!(
        super::super::super::roots::registered_permanent_printing(id).unwrap(),
        Some(super::super::super::roots::baseline(id).unwrap())
    );
    let forest = crate::card_def::card_id_by_name("Forest").unwrap();
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = GameState::new_from_libraries_with_starting_player_v1(
            &[forest; 40],
            &[forest; 40],
            crate::rl::card_name,
            110_401,
            p,
        );
        s.step = Step::Main1;
        let lands = (0..12)
            .map(|_| put(&mut s, p, "Island", Zone::Battlefield))
            .collect::<Vec<_>>();
        for name in ["Island", "Swamp"] {
            for _ in 0..4 {
                put(&mut s, p.opponent(), name, Zone::Battlefield);
            }
        }
        let source = put(&mut s, p, name, Zone::Hand);
        let counter = put(&mut s, p.opponent(), "Counterspell", Zone::Hand);
        let removal = put(&mut s, p.opponent(), "Hero's Downfall", Zone::Hand);
        let draws = (0..3)
            .map(|_| put(&mut s, p, "Think Twice", Zone::Hand))
            .collect::<Vec<_>>();
        let other_draw = put(&mut s, p.opponent(), "Think Twice", Zone::Hand);
        let distraction = put(&mut s, p, "Fleeting Distraction", Zone::Hand);
        let (mut own, mut native) = sessions(s);
        assert_eq!(own.fixture_state().players[p.index()].draws_this_turn, 0);
        cast(&mut own, &mut native, source);
        assert_eq!(own.fixture_state().objects.get(source).zone, Zone::Stack);
        assert_eq!(
            crate::engine::object_mana_value(own.fixture_state(), source),
            if mystic { 2 } else { 4 }
        );
        assert_eq!(
            lands
                .iter()
                .filter(|&&o| own.fixture_state().objects.get(o).tapped)
                .count(),
            if mystic { 2 } else { 4 }
        );
        assert_eq!(own.fixture_state().players[p.index()].mana_pool, [0; 6]);
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
        cast(&mut own, &mut native, other_draw);
        settle(&mut own, &mut native);
        assert_eq!(own.fixture_state().players[p.index()].draws_this_turn, 0);
        assert_eq!(
            own.fixture_state().players[p.opponent().index()].draws_this_turn,
            1
        );
        assert!(produced(own.fixture_state(), p, source, mystic).is_empty());
        cast(&mut own, &mut native, draws[0]);
        settle(&mut own, &mut native);
        assert_eq!(own.fixture_state().players[p.index()].draws_this_turn, 1);
        assert!(produced(own.fixture_state(), p, source, mystic).is_empty());
        cast(&mut own, &mut native, draws[1]);
        for _ in 0..128 {
            if own
                .fixture_state()
                .stack
                .last()
                .is_some_and(|i| i.source == source && i.kind == StackItemKind::TriggeredAbility)
            {
                break;
            }
            act(&mut own, &mut native, |a| {
                matches!(
                    a,
                    A::Pass { .. } | A::OrderTriggers { .. } | A::ChooseTriggerOrderNext { .. }
                )
            });
        }
        assert!(own
            .fixture_state()
            .stack
            .last()
            .is_some_and(|i| i.source == source && i.kind == StackItemKind::TriggeredAbility));
        assert_eq!(own.fixture_state().players[p.index()].draws_this_turn, 2);
        assert!(produced(own.fixture_state(), p, source, mystic).is_empty());
        checkpoint(&mut own, &mut native);
        let captured = own.snapshot();
        let captured_raw = native.snapshot_v1();
        for response in 0..3 {
            let mut expected = None;
            for _ in 0..2 {
                own.restore(&captured).unwrap();
                native.kaito_restore_v75(&captured_raw).unwrap();
                if response == 1 {
                    cast(&mut own, &mut native, removal);
                    choose(&mut own, &mut native, removal, source);
                }
                if response == 2 {
                    cast(&mut own, &mut native, distraction);
                    choose(&mut own, &mut native, distraction, source);
                }
                settle(&mut own, &mut native);
                let tokens = produced(own.fixture_state(), p, source, mystic);
                assert_eq!(tokens.len(), 1);
                for token in tokens {
                    let state = own.fixture_state();
                    let live = state.objects.get(token);
                    assert_eq!((live.owner, live.controller), (p, p));
                    assert!(live.v4.is_token && live.summoning_sick);
                    assert!(!live.tapped);
                    assert_eq!(live.counters.plus1_plus1, 0);
                    assert_eq!(live.damage, 0);
                    assert_eq!(
                        crate::engine::checked_effective_pt(state, token).unwrap(),
                        if mystic { (1, 1) } else { (2, 2) }
                    );
                    assert_eq!(
                        crate::engine::has_effective_keyword(state, token, Keywords::FLYING),
                        mystic
                    );
                }
                assert_eq!(
                    own.fixture_state().objects.get(source).zone,
                    if response == 1 {
                        Zone::Graveyard
                    } else {
                        Zone::Battlefield
                    }
                );
                cast(&mut own, &mut native, draws[2]);
                settle(&mut own, &mut native);
                assert_eq!(
                    own.fixture_state().players[p.index()].draws_this_turn,
                    if response == 2 { 4 } else { 3 }
                );
                assert_eq!(produced(own.fixture_state(), p, source, mystic).len(), 1);
                if let Some(ref old) = expected {
                    assert_eq!(own.fixture_state(), old);
                } else {
                    expected = Some(own.fixture_state().clone());
                }
                checkpoint(&mut own, &mut native);
            }
        }
    }
}

#[test]
fn paid_mystic_and_horde_reset_on_opponents_turn_all_existing_hordes_trigger_without_retroactive_new_copies_and_restore_both_seats(
) {
    let forest = crate::card_def::card_id_by_name("Forest").unwrap();
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = GameState::new_from_libraries_with_starting_player_v1(
            &[forest; 40],
            &[forest; 40],
            crate::rl::card_name,
            110_402,
            p,
        );
        s.step = Step::Main1;
        for _ in 0..14 {
            put(&mut s, p, "Island", Zone::Battlefield);
        }
        let mystic = put(&mut s, p, "Mischievous Mystic", Zone::Hand);
        let horde = put(&mut s, p, "Homunculus Horde", Zone::Hand);
        let draws = (0..4)
            .map(|_| put(&mut s, p, "Think Twice", Zone::Hand))
            .collect::<Vec<_>>();
        let (mut own, mut native) = sessions(s);
        cast(&mut own, &mut native, mystic);
        settle(&mut own, &mut native);
        cast(&mut own, &mut native, horde);
        settle(&mut own, &mut native);
        for &draw in &draws[..2] {
            cast(&mut own, &mut native, draw);
            settle(&mut own, &mut native);
        }
        assert_eq!(own.fixture_state().players[p.index()].draws_this_turn, 2);
        assert_eq!(produced(own.fixture_state(), p, mystic, true).len(), 1);
        assert_eq!(produced(own.fixture_state(), p, horde, false).len(), 1);
        reach_main(&mut own, &mut native, p.opponent());
        assert_eq!(own.fixture_state().players[p.index()].draws_this_turn, 0);
        checkpoint(&mut own, &mut native);
        let saved = own.snapshot();
        let raw = native.snapshot_v1();
        let mut expected = None;
        for _ in 0..2 {
            own.restore(&saved).unwrap();
            native.kaito_restore_v75(&raw).unwrap();
            cast(&mut own, &mut native, draws[2]);
            settle(&mut own, &mut native);
            assert_eq!(own.fixture_state().players[p.index()].draws_this_turn, 1);
            assert_eq!(produced(own.fixture_state(), p, mystic, true).len(), 1);
            assert_eq!(produced(own.fixture_state(), p, horde, false).len(), 1);
            cast(&mut own, &mut native, draws[3]);
            settle(&mut own, &mut native);
            assert_eq!(own.fixture_state().active_player, p.opponent());
            assert_eq!(own.fixture_state().players[p.index()].draws_this_turn, 2);
            assert_eq!(produced(own.fixture_state(), p, mystic, true).len(), 2);
            assert_eq!(produced(own.fixture_state(), p, horde, false).len(), 3);
            if let Some(ref old) = expected {
                assert_eq!(own.fixture_state(), old);
            } else {
                expected = Some(own.fixture_state().clone());
            }
            checkpoint(&mut own, &mut native);
        }
    }
}
