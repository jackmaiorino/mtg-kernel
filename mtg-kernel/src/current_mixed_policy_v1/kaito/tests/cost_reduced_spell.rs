use super::*;
use crate::{
    policy_observation_v6::tests::put,
    state::{GameState, Step, Zone},
};

#[test]
fn paid_arcane_zero_one_many_wizards_locked_discount_draws_removal_counter_and_restore_both_seats()
{
    paid_discount("Arcane Epiphany");
}
#[test]
fn paid_claws_cat_count_floor_locked_payment_current_set_later_entry_cleanup_and_restore_both_seats(
) {
    paid_discount("Claws Out");
}

fn paid_discount(name: &str) {
    let arcane = name == "Arcane Epiphany";
    let id = crate::card_def::card_id_by_name(name).unwrap();
    let meaning = super::super::super::roots::baseline(id).unwrap();
    assert_eq!(
        super::super::super::roots::registered_spell_printing(id).unwrap(),
        Some(meaning)
    );
    let forest = crate::card_def::card_id_by_name("Forest").unwrap();
    for p in [PlayerId::P0, PlayerId::P1] {
        for count in [0, 1, 2, 4] {
            let mut state = GameState::new_from_libraries_with_starting_player_v1(
                &[forest; 40],
                &[forest; 40],
                crate::rl::card_name,
                110_362,
                p,
            );
            state.step = Step::Main1;
            for land in ["Island", "Plains", "Mountain"] {
                for _ in 0..4 {
                    put(&mut state, p, land, Zone::Battlefield);
                }
            }
            for land in ["Island", "Swamp", "Forest"] {
                for _ in 0..2 {
                    put(&mut state, p.opponent(), land, Zone::Battlefield);
                }
            }
            let support = if arcane {
                "Erudite Wizard"
            } else {
                "Helpful Hunter"
            };
            let creatures: Vec<_> = (0..count)
                .map(|_| put(&mut state, p, support, Zone::Battlefield))
                .collect();
            let other = put(&mut state, p.opponent(), support, Zone::Battlefield);
            let hawk = put(&mut state, p, "Healer's Hawk", Zone::Battlefield);
            put(&mut state, p, support, Zone::Graveyard);
            let late = put(&mut state, p, support, Zone::Hand);
            let counter = put(&mut state, p.opponent(), "Counterspell", Zone::Hand);
            let removal = put(&mut state, p.opponent(), "Hero's Downfall", Zone::Hand);
            let spell = put(&mut state, p, name, Zone::Hand);
            let mut native = FastActorSessionV1::from_kaito_v75_fixture_state(state.clone());
            let mut own =
                KaitoSession::from_session(FastActorSessionV1::from_kaito_v75_fixture_state(state))
                    .unwrap();
            act(
                &mut own,
                &mut native,
                |a| matches!(a,A::CastSpell{source,..}if source.arena_id==spell.0),
            );
            let reduction = if arcane {
                usize::from(count != 0)
            } else {
                count.min(3)
            };
            let paid = 5 - reduction;
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
                if response == 2 && creatures.is_empty() {
                    continue;
                }
                let mut expected = None;
                for _ in 0..2 {
                    own.restore(&saved).unwrap();
                    native.kaito_restore_v75(&raw).unwrap();
                    if response != 0 {
                        act(&mut own, &mut native, |a| matches!(a, A::Pass { .. }));
                        let selected = if response == 1 { counter } else { removal };
                        act(
                            &mut own,
                            &mut native,
                            |a| matches!(a,A::CastSpell{source,..}if source.arena_id==selected.0),
                        );
                        act(&mut own, &mut native, |a| {
                            if response == 1 {
                                matches!(a,A::ChooseTarget{target:crate::rl::TargetRefV1::StackItem{source,..},..}if source.arena_id==spell.0)
                            } else {
                                matches!(a,A::ChooseTarget{target:crate::rl::TargetRefV1::Object{object},..}if object.arena_id==creatures[0].0)
                            }
                        });
                    }
                    settle(&mut own, &mut native);
                    let state = own.fixture_state();
                    assert_eq!(state.objects.get(spell).zone, Zone::Graveyard);
                    assert_eq!(state.players[p.index()].mana_pool, [0; 6]);
                    assert_eq!(
                        state.players[p.index()]
                            .battlefield
                            .iter()
                            .filter(|&&o| state.objects.get(o).tapped)
                            .count(),
                        paid
                    );
                    assert_eq!(
                        state.players[p.index()].library.len(),
                        if arcane && response != 1 { 37 } else { 40 }
                    );
                    let boosted = !arcane && response != 1;
                    for (i, &object) in creatures.iter().enumerate() {
                        let live = state.objects.get(object);
                        if response == 2 && i == 0 {
                            assert_eq!(live.zone, Zone::Graveyard);
                            continue;
                        }
                        assert_eq!(live.zone, Zone::Battlefield);
                        if arcane {
                            assert_eq!(live.counters.plus1_plus1, i32::from(response != 1));
                        } else {
                            assert_eq!(
                                crate::engine::checked_effective_pt(state, object).unwrap(),
                                if boosted { (3, 3) } else { (1, 1) }
                            );
                        }
                    }
                    assert_eq!(state.objects.get(other).counters.plus1_plus1, 0);
                    assert_eq!(
                        crate::engine::checked_effective_pt(state, other).unwrap(),
                        if arcane { (2, 3) } else { (1, 1) }
                    );
                    assert_eq!(
                        crate::engine::checked_effective_pt(state, hawk).unwrap(),
                        if boosted { (3, 3) } else { (1, 1) }
                    );
                    if !arcane {
                        act(
                            &mut own,
                            &mut native,
                            |a| matches!(a,A::CastSpell{source,..}if source.arena_id==late.0),
                        );
                        settle(&mut own, &mut native);
                        assert_eq!(
                            crate::engine::checked_effective_pt(own.fixture_state(), late).unwrap(),
                            (1, 1)
                        );
                        assert_eq!(own.fixture_state().players[p.index()].library.len(), 39);
                        super::ordinary_equipment::reach_main(&mut own, &mut native, p.opponent());
                        for &object in &creatures {
                            if own.fixture_state().objects.get(object).zone == Zone::Battlefield {
                                assert_eq!(
                                    crate::engine::checked_effective_pt(
                                        own.fixture_state(),
                                        object
                                    )
                                    .unwrap(),
                                    (1, 1)
                                );
                            }
                        }
                        assert_eq!(
                            crate::engine::checked_effective_pt(own.fixture_state(), hawk).unwrap(),
                            (1, 1)
                        );
                    }
                    if let Some(ref previous) = expected {
                        assert_eq!(own.fixture_state(), previous);
                    } else {
                        expected = Some(own.fixture_state().clone());
                    }
                }
            }
        }
    }
}
