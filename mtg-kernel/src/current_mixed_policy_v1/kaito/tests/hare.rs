use super::*;
use super::{
    surveil_many::{checkpoint, sessions},
    targeted_entry::{cast, choose},
};
use crate::{
    policy_observation_v6::tests::put,
    state::{GameState, StackItemKind, Step, Zone},
};

#[test]
fn paid_hare_live_other_creature_count_entry_batch_counter_source_or_other_removal_and_restore_both_seats(
) {
    let id = crate::card_def::card_id_by_name("Hare Apparent").unwrap();
    assert_eq!(
        super::super::super::roots::registered_permanent_printing(id).unwrap(),
        Some(super::super::super::roots::baseline(id).unwrap())
    );
    let forest = crate::card_def::card_id_by_name("Forest").unwrap();
    let rabbit = crate::card_def::card_id_by_name("Rabbit Token").unwrap();
    for p in [PlayerId::P0, PlayerId::P1] {
        for count in 0usize..=3 {
            let mut s = GameState::new_from_libraries_with_starting_player_v1(
                &[forest; 40],
                &[forest; 40],
                crate::rl::card_name,
                110_395,
                p,
            );
            s.step = Step::Main1;
            let lands = (0..2)
                .map(|_| put(&mut s, p, "Plains", Zone::Battlefield))
                .collect::<Vec<_>>();
            for land in ["Island", "Swamp", "Mountain"] {
                for _ in 0..2 {
                    put(&mut s, p.opponent(), land, Zone::Battlefield);
                }
            }
            // Initial named creatures are support operands. An opposing Hare
            // and an own unrelated Rabbit must not contribute to the count.
            let others = (0..count)
                .map(|_| put(&mut s, p, "Hare Apparent", Zone::Battlefield))
                .collect::<Vec<_>>();
            put(&mut s, p.opponent(), "Hare Apparent", Zone::Battlefield);
            let old_rabbit = put(&mut s, p, "Rabbit Token", Zone::Battlefield);
            put(&mut s, p, "Dazzling Angel", Zone::Battlefield);
            let source = put(&mut s, p, "Hare Apparent", Zone::Hand);
            let counter = put(&mut s, p.opponent(), "Counterspell", Zone::Hand);
            let removal = put(&mut s, p.opponent(), "Lightning Bolt", Zone::Hand);
            let (mut own, mut native) = sessions(s);
            cast(&mut own, &mut native, source);
            assert_eq!(own.fixture_state().objects.get(source).zone, Zone::Stack);
            assert!(lands
                .iter()
                .all(|&o| own.fixture_state().objects.get(o).tapped));
            assert_eq!(own.fixture_state().players[p.index()].mana_pool, [0; 6]);
            assert_eq!(
                crate::engine::object_mana_value(own.fixture_state(), source),
                2
            );
            let paid = own.snapshot();
            let raw = native.snapshot_v1();
            for response in 0..4 {
                if response == 2 && count == 0 {
                    continue;
                }
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
                        for _ in 0..128 {
                            if own.fixture_state().stack.iter().any(|i| {
                                i.source == source && i.kind == StackItemKind::TriggeredAbility
                            }) && own.fixture_state().engine.pending_triggers.is_empty()
                            {
                                break;
                            }
                            act(&mut own, &mut native, |a| {
                                matches!(
                                    a,
                                    A::Pass { .. }
                                        | A::OrderTriggers { .. }
                                        | A::ChooseTriggerOrderNext { .. }
                                )
                            });
                        }
                        assert!(own.fixture_state().stack.iter().any(
                            |i| i.source == source && i.kind == StackItemKind::TriggeredAbility
                        ));
                        checkpoint(&mut own, &mut native);
                        if response >= 2 {
                            cast(&mut own, &mut native, removal);
                            choose(
                                &mut own,
                                &mut native,
                                removal,
                                if response == 2 { others[0] } else { source },
                            );
                        }
                    }
                    settle(&mut own, &mut native);
                    let state = own.fixture_state();
                    let expected_count = if response == 1 {
                        0
                    } else {
                        count - usize::from(response == 2)
                    };
                    let created = state.players[p.index()]
                        .battlefield
                        .iter()
                        .copied()
                        .filter(|&o| o != old_rabbit && state.objects.get(o).card_def == rabbit)
                        .collect::<Vec<_>>();
                    assert_eq!(created.len(), expected_count);
                    assert_eq!(
                        state.objects.get(source).zone,
                        if response == 1 || response == 3 {
                            Zone::Graveyard
                        } else {
                            Zone::Battlefield
                        }
                    );
                    assert_eq!(
                        state.players[p.index()].life,
                        20 + if response == 1 {
                            0
                        } else {
                            1 + expected_count as i32
                        }
                    );
                    assert!(!state.players[p.opponent().index()]
                        .battlefield
                        .iter()
                        .any(|&o| state.objects.get(o).card_def == rabbit));
                    for o in created {
                        let live = state.objects.get(o);
                        assert!(live.v4.is_token && live.summoning_sick);
                        assert!(!live.tapped);
                        assert_eq!((live.owner, live.controller), (p, p));
                        assert_eq!(
                            crate::engine::checked_effective_pt(state, o).unwrap(),
                            (1, 1)
                        );
                    }
                    if let Some(ref previous) = expected {
                        assert_eq!(state, previous);
                    } else {
                        expected = Some(state.clone());
                    }
                    checkpoint(&mut own, &mut native);
                }
            }
        }
    }
}
