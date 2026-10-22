use super::surveil_many::{checkpoint, sessions};
use super::targeted_entry::{cast, choose};
use super::*;
use crate::{
    card_def::Keywords,
    policy_observation_v6::tests::put,
    state::{GameState, StackItemKind, Step, Zone},
};

#[test]
fn paid_guarded_heir_two_distinct_knights_counter_source_departure_and_restore_both_seats() {
    let name = "Guarded Heir";
    let id = crate::card_def::card_id_by_name(name).unwrap();
    assert_eq!(
        super::super::super::roots::registered_permanent_printing(id).unwrap(),
        Some(super::super::super::roots::baseline(id).unwrap())
    );
    let plains = crate::card_def::card_id_by_name("Plains").unwrap();
    for p in [PlayerId::P0, PlayerId::P1] {
        let q = p.opponent();
        let mut s = GameState::new_from_libraries_with_starting_player_v1(
            &[plains; 40],
            &[plains; 40],
            crate::rl::card_name,
            110_402,
            p,
        );
        s.step = Step::Main1;
        for _ in 0..6 {
            put(&mut s, p, "Plains", Zone::Battlefield);
        }
        for land in ["Island", "Swamp"] {
            for _ in 0..3 {
                put(&mut s, q, land, Zone::Battlefield);
            }
        }
        let source = put(&mut s, p, name, Zone::Hand);
        let counter = put(&mut s, q, "Counterspell", Zone::Hand);
        let removal = put(&mut s, q, "Hero's Downfall", Zone::Hand);
        let (mut own, mut native) = sessions(s);
        cast(&mut own, &mut native, source);
        assert_eq!(
            own.fixture_state().players[p.index()]
                .battlefield
                .iter()
                .filter(|&&o| own.fixture_state().objects.get(o).tapped)
                .count(),
            6
        );
        let paid = own.snapshot();
        let raw = native.snapshot_v1();
        let mut previous = None;
        for _ in 0..2 {
            own.restore(&paid).unwrap();
            native.kaito_restore_v75(&raw).unwrap();
            cast(&mut own, &mut native, counter);
            act(&mut own, &mut native, |a| {
                is_original_spell_target(a, source)
            });
            settle(&mut own, &mut native);
            assert_eq!(
                own.fixture_state().objects.get(source).zone,
                Zone::Graveyard
            );
            assert_eq!(own.fixture_state().players[p.index()].battlefield.len(), 6);
            if let Some(ref old) = previous {
                assert_eq!(own.fixture_state(), old);
            } else {
                previous = Some(own.fixture_state().clone());
            }
        }
        own.restore(&paid).unwrap();
        native.kaito_restore_v75(&raw).unwrap();
        for _ in 0..64 {
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
        assert_eq!(own.fixture_state().stack.last().unwrap().source, source);
        assert!(crate::engine::has_effective_keyword(
            own.fixture_state(),
            source,
            Keywords::LIFELINK
        ));
        assert_eq!(
            crate::engine::checked_effective_pt(own.fixture_state(), source).unwrap(),
            (1, 1)
        );
        checkpoint(&mut own, &mut native);
        let captured = own.snapshot();
        let captured_raw = native.snapshot_v1();
        for removed in [false, true] {
            let mut expected = None;
            for _ in 0..2 {
                own.restore(&captured).unwrap();
                native.kaito_restore_v75(&captured_raw).unwrap();
                if removed {
                    cast(&mut own, &mut native, removal);
                    choose(&mut own, &mut native, removal, source);
                }
                settle(&mut own, &mut native);
                let s = own.fixture_state();
                let tokens = s.players[p.index()]
                    .battlefield
                    .iter()
                    .copied()
                    .filter(|&o| s.objects.get(o).name == "Knight Token")
                    .collect::<Vec<_>>();
                assert_eq!(tokens.len(), 2);
                assert_ne!(tokens[0], tokens[1]);
                for token in tokens {
                    let o = s.objects.get(token);
                    assert!(o.v4.is_token);
                    assert!(o.summoning_sick);
                    assert!(!o.tapped);
                    assert_eq!(o.owner, p);
                    assert_eq!(o.controller, p);
                    assert_eq!(
                        crate::engine::checked_effective_pt(s, token).unwrap(),
                        (3, 3)
                    );
                    assert!(!crate::engine::has_effective_keyword(
                        s,
                        token,
                        Keywords::LIFELINK
                    ));
                }
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
                if let Some(ref old) = expected {
                    assert_eq!(s, old);
                } else {
                    expected = Some(s.clone());
                }
            }
        }
    }
}
