use super::*;
use super::{
    ordinary_equipment::reach_main,
    surveil_many::{checkpoint, sessions},
    targeted_entry::{cast, choose},
};
use crate::{
    card_def::Keywords,
    policy_observation_v6::tests::put,
    state::{GameState, Step, Zone},
};
#[test]
fn paid_inspiring_call_resolution_creature_snapshot_counter_token_removal_response_later_counter_destroy_and_cleanup_restore_both_seats(
) {
    let id = crate::card_def::card_id_by_name("Inspiring Call").unwrap();
    assert_eq!(
        super::super::super::roots::registered_spell_printing(id).unwrap(),
        Some(super::super::super::roots::baseline(id).unwrap())
    );
    let forest = crate::card_def::card_id_by_name("Forest").unwrap();
    for p in [PlayerId::P0, PlayerId::P1] {
        for initial_counter in [false, true] {
            let mut s = GameState::new_from_libraries_with_starting_player_v1(
                &[forest; 40],
                &[forest; 40],
                crate::rl::card_name,
                110_400,
                p,
            );
            s.step = Step::Main1;
            let lands = (0..4)
                .map(|_| put(&mut s, p, "Forest", Zone::Battlefield))
                .collect::<Vec<_>>();
            for name in ["Island", "Swamp"] {
                for _ in 0..3 {
                    put(&mut s, p.opponent(), name, Zone::Battlefield);
                }
            }
            // Initial counters test the selection predicate, not a paid counter
            // producer. The Veil response below is an actual paid counter source.
            let rat = put(&mut s, p, "Rat Token", Zone::Battlefield);
            s.objects.get_mut(rat).counters.plus1_plus1 = if initial_counter { 3 } else { 0 };
            let lions = put(&mut s, p, "Savannah Lions", Zone::Battlefield);
            let artifact = put(&mut s, p, "Goldvein Pick", Zone::Battlefield);
            s.objects.get_mut(artifact).counters.plus1_plus1 = 5;
            let foreign = put(&mut s, p.opponent(), "Serra Angel", Zone::Battlefield);
            s.objects.get_mut(foreign).counters.plus1_plus1 = 2;
            let source = put(&mut s, p, "Inspiring Call", Zone::Hand);
            let veil = put(&mut s, p, "Snakeskin Veil", Zone::Hand);
            let counter = put(&mut s, p.opponent(), "Counterspell", Zone::Hand);
            let removal = put(&mut s, p.opponent(), "Hero's Downfall", Zone::Hand);
            let (mut own, mut native) = sessions(s);
            cast(&mut own, &mut native, source);
            assert_eq!(own.fixture_state().objects.get(source).zone, Zone::Stack);
            assert_eq!(
                crate::engine::object_mana_value(own.fixture_state(), source),
                3
            );
            assert_eq!(
                lands
                    .iter()
                    .filter(|&&o| own.fixture_state().objects.get(o).tapped)
                    .count(),
                3
            );
            assert_eq!(own.fixture_state().players[p.index()].mana_pool, [0; 6]);
            let hand = own.fixture_state().players[p.index()].hand.len();
            let library = own.fixture_state().players[p.index()].library.len();
            let saved = own.snapshot();
            let raw = native.snapshot_v1();
            for response in 0..4 {
                let mut expected = None;
                for _ in 0..2 {
                    own.restore(&saved).unwrap();
                    native.kaito_restore_v75(&raw).unwrap();
                    if response == 2 {
                        cast(&mut own, &mut native, veil);
                        choose(&mut own, &mut native, veil, lions);
                    }
                    if response == 1 || response == 3 {
                        cast(
                            &mut own,
                            &mut native,
                            if response == 1 { counter } else { removal },
                        );
                        if response == 1 {
                            act(&mut own, &mut native, |a| {
                                is_original_spell_target(a, source)
                            });
                        } else {
                            choose(&mut own, &mut native, removal, rat);
                        }
                    }
                    settle(&mut own, &mut native);
                    let draws = if response == 1 {
                        0
                    } else {
                        usize::from(initial_counter && response != 3) + usize::from(response == 2)
                    };
                    let state = own.fixture_state();
                    assert_eq!(state.objects.get(source).zone, Zone::Graveyard);
                    assert_eq!(state.players[p.index()].library.len(), library - draws);
                    assert_eq!(
                        state.players[p.index()].hand.len(),
                        hand - usize::from(response == 2) + draws
                    );
                    assert_eq!(
                        crate::engine::has_effective_keyword(state, rat, Keywords::INDESTRUCTIBLE),
                        initial_counter && response != 1 && response != 3
                    );
                    assert_eq!(
                        crate::engine::has_effective_keyword(
                            state,
                            lions,
                            Keywords::INDESTRUCTIBLE
                        ),
                        response == 2
                    );
                    for o in [artifact, foreign] {
                        assert!(!crate::engine::has_effective_keyword(
                            state,
                            o,
                            Keywords::INDESTRUCTIBLE
                        ));
                    }
                    if response == 0 {
                        cast(&mut own, &mut native, veil);
                        choose(&mut own, &mut native, veil, lions);
                        settle(&mut own, &mut native);
                        assert_eq!(
                            own.fixture_state().objects.get(lions).counters.plus1_plus1,
                            1
                        );
                        assert!(!crate::engine::has_effective_keyword(
                            own.fixture_state(),
                            lions,
                            Keywords::INDESTRUCTIBLE
                        ));
                    }
                    if initial_counter && (response == 0 || response == 2) {
                        cast(&mut own, &mut native, removal);
                        choose(&mut own, &mut native, removal, rat);
                        settle(&mut own, &mut native);
                        assert_eq!(own.fixture_state().objects.get(rat).zone, Zone::Battlefield);
                    }
                    reach_main(&mut own, &mut native, p.opponent());
                    for o in [rat, lions] {
                        assert!(!crate::engine::has_effective_keyword(
                            own.fixture_state(),
                            o,
                            Keywords::INDESTRUCTIBLE
                        ));
                    }
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
}
