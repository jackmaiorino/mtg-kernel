use super::*;
use super::{
    surveil_many::{checkpoint, sessions},
    targeted_entry::{cast, choose},
};
use crate::{
    policy_observation_v6::tests::put,
    state::{GameState, Step, Zone},
};

#[test]
fn paid_revenge_normal_resolution_graveyard_count_rat_batch_tapped_entry_life_exile_response_counter_restore_both_seats(
) {
    paid_route(false);
}
#[test]
fn paid_revenge_flashback_four_mana_stack_value_exile_resolution_rat_batch_and_restore_both_seats()
{
    paid_route(true);
}

fn paid_route(flashback: bool) {
    let id = crate::card_def::card_id_by_name("Revenge of the Rats").unwrap();
    assert_eq!(
        super::super::super::roots::registered_spell_printing(id).unwrap(),
        Some(super::super::super::roots::baseline(id).unwrap())
    );
    let forest = crate::card_def::card_id_by_name("Forest").unwrap();
    let rat = crate::card_def::card_id_by_name("Rat Token").unwrap();
    for p in [PlayerId::P0, PlayerId::P1] {
        for creatures in 0usize..=3 {
            let mut s = GameState::new_from_libraries_with_starting_player_v1(
                &[forest; 40],
                &[forest; 40],
                crate::rl::card_name,
                110_393,
                p,
            );
            s.step = Step::Main1;
            let lands = (0..4)
                .map(|_| put(&mut s, p, "Swamp", Zone::Battlefield))
                .collect::<Vec<_>>();
            for _ in 0..2 {
                put(&mut s, p.opponent(), "Island", Zone::Battlefield);
            }
            put(&mut s, p.opponent(), "Forest", Zone::Battlefield);
            // Initial public graveyards and observers are support operands.
            // The only claimed cast here is Revenge's actual paid route.
            let corpses = ["Healer's Hawk", "Llanowar Elves", "Gleaming Barrier"]
                .into_iter()
                .take(creatures)
                .map(|name| put(&mut s, p, name, Zone::Graveyard))
                .collect::<Vec<_>>();
            let noncreatures = [
                put(&mut s, p, "Forest", Zone::Graveyard),
                put(&mut s, p, "Lightning Bolt", Zone::Graveyard),
            ];
            let opposing_corpse = put(&mut s, p.opponent(), "Healer's Hawk", Zone::Graveyard);
            put(&mut s, p, "Dazzling Angel", Zone::Battlefield);
            let ooze = put(&mut s, p.opponent(), "Scavenging Ooze", Zone::Battlefield);
            let counter = put(&mut s, p.opponent(), "Counterspell", Zone::Hand);
            let source = put(
                &mut s,
                p,
                "Revenge of the Rats",
                if flashback {
                    Zone::Graveyard
                } else {
                    Zone::Hand
                },
            );
            let (mut own, mut native) = sessions(s);
            cast(&mut own, &mut native, source);
            assert_eq!(own.fixture_state().objects.get(source).zone, Zone::Stack);
            assert_eq!(
                own.fixture_state().stack.last().unwrap().is_flashback,
                flashback
            );
            assert_eq!(
                crate::engine::object_mana_value(own.fixture_state(), source),
                4
            );
            assert!(lands
                .iter()
                .all(|&land| own.fixture_state().objects.get(land).tapped));
            assert_eq!(own.fixture_state().players[p.index()].mana_pool, [0; 6]);
            checkpoint(&mut own, &mut native);
            let saved = own.snapshot();
            let raw = native.snapshot_v1();
            for branch in 0..if creatures == 0 { 2 } else { 3 } {
                let mut expected = None;
                for _ in 0..2 {
                    own.restore(&saved).unwrap();
                    native.kaito_restore_v75(&raw).unwrap();
                    if branch != 1 {
                        act(&mut own, &mut native, |a| matches!(a, A::Pass { .. }));
                        if branch == 0 {
                            cast(&mut own, &mut native, counter);
                            act(&mut own, &mut native, |a| {
                                is_original_spell_target(a, source)
                            });
                        } else {
                            act(&mut own, &mut native, |a| {
                                matches!(a,
                                A::ActivateAbility { source: s, ability_index: 0, .. } if s.arena_id == ooze.0)
                            });
                            choose(&mut own, &mut native, ooze, corpses[0]);
                        }
                    }
                    settle(&mut own, &mut native);
                    let state = own.fixture_state();
                    let count = if branch == 0 {
                        0
                    } else {
                        creatures - usize::from(branch == 2)
                    };
                    assert_eq!(
                        state.objects.get(source).zone,
                        if flashback {
                            Zone::Exile
                        } else {
                            Zone::Graveyard
                        }
                    );
                    let tokens = state.players[p.index()]
                        .battlefield
                        .iter()
                        .copied()
                        .filter(|&o| state.objects.get(o).card_def == rat)
                        .collect::<Vec<_>>();
                    assert_eq!(tokens.len(), count);
                    for token in tokens {
                        let object = state.objects.get(token);
                        assert!(object.v4.is_token);
                        assert!(object.tapped);
                        assert!(object.summoning_sick);
                        assert_eq!(object.owner, p);
                        assert_eq!(object.controller, p);
                        assert_eq!(
                            crate::engine::checked_effective_pt(state, token).unwrap(),
                            (1, 1)
                        );
                    }
                    assert_eq!(state.players[p.index()].life, 20 + count as i32);
                    assert_eq!(
                        state.players[p.opponent().index()].life,
                        20 + i32::from(branch == 2)
                    );
                    assert_eq!(
                        state.objects.get(ooze).counters.plus1_plus1,
                        i32::from(branch == 2)
                    );
                    for (index, &corpse) in corpses.iter().enumerate() {
                        assert_eq!(
                            state.objects.get(corpse).zone,
                            if branch == 2 && index == 0 {
                                Zone::Exile
                            } else {
                                Zone::Graveyard
                            }
                        );
                    }
                    for noncreature in noncreatures {
                        assert_eq!(state.objects.get(noncreature).zone, Zone::Graveyard);
                    }
                    assert_eq!(state.objects.get(opposing_corpse).zone, Zone::Graveyard);
                    assert!(lands.iter().all(|&land| state.objects.get(land).tapped));
                    if let Some(ref old) = expected {
                        assert_eq!(state, old);
                    } else {
                        expected = Some(state.clone());
                    }
                }
            }
        }
    }
}
