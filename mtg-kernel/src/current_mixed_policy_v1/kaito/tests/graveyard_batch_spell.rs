use super::*;
use crate::{
    policy_observation_v6::tests::put,
    state::{GameState, Step, Zone},
};

#[test]
fn paid_raise_resolution_set_mana_value_boundary_entry_life_counter_and_restore_both_seats() {
    paid_graveyard_batch("Raise the Past");
}
#[test]
fn paid_rise_both_graveyards_owner_control_simultaneous_observers_and_restore_both_seats() {
    paid_graveyard_batch("Rise of the Dark Realms");
}

fn paid_graveyard_batch(name: &str) {
    let rise = name == "Rise of the Dark Realms";
    let id = crate::card_def::card_id_by_name(name).unwrap();
    let meaning = super::super::super::roots::baseline(id).unwrap();
    assert_eq!(
        super::super::super::roots::registered_spell_printing(id).unwrap(),
        Some(meaning)
    );
    let forest = crate::card_def::card_id_by_name("Forest").unwrap();
    let cat = crate::card_def::card_id_by_name("Cat Token").unwrap();
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut state = GameState::new_from_libraries_with_starting_player_v1(
            &[forest; 40],
            &[forest; 40],
            crate::rl::card_name,
            110_355,
            p,
        );
        state.step = Step::Main1;
        for name in ["Plains", "Swamp", "Forest"] {
            for _ in 0..4 {
                put(&mut state, p, name, Zone::Battlefield);
            }
        }
        for _ in 0..2 {
            put(&mut state, p.opponent(), "Island", Zone::Battlefield);
        }
        put(&mut state, p.opponent(), "Swamp", Zone::Battlefield);
        let elf = put(&mut state, p, "Llanowar Elves", Zone::Graveyard);
        let pride = put(&mut state, p, "Ajani's Pridemate", Zone::Graveyard);
        let parent = put(&mut state, p, "Prideful Parent", Zone::Graveyard);
        // The entry observer is later in the graveyard than the other own
        // creatures. It must see the entire simultaneous return in Rise.
        let angel = put(&mut state, p, "Dazzling Angel", Zone::Graveyard);
        let other_hawk = put(&mut state, p.opponent(), "Healer's Hawk", Zone::Graveyard);
        let other_parent = put(&mut state, p.opponent(), "Prideful Parent", Zone::Graveyard);
        let other_pride = put(
            &mut state,
            p.opponent(),
            "Ajani's Pridemate",
            Zone::Graveyard,
        );
        let excluded_land = put(&mut state, p, "Forest", Zone::Graveyard);
        let excluded_spell = put(&mut state, p, "Counterspell", Zone::Graveyard);
        let live_hawk = put(&mut state, p, "Healer's Hawk", Zone::Battlefield);
        let food = put(&mut state, p, "Food Token", Zone::Battlefield);
        let counter = put(&mut state, p.opponent(), "Counterspell", Zone::Hand);
        let stab = put(&mut state, p.opponent(), "Stab", Zone::Hand);
        let spell = put(&mut state, p, name, Zone::Hand);
        let mut native = FastActorSessionV1::from_kaito_v75_fixture_state(state.clone());
        let mut own =
            KaitoSession::from_session(FastActorSessionV1::from_kaito_v75_fixture_state(state))
                .unwrap();
        act(
            &mut own,
            &mut native,
            |a| matches!(a, A::CastSpell { source, .. } if source.arena_id == spell.0),
        );
        let paid = if rise { 9 } else { 4 };
        assert_eq!(own.fixture_state().objects.get(spell).zone, Zone::Stack);
        assert_eq!(
            own.fixture_state().players[p.index()]
                .battlefield
                .iter()
                .filter(|&&o| own.fixture_state().objects.get(o).tapped)
                .count(),
            paid
        );
        let saved = own.snapshot();
        let raw = native.snapshot_v1();
        for response in [0, 1, 2] {
            let countered = response == 1;
            let mut expected = None;
            for _ in 0..2 {
                own.restore(&saved).unwrap();
                native.kaito_restore_v75(&raw).unwrap();
                if response != 0 {
                    act(&mut own, &mut native, |a| matches!(a, A::Pass { .. }));
                    let selected = if countered { counter } else { stab };
                    act(&mut own, &mut native, |a| {
                        matches!(a,
                        A::CastSpell { source, .. } if source.arena_id == selected.0)
                    });
                    act(&mut own, &mut native, |a| {
                        if countered {
                            is_original_spell_target(a, spell)
                        } else {
                            matches!(a, A::ChooseTarget { target: crate::rl::TargetRefV1::Object { object }, .. }
                            if object.arena_id == live_hawk.0)
                        }
                    });
                }
                settle(&mut own, &mut native);
                let state = own.fixture_state();
                assert_eq!(state.objects.get(spell).zone, Zone::Graveyard);
                let mut returned = Vec::new();
                if !countered {
                    returned.extend([elf, pride]);
                    if rise {
                        returned.extend([parent, angel, other_hawk, other_parent, other_pride]);
                    }
                    if response == 2 {
                        returned.push(live_hawk);
                    }
                }
                // Each returned Parent creates a Cat. Those later entries
                // also trigger the newly returned Angel and both Pridemates.
                let cats: Vec<_> = state
                    .objects
                    .iter()
                    .filter(|(_, object)| {
                        object.card_def == cat && object.zone == Zone::Battlefield
                    })
                    .collect();
                assert_eq!(cats.len(), if rise && !countered { 2 } else { 0 });
                for (_, token) in &cats {
                    assert_eq!((token.owner, token.controller), (p, p));
                    assert!(token.v4.is_token && token.summoning_sick && !token.tapped);
                }
                let gains = if rise && !countered {
                    returned.len() - 1 + 2
                } else {
                    0
                };
                assert_eq!(state.players[p.index()].life, 20 + gains as i32);
                assert_eq!(state.players[p.opponent().index()].life, 20);
                for (object, owner) in [
                    (elf, p),
                    (pride, p),
                    (parent, p),
                    (angel, p),
                    (other_hawk, p.opponent()),
                    (other_parent, p.opponent()),
                    (other_pride, p.opponent()),
                ] {
                    let live = state.objects.get(object);
                    assert_eq!(live.owner, owner);
                    assert_eq!(
                        live.zone,
                        if returned.contains(&object) {
                            Zone::Battlefield
                        } else {
                            Zone::Graveyard
                        }
                    );
                    if returned.contains(&object) {
                        assert_eq!(live.controller, p);
                        assert_eq!(live.zone_change_count, 1);
                        assert!(!live.tapped && live.summoning_sick);
                    }
                }
                assert_eq!(state.objects.get(live_hawk).zone, Zone::Battlefield);
                assert_eq!(
                    state.objects.get(live_hawk).zone_change_count,
                    if response == 2 { 2 } else { 0 }
                );
                assert_eq!(state.objects.get(excluded_land).zone, Zone::Graveyard);
                assert_eq!(state.objects.get(excluded_spell).zone, Zone::Graveyard);
                for object in [pride, other_pride] {
                    assert_eq!(
                        state.objects.get(object).counters.plus1_plus1,
                        if returned.contains(&object) {
                            gains as i32
                        } else {
                            0
                        }
                    );
                }
                act(&mut own, &mut native, |a| {
                    matches!(a,
                    A::ActivateAbility { source, ability_index: 0, .. } if source.arena_id == food.0)
                });
                settle(&mut own, &mut native);
                let state = own.fixture_state();
                assert_eq!(state.players[p.index()].life, 23 + gains as i32);
                for object in [pride, other_pride] {
                    assert_eq!(
                        state.objects.get(object).counters.plus1_plus1,
                        if returned.contains(&object) {
                            gains as i32 + 1
                        } else {
                            0
                        }
                    );
                }
                assert_eq!(state.players[p.index()].mana_pool, [0; 6]);
                assert_eq!(
                    state.players[p.index()]
                        .battlefield
                        .iter()
                        .filter(|&&o| state.objects.get(o).tapped)
                        .count(),
                    paid + 2
                );
                if let Some(ref previous) = expected {
                    assert_eq!(state, previous);
                } else {
                    expected = Some(state.clone());
                }
            }
        }
    }
}
