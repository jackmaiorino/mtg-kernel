use super::*;
use crate::{
    policy_observation_v6::tests::put,
    state::{GameState, Step, Zone},
};

#[test]
fn paid_distinct_mana_values_zero_tokens_resolution_change_counter_and_restore_both_seats() {
    let id = crate::card_def::card_id_by_name("Lunar Insight").unwrap();
    let meaning = super::super::super::roots::baseline(id).unwrap();
    assert_eq!(
        super::super::super::roots::registered_spell_printing(id).unwrap(),
        Some(meaning)
    );
    let forest = crate::card_def::card_id_by_name("Forest").unwrap();
    for p in [PlayerId::P0, PlayerId::P1] {
        // No nonlands, two distinct values with a duplicate, then a zero-MV
        // token. Initial objects are resources; all payments/actions are native.
        for board in [0, 1, 2] {
            let mut state = GameState::new_from_libraries_with_starting_player_v1(
                &[forest; 40],
                &[forest; 40],
                crate::rl::card_name,
                110_353,
                p,
            );
            state.step = Step::Main1;
            for _ in 0..7 {
                put(&mut state, p, "Island", Zone::Battlefield);
            }
            let land = put(&mut state, p, "Soulstone Sanctuary", Zone::Battlefield);
            for _ in 0..2 {
                put(&mut state, p.opponent(), "Island", Zone::Battlefield);
            }
            put(&mut state, p.opponent(), "Swamp", Zone::Battlefield);
            put(
                &mut state,
                p.opponent(),
                "Swiftfoot Boots",
                Zone::Battlefield,
            );
            put(&mut state, p, "Quick-Draw Katana", Zone::Hand);
            put(&mut state, p, "Firebrand Archer", Zone::Graveyard);
            let hawk = if board != 0 {
                put(&mut state, p, "Prideful Parent", Zone::Battlefield);
                put(&mut state, p, "Prideful Parent", Zone::Battlefield);
                Some(put(&mut state, p, "Healer's Hawk", Zone::Battlefield))
            } else {
                None
            };
            if board == 2 {
                let token = put(&mut state, p, "Soldier Token", Zone::Battlefield);
                assert!(state.objects.get(token).v4.is_token);
            }
            let counter = put(&mut state, p.opponent(), "Counterspell", Zone::Hand);
            let stab = put(&mut state, p.opponent(), "Stab", Zone::Hand);
            let spell = put(&mut state, p, "Lunar Insight", Zone::Hand);
            let mut native = FastActorSessionV1::from_kaito_v75_fixture_state(state.clone());
            let mut own =
                KaitoSession::from_session(FastActorSessionV1::from_kaito_v75_fixture_state(state))
                    .unwrap();
            // A creature land still contributes no distinct value to this draw.
            act(&mut own, &mut native, |a| {
                matches!(a,
                A::ActivateAbility { source, ability_index: 0, .. } if source.arena_id == land.0)
            });
            settle(&mut own, &mut native);
            assert!(crate::engine::object_has_type(
                own.fixture_state(),
                land,
                crate::card_def::CardType::Creature
            ));
            act(&mut own, &mut native, |a| {
                matches!(a,
                A::CastSpell { source, .. } if source.arena_id == spell.0)
            });
            assert_eq!(own.fixture_state().objects.get(spell).zone, Zone::Stack);
            assert_eq!(
                own.fixture_state().players[p.index()]
                    .battlefield
                    .iter()
                    .filter(|&&o| own.fixture_state().objects.get(o).tapped)
                    .count(),
                7
            );
            let hand_before = own.fixture_state().players[p.index()].hand.len();
            let library_before = own.fixture_state().players[p.index()].library.len();
            let saved = own.snapshot();
            let raw = native.snapshot_v1();
            for response in [0, 1, 2] {
                if response == 2 && hawk.is_none() {
                    continue;
                }
                let drawn = if response == 1 || board == 0 {
                    0
                } else {
                    2 + usize::from(board == 2) - usize::from(response == 2)
                };
                let mut expected = None;
                for _ in 0..2 {
                    own.restore(&saved).unwrap();
                    native.kaito_restore_v75(&raw).unwrap();
                    if response != 0 {
                        act(&mut own, &mut native, |a| matches!(a, A::Pass { .. }));
                        let source = if response == 1 { counter } else { stab };
                        act(&mut own, &mut native, |a| {
                            matches!(a,
                            A::CastSpell { source: actual, .. } if actual.arena_id == source.0)
                        });
                        act(&mut own, &mut native, |a| {
                            if response == 1 {
                                is_original_spell_target(a, spell)
                            } else {
                                matches!(a, A::ChooseTarget { target: crate::rl::TargetRefV1::Object { object }, .. }
                                if object.arena_id == hawk.unwrap().0)
                            }
                        });
                    }
                    settle(&mut own, &mut native);
                    let state = own.fixture_state();
                    assert_eq!(state.objects.get(spell).zone, Zone::Graveyard);
                    assert_eq!(state.players[p.index()].hand.len(), hand_before + drawn);
                    assert_eq!(
                        state.players[p.index()].library.len(),
                        library_before - drawn
                    );
                    assert_eq!(state.players[p.opponent().index()].library.len(), 40);
                    assert_eq!(state.players[p.index()].mana_pool, [0; 6]);
                    assert_eq!(
                        state.players[p.index()]
                            .battlefield
                            .iter()
                            .filter(|&&o| state.objects.get(o).tapped)
                            .count(),
                        7
                    );
                    if response == 2 {
                        assert_eq!(state.objects.get(hawk.unwrap()).zone, Zone::Graveyard);
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
}
